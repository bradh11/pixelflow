//! The output thread end to end, with packets captured in memory.

use pf_frame::frame_buffers;
use pf_model::{
    ColorOrder, Controller, Generator, Port, PortSlot, Prop, Protocol, SacnConfig, ShapeSource, Show,
};
use pf_output::{
    ControllerState, OutputSettings, Recorded, RecordingTransport, build_plan, ddp, sacn, start_output,
};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const DDP_DEST: &str = "127.0.0.1:4048";
const SACN_DEST: &str = "127.0.0.2:5568";

/// A DDP controller with a 2-pixel GRB prop, and an sACN controller with a 2-pixel RGB prop.
fn show() -> Show {
    let mut show = Show::new("engine");
    show.settings.frame_rate = 100;
    let mut a = Prop::new(
        "A",
        ShapeSource::Generator(Generator::Line {
            nodes: 2,
            length: 1.0,
        }),
    );
    a.color_order = ColorOrder::Grb;
    let b = Prop::new(
        "B",
        ShapeSource::Generator(Generator::Line {
            nodes: 2,
            length: 1.0,
        }),
    );
    let mut ddp = Controller::new("WLED", "127.0.0.1", Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(a.id));
    ddp.ports.push(port);
    let mut sacn = Controller::new("FPP", "127.0.0.2", Protocol::Sacn(SacnConfig::default()));
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(b.id));
    sacn.ports.push(port);
    show.props = vec![a, b];
    show.controllers = vec![ddp, sacn];
    show
}

fn settings() -> OutputSettings {
    OutputSettings {
        cid: [1; 16],
        ..OutputSettings::default()
    }
}

/// Packets sent to `dest`, oldest first.
fn sent_to(recorded: &Recorded, dest: &str) -> Vec<Vec<u8>> {
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
fn sends_latest_frame_to_every_controller_then_blacks_out_on_stop() {
    let show = show();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    // A: (10,20,30) (40,50,60); B: (1,2,3) (4,5,6)
    writer
        .frame_mut()
        .copy_from_slice(&[10, 20, 30, 40, 50, 60, 1, 2, 3, 4, 5, 6]);
    writer.publish();

    let (transport, recorded) = RecordingTransport::new();
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    wait_until(|| sent_to(&recorded, SACN_DEST).len() >= 3);
    let stats = handle.stop();

    let ddp_packets = sent_to(&recorded, DDP_DEST);
    let first = &ddp_packets[0];
    assert_eq!(first[0], 0x41, "single DDP packet carries push");
    assert_eq!(&first[ddp::HEADER_LEN..], &[20, 10, 30, 50, 40, 60], "GRB order");
    let sacn_packets = sent_to(&recorded, SACN_DEST);
    assert_eq!(&sacn_packets[0][sacn::DATA_HEADER_LEN..], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(&sacn_packets[0][113..115], &[0, 1], "universe 1");
    assert_eq!(sacn_packets[1][111], sacn_packets[0][111].wrapping_add(1));

    let last_ddp = ddp_packets.last().unwrap();
    assert!(
        last_ddp[ddp::HEADER_LEN..].iter().all(|&b| b == 0),
        "blackout on stop"
    );
    let last_sacn = sacn_packets.last().unwrap();
    assert!(last_sacn[sacn::DATA_HEADER_LEN..].iter().all(|&b| b == 0));

    assert!(stats.frames >= 3);
    assert_eq!(stats.controllers.len(), 2);
    assert!(stats.controllers.iter().all(|c| c.state == ControllerState::Ok));
    assert_eq!(stats.controllers[1].packets_sent, sacn_packets.len() as u64);
}

#[test]
fn a_failing_controller_degrades_without_stopping_the_others() {
    let show = show();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (_writer, reader) = frame_buffers(plan.frame_len);
    let (transport, recorded) = RecordingTransport::new();
    let transport = transport.fail(DDP_DEST.parse().unwrap());
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    wait_until(|| sent_to(&recorded, SACN_DEST).len() >= 20);
    let stats = handle.stop();

    let wled = &stats.controllers[0];
    assert_eq!(wled.state, ControllerState::Degraded);
    assert_eq!(wled.packets_sent, 0);
    assert!(wled.send_errors >= 1);
    // Backoff: at 100 fps for 200 ms+, a failing controller is retried only a few times.
    assert!(wled.send_errors < 10, "retried {} times", wled.send_errors);
    assert_eq!(wled.last_error.as_deref(), Some("host unreachable"));
    assert_eq!(stats.controllers[1].state, ControllerState::Ok);
}

#[test]
fn unresolvable_controllers_are_reported_and_skipped() {
    let mut show = show();
    show.controllers[0].address = "bad:port".into();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (_writer, reader) = frame_buffers(plan.frame_len);
    let (transport, recorded) = RecordingTransport::new();
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    wait_until(|| sent_to(&recorded, SACN_DEST).len() >= 2);
    let stats = handle.stop();
    assert_eq!(stats.controllers[0].state, ControllerState::Unresolved);
    assert!(
        stats.controllers[0]
            .last_error
            .as_deref()
            .unwrap()
            .contains("could not resolve")
    );
}

#[test]
fn sync_universe_adds_a_sync_packet_per_frame() {
    let show = show();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (_writer, reader) = frame_buffers(plan.frame_len);
    let (transport, recorded) = RecordingTransport::new();
    let settings = OutputSettings {
        sync_universe: Some(500),
        ..settings()
    };
    let handle = start_output(plan, settings, reader, Box::new(transport));
    wait_until(|| sent_to(&recorded, "239.255.1.244:5568").len() >= 2);
    handle.stop();
    let data = sent_to(&recorded, SACN_DEST);
    assert_eq!(&data[0][109..111], &500u16.to_be_bytes());
    let sync = sent_to(&recorded, "239.255.1.244:5568");
    assert_eq!(sync[0].len(), sacn::SYNC_PACKET_LEN);
}

#[test]
fn blackout_reaches_a_controller_that_is_backing_off() {
    let show = show();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().fill(200);
    writer.publish();
    let (transport, recorded) = RecordingTransport::new();
    let transport = transport.fail(DDP_DEST.parse().unwrap());
    let failures = transport.failures();
    let failed_sends = transport.failed_sends();
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    // The first send fails; the next retry is not due for 250 ms.
    wait_until(|| failed_sends.load(Ordering::Relaxed) >= 1);
    failures.lock().unwrap().clear();
    handle.stop();

    // Nothing but the forced blackout can have reached the controller.
    let ddp_packets = sent_to(&recorded, DDP_DEST);
    assert!(
        !ddp_packets.is_empty(),
        "blackout must reach a backing-off controller"
    );
    for packet in &ddp_packets {
        assert!(packet[ddp::HEADER_LEN..].iter().all(|&b| b == 0), "{packet:?}");
    }
}

#[test]
fn transient_send_errors_do_not_degrade_a_controller() {
    let show = show();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (_writer, reader) = frame_buffers(plan.frame_len);
    let (transport, recorded) = RecordingTransport::new();
    let transport = transport.would_block(DDP_DEST.parse().unwrap());
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    std::thread::sleep(Duration::from_millis(100));
    let stats = handle.stop();

    let wled = &stats.controllers[0];
    assert_eq!(wled.state, ControllerState::Ok);
    assert!(wled.send_errors > 0);
    assert_eq!(wled.packets_sent, 0);
    assert_eq!(
        wled.last_error.as_deref(),
        Some("send buffer full; packets dropped")
    );
    let fpp = &stats.controllers[1];
    assert_eq!(fpp.state, ControllerState::Ok);
    assert!(fpp.packets_sent > 0);
    assert!(!sent_to(&recorded, SACN_DEST).is_empty());
}

#[test]
fn stats_are_available_immediately() {
    let mut show = show();
    // At 1 fps, publishing stats only every 10 frames would take ~10 s, past the wait_until deadline.
    show.settings.frame_rate = 1;
    show.controllers[0].address = "bad:port".into();
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (_writer, reader) = frame_buffers(plan.frame_len);
    let (transport, _recorded) = RecordingTransport::new();
    let handle = start_output(plan, settings(), reader, Box::new(transport));
    wait_until(|| handle.stats().controllers.len() == 2);
    let stats = handle.stats();
    assert_eq!(stats.controllers.len(), 2);
    assert_eq!(stats.controllers[0].state, ControllerState::Unresolved);
    handle.stop();
}
