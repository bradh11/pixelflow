//! Scale target: 200k pixels over ~1,200 sACN universes, one frame well under a 40 fps period.

use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, SacnConfig, ShapeSource, Show};
use pf_output::{OutputSettings, build_plan, render_controller, sacn::SacnPackets};
use std::time::Instant;

/// 20 controllers × 8 ports × 1,250-pixel props = 200,000 RGB pixels.
fn big_show() -> Show {
    let mut show = Show::new("scale");
    for c in 0..20 {
        let mut controller = Controller::new(
            format!("C{c}"),
            format!("10.0.{c}.1"),
            Protocol::Sacn(SacnConfig::default()),
        );
        for p in 0..8 {
            let prop = Prop::new(
                format!("C{c}P{p}"),
                ShapeSource::Generator(Generator::Line {
                    nodes: 1_250,
                    length: 1.0,
                }),
            );
            let mut port = Port::new(p + 1);
            port.slots.push(PortSlot::new(prop.id));
            controller.ports.push(port);
            show.props.push(prop);
        }
        show.controllers.push(controller);
    }
    show
}

#[test]
fn a_200k_pixel_frame_renders_and_packetizes_within_budget() {
    let show = big_show();
    let (map, report) = pf_mapping::map_show(&show);
    assert!(!report.has_errors());
    let universes = map.universe_count();
    assert!(universes >= 1_150, "{universes} universes");

    let plan = build_plan(&show, &map);
    let settings = OutputSettings::default();
    let frame: Vec<u8> = (0..plan.frame_len).map(|i| i as u8).collect();
    let mut work: Vec<(Vec<u8>, SacnPackets)> = plan
        .controllers
        .iter()
        .map(|c| {
            let pf_output::Wire::Sacn { universes, .. } = &c.wire else {
                unreachable!()
            };
            (
                vec![0u8; c.channel_count],
                SacnPackets::new(universes, false, "10.0.0.1:5568".parse().unwrap(), &settings),
            )
        })
        .collect();

    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        let mut bytes = 0usize;
        for (controller, (buffer, packets)) in plan.controllers.iter().zip(work.iter_mut()) {
            render_controller(&frame, controller, &plan.luts, buffer);
            packets.update(buffer);
            for i in 0..packets.len() {
                bytes += packets.packet(i).0.len();
            }
        }
        assert!(bytes > 600_000);
        best = best.min(start.elapsed().as_secs_f64());
    }
    // The 25 ms frame period at 40 fps is the hard limit; leave most of it for sending.
    if !cfg!(debug_assertions) {
        assert!(best < 0.010, "frame took {:.2} ms", best * 1000.0);
    }
    println!("200k-pixel frame: {:.2} ms, {universes} universes", best * 1000.0);
}
