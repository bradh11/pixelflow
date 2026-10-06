//! Prop shapes: parametric generators or measured point sets.

use crate::Vec3;
use serde::{Deserialize, Serialize};

/// Corner of a matrix where the first pixel is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

/// Direction the wiring runs first in a matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Strings run along rows.
    #[default]
    Horizontal,
    /// Strings run along columns.
    Vertical,
}

/// How a matrix's pixels are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MatrixWiring {
    pub start: Corner,
    pub orientation: Orientation,
    /// When true, every other string runs in the opposite direction (zig-zag).
    pub serpentine: bool,
}

impl Default for MatrixWiring {
    fn default() -> Self {
        Self {
            start: Corner::BottomLeft,
            orientation: Orientation::Horizontal,
            serpentine: true,
        }
    }
}

/// One stretch of a poly line, from one of its points to the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolySegment {
    /// Pixels on this stretch (unused while the line spreads its pixels evenly).
    pub nodes: u32,
    /// The two control points of a curved stretch (a cubic Bézier from this point to the next,
    /// as xLights draws curves), in prop-local coordinates; `None` for a straight stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<[Vec3; 2]>,
}

impl PolySegment {
    pub fn straight(nodes: u32) -> Self {
        Self { nodes, curve: None }
    }
}

/// Parametric prop shapes. Positions are produced by `pf-geometry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Generator {
    /// Straight run of evenly spaced pixels along X, centered on the origin.
    Line { nodes: u32, length: f32 },
    /// Half-ellipse from left to right; origin at the base center.
    Arch { nodes: u32, width: f32, height: f32 },
    /// Ring starting at the top and running clockwise; centered.
    Circle { nodes: u32, radius: f32 },
    /// Grid of pixels wired according to `wiring`; centered.
    Matrix {
        columns: u32,
        rows: u32,
        width: f32,
        height: f32,
        #[serde(default)]
        wiring: MatrixWiring,
    },
    /// Cone of strings running bottom to top; origin at the base center.
    Tree {
        strings: u32,
        nodes_per_string: u32,
        height: f32,
        base_radius: f32,
        top_radius: f32,
        #[serde(default)]
        serpentine: bool,
    },
    /// Star outline with `points` tips, pixels spaced evenly along the outline; centered.
    Star {
        points: u32,
        nodes: u32,
        outer_radius: f32,
        inner_radius: f32,
    },
    /// A line through any number of points, which can bend and curve (xLights' Poly Line).
    /// Pixels run from the first point to the last. Each stretch between two points has its own
    /// pixel count, spaced evenly with half a gap at each end (so the pixels stay evenly spaced
    /// across a corner), unless `spread_nodes` is set: then that many pixels are spread evenly
    /// along the whole line, the first on the first point, as xLights' "auto distribute" does.
    PolyLine {
        vertices: Vec<Vec3>,
        /// One per stretch: `vertices.len() - 1` of them.
        segments: Vec<PolySegment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spread_nodes: Option<u32>,
    },
    /// Free-form grid. `cells` is row-major starting at the top row;
    /// 0 is an empty cell and n places node n (1-based) in that cell.
    CustomGrid {
        columns: u32,
        rows: u32,
        cells: Vec<u32>,
    },
}

impl Generator {
    /// Number of pixels this generator produces.
    pub fn node_count(&self) -> u32 {
        match self {
            Generator::Line { nodes, .. }
            | Generator::Arch { nodes, .. }
            | Generator::Circle { nodes, .. }
            | Generator::Star { nodes, .. } => *nodes,
            Generator::Matrix { columns, rows, .. } => columns.saturating_mul(*rows),
            Generator::Tree {
                strings,
                nodes_per_string,
                ..
            } => strings.saturating_mul(*nodes_per_string),
            Generator::CustomGrid { cells, .. } => cells.iter().copied().max().unwrap_or(0),
            Generator::PolyLine {
                segments,
                spread_nodes,
                ..
            } => spread_nodes
                .unwrap_or_else(|| segments.iter().fold(0u32, |sum, s| sum.saturating_add(s.nodes))),
        }
    }
}

/// Where measured positions came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provenance {
    CameraMap,
    Import,
    Manual,
}

/// The source of a prop's pixel positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ShapeSource {
    /// Positions computed from parameters.
    Generator(Generator),
    /// Positions captured directly (camera mapping, import), in prop-local coordinates.
    Measured {
        points: Vec<Vec3>,
        provenance: Provenance,
    },
}

impl ShapeSource {
    /// Number of pixels in the shape.
    pub fn node_count(&self) -> u32 {
        match self {
            ShapeSource::Generator(g) => g.node_count(),
            ShapeSource::Measured { points, .. } => u32::try_from(points.len()).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_counts() {
        let cases = [
            (
                Generator::Line {
                    nodes: 50,
                    length: 10.0,
                },
                50,
            ),
            (
                Generator::Matrix {
                    columns: 20,
                    rows: 10,
                    width: 4.0,
                    height: 2.0,
                    wiring: MatrixWiring::default(),
                },
                200,
            ),
            (
                Generator::Tree {
                    strings: 16,
                    nodes_per_string: 50,
                    height: 5.0,
                    base_radius: 1.0,
                    top_radius: 0.1,
                    serpentine: false,
                },
                800,
            ),
            (
                Generator::CustomGrid {
                    columns: 3,
                    rows: 1,
                    cells: vec![2, 0, 5],
                },
                5,
            ),
        ];
        for (generator, expected) in cases {
            assert_eq!(generator.node_count(), expected, "{generator:?}");
        }
    }

    fn poly(spread_nodes: Option<u32>) -> Generator {
        Generator::PolyLine {
            vertices: vec![Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)],
            segments: vec![
                PolySegment::straight(10),
                PolySegment {
                    nodes: 5,
                    curve: Some([Vec3::new(1.5, 0.2, 0.0), Vec3::new(1.5, 0.8, 0.0)]),
                },
            ],
            spread_nodes,
        }
    }

    #[test]
    fn poly_line_counts_its_segments_or_its_spread() {
        assert_eq!(poly(None).node_count(), 15);
        assert_eq!(poly(Some(40)).node_count(), 40);
        let huge = Generator::PolyLine {
            vertices: vec![Vec3::ZERO; 3],
            segments: vec![PolySegment::straight(u32::MAX), PolySegment::straight(2)],
            spread_nodes: None,
        };
        assert_eq!(huge.node_count(), u32::MAX);
    }

    #[test]
    fn poly_line_json_is_camel_case_and_leaves_out_what_is_unset() {
        let json = serde_json::to_value(ShapeSource::Generator(poly(None))).unwrap();
        assert_eq!(json["type"], "polyLine");
        assert_eq!(json["vertices"][1]["x"], 1.0);
        assert_eq!(json["segments"][0], serde_json::json!({ "nodes": 10 }));
        assert_eq!(json["segments"][1]["curve"][0]["x"], 1.5);
        assert!(json.get("spreadNodes").is_none());
        let back: ShapeSource = serde_json::from_value(json).unwrap();
        assert_eq!(back, ShapeSource::Generator(poly(None)));
        let spread = serde_json::to_value(poly(Some(7))).unwrap();
        assert_eq!(spread["spreadNodes"], 7);
    }

    #[test]
    fn generator_shape_json_is_flat_and_camel_case() {
        let shape = ShapeSource::Generator(Generator::Arch {
            nodes: 50,
            width: 4.0,
            height: 2.0,
        });
        let json = serde_json::to_value(&shape).unwrap();
        assert_eq!(json["source"], "generator");
        assert_eq!(json["type"], "arch");
        assert_eq!(json["nodes"], 50);
        let back: ShapeSource = serde_json::from_value(json).unwrap();
        assert_eq!(back, shape);
    }

    #[test]
    fn measured_shape_round_trips_and_counts_points() {
        let shape = ShapeSource::Measured {
            points: vec![Vec3::ZERO, Vec3::ONE],
            provenance: Provenance::CameraMap,
        };
        assert_eq!(shape.node_count(), 2);
        let json = serde_json::to_string(&shape).unwrap();
        assert_eq!(serde_json::from_str::<ShapeSource>(&json).unwrap(), shape);
    }

    #[test]
    fn partial_matrix_wiring_fills_defaults() {
        let wiring: MatrixWiring = serde_json::from_str(r#"{ "start": "topLeft" }"#).unwrap();
        assert_eq!(
            wiring,
            MatrixWiring {
                start: Corner::TopLeft,
                ..MatrixWiring::default()
            }
        );
        assert!(wiring.serpentine);
    }

    #[test]
    fn tree_fields_use_camel_case() {
        let json = serde_json::to_value(Generator::Tree {
            strings: 2,
            nodes_per_string: 3,
            height: 1.0,
            base_radius: 1.0,
            top_radius: 0.5,
            serpentine: true,
        })
        .unwrap();
        assert_eq!(json["nodesPerString"], 3);
        assert_eq!(json["baseRadius"], 1.0);
    }
}
