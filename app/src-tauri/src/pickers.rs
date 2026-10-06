//! The system's open, save, and folder dialogs. Every one PixelFlow shows is asked for here,
//! so they all behave the same:
//!
//! - **They start somewhere sensible**: the folder last used for that kind of file, else the
//!   open show's (or sequence's) folder, else Documents. Left to itself macOS opens the panel
//!   wherever any panel was last, often Recents (a Spotlight search) or an iCloud or network
//!   folder, and the panel can take seconds to list it. Each candidate folder is looked at off
//!   the main thread with a short time limit, so a dead network drive is skipped, not waited on.
//! - **They're sheets on the main window**, not free-floating panels.
//! - **Paths come back without loss**: as path text (see `pf_model::path_to_text`), which the
//!   commands read back with `path_from_text`. Paths that went through the window's own
//!   dialog API came back as JavaScript strings, losing bytes that aren't UTF-8.
//!
//! How long each dialog takes is logged at debug level (`PIXELFLOW_LOG=debug`).

use crate::{AppState, Reply};
use pf_model::{path_from_text, path_to_text};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

/// How long a candidate starting folder may take to answer before it's passed over.
const FOLDER_WAIT: Duration = Duration::from_millis(400);
const FOLDERS_FILE: &str = "pickers.json";

/// What a dialog is for. The window names one of these; the shell decides the dialog's
/// filters, title, and starting folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PickKind {
    /// Open a PixelFlow show.
    Show,
    /// Save the show under a new name.
    ShowSave,
    /// An xLights show folder to import.
    XlightsFolder,
    /// An xLights sequence (`.xsq`) to import.
    XlightsSequence,
    /// An FPP sequence (`.fseq`) to add to the show.
    Fseq,
    /// Where to export an `.fseq`.
    FseqExport,
    /// Music for a sequence.
    Music,
    /// Open a PixelFlow sequence.
    SequenceDoc,
    /// Save the sequence under a new name.
    SequenceDocSave,
    /// A timing file to import.
    TimingFile,
    /// Where to write a timing track.
    TimingExport,
    /// A photo of the house.
    Photo,
    /// A 3D model of the house.
    HouseModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Open,
    Save,
    Folder,
}

/// Whose folder to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Near {
    Show,
    Sequence,
}

struct Spec {
    mode: Mode,
    title: &'static str,
    filters: &'static [(&'static str, &'static [&'static str])],
    /// Kinds with the same key share a remembered folder (opening and saving shows, say).
    key: &'static str,
    near: Near,
}

const MUSIC: &[&str] = &["mp3", "m4a", "wav", "ogg", "flac"];

impl PickKind {
    fn spec(self) -> Spec {
        use Mode::*;
        let (mode, title, filters, key, near): (_, _, &'static [(&str, &[&str])], _, _) = match self {
            Self::Show => (
                Open,
                "Open a show",
                &[("PixelFlow show", &["json"])],
                "show",
                Near::Show,
            ),
            Self::ShowSave => (
                Save,
                "Save the show",
                &[("PixelFlow show", &["json"])],
                "show",
                Near::Show,
            ),
            Self::XlightsFolder => (
                Folder,
                "Choose your xLights show folder",
                &[],
                "xlights",
                Near::Show,
            ),
            Self::XlightsSequence => (
                Open,
                "Choose an xLights sequence",
                &[("xLights sequence", &["xsq"])],
                "xlights",
                Near::Show,
            ),
            Self::Fseq => (
                Open,
                "Choose an FPP sequence",
                &[("FPP sequence", &["fseq"])],
                "fseq",
                Near::Show,
            ),
            Self::FseqExport => (
                Save,
                "Export the sequence for FPP",
                &[("FPP sequence", &["fseq"])],
                "fseq",
                Near::Sequence,
            ),
            Self::Music => (Open, "Choose music", &[("Music", MUSIC)], "music", Near::Sequence),
            Self::SequenceDoc => (
                Open,
                "Open a sequence",
                &[("PixelFlow sequence", &["json"])],
                "sequence",
                Near::Sequence,
            ),
            Self::SequenceDocSave => (
                Save,
                "Save the sequence",
                &[("PixelFlow sequence", &["json"])],
                "sequence",
                Near::Sequence,
            ),
            Self::TimingFile => (
                Open,
                "Import timing",
                &[
                    (
                        "Timing files (xLights .xtiming, Audacity labels .txt)",
                        &["xtiming", "txt"],
                    ),
                    ("xLights timing", &["xtiming"]),
                    ("Audacity labels", &["txt"]),
                ],
                "timing",
                Near::Sequence,
            ),
            Self::TimingExport => (
                Save,
                "Export timing",
                &[("xLights timing", &["xtiming"]), ("Audacity labels", &["txt"])],
                "timing",
                Near::Sequence,
            ),
            Self::Photo => (
                Open,
                "Choose a photo of your house",
                &[("Photo", crate::layout::IMAGE_EXTENSIONS)],
                "photo",
                Near::Show,
            ),
            Self::HouseModel => (
                Open,
                "Choose a 3D model of your house",
                &[("3D model", crate::house::MODEL_EXTENSIONS)],
                "model",
                Near::Show,
            ),
        };
        Spec {
            mode,
            title,
            filters,
            key,
            near,
        }
    }
}

/// The folder last used for each kind of file, kept in `pickers.json` in the config folder.
pub(crate) struct LastFolders {
    file: Option<PathBuf>,
    folders: Mutex<Option<HashMap<String, String>>>,
}

impl LastFolders {
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            file: dir.map(|d| d.join(FOLDERS_FILE)),
            folders: Mutex::new(None),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
        let mut folders = self.folders.lock().unwrap_or_else(PoisonError::into_inner);
        let folders = folders.get_or_insert_with(|| {
            self.file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        });
        f(folders)
    }

    fn get(&self, key: &str) -> Option<PathBuf> {
        self.with(|folders| folders.get(key).map(|t| path_from_text(t)))
    }

    fn set(&self, key: &str, folder: &Path) {
        let text = path_to_text(folder);
        let json = self.with(|folders| {
            (folders.get(key) != Some(&text)).then(|| {
                folders.insert(key.to_string(), text);
                serde_json::to_vec_pretty(folders).unwrap_or_default()
            })
        });
        if let (Some(json), Some(file)) = (json, &self.file) {
            let written = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(file, json));
            if let Err(error) = written {
                log::warn!("couldn't remember the dialog's folder: {error}");
            }
        }
    }
}

/// The first of `candidates` that is a folder, looking at all of them at once and waiting at
/// most `wait`: a drive that doesn't answer in time is passed over.
pub(crate) fn first_folder(candidates: Vec<PathBuf>, wait: Duration) -> Option<PathBuf> {
    let (tx, rx) = mpsc::channel();
    let mut answers: Vec<Option<bool>> = vec![None; candidates.len()];
    for (i, folder) in candidates.iter().cloned().enumerate() {
        let tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("pixelflow-dialog-folder".into())
            .spawn(move || {
                let _ = tx.send((i, folder.is_dir()));
            });
        if spawned.is_err() {
            answers[i] = Some(false);
        }
    }
    drop(tx);
    let deadline = Instant::now() + wait;
    loop {
        // The best answer so far, once everything before it has answered.
        for (i, answer) in answers.iter().enumerate() {
            match answer {
                Some(true) => return Some(candidates[i].clone()),
                Some(false) => continue,
                None => break,
            }
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        match rx.recv_timeout(left) {
            Ok((i, is_dir)) => answers[i] = Some(is_dir),
            Err(_) => break,
        }
    }
    // Out of time: the best folder that did answer.
    answers
        .iter()
        .position(|a| *a == Some(true))
        .map(|i| candidates[i].clone())
}

/// A dialog to show.
pub(crate) struct Pick {
    pub kind: PickKind,
    /// A title other than the kind's own ("Where is Song.mp3 now?").
    pub title: Option<String>,
    /// The file name a save dialog suggests.
    pub file_name: Option<String>,
    /// A folder to try before the usual ones (where a missing file was).
    pub first: Option<PathBuf>,
}

impl Pick {
    pub(crate) fn of(kind: PickKind) -> Self {
        Self {
            kind,
            title: None,
            file_name: None,
            first: None,
        }
    }
}

/// Where a dialog of `spec`'s kind should start looking, best first.
fn candidates<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    spec: &Spec,
    first: Option<PathBuf>,
) -> Vec<PathBuf> {
    let (show, sequence) = {
        let engine = state.engine();
        let folder = |p: Option<&Path>| p.and_then(Path::parent).map(Path::to_path_buf);
        (folder(engine.show_path()), folder(engine.sequence_path()))
    };
    let documents = app.path().document_dir().ok();
    let near = match spec.near {
        Near::Show => [show, sequence],
        Near::Sequence => [sequence, show],
    };
    first
        .into_iter()
        .chain(state.last_folders.get(spec.key))
        .chain(near.into_iter().flatten())
        .chain(documents)
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

/// Shows the dialog, as a sheet on the main window, starting in a sensible folder; the path
/// chosen, or `None` when cancelled. Remembers the folder for next time.
pub(crate) async fn pick<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    pick: Pick,
) -> Reply<Option<PathBuf>> {
    let started = Instant::now();
    let spec = pick.kind.spec();
    let options = candidates(app, state, &spec, pick.first);
    let start = tauri::async_runtime::spawn_blocking(move || first_folder(options, FOLDER_WAIT))
        .await
        .unwrap_or(None);
    log::debug!(
        "dialog {:?}: starting in {:?} (chosen in {} ms)",
        pick.kind,
        start,
        started.elapsed().as_millis()
    );
    let mut dialog = app
        .dialog()
        .file()
        .set_title(pick.title.unwrap_or_else(|| spec.title.to_string()));
    for (name, extensions) in spec.filters {
        dialog = dialog.add_filter(*name, extensions);
    }
    if let Some(folder) = &start {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = pick.file_name {
        dialog = dialog.set_file_name(name);
    }
    let window = app.get_webview_window("main");
    let mode = spec.mode;
    let asked = Instant::now();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        // A sheet on the main window (macOS); a dialog owned by it elsewhere.
        if let Some(window) = &window {
            dialog = dialog.set_parent(window);
        }
        match mode {
            Mode::Open => dialog.blocking_pick_file(),
            Mode::Save => dialog.blocking_save_file(),
            Mode::Folder => dialog.blocking_pick_folder(),
        }
    })
    .await
    .map_err(|_| "Something went wrong opening the file dialog.".to_string())?;
    let path = picked.and_then(|p| p.into_path().ok());
    log::debug!(
        "dialog {:?}: answered after {} ms on screen ({})",
        pick.kind,
        asked.elapsed().as_millis(),
        if path.is_some() { "chosen" } else { "cancelled" }
    );
    if let Some(path) = &path {
        let folder = path.parent().filter(|p| !p.as_os_str().is_empty());
        if let Some(folder) = folder {
            state.last_folders.set(spec.key, folder);
        }
    }
    Ok(path)
}

/// Shows a dialog of `kind` (a save dialog suggests `name`); the path chosen as path text, or
/// null when cancelled. The path isn't trusted for anything by being chosen here: photos and
/// house models are picked with their own commands, which let the window read them.
#[tauri::command]
pub(crate) async fn pick_path<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    kind: PickKind,
    name: Option<String>,
) -> Reply<Option<String>> {
    let mut request = Pick::of(kind);
    request.file_name = name.filter(|n| !n.is_empty() && !n.contains(['/', '\\']));
    Ok(pick(&app, &state, request).await?.map(|p| path_to_text(&p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_real_folder_wins() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&b).unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "").unwrap();
        assert_eq!(
            first_folder(vec![a, file, b.clone(), dir.path().to_path_buf()], FOLDER_WAIT),
            Some(b)
        );
        assert_eq!(first_folder(vec![], FOLDER_WAIT), None);
        assert_eq!(first_folder(vec![dir.path().join("none")], FOLDER_WAIT), None);
    }

    #[test]
    fn folders_are_remembered_per_kind_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let shows = dir.path().join("Shows");
        LastFolders::new(Some(config.clone())).set("show", &shows);
        let again = LastFolders::new(Some(config));
        assert_eq!(again.get("show"), Some(shows));
        assert_eq!(again.get("music"), None);
    }

    #[test]
    fn opening_and_saving_the_same_kind_share_a_folder() {
        assert_eq!(PickKind::Show.spec().key, PickKind::ShowSave.spec().key);
        assert_eq!(
            PickKind::SequenceDoc.spec().key,
            PickKind::SequenceDocSave.spec().key
        );
        assert_eq!(PickKind::Fseq.spec().key, PickKind::FseqExport.spec().key);
        assert_eq!(PickKind::XlightsFolder.spec().mode, Mode::Folder);
        assert_eq!(PickKind::ShowSave.spec().mode, Mode::Save);
    }
}
