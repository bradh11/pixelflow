//! Every generator yields exactly `node_count()` finite positions.

use pf_geometry::local_positions;
use pf_model::{Corner, Generator, MatrixWiring, Orientation, ShapeSource};
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
    ]
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
