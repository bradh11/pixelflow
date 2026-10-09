//! The timeline's vocals lane: the song's lead vocal brought forward (see
//! [`pf_analysis::vocal_track`]), how loud it is every few milliseconds and where it starts each
//! sound, so syllable starts can be seen and marks snap to them. Worked out once per version of
//! the music file and kept in memory.

use crate::Reply;
use crate::progress::{AudioTask, reporter};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::SystemTime;
use tauri::{AppHandle, Runtime, State};

/// Songs whose vocals are kept.
const KEPT: usize = 4;

/// The lead vocal of a song, for drawing and snapping.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VocalLane {
    /// Value `i` stands for `offset_ms + i * hop_ms`.
    pub hop_ms: f64,
    pub offset_ms: f64,
    /// How loud the voice is, 0–255.
    pub levels: Vec<u8>,
    /// Where the voice starts a sound (ms), in order: pitched starts and consonants together.
    pub onsets: Vec<u64>,
}

impl VocalLane {
    pub(crate) fn from_track(track: &pf_analysis::VocalTrack) -> Self {
        let mut onsets: Vec<u64> = track
            .onsets
            .iter()
            .chain(&track.consonant_onsets)
            .copied()
            .collect();
        onsets.sort_unstable();
        onsets.dedup();
        Self {
            hop_ms: track.hop_ms,
            offset_ms: track.offset_ms,
            levels: track
                .energy
                .iter()
                .map(|e| (e.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
            onsets,
        }
    }
}

/// Which music file, and its version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    size: u64,
}

type Cell = Arc<OnceLock<Result<Arc<VocalLane>, String>>>;

/// Vocals worked out (or being worked out) by music file.
#[derive(Default)]
pub(crate) struct VocalsState {
    lanes: Mutex<HashMap<Key, Cell>>,
}

/// The lead vocal of the music file at `path`, worked out once per version of the file (sending
/// progress events as it goes); asking again while it's worked out waits for that.
#[tauri::command]
pub(crate) async fn vocal_lane<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, VocalsState>,
    path: String,
) -> Reply<Arc<VocalLane>> {
    let report = reporter(&app, AudioTask::Vocals, &path);
    let file = pf_model::path_from_text(&path);
    let meta_path = file.clone();
    let meta = tauri::async_runtime::spawn_blocking(move || std::fs::metadata(&meta_path))
        .await
        .map_err(|_| "Something went wrong reading the music.".to_string())?
        .map_err(|_| format!("PixelFlow can't find the music file {}.", file.display()))?;
    let key = Key {
        path: file.clone(),
        modified: meta.modified().ok(),
        size: meta.len(),
    };
    let cell = {
        let mut lanes = state.lanes.lock().unwrap_or_else(PoisonError::into_inner);
        if !lanes.contains_key(&key) && lanes.len() >= KEPT {
            lanes.clear();
        }
        Arc::clone(lanes.entry(key.clone()).or_default())
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        cell.get_or_init(|| {
            let found = pf_analysis::vocal_track_file(&file, &|| false, &report)
                .map(|track| Arc::new(VocalLane::from_track(&track)))
                .map_err(|e| e.to_string());
            if found.is_err() {
                report(1.0);
            }
            found
        })
        .clone()
    })
    .await
    .map_err(|_| "Something went wrong finding the vocals.".to_string())?;
    if result.is_err() {
        // The next request tries again (the file may be fixed by then).
        state
            .lanes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&key);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lane_keeps_the_loudness_and_every_onset_in_order() {
        let track = pf_analysis::VocalTrack {
            hop_ms: 5.0,
            offset_ms: 2.5,
            energy: vec![0.0, 0.5, 1.0, 1.4],
            onsets: vec![100, 300],
            consonant_onsets: vec![90, 300, 500],
            ..Default::default()
        };
        let lane = VocalLane::from_track(&track);
        assert_eq!(lane.levels, [0, 128, 255, 255]);
        assert_eq!(lane.onsets, [90, 100, 300, 500]);
        assert_eq!((lane.hop_ms, lane.offset_ms), (5.0, 2.5));
    }
}
