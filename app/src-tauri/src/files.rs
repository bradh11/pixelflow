//! Files the show refers to that aren't where they were: checking for them, finding them again
//! in the show's folder, or letting the user locate them.
//!
//! The disk is never read while the engine is locked: a check or search is copied out of the
//! engine, run on its own, and its result handed back. A dead network drive then slows only the
//! check, not every edit.
//!
//! The window never names the new place of a file. A search runs here, only in the show's own
//! folder (and the folders below it) and the folder its file says it was saved in, both known
//! from the show file on disk; Locate… asks the user with the system's file dialog. A found
//! photo or house model becomes readable by the window only then.

use crate::pickers::{Pick, PickKind};
use crate::{AppState, Reply, message};
use pf_engine::{FileRole, FilesFound, FoundFile, MissingFile, SequenceEditResult, ShowSnapshot};
use pf_model::path_from_text;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::State;

/// Runs `work` (which reads the disk) away from the engine and the window.
async fn off_lock<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong looking at the show's files.".to_string())
}

/// Looks at whether the show's files are there (those not looked at yet, or `all` of them) and
/// answers with the show. One check runs at a time: asked again meanwhile, it answers with the
/// show as it is.
#[tauri::command]
pub(crate) async fn check_files(state: State<'_, AppState>, all: bool) -> Reply<ShowSnapshot> {
    if state.checking_files.swap(true, Ordering::AcqRel) {
        return Ok(state.engine().snapshot());
    }
    let check = state.engine().file_check(all);
    let status = off_lock(move || check.run()).await;
    state.checking_files.store(false, Ordering::Release);
    let mut engine = state.engine();
    engine.publish_file_status(status?);
    Ok(engine.snapshot())
}

/// Looks for every missing file of the show (or only `file`) by name in the show's folder and
/// the folders below it, then where the show was saved, and points the show at the ones it
/// finds, as one undo step. The answer says what was found where, and what is still missing.
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
    let outcome = off_lock(move || search.run()).await?;
    let report = state.engine().use_found_files(outcome).map_err(message)?;
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
    let Some(path) = pick(&app, &state, file, name.as_deref()).await? else {
        return Ok(None);
    };
    located(&state, file, &path).await.map(Some)
}

/// Points `file` at `path`, which the user chose in the system's file dialog.
pub(crate) async fn located(state: &AppState, file: FileRole, path: &Path) -> Reply<ShowSnapshot> {
    let chosen = path.to_path_buf();
    off_lock(move || pf_engine::check_chosen_file(&chosen))
        .await?
        .map_err(message)?;
    let snapshot = state.engine().relink_file(file, path).map_err(message)?;
    trust(state, file, path.to_path_buf());
    Ok(snapshot)
}

/// The system's "open file" dialog for a file of `file`'s kind, titled with the file's name.
async fn pick<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    file: FileRole,
    name: Option<&str>,
) -> Reply<Option<PathBuf>> {
    let kind = match file {
        FileRole::Sequence { .. } => PickKind::Fseq,
        FileRole::Music { .. } | FileRole::SequenceDocMusic => PickKind::Music,
        FileRole::Photo => PickKind::Photo,
        FileRole::HouseModel => PickKind::HouseModel,
        FileRole::Picture => PickKind::Picture,
    };
    let mut request = Pick::of(kind);
    request.title = Some(match name {
        Some(name) => format!("Where is {name} now?"),
        None => "Choose the file".to_string(),
    });
    crate::pickers::pick(app, state, request).await
}

/// The open sequence's music, when it isn't where the sequence says.
#[tauri::command]
pub(crate) async fn sequence_music_missing(state: State<'_, AppState>) -> Reply<Option<MissingFile>> {
    let Some(check) = state.engine().sequence_music_check() else {
        return Ok(None);
    };
    off_lock(move || check.run()).await
}

/// What looking for the open sequence's music found.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MusicFound {
    /// Where it was found (and is now used), or `None`.
    pub found: Option<FoundFile>,
    /// The sequence edit that pointed it there (one undo step), when it was used.
    pub result: Option<SequenceEditResult>,
    /// True when the search stopped before looking everywhere.
    pub gave_up: bool,
}

/// Looks for the open sequence's missing music by name in the show's folder and the sequence's
/// (and the folders below them), and uses it when found, as one undo step on the sequence.
#[tauri::command]
pub(crate) async fn find_sequence_music(state: State<'_, AppState>) -> Reply<MusicFound> {
    let search = state.engine().sequence_music_search().map_err(message)?;
    let outcome = off_lock(move || search.run()).await?;
    let result = state
        .engine()
        .use_found_sequence_music(&outcome)
        .map_err(message)?;
    Ok(MusicFound {
        found: result.as_ref().and_then(|_| outcome.found.first().cloned()),
        result,
        gave_up: outcome.gave_up,
    })
}

/// Asks the user where the open sequence's music is now, and uses the file they choose (one
/// undo step on the sequence). `None` when they cancel.
#[tauri::command]
pub(crate) async fn locate_sequence_music<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<SequenceEditResult>> {
    let name = state
        .engine()
        .sequence_music()
        .map(|music| pf_model::file_name_of(&pf_model::path_to_text(&music)));
    let Some(path) = pick(&app, &state, FileRole::SequenceDocMusic, name.as_deref()).await? else {
        return Ok(None);
    };
    let chosen = path.clone();
    off_lock(move || pf_engine::check_chosen_file(&chosen))
        .await?
        .map_err(message)?;
    state
        .engine()
        .relink_sequence_music(&path)
        .map(Some)
        .map_err(message)
}
