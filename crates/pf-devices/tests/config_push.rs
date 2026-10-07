//! "Send setup" and "Compare" against fake FPP and WLED devices answering real HTTP on
//! 127.0.0.1, built from recorded and documented responses. No real device is contacted.

use pf_devices::adapter::{
    DeviceAdapter, FalconAdapter, FppAdapter, SendStatus, WledAdapter, restore_setup, send_setup,
};
use pf_devices::setup::{ChangeKind, Setup, compare, show_setup, take_from_device};
use pf_devices::testing::{FakeFpp, FakeWled, SECRET_ENDPOINTS};
use pf_devices::{DeviceKind, HttpClient, plan_import};
use pf_model::{ColorOrder, Controller, Generator, Protocol, SacnConfig, ShapeSource, Show};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

fn http() -> HttpClient {
    HttpClient::new(Duration::from_secs(5))
}

/// The pixel hat's string outputs as FPP stores them (without the status key FPP adds).
fn hat_strings() -> Value {
    let mut doc: Value = serde_json::from_str(include_str!(
        "../fixtures/fpp-hat/api_channel_output_co-pixelStrings.json"
    ))
    .unwrap();
    doc.as_object_mut().unwrap().remove("status");
    doc
}

/// Imports a device into a new show, as the Controllers screen does.
fn import(adapter: &dyn DeviceAdapter, host: &str) -> (Show, Controller) {
    let device = adapter.probe(&http(), host).unwrap();
    let config = adapter.read_config(&http(), host).unwrap();
    let plan = plan_import(&device, &config, &Show::new("t"));
    let mut show = Show::new("t");
    show.props = plan.props;
    show.controllers.push(plan.controller.clone());
    (show, plan.controller)
}

fn set_line_nodes(show: &mut Show, name: &str, nodes: u32) {
    let prop = show.props.iter_mut().find(|p| p.name == name).unwrap();
    prop.shape = ShapeSource::Generator(Generator::Line {
        nodes,
        length: nodes as f32 * 0.05,
    });
}

fn ids(changes: &[pf_devices::setup::Change]) -> Vec<String> {
    changes.iter().map(|c| c.id.clone()).collect()
}

fn assert_never_asked_for_secrets(requests: &[String]) {
    for request in requests {
        for forbidden in SECRET_ENDPOINTS {
            assert!(!request.contains(forbidden), "requested {request}");
        }
        assert!(!request.contains("/api/network"), "requested {request}");
    }
}

/// Port 1: "Roof Line" (150 pixels, 1 null) then "Gutter" (50, GRB, reversed, 50%, gamma 2.2).
fn fake_hat() -> FakeFpp {
    FakeFpp::start().with_pixel_strings(hat_strings())
}

fn target(show: &Show, controller: &Controller, kind: DeviceKind) -> Setup {
    let controller = show
        .controllers
        .iter()
        .find(|c| c.id == controller.id)
        .unwrap_or(controller);
    show_setup(show, controller, pf_devices::setup::one_string_per_port(kind))
}

#[test]
fn fpp_read_import_plan_send_verify_and_restore() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    let names: Vec<_> = show.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Roof Line", "Gutter"]);

    // Freshly imported: the device already matches the show.
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter
        .plan_config(&snapshot, &target(&show, &controller, DeviceKind::Fpp))
        .unwrap();
    assert_eq!(ids(&plan.changes), Vec::<String>::new(), "{:#?}", plan.changes);
    assert!(plan.writes.is_empty());

    // The show changes: Gutter shrinks, Roof Line's controller color order is set, and a new
    // 20-pixel prop is wired after Gutter.
    set_line_nodes(&mut show, "Gutter", 30);
    show.controllers[0].ports[0].slots[0].controller_color_order = Some(ColorOrder::Bgr);
    let star = pf_model::Prop::new(
        "Star",
        ShapeSource::Generator(Generator::Line {
            nodes: 20,
            length: 1.0,
        }),
    );
    show.controllers[0].ports[0]
        .slots
        .push(pf_model::PortSlot::new(star.id));
    show.props.push(star);
    let wanted = target(&show, &controller, DeviceKind::Fpp);
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    let rows: Vec<_> = plan
        .changes
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.before.as_str(),
                c.after.as_str(),
                c.warning.is_some(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("port1/string1/colorOrder", "RGB", "BGR", false),
            ("port1/string2/pixels", "50", "30", true),
            ("port1/string3", "None", "20 pixels", false),
        ]
    );
    assert_eq!(plan.changes[1].kind, ChangeKind::Pixels);
    assert_eq!(plan.writes.len(), 1);
    // Nothing has been written yet.
    assert!(fpp.state().config_writes.is_empty());

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(
        outcome.report.status,
        SendStatus::Sent,
        "{}",
        outcome.report.message
    );
    assert!(outcome.report.mismatches.is_empty());
    assert!(!outcome.report.can_restore);
    let saved = fpp.state().pixel_strings.clone().unwrap();
    assert!(saved.get("status").is_none(), "FPP's status key isn't saved");
    let strings = &saved["channelOutputs"][0]["outputs"][0]["virtualStrings"];
    assert_eq!(strings[0]["colorOrder"], "BGR");
    assert_eq!(
        strings[0]["nullNodes"], 1,
        "the controller's own settings are kept"
    );
    assert_eq!(strings[1]["pixelCount"], 30);
    assert_eq!(strings[1]["reverse"], 1);
    assert_eq!(strings[1]["brightness"], 50);
    assert_eq!(strings[1]["gamma"], "2.2");
    assert_eq!(strings[1]["startChannel"], 450);
    assert_eq!(strings[2]["description"], "Star");
    assert_eq!(strings[2]["pixelCount"], 20);
    assert_eq!(strings[2]["startChannel"], 540);
    assert_eq!(strings[2]["colorOrder"], "RGB");
    // Port 2 (no strings in the show or on the FPP) is left as it was.
    assert_eq!(
        saved["channelOutputs"][0]["outputs"][1],
        hat_strings()["channelOutputs"][0]["outputs"][1]
    );

    // Read back, the FPP now matches.
    assert_eq!(adapter.verify_config(&http(), &host, &wanted).unwrap(), vec![]);

    // Putting the previous setup back restores the file exactly.
    let snapshot = outcome.snapshot.expect("a snapshot was taken before sending");
    let restored = restore_setup(&adapter, &http(), &host, &snapshot);
    assert!(restored.restored, "{}", restored.message);
    assert_eq!(fpp.state().pixel_strings.clone().unwrap(), hat_strings());
    assert_never_asked_for_secrets(&fpp.state().requests);
}

#[test]
fn fpp_failure_while_saving_offers_the_snapshot_back() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    set_line_nodes(&mut show, "Gutter", 30);
    let wanted = target(&show, &controller, DeviceKind::Fpp);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    fpp.state().fail_config_writes = Some((500, r#"{"status":"ERROR: Could not write file"}"#.into()));

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Failed);
    assert!(outcome.report.can_restore);
    assert!(
        outcome.report.message.contains("may have been only partly saved"),
        "{}",
        outcome.report.message
    );

    fpp.state().fail_config_writes = None;
    let restored = restore_setup(&adapter, &http(), &host, &outcome.snapshot.unwrap());
    assert!(restored.restored, "{}", restored.message);
    assert_eq!(fpp.state().pixel_strings.clone().unwrap(), hat_strings());
}

#[test]
fn fpp_that_ignores_the_new_setup_is_reported_as_a_mismatch() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    set_line_nodes(&mut show, "Gutter", 30);
    let wanted = target(&show, &controller, DeviceKind::Fpp);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    fpp.state().ignore_config_writes = true;

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Mismatch);
    assert!(outcome.report.can_restore);
    assert_eq!(ids(&outcome.report.mismatches), vec!["port1/string2/pixels"]);
    assert!(
        outcome.report.message.contains("doesn't match"),
        "{}",
        outcome.report.message
    );
}

#[test]
fn nothing_is_sent_when_the_snapshot_cant_be_read() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    set_line_nodes(&mut show, "Gutter", 30);
    let wanted = target(&show, &controller, DeviceKind::Fpp);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    fpp.state().fail_config_reads = Some(500);

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Refused);
    assert!(outcome.snapshot.is_none());
    assert!(!outcome.report.can_restore);
    assert!(
        outcome.report.message.contains("Nothing was sent"),
        "{}",
        outcome.report.message
    );
    assert!(fpp.state().config_writes.is_empty());
}

#[test]
fn nothing_is_sent_when_the_device_changed_since_the_plan_was_shown() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    set_line_nodes(&mut show, "Gutter", 30);
    let wanted = target(&show, &controller, DeviceKind::Fpp);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    // Someone edits the FPP's outputs on its own page meanwhile.
    fpp.state().pixel_strings.as_mut().unwrap()["channelOutputs"][0]["outputs"][0]["virtualStrings"][0]["pixelCount"] =
        json!(149);

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Refused);
    assert!(
        outcome.report.message.contains("changed since"),
        "{}",
        outcome.report.message
    );
    assert!(fpp.state().config_writes.is_empty());
}

#[test]
fn fpp_without_a_cape_has_nothing_to_set_up() {
    let fpp = FakeFpp::start();
    let err = FppAdapter.snapshot(&http(), fpp.address()).unwrap_err();
    assert!(err.to_string().contains("no pixel outputs"), "{err}");
}

#[test]
fn fpp_ports_the_cape_lacks_are_noted_not_sent() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (mut show, controller) = import(&adapter, &host);
    show.controllers[0].ports.push(pf_model::Port::new(9));
    let gutter = show.props[1].id;
    show.controllers[0].ports[1]
        .slots
        .push(pf_model::PortSlot::new(gutter));
    show.controllers[0].ports[0].slots.pop();
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter
        .plan_config(&snapshot, &target(&show, &controller, DeviceKind::Fpp))
        .unwrap();
    assert_eq!(ids(&plan.changes), vec!["port1/string2"]);
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("Port 9") && n.contains("2 ports")),
        "{:?}",
        plan.notes
    );
}

#[test]
fn fpp_status_says_when_it_is_playing() {
    let fpp = fake_hat();
    assert_eq!(FppAdapter.status(&http(), fpp.address()).unwrap().busy, None);
    fpp.state().play("Show.fseq");
    let busy = FppAdapter.status(&http(), fpp.address()).unwrap().busy.unwrap();
    assert!(busy.contains("playing Show.fseq"), "{busy}");
}

fn wled_adapter() -> WledAdapter {
    WledAdapter::with_settle(Duration::ZERO)
}

#[test]
fn wled_read_import_plan_send_verify_and_restore() {
    let wled = FakeWled::start();
    let host = wled.address().to_string();
    let adapter = wled_adapter();
    let original = wled.state().cfg.clone();
    let (mut show, controller) = import(&adapter, &host);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter
        .plan_config(&snapshot, &target(&show, &controller, DeviceKind::Wled))
        .unwrap();
    assert_eq!(ids(&plan.changes), Vec::<String>::new(), "{:#?}", plan.changes);

    // Output 1 grows to 80 pixels and takes RGB; output 2 follows it.
    set_line_nodes(&mut show, "Porch WLED Port 1", 80);
    show.controllers[0].ports[0].slots[0].controller_color_order = Some(ColorOrder::Rgb);
    let wanted = target(&show, &controller, DeviceKind::Wled);
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    let rows: Vec<_> = plan
        .changes
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.what.as_str(),
                c.before.as_str(),
                c.after.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("port1/string1/pixels", "Pixels", "60", "80"),
            ("port1/string1/colorOrder", "Color order", "GRB", "RGB"),
            ("port2/firstLed", "First LED", "60", "80"),
        ]
    );
    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(
        outcome.report.status,
        SendStatus::Sent,
        "{}",
        outcome.report.message
    );

    let state = wled.state();
    let ins = &state.cfg["hw"]["led"]["ins"];
    assert_eq!(
        (ins[0]["len"].clone(), ins[0]["order"].clone()),
        (json!(80), json!(1))
    );
    assert_eq!(
        (ins[1]["start"].clone(), ins[1]["len"].clone()),
        (json!(80), json!(60))
    );
    assert_eq!(ins[0]["skip"], 1, "the controller's own settings are kept");
    assert_eq!(state.cfg["hw"]["led"]["total"], 140);
    // Network and security settings are never sent; settings WLED resets when they're missing
    // are sent back as they were.
    for write in &state.config_writes {
        for key in ["ap", "id", "um", "eth", "wifi", "ota"] {
            assert!(write.get(key).is_none(), "sent {key}: {write}");
        }
        assert_eq!(write["nw"], json!({"linked_remote": ["aabbccddeeff"]}), "{write}");
        assert_eq!(write["hw"]["led"]["fps"], 42);
        assert_eq!(write["light"]["gc"]["bri"], 1);
        assert!(write.get("rb").is_none(), "never asks WLED to reboot");
    }
    drop(state);
    assert_eq!(adapter.verify_config(&http(), &host, &wanted).unwrap(), vec![]);

    let restored = restore_setup(&adapter, &http(), &host, &outcome.snapshot.unwrap());
    assert!(restored.restored, "{}", restored.message);
    let state = wled.state();
    assert_eq!(state.cfg["hw"], original["hw"]);
    assert_eq!(state.cfg["if"], original["if"]);
    assert_never_asked_for_secrets(&state.requests);
    assert!(!state.requests.iter().any(|r| r.contains("wsec")));
}

#[test]
fn wled_failure_partway_offers_the_snapshot_back() {
    let wled = FakeWled::start();
    let host = wled.address().to_string();
    let adapter = wled_adapter();
    let original = wled.state().cfg.clone();
    let (mut show, controller) = import(&adapter, &host);
    // New pixel count (the LED outputs) and sACN from universe 7 (the receive settings): two
    // saves, and the second one fails.
    set_line_nodes(&mut show, "Porch WLED Port 1", 80);
    show.controllers[0].protocol = Protocol::Sacn(SacnConfig {
        start_universe: Some(7),
        ..SacnConfig::default()
    });
    let wanted = target(&show, &controller, DeviceKind::Wled);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    assert!(
        ids(&plan.changes).contains(&"input/startUniverse".to_string()),
        "{:?}",
        ids(&plan.changes)
    );
    assert_eq!(plan.writes.len(), 2);
    wled.state().fail_writes_from = Some(2);

    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Failed);
    assert!(outcome.report.can_restore);
    assert!(
        outcome.report.message.contains("1 of 2"),
        "{}",
        outcome.report.message
    );
    assert_eq!(
        wled.state().cfg["hw"]["led"]["ins"][0]["len"],
        80,
        "the first save landed"
    );

    wled.state().fail_writes_from = None;
    let restored = restore_setup(&adapter, &http(), &host, &outcome.snapshot.unwrap());
    assert!(restored.restored, "{}", restored.message);
    assert_eq!(wled.state().cfg["hw"], original["hw"]);
}

#[test]
fn wled_that_ignores_the_new_setup_is_reported_as_a_mismatch() {
    let wled = FakeWled::start();
    let host = wled.address().to_string();
    let adapter = wled_adapter();
    let (mut show, controller) = import(&adapter, &host);
    set_line_nodes(&mut show, "Porch WLED Port 1", 80);
    let wanted = target(&show, &controller, DeviceKind::Wled);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter.plan_config(&snapshot, &wanted).unwrap();
    wled.state().ignore_writes = true;
    let outcome = send_setup(&adapter, &http(), &host, &snapshot, &wanted, &ids(&plan.changes));
    assert_eq!(outcome.report.status, SendStatus::Mismatch);
    assert!(ids(&outcome.report.mismatches).contains(&"port1/string1/pixels".to_string()));
}

#[test]
fn wled_turns_on_realtime_receive_and_leaves_outputs_it_cannot_add() {
    let wled = FakeWled::start();
    let host = wled.address().to_string();
    wled.state().cfg["if"]["live"]["en"] = json!(false);
    let adapter = wled_adapter();
    let (mut show, controller) = import(&adapter, &host);
    // A third output the WLED doesn't have.
    let id = show.props[0].id;
    let mut three = pf_model::Port::new(3);
    three.slots.push(pf_model::PortSlot::new(id));
    show.controllers[0].ports.push(three);
    let snapshot = adapter.snapshot(&http(), &host).unwrap();
    let plan = adapter
        .plan_config(&snapshot, &target(&show, &controller, DeviceKind::Wled))
        .unwrap();
    let rows: Vec<_> = plan
        .changes
        .iter()
        .map(|c| (c.id.as_str(), c.before.as_str(), c.after.as_str()))
        .collect();
    assert_eq!(rows, vec![("input/realtime", "Off", "On")]);
    assert!(
        plan.notes.iter().any(|n| n.contains("Output 3")),
        "{:?}",
        plan.notes
    );
}

#[test]
fn falcon_can_be_compared_but_not_sent_to_yet() {
    let reason = FalconAdapter.can_send().unwrap_err();
    assert!(reason.contains("Falcon"), "{reason}");
}

#[test]
fn compare_then_take_the_device_differences_into_the_show() {
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    let adapter = FppAdapter;
    let (show, controller) = import(&adapter, &host);
    // On the FPP's own page, Gutter grows to 60 pixels.
    fpp.state().pixel_strings.as_mut().unwrap()["channelOutputs"][0]["outputs"][0]["virtualStrings"][1]["pixelCount"] =
        json!(60);
    let config = adapter.read_config(&http(), &host).unwrap();
    let comparison = compare(&show, &controller, DeviceKind::Fpp, &config);
    assert_eq!(ids(&comparison.changes), vec!["port1/string2/pixels"]);
    let taken = take_from_device(
        &show,
        &controller,
        DeviceKind::Fpp,
        &config,
        &ids(&comparison.changes),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(taken.changed_props[0].node_count(), 60);
    // Nothing was written to the FPP.
    assert!(fpp.state().config_writes.is_empty());
}
