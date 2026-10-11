//! What a run measures, and how its verdict is reached.

use crate::alloc::{Counted, counted};
use crate::pipeline::{Setup, Wiring};
use crate::sequence::Variant;
use crate::sink::{Guarded, Listeners, Sent, SentNow, SinkKind};
use crate::stats::Spread;
use crate::system::{Used, threads, used};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// How the pipeline is driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// One frame after another on one thread, as fast as they go: what each stage costs.
    Asap,
    /// In real time on playback's two threads: late and dropped frames, and jitter.
    Paced,
    /// In real time in the engine itself, watched from outside: a check on the paced runs.
    Engine,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Asap => "asap",
            Mode::Paced => "paced",
            Mode::Engine => "engine",
        }
    }
}

/// How a run is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How many seconds of the sequence are measured.
    pub seconds: u32,
    /// How many seconds play first, reported apart and left out of everything else.
    pub warmup_seconds: u32,
    /// How often the output thread sends in a paced run (`None`: as live playback does).
    pub send_hz: Option<u16>,
}

/// How long each stage took, per frame (output stages: per pass), in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stages {
    pub render: Spread,
    /// Playback's copies of each frame for the app's preview.
    pub preview: Spread,
    /// Render and preview together: what the player thread does for one frame.
    pub player: Spread,
    pub map: Spread,
    pub packetize: Spread,
    pub send: Spread,
    /// Map, packetize, and send together: one pass of the output thread.
    pub output_pass: Spread,
    /// Everything done for one frame: render, preview, and one output pass.
    pub frame: Spread,
}

/// What became of the sequence's frames in the measured part of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frames {
    /// Frames that were due.
    pub due: u64,
    /// Frames that reached the wire (or the null sink).
    pub sent: u64,
    /// Frames that never did: skipped because the one before was still being drawn, or
    /// replaced before the output thread got to them.
    pub dropped: u64,
    /// Frames drawn and handed to the output thread after the next frame was due (flat out:
    /// frames that took longer than the frame period).
    pub late: u64,
}

impl Frames {
    /// Frames neither dropped nor late, in percent of the frames due.
    pub fn on_time_percent(&self) -> f64 {
        let on_time = self.due.saturating_sub(self.dropped + self.late);
        100.0 * on_time as f64 / self.due.max(1) as f64
    }
}

/// How well a real-time run kept time, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pacing {
    /// From a frame being due to the player starting to draw it.
    pub start_delay: Spread,
    /// From a frame being due to it being drawn and handed to the output thread.
    pub shown: Spread,
    /// From a frame being due to its first send finishing.
    pub latency: Spread,
    /// Between one frame's first send finishing and the next one's.
    pub interval: Spread,
    /// How far those intervals stray from their mean (standard deviation).
    pub jitter: f64,
    /// Passes the output thread made each second, and how many started over a period late.
    pub passes_per_second: f64,
    pub late_passes: u64,
}

/// What the process used in the measured part of a run.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    pub wall_seconds: f64,
    pub cpu_seconds: Option<f64>,
    /// CPU time over wall time: 1.0 is one core kept busy.
    pub cpu_cores: Option<f64>,
    pub user_seconds: Option<f64>,
    pub system_seconds: Option<f64>,
    /// The most memory the process held, setup and warm-up included.
    pub peak_rss_mb: Option<f64>,
    /// Threads the benchmark runs the pipeline on (the listeners' thread included), and every
    /// thread of the process as the system counts them.
    pub pipeline_threads: usize,
    pub process_threads: Option<usize>,
    pub allocations_per_frame: Option<f64>,
    pub allocated_kb_per_frame: Option<f64>,
    /// Flat out only: the allocations made drawing a frame, and those made sending it.
    pub render_allocations_per_frame: Option<f64>,
    pub output_allocations_per_pass: Option<f64>,
    /// The most the heap held at once, setup and warm-up included.
    pub peak_heap_mb: Option<f64>,
}

/// What was sent in the measured part of a run.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub packets: u64,
    pub bytes: u64,
    pub packets_per_second: f64,
    pub megabits_per_second: f64,
    /// Packets dropped because the send buffer was full.
    pub send_buffer_full: u64,
    /// Sends that failed for any other reason.
    pub send_failures: u64,
    /// Sends refused because they weren't to this machine.
    pub refused: u64,
    /// What the loopback listeners counted (none for the null sink).
    pub received_packets: Option<u64>,
    pub lost_percent: Option<f64>,
}

impl Network {
    /// Every packet went out.
    pub fn clean(&self) -> bool {
        self.send_buffer_full == 0 && self.send_failures == 0 && self.refused == 0
    }
}

/// The first frames of a run, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Warmup {
    pub frames: u64,
    /// The first frame of all: every target's pixel buffer is built as it is first drawn.
    pub first_frame: f64,
    pub mean: f64,
    pub max: f64,
}

impl Warmup {
    pub(crate) fn of(frames: &[f64]) -> Self {
        Self {
            frames: frames.len() as u64,
            first_frame: frames.first().copied().unwrap_or(0.0),
            mean: frames.iter().sum::<f64>() / frames.len().max(1) as f64,
            max: frames.iter().copied().fold(0.0, f64::max),
        }
    }
}

/// The share of a run's frames that must be on time for it to have held its frame rate, in
/// percent: the 99th percentile, as for every other budget.
pub const ON_TIME_PERCENT: f64 = 99.0;

/// Whether a run held the sequence's frame rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    /// Sustained, within budget, and clean.
    pub pass: bool,
    /// Frames neither dropped nor late, in percent of the frames due.
    pub on_time_percent: f64,
    /// At least [`ON_TIME_PERCENT`] of the frames were on time.
    pub sustained: bool,
    /// At the 99th percentile the work fits the frame period: the whole frame flat out; in real
    /// time, the player's work for a frame and the output thread's pass, each on its own thread.
    pub within_budget: bool,
    /// No packet was dropped or refused on the way out.
    pub clean: bool,
    pub summary: String,
}

impl Verdict {
    /// The verdict on a run with these `frames`, saying `summary`.
    pub(crate) fn of(frames: &Frames, within_budget: bool, clean: bool, summary: String) -> Self {
        let on_time_percent = frames.on_time_percent();
        let sustained = on_time_percent >= ON_TIME_PERCENT;
        Self {
            pass: sustained && within_budget && clean,
            on_time_percent,
            sustained,
            within_budget,
            clean,
            summary,
        }
    }
}

/// Everything measured in one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub tier: String,
    pub pixels: u32,
    pub variant: Variant,
    pub mode: Mode,
    pub sink: SinkKind,
    pub frame_ms: u32,
    /// Passes a second the output thread makes in a real-time run.
    pub send_hz: Option<u16>,
    pub wiring: Wiring,
    pub frames: Frames,
    /// Frames a second that reached the wire.
    pub fps: f64,
    pub stages: Stages,
    pub pacing: Option<Pacing>,
    /// The app reading the preview frame: a lock and a copy of the whole frame.
    pub preview_poll: Spread,
    /// How much of the frame was lit, on average: proof the pipeline had something to draw.
    pub lit_percent: f64,
    pub warmup: Warmup,
    pub resources: Resources,
    pub network: Network,
    pub verdict: Verdict,
    /// The machine's one-minute load average when the run began: what else it was doing.
    pub load_average: Option<f64>,
    pub setup: Setup,
    /// Anything about the run a reader should know.
    pub notes: Vec<String>,
}

/// How much of a frame is lit, in percent of its channels.
pub(crate) fn lit_percent(frame: &[u8]) -> f64 {
    100.0 * frame.iter().filter(|&&b| b > 0).count() as f64 / frame.len().max(1) as f64
}

/// Where a run's packets go: the transport's counters and, for loopback, the listeners.
pub(crate) struct Sinks {
    pub kind: SinkKind,
    pub sent: Arc<Sent>,
    pub listeners: Option<Listeners>,
}

impl Sinks {
    /// Opens the sink for a show with `controllers` controllers.
    pub fn open(kind: SinkKind, controllers: usize) -> Result<Self> {
        let listeners = match kind {
            SinkKind::Null => None,
            SinkKind::Loopback => {
                Some(Listeners::start(controllers).context("couldn't listen on the loopback interface")?)
            }
        };
        Ok(Self {
            kind,
            sent: Arc::new(Sent::default()),
            listeners,
        })
    }

    /// Each controller's listener, in show order (none for the null sink).
    pub fn addresses(&self) -> Option<&[SocketAddr]> {
        self.listeners.as_ref().map(Listeners::addresses)
    }

    /// A transport into the sink.
    pub fn transport(&self) -> Result<Guarded> {
        Guarded::open(self.kind, Arc::clone(&self.sent))
            .context("couldn't open a socket on the loopback interface")
    }
}

/// What the process had used and sent when the measured part of a run began.
pub(crate) struct Window {
    opened: Instant,
    used: Option<Used>,
    counted: Option<Counted>,
    sent: SentNow,
    received: Option<(u64, u64)>,
}

impl Window {
    pub fn open(sinks: &Sinks) -> Self {
        Self {
            used: used(),
            counted: counted(),
            sent: sinks.sent.now(),
            received: sinks.listeners.as_ref().map(Listeners::received),
            opened: Instant::now(),
        }
    }

    /// Closes the window `frames` frames later.
    pub fn close(self, frames: u64, pipeline_threads: usize, sinks: &Sinks) -> (Resources, Network) {
        let wall = self.opened.elapsed().as_secs_f64().max(f64::EPSILON);
        let (now, counted_now, sent_now) = (used(), counted(), sinks.sent.now());
        let received_now = sinks.listeners.as_ref().map(Listeners::received);
        let cpu = self.used.zip(now).map(|(a, b)| {
            (
                b.user.saturating_sub(a.user).as_secs_f64(),
                b.system.saturating_sub(a.system).as_secs_f64(),
            )
        });
        let per_frame = |count: u64| count as f64 / frames.max(1) as f64;
        let allocated = self.counted.zip(counted_now);
        let resources = Resources {
            wall_seconds: wall,
            cpu_seconds: cpu.map(|(user, system)| user + system),
            cpu_cores: cpu.map(|(user, system)| (user + system) / wall),
            user_seconds: cpu.map(|(user, _)| user),
            system_seconds: cpu.map(|(_, system)| system),
            peak_rss_mb: now.map(|u| u.peak_rss as f64 / f64::from(1 << 20)),
            pipeline_threads: pipeline_threads + usize::from(sinks.listeners.is_some()),
            process_threads: threads(),
            allocations_per_frame: allocated.map(|(a, b)| per_frame(b.allocations - a.allocations)),
            allocated_kb_per_frame: allocated.map(|(a, b)| per_frame(b.bytes - a.bytes) / 1024.0),
            render_allocations_per_frame: None,
            output_allocations_per_pass: None,
            peak_heap_mb: counted_now.map(|c| c.peak_live as f64 / f64::from(1 << 20)),
        };
        let packets = sent_now.packets - self.sent.packets;
        let bytes = sent_now.bytes - self.sent.bytes;
        let received_packets = self.received.zip(received_now).map(|(a, b)| b.0 - a.0);
        let network = Network {
            packets,
            bytes,
            packets_per_second: packets as f64 / wall,
            megabits_per_second: bytes as f64 * 8.0 / 1e6 / wall,
            send_buffer_full: sent_now.dropped - self.sent.dropped,
            send_failures: sent_now.failed - self.sent.failed,
            refused: sent_now.refused - self.sent.refused,
            received_packets,
            // A packet in flight when the window closes can make this a hair off either way.
            lost_percent: received_packets
                .filter(|_| packets > 0)
                .map(|got| (100.0 * (1.0 - got as f64 / packets as f64)).max(0.0)),
        };
        (resources, network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_passes_with_99_frames_in_100_on_time_within_budget_and_nothing_lost() {
        let frames = |dropped, late| Frames {
            due: 2400,
            sent: 2400 - dropped,
            dropped,
            late,
        };
        let verdict = |frames: Frames, within_budget, clean| {
            let v = Verdict::of(&frames, within_budget, clean, String::new());
            (v.pass, v.sustained)
        };
        assert_eq!(frames(3, 3).on_time_percent(), 99.75);
        assert_eq!(verdict(frames(0, 0), true, true), (true, true));
        // One frame in a hundred may be dropped or late, and no more.
        assert_eq!(verdict(frames(4, 20), true, true), (true, true));
        assert_eq!(verdict(frames(5, 20), true, true), (false, false));
        // On time, but a thread over its budget or packets lost on the way out.
        assert_eq!(verdict(frames(0, 0), false, true), (false, true));
        assert_eq!(verdict(frames(0, 0), true, false), (false, true));
    }

    #[test]
    fn a_frame_is_as_lit_as_its_channels_that_arent_zero() {
        assert_eq!(lit_percent(&[0, 0, 0, 0]), 0.0);
        assert_eq!(lit_percent(&[0, 9, 0, 255]), 50.0);
        assert_eq!(lit_percent(&[]), 0.0);
    }

    #[test]
    fn a_window_counts_only_what_happened_inside_it() {
        let sinks = Sinks::open(SinkKind::Null, 3).unwrap();
        let mut transport = sinks.transport().unwrap();
        let to: SocketAddr = "127.0.0.1:4048".parse().unwrap();
        pf_output::Transport::send_to(&mut transport, &[0; 10], to).unwrap();
        let window = Window::open(&sinks);
        for _ in 0..4 {
            pf_output::Transport::send_to(&mut transport, &[0; 25], to).unwrap();
        }
        let (resources, network) = window.close(2, 1, &sinks);
        assert_eq!((network.packets, network.bytes), (4, 100));
        assert!(network.clean() && network.received_packets.is_none());
        assert_eq!(resources.pipeline_threads, 1);
        assert!(resources.wall_seconds > 0.0);
    }
}
