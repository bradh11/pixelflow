//! Importing an xLights show folder, and xLights sequences onto the open show.

use crate::PathArg;
use crate::{AppState, Reply, message};
use pf_engine::{CheckedShow, SequenceSnapshot, ShowSnapshot};
use pf_xlights::vendor::{Mapping, Package};
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
    let from = folder.0.clone();
    let (show, summary, notes) = tauri::async_runtime::spawn_blocking(move || {
        let imported = pf_xlights::import_folder(&folder).map_err(|e| e.to_string())?;
        let show = CheckedShow::new(imported.show).map_err(message)?;
        Ok::<_, String>((show, imported.summary, imported.notes))
    })
    .await
    .map_err(|_| "Something went wrong reading the xLights show.".to_string())??;
    let snapshot = {
        let mut engine = state.engine();
        let snapshot = engine.adopt_show(show);
        // Its first save starts in the xLights folder.
        *state
            .imported_from
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((engine.show_generation(), from));
        snapshot
    };
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
///
/// With a `mapping` (see `vendor::inspect_xlights_sequence`), `path` may also be a vendor
/// package (`.zip`, `.xsqz`) or folder, and `sequence` one of the sequences in it: effects go
/// where the mapping says, and the mapping is remembered under `key` for next time. A zip's
/// music is copied into the show's `music` folder, or, while the show isn't saved, that of
/// `music_folder`, a folder the user picked in the shell's dialog. The pictures its Pictures
/// effects were found to use are copied into the show's `images` folder (while the show isn't
/// saved, they're used where they are, and a note says so).
#[tauri::command]
pub(crate) async fn import_xlights_sequence(
    state: State<'_, AppState>,
    path: PathArg,
    sequence: Option<String>,
    mapping: Option<Mapping>,
    key: Option<String>,
    music_folder: Option<PathArg>,
) -> Reply<XlightsSequenceImported> {
    let (show, pictures) = {
        let engine = state.engine();
        (engine.show().clone(), engine.picture_files())
    };
    let music = crate::vendor::music_folder(&state, music_folder.as_deref());
    let remember = mapping.clone().zip(key);
    let imported = tauri::async_runtime::spawn_blocking(move || {
        let mut imported = match mapping {
            None => pf_xlights::import_sequence_file(&path, &show, pf_audio::find_audio)
                .map_err(|e| e.to_string())?,
            Some(mapping) => {
                let package = Package::open(&path).map_err(|e| e.to_string())?;
                pf_xlights::vendor::import(
                    &package,
                    sequence.as_deref(),
                    &show,
                    &mapping,
                    music.as_deref(),
                    pf_audio::find_audio,
                )
                .map_err(|e| e.to_string())?
            }
        };
        // The pictures the import found on this computer become the show's own.
        let notes = pictures.adopt_all(&mut imported.sequence);
        imported.notes.splice(0..0, notes);
        Ok::<_, String>(imported)
    })
    .await
    .map_err(|_| "Something went wrong reading the xLights sequence.".to_string())??;
    if let Some((mapping, key)) = remember {
        let saved = std::sync::Arc::clone(&state.vendor_mappings);
        let _ =
            tauri::async_runtime::spawn_blocking(move || saved.set(&key, &mapping, crate::recent::now_ms()))
                .await;
    }
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
