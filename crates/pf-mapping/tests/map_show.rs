//! End-to-end behavior of `map_show` on small hand-built shows.

use pf_mapping::{Addressing, ChannelAddress, map_show};
use pf_model::{
    ColorOrder, Controller, Generator, IssueCode, NodeRange, Port, PortSlot, Prop, Protocol, SacnConfig,
    ShapeSource, Show,
};

fn line(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    )
}

fn controller(name: &str, protocol: Protocol, ports: Vec<Port>) -> Controller {
    let mut c = Controller::new(name, "10.0.0.10", protocol);
    c.ports = ports;
    c
}

fn port(number: u16, slots: Vec<PortSlot>) -> Port {
    let mut p = Port::new(number);
    p.slots = slots;
    p
}

fn sacn() -> Protocol {
    Protocol::Sacn(SacnConfig::default())
}

#[test]
fn props_are_laid_out_back_to_back_in_the_frame() {
    let mut show = Show::new("t");
    let a = line("A", 10);
    let mut b = line("B", 5);
    b.color_order = ColorOrder::Grbw;
    show.props = vec![a.clone(), b.clone()];

    let (map, _) = map_show(&show);
    assert_eq!(map.frame_len, 10 * 3 + 5 * 4);
    assert_eq!(map.prop_layout(a.id).unwrap().frame_offset, 0);
    assert_eq!(map.prop_layout(b.id).unwrap().frame_offset, 30);
}

#[test]
fn ddp_slots_follow_port_order_with_null_pixels_and_reverse() {
    let mut show = Show::new("t");
    let a = line("A", 10);
    let b = line("B", 4);
    let mut slot_b = PortSlot::new(b.id);
    slot_b.null_pixels = 2;
    slot_b.reverse = true;
    show.props = vec![a.clone(), b.clone()];
    show.controllers = vec![controller(
        "WLED",
        Protocol::Ddp,
        vec![port(1, vec![PortSlot::new(a.id)]), port(2, vec![slot_b])],
    )];

    let (map, report) = map_show(&show);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let out = &map.controllers[0];
    assert_eq!(out.channel_count, (10 + 2 + 4) * 3);
    assert_eq!(out.addressing, Addressing::Ddp);
    // B starts after A (30 channels) and 2 null pixels (6 channels); reversed, so node 3 is first.
    assert_eq!(out.spans[1].controller_channel, 36);
    let loc = map.locate(b.id, 3);
    assert_eq!(loc.len(), 1);
    assert_eq!(loc[0].port, 2);
    assert_eq!(loc[0].address, ChannelAddress::Ddp { offset: 36 });
    assert_eq!(map.locate(b.id, 0)[0].address, ChannelAddress::Ddp { offset: 45 });
}

#[test]
fn sacn_controllers_get_consecutive_automatic_universes() {
    let mut show = Show::new("t");
    let a = line("A", 400); // 1200 channels -> 3 universes of 510
    let b = line("B", 10);
    show.props = vec![a.clone(), b.clone()];
    show.controllers = vec![
        controller("One", sacn(), vec![port(1, vec![PortSlot::new(a.id)])]),
        controller("Two", sacn(), vec![port(1, vec![PortSlot::new(b.id)])]),
    ];

    let (map, report) = map_show(&show);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(map.universe_count(), 4);
    assert_eq!(
        map.locate(a.id, 170)[0].address,
        ChannelAddress::Sacn {
            universe: 2,
            channel: 1
        }
    );
    assert_eq!(
        map.locate(b.id, 0)[0].address,
        ChannelAddress::Sacn {
            universe: 4,
            channel: 1
        }
    );
}

#[test]
fn slot_overrides_port_brightness_and_gamma() {
    let mut show = Show::new("t");
    let a = line("A", 3);
    let b = line("B", 3);
    let mut slot_b = PortSlot::new(b.id);
    slot_b.brightness = Some(50);
    slot_b.gamma = Some(2.2);
    let mut p = port(1, vec![PortSlot::new(a.id), slot_b]);
    p.brightness = 80;
    show.props = vec![a, b];
    show.controllers = vec![controller("C", Protocol::Ddp, vec![p])];

    let (map, _) = map_show(&show);
    let spans = &map.controllers[0].spans;
    assert_eq!((spans[0].brightness, spans[0].gamma), (80, 1.0));
    assert_eq!((spans[1].brightness, spans[1].gamma), (50, 2.2));
}

#[test]
fn wiring_problems_are_reported_in_plain_language() {
    // Over capacity
    let mut show = Show::new("t");
    let a = line("Arch", 60);
    let mut p = port(3, vec![PortSlot::new(a.id)]);
    p.max_pixels = Some(50);
    show.props = vec![a];
    show.controllers = vec![controller("Falcon", Protocol::Ddp, vec![p])];
    let (_, report) = map_show(&show);
    assert!(report.has_code(IssueCode::PortOverCapacity));
    assert_eq!(
        report.issues[0].message,
        "Port 3 on 'Falcon' is over capacity by 10 pixels (60 of 50)."
    );

    // Not wired at all, and partially wired
    let mut show = Show::new("t");
    let a = line("Lonely", 5);
    let b = line("Half", 10);
    let mut half = PortSlot::new(b.id);
    half.segment = Some(NodeRange::new(0, 5));
    show.props = vec![a, b];
    show.controllers = vec![controller("C", Protocol::Ddp, vec![port(1, vec![half])])];
    let (_, report) = map_show(&show);
    let messages: Vec<_> = report.issues.iter().map(|i| i.message.as_str()).collect();
    assert!(messages.contains(&"The prop 'Lonely' is not wired to any controller port."));
    assert!(messages.contains(&"5 of 10 pixels on 'Half' are not wired to any port."));

    // Wired twice
    let mut show = Show::new("t");
    let a = line("Twice", 5);
    show.controllers = vec![controller(
        "C",
        Protocol::Ddp,
        vec![
            port(1, vec![PortSlot::new(a.id)]),
            port(2, vec![PortSlot::new(a.id)]),
        ],
    )];
    show.props = vec![a.clone()];
    let (map, report) = map_show(&show);
    assert!(report.has_code(IssueCode::NodeAssignedTwice));
    assert_eq!(map.locate(a.id, 0).len(), 2);
}

#[test]
fn pinned_universe_overlap_is_an_error_with_multicast_and_a_warning_with_unicast() {
    for multicast in [false, true] {
        let mut show = Show::new("t");
        let a = line("A", 10);
        let b = line("B", 10);
        let pinned = |start| {
            Protocol::Sacn(SacnConfig {
                start_universe: Some(start),
                multicast,
                ..SacnConfig::default()
            })
        };
        show.controllers = vec![
            controller("One", pinned(7), vec![port(1, vec![PortSlot::new(a.id)])]),
            controller("Two", pinned(7), vec![port(1, vec![PortSlot::new(b.id)])]),
        ];
        show.props = vec![a, b];
        let (_, report) = map_show(&show);
        assert!(report.has_code(IssueCode::UniverseCollision));
        assert_eq!(report.has_errors(), multicast);
        assert!(
            report.issues[0]
                .message
                .starts_with("'One' and 'Two' both use universes 7–7.")
        );
    }
}

#[test]
fn universes_beyond_63999_are_rejected() {
    let mut show = Show::new("t");
    let a = line("A", 400);
    show.controllers = vec![controller(
        "High",
        Protocol::Sacn(SacnConfig {
            start_universe: Some(63_998),
            ..SacnConfig::default()
        }),
        vec![port(1, vec![PortSlot::new(a.id)])],
    )];
    show.props = vec![a];
    let (_, report) = map_show(&show);
    assert!(report.has_code(IssueCode::UniverseOutOfRange));
}

#[test]
fn slots_with_missing_props_or_bad_segments_are_skipped() {
    let mut show = Show::new("t");
    let a = line("A", 5);
    let mut bad = PortSlot::new(a.id);
    bad.segment = Some(NodeRange::new(0, 99));
    show.controllers = vec![controller(
        "C",
        Protocol::Ddp,
        vec![port(1, vec![PortSlot::new(pf_model::PropId::new()), bad])],
    )];
    show.props = vec![a];
    let (map, _) = map_show(&show);
    assert!(map.controllers[0].spans.is_empty());
    assert_eq!(map.controllers[0].channel_count, 0);
}
