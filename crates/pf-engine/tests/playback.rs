//! Playing a rendered sequence to controllers, captured in memory.

use pf_engine::{Edit, Engine, EngineError, PatternSpec, TargetSpec};
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, SequenceChannels, ShapeSource};
use pf_output::{Recorded, RecordingTransport, Transport};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CHANNELS: u32 = 30;
const FRAMES: u32 = 12;
const STEP_MS: u8 = 25;

/// Channel value in `frame`: every channel of a frame holds the frame number + 1.
fn value(frame: u32) -> u8 {
    (frame % 250) as u8 + 1
}

/// Writes an uncompressed version 1 sequence: 30 channels, 12 frames, 25 ms apart.
fn write_sequence(dir: &Path) -> PathBuf {
    write_sequence_of(dir, "medley.fseq", FRAMES)
}

/// A sequence long enough (20 s) to still be playing when a test is done editing the show.
fn write_long_sequence(dir: &Path) -> PathBuf {
    write_sequence_of(dir, "long.fseq", 800)
}

fn write_sequence_of(dir: &Path, name: &str, frames: u32) -> PathBuf {
    let mut out = Vec::new();
    out.extend_from_slice(b"PSEQ");
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.push(STEP_MS);
    out.extend_from_slice(&[0; 9]);
    for frame in 0..frames {
        out.extend(std::iter::repeat_n(value(frame), CHANNELS as usize));
    }
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

/// A show with one DDP controller that takes sequence channels 1–30, wired to a 10-pixel strip.
fn engine_with_show(sequence_channels: bool) -> (Engine, Recorded, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = RecordingTransport::new();
    let mut engine =
        Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn Transport>));
    let prop = Prop::new(
        "Strip",
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    );
    let mut controller = Controller::new("Falcon", "127.0.0.1:4048", Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    controller.ports.push(port);
    if sequence_channels {
        controller.sequence_channels = Some(SequenceChannels {
            start: 1,
            count: CHANNELS,
            raw_ddp_offsets: false,
        });
    }
    engine
        .apply(vec![Edit::AddProp { prop }, Edit::AddController { controller }])
        .unwrap();
    (engine, recorded, dir)
}

fn packets(recorded: &Recorded) -> Vec<Vec<u8>> {
    packets_to(recorded, "127.0.0.1:4048")
}

fn packets_to(recorded: &Recorded, address: &str) -> Vec<Vec<u8>> {
    let dest: SocketAddr = address.parse().unwrap();
    recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, to)| *to == dest)
        .map(|(p, _)| p.clone())
        .collect()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn plays_frames_in_order_then_ends_dark() {
    let (mut engine, recorded, dir) = engine_with_show(true);
    let path = write_sequence(dir.path());
    let status = engine.start_playback(&path, 0).unwrap();
    assert_eq!(
        (status.state, status.duration_ms, status.frame_ms),
        ("playing", 300, 25)
    );
    assert!(status.notes.is_empty(), "{:?}", status.notes);
    wait_until(|| !packets(&recorded).is_empty());
    let first = packets(&recorded)[0][10];
    assert!(
        first == value(0) || first == value(1),
        "never a black first frame (got {first})"
    );

    wait_until(|| engine.playback_status().unwrap().state == "ended");
    let sent: Vec<u8> = packets(&recorded).iter().map(|p| p[10]).collect();
    let mut frames_seen: Vec<u8> = sent.clone();
    frames_seen.dedup();
    assert!(
        frames_seen.windows(2).all(|w| w[1] > w[0] || w[1] == 0),
        "in order: {frames_seen:?}"
    );
    assert!(
        frames_seen.contains(&value(FRAMES - 1)),
        "reached the last frame: {frames_seen:?}"
    );
    wait_until(|| packets(&recorded).last().unwrap()[10..].iter().all(|&b| b == 0));
    assert_eq!(
        engine.playback_status().unwrap().position_ms,
        300,
        "ended at the end"
    );

    // Seeking after the end plays again.
    engine.seek_playback(0).unwrap();
    assert_eq!(engine.playback_status().unwrap().state, "playing");

    engine.stop_playback();
    assert!(engine.playback_status().is_none());
}

#[test]
fn pause_seek_and_preview() {
    let (mut engine, recorded, dir) = engine_with_show(true);
    let path = write_sequence(dir.path());
    engine.start_playback(&path, 50).unwrap();
    assert_eq!(
        engine.playback_status().unwrap().position_ms,
        50,
        "started at frame 2"
    );
    engine.set_playback_paused(true).unwrap();
    engine.seek_playback(200).unwrap();
    wait_until(|| engine.live_frame().unwrap() == vec![value(8); 30]);
    assert_eq!(
        engine.sequence_frame().unwrap(),
        vec![value(8); CHANNELS as usize]
    );
    let status = engine.playback_status().unwrap();
    assert_eq!((status.state, status.position_ms), ("paused", 200));
    wait_until(|| packets(&recorded).last().unwrap()[10] == value(8));
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(engine.playback_status().unwrap().position_ms, 200, "stays paused");

    engine.set_playback_paused(false).unwrap();
    wait_until(|| engine.playback_status().unwrap().position_ms > 200);
}

#[test]
fn test_patterns_and_playback_replace_each_other() {
    let (mut engine, _recorded, dir) = engine_with_show(true);
    let path = write_sequence(dir.path());
    engine.start_playback(&path, 0).unwrap();
    let solid: PatternSpec = serde_json::from_value(serde_json::json!({ "kind": "solid" })).unwrap();
    engine.start_output(solid, TargetSpec::Show).unwrap();
    assert!(
        engine.playback_status().is_none(),
        "a test pattern stops playback"
    );
    engine.start_playback(&path, 0).unwrap();
    assert!(!engine.output_status().running, "playback stops the test pattern");
    engine.new_show("Next");
    assert!(engine.playback_status().is_none(), "a new show stops playback");
}

#[test]
fn explains_when_nothing_can_play() {
    let (mut engine, _recorded, dir) = engine_with_show(false);
    let path = write_sequence(dir.path());
    let error = engine.start_playback(&path, 0).unwrap_err();
    assert!(matches!(error, EngineError::Playback(_)));
    assert!(error.to_string().contains("Devices screen"), "{error}");

    let (mut engine, _recorded, _dir2) = engine_with_show(true);
    let missing = engine
        .start_playback(&dir.path().join("nope.fseq"), 0)
        .unwrap_err();
    assert!(
        missing
            .to_string()
            .starts_with("Could not read the sequence file"),
        "{missing}"
    );
}

#[test]
fn preview_props_place_every_pixel() {
    let (engine, _recorded, _dir) = engine_with_show(true);
    let props = engine.preview_props();
    assert_eq!(props.len(), 1);
    assert_eq!((props[0].frame_offset, props[0].channels_per_pixel), (0, 3));
    assert_eq!(props[0].points.len(), 20, "x, y for each of 10 pixels");
    let xs: Vec<f32> = props[0].points.iter().step_by(2).copied().collect();
    assert!(
        xs.windows(2).all(|w| w[1] > w[0]),
        "a line runs left to right: {xs:?}"
    );
}

#[test]
fn props_that_are_not_wired_are_still_drawn_in_the_preview() {
    let (mut engine, _recorded, _dir) = engine_with_show(true);
    let loose = Prop::new(
        "Not wired yet",
        ShapeSource::Generator(Generator::Line {
            nodes: 4,
            length: 1.0,
        }),
    );
    let id = loose.id;
    engine.apply(vec![Edit::AddProp { prop: loose }]).unwrap();
    let props = engine.preview_props();
    assert_eq!(props.len(), 2);
    let preview = props
        .iter()
        .find(|p| p.prop == id)
        .expect("the unwired prop is in the preview");
    assert_eq!(preview.points.len(), 8, "x, y for each of its 4 pixels");
    assert_eq!(
        preview.frame_offset, 30,
        "its colors follow the wired strip's in a live frame"
    );
}

#[test]
fn a_damaged_sequence_goes_dark_and_says_what_happened() {
    let (mut engine, recorded, dir) = engine_with_show(true);
    let path = write_sequence(dir.path());
    // Cut the file in the middle of frame 5.
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..28 + 5 * CHANNELS as usize + 7]).unwrap();
    engine.start_playback(&path, 0).unwrap();
    wait_until(|| engine.playback_status().unwrap().error.is_some());
    let status = engine.playback_status().unwrap();
    assert_eq!(status.state, "ended");
    wait_until(|| packets(&recorded).last().unwrap()[10..].iter().all(|&b| b == 0));
    assert!(
        engine.live_frame().unwrap().iter().all(|&b| b == 0),
        "preview cleared"
    );
    assert!(engine.sequence_frame().unwrap().iter().all(|&b| b == 0));
}

fn set_address(engine: &mut Engine, address: &str) {
    let mut controller = engine.show().controllers[0].clone();
    controller.address = address.to_string();
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
}

#[test]
fn removing_the_controller_stops_playback_and_says_why() {
    let (mut engine, _recorded, dir) = engine_with_show(true);
    let path = write_long_sequence(dir.path());
    engine.start_playback(&path, 0).unwrap();
    assert_eq!(engine.playback_stop_reason(), None);
    let id = engine.show().controllers[0].id;
    engine.apply(vec![Edit::RemoveController { id }]).unwrap();
    assert!(engine.playback_status().is_none());
    assert_eq!(
        engine.playback_stop_reason(),
        Some("Playback stopped because no controller has sequence channels anymore.")
    );
    engine.undo();
    engine.start_playback(&path, 0).unwrap();
    assert_eq!(engine.playback_stop_reason(), None, "starting again clears it");
}

#[test]
fn changing_a_controller_address_restarts_playback_to_the_new_address() {
    let (mut engine, recorded, dir) = engine_with_show(true);
    let path = write_long_sequence(dir.path());
    engine.start_playback(&path, 0).unwrap();
    wait_until(|| !packets(&recorded).is_empty());
    let generation = engine.playback_generation();
    set_address(&mut engine, "127.0.0.2:4048");
    assert_eq!(engine.playback_generation(), generation + 1, "restarted");
    wait_until(|| !packets_to(&recorded, "127.0.0.2:4048").is_empty());
    assert_eq!(engine.playback_status().unwrap().state, "playing");

    // Undo puts the address back, and playback follows.
    recorded.lock().unwrap().clear();
    engine.undo();
    wait_until(|| !packets(&recorded).is_empty());
    assert_eq!(engine.playback_status().unwrap().state, "playing");
}

#[test]
fn a_restart_keeps_the_position_and_the_pause() {
    let (mut engine, _recorded, dir) = engine_with_show(true);
    let path = write_long_sequence(dir.path());
    engine.start_playback(&path, 0).unwrap();
    engine.set_playback_paused(true).unwrap();
    engine.seek_playback(1000).unwrap();
    wait_until(|| engine.sequence_frame().unwrap()[0] == value(40));
    set_address(&mut engine, "127.0.0.2:4048");
    let status = engine.playback_status().unwrap();
    assert_eq!((status.state, status.position_ms), ("paused", 1000));
}

#[test]
fn moving_a_prop_does_not_restart_playback() {
    let (mut engine, _recorded, dir) = engine_with_show(true);
    let path = write_long_sequence(dir.path());
    engine.start_playback(&path, 0).unwrap();
    let generation = engine.playback_generation();
    let mut prop = engine.show().props[0].clone();
    prop.transform.position = pf_model::Vec3::new(3.0, 1.0, 0.0);
    engine.apply(vec![Edit::UpdateProp { prop }]).unwrap();
    assert_eq!(engine.playback_generation(), generation, "same session");
    let before = engine.playback_status().unwrap().position_ms;
    wait_until(|| engine.playback_status().unwrap().position_ms > before + 50);
}

/// Set `PIXELFLOW_FSEQ=/path/to/show.fseq` to play a real sequence for a second (to loopback only).
#[test]
fn real_sequence_plays_when_provided() {
    let Ok(path) = std::env::var("PIXELFLOW_FSEQ") else {
        return;
    };
    let (mut engine, recorded, _dir) = engine_with_show(false);
    let mut controller = engine.show().controllers[0].clone();
    controller.sequence_channels = Some(SequenceChannels {
        start: 1,
        count: 6147,
        raw_ddp_offsets: false,
    });
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    let status = engine.start_playback(Path::new(&path), 60_000).unwrap();
    eprintln!("{status:?}");
    std::thread::sleep(Duration::from_secs(1));
    let sent = packets(&recorded);
    let lit = sent.iter().filter(|p| p[10..].iter().any(|&b| b != 0)).count();
    eprintln!(
        "{} packets in 1 s, {lit} with light; position {} ms",
        sent.len(),
        engine.playback_status().unwrap().position_ms
    );
    assert!(lit > 0);
    assert!(
        engine.live_frame().unwrap().iter().any(|&b| b != 0),
        "the strip shows light"
    );
}

/// The music a test plays along with.
#[derive(Clone, Copy, Debug)]
enum Music {
    /// Keeps going for as long as the lights need.
    Endless,
    /// A song this many ms long: its clock stops there and it says it's finished.
    Song(u64),
    /// A sound device that stops calling back this many ms in: the clock stands still, unfinished.
    Stuck(u64),
    /// The music can't be opened.
    Broken,
    /// Opening the music crashes the player thread.
    Panics,
    /// Closing the music hangs for a long time (a stalled sound device).
    SlowToClose,
}

/// A clock that keeps silent time and records what playback asked of it.
struct LoggingClock {
    inner: pf_audio::SilentClock,
    log: Log,
    music: Music,
}

impl LoggingClock {
    fn song_ms(&self) -> Option<u64> {
        match self.music {
            Music::Song(ms) | Music::Stuck(ms) => Some(ms),
            _ => None,
        }
    }
}

impl pf_audio::AudioClock for LoggingClock {
    fn start(&mut self, position: Duration) {
        self.log
            .lock()
            .unwrap()
            .push(format!("start {}", position.as_millis()));
        self.inner.start(position);
    }
    fn pause(&mut self) {
        self.log.lock().unwrap().push("pause".into());
        self.inner.pause();
    }
    fn resume(&mut self) {
        self.log.lock().unwrap().push("resume".into());
        self.inner.resume();
    }
    fn seek(&mut self, position: Duration) {
        self.log
            .lock()
            .unwrap()
            .push(format!("seek {}", position.as_millis()));
        self.inner.seek(position);
    }
    fn position(&self) -> Duration {
        let now = self.inner.position();
        self.song_ms()
            .map_or(now, |ms| now.min(Duration::from_millis(ms)))
    }
    fn set_volume(&mut self, volume: f32) {
        self.log.lock().unwrap().push(format!("volume {volume}"));
    }
    fn finished(&self) -> bool {
        match self.music {
            Music::Song(ms) => self.inner.position() >= Duration::from_millis(ms),
            Music::Stuck(_) => false,
            _ => true,
        }
    }
}

impl Drop for LoggingClock {
    fn drop(&mut self) {
        if let Music::SlowToClose = self.music {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
}

type Log = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// An engine whose show has one sequence (`frames` long) with music played by a logging clock.
fn engine_with(
    music: Music,
    offset_ms: i32,
    frames: u32,
) -> (Engine, Log, pf_model::SequenceEntry, tempfile::TempDir) {
    let (engine, _recorded, dir) = engine_with_show(true);
    let (engine, log, entry) = with_music(engine, music, offset_ms, frames, dir.path());
    (engine, log, entry, dir)
}

fn with_music(
    engine: Engine,
    music: Music,
    offset_ms: i32,
    frames: u32,
    dir: &Path,
) -> (Engine, Log, pf_model::SequenceEntry) {
    let log: Log = Default::default();
    let opened = log.clone();
    let clocks: pf_engine::ClockFactory = std::sync::Arc::new(move |path: Option<&Path>| {
        opened.lock().unwrap().push(format!(
            "open {}",
            path.map_or("none".into(), |m| m.display().to_string())
        ));
        match music {
            Music::Broken => return Err(pf_audio::AudioError::NoOutput("no speakers".into())),
            Music::Panics => panic!("the sound system crashed"),
            _ => {}
        }
        Ok(Box::new(LoggingClock {
            inner: pf_audio::SilentClock::new(),
            log: opened.clone(),
            music,
        }) as Box<dyn pf_audio::AudioClock>)
    });
    let mut engine = engine.with_clocks(clocks);
    let path = write_sequence_of(dir, "medley.fseq", frames);
    let mut entry = pf_model::SequenceEntry::new("Medley", path.display().to_string());
    entry.audio = Some("/music/medley.mp3".into());
    entry.offset_ms = offset_ms;
    engine
        .apply(vec![Edit::AddSequence {
            sequence: entry.clone(),
        }])
        .unwrap();
    (engine, log, entry)
}

fn logged(log: &Log, line: &str) -> bool {
    log.lock().unwrap().iter().any(|l| l == line)
}

#[test]
fn sequences_play_with_their_music_lined_up_by_the_offset() {
    let (mut engine, log, entry, _dir) = engine_with(Music::Endless, 100, FRAMES);
    let status = engine.play_sequence(entry.id, 0).unwrap();
    assert_eq!(status.sequence, Some(entry.id));
    assert_eq!(status.music.as_deref(), Some(Path::new("/music/medley.mp3")));
    assert_eq!(status.offset_ms, 100);
    assert!(status.notes.is_empty(), "{:?}", status.notes);
    // The lights run 100 ms ahead, so the song starts once they're 100 ms in.
    wait_until(|| logged(&log, "start 0"));
    engine.set_playback_paused(true).unwrap();
    engine.seek_playback(250).unwrap();
    // Lights at 250 ms means music at 150 ms when the lights run 100 ms ahead.
    wait_until(|| logged(&log, "seek 150"));
    let entries = log.lock().unwrap().clone();
    assert_eq!(entries[0], "open /music/medley.mp3");
    assert!(entries.contains(&"pause".to_string()), "{entries:?}");

    engine.set_playback_volume(0.4).unwrap();
    wait_until(|| logged(&log, "volume 0.4"));
}

#[test]
fn lights_ahead_of_the_music_play_their_first_frames_before_the_song_starts() {
    let (engine, recorded, dir) = engine_with_show(true);
    let (mut engine, log, entry) = with_music(engine, Music::Endless, 100, FRAMES, dir.path());
    engine.play_sequence(entry.id, 0).unwrap();
    wait_until(|| logged(&log, "start 0"));
    let starts: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.starts_with("start"))
        .cloned()
        .collect();
    assert_eq!(starts, vec!["start 0"]);
    wait_until(|| engine.playback_status().unwrap().state == "ended");
    let sent: Vec<u8> = packets(&recorded).iter().map(|p| p[10]).collect();
    assert!(
        [value(1), value(2), value(3)].iter().any(|v| sent.contains(v)),
        "frames before the song starts are shown: {sent:?}"
    );
}

#[test]
fn starting_from_the_top_plays_the_whole_song_when_the_lights_run_behind() {
    let (mut engine, log, entry, _dir) = engine_with(Music::Endless, -100, FRAMES);
    engine.play_sequence(entry.id, 0).unwrap();
    wait_until(|| log.lock().unwrap().iter().any(|l| l.starts_with("start")));
    assert!(logged(&log, "start 0"), "{:?}", log.lock().unwrap());
    // Restarting from the top does the same.
    engine.seek_playback(0).unwrap();
    wait_until(|| logged(&log, "seek 0"));
    assert!(!logged(&log, "seek 100"), "{:?}", log.lock().unwrap());
}

#[test]
fn music_shorter_than_the_lights_still_ends() {
    // The song stops at 150 ms; the lights go on to 300 ms on their own, then end.
    let (mut engine, _log, entry, _dir) = engine_with(Music::Song(150), 0, FRAMES);
    engine.play_sequence(entry.id, 0).unwrap();
    wait_until(|| engine.playback_status().unwrap().state == "ended");
    let status = engine.playback_status().unwrap();
    assert_eq!((status.position_ms, status.error), (300, None));
    assert!(status.notes.is_empty(), "{:?}", status.notes);
}

#[test]
fn music_longer_than_the_lights_plays_out_then_stays_stopped() {
    let (mut engine, log, entry, _dir) = engine_with(Music::Song(500), 0, FRAMES);
    engine.play_sequence(entry.id, 0).unwrap();
    // The lights are done at 300 ms, but the song plays on to 500 ms.
    wait_until(|| engine.playback_status().unwrap().position_ms == 300);
    assert_eq!(engine.playback_status().unwrap().state, "playing");
    wait_until(|| engine.playback_status().unwrap().state == "ended");
    std::thread::sleep(Duration::from_millis(60));
    let entries = log.lock().unwrap().clone();
    let last = entries
        .iter()
        .rev()
        .find(|l| *l == "pause" || *l == "resume")
        .cloned();
    assert_eq!(
        last.as_deref(),
        Some("pause"),
        "the music stays stopped: {entries:?}"
    );
    assert_eq!(engine.playback_status().unwrap().state, "ended");

    // Seeking after the end plays again.
    engine.seek_playback(0).unwrap();
    wait_until(|| log.lock().unwrap().last().is_some_and(|l| l == "resume"));
    assert_eq!(engine.playback_status().unwrap().state, "playing");
}

#[test]
fn a_stalled_sound_device_does_not_freeze_the_lights() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Stuck(100), 0, 800);
    engine.play_sequence(entry.id, 0).unwrap();
    wait_until(|| engine.playback_status().unwrap().position_ms > 1200);
    let status = engine.playback_status().unwrap();
    assert_eq!(status.state, "playing");
    assert_eq!(
        status.notes,
        vec!["The sound output stopped responding, so the lights are keeping time on their own."]
    );
}

#[test]
fn stopping_never_waits_long_for_the_sound_device() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::SlowToClose, 0, FRAMES);
    engine.play_sequence(entry.id, 0).unwrap();
    let started = Instant::now();
    engine.stop_playback();
    assert!(engine.playback_status().is_none());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_crashed_player_says_so_instead_of_playing_forever() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Panics, 0, FRAMES);
    let status = engine.play_sequence(entry.id, 0).unwrap();
    assert_eq!(status.state, "ended");
    assert!(
        status
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("stopped unexpectedly"),
        "{status:?}"
    );
}

#[test]
fn editing_the_offset_applies_live_and_removing_the_sequence_stops_it() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Endless, 100, FRAMES);
    engine.play_sequence(entry.id, 0).unwrap();
    let generation = engine.playback_generation();
    let mut later = entry.clone();
    later.offset_ms = -80;
    engine
        .apply(vec![Edit::UpdateSequence { sequence: later }])
        .unwrap();
    assert_eq!(engine.playback_status().unwrap().offset_ms, -80);
    assert_eq!(
        engine.playback_generation(),
        generation,
        "no restart for an offset change"
    );

    engine.apply(vec![Edit::RemoveSequence { id: entry.id }]).unwrap();
    assert!(engine.playback_status().is_none());
    assert!(engine.playback_stop_reason().unwrap().contains("removed"));
}

#[test]
fn offsets_stay_within_ten_seconds() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Endless, 0, FRAMES);
    let mut far = entry.clone();
    far.offset_ms = 10_001;
    let error = engine
        .apply(vec![Edit::UpdateSequence { sequence: far }])
        .unwrap_err();
    assert!(error.to_string().contains("at most 10000 ms"), "{error}");
    assert_eq!(engine.show().sequences[0].offset_ms, 0);
}

#[test]
fn a_controller_edit_keeps_the_music_playing_without_reopening_it() {
    let (mut engine, log, entry, _dir) = engine_with(Music::Endless, 0, 800);
    engine.play_sequence(entry.id, 0).unwrap();
    let generation = engine.playback_generation();
    set_address(&mut engine, "127.0.0.2:4048");
    assert_eq!(
        engine.playback_generation(),
        generation + 1,
        "sends to the new address"
    );
    assert_eq!(engine.playback_status().unwrap().state, "playing");
    let opens = log
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.starts_with("open"))
        .count();
    assert_eq!(opens, 1, "{:?}", log.lock().unwrap());
    let before = engine.playback_status().unwrap().position_ms;
    wait_until(|| engine.playback_status().unwrap().position_ms > before + 50);
}

#[test]
fn new_music_while_paused_restarts_paused_without_playing() {
    let (mut engine, log, entry, _dir) = engine_with(Music::Endless, 0, 800);
    engine.play_sequence(entry.id, 0).unwrap();
    engine.set_playback_paused(true).unwrap();
    engine.seek_playback(1000).unwrap();
    wait_until(|| logged(&log, "seek 1000"));
    let mut changed = entry.clone();
    changed.audio = Some("/music/other.mp3".into());
    engine
        .apply(vec![Edit::UpdateSequence { sequence: changed }])
        .unwrap();
    let status = engine.playback_status().unwrap();
    assert_eq!((status.state, status.position_ms), ("paused", 1000));
    std::thread::sleep(Duration::from_millis(50));
    let entries = log.lock().unwrap().clone();
    let reopened = entries
        .iter()
        .position(|l| l == "open /music/other.mp3")
        .expect("opened the new music");
    let after = &entries[reopened..];
    assert!(after.contains(&"seek 1000".to_string()), "{after:?}");
    assert!(
        !after.iter().any(|l| l.starts_with("start") || l == "resume"),
        "never plays: {after:?}"
    );
}

#[test]
fn without_working_music_only_the_lights_play() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Broken, 100, FRAMES);
    let status = engine.play_sequence(entry.id, 0).unwrap();
    assert_eq!(status.state, "playing");
    assert_eq!(status.music, None);
    assert_eq!(
        status.notes,
        vec!["No sound output is available. Only the lights are playing."]
    );
}

#[test]
fn a_sequence_added_twice_gets_a_different_name() {
    let (mut engine, _log, entry, _dir) = engine_with(Music::Endless, 0, FRAMES);
    let mut again = pf_model::SequenceEntry::new("Medley", entry.path.clone());
    again.audio = entry.audio.clone();
    engine.add_sequence(again.clone()).unwrap();
    again.id = pf_model::SequenceId::new();
    let snapshot = engine.add_sequence(again).unwrap();
    let names: Vec<&str> = snapshot.show.sequences.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Medley", "Medley (2)", "Medley (3)"]);
}

#[test]
fn a_sequence_entry_finds_its_music() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_sequence(dir.path());
    let entry = pf_engine::sequence_entry_for(&path).unwrap();
    assert_eq!(
        (entry.name.as_str(), entry.audio.as_deref(), entry.offset_ms),
        ("medley", None, 0)
    );
    let song = dir.path().join("medley.mp3");
    std::fs::write(&song, b"x").unwrap();
    assert_eq!(
        pf_engine::sequence_entry_for(&path).unwrap().audio,
        Some(song.display().to_string())
    );
    assert!(pf_engine::sequence_entry_for(&dir.path().join("nope.fseq")).is_err());
}
