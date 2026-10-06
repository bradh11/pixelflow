//! Turns prop shapes into pixel positions.
//!
//! Every generator returns exactly `node_count()` points in prop-local coordinates,
//! in wiring order (index 0 is the first pixel on the wire).

mod arch;
mod circle;
mod custom_grid;
mod line;
mod matrix;
pub mod polyline;
mod star;
mod transform;
mod tree;

use pf_model::{Generator, Prop, ShapeSource, Vec3};

pub use transform::apply_transform;

/// Pixel positions in prop-local coordinates, in wiring order.
pub fn local_positions(shape: &ShapeSource) -> Vec<Vec3> {
    match shape {
        ShapeSource::Generator(generator) => generate(generator),
        ShapeSource::Measured { points, .. } => points.clone(),
    }
}

/// Pixel positions in layout coordinates (prop transform applied), in wiring order.
pub fn world_positions(prop: &Prop) -> Vec<Vec3> {
    local_positions(&prop.shape)
        .into_iter()
        .map(|p| apply_transform(p, &prop.transform))
        .collect()
}

fn generate(generator: &Generator) -> Vec<Vec3> {
    match *generator {
        Generator::Line { nodes, length } => line::positions(nodes, length),
        Generator::Arch { nodes, width, height } => arch::positions(nodes, width, height),
        Generator::Circle { nodes, radius } => circle::positions(nodes, radius),
        Generator::Matrix {
            columns,
            rows,
            width,
            height,
            wiring,
        } => matrix::positions(columns, rows, width, height, wiring),
        Generator::Tree {
            strings,
            nodes_per_string,
            height,
            base_radius,
            top_radius,
            serpentine,
        } => tree::positions(
            strings,
            nodes_per_string,
            height,
            base_radius,
            top_radius,
            serpentine,
        ),
        Generator::Star {
            points,
            nodes,
            outer_radius,
            inner_radius,
        } => star::positions(points, nodes, outer_radius, inner_radius),
        Generator::CustomGrid {
            columns,
            rows,
            ref cells,
        } => custom_grid::positions(columns, rows, cells),
        Generator::PolyLine {
            ref vertices,
            ref segments,
            spread_nodes,
        } => polyline::positions(vertices, segments, spread_nodes),
    }
}

/// Fraction `i / (n - 1)` in `[0, 1]`; a single node sits at 0.5.
pub(crate) fn spread(i: u32, n: u32) -> f32 {
    if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 }
}

/// Test helper: asserts two points are within 1e-4 of each other.
#[cfg(test)]
pub(crate) fn assert_close(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 1e-4, "expected {b:?}, got {a:?}");
}
