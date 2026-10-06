//! Importing a small but complete xLights show folder end to end.

use pf_model::{
    BufferStyle, ColorOrder, GroupMember, LineLayout, NodeRange, NodeRun, Phoneme, Protocol, RegionKind, Rgb,
    SequenceChannels, ShapeSource,
};
use pf_xlights::{ImportSummary, import_folder};
use std::path::Path;

fn sample() -> pf_xlights::XlightsImport {
    import_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-show")).unwrap()
}

#[test]
fn imports_props_controllers_and_groups() {
    let imported = sample();
    assert!(
        imported.notes.iter().all(|n| !n.contains("isn't wired")
            && !n.contains("Not wired")
            && !n.contains("submodels that aren't")),
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
    assert!(matches!(
        arches.shape,
        ShapeSource::Generator(pf_model::Generator::Arch { arches: 3, nodes: 50, .. })
    ));
    let everything = show.groups.iter().find(|g| g.name == "Everything").unwrap();
    // The nested group is flattened in place, and the submodel keeps its spot in the list.
    let names: Vec<String> = everything
        .members
        .iter()
        .map(|m| {
            let prop = show.prop(m.prop()).unwrap();
            match m {
                GroupMember::Prop(_) => prop.name.clone(),
                GroupMember::Region(r) => format!("{}/{}", prop.name, prop.region(r.region).unwrap().name),
            }
        })
        .collect();
    assert_eq!(
        names,
        [
            "Roofline",
            "Arches",
            "Porch Star/Center",
            "Mega Tree",
            "Window Matrix"
        ]
    );
}

#[test]
fn imports_submodels_and_faces() {
    let imported = sample();
    let show = &imported.show;
    let prop = |name: &str| show.props.iter().find(|p| p.name == name).unwrap();
    let run = |a, b| Some(NodeRun::new(a, b));

    let arches = prop("Arches");
    let names: Vec<_> = arches.regions.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Arch 1", "Tops", "Ends"]);
    assert_eq!(
        arches.regions[1].kind,
        RegionKind::Nodes {
            lines: vec![vec![run(19, 29)], vec![run(69, 79)], vec![run(119, 129)]],
            layout: LineLayout::Vertical,
            buffer: BufferStyle::StackedStrands,
        }
    );
    assert_eq!(
        arches.regions[2].kind,
        RegionKind::Nodes {
            lines: vec![
                vec![run(0, 4), run(45, 49)],
                vec![run(100, 104), None, run(149, 145)]
            ],
            layout: LineLayout::Horizontal,
            buffer: BufferStyle::KeepXy,
        }
    );

    let matrix = prop("Window Matrix");
    assert_eq!(
        matrix.regions[0].kind,
        RegionKind::SubBuffer {
            x1: 0.0,
            y1: 50.0,
            x2: 100.0,
            y2: 100.0
        }
    );
    assert_eq!(matrix.regions[1].name, "Singer");
    let RegionKind::Face(face) = &matrix.regions[1].kind else {
        panic!("Singer is a face")
    };
    assert_eq!(face.mouths[&Phoneme::Ai], vec![NodeRange::new(0, 4)]);
    assert_eq!(face.mouths[&Phoneme::Rest], vec![NodeRange::new(8, 10)]);
    assert_eq!(
        face.eyes_open,
        vec![NodeRange::new(60, 62), NodeRange::new(78, 80)]
    );
    assert_eq!(face.outline, vec![NodeRange::new(20, 40)]);
    let colors = face.colors.as_ref().unwrap();
    assert_eq!(colors.outline, Some(Rgb::new(255, 255, 0)));
    assert_eq!(colors.mouths.get(&Phoneme::O), None, "no color: white");

    let notes = imported.notes.join("\n");
    assert!(
        notes.contains("picture faces yet, so these weren't imported: Pictures (on Window Matrix)."),
        "{notes}"
    );
    assert!(
        notes.contains("states yet, so these weren't imported: Lights (on Window Matrix)."),
        "{notes}"
    );
    assert!(
        pf_model::validate_show(show)
            .issues
            .iter()
            .all(|i| i.code != pf_model::IssueCode::RegionOutOfBounds)
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
        pf_geometry::world_positions(prop)
            .iter()
            .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(x0, y0, x1, y1), p| {
                (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y))
            })
    };
    // The roofline is a single line in xLights, so it comes in as an editable line.
    let roofline = show.props.iter().find(|p| p.name == "Roofline").unwrap();
    assert!(
        matches!(
            roofline.shape,
            ShapeSource::Generator(pf_model::Generator::Line { .. })
        ),
        "{:?}",
        roofline.shape
    );
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

/// Props imported as editable shapes put every pixel exactly where xLights' own layout does
/// (each node at the middle of its lights, in channel order), as measured imports always have.
#[test]
fn editable_shapes_land_on_xlights_positions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-show");
    let layout =
        pf_xlights::parse_layout(&std::fs::read_to_string(dir.join("xlights_rgbeffects.xml")).unwrap())
            .unwrap();
    let show = sample().show;
    let canes = show.props.iter().find(|p| p.name == "Candy Canes").unwrap();
    assert!(
        matches!(
            canes.shape,
            ShapeSource::Generator(pf_model::Generator::CandyCanes {
                canes: 2,
                nodes_per_cane: 18,
                ..
            })
        ),
        "{:?}",
        canes.shape
    );
    let tree = show.props.iter().find(|p| p.name == "Mega Tree").unwrap();
    assert!(
        matches!(
            tree.shape,
            ShapeSource::Generator(pf_model::Generator::Tree { strings: 8, .. })
        ),
        "{:?}",
        tree.shape
    );
    let star = show.props.iter().find(|p| p.name == "Porch Star").unwrap();
    assert!(
        matches!(
            star.shape,
            ShapeSource::Generator(pf_model::Generator::CustomGrid {
                columns: 4,
                rows: 5,
                ..
            })
        ),
        "{:?}",
        star.shape
    );
    let matrix = show.props.iter().find(|p| p.name == "Window Matrix").unwrap();
    assert!(
        matches!(
            matrix.shape,
            ShapeSource::Generator(pf_model::Generator::Matrix { columns: 4, .. })
        ),
        "{:?}",
        matrix.shape
    );
    let mut editable = 0;
    for prop in &show.props {
        if !matches!(prop.shape, ShapeSource::Generator(_)) {
            continue;
        }
        editable += 1;
        let model = layout.models.iter().find(|m| m.name == prop.name).unwrap();
        // xLights' 3D layout, depth included, without the tilt its 2D view gives trees.
        let xlights = pf_xlights::upright_positions(model);
        let ours = pf_geometry::world_positions(prop);
        assert_eq!(ours.len(), xlights.len(), "{}", prop.name);
        for (a, b) in ours.iter().zip(&xlights) {
            assert!(
                (a.x - b[0] * 0.01).abs() < 2e-3
                    && (a.y - b[1] * 0.01).abs() < 2e-3
                    && (a.z - b[2] * 0.01).abs() < 2e-3,
                "{}: {a:?} vs {b:?}",
                prop.name
            );
        }
        // The same nodes as the front-view layout: for everything but the tree, in the same places.
        let front = pf_xlights::geometry(model);
        assert_eq!(front.nodes.len(), xlights.len(), "{}", prop.name);
    }
    // Every editable model in the sample show stays editable.
    assert_eq!(
        editable, 6,
        "roofline, arches, candy canes, mega tree, porch star, window matrix"
    );
    let tree = show.props.iter().find(|p| p.name == "Mega Tree").unwrap();
    let depth = pf_geometry::world_positions(tree)
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
    assert!(depth.1 - depth.0 > 1.0, "the mega tree is round in 3D: {depth:?}");
    assert_eq!(tree.transform.rotation_deg, pf_model::Vec3::ZERO, "and upright");
    let notes = sample().notes;
    assert!(
        notes.iter().any(|n| n
            == "xLights draws trees, spheres and cubes with a slight tilt in its 2D view; PixelFlow shows their real shape: Mega Tree."),
        "{notes:#?}"
    );
}
