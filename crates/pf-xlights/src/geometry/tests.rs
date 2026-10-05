//! Tests for the geometry port. Expected positions are worked out by hand from the xLights
//! formulas (see each test), not by re-running the implementation.

use super::{Geometry, geometry};
use crate::model::XmlModel;

fn model(display_as: &str, attrs: &[(&str, &str)]) -> XmlModel {
    XmlModel {
        name: "m".into(),
        display_as: display_as.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..XmlModel::default()
    }
}

fn geo(display_as: &str, attrs: &[(&str, &str)]) -> Geometry {
    geometry(&model(display_as, attrs))
}

#[track_caller]
fn near(p: [f32; 2], x: f64, y: f64) {
    let ok = (f64::from(p[0]) - x).abs() < 1e-3 && (f64::from(p[1]) - y).abs() < 1e-3;
    assert!(ok, "point {p:?} != ({x}, {y})");
}

/// The single light of the node with channel `ch`.
#[track_caller]
fn at(g: &Geometry, ch: u32) -> [f32; 2] {
    let n = g
        .nodes
        .iter()
        .find(|n| n.channel == ch)
        .unwrap_or_else(|| panic!("no node at channel {ch}"));
    n.points[0]
}

fn channels(g: &Geometry) -> Vec<u32> {
    g.nodes.iter().map(|n| n.channel).collect()
}

#[test]
fn single_line_spreads_lights_from_point_one_to_point_two() {
    let g = geo(
        "Single Line",
        &[
            ("NumStrings", "1"),
            ("NodesPerString", "3"),
            ("WorldPosX", "100"),
            ("WorldPosY", "50"),
            ("X2", "200"),
        ],
    );
    assert_eq!(channels(&g), [0, 3, 6]);
    assert_eq!(
        (g.channels, g.channels_per_node, g.approximate.as_deref()),
        (9, 3, None)
    );
    near(at(&g, 0), 100.0, 50.0);
    near(at(&g, 3), 200.0, 50.0);
    near(at(&g, 6), 300.0, 50.0);
}

#[test]
fn single_line_dir_r_reverses_channels_within_each_string() {
    // Two strings of two nodes from x=0 to x=300: lights at 0, 100, 200, 300. String 0 holds
    // channels 0..6 reversed (light 0 -> 3), string 1 channels 6..12 reversed (light 2 -> 9).
    let g = geo(
        "Single Line",
        &[("parm1", "2"), ("parm2", "2"), ("Dir", "R"), ("X2", "300")],
    );
    assert_eq!(channels(&g), [0, 3, 6, 9]);
    near(at(&g, 0), 100.0, 0.0);
    near(at(&g, 3), 0.0, 0.0);
    near(at(&g, 6), 300.0, 0.0);
    near(at(&g, 9), 200.0, 0.0);
}

#[test]
fn single_line_dumb_strings_are_one_single_channel_node_per_string() {
    let g = geo(
        "Single Line",
        &[
            ("NumStrings", "3"),
            ("NodesPerString", "10"),
            ("StringType", "Single Color White"),
            ("X2", "290"),
        ],
    );
    assert_eq!(channels(&g), [0, 1, 2]);
    assert_eq!((g.channels, g.channels_per_node), (3, 1));
    assert!(g.nodes.iter().all(|n| n.points.len() == 10));
    near(g.nodes[0].points[0], 0.0, 0.0);
    near(g.nodes[2].points[9], 290.0, 0.0);
}

#[test]
fn single_line_default_is_fifty_nodes_and_rgbw_uses_four_channels() {
    let g = geo("Single Line", &[("StringType", "GRBW Nodes"), ("X2", "10")]);
    assert_eq!((g.nodes.len(), g.channels_per_node, g.channels), (50, 4, 200));
}

#[test]
fn advanced_string_starts_relative_to_the_model_start() {
    let base = [
        ("NumStrings", "2"),
        ("NodesPerString", "3"),
        ("X2", "50"),
        ("Advanced", "1"),
        ("StartChannel", "!Ctl:1"),
    ];
    let mut a = base.to_vec();
    a.extend([("String1", "!Ctl:1"), ("String2", "!Ctl:101")]);
    let g = geo("Single Line", &a);
    assert_eq!(channels(&g), [0, 3, 6, 100, 103, 106]);
    assert_eq!((g.channels, g.approximate), (109, None));

    let mut b = base.to_vec();
    b.extend([("String1", "!Ctl:1"), ("String2", "!Other:1")]);
    let g = geo("Single Line", &b);
    assert_eq!(channels(&g), [0, 3, 6, 9, 12, 15]);
    assert!(g.approximate.is_some());
}

#[test]
fn channel_block_is_one_channel_per_node_centered_in_cells() {
    let g = geo(
        "Channel Block",
        &[("NumChannels", "4"), ("X2", "40"), ("StringType", "RGB Nodes")],
    );
    assert_eq!(
        (channels(&g), g.channels, g.channels_per_node),
        (vec![0, 1, 2, 3], 4, 1)
    );
    near(at(&g, 0), 5.0, 0.0);
    near(at(&g, 3), 35.0, 0.0);
}

/// 1 string of 32 nodes folded into 8 vertical strands of 4 (BW 8, BH 4); local cell (bx, by)
/// lands at (500 + 10 * (bx - 4), 300 + 10 * (by - 2)).
fn zigzag_matrix(extra: &[(&str, &str)]) -> Geometry {
    let mut a = vec![
        ("NumStrings", "1"),
        ("NodesPerString", "32"),
        ("StrandsPerString", "8"),
        ("WorldPosX", "500"),
        ("WorldPosY", "300"),
        ("ScaleX", "10"),
        ("ScaleY", "10"),
    ];
    a.extend_from_slice(extra);
    geo("Vert Matrix", &a)
}

#[test]
fn vertical_matrix_zig_zags_strands_within_a_string() {
    let g = zigzag_matrix(&[]);
    assert_eq!((g.nodes.len(), g.channels), (32, 96));
    near(at(&g, 0), 460.0, 280.0); // strand 0 bottom
    near(at(&g, 9), 460.0, 310.0); // strand 0 top
    near(at(&g, 12), 470.0, 310.0); // strand 1 starts at the top
    near(at(&g, 21), 470.0, 280.0);
    near(at(&g, 24), 480.0, 280.0); // strand 2 back at the bottom
    near(at(&g, 93), 530.0, 280.0); // last node: strand 7 ends at the bottom
}

#[test]
fn vertical_matrix_start_side_dir_and_no_zig() {
    near(at(&zigzag_matrix(&[("StartSide", "T")]), 0), 460.0, 310.0);
    near(at(&zigzag_matrix(&[("Dir", "R")]), 0), 530.0, 280.0);
    near(at(&zigzag_matrix(&[("NoZig", "true")]), 12), 470.0, 280.0);
}

#[test]
fn horizontal_matrix_rows_and_strands_per_string_remainder() {
    // 2 strings x 5: rows of 5 (BW 5, BH 2), both rows left to right (one strand per string).
    let g = geo("Horiz Matrix", &[("parm1", "2"), ("parm2", "5")]);
    near(at(&g, 0), -2.0, -1.0);
    near(at(&g, 12), 2.0, -1.0);
    near(at(&g, 15), -2.0, 0.0);
    // 10 nodes split in 3 strands keeps 9; string 1 still starts at 10 * 3 channels.
    let g = geo(
        "Matrix",
        &[
            ("NumStrings", "2"),
            ("NodesPerString", "10"),
            ("StrandsPerString", "3"),
        ],
    );
    assert_eq!(g.nodes.len(), 18);
    assert_eq!(channels(&g)[9], 30);
    assert_eq!(g.channels, 57);
}

#[test]
fn matrix_rotation_is_applied() {
    // A 2x1 horizontal matrix: local x = bufX - 1, so node 1 sits at local (0, 0) and node 0 at
    // (-1, 0); rotating 90 degrees about Z moves node 0 to (0, -1) (scaled by 10).
    let g = geo(
        "Matrix",
        &[
            ("NodesPerString", "2"),
            ("RotateZ", "90"),
            ("ScaleX", "10"),
            ("ScaleY", "10"),
        ],
    );
    near(at(&g, 0), 0.0, -10.0);
}

/// One arch of 5 nodes, 180 degrees: L = 5, midpoint 2, width 9; lights at local
/// (1, 0), (5, 5), (9, 0). With point 2 at +/-90 the scale is 10.
fn arch(extra: &[(&str, &str)]) -> Geometry {
    let mut a = vec![
        ("NumArches", "1"),
        ("NodesPerArch", "5"),
        ("WorldPosX", "100"),
        ("X2", "90"),
    ];
    a.extend_from_slice(extra);
    geo("Arches", &a)
}

#[test]
fn arch_lights_follow_a_half_circle() {
    let g = arch(&[]);
    assert_eq!(channels(&g), [0, 3, 6, 9, 12]);
    near(at(&g, 0), 110.0, 0.0);
    near(at(&g, 6), 150.0, 50.0);
    near(at(&g, 12), 190.0, 0.0);
    near(at(&arch(&[("Height", "2")]), 6), 150.0, 100.0);
}

#[test]
fn arch_with_negative_x2_is_mirrored_not_flipped() {
    let g = arch(&[("X2", "-90")]);
    near(at(&g, 0), 90.0, 0.0);
    near(at(&g, 6), 50.0, 50.0);
    near(at(&g, 12), 10.0, 0.0);
}

#[test]
fn arch_dir_r_reverses_channels() {
    let g = arch(&[("Dir", "R")]);
    near(at(&g, 12), 110.0, 0.0);
    near(at(&g, 0), 190.0, 0.0);
}

#[test]
fn candy_cane_has_an_upright_and_a_hook() {
    // 9 lights: 6 upright at x 0, then a hook of radius 1.5 centred at x 1.5, y 5; width 3,
    // so X2 = 30 scales by 10.
    let g = geo(
        "Candy Canes",
        &[("NumCanes", "1"), ("NodesPerCane", "9"), ("X2", "30")],
    );
    assert_eq!(g.nodes.len(), 9);
    near(at(&g, 0), 0.0, 0.0);
    near(at(&g, 15), 0.0, 50.0);
    let hook_y = 50.0 + 15.0 * (2.0 * std::f64::consts::PI / 3.0).sin();
    near(at(&g, 18), 7.5, hook_y);
    near(at(&g, 21), 22.5, hook_y);
    near(at(&g, 24), 30.0, 50.0);
    // Dir=R swaps the canes' start channels.
    let g = geo(
        "Candy Canes",
        &[
            ("NumCanes", "2"),
            ("NodesPerCane", "9"),
            ("Dir", "R"),
            ("X2", "30"),
        ],
    );
    assert_eq!(channels(&g)[0], 0);
    near(at(&g, 27), 0.0, 0.0);
}

#[test]
fn icicles_fill_drops_and_hang_by_height() {
    // 7 lights in drops of 3 and 4: columns 0 and 1, render width 1, X2 = 10 -> scale 10,
    // Height -1 hangs drops downward.
    let a = [
        ("NumStrings", "1"),
        ("NodesPerString", "7"),
        ("DropPattern", "3,4"),
        ("WorldPosY", "100"),
        ("X2", "10"),
        ("Height", "-1"),
    ];
    let g = geo("Icicles", &a);
    assert_eq!(g.nodes.len(), 7);
    near(at(&g, 0), 0.0, 100.0);
    near(at(&g, 6), 0.0, 80.0);
    near(at(&g, 9), 10.0, 100.0);
    near(at(&g, 18), 10.0, 70.0);
    let mut r = a.to_vec();
    r.push(("Dir", "R"));
    near(at(&geo("Icicles", &r), 0), 10.0, 100.0);
}

#[test]
fn custom_model_multi_light_nodes_gaps_and_occupied_centering() {
    // Occupied rows 1..3 and cols 1..3 -> centre (2, 2); node 1 has two lights, node 5 leaves a
    // gap in the numbering (channels 0, 3, 12).
    let a = [
        ("CustomModel", ",,,;,1,,2;,,5,;,1,,"),
        ("ScaleX", "10"),
        ("ScaleY", "10"),
        ("WorldPosX", "200"),
        ("WorldPosY", "100"),
    ];
    let g = geo("Custom", &a);
    assert_eq!((channels(&g), g.channels), (vec![0, 3, 12], 15));
    assert_eq!(g.nodes[0].points.len(), 2);
    near(g.nodes[0].points[0], 190.0, 110.0);
    near(g.nodes[0].points[1], 190.0, 90.0);
    near(at(&g, 3), 210.0, 110.0);
    near(at(&g, 12), 200.0, 100.0);
    let c = [
        ("CustomModelCompressed", "1,1,1;2,1,3;5,2,2;1,3,1"),
        a[1],
        a[2],
        a[3],
        a[4],
    ];
    assert_eq!(geo("Custom", &c), g);
}

#[test]
fn custom_model_depth_tilts_back_layers() {
    // Two layers: node 1 front (layer 0, z +0.5), node 2 back (layer 1, z -0.5); the 0.1 rad
    // 2D perspective moves y by -z * sin(0.1).
    let g = geo("Custom", &[("CustomModel", "1|2"), ("Depth", "2")]);
    let s = f64::from(0.1f32).sin();
    near(at(&g, 0), 0.0, -0.5 * s);
    near(at(&g, 3), 0.0, 0.5 * s);
}

#[test]
fn tree_wraps_strands_around_a_cone_with_perspective() {
    // 16 strands of 10: render height 30, radius 30/1.8/2, top radius 1/6 of that, first strand
    // at -180 + 3 degrees, then the 0.2 rad perspective tilt (values from the TreeModel formulas).
    let g = geo("Tree 360", &[("parm1", "16"), ("parm2", "10")]);
    assert_eq!(g.nodes.len(), 160);
    near(at(&g, 0), -0.436133, -13.047690);
    near(at(&g, 477), 0.463621, 14.961101);
}

#[test]
fn flat_tree_fans_strands() {
    // 4 strands of 5: bottom spread x4, top x0.9, height 10 centred, perspective cos(0.2).
    let g = geo("Tree Flat", &[("NumStrings", "4"), ("NodesPerString", "5")]);
    let c = f64::from(0.2f32).cos();
    near(at(&g, 0), -6.0, -5.0 * c);
    near(at(&g, 57), 1.35, 5.0 * c);
}

#[test]
fn ribbon_tree_nodes_have_three_lights() {
    let g = geo("Tree Ribbon", &[("NumStrings", "4"), ("NodesPerString", "5")]);
    assert!(g.nodes.iter().all(|n| n.points.len() == 3));
}

#[test]
fn circle_starts_at_top_without_start_side_and_goes_clockwise() {
    let g = geo(
        "Circle",
        &[
            ("NumStrings", "1"),
            ("NodesPerString", "4"),
            ("ScaleX", "10"),
            ("ScaleY", "10"),
        ],
    );
    near(at(&g, 0), 0.0, 20.0);
    near(at(&g, 3), 20.0, 0.0);
    near(at(&g, 6), 0.0, -20.0);
    near(at(&g, 9), -20.0, 0.0);
    let b = geo(
        "Circle",
        &[("NodesPerString", "4"), ("StartSide", "B"), ("Dir", "R")],
    );
    near(at(&b, 0), 0.0, -2.0);
    near(at(&b, 3), 2.0, 0.0);
}

#[test]
fn circle_layers_are_concentric_outer_first() {
    // Layer sizes are stored inner to outer; ring 0 (outer, 8 lights, radius 4) is wired first,
    // then the inner ring of 4 at centerPercent 50 -> radius 2.
    let g = geo(
        "Circle",
        &[
            ("NodesPerString", "12"),
            ("LayerSizes", "4,8"),
            ("centerPercent", "50"),
            ("StartSide", "T"),
        ],
    );
    assert_eq!(g.nodes.len(), 12);
    near(at(&g, 0), 0.0, 4.0);
    near(at(&g, 24), 0.0, 2.0);
}

#[test]
fn wreath_snaps_to_an_integer_grid() {
    // 4 lights, radius 2: default bottom start, then left, top, right.
    let g = geo("Wreath", &[("NumStrings", "1"), ("NodesPerString", "4")]);
    near(at(&g, 0), 0.0, -2.0);
    near(at(&g, 3), -2.0, 0.0);
    near(at(&g, 6), 0.0, 2.0);
    near(at(&g, 9), 2.0, 0.0);
}

#[test]
fn star_starts_at_the_bottom_crotch_and_visits_each_vertex() {
    // 10 lights on a 5-point star: buffer 11, outer radius 5.5, inner 5.5 / 2.618034. Bottom
    // centre start is the inner vertex straight down; the next light is the outer tip at 216 deg.
    let g = geo("Star", &[("NumStrings", "1"), ("NodesPerString", "10")]);
    assert_eq!(g.nodes.len(), 10);
    near(at(&g, 0), 0.0, -2.100813);
    near(at(&g, 3), -3.232819, -4.449593);
}

#[test]
fn spinner_arms_point_down_then_rotate() {
    // 2 arms of 3, no hollow: arm 0 points down (270 deg), numbering from the tip (StartSide B);
    // arm 1 is 180 degrees on.
    let g = geo(
        "Spinner",
        &[("NodesPerArm", "3"), ("ArmsPerString", "2"), ("Hollow", "0")],
    );
    assert_eq!(g.nodes.len(), 6);
    near(at(&g, 0), 0.0, -2.5);
    near(at(&g, 6), 0.0, -0.5);
    near(at(&g, 9), 0.0, 2.5);
}

#[test]
fn window_frame_goes_up_the_left_then_across_the_top() {
    let g = geo(
        "Window Frame",
        &[("TopNodes", "3"), ("SideNodes", "2"), ("BottomNodes", "3")],
    );
    assert_eq!(g.nodes.len(), 10);
    near(at(&g, 0), -2.5, -0.5);
    near(at(&g, 3), -2.5, 0.5);
    near(at(&g, 6), -1.25, 0.5);
    near(at(&g, 12), 1.25, 0.5);
    near(at(&g, 15), 2.5, 0.5);
    near(at(&g, 27), -1.25, -0.5);
}

#[test]
fn sphere_rescales_pre_version_8_files() {
    // 8 spokes x 4 latitudes: old files keep their look by scaling y by 4/8.
    let a = [
        ("NumStrings", "8"),
        ("NodesPerString", "4"),
        ("ScaleX", "10"),
        ("ScaleY", "10"),
        ("ScaleZ", "10"),
    ];
    let old = geo("Sphere", &a);
    let mut v8 = a.to_vec();
    v8.push(("versionNumber", "8"));
    let new = geo("Sphere", &v8);
    assert_eq!(old.nodes.len(), 32);
    let (yo, yn) = (at(&old, 0)[1], at(&new, 0)[1]);
    assert!((yo - yn * 0.5).abs() < 1e-4, "{yo} vs {yn}");
    // 4 x 4, version 8: south pole row at -176 deg latitude, radius 4 / 1.8 / 2, tilted 0.1 rad.
    let g = geo(
        "Sphere",
        &[
            ("NumStrings", "4"),
            ("NodesPerString", "4"),
            ("versionNumber", "8"),
        ],
    );
    near(at(&g, 0), 0.000233, -1.095129);
}

#[test]
fn cube_default_style_wires_columns_from_front_bottom_left() {
    let g = geo(
        "Cube",
        &[("CubeWidth", "2"), ("CubeHeight", "2"), ("CubeDepth", "2")],
    );
    assert_eq!((g.nodes.len(), g.channels), (8, 24));
    let c = f64::from(0.1f32).cos();
    near(at(&g, 0), -1.0, -c);
    near(at(&g, 3), -1.0, 0.0);
    let mut cells: Vec<_> = g
        .nodes
        .iter()
        .map(|n| (n.points[0][0].to_bits(), n.points[0][1].to_bits()))
        .collect();
    cells.sort_unstable();
    cells.dedup();
    assert!(
        cells.len() >= 4,
        "front and back cells overlap in 2D but columns differ"
    );
}

#[test]
fn poly_line_explicit_segments_space_lights_with_half_gaps() {
    let a = [
        ("NumPoints", "2"),
        ("PointData", "0,0,0,100,0,0"),
        ("Seg1", "4"),
        ("WorldPosX", "10"),
        ("WorldPosY", "20"),
    ];
    let g = geo("Poly Line", &a);
    assert_eq!(channels(&g), [0, 3, 6, 9]);
    near(at(&g, 0), 22.5, 20.0);
    near(at(&g, 9), 97.5, 20.0);
    let mut r = a.to_vec();
    r.push(("Dir", "R"));
    let g = geo("Poly Line", &r);
    near(at(&g, 9), 22.5, 20.0);
    near(at(&g, 0), 97.5, 20.0);
}

#[test]
fn poly_line_auto_distribution_starts_on_the_first_point() {
    // xLights seeds the segment start at half a gap, so the first light sits on point 1.
    let a = [
        ("NumPoints", "3"),
        ("PointData", "0,0,0,50,0,0,100,0,0"),
        ("NodesPerString", "4"),
    ];
    let g = geo("Poly Line", &a);
    assert_eq!(g.nodes.len(), 4);
    near(at(&g, 0), 0.0, 0.0);
    near(at(&g, 3), 25.0, 0.0);
    near(at(&g, 9), 75.0, 0.0);
}

#[test]
fn multipoint_places_one_node_per_point() {
    let g = geo(
        "MultiPoint",
        &[
            ("NumPoints", "3"),
            ("PointData", "0,0,0,10,5,0,20,0,0"),
            ("WorldPosX", "100"),
            ("WorldPosY", "100"),
        ],
    );
    assert_eq!(channels(&g), [0, 3, 6]);
    near(at(&g, 0), 100.0, 100.0);
    near(at(&g, 3), 110.0, 105.0);
    near(at(&g, 6), 120.0, 100.0);
}

#[test]
fn dmx_image_and_unknown_types() {
    let d = geo("DmxGeneral", &[("DmxChannelCount", "5")]);
    assert_eq!(
        (channels(&d), d.channels, d.channels_per_node),
        (vec![0, 1, 2, 3, 4], 5, 1)
    );
    assert!(d.approximate.is_some());
    let i = geo("Image", &[]);
    assert!(i.nodes.is_empty() && i.approximate.is_none() && i.channels == 0);
    let u = geo("Hologram", &[("parm1", "2"), ("parm2", "3")]);
    assert_eq!((u.nodes.len(), u.channels), (6, 18));
    assert!(u.approximate.is_some());
}

#[test]
fn huge_negative_and_garbage_inputs_never_panic() {
    let huge = geo(
        "Matrix",
        &[("NumStrings", "1000000000"), ("NodesPerString", "1000000000")],
    );
    assert!(huge.nodes.is_empty() && huge.approximate.is_some());
    let types = [
        "Single Line",
        "Poly Line",
        "Arches",
        "Candy Canes",
        "Circle",
        "Star",
        "Tree 360",
        "Tree Flat",
        "Tree Ribbon",
        "Tree",
        "Matrix",
        "Vert Matrix",
        "Custom",
        "Wreath",
        "Icicles",
        "Window Frame",
        "Spinner",
        "Sphere",
        "Channel Block",
        "MultiPoint",
        "Cube",
        "DmxMovingHead",
        "",
        "???",
    ];
    let values = [
        "-5",
        "0",
        "1",
        "2",
        "abc",
        "1e40",
        "nan",
        "-2147483648",
        "99999999999",
        "",
    ];
    let keys = [
        "NumStrings",
        "NodesPerString",
        "StrandsPerString",
        "LightsPerNode",
        "parm1",
        "parm2",
        "parm3",
        "NumArches",
        "NodesPerArch",
        "NumCanes",
        "NodesPerCane",
        "NumPoints",
        "Seg1",
        "PolyStrings",
        "CustomStrings",
        "NodeStart2",
        "LayerSizes",
        "DropPattern",
        "StarPoints",
        "TopNodes",
        "SideNodes",
        "BottomNodes",
        "NodesPerArm",
        "ArmsPerString",
        "CubeWidth",
        "CubeHeight",
        "CubeDepth",
        "ScaleX",
        "X2",
        "Height",
        "Arc",
        "TreeSpiralRotations",
        "exportFirstStrand",
        "starRatio",
        "DmxChannelCount",
        "NumChannels",
        "StringType",
        "Advanced",
        "String2",
    ];
    for t in types {
        for (i, v) in values.iter().enumerate() {
            let mut attrs: Vec<(&str, &str)> = keys.iter().map(|k| (*k, *v)).collect();
            attrs.push(("CustomModel", if i % 2 == 0 { "1,,-3;2,x,4|,9" } else { ";;;" }));
            attrs.push((
                "CustomModelCompressed",
                if i % 3 == 0 { "1,-1,2;3,4;5,0,0,0" } else { "" },
            ));
            attrs.push(("PointData", "1,2"));
            let g = geo(t, &attrs);
            assert!(g.nodes.len() <= 1_000_000);
            assert!(g.nodes.windows(2).all(|w| w[0].channel <= w[1].channel));
            assert!(
                g.nodes
                    .iter()
                    .flat_map(|n| &n.points)
                    .all(|p| p[0].is_finite() && p[1].is_finite())
            );
        }
    }
}
