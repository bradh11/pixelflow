//! Every generator yields exactly `node_count()` finite positions, and transforms place them as
//! the layout editor expects.

use pf_geometry::{local_positions, world_positions};
use pf_model::{
    Corner, Generator, MatrixWiring, Orientation, PolySegment, Prop, Provenance, ShapeSource, Transform, Vec3,
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
        (0u32..500, size.clone(), size.clone()).prop_map(|(nodes, width, height)| Generator::Arch {
            nodes,
            width,
            height
        }),
        (0u32..500, size.clone()).prop_map(|(nodes, radius)| Generator::Circle { nodes, radius }),
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
            0u32..32,
            0u32..100,
            size.clone(),
            size.clone(),
            size.clone(),
            any::<bool>()
        )
            .prop_map(
                |(strings, nodes_per_string, height, base_radius, top_radius, serpentine)| Generator::Tree {
                    strings,
                    nodes_per_string,
                    height,
                    base_radius,
                    top_radius,
                    serpentine,
                }
            ),
        (0u32..12, 0u32..500, size.clone(), size).prop_map(|(points, nodes, outer_radius, inner_radius)| {
            Generator::Star {
                points,
                nodes,
                outer_radius,
                inner_radius,
            }
        }),
        (1u32..20, 1u32..20)
            .prop_flat_map(|(columns, rows)| {
                let cells = proptest::collection::vec(0u32..50, (columns * rows) as usize);
                (Just(columns), Just(rows), cells)
            })
            .prop_map(|(columns, rows, cells)| Generator::CustomGrid { columns, rows, cells }),
        poly_line(),
        candy_canes(),
        icicles(),
    ]
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
        Generator::Arch {
            nodes: 7,
            width: 4.0,
            height: 2.0,
        },
        Generator::Circle {
            nodes: 8,
            radius: 1.5,
        },
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
        },
        Generator::Star {
            points: 5,
            nodes: 20,
            outer_radius: 1.0,
            inner_radius: 0.4,
        },
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
        },
        Generator::Icicles {
            strings: 2,
            lights_per_string: 10,
            drops: vec![3, 4, 5, 4],
            width: 4.0,
            drop_height: 0.4,
            alternate_nodes: true,
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
