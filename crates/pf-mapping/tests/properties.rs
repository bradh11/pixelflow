//! Invariants of `map_show` over randomly wired shows.

use pf_mapping::{Addressing, map_show};
use pf_model::{
    ColorOrder, Controller, Generator, IssueCode, Port, PortSlot, Prop, Protocol, SacnConfig, ShapeSource,
    Show, UniverseSize,
};
use proptest::prelude::*;

#[derive(Debug, Clone)]
struct Case {
    props: Vec<(u32, bool, u32)>, // (nodes, rgbw, null pixels)
    ports: usize,
    size: UniverseSize,
    straddle: bool,
}

fn case() -> impl Strategy<Value = Case> {
    (
        proptest::collection::vec((1u32..400, any::<bool>(), 0u32..5), 1..12),
        1usize..5,
        prop_oneof![Just(UniverseSize::CHANNELS_510), Just(UniverseSize::CHANNELS_512)],
        any::<bool>(),
    )
        .prop_map(|(props, ports, size, straddle)| Case {
            props,
            ports,
            size,
            straddle,
        })
}

fn build(case: &Case) -> Show {
    let mut show = Show::new("prop");
    let mut ports: Vec<Port> = (1..=case.ports as u16).map(Port::new).collect();
    for (i, &(nodes, rgbw, nulls)) in case.props.iter().enumerate() {
        let mut prop = Prop::new(
            format!("P{i}"),
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        );
        if rgbw {
            prop.color_order = ColorOrder::Rgbw;
        }
        let mut slot = PortSlot::new(prop.id);
        slot.null_pixels = nulls;
        ports[i % case.ports].slots.push(slot);
        show.props.push(prop);
    }
    let mut controller = Controller::new(
        "C",
        "10.0.0.1",
        Protocol::Sacn(SacnConfig {
            universe_size: case.size,
            allow_pixel_straddle: case.straddle,
            ..SacnConfig::default()
        }),
    );
    controller.ports = ports;
    show.controllers.push(controller);
    show
}

proptest! {
    #[test]
    fn mapping_invariants(case in case()) {
        let show = build(&case);
        let (map, report) = map_show(&show);
        prop_assert!(!report.has_code(IssueCode::NodeAssignedTwice));
        prop_assert!(!report.has_code(IssueCode::UnassignedNodes));

        // Frame holds every prop pixel exactly once.
        let expected: usize = show.props.iter().map(Prop::channel_count).sum();
        prop_assert_eq!(map.frame_len, expected);

        let out = &map.controllers[0];
        let Addressing::Sacn { universes, .. } = &out.addressing else {
            return Err(TestCaseError::fail("expected sACN"));
        };

        // Universes tile the controller's channels without gaps or overflow.
        let mut next = 0usize;
        for (n, u) in universes.iter().enumerate() {
            prop_assert_eq!(u.controller_channel, next);
            prop_assert!(u.len as usize <= case.size.channels() as usize);
            prop_assert_eq!(u.universe as usize, n + 1);
            next += u.len as usize;
        }
        prop_assert_eq!(next, out.channel_count);

        // Every node maps to exactly one location, and without straddling, each pixel's
        // channels fit inside its universe.
        for (prop, layout) in show.props.iter().zip(&map.props) {
            for node in 0..prop.node_count() {
                let locations = map.locate(prop.id, node);
                prop_assert_eq!(locations.len(), 1);
                if !case.straddle {
                    let pf_mapping::ChannelAddress::Sacn { channel, .. } = locations[0].address else {
                        return Err(TestCaseError::fail("expected sACN address"));
                    };
                    let last = channel as usize + layout.channels_per_pixel as usize - 1;
                    prop_assert!(last <= case.size.channels() as usize);
                }
            }
        }
    }
}
