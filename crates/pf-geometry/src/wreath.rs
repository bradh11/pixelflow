//! Wreaths as xLights lays them out (`WreathModel::InitWreath`): a ring of pixels rounded to a
//! square grid, scaled so the grid's radius is the prop's.

use pf_model::Vec3;
use std::f64::consts::PI;

/// xLights' grid is `nodes / 2` (rounded down) steps across the radius. Each pixel is rounded to
/// it in xLights' double arithmetic, stepping round the ring as it does, so pixels that fall
/// exactly halfway between two grid points round the same way. (xLights also draws a wreath of
/// an odd number of pixels a step down and left of its middle; PixelFlow keeps it centered.)
pub(crate) fn positions(
    nodes: u32,
    radius: f32,
    start_at_bottom: bool,
    counter_clockwise: bool,
) -> Vec<Vec3> {
    if nodes == 0 {
        return Vec::new();
    }
    let n = u64::from(nodes);
    let offset = (n / 2) as f64;
    let unit = f64::from(radius) / offset.max(1.0);
    let mut pct = if start_at_bottom { 0.5 } else { 0.0 };
    let step = 1.0 / n as f64;
    let incr = if counter_clockwise { -step } else { step };
    let mut out = Vec::with_capacity(nodes as usize);
    for _ in 0..nodes {
        let a = pct * 2.0 * PI;
        let x = (offset * a.sin() + offset + 0.5).trunc() - offset;
        let y = (offset * a.cos() + offset + 0.5).trunc() - offset;
        out.push(Vec3::new((x * unit) as f32, (y * unit) as f32, 0.0));
        pct += incr;
        if pct >= 1.0 {
            pct -= 1.0;
        }
        if pct < 0.0 {
            pct += 1.0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn assert_all(p: &[Vec3], want: &[(f32, f32)]) {
        assert_eq!(p.len(), want.len());
        for (a, &(x, y)) in p.iter().zip(want) {
            assert_close(*a, Vec3::new(x, y, 0.0));
        }
    }

    #[test]
    fn starts_at_the_top_and_runs_clockwise() {
        assert_all(
            &positions(4, 1.0, false, false),
            &[(0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)],
        );
    }

    #[test]
    fn can_start_at_the_bottom_and_run_counter_clockwise() {
        assert_all(
            &positions(4, 2.0, true, false),
            &[(0.0, -2.0), (-2.0, 0.0), (0.0, 2.0), (2.0, 0.0)],
        );
        assert_all(
            &positions(4, 2.0, true, true),
            &[(0.0, -2.0), (2.0, 0.0), (0.0, 2.0), (-2.0, 0.0)],
        );
    }

    #[test]
    fn pixels_round_to_the_nearest_grid_point() {
        // Eight pixels: the grid is four steps across the radius, so 45° (2.83, 2.83) rounds
        // to (3, 3).
        let p = positions(8, 4.0, false, false);
        assert_close(p[1], Vec3::new(3.0, 3.0, 0.0));
        // Half the size: the same grid, half as far apart.
        let p = positions(8, 2.0, false, false);
        assert_close(p[1], Vec3::new(1.5, 1.5, 0.0));
        // Five pixels: two grid steps across the radius; 72° (1.90, 0.62) rounds to (2, 1).
        let p = positions(5, 2.0, false, false);
        assert_close(p[0], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[1], Vec3::new(2.0, 1.0, 0.0));
    }

    #[test]
    fn one_pixel_sits_in_the_middle_and_none_is_empty() {
        assert_all(&positions(1, 3.0, false, false), &[(0.0, 0.0)]);
        assert!(positions(0, 1.0, false, false).is_empty());
    }
}
