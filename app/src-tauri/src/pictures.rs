//! The picture files Picture effects draw: choosing one, showing it in the effect's settings,
//! and finding the ones that have gone missing.
//!
//! A picture the user chooses (the system's file dialog, shown here) is copied into an `images`
//! folder next to the show file, and the effect stores `images/<name>`. Only that folder and
//! files chosen this session are ever read (see `pf_engine::PictureFiles`): a path the window
//! puts in an effect is not enough on its own. As with the show's other files, the disk is
//! never read while the engine is locked.

use crate::pickers::{Pick, PickKind};
use crate::{AppState, Reply, message};
use pf_engine::{MissingFile, SequenceEditResult};
use pf_sequence::SequenceIssue;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{Emitter, State};

/// Sent to the window when a picture a preview was waiting for has been read (or couldn't be),
/// so it draws the frame again.
pub(crate) const PICTURES_EVENT: &str = "pictures-arrived";
/// Larger pictures aren't sent to the window to show in the settings.
const MAX_SHOWN_BYTES: usize = 16 * 1024 * 1024;

/// Runs `work` (which reads the disk) away from the engine and the window.
async fn off_lock<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong looking at the pictures.".to_string())
}

/// Tells the window whenever a picture read in the background has arrived.
pub(crate) fn report_arrivals<R: tauri::Runtime>(app: &tauri::AppHandle<R>, engine: &pf_engine::Engine) {
    let app = app.clone();
    engine.pictures().on_arrival(Some(std::sync::Arc::new(move || {
        // A window that's gone has nothing to draw.
        let _ = app.emit(PICTURES_EVENT, ());
    })));
}

/// Takes `path` (a file the user chose) for the show, and makes sure it's read afresh: what an
/// effect stores for it.
async fn adopt(state: &AppState, path: std::path::PathBuf) -> Reply<String> {
    let files = state.engine().picture_files();
    let stored = off_lock(move || files.adopt(&path)).await??;
    state.engine().pictures().forget(&stored);
    Ok(stored)
}

/// Asks the user for a picture for a Picture effect. The one they pick is copied into the
/// show's images folder (while the show isn't saved, it's used where it is), and may then be
/// read. Returns what the effect stores for it; `None` when they cancel.
#[tauri::command]
pub(crate) async fn pick_picture<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<String>> {
    let Some(path) = crate::pickers::pick(&app, &state, Pick::of(PickKind::Picture)).await? else {
        return Ok(None);
    };
    adopt(&state, path).await.map(Some)
}

/// The bytes of the picture a Picture effect's `file` setting names, sent raw so the window can
/// show it without any file access of its own. Only pictures in the show's images folder, or
/// chosen this session, are read.
#[tauri::command]
pub(crate) async fn read_picture(state: State<'_, AppState>, file: String) -> Reply<Response> {
    let files = state.engine().picture_files();
    let name = pf_model::file_name_of(&file);
    let bytes = off_lock(move || files.read(&file))
        .await?
        .map_err(|why| format!("{name} can't be shown: {why}."))?;
    if bytes.len() > MAX_SHOWN_BYTES {
        return Err(format!("{name} is too large to show here."));
    }
    Ok(Response::new(bytes))
}

/// The pictures in the show's images folder, as a Picture effect's `file` setting names them.
#[tauri::command]
pub(crate) async fn list_pictures(state: State<'_, AppState>) -> Reply<Vec<String>> {
    let files = state.engine().picture_files();
    off_lock(move || files.listed()).await
}

/// The open sequence's pictures that can't be drawn, and its problems (which name them).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PicturesChecked {
    pub missing: Vec<MissingFile>,
    pub issues: Vec<SequenceIssue>,
}

async fn checked(state: &AppState) -> Reply<PicturesChecked> {
    let Some(check) = state.engine().sequence_picture_check() else {
        return Ok(PicturesChecked {
            missing: Vec::new(),
            issues: Vec::new(),
        });
    };
    let status = off_lock(move || check.run()).await?;
    let mut engine = state.engine();
    let missing = engine.publish_picture_status(status);
    Ok(PicturesChecked {
        missing,
        issues: engine.sequence_issues(),
    })
}

/// Looks at whether the open sequence's pictures are there. A picture that changed on disk is
/// read again the next time it's drawn.
#[tauri::command]
pub(crate) async fn check_sequence_pictures(state: State<'_, AppState>) -> Reply<PicturesChecked> {
    checked(&state).await
}

/// What finding or locating the open sequence's pictures did.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PicturesRelinked {
    /// The names of the pictures now pointed at where they are.
    pub found: Vec<String>,
    /// The sequence edit that pointed the effects there (one undo step), when any changed.
    pub result: Option<SequenceEditResult>,
    /// The pictures still missing, and the sequence's problems now.
    pub missing: Vec<MissingFile>,
    pub issues: Vec<SequenceIssue>,
    /// True when the search stopped before looking everywhere.
    pub gave_up: bool,
}

/// Looks for the open sequence's missing pictures by name in the show's folder and the
/// sequence's (and the folders below them). The ones found are copied into the show's images
/// folder and the effects pointed at them, as one undo step on the sequence.
#[tauri::command]
pub(crate) async fn find_sequence_pictures(state: State<'_, AppState>) -> Reply<PicturesRelinked> {
    let (search, files) = {
        let engine = state.engine();
        (
            engine.sequence_picture_search().map_err(message)?,
            engine.picture_files(),
        )
    };
    let folders = search.folders().to_vec();
    let (outcome, stored) = off_lock(move || {
        let outcome = search.run();
        // Only what's inside a searched folder (which a find always is) is ever taken.
        let stored: Vec<(String, String, String)> = outcome
            .found
            .iter()
            .filter(|f| {
                let to = pf_model::path_from_text(&f.to);
                folders.iter().any(|folder| to.starts_with(folder))
            })
            .filter_map(|f| {
                let stored = files.adopt(&pf_model::path_from_text(&f.to)).ok()?;
                Some((f.from.clone(), stored, f.name.clone()))
            })
            .collect();
        (outcome, stored)
    })
    .await?;
    let changes: Vec<(String, String)> = stored
        .iter()
        .map(|(from, to, _)| (from.clone(), to.clone()))
        .collect();
    let result = {
        let mut engine = state.engine();
        for (_, to, _) in &stored {
            engine.pictures().forget(to);
        }
        engine
            .use_found_sequence_pictures(&outcome, &changes)
            .map_err(message)?
    };
    let now = checked(&state).await?;
    Ok(PicturesRelinked {
        found: stored.into_iter().map(|(_, _, name)| name).collect(),
        result: result.filter(|r| r.changed),
        missing: now.missing,
        issues: now.issues,
        gave_up: outcome.gave_up,
    })
}

/// Asks the user where one of the open sequence's missing pictures is now (`path` as the
/// missing picture lists it), takes the file they choose for the show, and points the effects
/// at it, as one undo step on the sequence. `None` when they cancel.
#[tauri::command]
pub(crate) async fn locate_sequence_picture<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    path: String,
) -> Reply<Option<PicturesRelinked>> {
    let name = pf_model::file_name_of(&path);
    let mut request = Pick::of(PickKind::Picture);
    request.title = Some(format!("Where is {name} now?"));
    let Some(chosen) = crate::pickers::pick(&app, &state, request).await? else {
        return Ok(None);
    };
    let stored = adopt(&state, chosen).await?;
    let result = state
        .engine()
        .relink_sequence_pictures(&[(path, stored)])
        .map_err(message)?;
    let now = checked(&state).await?;
    Ok(Some(PicturesRelinked {
        found: vec![name],
        result: Some(result).filter(|r| r.changed),
        missing: now.missing,
        issues: now.issues,
        gave_up: false,
    }))
}
