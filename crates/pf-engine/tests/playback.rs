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
