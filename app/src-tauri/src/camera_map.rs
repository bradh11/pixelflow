//! Camera mapping: the window decodes the video (with the platform's own decoder) and sends
//! frame brightness and averaged slot frames here; `pf-camera-map` does the rest.

use crate::{AppState, Reply};
use pf_camera_map::{Base, CodeSpec, Decoded, Image, Plan, Sample};
use pf_engine::{CameraMapProp, TargetSpec};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::{InvokeBody, Request};

/// The header that describes the slot frames sent to `camera_map_decode`.
pub(crate) const FRAMES_HEADER: &str = "x-camera-map";

/// What a capture of a target covers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CameraMapTargetInfo {
    pixels: u32,
    /// How long one pass of the sequence takes, in seconds.
    seconds: f32,
    props: Vec<CameraMapProp>,
}

/// Where the sequence starts in the video, and the time span to average for each slot.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CameraMapSync {
    start: f64,
    score: f32,
    windows: Vec<(f64, f64)>,
}

/// The slot frames' size and the sequence they're from.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FramesInfo {
    width: usize,
    height: usize,
    pixels: u32,
    base: Base,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CameraMapPlan {
    props: Vec<CameraMapProp>,
    plan: Plan,
}

const NOTHING: &str =
    "Nothing on this target is set up to light. Pick a prop, port, or controller with pixels.";

/// The pixels a capture of `target` covers and how long the sequence runs.
#[tauri::command]
pub(crate) async fn camera_map_target(
    state: State<'_, AppState>,
    target: TargetSpec,
    base: Base,
) -> Reply<CameraMapTargetInfo> {
    let found = state.engine().camera_map_target(&target);
    let pixels = found.pixels();
    if pixels == 0 {
        return Err(NOTHING.into());
    }
    Ok(CameraMapTargetInfo {
        pixels,
        seconds: CodeSpec::new(pixels, base).duration(),
        props: found.props,
    })
}

/// Finds where the sequence starts from each frame's brightness.
#[tauri::command]
pub(crate) async fn camera_map_sync(samples: Vec<Sample>, pixels: u32, base: Base) -> Reply<CameraMapSync> {
    let spec = CodeSpec::new(pixels, base);
    let found = pf_camera_map::find_sync(&samples, &spec).ok_or_else(|| {
        format!(
            "Couldn't find the flashing sequence in this video. Record from before pressing Start until at least {:.0} seconds after, with the lights in view.",
            spec.duration().ceil()
        )
    })?;
    Ok(CameraMapSync {
        start: found.start,
        score: found.score,
        windows: pf_camera_map::slot_windows(found.start, &spec),
    })
}

/// Decodes the slot frames (raw RGB, one per slot, described by the [`FRAMES_HEADER`] header).
#[tauri::command]
pub(crate) async fn camera_map_decode(request: Request<'_>) -> Reply<Decoded> {
    let info: FramesInfo = request
        .headers()
        .get(FRAMES_HEADER)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| serde_json::from_str(h).ok())
        .ok_or("The video frames came without their size.")?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("The video frames didn't arrive as bytes.".into());
    };
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || decode_frames(&info, &bytes))
        .await
        .map_err(|_| "Something went wrong decoding the video.".to_string())?
}

pub(crate) fn decode_frames(info: &FramesInfo, bytes: &[u8]) -> Reply<Decoded> {
    let spec = CodeSpec::new(info.pixels, info.base);
    let images =
        Image::split_rgb8(bytes, info.width, info.height, spec.slot_count()).map_err(|e| e.to_string())?;
    pf_camera_map::decode(&images, &spec).map_err(|e| e.to_string())
}

/// Lines the decoded pixels up with the layout (by `anchors`, sequence indexes, or every pixel)
/// and works out each prop's measured points. Changes nothing.
#[tauri::command]
pub(crate) async fn camera_map_plan(
    state: State<'_, AppState>,
    target: TargetSpec,
    pixels: u32,
    decoded: Decoded,
    anchors: Vec<u32>,
) -> Reply<CameraMapPlan> {
    let found = state.engine().camera_map_target(&target);
    if found.pixels() != pixels {
        return Err(
            "The show changed since this video was recorded: this target now has a different number of pixels. Record it again."
                .into(),
        );
    }
    let plan = pf_camera_map::plan(&decoded, &found.owners, &found.inputs, &anchors);
    Ok(CameraMapPlan {
        props: found.props,
        plan,
    })
}
