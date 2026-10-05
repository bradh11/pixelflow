//! Playing a rendered sequence (`.fseq`) to the show's controllers, with a preview of the props.
//!
//! Each controller that knows where its data sits in a sequence ([`SequenceChannels`]) receives
//! that block of every frame unchanged, the way FPP sends it. The preview maps the same channels
//! back through the show's wiring onto the props.
//!
//! The lights follow the music: the frame due is the music's position plus the sequence's offset.

use crate::error::EngineError;
use crate::output::{ControllerStatus, controller_status};
use pf_audio::{AudioClock, AudioError, MusicPlayer, SilentClock};
use pf_frame::FrameWriter;
use pf_fseq::Sequence;
use pf_mapping::ChannelMap;
use pf_model::{Protocol, SequenceId, Show};
use pf_output::{OutputHandle, OutputSettings, PassthroughRoute, Transport, wire_order};
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
}

/// New controllers and layout to send to, after an edit to the show.
struct Rebuild {
    show: Show,
    map: ChannelMap,
    writer: FrameWriter,
}

/// What the player thread and the engine share.
struct Control {
    paused: bool,
    /// Where to jump to, in sequence (light) time.
    seek_to: Option<u64>,
    frame: u32,
    /// Both the lights and the music are done (or a read error stopped the lights).
    ended: bool,
    /// The lights are past their end (dark) while the music plays on.
    lights_done: bool,
    error: Option<String>,
    /// How far the lights run ahead of the music.
    offset_ms: i32,
    volume: f32,
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
            ended: false,
            lights_done: false,
            error: None,
            offset_ms: 0,
            volume: 1.0,
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

/// What the player thread draws with and writes to.
struct Frames {
    sequence: Sequence,
    writer: FrameWriter,
    show: Show,
    map: ChannelMap,
    preview: Arc<Mutex<Vec<u8>>>,
    raw: Arc<Mutex<Vec<u8>>>,
}

impl Frames {
    /// Reads `frame` and sends it, updating the preview. On a read error it goes dark and returns
    /// the error.
    fn show(&mut self, frame: u32) -> Result<(), String> {
        if let Err(error) = self.sequence.read_frame(frame, self.writer.frame_mut()) {
            self.dark();
            return Err(error.to_string());
        }
        paint_preview(
            &self.show,
            &self.map,
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
        *self.preview.lock().unwrap_or_else(PoisonError::into_inner) = vec![0; rebuild.map.frame_len];
        self.show = rebuild.show;
        self.map = rebuild.map;
        self.writer = rebuild.writer;
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
    let step_ms = u64::from(frames.sequence.header().step_ms.max(1));
    let total = frames.sequence.header().frames;
    let mut shown = Some(u32::try_from(start_ms / step_ms).unwrap_or(u32::MAX));
    let mut dark = false;
    let mut applied_volume = volume;
    let mut note: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        let (paused, seek, offset, volume, ended, rebuild) = {
            let mut c = lock(control);
            (
                c.paused,
                c.seek_to.take(),
                c.offset_ms,
                c.volume,
                c.ended,
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
        let light = match seek {
            // A jump while paused shows exactly the frame asked for.
            Some(target) if paused => target,
            _ => light_for(music_ms, offset),
        };
        let due = light / step_ms;
        if due >= u64::from(total) {
            if !dark {
                frames.dark();
                dark = true;
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
        if shown != Some(due) {
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

/// A sequence playing: a player thread reading frames on time and the output thread sending them.
pub(crate) struct PlaybackSession {
    request: PlayRequest,
    path: PathBuf,
    /// What the session was built from: when an edit changes either, it must restart.
    routes: Vec<PassthroughRoute>,
    map: ChannelMap,
    channels: usize,
    frames: u32,
    frame_ms: u32,
    notes: Vec<String>,
    control: Arc<Mutex<Control>>,
    stop: Arc<AtomicBool>,
    player: Option<JoinHandle<()>>,
    ready: Option<Receiver<()>>,
    handle: Option<OutputHandle>,
    preview: Arc<Mutex<Vec<u8>>>,
    /// The current sequence frame, as sent.
    raw: Arc<Mutex<Vec<u8>>>,
}

impl PlaybackSession {
    /// Starts playing from `position_ms` (paused there, with `paused`). Returns before the music
    /// is open: see [`PlaybackSession::take_ready`].
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
        let path = request.path.as_path();
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
        let plan = pf_output::build_passthrough_plan(&routes, channels, send_rate(header.step_ms));
        let step_ms = u64::from(header.step_ms.max(1));
        let start_frame = u32::try_from(position_ms / step_ms)
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
            paused,
            frame: start_frame,
            offset_ms: request.offset_ms,
            volume: request.volume,
            ..Control::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let player = {
            let frames = Frames {
                sequence,
                writer,
                show: show.clone(),
                map: map.clone(),
                preview: Arc::clone(&preview),
                raw: Arc::clone(&raw),
            };
            let (control, stop, clocks) = (Arc::clone(&control), Arc::clone(&stop), Arc::clone(clocks));
            let music = request.music.clone();
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
            request: request.clone(),
            path: path.to_path_buf(),
            routes,
            map: map.clone(),
            channels,
            frames: header.frames,
            frame_ms: header.step_ms,
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

    /// Sends to new controllers or a new layout from the same place, without reopening the
    /// sequence or the music (they keep playing).
    pub fn rebuild(
        &mut self,
        show: &Show,
        map: &ChannelMap,
        routes: Vec<PassthroughRoute>,
        transport: Box<dyn Transport>,
        settings: OutputSettings,
    ) {
        let plan = pf_output::build_passthrough_plan(&routes, self.channels, send_rate(self.frame_ms));
        let (mut writer, reader) = pf_frame::frame_buffers(self.channels);
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
        });
        if let Some(old) = self.handle.replace(handle) {
            old.stop();
        }
        self.routes = routes;
        self.map = map.clone();
    }

    pub fn set_paused(&self, paused: bool) {
        lock(&self.control).paused = paused;
    }

    /// Jumps to `position_ms` (clamped to the sequence).
    pub fn seek(&self, position_ms: u64) {
        let frame = u32::try_from(position_ms / u64::from(self.frame_ms.max(1))).unwrap_or(u32::MAX);
        let frame = frame.min(self.frames.saturating_sub(1));
        let mut c = lock(&self.control);
        c.seek_to = Some(u64::from(frame) * u64::from(self.frame_ms));
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

    /// What this session plays, with the current offset and volume.
    pub fn request(&self) -> PlayRequest {
        let c = lock(&self.control);
        PlayRequest {
            offset_ms: c.offset_ms,
            volume: c.volume,
            ..self.request.clone()
        }
    }

    /// The controller blocks and channel layout this session was built from.
    pub fn built_from(&self) -> (&[PassthroughRoute], &ChannelMap) {
        (&self.routes, &self.map)
    }

    /// Channels in each frame of the sequence.
    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn status(&self) -> PlaybackStatus {
        let c = lock(&self.control);
        let stats = self.handle.as_ref().map(OutputHandle::stats).unwrap_or_default();
        // The player thread stopped without saying why: it crashed.
        let crashed = !c.ended && self.player.as_ref().is_some_and(JoinHandle::is_finished);
        let error = c.error.clone().or_else(|| crashed.then(|| CRASHED.to_string()));
        let duration_ms = u64::from(self.frames) * u64::from(self.frame_ms);
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
                u64::from(c.frame) * u64::from(self.frame_ms)
            },
            duration_ms,
            frame_ms: self.frame_ms,
            controllers: controller_status(&stats),
            notes: self
                .notes
                .iter()
                .cloned()
                .chain(c.music_note.clone())
                .chain(c.clock_note.clone())
                .collect(),
            error,
            sequence: self.request.sequence,
            music: self.request.music.clone().filter(|_| c.music_note.is_none()),
            offset_ms: c.offset_ms,
            volume: c.volume,
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

/// A show entry for the sequence file at `path`: named after the file, with its music when it
/// can be found next to it (by the file name recorded in the sequence, or the sequence's own name).
pub fn sequence_entry_for(path: &Path) -> Result<pf_model::SequenceEntry, EngineError> {
    let sequence = Sequence::open(path).map_err(|e| EngineError::Playback(e.to_string()))?;
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Sequence".to_string());
    let mut entry = pf_model::SequenceEntry::new(name, path.display().to_string());
    entry.audio =
        pf_audio::find_audio(path, sequence.header().media.as_deref()).map(|p| p.display().to_string());
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
