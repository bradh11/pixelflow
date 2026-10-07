use pf_model::{StarStart, Vec3};
use std::f64::consts::{PI, TAU};

pub(crate) struct Star<'a> {
    pub points: u32,
    pub nodes: u32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start: StarStart,
    pub counter_clockwise: bool,
    pub layers: &'a [u32],
    pub inner_percent: u32,
    pub start_inside: bool,
}

/// Star outlines as xLights lays them out (`StarModel::InitModel`): pixels spaced evenly along
/// the closed outline from the `start` corner, clockwise (or counter-clockwise). Starting at the
/// bottom starts on the inner corner there, turning a star with an even number of points so it
/// has one; the legs are the bottom tips either side. With layers (innermost first), the
/// outlines are nested from full size in to `inner_percent` of it, each its own count of pixels
/// spread round it (the last one stopping when the pixels run out), the string going round the
/// outermost first (or the innermost); pixels beyond them sit in the middle.
pub(crate) fn positions(s: Star) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(s.nodes as usize);
    // `points` beyond half of u32::MAX would overflow the vertex count (show files are limited
    // far below this by `pf-model`).
    if s.points == 0 || s.points > u32::MAX / 2 {
        return vec![Vec3::ZERO; s.nodes as usize];
    }
    let single = [s.nodes];
    let layers = if s.layers.len() > 1 { s.layers } else { &single[..] };
    let lc = layers.len();
    let points = f64::from(s.points);
    let gap = TAU / points;
    let odd = s.points % 2 == 1;
    // xLights' angles run clockwise from the top.
    let (start_angle, start_outer) = match s.start {
        StarStart::Top => (0.0, true),
        StarStart::Bottom => (PI, false),
        StarStart::LeftLeg => (PI + if odd { gap / 2.0 } else { 0.0 }, true),
        StarStart::RightLeg => (PI - if odd { gap / 2.0 } else { 0.0 }, true),
    };
    let dir = if s.counter_clockwise { -1.0 } else { 1.0 };
    let (outer, inner) = (f64::from(s.outer_radius), f64::from(s.inner_radius));
    let segments = 2 * s.points as usize;
    let mut left = s.nodes;
    for k in 0..lc {
        if left == 0 {
            break;
        }
        let layer = if s.start_inside { k } else { lc - 1 - k };
        let size = if lc == 1 {
            1.0
        } else {
            let p = f64::from(s.inner_percent) / 100.0;
            p + (1.0 - p) * layer as f64 / (lc - 1) as f64
        };
        let corner = |v: usize| {
            let r = if v.is_multiple_of(2) == start_outer {
                outer
            } else {
                inner
            } * size;
            let a = start_angle + dir * v as f64 * gap / 2.0;
            (r * a.sin(), r * a.cos())
        };
        let (a, b) = (corner(0), corner(1));
        let edge = (b.0 - a.0).hypot(b.1 - a.1);
        let n = layers[layer];
        for i in 0..n.min(left) {
            let d = edge * segments as f64 * f64::from(i) / f64::from(n);
            let (seg, t) = if edge > 0.0 {
                let seg = ((d / edge) as usize).min(segments - 1);
                (seg, (d - seg as f64 * edge) / edge)
            } else {
                (0, 0.0)
            };
            let (p, q) = (corner(seg), corner(seg + 1));
            out.push(Vec3::new(
                (p.0 + (q.0 - p.0) * t) as f32,
                (p.1 + (q.1 - p.1) * t) as f32,
                0.0,
            ));
        }
        left -= n.min(left);
    }
    out.resize(s.nodes as usize, Vec3::ZERO);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use std::f32::consts::PI;

    fn star(points: u32, nodes: u32, outer_radius: f32, inner_radius: f32) -> Star<'static> {
        Star {
            points,
            nodes,
            outer_radius,
            inner_radius,
            start: StarStart::Top,
            counter_clockwise: false,
            layers: &[],
            inner_percent: 50,
            start_inside: false,
        }
    }

    #[test]
    fn absurd_point_counts_do_not_overflow() {
        assert_eq!(positions(star(u32::MAX, 3, 2.0, 1.0)).len(), 3);
    }

    #[test]
    fn first_pixel_is_top_tip_and_all_lie_within_outer_radius() {
        let p = positions(star(5, 100, 2.0, 1.0));
        assert_eq!(p.len(), 100);
        assert_close(p[0], Vec3::new(0.0, 2.0, 0.0));
        assert!(p.iter().all(|v| v.length() <= 2.0 + 1e-4));
    }

    #[test]
    fn one_pixel_per_vertex_lands_on_vertices() {
        let p = positions(star(4, 8, 2.0, 1.0));
        assert_close(
            p[1],
            Vec3::new(1.0 * (PI / 4.0).cos(), 1.0 * (PI / 4.0).sin(), 0.0),
        );
        assert_close(p[2], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn starts_at_the_bottom_or_a_leg_either_way_round() {
        // Five points, one pixel a corner: the bottom inner corner, then the left leg's tip.
        let p = positions(Star {
            start: StarStart::Bottom,
            ..star(5, 10, 2.0, 1.0)
        });
        assert_close(p[0], Vec3::new(0.0, -1.0, 0.0));
        let leg = 216f32.to_radians();
        assert_close(p[1], Vec3::new(2.0 * leg.sin(), 2.0 * leg.cos(), 0.0));
        let q = positions(Star {
            start: StarStart::RightLeg,
            counter_clockwise: true,
            ..star(5, 10, 2.0, 1.0)
        });
        let right = 144f32.to_radians();
        assert_close(q[0], Vec3::new(2.0 * right.sin(), 2.0 * right.cos(), 0.0));
        // Counter-clockwise from the right leg: up its right side to the inner corner there.
        let side = 108f32.to_radians();
        assert_close(q[1], Vec3::new(side.sin(), side.cos(), 0.0));
        // Four points from the bottom: turned so an inner corner is at the bottom (and top).
        let r = positions(Star {
            start: StarStart::Bottom,
            ..star(4, 8, 2.0, 1.0)
        });
        assert_close(r[0], Vec3::new(0.0, -1.0, 0.0));
        assert_close(r[4], Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn layers_nest_from_the_outside_in_or_the_inside_out() {
        let layers = [10, 20];
        let p = positions(Star {
            layers: &layers,
            ..star(5, 31, 2.0, 1.0)
        });
        assert_close(p[0], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[20], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[30], Vec3::ZERO);
        let q = positions(Star {
            layers: &layers,
            start_inside: true,
            inner_percent: 25,
            ..star(5, 30, 2.0, 1.0)
        });
        assert_close(q[0], Vec3::new(0.0, 0.5, 0.0));
        assert_close(q[10], Vec3::new(0.0, 2.0, 0.0));
    }
}
