//! How closely the lights keep to the music, measured in real time in the engine itself.
//!
//! The engine plays the synthetic sequence on a loop, following a music clock the benchmark
//! made (a silent stopwatch it can read from any thread), while the benchmark jumps about in it
//! now and then and, if asked, keeps every core busy with something else. Two things are noted
//! for every frame, both as the music's position at that moment:
//!
//! - **began**: when the player first read a time in the frame, which is when it begins to draw
//!   it. The clock itself notes this, so nothing is added by watching;
//! - **sent**: when the frame first went out to a controller. Two small marker props, on a
//!   controller of their own that is sent last, show the frame's number as colors; the
//!   transport reads it back out of that controller's packet.
//!
//! Against the frame's own time (its number times the frame time) those say how far behind the
//! music the lights were: at the 50th, 95th and 99th percentile and at worst, whether that
//! grew over the run (a straight line through it, in ms a minute), which frames never went out
//! or went out again, and how long a jump and a new round of the loop took to reach the wire.
//!
//! The playback simulations in `pf-engine` say what the player does with the time it is given,
//! exactly and every time. This says what a real machine gives it.

use crate::measure::{ON_TIME_PERCENT, Sinks};
use crate::pipeline::{Workload, engine_send_rate};
use crate::sequence::Variant;
use crate::show::tier_label;
use crate::sink::{Guarded, SinkKind};
use crate::stats::Spread;
use crate::system::load_average;
use anyhow::{Context, Result, anyhow, bail};
use pf_audio::{AudioClock, SilentClock};
use pf_engine::{CheckedShow, ClockFactory, Engine};
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, Rgb, ShapeSource, Show};
use pf_output::{Transport, ddp};
use pf_sequence::{Effect, EffectKind, Row, Sequence, Target};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Where the marker controller is when nothing is really sent: a loopback address of its own,
/// so its packets can be told from the others'.
const MARKER_ADDRESS: &str = "127.0.0.2";
/// Pixels in each marker prop.
const MARKER_NODES: u32 = 2;
/// A marker channel carries one hexadecimal digit, as one of sixteen levels this far apart.
const LEVEL: u8 = 17;
/// Frames either side of a jump's target that count as the new place.
const NEAR: u32 = 40;
/// Drift is judged by how late frames typically began in each stretch of this long, in ms.
const STRETCH: f64 = 5_000.0;

/// The line through how late frames typically began may rise or fall this much a minute, beyond
/// what its own uncertainty allows (three standard errors), before a run is said to drift. Real
/// drift is a steady gain: a millisecond a minute is a frame over a song.
pub const DRIFT_LIMIT: f64 = 1.0;
/// How long a jump, or a new round of the loop, may take to reach the wire, in milliseconds:
/// the player's look at what was asked (10 ms), a frame drawn, a send, and room to spare.
pub const RECOVERY_LIMIT: f64 = 100.0;

/// How a sync run is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOptions {
    /// How many seconds are measured.
    pub seconds: u32,
    /// How many seconds play first and are left out.
    pub warmup_seconds: u32,
    /// Threads that do nothing but keep a core busy while it plays.
    pub load_threads: usize,
    /// Seconds between the benchmark's jumps (0: it never jumps).
    pub seek_seconds: u32,
}

/// What became of the frames in the measured part of a sync run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFrames {
    /// Frames that were due between the first sent and the last.
    pub due: u64,
    /// Frames that went out.
    pub sent: u64,
    /// Frames that never did: skipped by the player, or replaced before the output got to them.
    pub missed: u64,
    /// Frames that went out again after a later one had (never, when playback is in order).
    pub repeated: u64,
    /// Frames from the old place that went out after a jump was asked for: already on their way.
    pub in_flight: u64,
}

/// Whether a sync run kept in step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncVerdict {
    pub pass: bool,
    /// At the 99th percentile a frame was on the wire within its own time and one send after.
    pub in_step: bool,
    /// The frames began no later at the end of the run than at its start ([`DRIFT_LIMIT`]).
    pub no_drift: bool,
    /// At least 99 frames in 100 went out.
    pub sustained: bool,
    /// No frame went out again after a later one.
    pub in_order: bool,
    /// Every jump and every new round reached the wire within [`RECOVERY_LIMIT`].
    pub recovers: bool,
    pub summary: String,
}

/// Everything measured in one sync run. Times are in milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub tier: String,
    pub pixels: u32,
    pub variant: Variant,
    pub sink: SinkKind,
    pub frame_ms: u32,
    /// Passes a second the engine's output thread makes.
    pub send_hz: u16,
    pub seconds: u32,
    pub sequence_seconds: u32,
    pub load_threads: usize,
    /// How far into a frame's own time the music was when the player began on it.
    pub began: Spread,
    /// How far past the start of a frame's own time the music was when the frame first went out.
    pub sent: Spread,
    /// A straight line through the median of `began` in each five seconds of the run, in ms a
    /// minute (positive: falling behind), and that slope's standard error: a short or a jittery
    /// run can't say much about drift.
    pub drift_per_minute: f64,
    pub drift_uncertainty: f64,
    pub frames: SyncFrames,
    /// From a jump being asked for to a frame from the new place going out.
    pub seek_recovery: Spread,
    /// From the music going back to its top to a frame of the new round going out.
    pub loop_recovery: Spread,
    /// How far past the lights' end the music was when it was taken back to its top.
    pub loop_late: Spread,
    pub verdict: SyncVerdict,
    /// The machine's one-minute load average when the run began and when it ended.
    pub load_before: Option<f64>,
    pub load_after: Option<f64>,
    pub notes: Vec<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The two marker props' colors for `frame`: its number plus one (so a dark frame is no
/// frame's), six hexadecimal digits, three to a prop.
fn marker_colors(frame: u32) -> [Rgb; 2] {
    let code = frame + 1;
    let digit = |n: u32| ((code >> (4 * n)) & 0xF) as u8 * LEVEL;
    [
        Rgb::new(digit(5), digit(4), digit(3)),
        Rgb::new(digit(2), digit(1), digit(0)),
    ]
}

/// The frame a marker controller's packet shows (none for a dark one, or one that isn't a
/// marker's).
fn marker_frame(packet: &[u8]) -> Option<u32> {
    let prop = MARKER_NODES as usize * 3;
    let data = packet.get(ddp::HEADER_LEN..ddp::HEADER_LEN + 2 * prop)?;
    let mut code = 0u32;
    for &level in data[..3].iter().chain(&data[prop..prop + 3]) {
        if level % LEVEL != 0 {
            return None;
        }
        code = code << 4 | u32::from(level / LEVEL);
    }
    code.checked_sub(1)
}

/// The workload's show and sequence with the marker added: two props on a DDP controller of
/// their own, after every other (so it is sent last), each showing half the frame's number
/// for exactly that frame.
fn marked(workload: &Workload, addresses: Option<&[SocketAddr]>) -> (Show, Sequence) {
    let mut show = workload.show_at(addresses);
    let mut sequence = workload.sequence.sequence.clone();
    let frame_ms = u64::from(workload.frame_ms());
    let frames = u32::try_from(sequence.duration_ms / frame_ms).unwrap_or(u32::MAX);
    let address = addresses
        .and_then(|a| a.get(show.controllers.len()))
        .map_or_else(|| MARKER_ADDRESS.to_string(), SocketAddr::to_string);
    let mut controller = Controller::new("Marker", address, Protocol::Ddp);
    let mut port = Port::new(1);
    for half in 0..2 {
        let line = Generator::Line {
            nodes: MARKER_NODES,
            length: 0.1,
        };
        let prop = Prop::new(format!("Marker {}", half + 1), ShapeSource::Generator(line));
        port.slots.push(PortSlot::new(prop.id));
        let mut row = Row::new(Target::Prop(prop.id));
        // One effect for each run of frames this half shows the same color for.
        let mut start = 0;
        for frame in 1..=frames {
            if frame == frames || marker_colors(frame)[half] != marker_colors(start)[half] {
                let effect = Effect::new(
                    EffectKind::On,
                    u64::from(start) * frame_ms,
                    u64::from(frame) * frame_ms,
                );
                row.layers[0]
                    .effects
                    .push(effect.with_palette([marker_colors(start)[half]]));
                start = frame;
            }
        }
        show.props.push(prop);
        sequence.rows.push(row);
    }
    controller.ports.push(port);
    show.controllers.push(controller);
    (show, sequence)
}

/// When a frame was noted, and where the music was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Noted {
    pub frame: u32,
    /// The music's position, in ms.
    pub music: f64,
    /// When, in ms since the run began.
    pub at: f64,
}

/// A jump: one the benchmark asked for, or the loop going round.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Jumped {
    /// When it was asked for (a loop: when the music was taken back), in ms since the run began.
    pub at: f64,
    /// The frame jumped to.
    pub frame: u32,
    /// For a loop: how far past the lights' end the music was.
    pub late: Option<f64>,
}

/// The music: a silent stopwatch that anything may read, which notes when the player first
/// reads a time in each frame and when it is taken back to the top.
struct Music {
    clock: Mutex<SilentClock>,
    began: Mutex<Vec<(u32, f64, Instant)>>,
    /// The music's position when it was taken back to its top, and when.
    rounds: Mutex<Vec<(f64, Instant)>>,
    frame_ms: f64,
    /// The sequence's length, in ms.
    length: f64,
}

impl Music {
    fn position(&self) -> f64 {
        ms(lock(&self.clock).position())
    }
}

/// The music as the player holds it: only the player reads the time here, so the first
/// reading in each frame is when the player began on that frame.
struct MusicClock {
    music: Arc<Music>,
    /// The frame the player last read a time in.
    frame: Cell<Option<u32>>,
}

impl AudioClock for MusicClock {
    fn start(&mut self, position: Duration) {
        lock(&self.music.clock).start(position);
    }

    fn pause(&mut self) {
        lock(&self.music.clock).pause();
    }

    fn resume(&mut self) {
        lock(&self.music.clock).resume();
    }

    fn seek(&mut self, position: Duration) {
        let mut clock = lock(&self.music.clock);
        let from = ms(clock.position());
        clock.seek(position);
        drop(clock);
        if position.is_zero() && from >= self.music.length {
            lock(&self.music.rounds).push((from, Instant::now()));
        }
    }

    fn position(&self) -> Duration {
        let position = lock(&self.music.clock).position();
        let music = ms(position);
        let frame = (music / self.music.frame_ms) as u32;
        if self.frame.replace(Some(frame)) != Some(frame) {
            lock(&self.music.began).push((frame, music, Instant::now()));
        }
        position
    }

    fn set_volume(&mut self, _volume: f32) {}

    fn set_speed(&mut self, speed: f32) {
        lock(&self.music.clock).set_speed(speed);
    }

    /// A song longer than the lights: the player follows it to their end.
    fn finished(&self) -> bool {
        false
    }
}

/// The benchmark's transport, noting where the music is when each new frame's marker goes out.
struct Marked {
    inner: Guarded,
    marker: SocketAddr,
    music: Arc<Music>,
    sent: Arc<Mutex<Vec<(u32, f64, Instant)>>>,
    last: Option<u32>,
}

impl Transport for Marked {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        let result = self.inner.send_to(packet, destination);
        if destination == self.marker
            && let Some(frame) = marker_frame(packet)
            && self.last != Some(frame)
        {
            self.last = Some(frame);
            lock(&self.sent).push((frame, self.music.position(), Instant::now()));
        }
        result
    }
}

/// Threads that do nothing but keep a core busy each, for as long as this lives.
struct Load {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Load {
    fn start(threads: usize) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let threads = (0..threads)
            .map(|n| {
                let stop = Arc::clone(&stop);
                std::thread::Builder::new()
                    .name(format!("pf-bench-load-{n}"))
                    .spawn(move || {
                        let mut x = n as u64;
                        while !stop.load(Ordering::Relaxed) {
                            for _ in 0..10_000 {
                                x = std::hint::black_box(x.wrapping_mul(6_364_136_223_846_793_005))
                                    .wrapping_add(1);
                            }
                        }
                    })
            })
            .collect::<io::Result<Vec<_>>>()
            .context("couldn't start the threads that keep the machine busy")?;
        Ok(Self { stop, threads })
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// What the notes of a run add up to.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Analysis {
    pub began: Vec<f64>,
    pub sent: Vec<f64>,
    pub drift_per_minute: f64,
    pub drift_uncertainty: f64,
    pub frames: SyncFrames,
    /// How long each jump took to reach the wire (none: it never did).
    pub recoveries: Vec<(Jumped, Option<f64>)>,
}

/// The least-squares line through `points`: its slope, and that slope's standard error (how far
/// off it may be, given how the points scatter about the line). No line, no slope.
fn line(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    if points.len() < 3 {
        return (0.0, 0.0);
    }
    let (mean_x, mean_y) = (
        points.iter().map(|p| p.0).sum::<f64>() / n,
        points.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let spread: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if spread == 0.0 {
        return (0.0, 0.0);
    }
    let slope = points
        .iter()
        .map(|p| (p.0 - mean_x) * (p.1 - mean_y))
        .sum::<f64>()
        / spread;
    let scatter: f64 = points
        .iter()
        .map(|p| (p.1 - mean_y - slope * (p.0 - mean_x)).powi(2))
        .sum();
    (slope, (scatter / (n - 2.0) / spread).sqrt())
}

/// How late frames typically began in each [`STRETCH`] of a run: the minutes in, and the median
/// of `starts` (when in ms, how late) there. A frame that began late because the one before it
/// took too long to draw says nothing of drift, and the median leaves it out.
fn stretches(starts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let Some(&(first, _)) = starts.first() else {
        return Vec::new();
    };
    let mut stretches: Vec<Vec<(f64, f64)>> = Vec::new();
    for &(at, late) in starts {
        let n = ((at - first) / STRETCH) as usize;
        if stretches.len() <= n {
            stretches.resize(n + 1, Vec::new());
        }
        stretches[n].push((at, late));
    }
    stretches
        .iter_mut()
        .filter(|stretch| !stretch.is_empty())
        .map(|stretch| {
            let minutes = stretch.iter().map(|p| p.0).sum::<f64>() / stretch.len() as f64 / 60_000.0;
            stretch.sort_by(|a, b| a.1.total_cmp(&b.1));
            let middle = stretch.len() / 2;
            let median = if stretch.len().is_multiple_of(2) {
                (stretch[middle - 1].1 + stretch[middle].1) / 2.0
            } else {
                stretch[middle].1
            };
            (minutes, median)
        })
        .collect()
}

/// Makes sense of a run's notes between `from` and `to` (ms since it began): `began` and `sent`
/// in the order they were noted, `jumps` in the order they happened.
pub(crate) fn analyze(
    began: &[Noted],
    sent: &[Noted],
    jumps: &[Jumped],
    frame_ms: f64,
    (from, to): (f64, f64),
) -> Analysis {
    let measured = |at: f64| at >= from && at < to;
    let late = |noted: &Noted| noted.music - f64::from(noted.frame) * frame_ms;
    let mut analysis = Analysis::default();
    // The player reads the clock again in a frame it has already begun (after a pause, say)
    // only by going back: a jump or a new round, which start a frame afresh.
    let starts: Vec<(f64, f64)> = began
        .iter()
        .filter(|n| measured(n.at))
        .map(|n| (n.at, late(n)))
        .collect();
    analysis.began = starts.iter().map(|p| p.1).collect();
    (analysis.drift_per_minute, analysis.drift_uncertainty) = line(&stretches(&starts));

    let mut next = 0;
    // The jump the lights are settling after, once a frame from the new place has gone out.
    let mut settling: Option<Jumped> = None;
    let mut last: Option<u32> = None;
    for noted in sent {
        while jumps.get(next).is_some_and(|j| j.at <= noted.at) {
            if let Some(unsettled) = settling.replace(jumps[next]) {
                analysis.recoveries.push((unsettled, None));
            }
            next += 1;
        }
        if let Some(jump) = settling {
            if noted.frame < jump.frame || noted.frame > jump.frame + NEAR {
                if measured(noted.at) {
                    analysis.frames.in_flight += 1;
                }
                continue;
            }
            analysis.recoveries.push((jump, Some(noted.at - jump.at)));
            settling = None;
            last = None;
        }
        if last.is_some_and(|before| noted.frame <= before) {
            if measured(noted.at) {
                analysis.frames.repeated += 1;
            }
            continue;
        }
        if measured(noted.at) {
            analysis.sent.push(late(noted));
            analysis.frames.sent += 1;
            if let Some(before) = last {
                analysis.frames.missed += u64::from(noted.frame - before - 1);
            }
        }
        last = Some(noted.frame);
    }
    analysis.frames.due = analysis.frames.sent + analysis.frames.missed;
    analysis.recoveries.retain(|(jump, _)| measured(jump.at));
    analysis
}

/// Plays the workload's sequence in the engine, looping, for `options.seconds` after the
/// warm-up, and reports how closely the lights kept to the music.
pub fn run_sync(workload: &Workload, sink: SinkKind, options: SyncOptions) -> Result<SyncResult> {
    let frame_ms = workload.frame_ms();
    let length_ms = workload.sequence.sequence.duration_ms;
    let frames = u32::try_from(length_ms / u64::from(frame_ms)).unwrap_or(u32::MAX);
    if frames <= 2 * NEAR {
        bail!("the sequence is too short to tell one round of it from the next: ask for a few seconds");
    }
    if options.seek_seconds > 0 && length_ms < 15_000 {
        bail!("the sequence is too short to jump about in: ask for fifteen seconds or more");
    }
    let warm = Duration::from_secs(u64::from(options.warmup_seconds));
    let total = warm + Duration::from_secs(u64::from(options.seconds.max(1)));

    let sinks = Sinks::open(sink, workload.show.show.controllers.len() + 1)?;
    let (show, sequence) = marked(workload, sinks.addresses());
    let (map, _) = pf_mapping::map_show(&show);
    let marker = pf_output::build_plan(&show, &map)
        .controllers
        .last()
        .and_then(|c| c.destination.clone().ok())
        .context("the marker controller has no address")?;

    let music = Arc::new(Music {
        clock: Mutex::default(),
        began: Mutex::default(),
        rounds: Mutex::default(),
        frame_ms: f64::from(frame_ms),
        length: length_ms as f64,
    });
    let sent: Arc<Mutex<Vec<(u32, f64, Instant)>>> = Arc::default();
    let clocks: ClockFactory = {
        let music = Arc::clone(&music);
        Arc::new(move |_| {
            Ok(Box::new(MusicClock {
                music: Arc::clone(&music),
                frame: Cell::new(None),
            }) as Box<dyn AudioClock>)
        })
    };
    let transport = {
        let (counted, music, sent) = (Arc::clone(&sinks.sent), Arc::clone(&music), Arc::clone(&sent));
        move || {
            Ok(Box::new(Marked {
                inner: Guarded::open(sink, Arc::clone(&counted))?,
                marker,
                music: Arc::clone(&music),
                sent: Arc::clone(&sent),
                last: None,
            }) as Box<dyn Transport>)
        }
    };
    let data = std::env::temp_dir().join(format!("pf-bench-{}", std::process::id()));
    let mut engine = Engine::new(data).with_transport(transport).with_clocks(clocks);
    engine.adopt_show(CheckedShow::new(show).map_err(|e| anyhow!("{e}"))?);
    engine.adopt_sequence_doc(sequence).map_err(|e| anyhow!("{e}"))?;
    engine.set_sequence_doc_loop(true);

    let load_before = load_average();
    let load = Load::start(options.load_threads)?;
    let began = Instant::now();
    let status = engine.play_sequence_doc(0).map_err(|e| anyhow!("{e}"))?;
    let mut notes = status.notes;

    // The benchmark's own jumps: on by seven seconds, then back by three, and so on, never to
    // the very top (that is the loop's) nor into the last second (the loop would follow at once).
    let mut jumps: Vec<Jumped> = Vec::new();
    let every = Duration::from_secs(u64::from(options.seek_seconds));
    let mut next_jump = warm + every;
    let mut checked = began;
    while began.elapsed() < total {
        if !every.is_zero() && began.elapsed() >= next_jump {
            next_jump += every;
            let now = music.position();
            let room = (length_ms as f64 - 2_000.0).max(f64::from(frame_ms));
            let step = if jumps.len().is_multiple_of(2) {
                7_000.0
            } else {
                -3_000.0
            };
            let target = (now + step).rem_euclid(room) + f64::from(frame_ms);
            let frame = (target / f64::from(frame_ms)) as u32;
            jumps.push(Jumped {
                at: ms(began.elapsed()),
                frame,
                late: None,
            });
            engine.seek_playback(u64::from(frame) * u64::from(frame_ms));
        }
        if checked.elapsed() >= Duration::from_millis(200) {
            checked = Instant::now();
            let status = engine.playback_status().context("the engine stopped playing")?;
            if let Some(error) = status.error {
                bail!("the engine stopped playing: {error}");
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    engine.stop_playback();
    drop(load);
    let load_after = load_average();
    let counted = sinks.sent.now();
    drop(sinks);

    let timeline = |notes: &Mutex<Vec<(u32, f64, Instant)>>| -> Vec<Noted> {
        lock(notes)
            .iter()
            .map(|&(frame, music, at)| Noted {
                frame,
                music,
                at: ms(at.saturating_duration_since(began)),
            })
            .collect()
    };
    jumps.extend(lock(&music.rounds).iter().map(|&(from, at)| Jumped {
        at: ms(at.saturating_duration_since(began)),
        frame: 0,
        late: Some(from - length_ms as f64),
    }));
    jumps.sort_by(|a, b| a.at.total_cmp(&b.at));
    let analysis = analyze(
        &timeline(&music.began),
        &timeline(&sent),
        &jumps,
        f64::from(frame_ms),
        (ms(warm), ms(total)),
    );
    if counted.refused + counted.failed > 0 {
        notes.push(format!(
            "{} packets were refused or failed to send.",
            counted.refused + counted.failed
        ));
    }
    Ok(result(
        workload,
        sink,
        options,
        &analysis,
        (load_before, load_after),
        notes,
    ))
}

/// A run's numbers and its verdict.
fn result(
    workload: &Workload,
    sink: SinkKind,
    options: SyncOptions,
    analysis: &Analysis,
    (load_before, load_after): (Option<f64>, Option<f64>),
    mut notes: Vec<String>,
) -> SyncResult {
    let frame_ms = workload.frame_ms();
    let send_hz = engine_send_rate(frame_ms);
    let recovery = |looped: bool| -> Vec<f64> {
        analysis
            .recoveries
            .iter()
            .filter(|(jump, _)| jump.late.is_some() == looped)
            .filter_map(|(_, took)| *took)
            .collect()
    };
    let lost = analysis
        .recoveries
        .iter()
        .filter(|(_, took)| took.is_none())
        .count();
    if lost > 0 {
        notes.push(format!(
            "{lost} jumps were overtaken by the next before a frame from the new place went out."
        ));
    }
    let (began, sent) = (Spread::of(&analysis.began), Spread::of(&analysis.sent));
    let (seek_recovery, loop_recovery) = (Spread::of(&recovery(false)), Spread::of(&recovery(true)));
    let loop_late: Vec<f64> = analysis
        .recoveries
        .iter()
        .filter_map(|(jump, _)| jump.late)
        .collect();
    let frames = analysis.frames;
    let budget = f64::from(frame_ms) + 1000.0 / f64::from(send_hz);
    let on_wire = 100.0 * frames.sent as f64 / frames.due.max(1) as f64;
    let in_step = sent.p99 <= budget;
    let no_drift = analysis.drift_per_minute.abs() <= DRIFT_LIMIT + 3.0 * analysis.drift_uncertainty;
    let (sustained, in_order) = (on_wire >= ON_TIME_PERCENT, frames.repeated == 0);
    let recovers = seek_recovery.max <= RECOVERY_LIMIT && loop_recovery.max <= RECOVERY_LIMIT && lost == 0;
    let worst = |what: &str, took: &Spread| match took.count {
        0 => format!("{what}: none"),
        n => format!("{what}: {n}, on the wire in {:.0} ms at worst", took.max),
    };
    let summary = format!(
        "frames began {:.1} ms into their time at the median and went out {:.1} ms in ({:.1} ms at the \
         99th percentile, {budget:.1} allowed); drift {:+.2} ms a minute, give or take {:.2}; {:.1}% of \
         {} frames went out ({} missed, {} sent again); {}; {}",
        began.p50,
        sent.p50,
        sent.p99,
        analysis.drift_per_minute,
        analysis.drift_uncertainty,
        on_wire,
        frames.due,
        frames.missed,
        frames.repeated,
        worst("jumps", &seek_recovery),
        worst("rounds of the loop", &loop_recovery),
    );
    SyncResult {
        tier: tier_label(workload.show.pixels()),
        pixels: workload.show.pixels(),
        variant: workload.sequence.summary.variant,
        sink,
        frame_ms,
        send_hz,
        seconds: options.seconds,
        sequence_seconds: u32::try_from(workload.sequence.sequence.duration_ms / 1000).unwrap_or(u32::MAX),
        load_threads: options.load_threads,
        began,
        sent,
        drift_per_minute: analysis.drift_per_minute,
        drift_uncertainty: analysis.drift_uncertainty,
        frames,
        seek_recovery,
        loop_recovery,
        loop_late: Spread::of(&loop_late),
        verdict: SyncVerdict {
            pass: in_step && no_drift && sustained && in_order && recovers,
            in_step,
            no_drift,
            sustained,
            in_order,
            recovers,
            summary,
        },
        load_before,
        load_after,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Controllers;
    use pf_output::{OutputSettings, RecordingTransport};

    /// A marker controller's packet for `frame`, as it leaves for the wire.
    fn packet(frame: u32) -> Vec<u8> {
        let mut packet = vec![0u8; ddp::HEADER_LEN];
        for color in marker_colors(frame) {
            for _ in 0..MARKER_NODES {
                packet.extend([color.r, color.g, color.b]);
            }
        }
        packet
    }

    #[test]
    fn a_frames_number_is_read_back_out_of_its_marker() {
        for frame in [0, 1, 14, 15, 16, 255, 256, 4094, 4095, 4096, 123_456, 0xFF_FFFE] {
            assert_eq!(marker_frame(&packet(frame)), Some(frame));
        }
        // A dark frame is no frame's marker, nor is anything else that isn't one.
        assert_eq!(marker_frame(&[0; ddp::HEADER_LEN + 12]), None);
        assert_eq!(marker_frame(&[0; ddp::HEADER_LEN + 3]), None);
        let mut dimmed = packet(40);
        dimmed[ddp::HEADER_LEN + 8] = 3;
        assert_eq!(marker_frame(&dimmed), None);
    }

    #[test]
    fn the_marker_comes_through_the_engines_own_drawing_and_wiring() {
        // A show with sACN and DDP controllers; the marker's is one more, sent after them.
        let workload = Workload::new(2_000, Variant::Typical, 4).unwrap().without_media();
        let (show, sequence) = marked(&workload, None);
        let errors = |report: &pf_model::ValidationReport| {
            report
                .issues
                .iter()
                .filter(|i| i.severity == pf_model::Severity::Error)
                .count()
        };
        assert_eq!(errors(&pf_model::validate_show(&show)), 0);
        assert!(pf_sequence::limit_problems(&sequence).is_empty());
        assert_eq!(show.controllers.len(), workload.show.show.controllers.len() + 1);
        let (map, report) = pf_mapping::map_show(&show);
        assert_eq!(errors(&report), 0);
        let plan = pf_output::build_plan(&show, &map);
        let marker = plan.controllers.last().unwrap().destination.clone().unwrap();
        assert!(marker.ip().is_loopback());
        assert_eq!(
            plan.controllers
                .iter()
                .filter(|c| c.destination == Ok(marker))
                .count(),
            1,
            "only the marker's packets go there"
        );
        let mut renderer = pf_render::Renderer::new(&show, &map);
        let mut controllers = Controllers::new(plan, &OutputSettings::default()).unwrap();
        let mut frame = vec![0u8; map.frame_len];
        let mut read = |frame: &[u8]| {
            let (mut transport, sent) = RecordingTransport::new();
            controllers.send(frame, &mut transport);
            let sent = sent.lock().unwrap();
            let (last, to) = sent.last().unwrap();
            assert_eq!(*to, marker, "the marker goes out last");
            marker_frame(last)
        };
        for index in 0..160 {
            renderer.render_frame(&sequence, u64::from(index), &mut frame);
            assert_eq!(read(&frame), Some(index));
        }
        frame.fill(0);
        assert_eq!(read(&frame), None);
    }

    /// `frame` noted at `at` ms with the music `late` ms into the frame's own time.
    fn noted(frame: u32, at: f64, late: f64) -> Noted {
        Noted {
            frame,
            music: f64::from(frame) * 25.0 + late,
            at,
        }
    }

    const ALL: (f64, f64) = (0.0, f64::MAX);

    #[test]
    fn frames_in_order_are_all_on_time() {
        let began: Vec<Noted> = (0..100)
            .map(|f| noted(f, f64::from(f) * 25.0 + 1.0, 1.0))
            .collect();
        let sent: Vec<Noted> = (0..100)
            .map(|f| noted(f, f64::from(f) * 25.0 + 9.0, 9.0))
            .collect();
        let found = analyze(&began, &sent, &[], 25.0, ALL);
        assert_eq!(
            found.frames,
            SyncFrames {
                due: 100,
                sent: 100,
                ..SyncFrames::default()
            }
        );
        assert!(found.began.iter().all(|&late| late == 1.0));
        assert!(found.sent.iter().all(|&late| late == 9.0));
        assert_eq!((found.drift_per_minute, found.drift_uncertainty), (0.0, 0.0));
        assert!(found.recoveries.is_empty());
    }

    #[test]
    fn frames_that_never_went_out_or_went_out_again_are_counted() {
        let sent: Vec<Noted> = [0, 1, 2, 5, 6, 4, 7, 7, 10]
            .iter()
            .enumerate()
            .map(|(i, &f)| noted(f, i as f64 * 25.0, 3.0))
            .collect();
        let frames = analyze(&[], &sent, &[], 25.0, ALL).frames;
        // 3, 4, 8 and 9 never went out when due; 4 and the second 7 went out after a later one.
        assert_eq!((frames.sent, frames.missed, frames.repeated), (7, 4, 2));
        assert_eq!(frames.due, 11);
    }

    #[test]
    fn a_jump_is_timed_to_the_first_frame_from_the_new_place() {
        let mut sent: Vec<Noted> = (0..10)
            .map(|f| noted(f, f64::from(f) * 25.0 + 5.0, 5.0))
            .collect();
        // Asked for at 220 ms: frame 9, already on its way, still goes out, then the new place.
        let jump = Jumped {
            at: 220.0,
            frame: 400,
            late: None,
        };
        sent.extend((0..10).map(|k| noted(400 + k, 262.0 + f64::from(k) * 25.0, 8.0)));
        // A round of the loop, with the last frames of the old round in flight.
        let round = Jumped {
            at: 600.0,
            frame: 0,
            late: Some(1.5),
        };
        sent.push(noted(1199, 601.0, 9.0));
        sent.extend((0..5).map(|k| noted(k, 614.0 + f64::from(k) * 25.0, 14.0)));
        let found = analyze(&[], &sent, &[jump, round], 25.0, ALL);
        assert_eq!(found.recoveries, vec![(jump, Some(42.0)), (round, Some(14.0))]);
        // Frame 9 went out after the jump was asked for, and 1199 after the music went back.
        assert_eq!(found.frames.in_flight, 2);
        // Nothing is missing or out of order for having jumped.
        assert_eq!((found.frames.missed, found.frames.repeated), (0, 0));
        assert_eq!(found.frames.sent, 9 + 10 + 5);
        assert_eq!(found.sent.len(), 24);
        assert!(found.sent.iter().all(|&late| (5.0..=14.0).contains(&late)));
    }

    #[test]
    fn a_jump_overtaken_by_the_next_never_reached_the_wire() {
        let first = Jumped {
            at: 100.0,
            frame: 400,
            late: None,
        };
        let second = Jumped {
            at: 105.0,
            frame: 800,
            late: None,
        };
        let sent = vec![noted(3, 90.0, 5.0), noted(4, 110.0, 5.0), noted(800, 120.0, 5.0)];
        let found = analyze(&[], &sent, &[first, second], 25.0, ALL);
        assert_eq!(found.recoveries, vec![(first, None), (second, Some(15.0))]);
    }

    #[test]
    fn only_what_happened_in_the_measured_part_counts() {
        let sent: Vec<Noted> = (0..100).map(|f| noted(f, f64::from(f) * 25.0, 2.0)).collect();
        let early = Jumped {
            at: 10.0,
            frame: 0,
            late: None,
        };
        let found = analyze(&sent, &sent, &[early], 25.0, (1000.0, 2000.0));
        assert_eq!((found.frames.sent, found.frames.due), (40, 40));
        assert_eq!((found.began.len(), found.sent.len()), (40, 40));
        assert!(found.recoveries.is_empty());
    }

    #[test]
    fn lights_falling_behind_show_as_drift() {
        // A minute of frames, each beginning a little later than the last: six ms by the end.
        let began: Vec<Noted> = (0..2400)
            .map(|f| {
                let at = f64::from(f) * 25.0;
                noted(f, at, 1.0 + at / 10_000.0)
            })
            .collect();
        let found = analyze(&began, &[], &[], 25.0, ALL);
        assert!(
            (found.drift_per_minute - 6.0).abs() < 1e-6,
            "{}",
            found.drift_per_minute
        );
        assert!(found.drift_uncertainty < 1e-6);
        // Frames that began late because the one before was slow to draw are not drift: here
        // one frame in three of the last third of the run, 20 ms late.
        let slow_to_draw: Vec<Noted> = (0..2400)
            .map(|f| {
                let late = if f >= 1600 && f % 3 == 0 { 20.0 } else { 1.0 };
                noted(f, f64::from(f) * 25.0, late)
            })
            .collect();
        let found = analyze(&slow_to_draw, &[], &[], 25.0, ALL);
        assert_eq!((found.drift_per_minute, found.drift_uncertainty), (0.0, 0.0));
        // A second of frames is too little to draw a line through.
        let found = analyze(&began[..40], &[], &[], 25.0, ALL);
        assert_eq!((found.drift_per_minute, found.drift_uncertainty), (0.0, 0.0));
    }

    #[test]
    fn the_verdict_says_what_was_out_of_step() {
        let workload = Workload::new(600, Variant::Typical, 4).unwrap();
        let options = SyncOptions {
            seconds: 60,
            warmup_seconds: 0,
            load_threads: 0,
            seek_seconds: 0,
        };
        let jump = Jumped {
            at: 0.0,
            frame: 400,
            late: None,
        };
        let good = Analysis {
            began: vec![1.0; 100],
            sent: vec![12.0; 100],
            frames: SyncFrames {
                due: 100,
                sent: 100,
                ..SyncFrames::default()
            },
            recoveries: vec![(jump, Some(30.0))],
            ..Analysis::default()
        };
        let verdict = |analysis: &Analysis| {
            result(
                &workload,
                SinkKind::Null,
                options,
                analysis,
                (None, None),
                Vec::new(),
            )
            .verdict
        };
        let all = verdict(&good);
        assert!(all.pass, "{all:?}");
        assert!(
            all.summary.contains("jumps: 1, on the wire in 30 ms at worst"),
            "{}",
            all.summary
        );
        assert!(
            all.summary.contains("rounds of the loop: none"),
            "{}",
            all.summary
        );

        // A frame and a send behind at the 99th percentile.
        let late = Analysis {
            sent: [vec![12.0; 98], vec![38.0; 2]].concat(),
            ..good.clone()
        };
        let v = verdict(&late);
        assert!(!v.pass && !v.in_step && v.no_drift && v.sustained && v.in_order && v.recovers);
        // A slope the run is sure of, and one it isn't.
        let drifting = Analysis {
            drift_per_minute: 2.0,
            drift_uncertainty: 0.1,
            ..good.clone()
        };
        assert!(!verdict(&drifting).no_drift);
        let unsure = Analysis {
            drift_uncertainty: 0.5,
            ..drifting
        };
        assert!(verdict(&unsure).no_drift);
        let dropping = Analysis {
            frames: SyncFrames {
                due: 100,
                sent: 98,
                missed: 2,
                repeated: 1,
                in_flight: 0,
            },
            ..good.clone()
        };
        let v = verdict(&dropping);
        assert!(!v.sustained && !v.in_order && v.in_step);
        let slow = Analysis {
            recoveries: vec![(jump, Some(130.0)), (jump, None)],
            ..good
        };
        assert!(!verdict(&slow).recovers);
    }
}
