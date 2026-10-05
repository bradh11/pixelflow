//! Show files on disk: atomic saves and autosave history.

use crate::error::EngineError;
use pf_model::Show;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_SUFFIX: &str = ".pixelflow.json";

static SAVE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Reads and parses a show file (running schema migrations).
pub fn load_show(path: &Path) -> Result<Show, EngineError> {
    let text = fs::read_to_string(path).map_err(|source| EngineError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    pf_model::show_from_json(&text).map_err(|source| EngineError::InvalidFile {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes the show so that a crash never leaves a half-written file: write a temporary file
/// in the same folder, flush it to disk, then rename it over the target.
pub fn save_show_atomic(path: &Path, show: &Show) -> Result<(), EngineError> {
    let write_err = |source| EngineError::Write {
        path: path.to_path_buf(),
        source,
    };
    let json = pf_model::show_to_json(show).map_err(|e| write_err(std::io::Error::other(e)))?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(write_err)?;
    let file_name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let n = SAVE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), n));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(write_err)
}

/// One autosaved version of a show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// File name inside the history folder; pass it back to restore.
    pub id: String,
    pub saved_at_ms: u64,
    pub size_bytes: u64,
}

/// Writes a timestamped copy of the show into `dir`, keeping the newest `keep` copies.
pub(crate) fn write_history(dir: &Path, show: &Show, keep: usize) -> Result<HistoryEntry, EngineError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut stamp = now;
    while dir.join(format!("{stamp}{HISTORY_SUFFIX}")).exists() {
        stamp += 1;
    }
    let id = format!("{stamp}{HISTORY_SUFFIX}");
    let path = dir.join(&id);
    save_show_atomic(&path, show)?;
    let entries = list_history(dir);
    for old in entries.iter().skip(keep.max(1)) {
        let _ = fs::remove_file(dir.join(&old.id));
    }
    let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(HistoryEntry {
        id,
        saved_at_ms: stamp,
        size_bytes,
    })
}

/// Autosaved versions in `dir`, newest first. A missing folder means no history.
pub(crate) fn list_history(dir: &Path) -> Vec<HistoryEntry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<HistoryEntry> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            let stamp: u64 = id.strip_suffix(HISTORY_SUFFIX)?.parse().ok()?;
            Some(HistoryEntry {
                size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                id,
                saved_at_ms: stamp,
            })
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.saved_at_ms));
    entries
}

/// The history folder for a show: one subfolder per saved file (or "untitled").
pub(crate) fn history_dir(data_dir: &Path, show_path: Option<&Path>) -> PathBuf {
    let key = match show_path {
        Some(path) => {
            // Different spellings of one existing file share a history folder.
            let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let path = canonical.as_path();
            let stem = path
                .file_name()
                .map(|n| n.to_string_lossy().replace(HISTORY_SUFFIX, ""))
                .unwrap_or_default();
            let hash = path
                .to_string_lossy()
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
                });
            format!("{stem}-{hash:016x}")
        }
        None => "untitled".to_string(),
    };
    data_dir.join("history").join(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn atomic_save_round_trips_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("show.pixelflow.json");
        let show = Show::new("Saved");
        save_show_atomic(&path, &show).unwrap();
        save_show_atomic(&path, &show).unwrap();
        assert_eq!(load_show(&path).unwrap(), show);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn load_reports_missing_and_invalid_files_plainly() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.json");
        assert!(
            load_show(&missing)
                .unwrap_err()
                .to_string()
                .starts_with("Could not read")
        );
        let bad = dir.path().join("bad.json");
        fs::write(&bad, "{ nope").unwrap();
        assert!(
            load_show(&bad)
                .unwrap_err()
                .to_string()
                .contains("is not a valid show file")
        );
    }

    #[test]
    fn history_keeps_newest_entries_first() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            write_history(dir.path(), &Show::new(format!("v{i}")), 3).unwrap();
        }
        let entries = list_history(dir.path());
        assert_eq!(entries.len(), 3);
        assert!(entries[0].saved_at_ms > entries[1].saved_at_ms);
        let newest = load_show(&dir.path().join(&entries[0].id)).unwrap();
        assert_eq!(newest.name, "v4");
    }

    #[test]
    fn history_dirs_are_stable_per_file_and_distinct() {
        let data = Path::new("/data");
        let a = history_dir(data, Some(Path::new("/shows/house.pixelflow.json")));
        assert_eq!(
            a,
            history_dir(data, Some(Path::new("/shows/house.pixelflow.json")))
        );
        assert_ne!(
            a,
            history_dir(data, Some(Path::new("/other/house.pixelflow.json")))
        );
        assert!(a.to_string_lossy().contains("house-"));
        assert_eq!(history_dir(data, None), Path::new("/data/history/untitled"));
    }

    #[test]
    fn history_never_deletes_the_entry_just_written() {
        let dir = tempfile::tempdir().unwrap();
        let entry = write_history(dir.path(), &Show::new("x"), 0).unwrap();
        let entries = list_history(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(dir.path().join(&entry.id).exists());
    }

    #[test]
    fn history_dirs_match_for_different_spellings_of_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("house.pixelflow.json");
        fs::write(&file, "{}").unwrap();
        let roundabout = dir.path().join(".").join("house.pixelflow.json");
        let data = Path::new("/data");
        assert_eq!(
            history_dir(data, Some(&file)),
            history_dir(data, Some(&roundabout))
        );
    }
}
