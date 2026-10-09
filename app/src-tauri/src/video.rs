//! Exporting the open sequence as an MP4 video (see `pf_video`): what the export dialog can
//! offer, the export itself with its progress, and cancelling it.

use crate::progress::Throttle;
use crate::{AppState, PathArg, Reply};
use pf_engine::SequenceExport;
use pf_video::{Ffmpeg, Progress, Stage, VideoError, VideoOptions, VideoSummary};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime, State};

/// The event a video export sends as it goes (see [`VideoProgress`]).
pub(crate) const VIDEO_PROGRESS_EVENT: &str = "video-export-progress";

/// Bumped by `cancel_video_export`: an export started before the bump stops.
static CANCELS: AtomicU64 = AtomicU64::new(0);

/// The ffmpeg found on this computer, looked for once.
fn ffmpeg() -> Option<Ffmpeg> {
    static FOUND: OnceLock<Option<Ffmpeg>> = OnceLock::new();
    FOUND.get_or_init(pf_video::ffmpeg::find).clone()
}

/// What the export dialog can offer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoChoices {
    /// ffmpeg's H.264 encoder, when ffmpeg is installed ("x264" or "VideoToolbox").
    pub ffmpeg: Option<&'static str>,
    /// Whether the show has a layout photo that can go behind the props.
    pub photo: bool,
}

/// What the export dialog asks for.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoRequest {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub photo: bool,
    pub pixel_size: f32,
    /// Encode with ffmpeg (when it's installed).
    pub ffmpeg: bool,
}

/// How far a video export has got.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoProgress {
    /// The file being written (to tell exports apart).
    pub path: String,
    pub stage: Stage,
    /// What's being done, for the label by the bar.
    pub label: &'static str,
    /// 0–1 of this stage, or none when it can't tell.
    pub fraction: Option<f32>,
}

/// The export dialog's choices for the open show.
#[tauri::command]
pub(crate) async fn video_export_choices(state: State<'_, AppState>) -> Reply<VideoChoices> {
    let photo = photo_allowed(&state, &state.engine().show().background);
    let found = tauri::async_runtime::spawn_blocking(ffmpeg).await.unwrap_or(None);
    Ok(VideoChoices {
        ffmpeg: found.map(|f| match f.codec {
            pf_video::ffmpeg::H264Codec::X264 => "x264",
            pf_video::ffmpeg::H264Codec::VideoToolbox => "VideoToolbox",
        }),
        photo,
    })
}

/// Whether the show's photo may be read: only one the window may read too.
fn photo_allowed(state: &AppState, background: &Option<pf_model::Background>) -> bool {
    background
        .as_ref()
        .is_some_and(|b| state.photos.contains(&pf_model::path_from_text(&b.path)))
}

/// Exports the open sequence as an MP4 at `path`, sending [`VIDEO_PROGRESS_EVENT`] events as it
/// goes; [`cancel_video_export`] stops it (the reply is then "The export was cancelled." and no
/// file is written). Rendering runs off the engine lock, so the app stays responsive.
#[tauri::command]
pub(crate) async fn export_video<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathArg,
    request: VideoRequest,
) -> Reply<VideoSummary> {
    let job = state.engine().sequence_export().map_err(crate::message)?;
    let photo = request.photo && photo_allowed(&state, &job.show().background);
    let mut options = options_for(&request)?;
    options.photo = photo;
    if request.ffmpeg {
        options.ffmpeg = tauri::async_runtime::spawn_blocking(ffmpeg).await.unwrap_or(None);
        if options.ffmpeg.is_none() {
            return Err("ffmpeg isn't installed any more. Export with the built-in encoder instead.".into());
        }
    }
    let started = CANCELS.load(Ordering::Acquire);
    tauri::async_runtime::spawn_blocking(move || {
        let result = run_video_export(&job, &options, &path, &CANCELS, started, |progress| {
            // A window that's gone can't show progress; the export carries on.
            let _ = app.emit(VIDEO_PROGRESS_EVENT, progress);
        });
        if let Ok(summary) = &result {
            log::info!(
                "exported a {}p{} video of {} frames in {} ms",
                summary.height,
                summary.fps,
                summary.frames,
                summary.elapsed_ms
            );
        }
        result
    })
    .await
    .map_err(|_| "Something went wrong exporting the video.".to_string())?
    .map_err(|e| e.to_string())
}

/// Cancels the video exports running now.
#[tauri::command]
pub(crate) async fn cancel_video_export() -> Reply<()> {
    CANCELS.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

/// The export's options from the dialog's request, refusing sizes and rates it doesn't offer.
fn options_for(request: &VideoRequest) -> Reply<VideoOptions> {
    if !matches!((request.width, request.height), (1280, 720) | (1920, 1080)) {
        return Err("Videos are 720p (1280×720) or 1080p (1920×1080).".into());
    }
    if !matches!(request.fps, 30 | 60) {
        return Err("Videos are 30 or 60 frames a second.".into());
    }
    if !(request.pixel_size.is_finite() && (0.25..=4.0).contains(&request.pixel_size)) {
        return Err("The pixel size is out of range.".into());
    }
    Ok(VideoOptions {
        width: request.width,
        height: request.height,
        fps: request.fps,
        start_ms: request.start_ms,
        end_ms: request.end_ms,
        photo: request.photo,
        pixel_size: request.pixel_size,
        ffmpeg: None,
    })
}

/// Runs an export, reporting its progress at most about ten times a second (and each stage's
/// start and end), until done or until `cancels` moves past `started`.
pub(crate) fn run_video_export(
    job: &SequenceExport,
    options: &VideoOptions,
    path: &Path,
    cancels: &AtomicU64,
    started: u64,
    mut report: impl FnMut(VideoProgress),
) -> Result<VideoSummary, VideoError> {
    let shown = path.display().to_string();
    let mut throttle = (Stage::Rendering, Throttle::default());
    pf_video::export_video(job, options, path, |progress: Progress| {
        if throttle.0 != progress.stage {
            throttle = (progress.stage, Throttle::default());
        }
        let fraction = progress.fraction;
        if throttle.1.due(fraction.unwrap_or(0.0), Instant::now()) {
            report(VideoProgress {
                path: shown.clone(),
                stage: progress.stage,
                label: progress.stage.label(),
                fraction,
            });
        }
        cancels.load(Ordering::Acquire) == started
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_engine::{Edit, Engine};
    use pf_model::{Generator, Prop, Rgb, ShapeSource};
    use pf_sequence::{Effect, EffectKind, Row, Target};

    fn job(dir: &Path) -> SequenceExport {
        let mut engine = Engine::new(dir);
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 20,
                length: 2.0,
            }),
        );
        engine.apply(vec![Edit::AddProp { prop: prop.clone() }]).unwrap();
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0]
            .effects
            .push(Effect::new(EffectKind::On, 0, 1000).with_palette(vec![Rgb::new(0, 255, 0)]));
        engine
            .new_sequence_doc_with_rows("Song", 1000, None, vec![row])
            .unwrap();
        engine.sequence_export().unwrap()
    }

    fn request() -> VideoRequest {
        VideoRequest {
            width: 1280,
            height: 720,
            fps: 30,
            start_ms: 0,
            end_ms: None,
            photo: false,
            pixel_size: 1.0,
            ffmpeg: false,
        }
    }

    #[test]
    fn exports_report_progress_by_stage() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path());
        let path = dir.path().join("Song.mp4");
        let mut reports = Vec::new();
        let options = options_for(&request()).unwrap();
        let summary =
            run_video_export(&job, &options, &path, &AtomicU64::new(0), 0, |p| reports.push(p)).unwrap();
        assert_eq!((summary.frames, summary.height), (30, 720));
        assert!(path.is_file());
        let first = &reports[0];
        assert_eq!(first.stage, Stage::Rendering);
        assert_eq!(first.label, "Rendering frames");
        assert_eq!(first.path, path.display().to_string());
        let last = reports.last().unwrap();
        assert_eq!((last.stage, last.fraction), (Stage::Writing, Some(1.0)));
        assert!(reports.len() < 40, "throttled: {}", reports.len());
        let json = serde_json::to_value(first).unwrap();
        assert_eq!(json["stage"], "rendering");
        assert!(json.get("fraction").is_some());
    }

    #[test]
    fn a_cancel_stops_the_export_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path());
        let path = dir.path().join("Song.mp4");
        let cancels = AtomicU64::new(0);
        let options = options_for(&request()).unwrap();
        let err = run_video_export(&job, &options, &path, &cancels, 0, |p| {
            if p.fraction.unwrap_or(0.0) > 0.0 {
                cancels.fetch_add(1, Ordering::AcqRel);
            }
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "The export was cancelled.");
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter(|e| e.as_ref().unwrap().path().is_file())
                .count(),
            0
        );
    }

    #[test]
    fn only_offered_sizes_and_rates() {
        assert!(options_for(&request()).is_ok());
        let hd = VideoRequest {
            width: 1920,
            height: 1080,
            fps: 60,
            ..request()
        };
        assert_eq!(options_for(&hd).unwrap().fps, 60);
        for bad in [
            VideoRequest {
                width: 4000,
                ..request()
            },
            VideoRequest { fps: 24, ..request() },
            VideoRequest {
                pixel_size: f32::NAN,
                ..request()
            },
            VideoRequest {
                pixel_size: 9.0,
                ..request()
            },
        ] {
            assert!(options_for(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn requests_come_camel_cased() {
        let request: VideoRequest = serde_json::from_value(serde_json::json!({
            "width": 1920, "height": 1080, "fps": 30, "startMs": 1500, "endMs": null,
            "photo": true, "pixelSize": 1.5, "ffmpeg": false
        }))
        .unwrap();
        assert_eq!(
            (request.start_ms, request.end_ms, request.pixel_size),
            (1500, None, 1.5)
        );
    }
}
