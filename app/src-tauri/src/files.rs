//! Files the show refers to that aren't where they were: finding them again in the show's
//! folder, or letting the user locate them.
//!
//! The window never names the new place of a file. A search runs here, only in the show's own
//! folder (and the folders below it), and Locate… asks the user with the system's file dialog;
//! a found photo or house model becomes readable by the window only then.

use crate::{AppState, Reply, message};
use pf_engine::{FileRole, FilesFound, FoundFile, MissingFile, SequenceEditResult, ShowSnapshot};
use pf_model::path_from_text;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// Looks for every missing file of the show (or only `file`) by name in the show's folder and
/// the folders below it, and points the show at the ones it finds, as one undo step. The
/// answer says what was found where, and what is still missing.
#[tauri::command]
pub(crate) async fn find_missing_files(
    state: State<'_, AppState>,
    file: Option<FileRole>,
) -> Reply<FilesFound> {
    let search = state.engine().file_search().map_err(message)?;
    let search = match file {
        Some(file) => search.only(file),
        None => search,
    };
    let folders = search.folders().to_vec();
    let found = tauri::async_runtime::spawn_blocking(move || search.run())
        .await
        .map_err(|_| "Something went wrong looking for the files.".to_string())?;
    let report = state.engine().use_found_files(found).map_err(message)?;
    for file in &report.found {
        trust_found(&state, file, &folders);
    }
    Ok(report)
}

/// Lets the window show a photo or house model the search found, when it is inside a searched
/// folder (which it always is; checked again here, since this decides what the window may read).
fn trust_found(state: &AppState, file: &FoundFile, folders: &[PathBuf]) {
    let path = path_from_text(&file.to);
    if !folders.iter().any(|folder| path.starts_with(folder)) {
        return;
    }
    trust(state, file.file, path);
}

/// Lets the window read `path` as the show's photo or house model.
fn trust(state: &AppState, file: FileRole, path: PathBuf) {
    match file {
        FileRole::Photo => state.photos.add(path),
        FileRole::HouseModel => state.models.add(path),
        _ => {}
    }
}

/// Asks the user where one of the show's files is now (the system's file dialog), and points
/// the show at the file they choose, as one undo step. `None` when they cancel.
#[tauri::command]
pub(crate) async fn locate_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    file: FileRole,
) -> Reply<Option<ShowSnapshot>> {
    let name = state
        .engine()
        .missing_files()
        .into_iter()
        .find(|m| m.file == file)
        .map(|m| m.name);
    let Some(path) = pick(&app, file, name.as_deref()).await? else {
        return Ok(None);
    };
    located(&state, file, &path).map(Some)
}

/// Points `file` at `path`, which the user chose in the system's file dialog.
pub(crate) fn located(state: &AppState, file: FileRole, path: &Path) -> Reply<ShowSnapshot> {
    let snapshot = state.engine().relink_file(file, path).map_err(message)?;
    trust(state, file, path.to_path_buf());
    Ok(snapshot)
}

/// The system's "open file" dialog for a file of `file`'s kind, titled with the file's name.
async fn pick<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file: FileRole,
    name: Option<&str>,
) -> Reply<Option<PathBuf>> {
    let (kind, extensions): (&str, &[&str]) = match file {
        FileRole::Sequence { .. } => ("FPP sequence", &["fseq"]),
        FileRole::Music { .. } | FileRole::SequenceDocMusic => {
            ("Music", &["mp3", "m4a", "wav", "ogg", "flac"])
        }
        FileRole::Photo => ("Photo", crate::layout::IMAGE_EXTENSIONS),
        FileRole::HouseModel => ("3D model", crate::house::MODEL_EXTENSIONS),
    };
    let title = match name {
        Some(name) => format!("Where is {name} now?"),
        None => "Choose the file".to_string(),
    };
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .add_filter(kind, extensions)
            .set_title(title)
            .blocking_pick_file()
    })
    .await
    .map_err(|_| "Something went wrong opening the file dialog.".to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()))
}

/// The open sequence's music, when it isn't where the sequence says.
#[tauri::command]
pub(crate) async fn sequence_music_missing(state: State<'_, AppState>) -> Reply<Option<MissingFile>> {
    Ok(state.engine().sequence_music_missing())
}

/// What looking for the open sequence's music found.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MusicFound {
    /// Where it was found (and is now used), or `None`.
    pub found: Option<FoundFile>,
    /// The sequence edit that pointed it there (one undo step), when it was used.
    pub result: Option<SequenceEditResult>,
}

/// Looks for the open sequence's missing music by name in the sequence's folder and the show's
/// (and the folders below them), and uses it when found, as one undo step on the sequence.
#[tauri::command]
pub(crate) async fn find_sequence_music(state: State<'_, AppState>) -> Reply<MusicFound> {
    let search = state.engine().sequence_music_search().map_err(message)?;
    let found = tauri::async_runtime::spawn_blocking(move || search.run())
        .await
        .map_err(|_| "Something went wrong looking for the music.".to_string())?;
    let Some(found) = found.into_iter().next() else {
        return Ok(MusicFound {
            found: None,
            result: None,
        });
    };
    let result = state.engine().use_found_sequence_music(&found).map_err(message)?;
    Ok(MusicFound {
        found: result.as_ref().map(|_| found),
        result,
    })
}

/// Asks the user where the open sequence's music is now, and uses the file they choose (one
/// undo step on the sequence). `None` when they cancel.
#[tauri::command]
pub(crate) async fn locate_sequence_music<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<SequenceEditResult>> {
    let name = state.engine().sequence_music_missing().map(|m| m.name);
    let Some(path) = pick(&app, FileRole::SequenceDocMusic, name.as_deref()).await? else {
        return Ok(None);
    };
    state
        .engine()
        .relink_sequence_music(&path)
        .map(Some)
        .map_err(message)
}
