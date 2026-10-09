//! How far long work on a music file has got, sent to the window as [`AUDIO_PROGRESS_EVENT`]
//! events for its progress bars: reading the music for its waveform or its length, working out
//! the audio track effects follow, finding the beats, and Find lyrics' on-device alignment. At
//! most about ten a second.

use serde::Serialize;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};

/// The event long work on a music file reports its progress with.
pub(crate) const AUDIO_PROGRESS_EVENT: &str = "audio-progress";

/// The least time between two events of one job (its start and end always go).
const MIN_GAP: Duration = Duration::from_millis(100);

/// What's being worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AudioTask {
    /// The music's length, when its header doesn't say.
    Probe,
    /// The music's loudness, for the timeline.
    Waveform,
    /// What effects that follow the music read.
    AudioTrack,
    /// Detect beats.
    Beats,
    /// Find lyrics' on-device alignment, bringing the voice forward.
    Separate,
    /// Find lyrics' on-device alignment, hearing the song letter by letter.
    Align,
}

impl AudioTask {
    /// What's being done, for the label by the bar.
    pub(crate) fn stage(self) -> &'static str {
        match self {
            AudioTask::Probe | AudioTask::Waveform => "Reading the music",
            AudioTask::AudioTrack => "Getting the music ready for effects",
            AudioTask::Beats => "Finding the beats",
            AudioTask::Separate => "Separating the vocals",
            AudioTask::Align => "Aligning the words",
        }
    }
}

/// One step forward in a job.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioProgress {
    pub task: AudioTask,
    /// The music file, as the window names it.
    pub path: String,
    pub stage: &'static str,
    /// How much is done (0–1); 1 when it's over, done or not.
    pub fraction: f32,
}

/// Lets a job's progress through at most once per gap, but always its start (0) and end (1).
#[derive(Debug)]
pub(crate) struct Throttle {
    gap: Duration,
    last: Mutex<Option<Instant>>,
}

impl Throttle {
    pub(crate) fn new(gap: Duration) -> Self {
        Self {
            gap,
            last: Mutex::new(None),
        }
    }

    /// Whether `fraction`, reported at `now`, goes out.
    pub(crate) fn due(&self, fraction: f32, now: Instant) -> bool {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let edge = fraction <= 0.0 || fraction >= 1.0;
        let due = edge || last.is_none_or(|at| now.saturating_duration_since(at) >= self.gap);
        if due {
            *last = Some(now);
        }
        due
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(MIN_GAP)
    }
}

/// Sends `task`'s progress on `path` (named as the window named it) to the window, throttled.
pub(crate) fn reporter<R: Runtime>(
    app: &AppHandle<R>,
    task: AudioTask,
    path: &str,
) -> impl Fn(f32) + Send + Sync + use<R> {
    let app = app.clone();
    let path = path.to_string();
    let throttle = Throttle::default();
    move |fraction| {
        if throttle.due(fraction, Instant::now()) {
            // A window that's gone can't show progress; the work carries on.
            let _ = app.emit(
                AUDIO_PROGRESS_EVENT,
                AudioProgress {
                    task,
                    path: path.clone(),
                    stage: task.stage(),
                    fraction,
                },
            );
        }
    }
}

/// Sends the progress of each song's audio track the engine works out (see
/// `pf_engine::AudioTracks::set_progress`).
pub(crate) fn audio_track_reporter<R: Runtime>(
    app: &AppHandle<R>,
) -> impl Fn(&Path, f32) + Send + Sync + use<R> {
    let app = app.clone();
    let throttle = Throttle::default();
    move |path, fraction| {
        if throttle.due(fraction, Instant::now()) {
            let _ = app.emit(
                AUDIO_PROGRESS_EVENT,
                AudioProgress {
                    task: AudioTask::AudioTrack,
                    path: pf_model::path_to_text(path),
                    stage: AudioTask::AudioTrack.stage(),
                    fraction,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_one_report_per_gap_but_always_the_start_and_end() {
        let throttle = Throttle::new(Duration::from_millis(100));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(throttle.due(0.0, at(0)));
        assert!(!throttle.due(0.1, at(10)));
        assert!(!throttle.due(0.2, at(99)));
        assert!(throttle.due(0.3, at(100)));
        assert!(!throttle.due(0.4, at(150)));
        assert!(throttle.due(1.0, at(160)));
        // A thousand reports over a second: about ten go out.
        let throttle = Throttle::new(Duration::from_millis(100));
        let sent = (0..1000)
            .filter(|&i| throttle.due(i as f32 / 1000.0, at(i)))
            .count();
        assert!((10..=11).contains(&sent), "{sent}");
        // A job's first report goes out even when it's not at 0.
        let fresh = Throttle::new(Duration::from_millis(100));
        assert!(fresh.due(0.5, at(0)));
    }

    #[test]
    fn progress_is_sent_camel_cased() {
        let json = serde_json::to_value(AudioProgress {
            task: AudioTask::AudioTrack,
            path: "/m/song.mp3".into(),
            stage: AudioTask::AudioTrack.stage(),
            fraction: 0.5,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "task": "audioTrack",
                "path": "/m/song.mp3",
                "stage": "Getting the music ready for effects",
                "fraction": 0.5,
            })
        );
    }
}
