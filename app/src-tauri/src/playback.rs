//! Sequence playback commands and the live preview.

use crate::{AppState, Reply, message};
use pf_engine::{PlaybackStatus, PreviewProp};
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
