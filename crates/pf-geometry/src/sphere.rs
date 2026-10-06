//! Spheres as xLights lays them out (`SphereModel::SetSphereCoord`): a vertical matrix's strands
//! wrapped round a globe.

use pf_model::{Corner, StrandStyle, Vec3};
use std::f64::consts::PI;

/// The point of a globe of radius 1 where xLights puts column `column` (of `columns`) and row
/// `row` (of `rows`, from the south). Columns start at the back (nudged a little right, as
/// xLights does) and run round the left side to the front; `degrees` short of a full turn leaves a gap
/// at the back. One row is a ring at `start_latitude`.
pub fn globe_point(
    column: f64,
    row: f64,
    columns: f64,
    rows: f64,
    start_latitude: f64,
    end_latitude: f64,
    degrees: f64,
) -> [f64; 3] {
    let remove = (360.0 - degrees).to_radians();
    let fudge = ((360.0 - degrees) / columns).to_radians();
    let h = PI / 2.0 + 0.003 - remove / 2.0 + column * (-2.0 * PI + remove - fudge) / columns;
    let v_incr = if rows > 1.0 {
        (end_latitude - start_latitude).to_radians() / (rows - 1.0)
    } else {
        0.0
    };
    let v = (start_latitude - 90.0).to_radians() + row * v_incr;
    let sv = v.sin();
    [h.cos() * sv, v.cos(), h.sin() * sv]
}

/// How far along its strand (from the strand's start) the `y`th pixel of `n` sits: alternating
/// goes out every other spot and comes back on the ones between.
pub(crate) fn along_strand(y: u32, n: u32, strand: u32, style: StrandStyle) -> u32 {
    match style {
        StrandStyle::ZigZag if strand % 2 == 1 => n - 1 - y,
        StrandStyle::ZigZag | StrandStyle::NoZigZag => y,
        StrandStyle::AlternatePixel if y < n.div_ceil(2) => 2 * y,
        StrandStyle::AlternatePixel => (n - (y + 1)) * 2 + 1,
    }
}

pub(crate) struct Sphere {
    pub columns: u32,
    pub rows: u32,
    pub radius: f32,
    pub start_latitude: f32,
    pub end_latitude: f32,
    pub degrees: f32,
    pub start: Corner,
    pub strand_style: StrandStyle,
}

/// Strand by strand (one per column), each running up from the south pole (down from the north
/// when `start` is at the top), the columns in order round the left side (the right side when
/// `start` is on the right).
pub(crate) fn positions(s: Sphere) -> Vec<Vec3> {
    let (columns, rows) = (s.columns, s.rows);
    let from_left = matches!(s.start, Corner::BottomLeft | Corner::TopLeft);
    let from_bottom = matches!(s.start, Corner::BottomLeft | Corner::BottomRight);
    let radius = f64::from(s.radius);
    let mut out = Vec::with_capacity(columns.saturating_mul(rows) as usize);
    for x in 0..columns {
        let column = if from_left { x } else { columns - 1 - x };
        for y in 0..rows {
            let along = along_strand(y, rows, x, s.strand_style);
            let row = if from_bottom { along } else { rows - 1 - along };
            let p = globe_point(
                f64::from(column),
                f64::from(row),
                f64::from(columns),
                f64::from(rows),
                f64::from(s.start_latitude),
                f64::from(s.end_latitude),
                f64::from(s.degrees),
            );
            out.push(Vec3::new(
                (p[0] * radius) as f32,
                (p[1] * radius) as f32,
                (p[2] * radius) as f32,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn globe(columns: u32, rows: u32, start: Corner, strand_style: StrandStyle) -> Vec<Vec3> {
        positions(Sphere {
            columns,
            rows,
            radius: 2.0,
            start_latitude: -90.0,
            end_latitude: 90.0,
            degrees: 360.0,
            start,
            strand_style,
        })
    }

    #[test]
    fn strands_run_pole_to_pole_round_the_left_side_first() {
        // Four columns a quarter turn apart, three rows: south pole, equator, north pole.
        let p = globe(4, 3, Corner::BottomLeft, StrandStyle::NoZigZag);
        assert_eq!(p.len(), 12);
        let n = 0.003f32.sin() * 2.0;
        assert_close(p[0], Vec3::new(0.0, -2.0, 0.0));
        // The first column is at the back (nudged a little toward the right)...
        assert_close(p[1], Vec3::new(n, 0.0, -2.0));
        assert_close(p[2], Vec3::new(0.0, 2.0, 0.0));
        // ...the second on the left, the third at the front, the fourth on the right.
        assert_close(p[4], Vec3::new(-2.0, 0.0, -n));
        assert_close(p[7], Vec3::new(-n, 0.0, 2.0));
        assert_close(p[10], Vec3::new(2.0, 0.0, n));
    }

    #[test]
    fn zig_zag_alternating_and_other_starts_change_the_order() {
        let zig = globe(4, 3, Corner::BottomLeft, StrandStyle::ZigZag);
        // The second strand comes back down from the north pole.
        assert_close(zig[3], Vec3::new(0.0, 2.0, 0.0));
        assert_close(zig[5], Vec3::new(0.0, -2.0, 0.0));
        // Alternating: south pole, north pole, then back to the equator.
        let alt = globe(4, 3, Corner::BottomLeft, StrandStyle::AlternatePixel);
        assert_close(alt[1], Vec3::new(0.0, 2.0, 0.0));
        assert_close(alt[2], Vec3::new(0.003f32.sin() * 2.0, 0.0, -2.0));
        // From the top right: down from the north, round the right side first.
        let p = globe(4, 3, Corner::TopRight, StrandStyle::NoZigZag);
        assert_close(p[0], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[1], Vec3::new(2.0, 0.0, 0.003f32.sin() * 2.0));
    }

    #[test]
    fn latitudes_and_degrees_bound_the_globe() {
        // Rows from 0° to 60°, half way round: the gap is at the back, centered.
        let p = positions(Sphere {
            columns: 2,
            rows: 2,
            radius: 1.0,
            start_latitude: 0.0,
            end_latitude: 60.0,
            degrees: 180.0,
            start: Corner::BottomLeft,
            strand_style: StrandStyle::NoZigZag,
        });
        // Column 0 starts a quarter turn round from the back (on the left); the columns step
        // (180° + 90°) / 2 = 135° on, so column 1 is 45° past the front toward the right.
        let h0 = (90.0f64 + 0.003f64.to_degrees() - 90.0).to_radians();
        assert_close(p[0], Vec3::new(-(h0.cos() as f32), 0.0, -(h0.sin() as f32)));
        let r60 = 60f32.to_radians();
        assert!((p[1].y - r60.sin()).abs() < 1e-5);
        let h1 = h0 - 135f64.to_radians();
        assert_close(p[2], Vec3::new(-(h1.cos() as f32), 0.0, -(h1.sin() as f32)));
        // One row: a ring at the first latitude.
        let ring = positions(Sphere {
            columns: 3,
            rows: 1,
            radius: 1.0,
            start_latitude: 0.0,
            end_latitude: 80.0,
            degrees: 360.0,
            start: Corner::BottomLeft,
            strand_style: StrandStyle::ZigZag,
        });
        assert!(
            ring.iter()
                .all(|p| p.y.abs() < 1e-6 && (p.length() - 1.0).abs() < 1e-5)
        );
    }
}
