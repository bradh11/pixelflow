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
#[serde(rename_all = "camelCase")]
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
