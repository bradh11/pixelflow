//! Sequence playback commands and the live preview.

use crate::{AppState, Reply, message};
use pf_audio::Waveform;
use pf_engine::{PlaybackStatus, PreviewProp, ShowSnapshot};
use pf_model::SequenceId;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, PoisonError};
use std::time::SystemTime;
use tauri::State;
use tauri::ipc::Response;

/// Plays a rendered sequence (`.fseq`) from `position_ms` to the controllers that know their
/// sequence channels.
#[tauri::command]
pub(crate) async fn start_playback(
    state: State<'_, AppState>,
    path: PathBuf,
    position_ms: u64,
) -> Reply<PlaybackStatus> {
    state.engine().start_playback(&path, position_ms).map_err(message)
}

#[tauri::command]
pub(crate) async fn pause_playback(
    state: State<'_, AppState>,
    paused: bool,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().set_playback_paused(paused))
}

#[tauri::command]
pub(crate) async fn seek_playback(
    state: State<'_, AppState>,
    position_ms: u64,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().seek_playback(position_ms))
}

#[tauri::command]
pub(crate) async fn stop_playback(state: State<'_, AppState>) -> Reply<()> {
    state.engine().stop_playback();
    Ok(())
}

#[tauri::command]
pub(crate) async fn playback_status(state: State<'_, AppState>) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().playback_status())
}

/// Why playback was stopped by an edit to the show (a plain sentence), if it was.
#[tauri::command]
pub(crate) async fn playback_stop_reason(state: State<'_, AppState>) -> Reply<Option<String>> {
    Ok(state.engine().playback_stop_reason().map(str::to_string))
}

/// The props' current colors (show frame bytes), sent raw rather than as JSON; empty when
/// nothing is playing or testing.
#[tauri::command]
pub(crate) async fn live_frame(state: State<'_, AppState>) -> Reply<Response> {
    Ok(Response::new(state.engine().live_frame().unwrap_or_default()))
}

/// The playing sequence's current frame (every channel, as sent), raw; empty when nothing plays.
#[tauri::command]
pub(crate) async fn sequence_frame(state: State<'_, AppState>) -> Reply<Response> {
    Ok(Response::new(state.engine().sequence_frame().unwrap_or_default()))
}

/// Every prop's pixel positions for the 2D preview.
#[tauri::command]
pub(crate) async fn preview_props(state: State<'_, AppState>) -> Reply<Vec<PreviewProp>> {
    Ok(state.engine().preview_props())
}

/// Adds the sequence file at `path` to the show (finding its music next to it), as one undo step.
#[tauri::command]
pub(crate) async fn add_sequence(state: State<'_, AppState>, path: PathBuf) -> Reply<ShowSnapshot> {
    let entry = tauri::async_runtime::spawn_blocking(move || pf_engine::sequence_entry_for(&path))
        .await
        .map_err(|_| "Something went wrong reading the sequence.".to_string())?
        .map_err(message)?;
    state.engine().add_sequence(entry).map_err(message)
}

/// Plays one of the show's sequences with its music. Waits for the music to open without holding
/// the engine, so other commands carry on meanwhile.
#[tauri::command]
pub(crate) async fn play_sequence(
    state: State<'_, AppState>,
    id: SequenceId,
    position_ms: u64,
) -> Reply<PlaybackStatus> {
    let ready = state.engine().begin_sequence(id, position_ms).map_err(message)?;
    tauri::async_runtime::spawn_blocking(move || ready.wait())
        .await
        .map_err(|_| "Something went wrong starting the music.".to_string())?;
    state
        .engine()
        .playback_status()
        .ok_or_else(|| "Playback stopped before it started.".to_string())
}

#[tauri::command]
pub(crate) async fn set_playback_volume(
    state: State<'_, AppState>,
    volume: f32,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().set_playback_volume(volume))
}

/// The music file's loudness over time, for the timeline. Decoded once per version of the file
/// (a changed file is decoded again); asking again while it decodes waits for that decode.
#[tauri::command]
pub(crate) async fn audio_waveform(
    state: State<'_, AppState>,
    path: PathBuf,
    slices: usize,
) -> Reply<Waveform> {
    let slices = slices.clamp(1, 20_000);
    let file = path.clone();
    let version = tauri::async_runtime::spawn_blocking(move || std::fs::metadata(&file))
        .await
        .map_err(|_| "Something went wrong reading the music.".to_string())?
        .map_err(|_| format!("PixelFlow can't find the music file {}.", path.display()))?;
    let key = WaveformKey {
        path: path.clone(),
        slices,
        modified: version.modified().ok(),
        size: version.len(),
    };
    let cell = {
        let mut cache = state.waveforms.lock().unwrap_or_else(PoisonError::into_inner);
        if !cache.contains_key(&key) && cache.len() >= WAVEFORM_CACHE {
            cache.clear();
        }
        Arc::clone(cache.entry(key.clone()).or_default())
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        cell.get_or_init(|| pf_audio::waveform(&path, slices).map_err(|e| e.to_string()))
            .clone()
    })
    .await
    .map_err(|_| "Something went wrong reading the music.".to_string())?;
    if result.is_err() {
        // Let the next request try again (the file may be fixed by then).
        state
            .waveforms
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&key);
    }
    result
}

/// Which waveform: the file, how finely it's sliced, and the file's version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WaveformKey {
    path: PathBuf,
    slices: usize,
    modified: Option<SystemTime>,
    size: u64,
}

/// A waveform, decoded at most once (concurrent requests wait for the same decode).
pub(crate) type WaveformCell = Arc<OnceLock<Result<Waveform, String>>>;

/// How many waveforms to keep before starting over.
const WAVEFORM_CACHE: usize = 32;
