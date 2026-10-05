//! Live output driven by the engine, captured in memory.

use pf_engine::{Edit, Engine, EngineError, PatternSpec, TargetSpec};
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource, Vec3};
use pf_output::{Recorded, RecordingTransport, Transport};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

fn line(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    )
}

/// An engine whose output goes to an in-memory recorder, with one DDP controller wired to one prop.
fn engine_with_show() -> (Engine, Recorded, Prop, Controller, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = RecordingTransport::new();
    let mut engine =
        Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn Transport>));
    let prop = line("Strip", 3);
    let mut controller = Controller::new("Bench", "127.0.0.1:4048", Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    controller.ports.push(port);
    engine
        .apply(vec![
            Edit::AddProp { prop: prop.clone() },
            Edit::AddController {
                controller: controller.clone(),
            },
        ])
        .unwrap();
    (engine, recorded, prop, controller, dir)
}

fn solid_red() -> PatternSpec {
    serde_json::from_value(serde_json::json!({ "kind": "solid", "color": "ff0000" })).unwrap()
}

fn packets_to(recorded: &Recorded, dest: &str) -> Vec<Vec<u8>> {
    let dest: SocketAddr = dest.parse().unwrap();
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
fn runs_a_pattern_and_blacks_out_on_stop() {
    let (mut engine, recorded, _prop, _controller, _dir) = engine_with_show();
    let status = engine.start_output(solid_red(), TargetSpec::Show).unwrap();
    assert!(status.running);
    wait_until(|| !packets_to(&recorded, "127.0.0.1:4048").is_empty());
    let first = &packets_to(&recorded, "127.0.0.1:4048")[0];
    assert_eq!(
        &first[10..],
        &[255, 0, 0, 255, 0, 0, 255, 0, 0],
        "first frame is already red"
    );

    let preview = engine.preview_frame().unwrap();
    assert_eq!(preview, vec![255, 0, 0, 255, 0, 0, 255, 0, 0]);

    let status = engine.stop_output();
    assert!(!status.running);
    let last = packets_to(&recorded, "127.0.0.1:4048").pop().unwrap();
    assert!(last[10..].iter().all(|&b| b == 0), "blackout on stop");
    assert!(engine.preview_frame().is_none());
}

#[test]
fn output_status_reports_controllers() {
    let (mut engine, _recorded, _prop, controller, _dir) = engine_with_show();
    engine.start_output(solid_red(), TargetSpec::Show).unwrap();
    wait_until(|| {
        engine
            .output_status()
            .controllers
            .first()
            .is_some_and(|c| c.packets_sent > 0)
    });
    let status = engine.output_status();
    assert_eq!(status.controllers[0].id, controller.id);
    assert_eq!(status.controllers[0].state, "ok");
    assert_eq!(status.target, Some(TargetSpec::Show));
}

#[test]
fn refuses_to_start_when_the_show_has_errors_or_the_color_is_bad() {
    let (mut engine, _recorded, _prop, mut controller, _dir) = engine_with_show();
    controller.ports[0].max_pixels = Some(1);
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    let err = engine.start_output(solid_red(), TargetSpec::Show).unwrap_err();
    assert!(matches!(err, EngineError::ShowHasErrors(_)));
    assert!(err.to_string().contains("over capacity"), "{err}");

    let bad = PatternSpec {
        color: "nope".into(),
        ..solid_red()
    };
    assert!(matches!(
        engine.start_output(bad, TargetSpec::Show),
        Err(EngineError::BadColor(_))
    ));
    assert!(!engine.output_status().running);
}

#[test]
fn moving_a_prop_keeps_output_running_but_rewiring_restarts_it() {
    let (mut engine, recorded, mut prop, controller, _dir) = engine_with_show();
    let generation = engine
        .start_output(solid_red(), TargetSpec::Show)
        .unwrap()
        .generation;

    prop.transform.position = Vec3::new(5.0, 0.0, 0.0);
    engine
        .apply(vec![Edit::UpdateProp { prop: prop.clone() }])
        .unwrap();
    assert_eq!(engine.output_status().generation, generation, "layout-only edit");

    let mut moved = controller.clone();
    moved.address = "127.0.0.2:4048".into();
    engine
        .apply(vec![Edit::UpdateController { controller: moved }])
        .unwrap();
    let status = engine.output_status();
    assert!(status.running);
    assert_eq!(status.generation, generation + 1, "rewired → restarted");
    wait_until(|| !packets_to(&recorded, "127.0.0.2:4048").is_empty());

    engine.undo();
    assert_eq!(engine.output_status().generation, generation + 2);
}

#[test]
fn rotating_scaling_and_the_background_photo_keep_output_running() {
    let (mut engine, _recorded, mut prop, _controller, _dir) = engine_with_show();
    let generation = engine
        .start_output(solid_red(), TargetSpec::Show)
        .unwrap()
        .generation;
    prop.transform.rotation_deg = Vec3::new(0.0, 0.0, 30.0);
    prop.transform.scale = Vec3::new(2.0, 0.5, 1.0);
    engine
        .apply(vec![
            Edit::UpdateProp { prop },
            Edit::SetBackground {
                background: Some(pf_model::Background::new("/photos/house.jpg", 0.0, 5.0, 10.0)),
            },
        ])
        .unwrap();
    let status = engine.output_status();
    assert!(status.running);
    assert_eq!(status.generation, generation, "layout-only edits");
    engine.undo();
    assert_eq!(engine.output_status().generation, generation);
}

#[test]
fn an_edit_that_introduces_errors_stops_output() {
    let (mut engine, _recorded, _prop, mut controller, _dir) = engine_with_show();
    engine.start_output(solid_red(), TargetSpec::Show).unwrap();
    controller.ports[0].max_pixels = Some(1);
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    let status = engine.output_status();
    assert!(!status.running);
    let reason = status.stop_reason.clone().expect("a reason is given");
    assert!(
        reason.starts_with("Output stopped because the show now has errors"),
        "{reason}"
    );
    let json = serde_json::to_value(&status).unwrap();
    assert!(json["stopReason"].is_string());
    // A deliberate stop clears the reason.
    assert_eq!(engine.stop_output().stop_reason, None);
    assert_eq!(engine.output_status().stop_reason, None);
}

#[test]
fn a_target_with_no_pixels_is_refused_and_one_that_empties_stops_output() {
    let (mut engine, _recorded, prop, controller, _dir) = engine_with_show();
    let nowhere = TargetSpec::Port {
        controller: controller.id,
        port: 9,
    };
    let err = engine.start_output(solid_red(), nowhere).unwrap_err();
    assert!(matches!(err, EngineError::NothingToLight));
    assert_eq!(
        err.to_string(),
        "The chosen target has no pixels to light. Wire props to it first."
    );
    assert!(!engine.output_status().running);

    let on_prop = TargetSpec::Prop { id: prop.id };
    engine.start_output(solid_red(), on_prop).unwrap();
    engine.apply(vec![Edit::RemoveProp { id: prop.id }]).unwrap();
    let status = engine.output_status();
    assert!(!status.running);
    assert!(status.stop_reason.is_some());
}

#[test]
fn opening_another_show_stops_output() {
    let (mut engine, _recorded, _prop, _controller, dir) = engine_with_show();
    let path = dir.path().join("x.pixelflow.json");
    engine.save_as(&path).unwrap();
    engine.start_output(solid_red(), TargetSpec::Show).unwrap();
    engine.open(&path).unwrap();
    assert!(!engine.output_status().running);
}
