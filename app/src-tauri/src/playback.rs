//! Sequence playback commands and the live preview.

use crate::{AppState, Reply, message};
use pf_audio::Waveform;
use pf_engine::{Edit, PlaybackStatus, PreviewProp, ShowSnapshot};
use pf_model::SequenceId;
use std::path::PathBuf;
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
    state
        .engine()
        .apply(vec![Edit::AddSequence { sequence: entry }])
        .map_err(message)
}

/// Plays one of the show's sequences with its music.
#[tauri::command]
pub(crate) async fn play_sequence(
    state: State<'_, AppState>,
    id: SequenceId,
    position_ms: u64,
) -> Reply<PlaybackStatus> {
    state.engine().play_sequence(id, position_ms).map_err(message)
}

#[tauri::command]
pub(crate) async fn set_playback_volume(
    state: State<'_, AppState>,
    volume: f32,
) -> Reply<Option<PlaybackStatus>> {
    Ok(state.engine().set_playback_volume(volume))
}

/// The music file's loudness over time, for the timeline (decoded once, then cached).
#[tauri::command]
pub(crate) async fn audio_waveform(
    state: State<'_, AppState>,
    path: PathBuf,
    slices: usize,
) -> Reply<Waveform> {
    let key = (path.clone(), slices);
    if let Some(cached) = state
        .waveforms
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Ok(cached.clone());
    }
    let waveform =
        tauri::async_runtime::spawn_blocking(move || pf_audio::waveform(&path, slices.clamp(1, 20_000)))
            .await
            .map_err(|_| "Something went wrong reading the music.".to_string())?
            .map_err(|e| e.to_string())?;
    state
        .waveforms
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, waveform.clone());
    Ok(waveform)
}
