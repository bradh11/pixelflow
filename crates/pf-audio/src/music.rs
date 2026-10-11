//! Playing a music file as the clock sequence playback follows.
//!
//! Nothing here waits on the sound device: jumps are handed to the audio thread, which applies
//! them before its next sample, and the play position is what the audio thread has handed to the
//! output (smoothed between its buffers). A stalled or unplugged device can't freeze the caller.
//! It can play slower than written (for timing marks by ear): the samples are stretched, so the
//! pitch drops with the speed.

use crate::clock::AudioClock;
use crate::decode::{next_sample, open_decoder};
use crate::error::AudioError;
use rodio::cpal::{BufferSize, StreamError};
use rodio::stream::DeviceSinkConfig;
use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, Sample, SampleRate, Source};
use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Frames per output buffer to ask the sound device for: small, so the clock moves in small steps
/// (the device's default is about 50 ms).
const BUFFER_FRAMES: u32 = 512;
/// Output delay assumed when the device doesn't say how big its buffers are.
const DEFAULT_LATENCY: Duration = Duration::from_millis(20);

/// What the caller and the audio thread share.
#[derive(Debug, Default)]
struct Shared {
    /// A jump the audio thread hasn't made yet: where to, and its number.
    seek: Mutex<Option<(Duration, u64)>>,
    /// The number of the last jump the audio thread made (or gave up on).
    applied: AtomicU64,
    /// Music time of the next sample the output will get, in nanoseconds.
    position_ns: AtomicU64,
    /// The music has played to its end (silence follows).
    finished: AtomicBool,
    /// Why the last jump failed.
    seek_error: Mutex<Option<String>>,
    /// The sound device went away (unplugged, or the system stopped the stream).
    device_lost: AtomicBool,
    /// How fast the music plays (an `f32`'s bits; 0 stands for full speed).
    speed: AtomicU32,
}

impl Shared {
    fn position(&self) -> Duration {
        Duration::from_nanos(self.position_ns.load(Ordering::Acquire))
    }

    fn speed(&self) -> f64 {
        let speed = f32::from_bits(self.speed.load(Ordering::Acquire));
        if speed > 0.0 { f64::from(speed) } else { 1.0 }
    }
}

/// The slowest the music plays (a quarter of its speed); it never plays faster than written.
pub const SLOWEST: f32 = 0.25;

fn nanos(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// The music as the output reads it. It makes jumps on the audio thread, counts what it has handed
/// over, and after the end of the song carries on with silence, so the clock keeps time (lights
/// longer than the song still finish) and a jump back plays the music again. Slowed down, each
/// frame it hands over lies between two of the song's, a little further on each time.
///
/// The channel count and sample rate are read once at the start: music files keep them for their
/// whole length.
struct MusicSource<S> {
    inner: S,
    shared: Arc<Shared>,
    channels: ChannelCount,
    rate: SampleRate,
    /// Where in the current frame the next sample falls (0 = first channel).
    channel: u16,
    /// Position of the last jump, and how many of the song's frames have played since (with a
    /// fraction of one while slowed down).
    base: Duration,
    frames: f64,
    ended: bool,
    /// The frame being handed over.
    out: Vec<Sample>,
    /// Slowed down: the song's frames either side of where it is, and how far between them (0–1).
    slow: Option<(Vec<Sample>, Vec<Sample>, f64)>,
}

impl<S: Source> MusicSource<S> {
    fn new(inner: S, shared: Arc<Shared>) -> Self {
        let channels = inner.channels();
        Self {
            channels,
            rate: inner.sample_rate(),
            inner,
            shared,
            channel: 0,
            base: Duration::ZERO,
            frames: 0.0,
            ended: false,
            out: vec![0.0; usize::from(channels.get())],
            slow: None,
        }
    }

    fn played(&self) -> Duration {
        self.base + Duration::from_secs_f64(self.frames / f64::from(self.rate.get()))
    }

    /// The song's next frame into `frame` (silence after its end).
    fn read_frame(&mut self, frame: &mut [Sample]) {
        for sample in frame {
            // This runs on the sound output's own thread: a decoder that panics part way
            // through a damaged file ends the music there, as the end of the file does.
            *sample = if self.ended {
                0.0
            } else if let Some(s) = next_sample(&mut self.inner) {
                s
            } else {
                self.ended = true;
                self.shared.finished.store(true, Ordering::Release);
                0.0
            };
        }
    }

    /// Works out the next frame to hand over, `speed` of the song's frames on from the last.
    fn next_frame(&mut self, speed: f64) {
        let mut out = std::mem::take(&mut self.out);
        if speed >= 1.0 {
            if let Some((_, ahead, _)) = self.slow.take() {
                // Back to full speed: the frame ahead, then straight on from there.
                out.copy_from_slice(&ahead);
                self.frames = self.frames.floor() + 2.0;
            } else {
                self.read_frame(&mut out);
                self.frames += 1.0;
            }
        } else {
            let (mut from, mut to, mut along) = match self.slow.take() {
                Some(slow) => slow,
                None => {
                    let mut from = vec![0.0; out.len()];
                    let mut to = vec![0.0; out.len()];
                    self.read_frame(&mut from);
                    self.read_frame(&mut to);
                    (from, to, 0.0)
                }
            };
            let t = along as f32;
            for ((o, a), b) in out.iter_mut().zip(&from).zip(&to) {
                *o = a + (b - a) * t;
            }
            along += speed;
            self.frames += speed;
            while along >= 1.0 {
                std::mem::swap(&mut from, &mut to);
                self.read_frame(&mut to);
                along -= 1.0;
            }
            self.slow = Some((from, to, along));
        }
        self.out = out;
    }

    /// Makes a requested jump. Never blocks: if the caller is handing one over right now, the
    /// next frame picks it up.
    fn jump_if_asked(&mut self) {
        let request = match self.shared.seek.try_lock() {
            Ok(mut seek) => seek.take(),
            Err(_) => None,
        };
        let Some((target, number)) = request else {
            return;
        };
        let inner = &mut self.inner;
        let jumped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.try_seek(target).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|_| Err("the music file is damaged".to_string()));
        match jumped {
            Ok(()) => {
                self.base = target;
                self.frames = 0.0;
                self.slow = None;
                self.ended = false;
                self.shared.finished.store(false, Ordering::Release);
                *self
                    .shared
                    .seek_error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = None;
            }
            Err(error) => {
                *self
                    .shared
                    .seek_error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(error);
            }
        }
        self.shared
            .position_ns
            .store(nanos(self.played()), Ordering::Release);
        self.shared.applied.store(number, Ordering::Release);
    }
}

impl<S: Source> Iterator for MusicSource<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.channel == 0 {
            self.jump_if_asked();
            self.shared
                .position_ns
                .store(nanos(self.played()), Ordering::Release);
            let speed = self.shared.speed();
            self.next_frame(speed);
        }
        let sample = self.out[usize::from(self.channel)];
        self.channel += 1;
        if self.channel >= self.channels.get() {
            self.channel = 0;
        }
        Some(sample)
    }
}

impl<S: Source> Source for MusicSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Where the music is being heard: `raw` is what the output has been handed, `since` how long ago
/// that last changed. The device plays a buffer about one buffer (`latency`) after taking it, and
/// takes a whole buffer at once, so the heard position is a buffer behind and moves smoothly
/// across it.
fn heard(raw: Duration, since: Duration, latency: Duration) -> Duration {
    raw.saturating_sub(latency) + since.min(latency)
}

/// How long the device's buffers last, from how it was opened.
fn buffer_time(config: &DeviceSinkConfig) -> Duration {
    match config.buffer_size() {
        BufferSize::Fixed(frames) => {
            Duration::from_secs_f64(f64::from(*frames) / f64::from(config.sample_rate().get()))
        }
        BufferSize::Default => DEFAULT_LATENCY,
    }
}

/// How the sound output was opened: what it says about its delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputInfo {
    pub sample_rate: u32,
    /// Frames per buffer, when the output was opened with a fixed size.
    pub buffer_frames: Option<u32>,
    /// About how long the output takes to play what it's handed: one buffer. The sound
    /// system's own delay after that (the device, a Bluetooth link) isn't reported.
    pub latency: Duration,
}

/// Plays a music file on the default sound output; its play position is the clock.
pub struct MusicPlayer {
    player: Player,
    shared: Arc<Shared>,
    /// About how long the output takes to play what it's handed.
    latency: Duration,
    output: Option<OutputInfo>,
    /// The last jump asked for: where to, and its number.
    pending: Duration,
    requested: u64,
    playing: bool,
    /// The last position the output reported, and when it changed.
    smooth: Cell<(Duration, Instant)>,
    /// The furthest position read since the jump with this number.
    furthest: Cell<(u64, Duration)>,
    // Keeps the output device open for as long as the player lives.
    _output: Option<MixerDeviceSink>,
}

impl MusicPlayer {
    /// Opens `path` on the default output, paused at the start.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        Self::open_source(open_decoder(path)?.0)
    }

    /// A metronome clicking every `every` (the first click at the start) on the default output,
    /// paused at the start: its position says exactly when each click is heard.
    pub fn metronome(every: Duration) -> Result<Self, AudioError> {
        Self::open_source(crate::metronome::Metronome::new(every))
    }

    /// How the sound output was opened (none without one).
    pub fn output(&self) -> Option<OutputInfo> {
        self.output
    }

    fn open_source<S: Source + Send + 'static>(inner: S) -> Result<Self, AudioError> {
        let shared = Arc::new(Shared::default());
        let source = MusicSource::new(inner, Arc::clone(&shared));
        let lost = Arc::clone(&shared);
        let on_error = move |error: StreamError| {
            if matches!(
                error,
                StreamError::DeviceNotAvailable | StreamError::StreamInvalidated
            ) {
                lost.device_lost.store(true, Ordering::Release);
            }
        };
        let mut output = DeviceSinkBuilder::from_default_device()
            .and_then(|b| {
                b.with_buffer_size(BufferSize::Fixed(BUFFER_FRAMES))
                    .with_error_callback(on_error.clone())
                    .open_stream()
            })
            .or_else(|_| {
                DeviceSinkBuilder::from_default_device()
                    .and_then(|b| b.with_error_callback(on_error).open_sink_or_fallback())
            })
            .or_else(|_| DeviceSinkBuilder::open_default_sink())
            .map_err(|e| AudioError::NoOutput(e.to_string()))?;
        output.log_on_drop(false);
        let config = output.config();
        let latency = buffer_time(config);
        let info = OutputInfo {
            sample_rate: config.sample_rate().get(),
            buffer_frames: match config.buffer_size() {
                BufferSize::Fixed(frames) => Some(*frames),
                BufferSize::Default => None,
            },
            latency,
        };
        let player = Player::connect_new(output.mixer());
        let mut music = Self::with_player(player, source, shared, latency, Some(output));
        music.output = Some(info);
        Ok(music)
    }

    fn with_player<S: Source + Send + 'static>(
        player: Player,
        source: MusicSource<S>,
        shared: Arc<Shared>,
        latency: Duration,
        output: Option<MixerDeviceSink>,
    ) -> Self {
        player.pause();
        player.append(source);
        Self {
            player,
            shared,
            latency,
            output: None,
            pending: Duration::ZERO,
            requested: 0,
            playing: false,
            smooth: Cell::new((Duration::ZERO, Instant::now())),
            furthest: Cell::new((0, Duration::ZERO)),
            _output: output,
        }
    }

    /// Whether a jump is still waiting for the audio thread.
    fn jumping(&self) -> bool {
        self.shared.applied.load(Ordering::Acquire) < self.requested
    }

    /// Whether the last jump the audio thread tried didn't work (the music played on instead).
    fn jump_failed(&self) -> bool {
        self.shared
            .seek_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

impl AudioClock for MusicPlayer {
    fn start(&mut self, position: Duration) {
        self.seek(position);
        self.resume();
    }

    fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
    }

    fn resume(&mut self) {
        self.player.play();
        self.playing = true;
    }

    fn seek(&mut self, position: Duration) {
        self.requested += 1;
        self.pending = position;
        *self.shared.seek.lock().unwrap_or_else(PoisonError::into_inner) = Some((position, self.requested));
    }

    fn position(&self) -> Duration {
        if self.jumping() {
            return self.pending;
        }
        let raw = self.shared.position();
        let now = Instant::now();
        let (last, changed) = self.smooth.get();
        let changed = if raw == last {
            changed
        } else {
            self.smooth.set((raw, now));
            now
        };
        let position = if self.playing {
            // The output's delay is in wall time; slowed down, less of the song fits in it.
            let speed = self.shared.speed();
            heard(raw, (now - changed).mul_f64(speed), self.latency.mul_f64(speed))
        } else {
            raw
        };
        // The music only moves forward between jumps, but the heard position is an estimate that
        // can step back by part of a buffer: just after a jump (read while the output is still
        // taking its first buffer from there), after a pause, or when the speed goes up. Lights
        // following it would flash the frame before, so a reading never falls behind an earlier
        // one, or behind the target of the jump it follows.
        let jump = self.shared.applied.load(Ordering::Acquire);
        let (since_jump, furthest) = self.furthest.get();
        let floor = if since_jump == jump {
            furthest
        } else if self.jump_failed() {
            Duration::ZERO
        } else {
            self.pending
        };
        let position = position.max(floor);
        self.furthest.set((jump, position));
        position
    }

    fn set_volume(&mut self, volume: f32) {
        self.player.set_volume(volume.clamp(0.0, 1.0));
    }

    fn set_speed(&mut self, speed: f32) {
        let speed = if speed.is_finite() {
            speed.clamp(SLOWEST, 1.0)
        } else {
            1.0
        };
        self.shared.speed.store(speed.to_bits(), Ordering::Release);
    }

    fn finished(&self) -> bool {
        !self.jumping() && self.shared.finished.load(Ordering::Acquire)
    }

    fn problem(&self) -> Option<String> {
        if self.shared.device_lost.load(Ordering::Acquire) {
            return Some("The sound output went away, so the music stopped. The lights keep playing.".into());
        }
        self.jump_failed().then(|| {
            "PixelFlow couldn't jump to that part of the music, so it plays on from where it was.".into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::queue::SourcesQueueOutput;

    const RATE: u32 = 8000;
    const SECONDS: u32 = 2;

    /// A mono 16-bit WAV whose samples rise steadily from 0 to 0.9 over two seconds, so a sample's
    /// value says where in the file it came from.
    fn write_ramp(path: &Path) {
        let total = RATE * SECONDS;
        let data: Vec<u8> = (0..total)
            .flat_map(|i| (((i as f32 / total as f32) * 0.9 * f32::from(i16::MAX)) as i16).to_le_bytes())
            .collect();
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend(data);
        std::fs::write(path, out).unwrap();
    }

    /// Where in the ramp file a sample came from, in milliseconds.
    fn ramp_ms(sample: Sample) -> f32 {
        sample / 0.9 * (SECONDS * 1000) as f32
    }

    /// A player on no sound device: the test pulls samples the way a device would.
    fn player_without_output() -> (MusicPlayer, SourcesQueueOutput, tempfile::TempDir) {
        player_with_buffer(Duration::ZERO)
    }

    /// The same, for a device that plays what it's handed `buffer` later.
    fn player_with_buffer(buffer: Duration) -> (MusicPlayer, SourcesQueueOutput, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ramp.wav");
        write_ramp(&path);
        let shared = Arc::new(Shared::default());
        let source = MusicSource::new(open_decoder(&path).unwrap().0, Arc::clone(&shared));
        let (player, output) = Player::new();
        let music = MusicPlayer::with_player(player, source, shared, buffer, None);
        (music, output, dir)
    }

    /// Plays `ms` of sound out of the output; returns the last sample.
    fn pull(output: &mut SourcesQueueOutput, ms: u32) -> Sample {
        (0..RATE * ms / 1000)
            .map(|_| output.next().unwrap())
            .last()
            .unwrap()
    }

    fn assert_near(actual: Duration, expected_ms: u64) {
        let ms = actual.as_secs_f64() * 1000.0;
        assert!(
            (ms - expected_ms as f64).abs() < 12.0,
            "{ms} ms, expected about {expected_ms}"
        );
    }

    #[test]
    fn jumps_back_in_a_real_file() {
        let (mut music, mut output, _dir) = player_without_output();
        music.start(Duration::ZERO);
        pull(&mut output, 1000);
        assert_near(music.position(), 1000);
        music.seek(Duration::from_millis(250));
        assert_eq!(
            music.position(),
            Duration::from_millis(250),
            "reported right away"
        );
        let sample = pull(&mut output, 100);
        assert_near(music.position(), 350);
        let heard = ramp_ms(sample);
        assert!(
            (heard - 350.0).abs() < 12.0,
            "the music itself went back: {heard} ms"
        );
        assert_eq!(music.problem(), None);
    }

    #[test]
    fn keeps_time_after_the_song_ends_and_plays_again_after_a_jump_back() {
        let (mut music, mut output, _dir) = player_without_output();
        music.start(Duration::from_millis(1500));
        pull(&mut output, 400);
        assert!(!music.finished());
        let sample = pull(&mut output, 600);
        assert_eq!(sample, 0.0, "silence after the end");
        assert!(music.finished());
        assert_near(music.position(), 2500);
        music.seek(Duration::from_millis(500));
        assert!(!music.finished(), "a jump back plays again");
        let sample = pull(&mut output, 100);
        assert!(!music.finished());
        assert!((ramp_ms(sample) - 600.0).abs() < 12.0, "{}", ramp_ms(sample));
        assert_near(music.position(), 600);
    }

    #[test]
    fn a_jump_to_the_top_at_the_end_plays_the_song_again_from_the_next_sample() {
        // How a looping sequence goes round: the jump lands on the audio thread before its next
        // sample, so the song starts over with no gap and no slip, loop after loop.
        let (mut music, mut output, _dir) = player_without_output();
        music.start(Duration::from_millis(1500));
        for _ in 0..3 {
            pull(&mut output, 495);
            music.seek(Duration::ZERO);
            let first = output.next().unwrap();
            assert!(ramp_ms(first) < 1.0, "{} ms", ramp_ms(first));
            let sample = pull(&mut output, 20);
            assert!((ramp_ms(sample) - 20.0).abs() < 2.0, "{}", ramp_ms(sample));
            assert_near(music.position(), 20);
            pull(&mut output, 1480);
            assert_near(music.position(), 1500);
        }
        assert!(!music.finished());
    }

    #[test]
    fn stands_still_while_paused_and_seeks_while_paused() {
        let (mut music, mut output, _dir) = player_without_output();
        music.start(Duration::ZERO);
        pull(&mut output, 300);
        music.pause();
        pull(&mut output, 10); // the pause reaches the audio thread within 5 ms
        let paused = music.position();
        pull(&mut output, 500);
        assert_eq!(music.position(), paused);
        music.seek(Duration::from_millis(1200));
        pull(&mut output, 100);
        assert_eq!(
            music.position(),
            Duration::from_millis(1200),
            "the jump waits for play"
        );
        music.resume();
        let sample = pull(&mut output, 100);
        assert_near(music.position(), 1290);
        assert!((ramp_ms(sample) - 1290.0).abs() < 15.0, "{}", ramp_ms(sample));
    }

    #[test]
    fn plays_slower_and_back_at_full_speed() {
        let (mut music, mut output, _dir) = player_without_output();
        music.start(Duration::from_millis(500));
        music.set_speed(0.5);
        let sample = pull(&mut output, 400);
        assert_near(music.position(), 700);
        assert!(
            (ramp_ms(sample) - 700.0).abs() < 12.0,
            "the music itself is slowed: {}",
            ramp_ms(sample)
        );
        music.set_speed(1.0);
        let sample = pull(&mut output, 200);
        assert_near(music.position(), 900);
        assert!((ramp_ms(sample) - 900.0).abs() < 12.0, "{}", ramp_ms(sample));
        music.set_speed(0.75);
        music.seek(Duration::from_millis(100));
        pull(&mut output, 400);
        assert_near(music.position(), 400);
    }

    #[test]
    fn nothing_waits_on_a_stalled_sound_device() {
        // Nobody pulls samples, like a device that stopped calling back.
        let (mut music, _output, _dir) = player_without_output();
        let started = Instant::now();
        music.start(Duration::ZERO);
        music.seek(Duration::from_secs(1));
        music.pause();
        music.seek(Duration::from_millis(300));
        music.set_volume(0.5);
        assert_eq!(music.position(), Duration::from_millis(300));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn a_jump_never_reads_before_its_target() {
        let (mut music, mut output, _dir) = player_with_buffer(Duration::from_millis(10));
        music.start(Duration::ZERO);
        pull(&mut output, 500);
        let target = Duration::from_millis(1000);
        music.seek(target);
        assert_eq!(music.position(), target);
        // Read while the output is part way through taking its first buffer from there.
        pull(&mut output, 1);
        assert_eq!(music.position(), target, "not a buffer's length before it");
        pull(&mut output, 50);
        let position = music.position();
        assert!(
            position > target && position <= Duration::from_millis(1051),
            "{position:?}"
        );
    }

    #[test]
    fn the_position_never_runs_backwards_between_jumps() {
        let (mut music, mut output, _dir) = player_with_buffer(Duration::from_millis(10));
        music.start(Duration::ZERO);
        music.set_speed(0.5);
        pull(&mut output, 400);
        let slowed = music.position();
        // At full speed a buffer holds twice as much of the song, so what is heard is put
        // further behind what was handed over.
        music.set_speed(1.0);
        assert_eq!(music.position(), slowed);
        pull(&mut output, 100);
        let playing = music.position();
        music.pause();
        let paused = music.position();
        assert!(paused >= playing, "{paused:?} after {playing:?}");
        music.resume();
        assert!(music.position() >= paused);
        // Only a jump takes it back.
        music.seek(Duration::from_millis(100));
        assert_eq!(music.position(), Duration::from_millis(100));
        pull(&mut output, 20);
        assert_near(music.position(), 115);
    }

    #[test]
    fn a_jump_that_fails_leaves_the_position_with_the_music() {
        /// Sound that can't be jumped in.
        struct Tone;
        impl Iterator for Tone {
            type Item = Sample;
            fn next(&mut self) -> Option<Sample> {
                Some(0.25)
            }
        }
        impl Source for Tone {
            fn current_span_len(&self) -> Option<usize> {
                None
            }
            fn channels(&self) -> ChannelCount {
                ChannelCount::MIN
            }
            fn sample_rate(&self) -> SampleRate {
                SampleRate::new(RATE).unwrap()
            }
            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
        let shared = Arc::new(Shared::default());
        let source = MusicSource::new(Tone, Arc::clone(&shared));
        let (player, mut output) = Player::new();
        let mut music = MusicPlayer::with_player(player, source, shared, Duration::from_millis(10), None);
        music.start(Duration::ZERO);
        pull(&mut output, 500);
        assert_near(music.position(), 495);
        music.seek(Duration::from_secs(5));
        pull(&mut output, 100);
        // It plays on from where it was, and the clock says so: it isn't held at the place
        // the jump never reached.
        assert!(music.problem().is_some());
        assert_near(music.position(), 595);
    }

    #[test]
    fn the_heard_position_trails_the_output_by_a_buffer_and_moves_smoothly() {
        let buffer = Duration::from_millis(10);
        let at =
            |raw: u64, since: u64| heard(Duration::from_millis(raw), Duration::from_millis(since), buffer);
        assert_eq!(
            at(100, 0),
            Duration::from_millis(90),
            "just handed a buffer: not heard yet"
        );
        assert_eq!(at(100, 4), Duration::from_millis(94));
        assert_eq!(at(100, 10), Duration::from_millis(100));
        assert_eq!(
            at(100, 50),
            Duration::from_millis(100),
            "never runs past what was handed over"
        );
        assert_eq!(at(5, 0), Duration::ZERO);
    }
}
