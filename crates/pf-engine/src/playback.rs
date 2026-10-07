//! Playing sequences with their music: a rendered sequence (`.fseq`), or an authored sequence
//! document rendered live, with a preview of the props.
//!
//! For a file, each controller that knows where its data sits in the sequence
//! ([`pf_model::SequenceChannels`]) receives that block of every frame unchanged, the way FPP sends
//! it, and the preview maps the same channels back through the show's wiring onto the props. An
//! authored sequence is rendered into the show frame and sent through the show's normal output
//! plan, like a test pattern.
//!
//! Both follow the same clock and player loop: the frame due is the music's position plus the
//! sequence's offset (the music, or a silent stopwatch when there is none).

use crate::error::EngineError;
use crate::output::{ControllerStatus, OutputKey, controller_status, output_key};
use pf_audio::{AudioClock, AudioError, MusicPlayer, SilentClock};
use pf_frame::FrameWriter;
use pf_fseq::Sequence;
use pf_mapping::ChannelMap;
use pf_model::{Protocol, SequenceId, Show};
use pf_output::{OutputHandle, OutputPlan, OutputSettings, PassthroughRoute, Transport, wire_order};
use pf_render::Renderer;
use pf_sequence::Sequence as SequenceDoc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Highest output send rate, in packets per controller per second.
const MAX_SEND_RATE: u32 = 120;

/// The highest sACN universe number.
const MAX_UNIVERSE: usize = 63_999;

/// How often the player checks for pause, seek, and stop while waiting.
const POLL: Duration = Duration::from_millis(10);

/// How long the music may stand still while playing before the lights keep time on their own
/// (a sound device that stopped responding).
const STALL: Duration = Duration::from_secs(1);

/// How long starting waits for the music to open.
const READY_LIMIT: Duration = Duration::from_secs(5);

/// How long stopping waits for the player before leaving it to finish by itself (a stalled sound
/// device can hold it up).
const JOIN_LIMIT: Duration = Duration::from_secs(1);

const CRASHED: &str = "Playback stopped unexpectedly. Press play to try again.";

const STALL_NOTE: &str = "The sound output stopped responding, so the lights are keeping time on their own.";

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
    /// The show's sequence entry being played, if any.
    pub sequence: Option<SequenceId>,
    /// The music playing along, if any.
    pub music: Option<PathBuf>,
    pub offset_ms: i32,
    pub volume: f32,
    /// True when playing an authored sequence document rather than a file.
    pub authored: bool,
    /// Playing again from the top each time it reaches the end (lights and music together).
    pub looping: bool,
}

/// New controllers and layout to send to, after an edit to the show.
struct Rebuild {
    show: Show,
    map: ChannelMap,
    writer: FrameWriter,
    /// For an authored sequence: a renderer for the edited show.
    renderer: Option<Renderer>,
}

/// What the player thread and the engine share.
struct Control {
    paused: bool,
    /// Where to jump to, in sequence (light) time.
    seek_to: Option<u64>,
    frame: u32,
    /// Frames in the sequence and the time between them (kept up to date by the player: an
    /// authored sequence can change length while it plays).
    frames: u32,
    frame_ms: u32,
    /// Both the lights and the music are done (or a read error stopped the lights).
    ended: bool,
    /// The lights are past their end (dark) while the music plays on.
    lights_done: bool,
    error: Option<String>,
    /// How far the lights run ahead of the music.
    offset_ms: i32,
    volume: f32,
    /// At the end, go back to the top and play on (see [`PlaybackSession::set_looping`]).
    looping: bool,
    /// Why the music isn't playing at all.
    music_note: Option<String>,
    /// Trouble with the music after it started.
    clock_note: Option<String>,
    rebuild: Option<Rebuild>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            paused: false,
            seek_to: None,
            frame: 0,
            frames: 0,
            frame_ms: 25,
            ended: false,
            lights_done: false,
            error: None,
            offset_ms: 0,
            volume: 1.0,
            looping: false,
            music_note: None,
            clock_note: None,
            rebuild: None,
        }
    }
}

/// Makes the clock playback follows: the music when there is a file, else a silent stopwatch.
pub type ClockFactory = Arc<dyn Fn(Option<&Path>) -> Result<Box<dyn AudioClock>, AudioError> + Send + Sync>;

/// The real clock: plays the music on the default sound output.
pub fn music_clocks() -> ClockFactory {
    Arc::new(|music: Option<&Path>| match music {
        Some(path) => MusicPlayer::open(path).map(|p| Box::new(p) as Box<dyn AudioClock>),
        None => Ok(Box::new(SilentClock::new()) as Box<dyn AudioClock>),
    })
}

/// Music position (ms; negative before the song starts) for lights at `light` ms. From the very
/// top, the song plays from its start even when the lights run behind it: they hold their first
/// frame until the music catches up.
fn music_for(light: u64, offset_ms: i32) -> i64 {
    let music = i64::try_from(light)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::from(offset_ms));
    if light == 0 { music.min(0) } else { music }
}

/// Lights time for a music position: `music + offset`, never below zero.
fn light_for(music_ms: i64, offset_ms: i32) -> u64 {
    u64::try_from(music_ms.saturating_add(i64::from(offset_ms)).max(0)).unwrap_or(0)
}

fn millis(ms: i64) -> Duration {
    Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// What to play: the sequence file, its music, and how they line up.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayRequest {
    pub path: PathBuf,
    pub music: Option<PathBuf>,
    pub offset_ms: i32,
    pub volume: f32,
    /// The show's sequence entry, when playing one.
    pub sequence: Option<SequenceId>,
}

/// Waits until a starting sequence's music is open and playing (or known to be unavailable).
/// Holds nothing of the engine, so callers can wait without blocking other commands.
#[derive(Debug)]
pub struct PlaybackReady(Option<Receiver<()>>);

impl PlaybackReady {
    /// Waits up to a few seconds (returns early if the player stops).
    pub fn wait(self) {
        if let Some(ready) = self.0 {
            let _ = ready.recv_timeout(READY_LIMIT);
        }
    }
}

fn lock(control: &Mutex<Control>) -> std::sync::MutexGuard<'_, Control> {
    control.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Works out which block of the sequence each controller receives.
pub(crate) fn routes(show: &Show, channels: usize) -> (Vec<PassthroughRoute>, Vec<String>) {
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
        let mut count = (range.count as usize).min(channels - start);
        if count < range.count as usize {
            notes.push(format!(
                "{} expects {} channels, but this sequence only has {count} for it.",
                controller.name, range.count
            ));
        }
        if let Protocol::Sacn(sacn) = &controller.protocol {
            // Universe numbers stop at 63999: leave out anything that would go past it.
            let size = usize::from(sacn.universe_size.channels());
            let first = usize::from(sacn.start_universe.unwrap_or(1));
            let room = (MAX_UNIVERSE + 1).saturating_sub(first) * size;
            if count > room {
                notes.push(format!(
                    "{} would need sACN universes past {MAX_UNIVERSE}, which don't exist, so the channels beyond that are left out.",
                    controller.name
                ));
                count = room;
                if count == 0 {
                    continue;
                }
            }
        }
        routes.push(PassthroughRoute {
            id: controller.id,
            name: controller.name.clone(),
            address: controller.address.clone(),
            protocol: controller.protocol,
            start,
            count,
            ddp_offset_base: if range.raw_ddp_offsets {
                u32::try_from(start).unwrap_or(u32::MAX)
            } else {
                0
            },
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
        let Some(range) = controller
            .sequence_channels
            .filter(|r| r.start >= 1 && r.count >= 1)
        else {
            continue;
        };
        let base = range.start as usize - 1;
        // Only this controller's own block of the sequence may feed its props.
        let end = base.saturating_add(range.count as usize);
        for span in &output.spans {
            let cpp = usize::from(span.channels_per_pixel);
            let order = wire_order(span.color_order);
            let pixels = span.pixels as usize;
            for k in 0..pixels {
                let wire = if span.reverse { pixels - 1 - k } else { k };
                let src = base + span.controller_channel + wire * cpp;
                let dst = span.frame_offset + k * cpp;
                if src + cpp > end {
                    continue;
                }
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

/// Keeps the music's time for the lights: the music clock, a count-in while the lights play
/// before the song starts, and a stopwatch from where the music stood once it is over or stops
/// responding.
struct MusicTime {
    clock: Box<dyn AudioClock>,
    /// Counting in to the start of the song: a stopwatch, and how long the count-in lasts.
    count_in: Option<(SilentClock, Duration)>,
    paused: bool,
    /// The clock's last position, and when it last moved (or was resumed or jumped).
    last: Duration,
    moved: Instant,
}

impl MusicTime {
    fn start(mut clock: Box<dyn AudioClock>, music_ms: i64, paused: bool) -> Self {
        let mut count_in = None;
        if music_ms >= 0 {
            if paused {
                clock.seek(millis(music_ms));
            } else {
                clock.start(millis(music_ms));
            }
        } else {
            let mut watch = SilentClock::new();
            if !paused {
                watch.start(Duration::ZERO);
            }
            count_in = Some((watch, millis(-music_ms)));
        }
        let last = clock.position();
        Self {
            clock,
            count_in,
            paused,
            last,
            moved: Instant::now(),
        }
    }

    /// Moves to `music_ms` (negative: a count-in before the song).
    fn jump(&mut self, music_ms: i64) {
        if music_ms < 0 {
            if self.count_in.is_none() && !self.paused {
                self.clock.pause();
            }
            self.clock.seek(Duration::ZERO);
            let mut watch = SilentClock::new();
            if !self.paused {
                watch.start(Duration::ZERO);
            }
            self.count_in = Some((watch, millis(-music_ms)));
        } else {
            let counting = self.count_in.take().is_some();
            self.clock.seek(millis(music_ms));
            if counting && !self.paused {
                self.clock.resume();
            }
        }
        self.last = self.clock.position();
        self.moved = Instant::now();
    }

    fn set_paused(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        match &mut self.count_in {
            Some((watch, _)) if paused => watch.pause(),
            Some((watch, _)) => watch.resume(),
            None if paused => self.clock.pause(),
            None => self.clock.resume(),
        }
        self.moved = Instant::now();
    }

    /// The music's position in ms (negative while counting in).
    fn now_ms(&mut self) -> i64 {
        if let Some((watch, length)) = &self.count_in {
            let elapsed = watch.position();
            if elapsed < *length {
                return -i64::try_from((*length - elapsed).as_millis()).unwrap_or(i64::MAX);
            }
            // The count-in is over: the song starts.
            self.count_in = None;
            self.clock.start(Duration::ZERO);
            self.last = Duration::ZERO;
            self.moved = Instant::now();
            return 0;
        }
        let position = self.clock.position();
        let now = Instant::now();
        if position != self.last {
            self.last = position;
            self.moved = now;
        }
        let still = now - self.moved;
        let own_time = !self.paused && (self.clock.finished() || still > STALL);
        let music = if own_time { self.last + still } else { position };
        i64::try_from(music.as_millis()).unwrap_or(i64::MAX)
    }

    /// The music should be moving but has stood still for a while.
    fn stalled(&self) -> bool {
        !self.paused && self.count_in.is_none() && !self.clock.finished() && self.moved.elapsed() > STALL
    }

    /// Nothing more of the song is left to hear.
    fn music_done(&self) -> bool {
        self.count_in.is_none() && (self.clock.finished() || self.stalled())
    }
}

/// Where a playing session's frames come from: a rendered `.fseq` file, or an authored
/// sequence rendered live.
trait FrameSource: Send + 'static {
    /// Frames in the sequence and the time between them. Asked every loop: an authored sequence
    /// can change length while it plays.
    fn timing(&mut self) -> (u32, u32);
    /// Fills `out` (what the output thread sends) with frame `index`. Reading and rendering
    /// happen here, without holding the preview.
    fn frame(&mut self, index: u32, out: &mut [u8]) -> Result<(), String>;
    /// Paints the props' show frame for the preview from the frame in `out` (quick: the preview
    /// is locked meanwhile).
    fn paint(&self, out: &[u8], preview: &mut [u8]);
    /// True when the current frame must be drawn again although time hasn't moved (the
    /// sequence or the show was edited).
    fn changed(&mut self) -> bool {
        false
    }
    /// Draws for an edited show (new controllers or layout) from now on.
    fn relayout(&mut self, show: Show, map: ChannelMap, renderer: Option<Renderer>);
}

/// Frames read from a rendered sequence file; the preview maps them back onto the props.
struct FileFrames {
    sequence: Sequence,
    show: Show,
    map: ChannelMap,
}

impl FrameSource for FileFrames {
    fn timing(&mut self) -> (u32, u32) {
        let header = self.sequence.header();
        (header.frames, header.step_ms)
    }

    fn frame(&mut self, index: u32, out: &mut [u8]) -> Result<(), String> {
        self.sequence.read_frame(index, out).map_err(|e| e.to_string())
    }

    fn paint(&self, out: &[u8], preview: &mut [u8]) {
        paint_preview(&self.show, &self.map, out, preview);
    }

    fn relayout(&mut self, show: Show, map: ChannelMap, _renderer: Option<Renderer>) {
        self.show = show;
        self.map = map;
    }
}

/// Edits waiting to reach a playing authored sequence.
#[derive(Default)]
struct Pending {
    doc: Option<Arc<SequenceDoc>>,
    renderer: Option<Renderer>,
}

/// An edited document or show, handed to the player thread without stopping it.
#[derive(Default)]
pub(crate) struct LiveUpdates {
    waiting: AtomicBool,
    pending: Mutex<Pending>,
}

impl LiveUpdates {
    fn send(&self, update: impl FnOnce(&mut Pending)) {
        update(&mut self.pending.lock().unwrap_or_else(PoisonError::into_inner));
        self.waiting.store(true, Ordering::Release);
    }
}

/// Frames rendered from an authored sequence, straight into the show frame.
struct RenderedFrames {
    doc: Arc<SequenceDoc>,
    renderer: Renderer,
    updates: Arc<LiveUpdates>,
}

impl FrameSource for RenderedFrames {
    fn timing(&mut self) -> (u32, u32) {
        let frames = u32::try_from(self.doc.frame_count()).unwrap_or(u32::MAX);
        (frames, self.doc.frame_ms.max(1))
    }

    fn frame(&mut self, index: u32, out: &mut [u8]) -> Result<(), String> {
        self.renderer.render_frame(&self.doc, u64::from(index), out);
        Ok(())
    }

    fn paint(&self, out: &[u8], preview: &mut [u8]) {
        preview.copy_from_slice(out);
    }

    fn changed(&mut self) -> bool {
        if !self.updates.waiting.swap(false, Ordering::Acquire) {
            return false;
        }
        let mut pending = self
            .updates
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(doc) = pending.doc.take() {
            self.doc = doc;
        }
        if let Some(renderer) = pending.renderer.take() {
            self.renderer = renderer;
        }
        true
    }

    fn relayout(&mut self, show: Show, map: ChannelMap, renderer: Option<Renderer>) {
        self.renderer = renderer.unwrap_or_else(|| Renderer::new(&show, &map));
    }
}

/// What the player thread draws with and writes to.
struct Frames {
    source: Box<dyn FrameSource>,
    writer: FrameWriter,
    preview: Arc<Mutex<Vec<u8>>>,
    /// The current frame, as sent.
    raw: Arc<Mutex<Vec<u8>>>,
}

impl Frames {
    /// Draws `frame` and sends it, updating the preview. On an error it goes dark and returns
    /// the error.
    fn show(&mut self, frame: u32) -> Result<(), String> {
        if let Err(error) = self.source.frame(frame, self.writer.frame_mut()) {
            self.dark();
            return Err(error);
        }
        self.source.paint(
            self.writer.frame_mut(),
            &mut self.preview.lock().unwrap_or_else(PoisonError::into_inner),
        );
        self.raw
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .copy_from_slice(self.writer.frame_mut());
        self.writer.publish();
        Ok(())
    }

    fn dark(&mut self) {
        self.writer.frame_mut().fill(0);
        self.writer.publish();
        self.preview
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .fill(0);
        self.raw.lock().unwrap_or_else(PoisonError::into_inner).fill(0);
    }

    fn rebuild(&mut self, rebuild: Rebuild) {
        let Rebuild {
            show,
            map,
            mut writer,
            renderer,
        } = rebuild;
        *self.preview.lock().unwrap_or_else(PoisonError::into_inner) = vec![0; map.frame_len];
        // An authored sequence's frame is the show frame, so its size follows the layout.
        self.raw
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .resize(writer.frame_mut().len(), 0);
        self.writer = writer;
        self.source.relayout(show, map, renderer);
    }
}

/// Records that playback stopped if the player thread panics (before anyone waiting for it hears
/// that it's gone).
struct CrashGuard<'a>(&'a Mutex<Control>);

impl Drop for CrashGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let mut c = lock(self.0);
            c.ended = true;
            c.error.get_or_insert_with(|| CRASHED.to_string());
        }
    }
}

/// The player thread: follows the music and sends the frame that is due.
fn run_player(
    mut frames: Frames,
    control: &Mutex<Control>,
    stop: &AtomicBool,
    clocks: &ClockFactory,
    music: Option<&Path>,
    start_ms: u64,
    ready: std::sync::mpsc::Sender<()>,
) {
    let _guard = CrashGuard(control);
    // The clock lives on this thread (sound devices needn't be shareable). Without working
    // music, a silent stopwatch keeps time instead.
    let clock: Box<dyn AudioClock> = match clocks(music) {
        Ok(clock) => clock,
        Err(error) => {
            lock(control).music_note = Some(format!("{error} Only the lights are playing."));
            Box::new(SilentClock::new())
        }
    };
    let (offset, volume, paused) = {
        let c = lock(control);
        (c.offset_ms, c.volume, c.paused)
    };
    let mut time = MusicTime::start(clock, music_for(start_ms, offset), paused);
    time.clock.set_volume(volume);
    let _ = ready.send(());
    let (_, first_step) = frames.source.timing();
    let mut shown = Some(u32::try_from(start_ms / u64::from(first_step.max(1))).unwrap_or(u32::MAX));
    let mut dark = false;
    let mut applied_volume = volume;
    let mut note: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        let (total, step) = frames.source.timing();
        let step = step.max(1);
        let step_ms = u64::from(step);
        let (paused, seek, offset, volume, ended, looping, rebuild) = {
            let mut c = lock(control);
            c.frames = total;
            c.frame_ms = step;
            (
                c.paused,
                c.seek_to.take(),
                c.offset_ms,
                c.volume,
                c.ended,
                c.looping,
                c.rebuild.take(),
            )
        };
        if let Some(rebuild) = rebuild {
            frames.rebuild(rebuild);
            shown = None;
            dark = false;
        }
        if volume != applied_volume {
            time.clock.set_volume(volume);
            applied_volume = volume;
        }
        if let Some(target) = seek {
            time.jump(music_for(target, offset));
            shown = None;
            dark = false;
            lock(control).lights_done = false;
        }
        // Once everything is done the music stays stopped; seeking clears `ended` and plays again.
        time.set_paused(paused || ended);
        let trouble = time
            .clock
            .problem()
            .or_else(|| time.stalled().then(|| STALL_NOTE.to_string()));
        if trouble != note {
            lock(control).clock_note = trouble.clone();
            note = trouble;
        }
        let music_ms = time.now_ms();
        let mut light = match seek {
            // A jump while paused shows exactly the frame asked for.
            Some(target) if paused => target,
            _ => light_for(music_ms, offset),
        };
        if looping && !paused && !ended && total > 0 && light / step_ms >= u64::from(total) {
            // Round again: the music goes back to its top the moment the lights reach their end,
            // and the lights follow it from there, so the two start every loop together and
            // nothing builds up between them. (The player wakes on frame boundaries, so this is
            // within a few ms of the end; the music's jump lands before its next sample.)
            time.jump(music_for(0, offset));
            shown = None;
            if dark {
                dark = false;
                lock(control).lights_done = false;
            }
            light = light_for(time.now_ms(), offset);
        }
        let due = light / step_ms;
        // An edited sequence or show redraws the current frame, even while paused.
        let changed = frames.source.changed();
        if due >= u64::from(total) {
            if !dark {
                frames.dark();
                dark = true;
                shown = None;
                lock(control).lights_done = true;
            }
            if !ended && time.music_done() {
                lock(control).ended = true;
                time.set_paused(true);
            }
            std::thread::sleep(POLL);
            continue;
        }
        if dark {
            dark = false;
            lock(control).lights_done = false;
        }
        let due = due as u32;
        if shown != Some(due) || changed {
            if let Err(error) = frames.show(due) {
                let mut c = lock(control);
                c.error = Some(error);
                c.ended = true;
                return;
            }
            lock(control).frame = due;
            shown = Some(due);
        }
        // Wake at the next frame boundary (or sooner, to notice pause/seek/stop).
        let until_next = step_ms - light % step_ms;
        std::thread::sleep(Duration::from_millis(until_next).min(POLL));
    }
}

/// How often the output sends: twice the sequence's rate, so every frame goes out at least once
/// (resending a frame is harmless). That holds for steps of about 17 ms or more; for shorter steps
/// the `MAX_SEND_RATE` cap applies and a frame may occasionally be skipped.
fn send_rate(step_ms: u32) -> u16 {
    u16::try_from((2000 / step_ms.max(1)).clamp(1, MAX_SEND_RATE)).unwrap_or(1)
}

/// The output plan for an authored sequence: the show's normal plan (like a test pattern), or
/// nothing to send when the show has errors (with notes saying why only the preview plays) or
/// sending is turned off (`send` false: the preview alone, while editing).
fn document_plan(
    show: &Show,
    map: &ChannelMap,
    show_error: Option<&str>,
    send: bool,
    frame_ms: u32,
) -> (OutputPlan, Vec<String>) {
    let mut notes = Vec::new();
    let nothing = || OutputPlan {
        frame_len: map.frame_len,
        frame_rate: 1,
        controllers: Vec::new(),
        luts: Vec::new(),
    };
    let mut plan = if let Some(error) = show_error {
        notes.push(format!(
            "The show has errors, so only the preview plays (nothing is sent to controllers): {error}"
        ));
        nothing()
    } else if !send {
        nothing()
    } else {
        pf_output::build_plan(show, map)
    };
    if show_error.is_none() && send && plan.controllers.iter().all(|c| c.spans.is_empty()) {
        notes.push("No props are wired to a controller, so only the preview shows the sequence.".to_string());
    }
    plan.frame_rate = send_rate(frame_ms);
    (plan, notes)
}

/// What a session plays, and what it was built from (when an edit changes that, its output is
/// rebuilt or it restarts).
pub(crate) enum SessionKind {
    File {
        request: PlayRequest,
        routes: Vec<PassthroughRoute>,
        map: ChannelMap,
        channels: usize,
    },
    Document {
        music: Option<PathBuf>,
        key: OutputKey,
        map: ChannelMap,
        /// Whether the show had errors (then only the preview plays).
        preview_only: bool,
        /// Whether sending to controllers was turned on.
        sending: bool,
        /// The frame time the output's send rate was chosen for.
        frame_ms: u32,
        updates: Arc<LiveUpdates>,
    },
}

/// What an authored sequence session plays.
pub(crate) struct DocumentRequest {
    pub doc: Arc<SequenceDoc>,
    /// The document's file, if it has been saved.
    pub path: Option<PathBuf>,
    pub music: Option<PathBuf>,
    /// The show's first error, if it has any (then only the preview plays).
    pub show_error: Option<String>,
    /// Send to the controllers (false: only the preview plays).
    pub send: bool,
    pub volume: f32,
    /// Play again from the top each time it reaches the end.
    pub looping: bool,
}

/// Everything [`PlaybackSession::launch`] needs besides the frames.
struct Launch {
    plan: OutputPlan,
    out_len: usize,
    preview_len: usize,
    music: Option<PathBuf>,
    offset_ms: i32,
    volume: f32,
    looping: bool,
}

/// A sequence playing: a player thread producing frames on time and the output thread sending them.
pub(crate) struct PlaybackSession {
    kind: SessionKind,
    path: PathBuf,
    notes: Vec<String>,
    control: Arc<Mutex<Control>>,
    stop: Arc<AtomicBool>,
    player: Option<JoinHandle<()>>,
    ready: Option<Receiver<()>>,
    handle: Option<OutputHandle>,
    preview: Arc<Mutex<Vec<u8>>>,
    /// The current frame, as sent.
    raw: Arc<Mutex<Vec<u8>>>,
}

impl PlaybackSession {
    /// Plays a rendered sequence file to the controllers that know their sequence channels, from
    /// `position_ms` (paused there, with `paused`). Returns before the music is open: see
    /// [`PlaybackSession::take_ready`].
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        show: &Show,
        map: &ChannelMap,
        request: &PlayRequest,
        position_ms: u64,
        paused: bool,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
        clocks: &ClockFactory,
    ) -> Result<Self, EngineError> {
        let sequence = Sequence::open(&request.path).map_err(|e| match e {
            pf_fseq::FseqError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                EngineError::Playback(format!(
                    "{} isn't where it was. Use Find again or Locate… to show PixelFlow where it is now.",
                    pf_model::file_name_of(&pf_model::path_to_text(&request.path))
                ))
            }
            e => EngineError::Playback(e.to_string()),
        })?;
        let header = sequence.header().clone();
        let channels = header.channels as usize;
        let (routes, notes) = routes(show, channels);
        if routes.is_empty() {
            return Err(EngineError::Playback(
                "None of your controllers knows which sequence channels are theirs yet. Add them from \
                 your FPP's output list on the Controllers screen."
                    .to_string(),
            ));
        }
        let plan = pf_output::build_passthrough_plan(&routes, channels, send_rate(header.step_ms));
        let source = FileFrames {
            sequence,
            show: show.clone(),
            map: map.clone(),
        };
        let launch = Launch {
            plan,
            out_len: channels,
            preview_len: map.frame_len,
            music: request.music.clone(),
            offset_ms: request.offset_ms,
            volume: request.volume,
            looping: false,
        };
        let kind = SessionKind::File {
            request: request.clone(),
            routes,
            map: map.clone(),
            channels,
        };
        Self::launch(
            Box::new(source),
            launch,
            kind,
            request.path.clone(),
            notes,
            position_ms,
            paused,
            transport,
            settings,
            clocks,
        )
    }

    /// Plays an authored sequence, rendered live into the show frame and sent through the show's
    /// normal output plan (like a test pattern), from `position_ms` (paused there, with `paused`).
    /// When the show has errors, only the preview plays. Returns before the music is open: see
    /// [`PlaybackSession::take_ready`].
    #[allow(clippy::too_many_arguments)]
    pub fn start_document(
        show: &Show,
        map: &ChannelMap,
        request: DocumentRequest,
        position_ms: u64,
        paused: bool,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
        clocks: &ClockFactory,
    ) -> Result<Self, EngineError> {
        let DocumentRequest {
            doc,
            path,
            music,
            show_error,
            send,
            volume,
            looping,
        } = request;
        let (plan, notes) = document_plan(show, map, show_error.as_deref(), send, doc.frame_ms);
        let updates = Arc::new(LiveUpdates::default());
        let frame_ms = doc.frame_ms;
        let source = RenderedFrames {
            doc,
            renderer: Renderer::new(show, map),
            updates: Arc::clone(&updates),
        };
        let launch = Launch {
            plan,
            out_len: map.frame_len,
            preview_len: map.frame_len,
            music: music.clone(),
            offset_ms: 0,
            volume,
            looping,
        };
        let kind = SessionKind::Document {
            music,
            key: output_key(show, map),
            map: map.clone(),
            preview_only: show_error.is_some(),
            sending: send,
            frame_ms,
            updates,
        };
        Self::launch(
            Box::new(source),
            launch,
            kind,
            path.unwrap_or_default(),
            notes,
            position_ms,
            paused,
            transport,
            settings,
            clocks,
        )
    }

    /// Shows the first frame, starts the output, and starts the player thread on the clock.
    #[allow(clippy::too_many_arguments)]
    fn launch(
        mut source: Box<dyn FrameSource>,
        launch: Launch,
        kind: SessionKind,
        path: PathBuf,
        notes: Vec<String>,
        position_ms: u64,
        paused: bool,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
        clocks: &ClockFactory,
    ) -> Result<Self, EngineError> {
        let (frames, step_ms) = source.timing();
        let step_ms = u64::from(step_ms.max(1));
        let start_frame = u32::try_from(position_ms / step_ms)
            .unwrap_or(u32::MAX)
            .min(frames.saturating_sub(1));

        let (mut writer, reader) = pf_frame::frame_buffers(launch.out_len);
        let mut preview_frame = vec![0u8; launch.preview_len];
        // Publish the first frame before output starts so controllers never see a black frame first.
        if frames > 0 {
            source
                .frame(start_frame, writer.frame_mut())
                .map_err(EngineError::Playback)?;
            source.paint(writer.frame_mut(), &mut preview_frame);
        }
        let raw = Arc::new(Mutex::new(writer.frame_mut().to_vec()));
        writer.publish();
        let preview = Arc::new(Mutex::new(preview_frame));
        let handle = pf_output::start_output(launch.plan, settings, reader, transport);

        let control = Arc::new(Mutex::new(Control {
            paused,
            frame: start_frame,
            frames,
            frame_ms: step_ms as u32,
            offset_ms: launch.offset_ms,
            volume: launch.volume,
            looping: launch.looping,
            ..Control::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let player = {
            let frames = Frames {
                source,
                writer,
                preview: Arc::clone(&preview),
                raw: Arc::clone(&raw),
            };
            let (control, stop, clocks) = (Arc::clone(&control), Arc::clone(&stop), Arc::clone(clocks));
            let music = launch.music;
            let start_ms = u64::from(start_frame) * step_ms;
            std::thread::Builder::new()
                .name("pixelflow-playback".into())
                .spawn(move || {
                    run_player(
                        frames,
                        &control,
                        &stop,
                        &clocks,
                        music.as_deref(),
                        start_ms,
                        ready_tx,
                    );
                })
                .map_err(EngineError::Network)?
        };
        Ok(Self {
            kind,
            path,
            notes,
            control,
            stop,
            player: Some(player),
            ready: Some(ready_rx),
            handle: Some(handle),
            preview,
            raw,
        })
    }

    /// Something to wait on until the music is open (once; later calls wait for nothing).
    pub fn take_ready(&mut self) -> PlaybackReady {
        PlaybackReady(self.ready.take())
    }

    /// For a file: sends to new controllers or a new layout from the same place, without
    /// reopening the sequence or the music (they keep playing).
    pub fn rebuild(
        &mut self,
        show: &Show,
        map: &ChannelMap,
        routes: Vec<PassthroughRoute>,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
    ) {
        let frame_ms = lock(&self.control).frame_ms;
        let SessionKind::File {
            routes: built_routes,
            map: built_map,
            channels,
            ..
        } = &mut self.kind
        else {
            return;
        };
        let plan = pf_output::build_passthrough_plan(&routes, *channels, send_rate(frame_ms));
        let (mut writer, reader) = pf_frame::frame_buffers(*channels);
        // Start the new output on the frame showing now, not a black one.
        writer
            .frame_mut()
            .copy_from_slice(&self.raw.lock().unwrap_or_else(PoisonError::into_inner));
        writer.publish();
        let handle = pf_output::start_output(plan, settings, reader, transport);
        lock(&self.control).rebuild = Some(Rebuild {
            show: show.clone(),
            map: map.clone(),
            writer,
            renderer: None,
        });
        if let Some(old) = self.handle.replace(handle) {
            old.stop();
        }
        *built_routes = routes;
        *built_map = map.clone();
    }

    /// For an authored sequence: sends through the edited show's output plan (new wiring,
    /// addresses, or layout; errors appearing or fixed; a new frame time) from the same place,
    /// without reopening the music (it keeps playing). `send` false sends nothing (the preview
    /// alone).
    #[allow(clippy::too_many_arguments)]
    pub fn rebuild_document(
        &mut self,
        show: &Show,
        map: &ChannelMap,
        doc: &SequenceDoc,
        show_error: Option<&str>,
        send: bool,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
    ) {
        let SessionKind::Document {
            key,
            map: built_map,
            preview_only,
            sending,
            frame_ms,
            updates,
            ..
        } = &mut self.kind
        else {
            return;
        };
        // A renderer still waiting for the player (from a move just before) was made for the old
        // layout: drop it, or it would replace this rebuild's newer one.
        updates
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .renderer = None;
        let (plan, notes) = document_plan(show, map, show_error, send, doc.frame_ms);
        let mut renderer = Renderer::new(show, map);
        let (mut writer, reader) = pf_frame::frame_buffers(map.frame_len);
        // Start the new output on the moment showing now, not a black frame.
        let position_ms = {
            let c = lock(&self.control);
            u64::from(c.frame) * u64::from(c.frame_ms)
        };
        renderer.render(doc, position_ms, writer.frame_mut());
        writer.publish();
        let handle = pf_output::start_output(plan, settings, reader, transport);
        lock(&self.control).rebuild = Some(Rebuild {
            show: show.clone(),
            map: map.clone(),
            writer,
            renderer: Some(renderer),
        });
        if let Some(old) = self.handle.replace(handle) {
            old.stop();
        }
        *key = output_key(show, map);
        *built_map = map.clone();
        *preview_only = show_error.is_some();
        *sending = send;
        *frame_ms = doc.frame_ms;
        self.notes = notes;
    }

    pub fn set_paused(&self, paused: bool) {
        lock(&self.control).paused = paused;
    }

    /// Jumps to `position_ms` (clamped to the sequence).
    pub fn seek(&self, position_ms: u64) {
        let mut c = lock(&self.control);
        let frame_ms = c.frame_ms.max(1);
        let frame = u32::try_from(position_ms / u64::from(frame_ms)).unwrap_or(u32::MAX);
        let frame = frame.min(c.frames.saturating_sub(1));
        c.seek_to = Some(u64::from(frame) * u64::from(frame_ms));
        c.frame = frame;
        if c.error.is_none() {
            // Seeking after the end plays again (the player thread is still running).
            c.ended = false;
            c.lights_done = false;
        }
    }

    /// Shifts the lights against the music (positive: lights ahead), live.
    pub fn set_offset(&self, offset_ms: i32) {
        lock(&self.control).offset_ms = offset_ms;
    }

    /// Sets the music volume (0.0–1.0), live.
    pub fn set_volume(&self, volume: f32) {
        lock(&self.control).volume = volume.clamp(0.0, 1.0);
    }

    /// Plays again from the top each time the lights reach the end (the music jumps back with
    /// them), instead of ending. Turned on after the end, it doesn't start again by itself.
    pub fn set_looping(&self, looping: bool) {
        lock(&self.control).looping = looping;
    }

    /// What the session plays and what it was built from.
    pub fn kind(&self) -> &SessionKind {
        &self.kind
    }

    /// For a file: what this session plays, with the current offset and volume.
    pub fn request(&self) -> Option<PlayRequest> {
        let SessionKind::File { request, .. } = &self.kind else {
            return None;
        };
        let c = lock(&self.control);
        Some(PlayRequest {
            offset_ms: c.offset_ms,
            volume: c.volume,
            ..request.clone()
        })
    }

    /// For an authored sequence: shows the edited document from the next frame on.
    pub fn update_document(&self, doc: Arc<SequenceDoc>) {
        if let SessionKind::Document { updates, .. } = &self.kind {
            updates.send(|p| p.doc = Some(doc));
        }
    }

    /// For an authored sequence: draws with a renderer for the edited show (props moved, say)
    /// from the next frame on.
    pub fn update_renderer(&self, renderer: Renderer) {
        if let SessionKind::Document { updates, .. } = &self.kind {
            updates.send(|p| p.renderer = Some(renderer));
        }
    }

    pub fn status(&self) -> PlaybackStatus {
        let c = lock(&self.control);
        let stats = self.handle.as_ref().map(OutputHandle::stats).unwrap_or_default();
        // The player thread stopped without saying why: it crashed.
        let crashed = !c.ended && self.player.as_ref().is_some_and(JoinHandle::is_finished);
        let error = c.error.clone().or_else(|| crashed.then(|| CRASHED.to_string()));
        let duration_ms = u64::from(c.frames) * u64::from(c.frame_ms);
        let (sequence, music, authored) = match &self.kind {
            SessionKind::File { request, .. } => (request.sequence, request.music.clone(), false),
            SessionKind::Document { music, .. } => (None, music.clone(), true),
        };
        PlaybackStatus {
            state: if c.ended || crashed {
                "ended"
            } else if c.paused {
                "paused"
            } else {
                "playing"
            },
            path: self.path.clone(),
            position_ms: if (c.ended || c.lights_done) && error.is_none() {
                duration_ms
            } else {
                u64::from(c.frame) * u64::from(c.frame_ms)
            },
            duration_ms,
            frame_ms: c.frame_ms,
            controllers: controller_status(&stats),
            notes: self
                .notes
                .iter()
                .cloned()
                .chain(c.music_note.clone())
                .chain(c.clock_note.clone())
                .collect(),
            error,
            sequence,
            music: music.filter(|_| c.music_note.is_none()),
            offset_ms: c.offset_ms,
            volume: c.volume,
            authored,
            looping: c.looping,
        }
    }

    /// The props as they look in the current frame (show frame: prop order, RGB/RGBW per pixel).
    pub fn preview(&self) -> Vec<u8> {
        self.preview
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// For a file: the current sequence frame (every channel of the sequence, as sent).
    pub fn sequence_frame(&self) -> Option<Vec<u8>> {
        matches!(self.kind, SessionKind::File { .. })
            .then(|| self.raw.lock().unwrap_or_else(PoisonError::into_inner).clone())
    }

    /// Stops the player, then stops output (which blacks out the controllers).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(player) = self.player.take() {
            // The player notices within one poll. If it's stuck closing a stalled sound device,
            // leave it to finish by itself rather than freezing whoever is stopping it.
            let deadline = Instant::now() + JOIN_LIMIT;
            while !player.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if player.is_finished() {
                let _ = player.join();
            }
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

/// Resolves a document's music path: relative paths are relative to the document's folder.
pub(crate) fn document_music(doc_path: Option<&Path>, audio: Option<&str>) -> Option<PathBuf> {
    let audio = audio.filter(|a| !a.is_empty())?;
    if pf_model::is_full_path_text(audio) {
        return Some(pf_model::path_from_text(audio));
    }
    // Relative music is next to the document; an unsaved document has no folder yet, so its
    // relative music isn't looked for (not in whatever folder the app happens to run in).
    doc_path
        .and_then(Path::parent)
        .map(|dir| pf_model::path_from_text(&pf_model::resolve_text(audio, dir)))
}

/// A show entry for the sequence file at `path`: named after the file, with its music when it
/// can be found next to it (by the file name recorded in the sequence, or the sequence's own name).
pub fn sequence_entry_for(path: &Path) -> Result<pf_model::SequenceEntry, EngineError> {
    let sequence = Sequence::open(path).map_err(|e| EngineError::Playback(e.to_string()))?;
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Sequence".to_string());
    let mut entry = pf_model::SequenceEntry::new(name, pf_model::path_to_text(path));
    entry.audio =
        pf_audio::find_audio(path, sequence.header().media.as_deref()).map(|p| pf_model::path_to_text(&p));
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Protocol, SequenceChannels};

    fn controller(name: &str, channels: Option<(u32, u32)>) -> Controller {
        let mut c = Controller::new(name, "127.0.0.1", Protocol::Ddp);
        c.sequence_channels = channels.map(|(start, count)| SequenceChannels {
            start,
            count,
            raw_ddp_offsets: false,
        });
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

    #[test]
    fn raw_ddp_controllers_offset_their_packets_and_sacn_stops_at_universe_63999() {
        use pf_model::{SacnConfig, UniverseSize};
        let mut raw = controller("Raw", Some((6001, 300)));
        raw.sequence_channels.as_mut().unwrap().raw_ddp_offsets = true;
        let mut late = Controller::new(
            "Late",
            "127.0.0.2",
            Protocol::Sacn(SacnConfig {
                start_universe: Some(63_999),
                universe_size: UniverseSize::Channels512,
                ..SacnConfig::default()
            }),
        );
        late.sequence_channels = Some(SequenceChannels {
            start: 1,
            count: 1000,
            raw_ddp_offsets: false,
        });
        let mut show = Show::new("t");
        show.controllers = vec![raw, late];
        let (routes, notes) = routes(&show, 6400);
        assert_eq!((routes[0].start, routes[0].ddp_offset_base), (6000, 6000));
        assert_eq!((routes[1].count, routes[1].ddp_offset_base), (512, 0));
        assert!(notes[0].contains("past 63999"), "{notes:?}");
    }

    #[test]
    fn previews_only_read_a_controllers_own_block() {
        use pf_mapping::map_show;
        use pf_model::{Generator, Port, PortSlot, Prop, ShapeSource};
        let prop = Prop::new(
            "Strip",
            ShapeSource::Generator(Generator::Line {
                nodes: 4,
                length: 1.0,
            }),
        );
        let mut c = controller("Falcon", Some((1, 6))); // only 6 of the strip's 12 channels
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(prop.id));
        c.ports.push(port);
        let mut show = Show::new("t");
        show.props.push(prop);
        show.controllers.push(c);
        let (map, _) = map_show(&show);
        let sequence = vec![9u8; 30];
        let mut preview = vec![0u8; map.frame_len];
        paint_preview(&show, &map, &sequence, &mut preview);
        assert_eq!(preview, vec![9, 9, 9, 9, 9, 9, 0, 0, 0, 0, 0, 0]);
    }
}
