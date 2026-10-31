//! DMX output end to end: fixtures over Art-Net (real UDP on the loopback interface) and sACN,
//! and to a USB DMX interface (a pretend serial port). Nothing leaves this machine.

use pf_frame::frame_buffers;
use pf_model::{
    ArtNetConfig, Controller, DmxFixture, Generator, PortSlot, Prop, Protocol, SacnConfig, ShapeSource, Show,
    dmx_layout,
};
use pf_output::{
    ControllerState, FakeSerialPorts, OutputSettings, RecordingTransport, UdpTransport, artnet, build_plan,
    sacn, start_output, usb,
};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn flood(name: &str, layout: &str, address: u16) -> Prop {
    let mut prop = Prop::new(
        name,
        ShapeSource::Generator(Generator::Line {
            nodes: 1,
            length: 0.5,
        }),
    );
    prop.dmx = Some(DmxFixture::from_layout(dmx_layout(layout).unwrap(), 1, address));
    prop
}

/// Two floods on one universe of a node at `address` sent `protocol`, the first warm orange.
fn show(address: &str, protocol: Protocol) -> (Show, Vec<u8>) {
    let a = flood("Left", "dimmer-rgb", 1);
    let b = flood("Right", "rgbw", 101);
    let mut node = Controller::dmx("Node", address, protocol, 1);
    node.ports[0].slots = vec![PortSlot::new(a.id), PortSlot::new(b.id)];
    let mut show = Show::new("dmx");
    show.settings.frame_rate = 100;
    show.props = vec![a, b];
    show.controllers = vec![node];
    let (map, _) = pf_mapping::map_show(&show);
    let mut frame = vec![0u8; map.frame_len];
    frame[..6].copy_from_slice(&[128, 64, 0, 255, 255, 255]);
    map.write_fixtures(&mut frame);
    (show, frame)
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn art_net_frames_arrive_over_udp_as_whole_universes() {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (show, frame) = show(
        &receiver.local_addr().unwrap().to_string(),
        Protocol::ArtNet(ArtNetConfig {
            start_universe: Some(7),
            ..ArtNetConfig::default()
        }),
    );
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));

    let mut buf = [0u8; 1500];
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    handle.stop();
    assert_eq!(n, artnet::HEADER_LEN + 512);
    assert_eq!(&buf[..8], b"Art-Net\0");
    assert_eq!(buf[14], 7, "universe 7");
    let dmx = &buf[artnet::HEADER_LEN..n];
    // Left at 1: dimmer at half, the orange at full. Right at 101: white.
    assert_eq!(&dmx[..4], &[128, 255, 128, 0]);
    assert_eq!(&dmx[100..104], &[0, 0, 0, 255]);
    assert!(dmx[4..100].iter().all(|&b| b == 0));
    assert!(dmx[104..].iter().all(|&b| b == 0));
}

#[test]
fn sacn_sends_a_dmx_universe_whole_and_blacks_it_out_on_stop() {
    let (show, frame) = show("127.0.0.3", Protocol::Sacn(SacnConfig::default()));
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let (transport, recorded) = RecordingTransport::new();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| !recorded.lock().unwrap().is_empty());
    handle.stop();
    let packets = recorded.lock().unwrap();
    let first = &packets[0].0;
    assert_eq!(first.len(), sacn::DATA_HEADER_LEN + 512, "a whole universe");
    assert_eq!(
        &first[sacn::DATA_HEADER_LEN..sacn::DATA_HEADER_LEN + 4],
        &[128, 255, 128, 0]
    );
    let black = packets
        .iter()
        .rev()
        .find(|(p, _)| p[112] & 0x40 == 0)
        .expect("a black frame before the stream ends");
    assert!(black.0[sacn::DATA_HEADER_LEN..].iter().all(|&b| b == 0));
}

#[test]
fn a_usb_interface_gets_pro_messages_and_black_on_stop() {
    let (show, frame) = show("/dev/cu.usbserial-TEST", Protocol::DmxUsb);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let ports = FakeSerialPorts::default();
    let written = std::sync::Arc::clone(&ports.written);
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
        .unwrap()
        .with_serial(Box::new(ports));
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| !written.lock().unwrap().is_empty());
    let stats = handle.stop();
    assert_eq!(stats.controllers[0].last_error, None);
    let written = written.lock().unwrap();
    let (port, message) = &written[0];
    assert_eq!(port, "/dev/cu.usbserial-TEST");
    assert_eq!(message.len(), usb::PRO_MESSAGE_LEN);
    assert_eq!(&message[..5], &[0x7E, 6, 0x01, 0x02, 0]);
    assert_eq!(&message[5..9], &[128, 255, 128, 0]);
    assert_eq!(*message.last().unwrap(), 0xE7);
    let (_, last) = written.last().unwrap();
    assert!(last[5..517].iter().all(|&b| b == 0), "black at the end");
}

#[test]
fn a_usb_interface_is_sent_no_faster_than_the_dmx_line_carries() {
    // A show at 100 frames a second: the interface gets about 44 messages a second.
    let (show, frame) = show("COM3", Protocol::DmxUsb);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let ports = FakeSerialPorts::default();
    let written = std::sync::Arc::clone(&ports.written);
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
        .unwrap()
        .with_serial(Box::new(ports));
    let started = Instant::now();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    std::thread::sleep(Duration::from_millis(500));
    let sent = written.lock().unwrap().len();
    let elapsed = started.elapsed();
    let stats = handle.stop();
    // At most one per gap, plus the first and one caught up.
    let most = (elapsed.as_secs_f64() / usb::MIN_GAP.as_secs_f64()) as usize + 2;
    assert!(sent <= most, "{sent} messages in {elapsed:?}, at most {most}");
    assert!(
        (sent as u64) < stats.frames,
        "{sent} messages for {} frames",
        stats.frames
    );
}

#[test]
fn an_interface_unplugged_mid_show_is_reported_and_picked_up_again_when_plugged_back_in() {
    let (show, frame) = show("/dev/cu.usbserial-TEST", Protocol::DmxUsb);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let ports = FakeSerialPorts::default();
    let written = std::sync::Arc::clone(&ports.written);
    let unplugged = std::sync::Arc::clone(&ports.unplugged);
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
        .unwrap()
        .with_serial(Box::new(ports));
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| !written.lock().unwrap().is_empty());

    unplugged.lock().unwrap().push("/dev/cu.usbserial-TEST".into());
    wait_until(|| {
        handle
            .stats()
            .controllers
            .first()
            .is_some_and(|c| c.state == ControllerState::Degraded && c.last_error.is_some())
    });
    let error = handle.stats().controllers[0].last_error.clone().unwrap();
    assert!(
        error.starts_with(
            "the USB DMX interface on /dev/cu.usbserial-TEST stopped answering: was it unplugged?"
        ),
        "{error}"
    );

    // Back in: the next try opens it again and the error clears.
    let before = written.lock().unwrap().len();
    unplugged.lock().unwrap().clear();
    wait_until(|| {
        handle
            .stats()
            .controllers
            .first()
            .is_some_and(|c| c.state == ControllerState::Ok && c.last_error.is_none())
    });
    assert!(written.lock().unwrap().len() > before);
    handle.stop();
}

/// A 16-bit moving head at address 20 of a node at `address`, its fixture's channels set to
/// point it 90° right and 45° forward, lit red at full: what a DMX effect or a test by hand
/// leaves in the frame.
fn head_show(address: &str, protocol: Protocol) -> (Show, Vec<u8>, Vec<u8>) {
    let head = flood("Spot", "head-rgbw-16", 20);
    let fixture = head.dmx.clone().unwrap();
    let mut node = Controller::dmx("Node", address, protocol, 1);
    node.ports[0].slots = vec![PortSlot::new(head.id)];
    let mut show = Show::new("heads");
    show.settings.frame_rate = 100;
    show.props = vec![head];
    show.controllers = vec![node];
    let (map, _) = pf_mapping::map_show(&show);
    let layout = map.fixture(show.props[0].id).unwrap();
    let axes = fixture.axes();
    let mut values: Vec<Option<u8>> = vec![None; fixture.channels.len()];
    for (axis, degrees) in axes.iter().zip([90.0, 45.0]) {
        let (coarse, fine) = axis.values(degrees);
        values[axis.coarse] = Some(coarse);
        values[axis.fine.unwrap()] = fine;
    }
    let mut frame = vec![0u8; map.frame_len];
    frame[layout.cells_offset] = 255;
    layout.write(&mut frame, &values);
    let sent = frame[layout.dmx_offset..layout.dmx_offset + fixture.channels.len()].to_vec();
    (show, frame, sent)
}

#[test]
fn a_moving_head_is_sent_its_position_and_colour_and_stop_leaves_it_where_it_points() {
    let (show, frame, sent) = head_show("127.0.0.3", Protocol::Sacn(SacnConfig::default()));
    // Pan 90° of 540° and tilt 45° of 270°, each from Home in the middle: 43690 (170, 170).
    // Then speed, the dimmer at full, strobe, and red.
    assert_eq!(sent, vec![170, 170, 170, 170, 0, 255, 0, 255, 0, 0, 0]);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    assert_eq!(
        plan.controllers[0].holds,
        vec![19, 20, 21, 22],
        "pan, pan fine, tilt, tilt fine"
    );
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let (transport, recorded) = RecordingTransport::new();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| !recorded.lock().unwrap().is_empty());
    handle.stop();
    let packets = recorded.lock().unwrap();
    let dmx = |p: &[u8]| p[sacn::DATA_HEADER_LEN..].to_vec();
    assert_eq!(&dmx(&packets[0].0)[19..30], &sent[..]);
    let black = packets
        .iter()
        .rev()
        .find(|(p, _)| p[112] & 0x40 == 0)
        .map(|(p, _)| dmx(p))
        .expect("a black frame before the stream ends");
    assert_eq!(
        &black[19..23],
        &[170, 170, 170, 170],
        "the head stays where it points"
    );
    assert!(black[23..].iter().all(|&b| b == 0), "its light goes out");
    assert!(black[..19].iter().all(|&b| b == 0));
}

#[test]
fn a_moving_head_on_art_net_and_usb_gets_the_same_channels() {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (show, frame, sent) = head_show(
        &receiver.local_addr().unwrap().to_string(),
        Protocol::ArtNet(ArtNetConfig::default()),
    );
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    let mut buf = [0u8; 1500];
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    handle.stop();
    assert_eq!(&buf[artnet::HEADER_LEN..n][19..30], &sent[..]);

    let (show, frame, sent) = head_show("/dev/cu.usbserial-TEST", Protocol::DmxUsb);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let ports = FakeSerialPorts::default();
    let written = std::sync::Arc::clone(&ports.written);
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
        .unwrap()
        .with_serial(Box::new(ports));
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| !written.lock().unwrap().is_empty());
    handle.stop();
    let written = written.lock().unwrap();
    // The Pro message's data starts after its 5-byte header and the start code.
    assert_eq!(&written[0].1[5 + 19..5 + 30], &sent[..]);
    let (_, last) = written.last().unwrap();
    assert_eq!(
        &last[5 + 19..5 + 23],
        &[170, 170, 170, 170],
        "stop leaves the head where it is"
    );
    assert!(last[5 + 23..517].iter().all(|&b| b == 0));
}

#[test]
fn a_head_on_a_pixel_controllers_dmx_output_is_held_at_its_own_channels() {
    // 100 RGB pixels, then a 16-bit head right after them on the same port (channels 301-311).
    let pixels = Prop::new(
        "Eaves",
        ShapeSource::Generator(Generator::Line {
            nodes: 100,
            length: 10.0,
        }),
    );
    let head = flood("Spot", "head-rgbw-16", 301);
    let mut falcon = Controller::new("Falcon", "192.0.2.22", Protocol::Sacn(SacnConfig::default()));
    falcon.ports.push(pf_model::Port::new(1));
    falcon.ports[0].slots = vec![PortSlot::new(pixels.id), PortSlot::new(head.id)];
    let mut show = Show::new("mixed");
    show.props = vec![pixels, head];
    show.controllers = vec![falcon];
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    assert_eq!(plan.controllers[0].holds, vec![300, 301, 302, 303]);
    let mut buffer = vec![9u8; 311];
    plan.controllers[0].blacken(&mut buffer);
    assert!(buffer[..300].iter().all(|&b| b == 0), "the pixels go dark");
    assert_eq!(&buffer[300..304], &[9, 9, 9, 9], "the head's pan and tilt stay");
    assert!(buffer[304..].iter().all(|&b| b == 0), "its light goes out");
}

#[test]
fn a_sequence_played_straight_through_leaves_a_head_where_it_points_at_a_stop() {
    let (show, _, _) = head_show("192.0.2.30", Protocol::Sacn(SacnConfig::default()));
    let (map, _) = pf_mapping::map_show(&show);
    let node = &show.controllers[0];
    let route = pf_output::PassthroughRoute {
        id: node.id,
        name: node.name.clone(),
        address: node.address.clone(),
        protocol: node.protocol,
        start: 1000,
        count: 512,
        ddp_offset_base: 0,
    };
    let mut plan = pf_output::build_passthrough_plan(&[route], 2000, 40);
    assert!(plan.controllers[0].holds.is_empty());
    pf_output::hold_motors(&mut plan, &map);
    assert_eq!(plan.controllers[0].holds, vec![19, 20, 21, 22]);
}
