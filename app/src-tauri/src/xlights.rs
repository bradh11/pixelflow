//! Importing an xLights show folder, and xLights sequences onto the open show.

use crate::PathArg;
use crate::{AppState, Reply, message};
use pf_engine::{CheckedShow, SequenceSnapshot, ShowSnapshot};
use pf_xlights::{ImportSummary, SequenceImportSummary};
use serde::Serialize;
use tauri::State;

/// The imported show and what wasn't imported exactly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XlightsImported {
    snapshot: ShowSnapshot,
    summary: ImportSummary,
    notes: Vec<String>,
}

/// Imports the xLights show in `folder` as a new, unsaved show. Reading, converting, and
/// checking the show all happen off the engine lock.
#[tauri::command]
pub(crate) async fn import_xlights(state: State<'_, AppState>, folder: PathArg) -> Reply<XlightsImported> {
    let (show, summary, notes) = tauri::async_runtime::spawn_blocking(move || {
        let imported = pf_xlights::import_folder(&folder).map_err(|e| e.to_string())?;
        let show = CheckedShow::new(imported.show).map_err(message)?;
        Ok::<_, String>((show, imported.summary, imported.notes))
    })
    .await
    .map_err(|_| "Something went wrong reading the xLights show.".to_string())??;
    let snapshot = state.engine().adopt_show(show);
    let snapshot = state.trusting(snapshot);
    Ok(XlightsImported {
        snapshot,
        summary,
        notes,
    })
}

/// The imported sequence, opened in the sequence editor, and what wasn't imported exactly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XlightsSequenceImported {
    snapshot: SequenceSnapshot,
    summary: SequenceImportSummary,
    notes: Vec<String>,
}

/// Imports the xLights sequence (`.xsq`) at `path` onto the open show and opens it as a new,
/// unsaved sequence. It replaces the open sequence without asking, so callers check for unsaved
/// changes first (the app's store does, through `get_sequence_doc`). Reading and converting
/// happen off the engine lock.
#[tauri::command]
pub(crate) async fn import_xlights_sequence(
    state: State<'_, AppState>,
    path: PathArg,
) -> Reply<XlightsSequenceImported> {
    let show = state.engine().show().clone();
    let imported = tauri::async_runtime::spawn_blocking(move || {
        pf_xlights::import_sequence_file(&path, &show, pf_audio::find_audio).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Something went wrong reading the xLights sequence.".to_string())??;
    let snapshot = state
        .engine()
        .adopt_sequence_doc(imported.sequence)
        .map_err(message)?;
    Ok(XlightsSequenceImported {
        snapshot,
        summary: imported.summary,
        notes: imported.notes,
    })
}
