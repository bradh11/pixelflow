//! Testing DMX fixtures on the Test screen: patterns light them through their colour, and their
//! channels can be set by hand while output runs. Captured in memory; nothing is sent.

use pf_engine::{Edit, Engine, EngineError, PatternKind, PatternSpec, TargetSpec};
use pf_model::{
    ArtNetConfig, Controller, DmxFixture, Generator, PortSlot, Prop, Protocol, ShapeSource, dmx_layout,
};
use pf_output::{Recorded, RecordingTransport, Transport, artnet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

const NODE: &str = "127.0.0.1:6454";

/// An engine sending to a recorder, with a 7-channel flood at address 3 on an Art-Net node, and
/// a strip on no controller.
fn engine_with_flood() -> (Engine, Recorded, Prop, Controller, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = RecordingTransport::new();
    let mut engine =
        Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn Transport>));
    let mut flood = Prop::new(
        "Flood",
        ShapeSource::Generator(Generator::Line {
            nodes: 1,
            length: 0.5,
        }),
    );
    let mut dmx = DmxFixture::from_layout(dmx_layout("dimmer-rgbw-strobe-mode").unwrap(), 1, 3);
    dmx.channels[6].value = 40;
    flood.dmx = Some(dmx);
    let mut node = Controller::dmx("Node", "127.0.0.1", Protocol::ArtNet(ArtNetConfig::default()), 1);
    node.ports[0].slots.push(PortSlot::new(flood.id));
    engine
        .apply(vec![
            Edit::AddProp { prop: flood.clone() },
            Edit::AddController {
                controller: node.clone(),
            },
        ])
        .unwrap();
    (engine, recorded, flood, node, dir)
}

/// The DMX channels of the packets sent to the node, oldest first.
fn universes(recorded: &Recorded) -> Vec<Vec<u8>> {
    let node: SocketAddr = NODE.parse().unwrap();
    recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, to)| *to == node)
        .map(|(p, _)| p[artnet::HEADER_LEN..].to_vec())
        .collect()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn spec(kind: PatternKind, color: &str, channels: Vec<u8>) -> PatternSpec {
    PatternSpec {
        kind,
        color: color.into(),
        channels,
    }
}

#[test]
fn a_pattern_lights_a_flood_through_its_colour() {
    let (mut engine, recorded, _flood, node, _dir) = engine_with_flood();
    engine
        .start_output(
            spec(PatternKind::Solid, "ff8000", vec![]),
            TargetSpec::Controller { id: node.id },
        )
        .unwrap();
    wait_until(|| !universes(&recorded).is_empty());
    let first = &universes(&recorded)[0];
    assert_eq!(first.len(), 512, "a whole universe");
    // From address 3: dimmer full, orange, no white, strobe 0, the mode held at 40.
    assert_eq!(&first[..9], &[0, 0, 255, 255, 128, 0, 0, 0, 40]);
    assert_eq!(&engine.preview_frame().unwrap()[..3], &[255, 128, 0]);
    engine.stop_output();
}

#[test]
fn a_floods_channels_can_be_set_by_hand_while_output_runs() {
    let (mut engine, recorded, flood, _node, _dir) = engine_with_flood();
    engine
        .start_output(
            spec(PatternKind::Channels, "000000", vec![128, 0, 0, 255]),
            TargetSpec::Prop { id: flood.id },
        )
        .unwrap();
    wait_until(|| !universes(&recorded).is_empty());
    assert_eq!(&universes(&recorded)[0][2..9], &[128, 0, 0, 255, 0, 0, 40]);
    assert_eq!(&engine.preview_frame().unwrap()[..3], &[0, 0, 128], "half blue");
    let generation = engine.output_status().generation;
    let status = engine.set_test_channels(vec![255, 255, 0, 0, 0, 200, 40]);
    assert_eq!(status.generation, generation, "no restart, so no black frame");
    wait_until(|| universes(&recorded).last().is_some_and(|u| u[2] == 255));
    assert_eq!(
        &universes(&recorded).last().unwrap()[2..9],
        &[255, 255, 0, 0, 0, 200, 40]
    );
    let last = engine.stop_output();
    assert!(!last.running);
    assert!(
        universes(&recorded).last().unwrap().iter().all(|&b| b == 0),
        "black on stop"
    );
}

#[test]
fn only_a_fixture_can_have_its_channels_set_by_hand() {
    let (mut engine, _recorded, _flood, node, _dir) = engine_with_flood();
    let refused = engine.start_output(
        spec(PatternKind::Channels, "000000", vec![255]),
        TargetSpec::Controller { id: node.id },
    );
    assert!(matches!(refused, Err(EngineError::NotAFixture)), "{refused:?}");
}
