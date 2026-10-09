//! Sequencer commands: authoring a sequence document, playing it live, exporting it to `.fseq`,
//! and detecting beats in its music.

use crate::progress::AudioTask;
use crate::{AppState, PathArg, Reply, message};
use pf_analysis::Analysis;
use pf_engine::{
    Engine, EngineError, ExportLayout, ExportSummary, PlaybackStatus, SequenceEdit, SequenceEditResult,
    SequenceExport, SequenceRecovery, SequenceSnapshot, ShowSnapshot,
};
use pf_sequence::{EffectInfo, Row, TimingKind, TimingTrack};
use serde::Serialize;
use std::path::Path;
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

/// Starts a new, unsaved sequence with its music, if any, and its first rows, if given (a row for
/// every prop and group), replacing the open one (the UI asks first if it has changes). It starts
/// with nothing to undo and no unsaved changes.
#[tauri::command]
pub(crate) async fn new_sequence_doc(
    state: State<'_, AppState>,
    name: String,
    duration_ms: u64,
    audio: Option<String>,
    rows: Option<Vec<Row>>,
) -> Reply<SequenceSnapshot> {
    state
        .engine()
        .new_sequence_doc_with_rows(&name, duration_ms, audio.as_deref(), rows.unwrap_or_default())
        .map_err(message)
}

/// Unsaved sequences an earlier run of PixelFlow kept (newest first), to offer back.
#[tauri::command]
pub(crate) async fn sequence_recoveries(state: State<'_, AppState>) -> Reply<Vec<SequenceRecovery>> {
    Ok(state.engine().sequence_recoveries())
}

/// Opens a kept unsaved sequence, with unsaved changes (replacing the open one; the UI asks
/// first if it has changes).
#[tauri::command]
pub(crate) async fn recover_sequence(state: State<'_, AppState>, id: String) -> Reply<SequenceSnapshot> {
    state.engine().recover_sequence(&id).map_err(message)
}

/// Throws away a kept unsaved sequence.
#[tauri::command]
pub(crate) async fn discard_sequence_recovery(state: State<'_, AppState>, id: String) -> Reply<()> {
    state.engine().discard_sequence_recovery(&id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn open_sequence_doc(state: State<'_, AppState>, path: String) -> Reply<SequenceSnapshot> {
    state
        .engine()
        .open_sequence_doc(&pf_model::path_from_text(&path))
        .map_err(message)
}

#[tauri::command]
pub(crate) async fn save_sequence_doc(state: State<'_, AppState>) -> Reply<SequenceSnapshot> {
    state.engine().save_sequence_doc().map_err(message)
}

#[tauri::command]
pub(crate) async fn save_sequence_doc_as(
    state: State<'_, AppState>,
    path: PathArg,
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

/// Whether a playing sequence document is sent to the controllers or only shown in the preview
/// (switches at once while playing). Returns the playback state, if anything is playing.
#[tauri::command]
pub(crate) async fn set_sequence_doc_output(
    state: State<'_, AppState>,
    send: bool,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().set_sequence_doc_output(send))
}

/// Whether the open sequence plays again from the top each time it reaches the end, its music
/// going back with it (switches at once while playing). Returns the playback state, if anything
/// is playing.
#[tauri::command]
pub(crate) async fn set_sequence_doc_loop(
    state: State<'_, AppState>,
    looping: bool,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().set_sequence_doc_loop(looping))
}

/// Adds an exported `.fseq` of the open sequence to the show's playlist (one undo step on the
/// show), named after the sequence and with its music.
#[tauri::command]
pub(crate) async fn add_sequence_doc_to_show(
    state: State<'_, AppState>,
    path: PathArg,
) -> Reply<ShowSnapshot> {
    state.engine().add_sequence_doc_to_show(&path).map_err(message)
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
    path: PathArg,
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
pub(crate) async fn analyze_audio(path: String) -> Reply<Analysis> {
    let path = pf_model::path_from_text(&path);
    tauri::async_runtime::spawn_blocking(move || pf_analysis::analyze_file(&path))
        .await
        .map_err(|_| "Something went wrong analyzing the music.".to_string())?
        .map_err(|e| e.to_string())
}

/// Detects beats in the open sequence's music and adds Beats, Bars, Onsets, and Drums timing
/// tracks (replacing earlier ones), and Sections, Accents, and Moments (unless the sequence has
/// them already: the user's own are kept), as one undo step; Accents, Moments, and Drums only
/// when the song has some. Moments find shouts in the sequence's sung words, if it has a words
/// track. The analysis runs without holding the engine; the tracks are only added if the same
/// sequence, with the same music, is still open.
#[tauri::command]
pub(crate) async fn detect_beats<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<SequenceEditResult> {
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
    let report = crate::progress::reporter(&app, AudioTask::Beats, &pf_model::path_to_text(&music));
    let analysis = tauri::async_runtime::spawn_blocking(move || {
        let found = pf_analysis::analyze_file_reporting(&analyzed, &|| false, &report);
        if found.is_err() {
            report(1.0);
        }
        found
    })
    .await
    .map_err(|_| "Something went wrong analyzing the music.".to_string())?
    .map_err(|e| e.to_string())?;
    let mut engine = state.engine();
    let moments = match engine.sequence_document() {
        Some(seq) => pf_ai::song::moments_track(&analysis, seq),
        None => analysis.moments_track(),
    };
    let mut tracks = analysis.timing_tracks();
    let found = [
        analysis.sections_track(),
        analysis.accents_track(),
        moments,
        analysis.drums_track(),
    ];
    tracks.extend(found.into_iter().filter(|t| !t.marks.is_empty()));
    add_detected_tracks(&mut engine, doc, &music, tracks)
}

/// What importing a timing file did: the edit's reply, the tracks added (by name, as they were
/// named in the sequence), and what didn't come across.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimingImported {
    pub result: SequenceEditResult,
    pub tracks: Vec<String>,
    pub notes: Vec<String>,
}

/// Imports the timing tracks in an xLights `.xtiming` file or an Audacity label file (`.txt`)
/// into the open sequence, after its other tracks, as one undo step. The file is read without
/// holding the engine; the tracks are only added if the same sequence is still open.
#[tauri::command]
pub(crate) async fn import_timing_file(state: State<'_, AppState>, path: PathArg) -> Reply<TimingImported> {
    let (doc, duration_ms) = {
        let engine = state.engine();
        let doc = engine
            .sequence_doc_id()
            .ok_or_else(|| EngineError::NoSequence.to_string())?;
        let duration = engine.sequence_document().map_or(0, |s| s.duration_ms);
        (doc, duration)
    };
    let import =
        tauri::async_runtime::spawn_blocking(move || pf_xlights::read_timing_file(&path, duration_ms))
            .await
            .map_err(|_| "Something went wrong reading the timing file.".to_string())?
            .map_err(|e| e.to_string())?;
    add_imported_tracks(&mut state.engine(), doc, import)
}

/// Adds tracks read from a timing file, if the sequence `doc` is still the open one.
pub(crate) fn add_imported_tracks(
    engine: &mut Engine,
    doc: u64,
    import: pf_xlights::TimingFileImport,
) -> Reply<TimingImported> {
    if engine.sequence_doc_id() != Some(doc) {
        return Err(
            "Another sequence was opened while the timing file was being read. Import it again.".to_string(),
        );
    }
    if import.tracks.is_empty() {
        return Err("That file has no timing marks PixelFlow can use.".to_string());
    }
    let before = engine.sequence_document().map_or(0, |s| s.timing_tracks.len());
    let result = engine.add_timing_tracks(import.tracks).map_err(message)?;
    let tracks = engine
        .sequence_document()
        .map(|s| s.timing_tracks[before..].iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default();
    Ok(TimingImported {
        result,
        tracks,
        notes: import.notes,
    })
}

/// The tracks an export of track `id` writes, as xLights layers: a lyrics track takes its words
/// and phonemes tracks ("Name (words)", "Name (phonemes)") along into `.xtiming` files.
pub(crate) fn export_layers(
    seq: &pf_sequence::Sequence,
    id: pf_sequence::TimingTrackId,
) -> Option<Vec<TimingTrack>> {
    let track = seq.timing_track(id)?;
    let mut layers = vec![track.clone()];
    if track.kind == pf_sequence::TimingKind::Lyrics {
        for (suffix, kind) in [
            ("words", pf_sequence::TimingKind::Words),
            ("phonemes", pf_sequence::TimingKind::Phonemes),
        ] {
            let name = format!("{} ({suffix})", track.name);
            match seq
                .timing_tracks
                .iter()
                .find(|t| t.name == name && t.kind == kind)
            {
                Some(layer) => layers.push(layer.clone()),
                None => break,
            }
        }
    }
    Some(layers)
}

/// Writes timing track `id` to `path`: an xLights timing file (`.xtiming` or `.xml`; a lyrics
/// track with its words and phonemes), or Audacity labels (`.txt`). Nothing else is written. The
/// track is copied out of the engine first; the file is written without holding it, all at once
/// (a failed write leaves any earlier file as it was). Returns how many marks went.
#[tauri::command]
pub(crate) async fn export_timing_track(
    state: State<'_, AppState>,
    id: pf_sequence::TimingTrackId,
    path: PathArg,
) -> Reply<usize> {
    let layers = {
        let engine = state.engine();
        let seq = engine
            .sequence_document()
            .ok_or_else(|| EngineError::NoSequence.to_string())?;
        export_layers(seq, id)
            .ok_or_else(|| "That timing track isn't in the sequence anymore.".to_string())?
    };
    tauri::async_runtime::spawn_blocking(move || write_timing_file(&path, &layers))
        .await
        .map_err(|_| "Something went wrong writing the timing file.".to_string())?
}

/// Writes `layers` (a track and its further layers) to `path`, by its extension.
pub(crate) fn write_timing_file(path: &Path, layers: &[TimingTrack]) -> Reply<usize> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let text = match extension.as_str() {
        "xtiming" | "xml" => pf_xlights::xtiming(&[layers.iter().collect()]),
        "txt" => pf_sequence::audacity_labels(&layers[0].marks),
        _ => {
            return Err(
                "Timing tracks are saved as xLights timing files (.xtiming) or Audacity labels (.txt)."
                    .to_string(),
            );
        }
    };
    pf_engine::write_atomic(path, text.as_bytes()).map_err(message)?;
    Ok(layers[0].marks.len())
}

/// Whether a detected track is one the user shapes by hand (sections, accents, moments): one
/// already in the sequence by that name is kept instead of replaced.
fn shaped_by_hand(track: &TimingTrack) -> bool {
    track.kind == TimingKind::Sections || track.name == "Accents" || track.name == "Moments"
}

/// Adds detected timing tracks, if the sequence `doc` with music `music` is still the open one.
/// Sections, Accents, and Moments already there are the user's and stay as they are.
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
    let have: Vec<String> = engine
        .sequence_document()
        .map(|d| d.timing_tracks.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default();
    let tracks = tracks
        .into_iter()
        .filter(|t| !(shaped_by_hand(t) && have.contains(&t.name)))
        .collect();
    engine.replace_timing_tracks(tracks).map_err(message)
}
