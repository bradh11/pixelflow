//! DMX fixtures on DMX controllers: universes, addresses, and the fixtures' channels in the frame.

use pf_mapping::{Addressing, ChannelAddress, map_show};
use pf_model::{
    ArtNetConfig, Controller, DmxFixture, Generator, IssueCode, PortSlot, Prop, Protocol, SacnConfig,
    ShapeSource, Show, dmx_layout,
};

fn line(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    )
}

fn flood(name: &str, layout: &str, address: u16) -> Prop {
    let mut prop = line(name, 1);
    prop.dmx = Some(DmxFixture::from_layout(dmx_layout(layout).unwrap(), 1, address));
    prop
}

fn node(protocol: Protocol, universes: u16, slots: &[(usize, &Prop)]) -> Controller {
    let mut c = Controller::dmx("Node", "192.0.2.20", protocol, universes);
    for &(port, prop) in slots {
        c.ports[port].slots.push(PortSlot::new(prop.id));
    }
    c
}

fn art_net() -> Protocol {
    Protocol::ArtNet(ArtNetConfig::default())
}

#[test]
fn fixtures_share_a_universe_at_their_own_addresses() {
    let a = flood("Left flood", "dimmer-rgbw-strobe-mode", 1);
    let b = flood("Right flood", "rgb", 20);
    let mut show = Show::new("Floods");
    show.controllers.push(node(
        Protocol::Sacn(SacnConfig::default()),
        1,
        &[(0, &b), (0, &a)],
    ));
    show.props = vec![a.clone(), b.clone()];
    let (map, report) = map_show(&show);
    assert!(report.issues.is_empty(), "{:?}", report.issues);

    // Each fixture's cell, then their channels after every pixel.
    assert_eq!(map.props[0].frame_offset, 0);
    assert_eq!(map.props[1].frame_offset, 3);
    assert_eq!(map.fixtures.len(), 2);
    assert_eq!(map.fixtures[0].dmx_offset, 6);
    assert_eq!(map.fixtures[1].dmx_offset, 13);
    assert_eq!(map.frame_len, 16);

    let out = &map.controllers[0];
    assert_eq!(out.channel_count, 512, "a whole universe");
    let Addressing::Sacn { universes, .. } = &out.addressing else {
        panic!("sACN")
    };
    assert_eq!(universes.len(), 1);
    assert_eq!((universes[0].universe, universes[0].len), (1, 512));
    // Spans in channel order, each at its fixture's address.
    let spans: Vec<(usize, usize, u32)> = out
        .spans
        .iter()
        .map(|s| (s.controller_channel, s.frame_offset, s.pixels))
        .collect();
    assert_eq!(spans, vec![(0, 6, 7), (19, 13, 3)]);
    assert_eq!(
        map.locate(b.id, 0)[0].address,
        ChannelAddress::Sacn {
            universe: 1,
            channel: 20
        }
    );
}

#[test]
fn each_port_of_a_dmx_controller_is_its_own_universe_and_art_net_counts_from_zero() {
    let a = flood("A", "rgb", 1);
    let b = flood("B", "rgb", 1);
    let mut show = Show::new("Two universes");
    show.props = vec![a.clone(), b.clone()];
    show.controllers.push(node(art_net(), 2, &[(0, &a), (1, &b)]));
    // A pixel controller on sACN numbers its universes on its own, from 1.
    let pixels = line("Arch", 10);
    let mut controller = Controller::new("Pixels", "192.0.2.21", Protocol::Sacn(SacnConfig::default()));
    controller.ports.push(pf_model::Port::new(1));
    controller.ports[0].slots.push(PortSlot::new(pixels.id));
    show.props.push(pixels);
    show.controllers.push(controller);
    let (map, report) = map_show(&show);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let Addressing::ArtNet { universes } = &map.controllers[0].addressing else {
        panic!("Art-Net")
    };
    let numbered: Vec<(u16, usize, u16)> = universes
        .iter()
        .map(|u| (u.universe, u.controller_channel, u.len))
        .collect();
    assert_eq!(numbered, vec![(0, 0, 512), (1, 512, 512)]);
    assert_eq!(map.controllers[0].spans[1].controller_channel, 512);
    assert_eq!(
        map.locate(b.id, 0)[0].address,
        ChannelAddress::ArtNet {
            universe: 1,
            channel: 1
        }
    );
    assert_eq!(map.controllers[1].addressing.universes()[0].universe, 1);
    assert_eq!(map.universe_count(), 3);
}

#[test]
fn art_net_universes_collide_only_with_art_net() {
    let mut pinned = node(
        Protocol::ArtNet(ArtNetConfig {
            start_universe: Some(3),
            ..ArtNetConfig::default()
        }),
        2,
        &[],
    );
    pinned.name = "First".into();
    let mut second = pinned.clone();
    second.id = pf_model::ControllerId::new();
    second.name = "Second".into();
    let mut show = Show::new("Collide");
    show.controllers = vec![pinned, second];
    let (_, report) = map_show(&show);
    let collisions: Vec<&str> = report
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::UniverseCollision)
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(
        collisions,
        vec![
            "'First' and 'Second' both use Art-Net universes 3–4. That only matters if they are the same device."
        ]
    );

    // The same numbers on sACN don't collide with Art-Net's.
    show.controllers[1].protocol = Protocol::Sacn(SacnConfig {
        start_universe: Some(3),
        ..SacnConfig::default()
    });
    let (_, report) = map_show(&show);
    assert!(!report.has_code(IssueCode::UniverseCollision));

    // Art-Net stops at 32767.
    show.controllers[0].protocol = Protocol::ArtNet(ArtNetConfig {
        start_universe: Some(32_767),
        ..ArtNetConfig::default()
    });
    let (_, report) = map_show(&show);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == IssueCode::UniverseOutOfRange
                && i.message
                    .contains("Art-Net universes must be between 0 and 32767")),
        "{:?}",
        report.issues
    );
}

#[test]
fn overlapping_fixtures_and_pixels_on_a_dmx_controller_are_reported() {
    let a = flood("A", "dimmer-rgb", 1);
    let b = flood("B", "rgb", 3);
    let arch = line("Arch", 5);
    let stray = flood("Stray", "rgb", 1);
    let mut show = Show::new("Problems");
    show.props = vec![a.clone(), b.clone(), arch.clone(), stray.clone()];
    show.controllers
        .push(node(art_net(), 1, &[(0, &a), (0, &b), (0, &arch)]));
    let mut pixels = Controller::new("Pixels", "192.0.2.21", Protocol::Ddp);
    pixels.ports.push(pf_model::Port::new(1));
    pixels.ports[0].slots.push(PortSlot::new(stray.id));
    show.controllers.push(pixels);
    let (map, report) = map_show(&show);
    let found = |code| {
        report
            .issues
            .iter()
            .filter(|i| i.code == code)
            .map(|i| i.message.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        found(IssueCode::DmxChannelsOverlap),
        vec!["'B' and 'A' share channels on port 1 of 'Node', so the later one's values are sent there."]
    );
    assert_eq!(found(IssueCode::WrongControllerKind).len(), 1, "the arch");
    assert!(
        !report.has_code(IssueCode::UnassignedNodes),
        "wired to the wrong kind isn't also 'not wired': {:?}",
        report.issues
    );
    // The arch isn't sent; the stray flood is, on the pixel controller.
    assert_eq!(map.controllers[0].spans.len(), 2);
    assert_eq!(map.controllers[1].spans.len(), 1);
}

#[test]
fn a_pixel_controller_carries_a_fixture_whole_where_it_falls_in_its_universes() {
    // 160 RGB pixels (480 channels), 30 null channels, then a 7-channel flood at channel 511 of
    // universe 1: it doesn't fit there, so it starts universe 2 at channel 1.
    let arch = line("Arch", 160);
    let mut flood = flood("Flood", "dimmer-rgbw-strobe-mode", 1);
    let mut controller = Controller::new(
        "Falcon",
        "192.0.2.22",
        Protocol::Sacn(SacnConfig {
            start_universe: Some(1),
            universe_size: pf_model::UniverseSize::CHANNELS_512,
            ..SacnConfig::default()
        }),
    );
    controller.ports.push(pf_model::Port::new(1));
    controller.ports[0].slots.push(PortSlot::new(arch.id));
    let mut slot = PortSlot::new(flood.id);
    slot.null_pixels = 30;
    controller.ports[0].slots.push(slot);
    let mut show = Show::new("Mixed");
    show.props = vec![arch, flood.clone()];
    show.controllers.push(controller.clone());
    let (map, report) = map_show(&show);
    let universes: Vec<(u16, u16)> = map.controllers[0]
        .addressing
        .universes()
        .iter()
        .map(|u| (u.universe, u.len))
        .collect();
    assert_eq!(universes, vec![(1, 510), (2, 7)], "the flood isn't split");
    assert_eq!(
        map.locate(flood.id, 0)[0].address,
        ChannelAddress::Sacn {
            universe: 2,
            channel: 1
        }
    );
    assert!(!report.has_code(IssueCode::WrongControllerKind));
    assert!(
        !report.has_code(IssueCode::DmxAddressMismatch),
        "{:?}",
        report.issues
    );

    // Set to another address than it lands on: said so.
    flood.dmx.as_mut().unwrap().address = 9;
    show.props[1] = flood;
    let (_, report) = map_show(&show);
    let found: Vec<&str> = report
        .issues
        .iter()
        .filter(|i| i.code == IssueCode::DmxAddressMismatch)
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(
        found,
        vec![
            "The DMX fixture 'Flood' is set to address 9, but on 'Falcon' its channels start at 1 of their universe."
        ]
    );
}

#[test]
fn a_usb_interface_sends_one_universe() {
    let a = flood("A", "rgb", 510);
    let mut show = Show::new("USB");
    show.props = vec![a.clone()];
    show.controllers.push(node(Protocol::DmxUsb, 2, &[(0, &a)]));
    let (map, report) = map_show(&show);
    assert!(report.has_code(IssueCode::DmxUniverses));
    assert_eq!(map.controllers[0].addressing, Addressing::DmxUsb);
    assert_eq!(map.controllers[0].channel_count, 512);
    assert_eq!(
        map.locate(a.id, 0)[0].address,
        ChannelAddress::DmxUsb { channel: 510 }
    );
}

#[test]
fn a_fixture_that_cant_be_used_takes_no_channels() {
    let mut a = flood("A", "rgb", 511);
    let b = flood("B", "rgb", 1);
    let mut show = Show::new("Past the end");
    show.props = vec![a.clone(), b.clone()];
    show.controllers.push(node(art_net(), 1, &[(0, &a), (0, &b)]));
    let (map, _) = map_show(&show);
    assert_eq!(map.fixtures.len(), 1, "only B");
    assert_eq!(map.controllers[0].spans.len(), 1);
    a.dmx.as_mut().unwrap().address = 1;
    assert!(pf_model::validate_show(&show).has_code(IssueCode::InvalidFixture));
}

#[test]
fn the_colour_drawn_on_a_fixture_becomes_its_channels() {
    let a = flood("A", "dimmer-rgbw-strobe-mode", 1);
    let mut show = Show::new("Colour");
    show.props = vec![a.clone()];
    let mut dmx = a.dmx.clone().unwrap();
    dmx.channels[6].value = 33;
    show.props[0].dmx = Some(dmx);
    let (map, _) = map_show(&show);
    let mut frame = vec![0u8; map.frame_len];
    frame[..3].copy_from_slice(&[128, 64, 64]);
    map.write_fixtures(&mut frame);
    // Dimmer at the brightest channel, the colour brought up to full, white out of it.
    assert_eq!(&frame[3..], &[128, 127, 0, 0, 128, 0, 33]);
    assert_eq!(
        &frame[..3],
        &[128, 64, 64],
        "the cell shows what the channels make"
    );

    // An effect (or a test by hand) setting the dimmer to half.
    let mut overrides = vec![None; 7];
    overrides[0] = Some(64);
    map.fixtures[0].write(&mut frame, &overrides);
    assert_eq!(frame[3], 64);
    assert_eq!(&frame[..3], &[64, 32, 32]);

    map.fixtures[0].write_values(&mut frame, &[None, Some(255), None, Some(255)]);
    assert_eq!(&frame[3..], &[0, 255, 0, 255, 0, 0, 33]);
    assert_eq!(&frame[..3], &[0, 0, 0], "no dimmer, no light");
}
