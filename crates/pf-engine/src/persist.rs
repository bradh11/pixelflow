//! Show files on disk: atomic saves and autosave history.

use crate::error::EngineError;
use pf_model::Show;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_SUFFIX: &str = ".pixelflow.json";

static SAVE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The folder a file is in, when the path names one.
pub(crate) fn folder_of(path: &Path) -> Option<&Path> {
    path.parent().filter(|p| !p.as_os_str().is_empty())
}

/// A show file read from disk, with its files already looked for, so opening it in the engine
/// reads nothing more (see [`read_show`]).
#[derive(Debug, Clone)]
pub struct LoadedShow {
    pub(crate) show: Show,
    /// The folder the file says it was saved in.
    pub(crate) saved_in: Option<PathBuf>,
    /// Whether each of the show's files is there, by path text.
    pub(crate) status: HashMap<String, bool>,
    /// For files that are nowhere: where they were when the show was saved, by path text.
    pub(crate) was_at: HashMap<String, String>,
}

impl LoadedShow {
    pub fn show(&self) -> &Show {
        &self.show
    }
}

/// An autosaved version of the open show, to read without holding the engine.
#[derive(Debug, Clone)]
pub struct HistoryFile {
    pub(crate) path: PathBuf,
    /// The show file's folder, where its relative paths start.
    pub(crate) folder: Option<PathBuf>,
}

impl HistoryFile {
    /// Reads the version and looks for its files (reads the disk).
    pub fn read(&self) -> Result<LoadedShow, EngineError> {
        read_show_in(&self.path, self.folder.as_deref())
    }
}

/// Reads and parses a show file (running schema migrations). File paths stored relative to the
/// show file come back in full, starting in its folder.
pub fn load_show(path: &Path) -> Result<Show, EngineError> {
    Ok(read_show(path)?.show)
}

/// Reads a show file and looks for its files (this reads the disk: do it without holding the
/// engine, then hand the result to [`crate::Engine::open_read`]). A relative path starts in the
/// show file's folder; when nothing is there but the file is where the show was saved (the show
/// file moved on its own), that full path is used instead.
pub fn read_show(path: &Path) -> Result<LoadedShow, EngineError> {
    read_show_in(path, folder_of(path))
}

/// Reads a show file whose relative file paths start in `folder` (an autosaved copy kept away
/// from the show file it belongs to), or stay relative without one.
pub(crate) fn read_show_in(path: &Path, folder: Option<&Path>) -> Result<LoadedShow, EngineError> {
    let text = fs::read_to_string(path).map_err(|source| EngineError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let (mut show, saved_in) =
        pf_model::show_file_from_json(&text).map_err(|source| EngineError::InvalidFile {
            path: path.to_path_buf(),
            source,
        })?;
    let saved_in = saved_in.map(|s| pf_model::path_from_text(&s));
    let mut status = HashMap::new();
    let mut was_at = HashMap::new();
    let mut there = |text: &str| -> bool {
        *status
            .entry(text.to_string())
            .or_insert_with(|| crate::files::exists(text))
    };
    for stored in show.file_paths_mut() {
        if stored.trim().is_empty() {
            continue;
        }
        let Some(folder) = folder else {
            there(stored);
            continue;
        };
        let here = pf_model::resolve_text(stored, folder);
        let before = saved_in
            .as_deref()
            .filter(|s| *s != folder && !pf_model::is_full_path_text(stored))
            .map(|s| pf_model::resolve_text(stored, s));
        if there(&here) {
            *stored = here;
        } else if let Some(before) = before {
            if there(&before) {
                *stored = before;
            } else {
                was_at.insert(here.clone(), before);
                *stored = here;
            }
        } else {
            *stored = here;
        }
    }
    status.retain(|path, _| {
        crate::files::files_of(&show)
            .iter()
            .any(|(_, _, p)| *p == path.as_str())
    });
    Ok(LoadedShow {
        show,
        saved_in,
        status,
        was_at,
    })
}

/// Writes the show so that a crash never leaves a half-written file: write a temporary file
/// in the same folder, flush it to disk, then rename it over the target. Files inside the show
/// file's folder are stored relative to it, so the folder can move; the file also says which
/// folder that was (`savedIn`), so a show file moved on its own still finds them.
pub fn save_show_atomic(path: &Path, show: &Show) -> Result<(), EngineError> {
    match folder_of(path) {
        Some(folder) => write_show(path, &show.with_paths_relative_to(folder), Some(folder)),
        None => write_show(path, show, None),
    }
}

/// Writes the show as it is (paths unchanged), atomically, saying it was saved in `saved_in`.
fn write_show(path: &Path, show: &Show, saved_in: Option<&Path>) -> Result<(), EngineError> {
    let saved_in = saved_in.map(pf_model::path_to_text);
    let json = pf_model::show_file_to_json(show, saved_in.as_deref()).map_err(|e| EngineError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    write_atomic(path, json.as_bytes())
}

/// Writes `bytes` to `path` atomically: a temporary file in the same folder, flushed to disk,
/// then renamed over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let write_err = |source| EngineError::Write {
        path: path.to_path_buf(),
        source,
    };
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
        file.write_all(bytes)?;
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

/// Writes a timestamped copy of the show into `dir`, keeping the newest `keep` copies. File
/// paths are written as `show` holds them (relative to `saved_in`, the show file's folder, when
/// it has one: see [`read_show_in`]).
pub(crate) fn write_history(
    dir: &Path,
    show: &Show,
    saved_in: Option<&Path>,
    keep: usize,
) -> Result<HistoryEntry, EngineError> {
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
    write_show(&path, show, saved_in)?;
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
            write_history(dir.path(), &Show::new(format!("v{i}")), None, 3).unwrap();
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
        let entry = write_history(dir.path(), &Show::new("x"), None, 0).unwrap();
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
