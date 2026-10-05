//! Sequencer commands: authoring a sequence document, playing it live, exporting it to `.fseq`,
//! and detecting beats in its music.

use crate::{AppState, Reply, message};
use pf_analysis::Analysis;
use pf_engine::{ExportLayout, ExportSummary, PlaybackStatus, SequenceEdit, SequenceSnapshot};
use std::path::PathBuf;
use tauri::State;
use tauri::ipc::Response;

/// Starts a new, unsaved sequence (replacing the open one; the UI asks first if it has changes).
#[tauri::command]
pub(crate) async fn new_sequence_doc(
    state: State<'_, AppState>,
    name: String,
    duration_ms: u64,
) -> Reply<SequenceSnapshot> {
    state
        .engine()
        .new_sequence_doc(&name, duration_ms)
        .map_err(message)
}

#[tauri::command]
pub(crate) async fn open_sequence_doc(state: State<'_, AppState>, path: PathBuf) -> Reply<SequenceSnapshot> {
    state.engine().open_sequence_doc(&path).map_err(message)
}

#[tauri::command]
pub(crate) async fn save_sequence_doc(state: State<'_, AppState>) -> Reply<SequenceSnapshot> {
    state.engine().save_sequence_doc().map_err(message)
}

#[tauri::command]
pub(crate) async fn save_sequence_doc_as(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Reply<SequenceSnapshot> {
    state.engine().save_sequence_doc_as(&path).map_err(message)
}

#[tauri::command]
pub(crate) async fn close_sequence_doc(state: State<'_, AppState>) -> Reply<()> {
    state.engine().close_sequence_doc();
    Ok(())
}

/// The open sequence, or null when none is open.
#[tauri::command]
pub(crate) async fn get_sequence_doc(state: State<'_, AppState>) -> Reply<Option<SequenceSnapshot>> {
    Ok(state.engine().sequence_doc())
}

/// Applies a batch of sequence edits as one undo step.
#[tauri::command]
pub(crate) async fn edit_sequence(
    state: State<'_, AppState>,
    edits: Vec<SequenceEdit>,
) -> Reply<SequenceSnapshot> {
    state.engine().edit_sequence(edits).map_err(message)
}

#[tauri::command]
pub(crate) async fn undo_sequence(state: State<'_, AppState>) -> Reply<SequenceSnapshot> {
    state.engine().undo_sequence().map_err(message)
}

#[tauri::command]
pub(crate) async fn redo_sequence(state: State<'_, AppState>) -> Reply<SequenceSnapshot> {
    state.engine().redo_sequence().map_err(message)
}

/// The open sequence as it looks at `position_ms` (show frame bytes, raw), for scrubbing; empty
/// when no sequence is open.
#[tauri::command]
pub(crate) async fn sequence_doc_frame(state: State<'_, AppState>, position_ms: u64) -> Reply<Response> {
    Ok(Response::new(
        state.engine().sequence_doc_frame(position_ms).unwrap_or_default(),
    ))
}

/// Plays the open sequence live with its music, through the show's output plan. Waits for the
/// music to open without holding the engine, so other commands carry on meanwhile.
#[tauri::command]
pub(crate) async fn play_sequence_doc(state: State<'_, AppState>, position_ms: u64) -> Reply<PlaybackStatus> {
    let ready = state.engine().begin_sequence_doc(position_ms).map_err(message)?;
    tauri::async_runtime::spawn_blocking(move || ready.wait())
        .await
        .map_err(|_| "Something went wrong starting the music.".to_string())?;
    state
        .engine()
        .playback_status()
        .ok_or_else(|| "Playback stopped before it started.".to_string())
}

/// How an export would lay out the controllers' channels (changes nothing).
#[tauri::command]
pub(crate) async fn sequence_export_layout(state: State<'_, AppState>) -> Reply<ExportLayout> {
    Ok(state.engine().sequence_export().map_err(message)?.layout())
}

/// Exports the open sequence as an `.fseq` file. Rendering runs off the engine lock, so the app
/// stays responsive.
#[tauri::command]
pub(crate) async fn export_sequence_doc(state: State<'_, AppState>, path: PathBuf) -> Reply<ExportSummary> {
    let job = state.engine().sequence_export().map_err(message)?;
    tauri::async_runtime::spawn_blocking(move || job.run(&path, |_, _| {}))
        .await
        .map_err(|_| "Something went wrong exporting the sequence.".to_string())?
        .map_err(message)
}

/// Finds the tempo, beats, bars, and onsets in a music file (changes nothing).
#[tauri::command]
pub(crate) async fn analyze_audio(path: PathBuf) -> Reply<Analysis> {
    tauri::async_runtime::spawn_blocking(move || pf_analysis::analyze_file(&path))
        .await
        .map_err(|_| "Something went wrong analyzing the music.".to_string())?
        .map_err(|e| e.to_string())
}

/// Detects beats in the open sequence's music and adds Beats, Bars, and Onsets timing tracks
/// (replacing earlier ones), as one undo step.
#[tauri::command]
pub(crate) async fn detect_beats(state: State<'_, AppState>) -> Reply<SequenceSnapshot> {
    let music = {
        let engine = state.engine();
        if engine.sequence_doc().is_none() {
            return Err(pf_engine::EngineError::NoSequence.to_string());
        }
        engine
            .sequence_music()
            .ok_or_else(|| "This sequence has no music yet. Choose a song for it first.".to_string())?
    };
    let analysis = tauri::async_runtime::spawn_blocking(move || pf_analysis::analyze_file(&music))
        .await
        .map_err(|_| "Something went wrong analyzing the music.".to_string())?
        .map_err(|e| e.to_string())?;
    state
        .engine()
        .replace_timing_tracks(analysis.timing_tracks())
        .map_err(message)
}
