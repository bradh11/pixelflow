//! Turns prop shapes into pixel positions.
//!
//! Every generator returns exactly `node_count()` points in prop-local coordinates,
//! in wiring order (index 0 is the first pixel on the wire).

mod arch;
mod candy_canes;
mod circle;
mod cube;
mod custom_grid;
mod icicles;
mod line;
mod matrix;
pub mod polyline;
mod sphere;
mod spinner;
mod star;
mod transform;
mod tree;
mod window_frame;
mod wreath;

use pf_model::{Generator, Prop, ShapeSource, Vec3};

pub use cube::cube_cells;
pub use icicles::icicle_column_gaps;
pub use sphere::globe_point;
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

/// The grid a shape's pixels are drawn on when an effect draws on the prop by itself: xLights'
/// default render buffer for that kind of model, which follows the strings rather than where
/// the pixels sit in the layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnGrid {
    pub columns: u32,
    pub rows: u32,
    /// Each pixel's column (from the left) and row (from the bottom), in wiring order. Pixels
    /// can share a cell, and cells can be empty.
    pub cells: Vec<[u32; 2]>,
}

/// The grid of its own that `generator`'s shape is drawn on. `None` for the shapes drawn where
/// their pixels sit: lines and poly lines (xLights runs their buffer from the first point drawn
/// to the last, whichever end is wired first, which a PixelFlow line doesn't record), icicles
/// (the same goes for them), stars and wreaths (whose xLights buffer is a picture of the shape),
/// cubes, and matrices (which the renderer lays out itself).
pub fn own_grid(generator: &Generator) -> Option<OwnGrid> {
    let grid = match *generator {
        Generator::Tree {
            strings,
            nodes_per_string,
            serpentine,
            start,
            strands_per_string,
            alternate_nodes,
            ..
        } => tree::grid(tree::Wiring {
            strings,
            nodes_per_string,
            serpentine,
            start,
            strands_per_string,
            alternate_nodes,
        }),
        Generator::Arch {
            nodes,
            arches,
            start_right,
            ref layers,
            zig_zag,
            start_inside,
            ..
        } => arch::grid(arch::Wiring {
            nodes,
            arches,
            start_right,
            layers,
            zig_zag,
            start_inside,
        }),
        Generator::CandyCanes {
            canes,
            nodes_per_cane,
            alternate_nodes,
            start_right,
            ..
        } => candy_canes::grid(canes, nodes_per_cane, alternate_nodes, start_right),
        Generator::Circle {
            nodes,
            ref layers,
            start_inside,
            ..
        } => circle::grid(nodes, layers, start_inside),
        Generator::WindowFrame {
            top,
            sides,
            bottom,
            start,
            counter_clockwise,
            ..
        } => window_frame::grid(top, sides, bottom, start, counter_clockwise),
        Generator::Spinner {
            arms,
            nodes_per_arm,
            zig_zag,
            alternate,
            from_center,
            clockwise,
            ..
        } => spinner::grid(spinner::Wiring {
            arms,
            nodes_per_arm,
            zig_zag,
            alternate,
            from_center,
            clockwise,
        }),
        Generator::Sphere {
            columns,
            rows,
            start,
            strand_style,
            ..
        } => sphere::grid(columns, rows, start, strand_style),
        Generator::CustomGrid {
            columns,
            rows,
            ref cells,
        } => custom_grid::grid(columns, rows, cells),
        Generator::Line { .. }
        | Generator::PolyLine { .. }
        | Generator::Icicles { .. }
        | Generator::Star { .. }
        | Generator::Wreath { .. }
        | Generator::Cube { .. }
        | Generator::Matrix { .. } => return None,
    };
    // A shape with no pixels has no grid.
    (grid.columns > 0 && grid.rows > 0 && !grid.cells.is_empty()).then_some(grid)
}

fn generate(generator: &Generator) -> Vec<Vec3> {
    match *generator {
        Generator::Line { nodes, length } => line::positions(nodes, length),
        Generator::Arch {
            nodes,
            width,
            height,
            arches,
            arc,
            gap,
            skew_deg,
            start_right,
            ref layers,
            hollow,
            zig_zag,
            start_inside,
        } => arch::positions(arch::Arch {
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
        }),
        Generator::Circle {
            nodes,
            radius,
            ref layers,
            inner_percent,
            start_inside,
            start_at_bottom,
            counter_clockwise,
        } => circle::positions(circle::Circle {
            nodes,
            radius,
            layers,
            inner_percent,
            start_inside,
            start_at_bottom,
            counter_clockwise,
        }),
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
            style,
            degrees,
            start_angle,
            start,
            strands_per_string,
            alternate_nodes,
            spiral_rotations,
        } => tree::positions(tree::Tree {
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
        }),
        Generator::Star {
            points,
            nodes,
            outer_radius,
            inner_radius,
            start,
            counter_clockwise,
            ref layers,
            inner_percent,
            start_inside,
        } => star::positions(star::Star {
            points,
            nodes,
            outer_radius,
            inner_radius,
            start,
            counter_clockwise,
            layers,
            inner_percent,
            start_inside,
        }),
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
            start_right,
        } => candy_canes::positions(candy_canes::Canes {
            canes,
            nodes_per_cane,
            width,
            height,
            cane_height,
            reverse,
            sticks,
            alternate_nodes,
            skew_deg,
            start_right,
        }),
        Generator::Icicles {
            strings,
            lights_per_string,
            ref drops,
            width,
            drop_height,
            alternate_nodes,
        } => icicles::positions(icicles::Icicles {
            strings,
            lights_per_string,
            drops,
            width,
            drop_height,
            alternate_nodes,
        }),
        Generator::WindowFrame {
            top,
            sides,
            bottom,
            width,
            height,
            start,
            counter_clockwise,
        } => window_frame::positions(window_frame::Frame {
            top,
            sides,
            bottom,
            width,
            height,
            start,
            counter_clockwise,
        }),
        Generator::Wreath {
            nodes,
            radius,
            start_at_bottom,
            counter_clockwise,
        } => wreath::positions(nodes, radius, start_at_bottom, counter_clockwise),
        Generator::Spinner {
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
        } => spinner::positions(spinner::Spinner {
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
        }),
        Generator::Sphere {
            columns,
            rows,
            radius,
            start_latitude,
            end_latitude,
            degrees,
            start,
            strand_style,
        } => sphere::positions(sphere::Sphere {
            columns,
            rows,
            radius,
            start_latitude,
            end_latitude,
            degrees,
            start,
            strand_style,
        }),
        Generator::Cube {
            width,
            height,
            depth,
            spacing,
            start,
            style,
            strand_style,
            strand_per_layer,
        } => cube::positions(cube::Cube {
            width,
            height,
            depth,
            spacing,
            start,
            style,
            strand_style,
            strand_per_layer,
        }),
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
