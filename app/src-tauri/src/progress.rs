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
    /// The lead vocal, for the timeline's vocals lane.
    Vocals,
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
            AudioTask::Vocals => "Finding the vocals",
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
    use crate::testing::{app, authored, call, heard, write_clicks};
    use serde_json::{Value, json};
    use tauri::test::MockRuntime;
    use tauri::{Manager, WebviewWindow};

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

    /// The reports sent so far, taken away so the next job's are counted from none.
    fn taken(events: &Mutex<Vec<Value>>) -> Vec<Value> {
        std::mem::take(&mut *events.lock().unwrap())
    }

    /// Checks `events` are one job's reports: all `task`'s on `path` under its label, never
    /// going backwards, and over (1) at the end. Returns how far each said the job was.
    fn one_job(events: &[Value], task: &str, stage: &str, path: &str) -> Vec<f64> {
        let wanted = json!({ "task": task, "path": path, "stage": stage });
        for event in events {
            let mut told = event.clone();
            assert!(
                told.as_object_mut().unwrap().remove("fraction").is_some(),
                "{event}"
            );
            assert_eq!(told, wanted);
        }
        let fractions: Vec<f64> = events.iter().map(|e| e["fraction"].as_f64().unwrap()).collect();
        assert!(
            fractions.windows(2).all(|pair| pair[0] <= pair[1]),
            "{fractions:?}"
        );
        assert_eq!(fractions.last(), Some(&1.0), "{fractions:?}");
        fractions
    }

    /// Like [`one_job`], for a job that reads a whole song: it says so before it's over.
    fn one_long_job(events: &[Value], task: &str, stage: &str, path: &str) {
        let fractions = one_job(events, task, stage, path);
        assert!(fractions.len() >= 2 && fractions[0] < 1.0, "{fractions:?}");
    }

    #[test]
    fn a_waveform_reports_its_reading_once_per_version_of_the_file() {
        let (app, webview, dir) = app();
        let song = dir.path().join("clicks.wav");
        write_clicks(&song);
        let path = pf_model::path_to_text(&song);
        let events = heard(&app, "audio-progress");
        let ask = || call(&webview, "audio_waveform", json!({ "path": path, "slices": 100 }));
        let waveform = ask().unwrap();
        one_long_job(&taken(&events), "waveform", "Reading the music", &path);
        // Kept: asked again, nothing is read, so nothing is reported.
        assert_eq!(ask().unwrap(), waveform);
        assert_eq!(taken(&events), Vec::<Value>::new());
    }

    #[test]
    fn a_file_that_cant_be_read_ends_its_bar_and_is_read_again_when_asked_again() {
        let (app, webview, dir) = app();
        let noise = dir.path().join("noise.wav");
        std::fs::write(&noise, b"not music at all").unwrap();
        let path = pf_model::path_to_text(&noise);
        // Beats are found in the open sequence's music.
        call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Noise", "durationMs": 1000, "audio": path }),
        )
        .unwrap();
        let events = heard(&app, "audio-progress");
        for (command, args, task, stage) in [
            ("detect_beats", json!({}), "beats", "Finding the beats"),
            (
                "audio_waveform",
                json!({ "path": path, "slices": 100 }),
                "waveform",
                "Reading the music",
            ),
            (
                "vocal_lane",
                json!({ "path": path }),
                "vocals",
                "Finding the vocals",
            ),
            (
                "probe_audio",
                json!({ "path": path }),
                "probe",
                "Reading the music",
            ),
        ] {
            for _ in 0..2 {
                call(&webview, command, args.clone()).unwrap_err();
                assert_eq!(one_job(&taken(&events), task, stage, &path), [1.0], "{command}");
            }
        }
    }

    #[test]
    fn a_length_the_header_gives_reports_only_that_its_over() {
        let (app, webview, dir) = app();
        let song = dir.path().join("silence.wav");
        std::fs::write(&song, pf_audio::wav_bytes(&vec![0.0; 8_000], 16_000)).unwrap();
        let path = pf_model::path_to_text(&song);
        let events = heard(&app, "audio-progress");
        let info = call(&webview, "probe_audio", json!({ "path": path })).unwrap();
        assert_eq!(
            (&info["durationMs"], &info["foundBy"]),
            (&json!(500), &json!("header"))
        );
        assert_eq!(
            one_job(&taken(&events), "probe", "Reading the music", &path),
            [1.0]
        );
    }

    #[test]
    fn the_vocals_lane_reports_its_reading() {
        let (app, webview, dir) = app();
        let song = dir.path().join("clicks.wav");
        write_clicks(&song);
        let path = pf_model::path_to_text(&song);
        let events = heard(&app, "audio-progress");
        call(&webview, "vocal_lane", json!({ "path": path })).unwrap();
        one_long_job(&taken(&events), "vocals", "Finding the vocals", &path);
    }

    /// A sequence (see `authored`) whose music is a song with a beat. Returns the song, as the
    /// window names it.
    fn authored_with_music(webview: &WebviewWindow<MockRuntime>, dir: &Path) -> String {
        authored(webview);
        let song = dir.join("clicks.wav");
        write_clicks(&song);
        let path = pf_model::path_to_text(&song);
        call(
            webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "updateInfo", "name": "Song", "audio": path,
                                "durationMs": 2000, "frameMs": 25 }] }),
        )
        .unwrap();
        path
    }

    #[test]
    fn finding_the_beats_reports_on_the_sequences_music() {
        let (app, webview, dir) = app();
        let path = authored_with_music(&webview, dir.path());
        let events = heard(&app, "audio-progress");
        // Analyzing a file for its own sake reports nothing.
        call(&webview, "analyze_audio", json!({ "path": path })).unwrap();
        assert_eq!(taken(&events), Vec::<Value>::new());
        call(&webview, "detect_beats", json!({})).unwrap();
        one_long_job(&taken(&events), "beats", "Finding the beats", &path);
    }

    #[test]
    fn an_export_reports_the_audio_track_it_works_out_first() {
        let (app, webview, dir) = app();
        // As the app does when it starts.
        app.state::<crate::AppState>()
            .engine()
            .set_audio_progress(Some(std::sync::Arc::new(audio_track_reporter(app.handle()))));
        let path = authored_with_music(&webview, dir.path());
        let events = heard(&app, "audio-progress");
        let export = || {
            call(
                &webview,
                "export_sequence_doc",
                json!({ "path": dir.path().join("Song.fseq") }),
            )
            .unwrap()
        };
        export();
        one_long_job(
            &taken(&events),
            "audioTrack",
            "Getting the music ready for effects",
            &path,
        );
        // Kept for the song: the next export has nothing to work out.
        export();
        assert_eq!(taken(&events), Vec::<Value>::new());
    }
}
