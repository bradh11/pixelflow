//! Importing an xLights show folder.

use crate::{AppState, Reply, message};
use pf_engine::{CheckedShow, ShowSnapshot};
use pf_xlights::ImportSummary;
use serde::Serialize;
use std::path::PathBuf;
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
pub(crate) async fn import_xlights(state: State<'_, AppState>, folder: PathBuf) -> Reply<XlightsImported> {
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
