//! Importing a small but complete xLights show folder end to end.

use pf_model::{ColorOrder, Protocol, SequenceChannels, ShapeSource};
use pf_xlights::{ImportSummary, import_folder};
use std::path::Path;

fn sample() -> pf_xlights::XlightsImport {
    import_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-show")).unwrap()
}

#[test]
fn imports_props_controllers_and_groups() {
    let imported = sample();
    assert!(
        imported
            .notes
            .iter()
            .all(|n| !n.contains("isn't wired") && !n.contains("Not wired")),
        "{:#?}",
        imported.notes
    );
    assert_eq!(
        imported.summary,
        ImportSummary {
            props: 6,
            pixels: 100 + 150 + 400 + 80 + 7 + 36,
            controllers: 2,
            wired: 6,
            groups: 2,
        }
    );
    let show = &imported.show;
    assert_eq!(show.name, "sample-show");
    let falcon = &show.controllers[0];
    assert_eq!(
        (falcon.name.as_str(), falcon.address.as_str()),
        ("Falcon_F16V5_B9F5", "192.0.2.20")
    );
    assert_eq!(falcon.protocol, Protocol::Ddp);
    assert_eq!(
        falcon.sequence_channels,
        Some(SequenceChannels {
            start: 1,
            count: 2400,
            raw_ddp_offsets: false
        })
    );
    let Protocol::Sacn(porch) = show.controllers[1].protocol else {
        panic!("Porch is sACN")
    };
    assert_eq!(porch.start_universe, Some(20));
    assert_eq!(show.controllers[1].sequence_channels.unwrap().start, 2401);

    let arches = show.props.iter().find(|p| p.name == "Arches").unwrap();
    assert_eq!(arches.color_order, ColorOrder::Grb);
    assert!(matches!(arches.shape, ShapeSource::Measured { .. }));
    let everything = show.groups.iter().find(|g| g.name == "Everything").unwrap();
    assert_eq!(
        everything.members.len(),
        4,
        "nested group flattened, submodel skipped"
    );
}

/// Every prop sits on exactly the controller channel xLights gives it.
#[test]
fn channels_match_xlights() {
    let show = sample().show;
    let (map, report) = pf_mapping::map_show(&show);
    assert!(!report.has_errors(), "{report:?}");
    let expected = [
        ("Roofline", 0, 0),
        ("Arches", 0, 300),
        ("Mega Tree", 0, 750),
        ("Window Matrix", 0, 1950),
        ("Porch Star", 1, 0),
        ("Candy Canes", 1, 21),
    ];
    for (name, controller, channel) in expected {
        let prop = show.props.iter().find(|p| p.name == name).unwrap();
        let span = map.controllers[controller]
            .spans
            .iter()
            .find(|s| s.prop == prop.id)
            .unwrap_or_else(|| panic!("{name} isn't wired"));
        assert_eq!(span.controller_channel, channel, "{name}");
    }
}

/// The layout keeps xLights' arrangement: everything inside the 1280x720 preview, the roofline
/// above the arches, and props where they were placed.
#[test]
fn positions_follow_the_xlights_layout() {
    let show = sample().show;
    let bounds = |name: &str| {
        let prop = show.props.iter().find(|p| p.name == name).unwrap();
        let ShapeSource::Measured { points, .. } = &prop.shape else {
            panic!("measured")
        };
        points
            .iter()
            .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(x0, y0, x1, y1), p| {
                (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y))
            })
    };
    let roof = bounds("Roofline");
    assert!(
        (roof.0 - 1.0).abs() < 0.02 && (roof.2 - 11.0).abs() < 0.02,
        "roofline spans x 100..1100: {roof:?}"
    );
    assert!(
        (roof.1 - 6.0).abs() < 0.01 && (roof.3 - 6.0).abs() < 0.01,
        "roofline is level at y 600: {roof:?}"
    );
    let arches = bounds("Arches");
    assert!(arches.3 < roof.1, "arches sit below the roofline");
    let tree = bounds("Mega Tree");
    assert!(
        tree.0 > 7.0 && tree.2 < 11.0,
        "tree is centered near x 900: {tree:?}"
    );
    for name in [
        "Roofline",
        "Arches",
        "Mega Tree",
        "Window Matrix",
        "Porch Star",
        "Candy Canes",
    ] {
        let (x0, y0, x1, y1) = bounds(name);
        assert!(
            x0 >= 0.0 && y0 >= 0.0 && x1 <= 12.8 && y1 <= 7.2,
            "{name} inside the preview: {:?}",
            (x0, y0, x1, y1)
        );
    }
}
