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
    frame as u8 + 1
}

/// Writes an uncompressed version 1 sequence: 30 channels, 12 frames, 25 ms apart.
fn write_sequence(dir: &Path) -> PathBuf {
    let mut out = Vec::new();
    out.extend_from_slice(b"PSEQ");
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&FRAMES.to_le_bytes());
    out.push(STEP_MS);
    out.extend_from_slice(&[0; 9]);
    for frame in 0..FRAMES {
        out.extend(std::iter::repeat_n(value(frame), CHANNELS as usize));
    }
    let path = dir.join("medley.fseq");
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
        });
    }
    engine
        .apply(vec![Edit::AddProp { prop }, Edit::AddController { controller }])
        .unwrap();
    (engine, recorded, dir)
}

fn packets(recorded: &Recorded) -> Vec<Vec<u8>> {
    let dest: SocketAddr = "127.0.0.1:4048".parse().unwrap();
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
