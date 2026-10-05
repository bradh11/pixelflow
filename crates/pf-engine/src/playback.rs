//! Playing a rendered sequence (`.fseq`) to the show's controllers, with a preview of the props.
//!
//! Each controller that knows where its data sits in a sequence ([`SequenceChannels`]) receives
//! that block of every frame unchanged, the way FPP sends it. The preview maps the same channels
//! back through the show's wiring onto the props.

use crate::error::EngineError;
use crate::output::{ControllerStatus, controller_status};
use pf_fseq::Sequence;
use pf_mapping::ChannelMap;
use pf_model::Show;
use pf_output::{OutputHandle, OutputSettings, PassthroughRoute, Transport, wire_order};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Highest output send rate, in packets per controller per second.
const MAX_SEND_RATE: u32 = 120;

/// How often the player checks for pause, seek, and stop while waiting.
const POLL: Duration = Duration::from_millis(10);

/// Playback state, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    /// `playing`, `paused`, or `ended`.
    pub state: &'static str,
    pub path: PathBuf,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub frame_ms: u32,
    /// Controllers receiving the sequence, with their send health.
    pub controllers: Vec<ControllerStatus>,
    /// Plain-language notes, such as controllers that were left out and why.
    pub notes: Vec<String>,
    /// Why playback stopped by itself (a damaged file, for example).
    pub error: Option<String>,
}

/// What the player thread and the engine share.
#[derive(Debug, Default)]
struct Control {
    paused: bool,
    seek_to: Option<u32>,
    frame: u32,
    ended: bool,
    error: Option<String>,
}

fn lock(control: &Mutex<Control>) -> std::sync::MutexGuard<'_, Control> {
    control.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Works out which block of the sequence each controller receives.
fn routes(show: &Show, channels: usize) -> (Vec<PassthroughRoute>, Vec<String>) {
    let mut routes = Vec::new();
    let mut unknown = Vec::new();
    let mut notes = Vec::new();
    for controller in &show.controllers {
        let Some(range) = controller
            .sequence_channels
            .filter(|r| r.start >= 1 && r.count >= 1)
        else {
            unknown.push(controller.name.clone());
            continue;
        };
        let start = range.start as usize - 1;
        if start >= channels {
            notes.push(format!(
                "{} starts at channel {}, past the end of this sequence ({channels} channels), so it gets nothing.",
                controller.name, range.start
            ));
            continue;
        }
        let count = (range.count as usize).min(channels - start);
        if count < range.count as usize {
            notes.push(format!(
                "{} expects {} channels, but this sequence only has {count} for it.",
                controller.name, range.count
            ));
        }
        routes.push(PassthroughRoute {
            id: controller.id,
            name: controller.name.clone(),
            address: controller.address.clone(),
            protocol: controller.protocol,
            start,
            count,
        });
    }
    if !unknown.is_empty() && !routes.is_empty() {
        notes.push(format!(
            "Not playing to {} because PixelFlow doesn't know which sequence channels are theirs.",
            unknown.join(", ")
        ));
    }
    (routes, notes)
}

/// Copies the sequence channels of every controller with known sequence channels onto its props
/// (`preview` is a show frame: prop order, RGB/RGBW per pixel).
fn paint_preview(show: &Show, map: &ChannelMap, sequence_frame: &[u8], preview: &mut [u8]) {
    for (controller, output) in show.controllers.iter().zip(&map.controllers) {
        let Some(range) = controller.sequence_channels.filter(|r| r.start >= 1) else {
            continue;
        };
        let base = range.start as usize - 1;
        for span in &output.spans {
            let cpp = usize::from(span.channels_per_pixel);
            let order = wire_order(span.color_order);
            let pixels = span.pixels as usize;
            for k in 0..pixels {
                let wire = if span.reverse { pixels - 1 - k } else { k };
                let src = base + span.controller_channel + wire * cpp;
                let dst = span.frame_offset + k * cpp;
                let (Some(source), Some(target)) = (
                    sequence_frame.get(src..src + cpp),
                    preview.get_mut(dst..dst + cpp),
                ) else {
                    continue;
                };
                for (j, &value) in source.iter().enumerate() {
                    if let Some(channel) = target.get_mut(usize::from(order[j])) {
                        *channel = value;
                    }
                }
            }
        }
    }
}

/// A sequence playing: a player thread reading frames on time and the output thread sending them.
pub(crate) struct PlaybackSession {
    path: PathBuf,
    frames: u32,
    frame_ms: u32,
    notes: Vec<String>,
    control: Arc<Mutex<Control>>,
    stop: Arc<AtomicBool>,
    player: Option<JoinHandle<()>>,
    handle: Option<OutputHandle>,
    preview: Arc<Mutex<Vec<u8>>>,
    /// The current sequence frame, as sent.
    raw: Arc<Mutex<Vec<u8>>>,
}

impl PlaybackSession {
    pub fn start(
        show: &Show,
        map: &ChannelMap,
        path: &Path,
        position_ms: u64,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
    ) -> Result<Self, EngineError> {
        let mut sequence = Sequence::open(path).map_err(|e| EngineError::Playback(e.to_string()))?;
        let header = sequence.header().clone();
        let channels = header.channels as usize;
        let (routes, notes) = routes(show, channels);
        if routes.is_empty() {
            return Err(EngineError::Playback(
                "None of your controllers knows which sequence channels are theirs yet. Add them from \
                 your FPP's output list on the Devices screen."
                    .to_string(),
            ));
        }
        // The output thread keeps its own clock, so it sends at twice the sequence's rate: every
        // frame then goes out at least once (resending a frame is harmless).
        let frame_rate = u16::try_from((2000 / header.step_ms).clamp(1, MAX_SEND_RATE)).unwrap_or(1);
        let plan = pf_output::build_passthrough_plan(&routes, channels, frame_rate);
        let start_frame = u32::try_from(position_ms / u64::from(header.step_ms))
            .unwrap_or(u32::MAX)
            .min(header.frames.saturating_sub(1));

        let (mut writer, reader) = pf_frame::frame_buffers(channels);
        let mut preview_frame = vec![0u8; map.frame_len];
        // Publish the first frame before output starts so controllers never see a black frame first.
        if header.frames > 0 {
            sequence
                .read_frame(start_frame, writer.frame_mut())
                .map_err(|e| EngineError::Playback(e.to_string()))?;
            paint_preview(show, map, writer.frame_mut(), &mut preview_frame);
        }
        let raw = Arc::new(Mutex::new(writer.frame_mut().to_vec()));
        writer.publish();
        let preview = Arc::new(Mutex::new(preview_frame));
        let handle = pf_output::start_output(plan, settings, reader, transport);

        let control = Arc::new(Mutex::new(Control {
            frame: start_frame,
            ..Control::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let player = {
            let (control, stop, preview) = (Arc::clone(&control), Arc::clone(&stop), Arc::clone(&preview));
            let raw = Arc::clone(&raw);
            let control_for_reads = Arc::clone(&control);
            let (show, map) = (show.clone(), map.clone());
            let step = Duration::from_millis(u64::from(header.step_ms));
            let frames = header.frames;
            std::thread::Builder::new()
                .name("pixelflow-playback".into())
                .spawn(move || {
                    // Frame `base_frame` was due at `base_time`; later frames follow every `step`.
                    let (mut base_frame, mut base_time) = (start_frame, Instant::now());
                    let mut shown = Some(start_frame);
                    let mut show_frame = |frame: u32, writer: &mut pf_frame::FrameWriter| -> bool {
                        if let Err(error) = sequence.read_frame(frame, writer.frame_mut()) {
                            let mut c = lock(&control_for_reads);
                            c.error = Some(error.to_string());
                            c.ended = true;
                            return false;
                        }
                        paint_preview(
                            &show,
                            &map,
                            writer.frame_mut(),
                            &mut preview.lock().unwrap_or_else(PoisonError::into_inner),
                        );
                        raw.lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .copy_from_slice(writer.frame_mut());
                        writer.publish();
                        lock(&control_for_reads).frame = frame;
                        true
                    };
                    while !stop.load(Ordering::Relaxed) {
                        let (paused, seek, current) = {
                            let mut c = lock(&control);
                            (c.paused, c.seek_to.take(), c.frame)
                        };
                        if let Some(target) = seek {
                            base_frame = target.min(frames.saturating_sub(1));
                            shown = None;
                            lock(&control).ended = false;
                        }
                        if paused {
                            // Hold the current frame (or show where a seek landed), and resume from it.
                            let hold = if seek.is_some() { base_frame } else { current };
                            if shown != Some(hold) {
                                if !show_frame(hold, &mut writer) {
                                    return;
                                }
                                shown = Some(hold);
                            }
                            base_frame = hold;
                            base_time = Instant::now();
                            std::thread::sleep(POLL);
                            continue;
                        }
                        if seek.is_some() {
                            base_time = Instant::now();
                        }
                        let elapsed = base_time.elapsed().as_millis() / step.as_millis().max(1);
                        let due = u64::from(base_frame) + u64::try_from(elapsed).unwrap_or(u64::MAX);
                        if due >= u64::from(frames) {
                            if !lock(&control).ended {
                                writer.frame_mut().fill(0);
                                writer.publish();
                                preview.lock().unwrap_or_else(PoisonError::into_inner).fill(0);
                                raw.lock().unwrap_or_else(PoisonError::into_inner).fill(0);
                                lock(&control).ended = true;
                            }
                            std::thread::sleep(POLL);
                            continue;
                        }
                        let due = due as u32;
                        if shown != Some(due) {
                            if !show_frame(due, &mut writer) {
                                return;
                            }
                            shown = Some(due);
                        }
                        let next_due = base_time + step * (due - base_frame + 1);
                        std::thread::sleep(next_due.saturating_duration_since(Instant::now()).min(POLL));
                    }
                })
                .map_err(EngineError::Network)?
        };
        Ok(Self {
            path: path.to_path_buf(),
            frames: header.frames,
            frame_ms: header.step_ms,
            notes,
            control,
            stop,
            player: Some(player),
            handle: Some(handle),
            preview,
            raw,
        })
    }

    pub fn set_paused(&self, paused: bool) {
        lock(&self.control).paused = paused;
    }

    /// Jumps to `position_ms` (clamped to the sequence).
    pub fn seek(&self, position_ms: u64) {
        let frame = u32::try_from(position_ms / u64::from(self.frame_ms.max(1))).unwrap_or(u32::MAX);
        let frame = frame.min(self.frames.saturating_sub(1));
        let mut c = lock(&self.control);
        c.seek_to = Some(frame);
        c.frame = frame;
    }

    pub fn status(&self) -> PlaybackStatus {
        let c = lock(&self.control);
        let stats = self.handle.as_ref().map(OutputHandle::stats).unwrap_or_default();
        PlaybackStatus {
            state: if c.ended {
                "ended"
            } else if c.paused {
                "paused"
            } else {
                "playing"
            },
            path: self.path.clone(),
            position_ms: u64::from(c.frame) * u64::from(self.frame_ms),
            duration_ms: u64::from(self.frames) * u64::from(self.frame_ms),
            frame_ms: self.frame_ms,
            controllers: controller_status(&stats),
            notes: self.notes.clone(),
            error: c.error.clone(),
        }
    }

    /// The props as they look in the current frame (show frame: prop order, RGB/RGBW per pixel).
    pub fn preview(&self) -> Vec<u8> {
        self.preview
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The current sequence frame (every channel of the sequence, as sent to the controllers).
    pub fn sequence_frame(&self) -> Vec<u8> {
        self.raw.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Stops the player, then stops output (which blacks out the controllers).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(player) = self.player.take() {
            let _ = player.join();
        }
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
    }
}

impl Drop for PlaybackSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Protocol, SequenceChannels};

    fn controller(name: &str, channels: Option<(u32, u32)>) -> Controller {
        let mut c = Controller::new(name, "127.0.0.1", Protocol::Ddp);
        c.sequence_channels = channels.map(|(start, count)| SequenceChannels { start, count });
        c
    }

    #[test]
    fn routes_use_known_sequence_channels_and_explain_the_rest() {
        let mut show = Show::new("t");
        show.controllers = vec![
            controller("Falcon", Some((1, 6147))),
            controller("Porch", None),
            controller("Garage", Some((6100, 100))),
            controller("Far", Some((9000, 10))),
        ];
        let (routes, notes) = routes(&show, 6148);
        let blocks: Vec<_> = routes
            .iter()
            .map(|r| (r.name.as_str(), r.start, r.count))
            .collect();
        assert_eq!(blocks, vec![("Falcon", 0, 6147), ("Garage", 6099, 49)]);
        assert_eq!(
            notes,
            vec![
                "Garage expects 100 channels, but this sequence only has 49 for it.",
                "Far starts at channel 9000, past the end of this sequence (6148 channels), so it gets nothing.",
                "Not playing to Porch because PixelFlow doesn't know which sequence channels are theirs.",
            ]
        );
    }
}
