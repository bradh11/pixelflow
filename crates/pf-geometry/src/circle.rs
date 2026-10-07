use pf_model::Vec3;
use std::f64::consts::{PI, TAU};

pub(crate) struct Circle<'a> {
    pub nodes: u32,
    pub radius: f32,
    pub layers: &'a [u32],
    pub inner_percent: u32,
    pub start_inside: bool,
    pub start_at_bottom: bool,
    pub counter_clockwise: bool,
}

/// Rings as xLights lays them out (`CircleModel::SetCircleCoord`): each ring's pixels evenly
/// round it from the top (or bottom), clockwise (or counter-clockwise). With layers (innermost
/// first), the rings are evenly spaced from `radius` in to `inner_percent` of it, the string going
/// round the outermost first (or the innermost); each ring holds its own count or what's left,
/// and pixels beyond the rings sit in the middle.
pub(crate) fn positions(c: Circle) -> Vec<Vec3> {
    let single = [c.nodes];
    let rings = if c.layers.len() > 1 { c.layers } else { &single[..] };
    let lc = rings.len();
    let (outer, inner) = (
        f64::from(c.radius),
        f64::from(c.radius) * f64::from(c.inner_percent) / 100.0,
    );
    let start = if c.start_at_bottom { -PI } else { 0.0 };
    let mut out = Vec::with_capacity(c.nodes as usize);
    let mut left = c.nodes;
    for k in 0..lc {
        let ring = if c.start_inside { k } else { lc - 1 - k };
        let radius = if lc == 1 {
            outer
        } else {
            inner + (outer - inner) * ring as f64 / (lc - 1) as f64
        };
        let count = left.min(rings[ring]);
        for n in 0..count {
            let mut angle = start + TAU * f64::from(n) / f64::from(count);
            if c.counter_clockwise {
                angle = -angle;
            }
            out.push(Vec3::new(
                (angle.sin() * radius) as f32,
                (angle.cos() * radius) as f32,
                0.0,
            ));
        }
        left -= count;
    }
    out.resize(c.nodes as usize, Vec3::ZERO);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn ring(nodes: u32) -> Circle<'static> {
        Circle {
            nodes,
            radius: 1.0,
            layers: &[],
            inner_percent: 50,
            start_inside: false,
            start_at_bottom: false,
            counter_clockwise: false,
        }
    }

    #[test]
    fn starts_at_top_and_runs_clockwise() {
        let p = positions(ring(4));
        assert_close(p[0], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[1], Vec3::new(1.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, -1.0, 0.0));
        assert_close(p[3], Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn starts_at_the_bottom_and_runs_counter_clockwise() {
        let p = positions(Circle {
            start_at_bottom: true,
            counter_clockwise: true,
            ..ring(4)
        });
        assert_close(p[0], Vec3::new(0.0, -1.0, 0.0));
        assert_close(p[1], Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn rings_go_from_the_outside_in_or_the_inside_out() {
        // 2 pixels inside at half the size, 4 outside.
        let layers = [2, 4];
        let p = positions(Circle {
            layers: &layers,
            ..ring(6)
        });
        assert_close(p[0], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[4], Vec3::new(0.0, 0.5, 0.0));
        assert_close(p[5], Vec3::new(0.0, -0.5, 0.0));
        let q = positions(Circle {
            layers: &layers,
            start_inside: true,
            ..ring(7)
        });
        assert_close(q[1], Vec3::new(0.0, -0.5, 0.0));
        assert_close(q[3], Vec3::new(1.0, 0.0, 0.0));
        // The pixel beyond the rings sits in the middle.
        assert_close(q[6], Vec3::ZERO);
        // Fewer pixels than the rings hold: the last ring spreads what it gets.
        let r = positions(Circle {
            layers: &layers,
            ..ring(5)
        });
        assert_close(r[4], Vec3::new(0.0, 0.5, 0.0));
    }
}
