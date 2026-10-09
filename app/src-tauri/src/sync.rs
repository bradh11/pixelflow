//! Preview sync: a metronome played through the same sound output as the music, so the window can
//! flash in time with what is heard and work out how far its picture is from the sound. Only the
//! window's own preview is moved by what's found; controllers, FPP and exports never are.

use crate::Reply;
use pf_audio::{AudioClock, MusicPlayer};
use serde::Serialize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::State;

/// The slowest and quickest click.
const INTERVAL_MS: (u64, u64) = (250, 2_000);

/// Opens a metronome clicking every given time, with what the sound output says about its delay.
pub(crate) type MetronomeOpener = Arc<
    dyn Fn(Duration) -> Result<(Box<dyn AudioClock + Send>, Option<pf_audio::OutputInfo>), String>
        + Send
        + Sync,
>;

/// The metronome playing now, if any.
pub(crate) struct SyncState {
    open: MetronomeOpener,
    playing: Mutex<Option<Box<dyn AudioClock + Send>>>,
}

impl SyncState {
    /// The real thing: clicks on the default sound output.
    pub(crate) fn live() -> Self {
        Self::with_opener(Arc::new(|every| {
            let player = MusicPlayer::metronome(every).map_err(|e| e.to_string())?;
            let output = player.output();
            Ok((Box::new(player) as Box<dyn AudioClock + Send>, output))
        }))
    }

    pub(crate) fn with_opener(open: MetronomeOpener) -> Self {
        Self {
            open,
            playing: Mutex::default(),
        }
    }

    fn playing(&self) -> std::sync::MutexGuard<'_, Option<Box<dyn AudioClock + Send>>> {
        self.playing.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A metronome that started: how often it clicks, and what the sound output says about its delay
/// (the time one buffer takes; the device's own delay after that isn't reported).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncClick {
    pub interval_ms: u64,
    pub output_latency_ms: Option<f64>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
}

/// Starts a metronome clicking every `interval_ms` (the first click at 0), replacing one already
/// playing.
#[tauri::command]
pub(crate) async fn sync_click_start(state: State<'_, SyncState>, interval_ms: u64) -> Reply<SyncClick> {
    let interval_ms = interval_ms.clamp(INTERVAL_MS.0, INTERVAL_MS.1);
    state.playing().take();
    let open = Arc::clone(&state.open);
    let (mut clock, output) =
        tauri::async_runtime::spawn_blocking(move || open(Duration::from_millis(interval_ms)))
            .await
            .map_err(|_| "Something went wrong starting the click.".to_string())?
            .map_err(|e| format!("PixelFlow couldn't play the click: {e}"))?;
    clock.start(Duration::ZERO);
    match output {
        Some(o) => log::info!(
            "preview sync: sound output at {} Hz, {} frame buffers, about {:.1} ms from handing over to playing (the device's own delay isn't reported)",
            o.sample_rate,
            o.buffer_frames
                .map_or_else(|| "default".to_string(), |f| f.to_string()),
            o.latency.as_secs_f64() * 1000.0
        ),
        None => log::info!("preview sync: the sound output says nothing about its delay"),
    }
    *state.playing() = Some(clock);
    Ok(SyncClick {
        interval_ms,
        output_latency_ms: output.map(|o| o.latency.as_secs_f64() * 1000.0),
        sample_rate: output.map(|o| o.sample_rate),
        buffer_frames: output.and_then(|o| o.buffer_frames),
    })
}

/// Where the metronome is being heard (ms since its first click), or nothing when it isn't playing.
#[tauri::command]
pub(crate) async fn sync_click_position(state: State<'_, SyncState>) -> Reply<Option<f64>> {
    Ok(state
        .playing()
        .as_ref()
        .map(|c| c.position().as_secs_f64() * 1000.0))
}

#[tauri::command]
pub(crate) async fn sync_click_stop(state: State<'_, SyncState>) -> Reply<()> {
    state.playing().take();
    Ok(())
}
