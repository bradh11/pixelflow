//! Live test-pattern output driven by the engine.

use crate::error::EngineError;
use pf_mapping::{ChannelMap, ControllerOutput};
use pf_model::{ControllerId, GroupId, PropId, Protocol, Show};
use pf_output::{ControllerState, OutputHandle, OutputSettings, OutputStats, Transport};
use pf_patterns::{Pattern, Preset, Rgbw, Target, TargetRange, render, resolve_target};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Which built-in pattern to run, and its color as hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternSpec {
    pub kind: PatternKind,
    /// `rrggbb` or `rrggbbww`; ignored by `cycle` and `identify`.
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "ffffff".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatternKind {
    Solid,
    Cycle,
    Chase,
    Ramp,
    Alternate,
    Identify,
    Walk,
    /// The camera-mapping sequence, colour coded (see `pf_camera_map`).
    CameraMap,
    /// The camera-mapping sequence in white only (single-colour pixels, colour-blind cameras).
    CameraMapBinary,
}

impl PatternSpec {
    pub(crate) fn to_pattern(&self) -> Result<Pattern, EngineError> {
        let color = Rgbw::from_hex(&self.color).ok_or_else(|| EngineError::BadColor(self.color.clone()))?;
        let preset = match self.kind {
            PatternKind::Solid => Preset::Solid,
            PatternKind::Cycle => Preset::Cycle,
            PatternKind::Chase => Preset::Chase,
            PatternKind::Ramp => Preset::Ramp,
            PatternKind::Alternate => Preset::Alternate,
            PatternKind::Identify => Preset::Identify,
            PatternKind::Walk => Preset::Walk,
            PatternKind::CameraMap | PatternKind::CameraMapBinary => {
                return Ok(Pattern::CameraMap {
                    base: if self.kind == PatternKind::CameraMap {
                        pf_camera_map::Base::Four
                    } else {
                        pf_camera_map::Base::Two
                    },
                    slot_seconds: pf_camera_map::DEFAULT_SLOT_SECONDS,
                });
            }
        };
        Ok(Pattern::preset(preset, color))
    }
}

/// What the pattern lights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TargetSpec {
    Show,
    Prop { id: PropId },
    Group { id: GroupId },
    Controller { id: ControllerId },
    Port { controller: ControllerId, port: u16 },
}

impl From<&TargetSpec> for Target {
    fn from(spec: &TargetSpec) -> Target {
        match spec {
            TargetSpec::Show => Target::Show,
            TargetSpec::Prop { id } => Target::Prop(*id),
            TargetSpec::Group { id } => Target::Group(*id),
            TargetSpec::Controller { id } => Target::Controller(*id),
            TargetSpec::Port { controller, port } => Target::Port {
                controller: *controller,
                port: *port,
            },
        }
    }
}

/// One controller's live output health, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerStatus {
    pub id: ControllerId,
    pub name: String,
    /// `ok`, `degraded`, or `unresolved`.
    pub state: &'static str,
    pub packets_sent: u64,
    pub send_errors: u64,
    pub last_error: Option<String>,
}

/// Live output state, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputStatus {
    pub running: bool,
    /// Increases every time output (re)starts.
    pub generation: u64,
    pub pattern: Option<PatternSpec>,
    pub target: Option<TargetSpec>,
    pub frames: u64,
    pub late_frames: u64,
    pub achieved_fps: f32,
    pub controllers: Vec<ControllerStatus>,
    /// Why output stopped by itself, when it did (cleared by a deliberate stop or a new show).
    pub stop_reason: Option<String>,
}

impl OutputStatus {
    pub(crate) fn stopped(generation: u64, stop_reason: Option<String>) -> Self {
        Self {
            running: false,
            generation,
            pattern: None,
            target: None,
            frames: 0,
            late_frames: 0,
            achieved_fps: 0.0,
            controllers: Vec::new(),
            stop_reason,
        }
    }
}

/// What a running session depends on besides the target pixels. Comparing keys is cheap and
/// needs no DNS, unlike building a full output plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutputKey {
    controllers: Vec<ControllerOutput>,
    addresses: Vec<(String, Protocol)>,
    frame_rate: u16,
}

pub(crate) fn output_key(show: &Show, map: &ChannelMap) -> OutputKey {
    OutputKey {
        controllers: map.controllers.clone(),
        addresses: show
            .controllers
            .iter()
            .map(|c| (c.address.clone(), c.protocol))
            .collect(),
        frame_rate: show.settings.frame_rate,
    }
}

pub(crate) fn controller_status(stats: &OutputStats) -> Vec<ControllerStatus> {
    stats
        .controllers
        .iter()
        .map(|c| ControllerStatus {
            id: c.controller,
            name: c.name.clone(),
            state: match c.state {
                ControllerState::Ok => "ok",
                ControllerState::Degraded => "degraded",
                ControllerState::Unresolved => "unresolved",
            },
            packets_sent: c.packets_sent,
            send_errors: c.send_errors,
            last_error: c.last_error.clone(),
        })
        .collect()
}

/// A running pattern: a content thread painting frames and the output thread sending them.
pub(crate) struct OutputSession {
    pub pattern: PatternSpec,
    pub target: TargetSpec,
    pub key: OutputKey,
    pub targets: Vec<TargetRange>,
    pub generation: u64,
    handle: Option<OutputHandle>,
    stop: Arc<AtomicBool>,
    content: Option<JoinHandle<()>>,
    preview: Arc<Mutex<Vec<u8>>>,
}

impl OutputSession {
    pub fn start(
        show: &Show,
        map: &ChannelMap,
        pattern_spec: PatternSpec,
        target: TargetSpec,
        transport: Box<dyn Transport>,
        generation: u64,
        settings: OutputSettings,
    ) -> Result<Self, EngineError> {
        let pattern = pattern_spec.to_pattern()?;
        let targets = resolve_target(show, map, &Target::from(&target));
        let plan = pf_output::build_plan(show, map);
        let (mut writer, reader) = pf_frame::frame_buffers(plan.frame_len);
        // Publish the first frame before output starts so controllers never see a black frame first.
        render(&pattern, 0.0, &targets, writer.frame_mut());
        let preview = Arc::new(Mutex::new(writer.frame_mut().to_vec()));
        writer.publish();
        let handle = pf_output::start_output(plan.clone(), settings, reader, transport);

        let stop = Arc::new(AtomicBool::new(false));
        let period = Duration::from_secs_f64(1.0 / f64::from(plan.frame_rate.max(1)));
        let content = {
            let stop = Arc::clone(&stop);
            let preview = Arc::clone(&preview);
            let targets = targets.clone();
            let spawned = std::thread::Builder::new()
                .name("pixelflow-content".into())
                .spawn(move || {
                    let started = Instant::now();
                    let mut next = started;
                    while !stop.load(Ordering::Relaxed) {
                        let frame = writer.frame_mut();
                        render(&pattern, started.elapsed().as_secs_f32(), &targets, frame);
                        preview
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .copy_from_slice(frame);
                        writer.publish();
                        // Pace by deadline so render time doesn't stretch the frame period.
                        next += period;
                        let now = Instant::now();
                        if now.saturating_duration_since(next) > period {
                            next = now + period;
                        }
                        std::thread::sleep(next.saturating_duration_since(now));
                    }
                });
            // On failure the output handle is dropped, which blacks out the controllers.
            spawned.map_err(EngineError::Network)?
        };
        Ok(Self {
            pattern: pattern_spec,
            target,
            key: output_key(show, map),
            targets,
            generation,
            handle: Some(handle),
            stop,
            content: Some(content),
            preview,
        })
    }

    pub fn status(&self) -> OutputStatus {
        let stats = self.handle.as_ref().map(OutputHandle::stats).unwrap_or_default();
        OutputStatus {
            running: true,
            generation: self.generation,
            pattern: Some(self.pattern.clone()),
            target: Some(self.target.clone()),
            frames: stats.frames,
            late_frames: stats.late_frames,
            achieved_fps: stats.achieved_fps,
            controllers: controller_status(&stats),
            stop_reason: None,
        }
    }

    /// The most recent frame painted (prop order, canonical RGB/RGBW).
    pub fn preview(&self) -> Vec<u8> {
        self.preview
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Stops painting, then stops output (which blacks out the controllers).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(content) = self.content.take() {
            let _ = content.join();
        }
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
    }
}

impl Drop for OutputSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_serialize_for_the_ui() {
        let pattern: PatternSpec = serde_json::from_str(r#"{ "kind": "chase" }"#).unwrap();
        assert_eq!(pattern.color, "ffffff");
        let target = TargetSpec::Port {
            controller: ControllerId(Default::default()),
            port: 2,
        };
        let json = serde_json::to_value(&target).unwrap();
        assert_eq!(json["type"], "port");
        assert_eq!(json["port"], 2);
        assert_eq!(serde_json::from_value::<TargetSpec>(json).unwrap(), target);
    }

    #[test]
    fn camera_map_kinds_play_the_coded_sequence() {
        let spec: PatternSpec = serde_json::from_str(r#"{ "kind": "cameraMap" }"#).unwrap();
        assert!(matches!(
            spec.to_pattern().unwrap(),
            Pattern::CameraMap {
                base: pf_camera_map::Base::Four,
                ..
            }
        ));
        let spec: PatternSpec = serde_json::from_str(r#"{ "kind": "cameraMapBinary" }"#).unwrap();
        assert!(matches!(
            spec.to_pattern().unwrap(),
            Pattern::CameraMap {
                base: pf_camera_map::Base::Two,
                ..
            }
        ));
    }

    #[test]
    fn bad_colors_are_reported_plainly() {
        let spec = PatternSpec {
            kind: PatternKind::Solid,
            color: "red".into(),
        };
        assert_eq!(
            spec.to_pattern().unwrap_err().to_string(),
            "'red' is not a color. Use six or eight hex digits, like ff8000."
        );
    }

    fn two_controller_show() -> Show {
        let mut show = Show::new("k");
        show.props.push(pf_model::Prop::new(
            "A",
            pf_model::ShapeSource::Generator(pf_model::Generator::Line {
                nodes: 4,
                length: 1.0,
            }),
        ));
        let mut c = pf_model::Controller::new("C", "bad host:x", Protocol::Ddp);
        let mut port = pf_model::Port::new(1);
        port.slots.push(pf_model::PortSlot::new(show.props[0].id));
        c.ports.push(port);
        show.controllers.push(c);
        show
    }

    fn key_of(show: &Show) -> OutputKey {
        output_key(show, &pf_mapping::map_show(show).0)
    }

    #[test]
    fn the_output_key_ignores_layout_but_sees_wiring_and_addresses() {
        let show = two_controller_show();
        let base = key_of(&show);

        let mut moved = show.clone();
        moved.props[0].transform.position = pf_model::Vec3::new(9.0, 1.0, 0.0);
        assert_eq!(key_of(&moved), base, "moving a prop does not change the key");

        let mut renamed_address = show.clone();
        renamed_address.controllers[0].address = "other".into();
        assert_ne!(key_of(&renamed_address), base);

        let mut protocol = show.clone();
        protocol.controllers[0].protocol = Protocol::Sacn(Default::default());
        assert_ne!(key_of(&protocol), base);

        let mut wired = show.clone();
        wired.controllers[0].ports[0]
            .slots
            .push(pf_model::PortSlot::new(wired.props[0].id));
        assert_ne!(key_of(&wired), base, "adding a slot changes the key");

        let mut faster = show.clone();
        faster.settings.frame_rate += 1;
        assert_ne!(key_of(&faster), base);
    }
}
