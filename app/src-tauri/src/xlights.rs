//! Importing an xLights show folder.

use crate::{AppState, Reply, message};
use pf_engine::ShowSnapshot;
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

/// Imports the xLights show in `folder` as a new, unsaved show. Reading and converting the files
/// happens off the engine lock.
#[tauri::command]
pub(crate) async fn import_xlights(state: State<'_, AppState>, folder: PathBuf) -> Reply<XlightsImported> {
    let imported = tauri::async_runtime::spawn_blocking(move || pf_xlights::import_folder(&folder))
        .await
        .map_err(|_| "Something went wrong reading the xLights show.".to_string())?
        .map_err(|e| e.to_string())?;
    let snapshot = state.engine().adopt_show(imported.show).map_err(message)?;
    Ok(XlightsImported {
        snapshot,
        summary: imported.summary,
        notes: imported.notes,
    })
}
