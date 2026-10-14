//! DMX output end to end: fixtures over Art-Net (real UDP on the loopback interface) and sACN,
//! and to a USB DMX interface (a pretend serial port). Nothing leaves this machine.

use pf_frame::frame_buffers;
use pf_model::{
    ArtNetConfig, Controller, DmxFixture, Generator, PortSlot, Prop, Protocol, SacnConfig, ShapeSource, Show,
    dmx_layout,
};
use pf_output::{
    FakeSerialPorts, OutputSettings, RecordingTransport, UdpTransport, artnet, build_plan, sacn,
    start_output, usb,
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
fn without_a_serial_library_a_usb_interface_says_why_nothing_is_sent() {
    let (show, frame) = show("COM3", Protocol::DmxUsb);
    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer.frame_mut().copy_from_slice(&frame);
    writer.publish();
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));
    wait_until(|| {
        handle
            .stats()
            .controllers
            .first()
            .is_some_and(|c| c.last_error.is_some())
    });
    let stats = handle.stop();
    let error = stats.controllers[0].last_error.clone().unwrap();
    assert!(error.contains("can't open USB DMX interfaces yet"), "{error}");
}
