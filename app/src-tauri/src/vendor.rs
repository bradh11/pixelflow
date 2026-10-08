//! Vendor sequences: looking inside a package and suggesting how its models map onto the open
//! show, the mappings remembered for next time, and xLights mapping files (`.xmap`).
//!
//! A mapping is remembered (in `vendor-mappings.json` in the config folder) under the vendor's
//! layout, so importing the song again, or another song from the same vendor, starts from it.

use crate::{AppState, PathArg, Reply};
use pf_model::path_to_text;
use pf_xlights::vendor::{self, Inspection, Mapping, Package};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use tauri::State;

const MAPPINGS_FILE: &str = "vendor-mappings.json";
/// Most vendors remembered; the least recently used is forgotten first.
const MAX_REMEMBERED: usize = 200;
/// Largest remembered-mappings file read.
const MAX_MAPPINGS_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Remembered {
    mapping: Mapping,
    /// When it was last saved (ms since 1970).
    used: u64,
}

/// The mappings used last, by vendor (see [`Inspection::key`]).
pub(crate) struct SavedMappings {
    file: Option<PathBuf>,
    mappings: Mutex<Option<BTreeMap<String, Remembered>>>,
}

impl SavedMappings {
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            file: dir.map(|d| d.join(MAPPINGS_FILE)),
            mappings: Mutex::new(None),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut BTreeMap<String, Remembered>) -> T) -> T {
        let mut mappings = self.mappings.lock().unwrap_or_else(PoisonError::into_inner);
        let mappings = mappings.get_or_insert_with(|| {
            self.file
                .as_ref()
                .and_then(|f| crate::recent::read_small(f, MAX_MAPPINGS_BYTES))
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        });
        f(mappings)
    }

    pub(crate) fn get(&self, key: &str) -> Option<Mapping> {
        self.with(|m| m.get(key).map(|r| r.mapping.clone()))
    }

    /// Remembers `mapping` for `key`; the file is replaced whole (see `recent::write_atomic`).
    pub(crate) fn set(&self, key: &str, mapping: &Mapping, now: u64) {
        self.with(|m| {
            m.insert(
                key.to_string(),
                Remembered {
                    mapping: mapping.clone(),
                    used: now,
                },
            );
            while m.len() > MAX_REMEMBERED {
                let Some(oldest) = m.iter().min_by_key(|(_, r)| r.used).map(|(k, _)| k.clone()) else {
                    break;
                };
                m.remove(&oldest);
            }
            let Some(file) = &self.file else { return };
            let json = serde_json::to_vec(m).unwrap_or_default();
            if let Err(error) = crate::recent::write_atomic(file, &json) {
                log::warn!("couldn't remember the vendor mapping: {error}");
            }
        });
    }
}

/// Where a package's music goes: the saved show's `music` folder, else that of a folder the
/// user picked in the shell's dialog (as for downloads from an FPP); `None` while there's
/// neither.
pub(crate) fn music_folder(state: &AppState, chosen: Option<&Path>) -> Option<PathBuf> {
    if let Some(folder) = state.engine().show_path().and_then(Path::parent) {
        return Some(folder.join("music"));
    }
    chosen
        .filter(|f| f.is_absolute() && state.download_folders.contains(f))
        .map(|f| f.join("music"))
}

/// A package's contents, the suggested mapping, and where its music would be saved.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Inspected {
    #[serde(flatten)]
    inspection: Inspection,
    /// The folder a zip's music is copied into; null while the show isn't saved.
    music_folder: Option<String>,
}

/// Looks inside the vendor sequence, package, or folder at `path` (its sequence `sequence`, or
/// its best one) and suggests how it maps onto the open show, starting from the mapping used
/// last time for the same vendor. Reads only; nothing is imported.
#[tauri::command]
pub(crate) async fn inspect_xlights_sequence(
    state: State<'_, AppState>,
    path: PathArg,
    sequence: Option<String>,
) -> Reply<Inspected> {
    let show = state.engine().show().clone();
    let saved = std::sync::Arc::clone(&state.vendor_mappings);
    let inspection = tauri::async_runtime::spawn_blocking(move || {
        let package = Package::open(&path).map_err(|e| e.to_string())?;
        vendor::inspect(&package, sequence.as_deref(), &show, |key| saved.get(key)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Something went wrong reading the xLights sequence.".to_string())??;
    let music_folder = music_folder(&state, None).map(|f| path_to_text(&f));
    Ok(Inspected {
        inspection,
        music_folder,
    })
}

/// A mapping read from an `.xmap` file.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XmapRead {
    mapping: Mapping,
    nodes_skipped: usize,
}

/// Reads the xLights mapping file at `path`.
#[tauri::command]
pub(crate) async fn read_xmap(path: PathArg) -> Reply<XmapRead> {
    tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&*path)
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?
            .len();
        if size > vendor::MAX_XMAP_BYTES as u64 {
            return Err(format!("{} is too large to be a mapping file.", path.display()));
        }
        let bytes = std::fs::read(&*path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        let read = vendor::read_xmap(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())?;
        Ok(XmapRead {
            mapping: read.mapping,
            nodes_skipped: read.nodes_skipped,
        })
    })
    .await
    .map_err(|_| "Something went wrong reading the mapping file.".to_string())?
}

/// Writes `mapping` to `path` as an xLights mapping file. Only `.xmap` files are written.
#[tauri::command]
pub(crate) async fn write_xmap(path: PathArg, mapping: Mapping) -> Reply<()> {
    let is_xmap = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmap"));
    if !is_xmap {
        return Err("Mappings are saved as .xmap files.".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::recent::write_atomic(&path, vendor::write_xmap(&mapping).as_bytes())
            .map_err(|e| format!("Couldn't save {}: {e}", path.display()))
    })
    .await
    .map_err(|_| "Something went wrong saving the mapping file.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_are_remembered_across_runs_and_the_oldest_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let mut mapping = Mapping::default();
        mapping.add("Mega Tree", "Tree");
        let saved = SavedMappings::new(Some(config.clone()));
        saved.set("layout:1", &mapping, 1);
        assert_eq!(
            SavedMappings::new(Some(config.clone())).get("layout:1"),
            Some(mapping.clone())
        );
        assert_eq!(saved.get("layout:2"), None);
        for i in 0..MAX_REMEMBERED as u64 + 5 {
            saved.set(&format!("other:{i}"), &mapping, 10 + i);
        }
        let again = SavedMappings::new(Some(config));
        assert_eq!(again.get("layout:1"), None, "the least recently used went first");
        assert!(again.get(&format!("other:{}", MAX_REMEMBERED + 4)).is_some());
    }
}
