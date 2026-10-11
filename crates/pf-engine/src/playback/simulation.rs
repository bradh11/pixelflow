//! Playback's timing, checked without waiting for it.
//!
//! The player loop ([`run_player`]) runs here on the test's own thread against a made-up machine.
//! Time moves only when the player sleeps or draws; the sound device is a stopwatch on that time
//! with a real device's faults (its clock runs a little fast or slow, it stops calling back);
//! drawing a frame costs what the test says; and the user's clicks happen at the times the test
//! scripts. Nothing reads the machine's clock, so ten minutes of playback take a moment and every
//! run is the same as the last.
//!
//! These tests carry the guarantees about keeping in step: the frame showing follows the music
//! however long drawing takes, never slips further behind over a long run, and a jump, a pause, a
//! loop, a stop, and a stalled device each take effect at once. The tests that play in real time
//! (in `tests/`) only show that the same loop runs on a real thread.

use super::*;
use std::collections::VecDeque;
use std::ops::Range;

/// The frame time every simulation plays at.
const STEP_MS: u32 = 25;
const STEP: u64 = STEP_MS as u64;

/// The most a simulated sleep overruns: a real one always does, a little.
const OVERSLEEP: Duration = Duration::from_micros(1500);

/// How late the player may start a frame it had time for: one sleep's overrun, the millisecond
/// the lights' time is rounded down by, and the same again when a device running slow makes
/// the player wake a moment early and look once more.
const ON_TIME_MS: f64 = 3.6;

const fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Repeatable "random" numbers: every run of a test sees the same ones.
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `limit` (0 when there is nothing below it).
    fn below(&mut self, limit: u64) -> u64 {
        if limit == 0 { 0 } else { self.next() % limit }
    }

    fn between(&mut self, low: Duration, high: Duration) -> Duration {
        let spread = u64::try_from((high - low).as_nanos()).unwrap();
        low + Duration::from_nanos(self.below(spread + 1))
    }
}

/// How long drawing a frame takes.
#[derive(Debug, Clone, Copy)]
enum Draw {
    /// Anywhere between the two, differently for every frame.
    Between(Duration, Duration),
    /// The first, except that one frame in `one_in` takes the second.
    Spikes {
        usual: Duration,
        spike: Duration,
        one_in: u64,
    },
}

impl Draw {
    fn cost(self, random: &mut Random) -> Duration {
        match self {
            Draw::Between(low, high) => random.between(low, high),
            Draw::Spikes { usual, spike, one_in } => {
                if random.below(one_in) == 0 {
                    spike
                } else {
                    usual
                }
            }
        }
    }
}

/// What the player asked of the sound device.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Call {
    Start(Duration),
    Pause,
    Resume,
    Seek { from: Duration, to: Duration },
}

/// A made-up sound device: a stopwatch on the simulation's time.
struct Device {
    /// The music's position when it last started, paused, jumped, or changed speed.
    base: Duration,
    /// When it has been running since (`None`: paused).
    since: Option<Duration>,
    speed: f64,
    /// How fast its clock runs against the machine's: no sound card's is exact.
    rate: f64,
    /// While the time is inside one of these it stands still, as a device that stopped calling
    /// back does. They don't overlap.
    stalls: Vec<Range<Duration>>,
    /// How long the song is (`None`: there is no music, only a stopwatch).
    song: Option<Duration>,
    calls: Vec<(Duration, Call)>,
}

impl Device {
    /// How much of `from..to` it was running for.
    fn running(&self, from: Duration, to: Duration) -> Duration {
        let mut running = to.saturating_sub(from);
        for stall in &self.stalls {
            let stalled = stall.end.min(to).saturating_sub(stall.start.max(from));
            running = running.saturating_sub(stalled);
        }
        running
    }

    fn position(&self, now: Duration) -> Duration {
        let played = self
            .since
            .map_or(Duration::ZERO, |since| self.running(since, now));
        self.base + played.mul_f64(self.rate * self.speed)
    }

    /// Carries on from where it is, counting from `now`.
    fn rebase(&mut self, now: Duration) {
        self.base = self.position(now);
        if self.since.is_some() {
            self.since = Some(now);
        }
    }
}

/// The device as the player holds it.
struct SimClock(Arc<Mutex<World>>);

impl AudioClock for SimClock {
    fn start(&mut self, position: Duration) {
        let mut world = world(&self.0);
        let now = world.now;
        world.device.base = position;
        world.device.since = Some(now);
        world.device.calls.push((now, Call::Start(position)));
    }

    fn pause(&mut self) {
        let mut world = world(&self.0);
        let now = world.now;
        world.device.base = world.device.position(now);
        world.device.since = None;
        world.device.calls.push((now, Call::Pause));
    }

    fn resume(&mut self) {
        let mut world = world(&self.0);
        let now = world.now;
        if world.device.since.is_none() {
            world.device.since = Some(now);
        }
        world.device.calls.push((now, Call::Resume));
    }

    fn seek(&mut self, position: Duration) {
        let mut world = world(&self.0);
        let now = world.now;
        let from = world.device.position(now);
        world.device.base = position;
        if world.device.since.is_some() {
            world.device.since = Some(now);
        }
        world.device.calls.push((now, Call::Seek { from, to: position }));
    }

    fn position(&self) -> Duration {
        let world = world(&self.0);
        world.device.position(world.now)
    }

    fn set_volume(&mut self, _volume: f32) {}

    fn set_speed(&mut self, speed: f32) {
        let mut world = world(&self.0);
        let now = world.now;
        world.device.rebase(now);
        world.device.speed = f64::from(speed);
    }

    fn finished(&self) -> bool {
        let world = world(&self.0);
        world
            .device
            .song
            .is_none_or(|length| world.device.position(world.now) >= length)
    }
}

/// Something the user does while it plays.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Act {
    Seek(u64),
    Pause,
    Resume,
    Offset(i32),
    Speed(f32),
    Loop(bool),
    Stop,
    /// The machine does something else for this long: the player wakes that much late.
    Hiccup(Duration),
}

/// A frame the player drew and handed to the output.
#[derive(Debug, Clone, Copy)]
struct Shown {
    frame: u32,
    /// When drawing it began, and when it was handed over.
    began: Duration,
    sent: Duration,
    /// Where the device was at each.
    music_began: Duration,
    music_sent: Duration,
}

impl Shown {
    /// How far into the frame's own time the lights' time (music plus `offset_ms`) was when
    /// drawing began: under a frame, or the frame was shown late.
    fn began_ms(&self, offset_ms: i32) -> f64 {
        ms(self.music_began) + f64::from(offset_ms) - f64::from(self.frame) * f64::from(STEP_MS)
    }

    /// The same when it was handed over: how far the music had got past the frame's start.
    fn skew_ms(&self, offset_ms: i32) -> f64 {
        ms(self.music_sent) + f64::from(offset_ms) - f64::from(self.frame) * f64::from(STEP_MS)
    }
}

/// What the player made of one look at the clock.
#[derive(Debug, Clone)]
struct Tick {
    /// When it looked.
    at: Duration,
    /// The lights' time it worked out.
    light: u64,
    /// Where the device was then.
    music: Duration,
    lights_done: bool,
    note: Option<String>,
}

/// The made-up machine.
struct World {
    /// Time since the simulation began.
    now: Duration,
    device: Device,
    /// What the user does and when, in order.
    script: VecDeque<(Duration, Act)>,
    /// When each of those reached the player.
    acted: Vec<Duration>,
    /// When the run is over.
    end: Duration,
    draw: Draw,
    oversleep: Duration,
    random: Random,
    shown: Vec<Shown>,
    ticks: Vec<Tick>,
    sleeps: u64,
}

fn world(world: &Mutex<World>) -> std::sync::MutexGuard<'_, World> {
    world.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The simulation's time: sleeping moves it on, and is when the user's clicks arrive.
struct SimTime {
    /// Simulated time counts from here (an `Instant` can't be made from nothing).
    origin: Instant,
    world: Arc<Mutex<World>>,
    control: Arc<Mutex<Control>>,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for SimTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SimTime")
    }
}

impl SimTime {
    fn act(&self, act: Act) {
        match act {
            Act::Seek(position_ms) => lock(&self.control).seek(position_ms),
            Act::Pause => lock(&self.control).paused = true,
            Act::Resume => lock(&self.control).paused = false,
            Act::Offset(offset_ms) => lock(&self.control).offset_ms = offset_ms,
            Act::Speed(speed) => lock(&self.control).speed = speed,
            Act::Loop(looping) => lock(&self.control).looping = looping,
            Act::Stop => self.stop.store(true, Ordering::Relaxed),
            Act::Hiccup(length) => world(&self.world).now += length,
        }
    }
}

impl TimeSource for SimTime {
    fn now(&self) -> Instant {
        self.origin + world(&self.world).now
    }

    fn sleep(&self, duration: Duration) {
        // What the player made of this turn, before time moves on.
        let (reading, lights_done, note) = {
            let control = lock(&self.control);
            (control.reading, control.lights_done, control.clock_note.clone())
        };
        let due = {
            let mut world = world(&self.world);
            if let Some((light, read)) = reading {
                let at = read.saturating_duration_since(self.origin);
                let music = world.device.position(at);
                world.ticks.push(Tick {
                    at,
                    light,
                    music,
                    lights_done,
                    note,
                });
            }
            world.sleeps += 1;
            // A player that never lets time pass would spin here for ever.
            assert!(
                world.sleeps < 4_000 + world.end.as_millis() as u64 * 4,
                "the player keeps looking at the clock without waiting"
            );
            // A real sleep always overruns a little; one of no time at all returns at once.
            let overrun = if duration.is_zero() {
                Duration::ZERO
            } else {
                let most = world.oversleep;
                world.random.between(Duration::ZERO, most)
            };
            world.now += duration + overrun;
            let mut due = Vec::new();
            while world.script.front().is_some_and(|(when, _)| *when <= world.now) {
                let (_, act) = world.script.pop_front().unwrap();
                let now = world.now;
                world.acted.push(now);
                due.push(act);
            }
            due
        };
        for act in due {
            self.act(act);
        }
        let world = world(&self.world);
        if world.now >= world.end {
            self.stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Frames that cost time to draw and say when they were drawn.
struct SimFrames {
    world: Arc<Mutex<World>>,
    frames: u32,
}

impl FrameSource for SimFrames {
    fn timing(&mut self) -> (u32, u32) {
        (self.frames, STEP_MS)
    }

    fn frame(&mut self, index: u32, out: &mut [u8]) -> Result<(), String> {
        let mut world = world(&self.world);
        let world = &mut *world;
        let began = world.now;
        let music_began = world.device.position(began);
        world.now += world.draw.cost(&mut world.random);
        world.shown.push(Shown {
            frame: index,
            began,
            sent: world.now,
            music_began,
            music_sent: world.device.position(world.now),
        });
        out.copy_from_slice(&index.to_le_bytes());
        Ok(())
    }

    fn paint(&self, out: &[u8], preview: &mut [u8]) {
        preview.copy_from_slice(out);
    }

    fn relayout(&mut self, _show: Show, _map: ChannelMap, _renderer: Option<Renderer>) {}
}

/// A simulation to run: what plays, on what machine, and what the user does meanwhile.
struct Sim {
    frames: u32,
    start_ms: u64,
    paused: bool,
    offset_ms: i32,
    looping: bool,
    rate: f64,
    song: Option<Duration>,
    stalls: Vec<Range<Duration>>,
    draw: Draw,
    oversleep: Duration,
    seed: u64,
    script: Vec<(u64, Act)>,
}

/// What a simulation did.
struct Run {
    shown: Vec<Shown>,
    ticks: Vec<Tick>,
    calls: Vec<(Duration, Call)>,
    /// When each scripted act reached the player.
    acted: Vec<Duration>,
    /// When the player stopped.
    finished: Duration,
    /// Where the device was then.
    music: Duration,
    ended: bool,
    lights_done: bool,
    /// The frame the player says is showing.
    frame: u32,
}

impl Sim {
    /// `frames` frames with music longer than they are, drawn quickly on a machine whose sound
    /// device keeps perfect time.
    fn new(frames: u32) -> Self {
        Self {
            frames,
            start_ms: 0,
            paused: false,
            offset_ms: 0,
            looping: false,
            rate: 1.0,
            song: Some(Duration::from_secs(24 * 3600)),
            stalls: Vec::new(),
            draw: Draw::Between(Duration::from_micros(200), at(3)),
            oversleep: OVERSLEEP,
            seed: 1,
            script: Vec::new(),
        }
    }

    fn act(mut self, when_ms: u64, act: Act) -> Self {
        self.script.push((when_ms, act));
        self
    }

    /// Plays until `until_ms` of simulated time have passed (or a scripted stop).
    fn run(self, until_ms: u64) -> Run {
        let start_frame = u32::try_from(self.start_ms / STEP)
            .unwrap()
            .min(self.frames.saturating_sub(1));
        let control = Arc::new(Mutex::new(Control {
            paused: self.paused,
            frame: start_frame,
            frames: self.frames,
            frame_ms: STEP_MS,
            offset_ms: self.offset_ms,
            looping: self.looping,
            ..Control::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut script = self.script;
        script.sort_by_key(|&(when, _)| when);
        let shared = Arc::new(Mutex::new(World {
            now: Duration::ZERO,
            device: Device {
                base: Duration::ZERO,
                since: None,
                speed: 1.0,
                rate: self.rate,
                stalls: self.stalls,
                song: self.song,
                calls: Vec::new(),
            },
            script: script.into_iter().map(|(when, act)| (at(when), act)).collect(),
            acted: Vec::new(),
            end: at(until_ms),
            draw: self.draw,
            oversleep: self.oversleep,
            random: Random::new(self.seed),
            shown: Vec::new(),
            ticks: Vec::new(),
            sleeps: 0,
        }));
        let time: Arc<dyn TimeSource> = Arc::new(SimTime {
            origin: Instant::now(),
            world: Arc::clone(&shared),
            control: Arc::clone(&control),
            stop: Arc::clone(&stop),
        });
        let (writer, _reader) = pf_frame::frame_buffers(4);
        let frames = Frames {
            source: Box::new(SimFrames {
                world: Arc::clone(&shared),
                frames: self.frames,
            }),
            writer,
            preview: Arc::new(Mutex::new(vec![0; 4])),
            raw: Arc::new(Mutex::new(vec![0; 4])),
        };
        let device = Arc::clone(&shared);
        let clocks: ClockFactory =
            Arc::new(move |_| Ok(Box::new(SimClock(Arc::clone(&device))) as Box<dyn AudioClock>));
        let (ready, _waiting) = std::sync::mpsc::channel();
        run_player(
            frames,
            &control,
            &stop,
            &clocks,
            None,
            u64::from(start_frame) * STEP,
            ready,
            &time,
        );
        let control = lock(&control);
        let mut world = world(&shared);
        Run {
            shown: std::mem::take(&mut world.shown),
            ticks: std::mem::take(&mut world.ticks),
            calls: std::mem::take(&mut world.device.calls),
            acted: std::mem::take(&mut world.acted),
            finished: world.now,
            music: world.device.position(world.now),
            ended: control.ended,
            lights_done: control.lights_done,
            frame: control.frame,
        }
    }
}

impl Run {
    /// The frames shown, in the order they were shown.
    fn frames(&self) -> Vec<u32> {
        self.shown.iter().map(|s| s.frame).collect()
    }

    /// The frames whose drawing began in `from..to`.
    fn shown_in(&self, from: Duration, to: Duration) -> Vec<Shown> {
        self.shown
            .iter()
            .copied()
            .filter(|s| s.began >= from && s.began < to)
            .collect()
    }

    /// What the device was asked at exactly `when`.
    fn calls_at(&self, when: Duration) -> Vec<Call> {
        self.calls
            .iter()
            .filter(|(at, _)| *at == when)
            .map(|&(_, call)| call)
            .collect()
    }
}

/// Ten minutes of 25 ms frames.
const TEN_MINUTES: u32 = 24_000;

/// Plays ten minutes on a machine whose sound device runs at `rate`, drawing as `draw` says.
fn ten_minutes(rate: f64, draw: Draw, seed: u64) -> Run {
    let sim = Sim {
        rate,
        draw,
        seed,
        ..Sim::new(TEN_MINUTES)
    };
    // The lights end when the music reaches their end, whenever the machine's clock says that is.
    sim.run((600_000.0 / rate) as u64 + 1_000)
}

#[test]
fn the_frame_showing_follows_the_music_for_ten_minutes_and_never_slips_further_behind() {
    // Drawing takes most of a frame's time, differently every frame, and the sound device's
    // clock is off by a twentieth of a percent either way (a third of a second in ten minutes).
    let heavy = Draw::Between(at(8), at(21));
    for (rate, seed) in [(1.0, 1), (1.0005, 2), (0.9995, 3)] {
        let run = ten_minutes(rate, heavy, seed);
        // Every frame after the first (shown before the player starts) is drawn once, in order.
        assert_eq!(run.frames(), (1..TEN_MINUTES).collect::<Vec<_>>(), "rate {rate}");
        for shown in &run.shown {
            let began = shown.began_ms(0);
            assert!(
                (0.0..=ON_TIME_MS).contains(&began),
                "frame {} began {began} ms into its time (rate {rate})",
                shown.frame
            );
            // Handed over before the next frame was due: the lights are never a frame behind.
            let skew = shown.skew_ms(0);
            assert!(skew < 25.0, "frame {} went out {skew} ms in", shown.frame);
        }
        // Nothing builds up: the last minute is no further behind the music than the first.
        let mean = |frames: &[Shown]| frames.iter().map(|s| s.skew_ms(0)).sum::<f64>() / frames.len() as f64;
        let worst = |frames: &[Shown]| frames.iter().map(|s| s.skew_ms(0)).fold(0.0, f64::max);
        let (first, last) = (&run.shown[..2400], &run.shown[run.shown.len() - 2400..]);
        assert!(
            (mean(first) - mean(last)).abs() < 0.5,
            "{} ms at first, {} ms at last (rate {rate})",
            mean(first),
            mean(last)
        );
        assert!((worst(first) - worst(last)).abs() < 1.0, "rate {rate}");
        // It was the music the lights kept to, not the machine's clock: the two had parted by a
        // third of a second by the end.
        let apart = ms(run.finished) - ms(run.music);
        let expected = ms(run.finished) * (1.0 - rate);
        assert!((apart - expected).abs() < 1.0, "{apart} ms apart (rate {rate})");
        assert!(run.lights_done, "the lights reached their end with the music");
    }
}

#[test]
fn a_frame_that_takes_too_long_is_followed_by_the_frame_due_then_not_by_the_ones_missed() {
    // One frame in forty takes 60 ms to draw: more than two frames' time.
    let spikes = Draw::Spikes {
        usual: at(2),
        spike: at(60),
        one_in: 40,
    };
    let run = Sim {
        draw: spikes,
        seed: 7,
        ..Sim::new(TEN_MINUTES)
    }
    .run(601_000);
    let mut skipped = 0;
    for pair in run.shown.windows(2) {
        let (before, shown) = (pair[0], pair[1]);
        let began = shown.began_ms(0);
        if before.sent - before.began < at(20) {
            assert_eq!(shown.frame, before.frame + 1);
            assert!(
                (0.0..=ON_TIME_MS).contains(&began),
                "frame {}: {began}",
                shown.frame
            );
        } else {
            // The frame whose whole time went by is never shown, late: the player draws the one
            // due the moment the slow frame is done, and is back on time with the next.
            assert_eq!(shown.frame, before.frame + 2);
            assert_eq!(shown.began, before.sent);
            assert!((0.0..25.0).contains(&began), "frame {}: {began}", shown.frame);
            skipped += 1;
        }
    }
    assert!(skipped > 400, "{skipped} slow frames");
    assert_eq!(run.shown.len(), TEN_MINUTES as usize - 1 - skipped);
}

/// The music position the device is sent to for lights at `light_ms`, worked out the long way:
/// the lights run `offset_ms` ahead of the music, and from the very top the song plays from its
/// start. Negative: the lights play that long before the song starts.
fn music_wanted(light_ms: u64, offset_ms: i32) -> i64 {
    let music = light_ms as i64 - i64::from(offset_ms);
    if light_ms == 0 { music.min(0) } else { music }
}

#[test]
fn a_jump_shows_its_frame_at_once_and_plays_on_from_there() {
    const FRAMES: u32 = 2400;
    for seed in 0..40 {
        let offset_ms = [0, 0, -700, 350, 1210][seed as usize % 5];
        let mut random = Random::new(1000 + seed);
        let mut sim = Sim {
            offset_ms,
            seed,
            ..Sim::new(FRAMES)
        };
        let mut when = 0;
        let mut targets = Vec::new();
        for _ in 0..25 {
            when += 200 + random.below(1500);
            // Anywhere in the minute, sometimes past its end, sometimes the very top.
            let target = match random.below(8) {
                0 => 0,
                _ => random.below(63_000),
            };
            targets.push(target);
            sim = sim.act(when, Act::Seek(target));
        }
        let run = sim.run(when + 2_000);
        assert_eq!(run.acted.len(), targets.len());
        for (i, &target) in targets.iter().enumerate() {
            let said = format!("jump {i} to {target} ms (seed {seed}, offset {offset_ms})");
            let frame = u32::try_from(target / STEP).unwrap().min(FRAMES - 1);
            let acted = run.acted[i];
            let until = run.acted.get(i + 1).copied().unwrap_or(run.finished);
            let shown = run.shown_in(acted, until);
            // The frame asked for is drawn the moment the player hears of the jump.
            assert_eq!((shown[0].frame, shown[0].began), (frame, acted), "{said}");
            // The music is sent to the same place, less the offset; before its start, it waits
            // at the top while the lights play up to it.
            let music = music_wanted(u64::from(frame) * STEP, offset_ms);
            let to = at(u64::try_from(music.max(0)).unwrap());
            let sent = run.calls_at(acted);
            assert!(
                sent.iter()
                    .any(|call| matches!(call, Call::Seek { to: t, .. } if *t == to)),
                "{said}: {sent:?}"
            );
            // From there every frame follows in order, each on time by the music.
            for (k, later) in shown.iter().enumerate() {
                assert_eq!(later.frame, frame + k as u32, "{said}");
                if music >= 0 && k > 0 {
                    let began = later.began_ms(offset_ms);
                    assert!(
                        (0.0..=ON_TIME_MS).contains(&began),
                        "{said}: frame {} began {began}",
                        later.frame
                    );
                }
            }
        }
    }
}

#[test]
fn the_offset_puts_the_lights_ahead_of_the_music_and_a_negative_one_behind() {
    for offset_ms in [0, 500, -500, 1210, -35] {
        let run = Sim {
            offset_ms,
            ..Sim::new(400)
        }
        .run(13_000);
        let started = run
            .calls
            .iter()
            .find_map(|&(when, call)| (call == Call::Start(Duration::ZERO)).then_some(when))
            .unwrap();
        if offset_ms > 0 {
            // Lights ahead: they play their first frames before the song starts, which it does
            // when they have got as far ahead as the offset says.
            let lead = ms(started) - f64::from(offset_ms);
            assert!((0.0..12.0).contains(&lead), "the song started {lead} ms late");
            let before: Vec<u32> = run
                .shown_in(Duration::ZERO, started)
                .iter()
                .map(|s| s.frame)
                .collect();
            let expected: Vec<u32> = (1..=u32::try_from(offset_ms).unwrap() / STEP_MS).collect();
            assert_eq!(before, expected, "offset {offset_ms}");
        } else {
            // Lights behind (or level): the song plays from its start at once, and the lights
            // hold their first frame until the music is that far in.
            assert_eq!(started, Duration::ZERO);
            let first = run.shown[0];
            assert_eq!(first.frame, 1);
            let wanted = f64::from(STEP_MS as i32 - offset_ms);
            let music = ms(first.music_began);
            assert!((wanted..=wanted + ON_TIME_MS).contains(&music), "{music}");
        }
        // Whenever the song is playing, the lights are at the music's position plus the offset.
        for tick in run.ticks.iter().filter(|t| t.at > started && !t.lights_done) {
            let music = i64::try_from(tick.music.as_millis()).unwrap();
            let wanted = u64::try_from((music + i64::from(offset_ms)).max(0)).unwrap();
            assert_eq!(tick.light, wanted, "offset {offset_ms} at {:?}", tick.at);
        }
        // So the frame drawn is the one for where the music is, plus the offset: with the
        // music one second in, lights half a second ahead show the frame at 1.5 s.
        for shown in run.shown.iter().filter(|s| s.began > started) {
            let music = i64::try_from(shown.music_began.as_millis()).unwrap();
            assert_eq!(
                i64::from(shown.frame),
                (music + i64::from(offset_ms)) / 25,
                "offset {offset_ms}"
            );
        }
    }
}

#[test]
fn an_offset_changed_while_playing_moves_the_lights_at_once() {
    let run = Sim::new(4000)
        .act(2_000, Act::Offset(800))
        .act(4_000, Act::Offset(-300))
        .run(6_000);
    for (i, offset_ms) in [800, -300].into_iter().enumerate() {
        let acted = run.acted[i];
        let shown = run.shown_in(acted, acted + at(1))[0];
        let music = i64::try_from(shown.music_began.as_millis()).unwrap();
        assert_eq!(shown.began, acted);
        assert_eq!(i64::from(shown.frame), (music + offset_ms) / 25);
    }
    // The music itself is left alone: only the lights move.
    assert!(
        run.calls
            .iter()
            .all(|(_, call)| !matches!(call, Call::Seek { .. }))
    );
}

#[test]
fn a_pause_holds_the_frame_and_playing_again_carries_on_from_it() {
    const FRAMES: u32 = 2400;
    for seed in 0..20 {
        let mut random = Random::new(2000 + seed);
        let mut sim = Sim {
            seed,
            ..Sim::new(FRAMES)
        };
        let mut when = 0;
        for _ in 0..12 {
            when += 150 + random.below(2000);
            sim = sim.act(when, Act::Pause);
            when += 20 + random.below(3000);
            sim = sim.act(when, Act::Resume);
        }
        let run = sim.run(when + 1_000);
        // Across every pause the frames run on in order, none shown twice and none missed, each
        // on time by the music: playing again starts where the pause stopped.
        let last = run.shown.last().unwrap().frame;
        assert_eq!(run.frames(), (1..=last).collect::<Vec<_>>(), "seed {seed}");
        for shown in &run.shown {
            let began = shown.began_ms(0);
            assert!(
                (0.0..=ON_TIME_MS).contains(&began),
                "seed {seed}: frame {} began {began}",
                shown.frame
            );
        }
        for pause in run.acted.chunks(2) {
            let (paused, resumed) = (pause[0], pause[1]);
            assert!(run.calls_at(paused).contains(&Call::Pause), "seed {seed}");
            assert!(run.calls_at(resumed).contains(&Call::Resume), "seed {seed}");
            // Nothing is drawn while paused, but for a frame that fell due the moment it began.
            let during = run.shown_in(paused, resumed);
            assert!(
                during.iter().all(|s| s.began == paused),
                "seed {seed}: {during:?}"
            );
            // The music and the lights both stand still.
            let mut held = run.ticks.iter().filter(|t| t.at >= paused && t.at < resumed);
            let first = held.next().unwrap();
            assert!(
                held.all(|t| (t.music, t.light) == (first.music, first.light)),
                "seed {seed}"
            );
        }
        // All that time paused is time the music didn't play.
        let waited: Duration = run.acted.chunks(2).map(|pause| pause[1] - pause[0]).sum();
        let played = ms(run.finished) - ms(waited);
        assert!((ms(run.music) - played).abs() < 0.001, "seed {seed}");
    }
}

#[test]
fn a_jump_while_paused_shows_exactly_that_frame_and_waits() {
    for offset_ms in [0, 400, -400] {
        let run = Sim {
            offset_ms,
            ..Sim::new(2400)
        }
        .act(1_000, Act::Pause)
        .act(1_500, Act::Seek(30_010))
        .act(2_500, Act::Seek(110))
        .act(3_500, Act::Resume)
        .run(4_500);
        let (first, second, resumed) = (run.acted[1], run.acted[2], run.acted[3]);
        // Each jump draws its frame once, at once, and nothing else is drawn until play.
        let frames = |from, to| {
            run.shown_in(from, to)
                .iter()
                .map(|s| (s.frame, s.began))
                .collect::<Vec<_>>()
        };
        assert_eq!(frames(first, second), [(1200, first)], "offset {offset_ms}");
        assert_eq!(frames(second, resumed), [(4, second)], "offset {offset_ms}");
        let held = |from, to, light| {
            run.ticks
                .iter()
                .filter(|t| t.at > from && t.at < to)
                .all(|t| t.light == light)
        };
        assert!(held(first, second, 30_000), "offset {offset_ms}");
        assert!(held(second, resumed, 100), "offset {offset_ms}");
        // The music waits at the same place (at its top, when the lights are before its start).
        let to = at(u64::try_from((100 - i64::from(offset_ms)).max(0)).unwrap());
        assert!(
            run.calls_at(second)
                .iter()
                .any(|call| matches!(call, Call::Seek { to: t, .. } if *t == to)),
            "offset {offset_ms}"
        );
        // Playing again goes on from that frame.
        let after: Vec<u32> = run
            .shown_in(resumed, run.finished)
            .iter()
            .map(|s| s.frame)
            .collect();
        assert_eq!(after[..5], [5, 6, 7, 8, 9], "offset {offset_ms}");
    }
}

#[test]
fn a_loop_goes_back_to_the_top_with_the_music_every_time_round() {
    const FRAMES: u32 = 80;
    const LENGTH: f64 = 2000.0;
    // With music, and with none (a stopwatch).
    for song in [Some(Duration::from_secs(3600)), None] {
        let run = Sim {
            looping: true,
            song,
            draw: Draw::Between(at(1), at(20)),
            ..Sim::new(FRAMES)
        }
        .run(600_000);
        let jumps: Vec<(Duration, Duration)> = run
            .calls
            .iter()
            .filter_map(|&(when, call)| match call {
                Call::Seek { from, to } => {
                    assert_eq!(to, Duration::ZERO, "the music goes back to its very top");
                    Some((when, from))
                }
                _ => None,
            })
            .collect();
        assert!(jumps.len() >= 299, "{} times round", jumps.len());
        for (round, pair) in jumps.windows(2).enumerate() {
            let ((jumped, from), (next, _)) = (pair[0], pair[1]);
            // The music is taken back the moment the lights reach their end, and the first
            // frame is drawn in that same moment.
            let over = ms(from) - LENGTH;
            assert!(
                (0.0..=ON_TIME_MS).contains(&over),
                "round {round}: {over} ms past the end"
            );
            let shown = run.shown_in(jumped, next);
            assert_eq!(shown[0].began, jumped, "round {round}");
            // Every frame of the round is drawn once, in order, on time by the music.
            let frames: Vec<u32> = shown.iter().map(|s| s.frame).collect();
            assert_eq!(frames, (0..FRAMES).collect::<Vec<_>>(), "round {round}");
            for frame in &shown {
                let began = frame.began_ms(0);
                assert!((0.0..=ON_TIME_MS).contains(&began), "round {round}: {began}");
            }
            // So a round lasts the sequence's length and no more than that moment: nothing is
            // carried from one round into the next.
            let lasted = ms(next - jumped) - LENGTH;
            assert!(
                (0.0..=ON_TIME_MS).contains(&lasted),
                "round {round} lasted {lasted} ms too long"
            );
        }
        assert!(run.ticks.iter().all(|t| !t.lights_done), "a loop never goes dark");
        assert!(!run.ended);
    }
}

#[test]
fn a_loop_starts_again_together_even_when_the_player_wakes_late_at_the_end() {
    // The machine is busy for 90 ms just as the lights reach their end.
    let run = Sim {
        looping: true,
        ..Sim::new(80)
    }
    .act(1_990, Act::Hiccup(at(90)))
    .run(3_000);
    let (jumped, from) = run
        .calls
        .iter()
        .find_map(|&(when, call)| match call {
            Call::Seek { from, .. } => Some((when, from)),
            _ => None,
        })
        .unwrap();
    assert!(from >= at(2_080), "{from:?}");
    let shown = run.shown_in(jumped, run.finished);
    assert_eq!((shown[0].frame, shown[0].began), (0, jumped));
    assert!(shown.iter().all(|s| (0.0..=ON_TIME_MS).contains(&s.began_ms(0))));
}

#[test]
fn turning_looping_off_lets_the_sequence_end_this_time_round() {
    let run = Sim {
        looping: true,
        song: None,
        ..Sim::new(80)
    }
    .act(5_000, Act::Loop(false))
    .run(8_000);
    assert!(run.ended && run.lights_done);
    // Twice round in full, then the rest of the third.
    assert_eq!(run.frames().len(), 79 + 80 + 80);
    assert_eq!(run.shown.last().unwrap().frame, 79);
}

#[test]
fn stopping_stops_at_once() {
    let run = Sim::new(2400).act(3_217, Act::Stop).run(10_000);
    let stopped = run.acted[0];
    // The player notices within one sleep, and draws nothing more.
    assert!(stopped - at(3_217) <= POLL + OVERSLEEP, "{stopped:?}");
    assert_eq!(run.finished, stopped);
    assert!(run.shown.iter().all(|s| s.sent <= stopped));
    assert_eq!(run.shown.last().unwrap().frame, 128);
    assert_eq!(run.frame, 128);
}

#[test]
fn the_lights_end_dark_and_playback_ends_when_the_music_has_too() {
    // Music as long as the lights' ten seconds, shorter, longer, and none at all.
    for (song, ends_ms) in [
        (Some(at(10_000)), 10_000.0),
        (Some(at(4_000)), 10_000.0),
        (Some(at(13_000)), 13_000.0),
        (None, 10_000.0),
    ] {
        let run = Sim {
            song,
            ..Sim::new(400)
        }
        .run(15_000);
        assert_eq!(run.frames(), (1..400).collect::<Vec<_>>(), "{song:?}");
        assert!(run.ended && run.lights_done, "{song:?}");
        // The lights go dark at their end, and the music is stopped when both are done.
        let dark = run.ticks.iter().find(|t| t.lights_done).unwrap();
        assert!(
            (ms(dark.at) - 10_000.0).abs() <= ON_TIME_MS,
            "{song:?}: dark at {:?}",
            dark.at
        );
        let (stopped, _) = *run.calls.iter().rfind(|(_, call)| *call == Call::Pause).unwrap();
        let late = ms(stopped) - ends_ms;
        assert!(
            (0.0..=12.0).contains(&late),
            "{song:?}: stopped {late} ms after the end"
        );
    }
}

#[test]
fn a_jump_back_after_the_end_plays_again() {
    let run = Sim {
        song: None,
        ..Sim::new(400)
    }
    .act(12_000, Act::Seek(9_000))
    .run(14_000);
    let jumped = run.acted[0];
    let again: Vec<u32> = run
        .shown_in(jumped, run.finished)
        .iter()
        .map(|s| s.frame)
        .collect();
    assert_eq!(again, (360..400).collect::<Vec<_>>());
    assert!(run.ended, "and ends again");
}

#[test]
fn playing_slower_slows_the_lights_with_the_music() {
    let run = Sim::new(2400)
        .act(2_000, Act::Speed(0.5))
        .act(6_000, Act::Speed(1.0))
        .run(8_000);
    // 2 s as written, 4 s at half speed, 2 s as written: 6 s of the sequence.
    let last = run.shown.last().unwrap().frame;
    assert!((238..=240).contains(&last), "{last}");
    assert_eq!(run.frames(), (1..=last).collect::<Vec<_>>());
    for shown in &run.shown {
        let began = shown.began_ms(0);
        assert!(
            (0.0..=ON_TIME_MS).contains(&began),
            "frame {} began {began}",
            shown.frame
        );
    }
}

/// A minute of lights on a sound device that stops calling back five seconds in, until `back`.
fn stalling(back: Duration) -> Sim {
    Sim {
        stalls: vec![at(5_000)..back],
        ..Sim::new(2400)
    }
}

/// The ticks the player took in `from..to`.
fn ticks(run: &Run, from: Duration, to: Duration) -> impl Iterator<Item = &Tick> {
    run.ticks.iter().filter(move |t| t.at >= from && t.at < to)
}

#[test]
fn a_stalled_sound_device_neither_stops_the_lights_nor_lets_them_run_ahead() {
    let run = stalling(at(8_000)).run(12_000);
    for tick in &run.ticks {
        // Never ahead of where the music would be by now had it never stopped.
        assert!(
            tick.light <= tick.at.as_millis() as u64,
            "{} ms at {:?}",
            tick.light,
            tick.at
        );
    }
    // For a second the lights wait for the music: it may only have hiccuped.
    for tick in ticks(&run, at(5_020), at(5_990)) {
        assert_eq!((tick.light, &tick.note), (5_000, &None), "at {:?}", tick.at);
    }
    // Then they keep time on their own, as if the music had played on, and say so.
    for tick in ticks(&run, at(6_020), at(8_000)) {
        let behind = tick.at.as_millis() as u64 - tick.light;
        assert!(behind <= 12, "{behind} ms behind at {:?}", tick.at);
        assert_eq!(tick.note.as_deref(), Some(STALL_NOTE), "at {:?}", tick.at);
    }
    // When the device comes back it plays on from where it stopped, and the lights go back to
    // the music at once.
    for tick in ticks(&run, at(8_020), run.finished) {
        assert_eq!(tick.light, tick.music.as_millis() as u64, "at {:?}", tick.at);
        assert_eq!(tick.note, None, "at {:?}", tick.at);
    }
    let back = run.shown_in(at(8_000), run.finished);
    assert!((200..=201).contains(&back[0].frame), "{}", back[0].frame);
    assert!(
        back.iter()
            .skip(1)
            .all(|s| (0.0..=ON_TIME_MS).contains(&s.began_ms(0)))
    );
}
