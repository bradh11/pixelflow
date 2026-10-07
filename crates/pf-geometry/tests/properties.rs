//! Every generator yields exactly `node_count()` finite positions, and transforms place them as
//! the layout editor expects.

use pf_geometry::{local_positions, world_positions};
use pf_model::{
    Corner, CubeStart, CubeStyle, Generator, MatrixWiring, Orientation, PolySegment, Prop, Provenance,
    ShapeSource, StarStart, StrandStyle, Transform, TreeStyle, Vec3,
};
use proptest::prelude::*;

fn corner() -> impl Strategy<Value = Corner> {
    prop_oneof![
        Just(Corner::BottomLeft),
        Just(Corner::BottomRight),
        Just(Corner::TopLeft),
        Just(Corner::TopRight)
    ]
}

fn generator() -> impl Strategy<Value = Generator> {
    let size = 0.1f32..100.0;
    prop_oneof![
        (0u32..500, size.clone()).prop_map(|(nodes, length)| Generator::Line { nodes, length }),
        arch(),
        circle(),
        (
            0u32..40,
            0u32..40,
            size.clone(),
            size.clone(),
            corner(),
            any::<bool>(),
            any::<bool>()
        )
            .prop_map(|(columns, rows, width, height, start, vertical, serpentine)| {
                Generator::Matrix {
                    columns,
                    rows,
                    width,
                    height,
                    wiring: MatrixWiring {
                        start,
                        orientation: if vertical {
                            Orientation::Vertical
                        } else {
                            Orientation::Horizontal
                        },
                        serpentine,
                    },
                }
            }),
        (
            (0u32..32, 0u32..100),
            (size.clone(), size.clone(), size.clone()),
            any::<bool>(),
            prop_oneof![
                Just(TreeStyle::Round),
                Just(TreeStyle::Flat),
                Just(TreeStyle::Ribbon)
            ],
            (1f32..=360.0, -360f32..360.0),
            (corner(), 0u32..8, any::<bool>(), -12f32..12.0),
        )
            .prop_map(
                |(
                    (strings, nodes_per_string),
                    (height, base_radius, top_radius),
                    serpentine,
                    style,
                    (degrees, start_angle),
                    (start, strands_per_string, alternate_nodes, spiral_rotations),
                )| {
                    Generator::Tree {
                        strings,
                        nodes_per_string,
                        height,
                        base_radius,
                        top_radius,
                        serpentine,
                        style,
                        degrees,
                        start_angle,
                        start,
                        strands_per_string,
                        alternate_nodes,
                        spiral_rotations,
                    }
                }
            ),
        star(),
        (1u32..20, 1u32..20)
            .prop_flat_map(|(columns, rows)| {
                let cells = proptest::collection::vec(0u32..50, (columns * rows) as usize);
                (Just(columns), Just(rows), cells)
            })
            .prop_map(|(columns, rows, cells)| Generator::CustomGrid { columns, rows, cells }),
        poly_line(),
        candy_canes(),
        icicles(),
        window_frame(),
        (0u32..500, 0.1f32..20.0, any::<bool>(), any::<bool>()).prop_map(
            |(nodes, radius, start_at_bottom, counter_clockwise)| Generator::Wreath {
                nodes,
                radius,
                start_at_bottom,
                counter_clockwise,
            }
        ),
        spinner(),
        sphere(),
        cube(),
    ]
}

fn strand_style() -> impl Strategy<Value = StrandStyle> {
    prop_oneof![
        Just(StrandStyle::ZigZag),
        Just(StrandStyle::NoZigZag),
        Just(StrandStyle::AlternatePixel)
    ]
}

fn sphere() -> impl Strategy<Value = Generator> {
    (
        (0u32..30, 0u32..30, 0.1f32..20.0),
        (-90f32..=90.0, -90f32..=90.0, 1f32..=360.0),
        corner(),
        strand_style(),
    )
        .prop_map(
            |((columns, rows, radius), (start_latitude, end_latitude, degrees), start, strand_style)| {
                Generator::Sphere {
                    columns,
                    rows,
                    radius,
                    start_latitude,
                    end_latitude,
                    degrees,
                    start,
                    strand_style,
                }
            },
        )
}

fn cube() -> impl Strategy<Value = Generator> {
    (
        (0u32..8, 0u32..8, 0u32..8, 0.01f32..2.0),
        (0usize..8, 0usize..6),
        strand_style(),
        any::<bool>(),
    )
        .prop_map(
            |((width, height, depth, spacing), (start, style), strand_style, strand_per_layer)| {
                const STARTS: [CubeStart; 8] = [
                    CubeStart::FrontBottomLeft,
                    CubeStart::FrontBottomRight,
                    CubeStart::FrontTopLeft,
                    CubeStart::FrontTopRight,
                    CubeStart::BackBottomLeft,
                    CubeStart::BackBottomRight,
                    CubeStart::BackTopLeft,
                    CubeStart::BackTopRight,
                ];
                const STYLES: [CubeStyle; 6] = [
                    CubeStyle::VerticalFrontBack,
                    CubeStyle::VerticalLeftRight,
                    CubeStyle::HorizontalFrontBack,
                    CubeStyle::HorizontalLeftRight,
                    CubeStyle::StackedFrontBack,
                    CubeStyle::StackedLeftRight,
                ];
                Generator::Cube {
                    width,
                    height,
                    depth,
                    spacing,
                    start: STARTS[start],
                    style: STYLES[style],
                    strand_style,
                    strand_per_layer,
                }
            },
        )
}

/// Circles, plain and layered, including layers holding more or fewer pixels than the circle.
fn circle() -> impl Strategy<Value = Generator> {
    (
        (0u32..300, 0.1f32..50.0),
        (proptest::collection::vec(0u32..40, 0..6), 0u32..=100),
        (any::<bool>(), any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(
                (nodes, radius),
                (layers, inner_percent),
                (start_inside, start_at_bottom, counter_clockwise),
            )| {
                Generator::Circle {
                    nodes,
                    radius,
                    layers,
                    inner_percent,
                    start_inside,
                    start_at_bottom,
                    counter_clockwise,
                }
            },
        )
}

/// Stars from every start, plain and layered.
fn star() -> impl Strategy<Value = Generator> {
    let start = prop_oneof![
        Just(StarStart::Top),
        Just(StarStart::Bottom),
        Just(StarStart::LeftLeg),
        Just(StarStart::RightLeg)
    ];
    (
        (0u32..12, 0u32..300, 0.1f32..50.0, 0.0f32..50.0),
        (start, any::<bool>()),
        (
            proptest::collection::vec(0u32..40, 0..6),
            0u32..=100,
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                (points, nodes, outer_radius, inner_radius),
                (start, counter_clockwise),
                (layers, inner_percent, start_inside),
            )| Generator::Star {
                points,
                nodes,
                outer_radius,
                inner_radius,
                start,
                counter_clockwise,
                layers,
                inner_percent,
                start_inside,
            },
        )
}

/// Arches: rows of them and layered ones, including arcs and leans past xLights' limits and
/// layers that hold more or fewer pixels than the arch has.
fn arch() -> impl Strategy<Value = Generator> {
    (
        (0u32..300, 0.1f32..50.0, 0.1f32..50.0, 0u32..6),
        (-10f32..400.0, -5f32..5.0, -200f32..200.0, any::<bool>()),
        (
            proptest::collection::vec(0u32..40, 0..6),
            0u32..=100,
            any::<bool>(),
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                (nodes, width, height, arches),
                (arc, gap, skew_deg, start_right),
                (layers, hollow, zig_zag, start_inside),
            )| Generator::Arch {
                nodes,
                width,
                height,
                arches,
                arc,
                gap,
                skew_deg,
                start_right,
                layers,
                hollow,
                zig_zag,
                start_inside,
            },
        )
}

fn window_frame() -> impl Strategy<Value = Generator> {
    (
        (0u32..40, 0u32..40, 0u32..40),
        (0.1f32..20.0, 0.1f32..20.0),
        corner(),
        any::<bool>(),
    )
        .prop_map(
            |((top, sides, bottom), (width, height), start, counter_clockwise)| Generator::WindowFrame {
                top,
                sides,
                bottom,
                width,
                height,
                start,
                counter_clockwise,
            },
        )
}

fn spinner() -> impl Strategy<Value = Generator> {
    (
        (0u32..40, 0u32..30, 0u32..=100),
        (-360f32..360.0, 1f32..=360.0, 0.1f32..20.0),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(
                (arms, nodes_per_arm, hollow),
                (start_angle, arc, radius),
                (zig_zag, alternate, from_center, clockwise),
            )| Generator::Spinner {
                arms,
                nodes_per_arm,
                hollow,
                start_angle,
                arc,
                zig_zag,
                alternate,
                from_center,
                clockwise,
                radius,
            },
        )
}

fn candy_canes() -> impl Strategy<Value = Generator> {
    (
        (0u32..8, 0u32..60, 0.1f32..20.0),
        (-3f32..3.0, -3f32..3.0, -90f32..90.0),
        (any::<bool>(), any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(
                (canes, nodes_per_cane, width),
                (height, cane_height, skew_deg),
                (reverse, sticks, alternate_nodes),
            )| {
                Generator::CandyCanes {
                    canes,
                    nodes_per_cane,
                    width,
                    height,
                    cane_height,
                    reverse,
                    sticks,
                    alternate_nodes,
                    skew_deg,
                    start_right: canes % 2 == 0,
                }
            },
        )
}

/// Icicles, including patterns with gaps, no drops at all, or no drop holding pixels.
fn icicles() -> impl Strategy<Value = Generator> {
    (
        0u32..6,
        0u32..120,
        proptest::collection::vec(0u32..8, 0..8),
        0.1f32..20.0,
        -5f32..5.0,
        any::<bool>(),
    )
        .prop_map(
            |(strings, lights_per_string, drops, width, drop_height, alternate_nodes)| Generator::Icicles {
                strings,
                lights_per_string,
                drops,
                width,
                drop_height,
                alternate_nodes,
            },
        )
}

/// Poly lines, including damaged ones: too few points, or stretches that don't match them.
fn poly_line() -> impl Strategy<Value = Generator> {
    let point = (-50f32..50.0, -50f32..50.0, -5f32..5.0).prop_map(|(x, y, z)| Vec3::new(x, y, z));
    let segment =
        (0u32..60, proptest::option::of((point.clone(), point.clone()))).prop_map(|(nodes, curve)| {
            PolySegment {
                nodes,
                curve: curve.map(|(a, b)| [a, b]),
            }
        });
    (
        proptest::collection::vec(point, 0..12),
        proptest::collection::vec(segment, 0..12),
        proptest::option::of(0u32..300),
    )
        .prop_map(|(vertices, segments, spread_nodes)| Generator::PolyLine {
            vertices,
            segments,
            spread_nodes,
        })
}

proptest! {
    #[test]
    fn generators_produce_node_count_finite_points(generator in generator()) {
        let shape = ShapeSource::Generator(generator);
        let points = local_positions(&shape);
        prop_assert_eq!(points.len(), shape.node_count() as usize);
        prop_assert!(points.iter().all(|p| p.is_finite()));
    }
}

fn shape() -> impl Strategy<Value = ShapeSource> {
    prop_oneof![
        generator().prop_map(ShapeSource::Generator),
        proptest::collection::vec((-50f32..50.0, -50f32..50.0, -5f32..5.0), 0..60).prop_map(|points| {
            ShapeSource::Measured {
                points: points.into_iter().map(|(x, y, z)| Vec3::new(x, y, z)).collect(),
                provenance: Provenance::Import,
            }
        }),
    ]
}

fn close(a: Vec3, b: Vec3, scale: f32) -> bool {
    (a - b).length() <= 1e-3 * (1.0 + scale)
}

proptest! {
    /// What the layout editor relies on: a prop's transform scales its local points about the
    /// prop's origin, turns them about that origin (Z: counter-clockwise in the front view), then
    /// moves them, for every kind of shape.
    #[test]
    fn transforms_scale_and_rotate_about_the_prop_origin(
        shape in shape(),
        degrees in -360f32..360.0,
        sx in 0.1f32..5.0,
        sy in 0.1f32..5.0,
        x in -100f32..100.0,
        y in -100f32..100.0,
    ) {
        let mut prop = Prop::new("P", shape);
        prop.transform = Transform {
            position: Vec3::new(x, y, 0.0),
            rotation_deg: Vec3::new(0.0, 0.0, degrees),
            scale: Vec3::new(sx, sy, 1.0),
        };
        let local = local_positions(&prop.shape);
        let world = world_positions(&prop);
        prop_assert_eq!(local.len(), world.len());
        let (s, c) = degrees.to_radians().sin_cos();
        for (l, w) in local.iter().zip(&world) {
            let (px, py) = (l.x * sx, l.y * sy);
            let expected = Vec3::new(x + px * c - py * s, y + px * s + py * c, l.z);
            let size = l.length() * sx.max(sy) + x.abs().max(y.abs());
            prop_assert!(close(*w, expected, size), "{:?} vs {:?}", w, expected);
        }
    }
}

#[test]
fn a_quarter_turn_maps_right_to_up_for_every_generator() {
    let generators = [
        Generator::Line {
            nodes: 5,
            length: 4.0,
        },
        Generator::arch(7, 4.0, 2.0),
        Generator::circle(8, 1.5),
        Generator::Matrix {
            columns: 4,
            rows: 3,
            width: 4.0,
            height: 2.0,
            wiring: MatrixWiring::default(),
        },
        Generator::Tree {
            strings: 4,
            nodes_per_string: 5,
            height: 5.0,
            base_radius: 1.5,
            top_radius: 0.2,
            serpentine: true,
            style: TreeStyle::Round,
            degrees: 360.0,
            start_angle: 0.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
        },
        Generator::star(5, 20, 1.0, 0.4),
        Generator::CustomGrid {
            columns: 3,
            rows: 2,
            cells: vec![1, 0, 2, 0, 3, 0],
        },
        Generator::PolyLine {
            vertices: vec![Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 3.0, 0.0)],
            segments: vec![
                PolySegment::straight(4),
                PolySegment {
                    nodes: 6,
                    curve: Some([Vec3::new(3.0, 1.0, 0.0), Vec3::new(3.0, 2.0, 0.0)]),
                },
            ],
            spread_nodes: None,
        },
        Generator::CandyCanes {
            canes: 2,
            nodes_per_cane: 12,
            width: 3.0,
            height: 1.2,
            cane_height: 0.8,
            reverse: true,
            sticks: false,
            alternate_nodes: true,
            skew_deg: 10.0,
            start_right: true,
        },
        Generator::Icicles {
            strings: 2,
            lights_per_string: 10,
            drops: vec![3, 4, 5, 4],
            width: 4.0,
            drop_height: 0.4,
            alternate_nodes: true,
        },
        Generator::WindowFrame {
            top: 6,
            sides: 4,
            bottom: 5,
            width: 3.0,
            height: 2.0,
            start: Corner::TopRight,
            counter_clockwise: true,
        },
        Generator::Wreath {
            nodes: 30,
            radius: 1.5,
            start_at_bottom: true,
            counter_clockwise: false,
        },
        Generator::Spinner {
            arms: 5,
            nodes_per_arm: 8,
            hollow: 20,
            start_angle: 30.0,
            arc: 270.0,
            zig_zag: true,
            alternate: false,
            from_center: true,
            clockwise: true,
            radius: 2.0,
        },
    ];
    for generator in generators {
        let mut prop = Prop::new("P", ShapeSource::Generator(generator.clone()));
        prop.transform.position = Vec3::new(10.0, 20.0, 0.0);
        prop.transform.rotation_deg = Vec3::new(0.0, 0.0, 90.0);
        prop.transform.scale = Vec3::new(2.0, 2.0, 2.0);
        for (l, w) in local_positions(&prop.shape).iter().zip(world_positions(&prop)) {
            let expected = Vec3::new(10.0 - 2.0 * l.y, 20.0 + 2.0 * l.x, 2.0 * l.z);
            assert!(close(w, expected, 30.0), "{generator:?}: {w:?} vs {expected:?}");
        }
    }
}
