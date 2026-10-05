//! A real Falcon setup, imported and played: smart receivers sharing a port number must not be
//! summed against one port's limit, and nothing about a port's pixel count may stop output.

use pf_devices::testing::{FALCON, falcon_query, network};
use pf_devices::{identify, plan_import, read_config};
use pf_engine::{Edit, Engine, PatternSpec, TargetSpec};
use pf_model::IssueCode;
use pf_output::{RecordingTransport, Transport};
use std::time::{Duration, Instant};

/// Port 17 (`p` 16) feeds smart receivers A, B and C with 600 pixels each: legal on an F16V5.
fn receivers_on_one_port(pixels: u32) -> String {
    let string = |r: u32, s: u32| {
        format!(
            r#"{{"p":16,"s":{s},"r":{r},"v":0,"u":0,"sc":{},"n":{pixels},"z":0,"ns":0,"ne":0,"g":10,"o":0,"b":100,"gp":1,"nm":"Receiver {r}","bl":0,"l":14}}"#,
            (r - 1) * pixels * 3
        )
    };
    format!(
        r#"{{"R":200,"T":"Q","F":1,"B":0,"M":"SP","RB":0,"P":{{"A":[{},{},{}]}},"W":" ","L":""}}"#,
        string(1, 0),
        string(2, 0),
        string(3, 0)
    )
}

fn import(pixels: u32) -> (Engine, pf_output::Recorded, tempfile::TempDir) {
    let http = network().with_post(
        FALCON,
        "/api",
        &falcon_query("SP", 0),
        &receivers_on_one_port(pixels),
    );
    let device = identify(&http, FALCON, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    let plan = plan_import(&device, &config, &pf_model::Show::new("t"));
    assert_eq!(plan.controller.ports.len(), 1);
    let port = &plan.controller.ports[0];
    assert_eq!(
        (port.number, port.max_pixels, port.slots.len()),
        (17, Some(1024), 3)
    );

    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = RecordingTransport::new();
    let mut engine =
        Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn Transport>));
    let mut edits: Vec<Edit> = plan
        .props
        .into_iter()
        .map(|prop| Edit::AddProp { prop })
        .collect();
    edits.push(Edit::AddController {
        controller: plan.controller,
    });
    engine.apply(edits).unwrap();
    (engine, recorded, dir)
}

fn start_and_send(engine: &mut Engine, recorded: &pf_output::Recorded) {
    let pattern: PatternSpec =
        serde_json::from_value(serde_json::json!({ "kind": "solid", "color": "ff0000" })).unwrap();
    let status = engine.start_output(pattern, TargetSpec::Show).unwrap();
    assert!(status.running);
    let deadline = Instant::now() + Duration::from_secs(5);
    while recorded.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "no packets sent");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn several_smart_receivers_on_one_port_import_cleanly_and_play() {
    let (mut engine, recorded, _dir) = import(600);
    let snapshot = engine.snapshot();
    assert!(
        !snapshot
            .issues
            .iter()
            .any(|i| i.code == IssueCode::PortOverCapacity),
        "{:?}",
        snapshot.issues
    );
    start_and_send(&mut engine, &recorded);
}

#[test]
fn a_receiver_over_the_limit_is_reported_but_still_plays() {
    let (mut engine, recorded, _dir) = import(1100);
    let snapshot = engine.snapshot();
    let over: Vec<_> = snapshot
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::PortOverCapacity)
        .collect();
    assert_eq!(over.len(), 3, "one per receiver: {:?}", snapshot.issues);
    assert!(over.iter().all(|i| i.severity == pf_model::Severity::Warning));
    start_and_send(&mut engine, &recorded);
}
