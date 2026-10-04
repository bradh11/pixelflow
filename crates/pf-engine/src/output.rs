//! Live test-pattern output driven by the engine.

use crate::error::EngineError;
use pf_mapping::ChannelMap;
use pf_model::{ControllerId, GroupId, PropId, Show};
use pf_output::{ControllerState, OutputHandle, OutputPlan, OutputSettings, OutputStats, Transport};
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
}

impl OutputStatus {
    pub(crate) fn stopped(generation: u64) -> Self {
        Self {
            running: false,
            generation,
            pattern: None,
            target: None,
            frames: 0,
            late_frames: 0,
            achieved_fps: 0.0,
            controllers: Vec::new(),
        }
    }
}

fn controller_status(stats: &OutputStats) -> Vec<ControllerStatus> {
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
    pub plan: OutputPlan,
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
        plan: OutputPlan,
        pattern_spec: PatternSpec,
        target: TargetSpec,
        transport: Box<dyn Transport>,
        generation: u64,
    ) -> Result<Self, EngineError> {
        let pattern = pattern_spec.to_pattern()?;
        let targets = resolve_target(show, map, &Target::from(&target));
        let (mut writer, reader) = pf_frame::frame_buffers(plan.frame_len);
        // Publish the first frame before output starts so controllers never see a black frame first.
        render(&pattern, 0.0, &targets, writer.frame_mut());
        let preview = Arc::new(Mutex::new(writer.frame_mut().to_vec()));
        writer.publish();
        let handle = pf_output::start_output(plan.clone(), OutputSettings::default(), reader, transport);

        let stop = Arc::new(AtomicBool::new(false));
        let period = Duration::from_secs_f64(1.0 / f64::from(plan.frame_rate.max(1)));
        let content = {
            let stop = Arc::clone(&stop);
            let preview = Arc::clone(&preview);
            let targets = targets.clone();
            std::thread::Builder::new()
                .name("pixelflow-content".into())
                .spawn(move || {
                    let started = Instant::now();
                    while !stop.load(Ordering::Relaxed) {
                        let frame = writer.frame_mut();
                        render(&pattern, started.elapsed().as_secs_f32(), &targets, frame);
                        preview
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .copy_from_slice(frame);
                        writer.publish();
                        std::thread::sleep(period);
                    }
                })
                .expect("spawn content thread")
        };
        Ok(Self {
            pattern: pattern_spec,
            target,
            plan,
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
}
