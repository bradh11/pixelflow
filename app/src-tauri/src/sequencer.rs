//! Sequencer commands: authoring a sequence document, playing it live, exporting it to `.fseq`,
//! and detecting beats in its music.

use crate::{AppState, Reply, message};
use pf_analysis::Analysis;
use pf_engine::{
    Engine, EngineError, ExportLayout, ExportSummary, PlaybackStatus, SequenceEdit, SequenceEditResult,
    SequenceExport, SequenceSnapshot,
};
use pf_sequence::{EffectInfo, TimingTrack};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// The event an export sends as it goes (see [`ExportProgress`]).
pub(crate) const EXPORT_PROGRESS_EVENT: &str = "sequence-export-progress";

/// How far an export has got: sent as [`EXPORT_PROGRESS_EVENT`] each time the percentage
/// changes (and at the end), so at most about 100 times per export.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportProgress {
    /// The file being written (to tell exports apart).
    pub path: String,
    pub frames_done: u32,
    pub frames: u32,
    /// 0–100.
    pub percent: u32,
}

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

/// Applies a batch of sequence edits as one undo step and answers with what changed. Edits that
/// carry the same `gesture` id as the edit before (a drag sends one per move) merge into that
/// edit's undo step.
#[tauri::command]
pub(crate) async fn edit_sequence(
    state: State<'_, AppState>,
    edits: Vec<SequenceEdit>,
    gesture: Option<String>,
) -> Reply<SequenceEditResult> {
    state
        .engine()
        .edit_sequence_gesture(edits, gesture.as_deref())
        .map_err(message)
}

/// Every effect kind with its settings: labels, ranges, steps, units, defaults, and choices (the
/// same table the engine clamps and checks settings with).
#[tauri::command]
pub(crate) async fn effect_catalog() -> Reply<Vec<EffectInfo>> {
    Ok(pf_sequence::effect_catalog())
}

#[tauri::command]
pub(crate) async fn undo_sequence(state: State<'_, AppState>) -> Reply<SequenceEditResult> {
    state.engine().undo_sequence().map_err(message)
}

#[tauri::command]
pub(crate) async fn redo_sequence(state: State<'_, AppState>) -> Reply<SequenceEditResult> {
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

/// Exports the open sequence as an `.fseq` file, sending [`EXPORT_PROGRESS_EVENT`] events as it
/// goes; [`cancel_sequence_export`] stops it (the reply is then "The export was cancelled." and
/// no file is written). Rendering runs off the engine lock, so the app stays responsive.
#[tauri::command]
pub(crate) async fn export_sequence_doc<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Reply<ExportSummary> {
    let job = state.engine().sequence_export().map_err(message)?;
    let started = state.export_cancels.load(Ordering::Acquire);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        run_export(&job, &path, &state.export_cancels, started, |progress| {
            // A window that's gone can't show progress; the export carries on.
            let _ = handle.emit(EXPORT_PROGRESS_EVENT, progress);
        })
    })
    .await
    .map_err(|_| "Something went wrong exporting the sequence.".to_string())?
    .map_err(message)
}

/// Runs an export, reporting progress each time the percentage changes, until done or until
/// `cancels` moves past `started`.
pub(crate) fn run_export(
    job: &SequenceExport,
    path: &Path,
    cancels: &AtomicU64,
    started: u64,
    mut report: impl FnMut(ExportProgress),
) -> Result<ExportSummary, EngineError> {
    let shown = path.display().to_string();
    let mut last_percent = None;
    job.run(path, |done, total| {
        let percent = (u64::from(done) * 100 / u64::from(total.max(1))) as u32;
        if last_percent != Some(percent) || done == total {
            last_percent = Some(percent);
            report(ExportProgress {
                path: shown.clone(),
                frames_done: done,
                frames: total,
                percent,
            });
        }
        cancels.load(Ordering::Acquire) == started
    })
}

/// Cancels the exports running now (they stop before their next frame).
#[tauri::command]
pub(crate) async fn cancel_sequence_export(state: State<'_, AppState>) -> Reply<()> {
    state.export_cancels.fetch_add(1, Ordering::AcqRel);
    Ok(())
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
/// (replacing earlier ones), as one undo step. The analysis runs without holding the engine; the
/// tracks are only added if the same sequence, with the same music, is still open.
#[tauri::command]
pub(crate) async fn detect_beats(state: State<'_, AppState>) -> Reply<SequenceEditResult> {
    let (doc, music) = {
        let engine = state.engine();
        let doc = engine
            .sequence_doc_id()
            .ok_or_else(|| EngineError::NoSequence.to_string())?;
        let music = engine
            .sequence_music()
            .ok_or_else(|| "This sequence has no music yet. Choose a song for it first.".to_string())?;
        (doc, music)
    };
    let analyzed = music.clone();
    let analysis = tauri::async_runtime::spawn_blocking(move || pf_analysis::analyze_file(&analyzed))
        .await
        .map_err(|_| "Something went wrong analyzing the music.".to_string())?
        .map_err(|e| e.to_string())?;
    add_detected_tracks(&mut state.engine(), doc, &music, analysis.timing_tracks())
}

/// Adds detected timing tracks, if the sequence `doc` with music `music` is still the open one.
pub(crate) fn add_detected_tracks(
    engine: &mut Engine,
    doc: u64,
    music: &Path,
    tracks: Vec<TimingTrack>,
) -> Reply<SequenceEditResult> {
    if engine.sequence_doc_id() != Some(doc) || engine.sequence_music().as_deref() != Some(music) {
        return Err(
            "The sequence or its music changed while the beats were being found. Run beat detection again."
                .to_string(),
        );
    }
    engine.replace_timing_tracks(tracks).map_err(message)
}
