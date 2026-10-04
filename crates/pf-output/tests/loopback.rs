//! Real UDP output to a receiver on the loopback interface.

use pf_frame::frame_buffers;
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource, Show};
use pf_output::{OutputSettings, UdpTransport, build_plan, ddp, start_output};
use std::net::UdpSocket;
use std::time::Duration;

#[test]
fn ddp_frames_arrive_over_udp() {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let address = receiver.local_addr().unwrap().to_string();

    let mut show = Show::new("loopback");
    let prop = Prop::new(
        "A",
        ShapeSource::Generator(Generator::Line {
            nodes: 3,
            length: 1.0,
        }),
    );
    let mut controller = Controller::new("Local", address, Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    controller.ports.push(port);
    show.props.push(prop);
    show.controllers.push(controller);

    let (map, _) = pf_mapping::map_show(&show);
    let plan = build_plan(&show, &map);
    let (mut writer, reader) = frame_buffers(plan.frame_len);
    writer
        .frame_mut()
        .copy_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255]);
    writer.publish();
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let handle = start_output(plan, OutputSettings::default(), reader, Box::new(transport));

    let mut buf = [0u8; 1500];
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    handle.stop();
    assert_eq!(n, ddp::HEADER_LEN + 9);
    assert_eq!(&buf[ddp::HEADER_LEN..n], &[255, 0, 0, 0, 255, 0, 0, 0, 255]);
}
