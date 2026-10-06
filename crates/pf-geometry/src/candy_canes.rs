//! Candy canes as xLights lays them out (`CandyCaneModel::SetCaneCoord`, one light per node),
//! scaled to the prop's width.

use pf_model::Vec3;
use std::f64::consts::PI;

/// Space between neighbouring canes, in pixel spacings (xLights' `caneGap`).
const CANE_GAP: f64 = 2.0;

/// The settings of a row of candy canes (see `pf_model::Generator::CandyCanes`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Canes {
    pub canes: u32,
    pub nodes_per_cane: u32,
    pub width: f32,
    pub height: f32,
    pub cane_height: f32,
    pub reverse: bool,
    pub sticks: bool,
    pub alternate_nodes: bool,
    pub skew_deg: f32,
}

/// Where the `x`th pixel on a cane sits along it: with alternate nodes the pixels go up every
/// other spot and come back down the ones between (xLights' `bufY`).
fn spot(x: u32, n: u32, alternate: bool) -> u32 {
    if !alternate {
        x
    } else if x < n.div_ceil(2) {
        2 * x
    } else {
        (n - (x + 1)) * 2 + 1
    }
}

pub(crate) fn positions(c: Canes) -> Vec<Vec3> {
    let n = c.nodes_per_cane;
    if c.canes == 0 || n == 0 {
        return Vec::new();
    }
    // xLights' local units are pixel spacings: each cane is a third as wide as it has pixels,
    // and two-thirds of them (rounded down) run up the stick.
    let lights = f64::from(n);
    let cane_width = lights * 3.0 / 9.0;
    let upright = (u64::from(n) * 6 / 9) as u32;
    let arc = f64::from(n - upright);
    let total = f64::from(c.canes) * cane_width + f64::from(c.canes - 1) * CANE_GAP;
    let k = f64::from(c.width) / total;
    let (mh, ch) = (f64::from(c.height), f64::from(c.cane_height));
    let radius = cane_width / 2.0 * mh;
    let (sin, cos) = f64::from(c.skew_deg).to_radians().sin_cos();

    let mut out = Vec::with_capacity(c.canes as usize * n as usize);
    for i in 0..c.canes {
        let left = f64::from(i) * (cane_width + CANE_GAP);
        for x in 0..n {
            let p = spot(x, n, c.alternate_nodes);
            let (foot, px, py) = if c.sticks {
                let foot = left + cane_width / 2.0;
                (foot, foot, ch * f64::from(p) * mh)
            } else {
                let foot = if c.reverse { left + cane_width } else { left };
                if p < upright {
                    (foot, foot, ch * f64::from(p) * mh)
                } else {
                    // Round the hook from the top of the stick over to the far side.
                    let a = PI - PI * f64::from(p - upright + 1) / arc;
                    let along = radius + a.cos() * radius;
                    let px = if c.reverse { foot - along } else { foot + along };
                    let top = f64::from(upright) - 1.0;
                    (foot, px, ch * (top * mh + a.sin() * radius))
                }
            };
            // Each cane leans about its foot.
            let dx = px - foot;
            let (rx, ry) = (dx * cos - py * sin + foot, dx * sin + py * cos);
            out.push(Vec3::new(((rx - total / 2.0) * k) as f32, (ry * k) as f32, 0.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    /// One cane of nine pixels, three wide: six up the stick, three round the hook.
    fn cane() -> Canes {
        Canes {
            canes: 1,
            nodes_per_cane: 9,
            width: 3.0,
            height: 1.0,
            cane_height: 1.0,
            reverse: false,
            sticks: false,
            alternate_nodes: false,
            skew_deg: 0.0,
        }
    }

    const HOOK_TOP: f32 = 5.0 + 1.299_038; // 5 + 1.5 * sin(60°)

    #[test]
    fn runs_up_the_stick_then_round_the_hook() {
        let p = positions(cane());
        assert_eq!(p.len(), 9);
        for (i, q) in p.iter().take(6).enumerate() {
            assert_close(*q, Vec3::new(-1.5, i as f32, 0.0));
        }
        assert_close(p[6], Vec3::new(-0.75, HOOK_TOP, 0.0));
        assert_close(p[7], Vec3::new(0.75, HOOK_TOP, 0.0));
        assert_close(p[8], Vec3::new(1.5, 5.0, 0.0));
    }

    #[test]
    fn reversed_hooks_point_left() {
        let p = positions(Canes {
            reverse: true,
            ..cane()
        });
        assert_close(p[0], Vec3::new(1.5, 0.0, 0.0));
        assert_close(p[6], Vec3::new(0.75, HOOK_TOP, 0.0));
        assert_close(p[8], Vec3::new(-1.5, 5.0, 0.0));
    }

    #[test]
    fn canes_stand_two_pixel_spaces_apart_and_fill_the_width() {
        // Two canes three wide with a gap of two: eight across, scaled to sixteen.
        let p = positions(Canes {
            canes: 2,
            width: 16.0,
            ..cane()
        });
        assert_eq!(p.len(), 18);
        assert_close(p[0], Vec3::new(-8.0, 0.0, 0.0));
        assert_close(p[8], Vec3::new(-2.0, 10.0, 0.0));
        assert_close(p[9], Vec3::new(2.0, 0.0, 0.0));
        assert_close(p[17], Vec3::new(8.0, 10.0, 0.0));
    }

    #[test]
    fn sticks_stand_in_the_middle_of_their_cane() {
        let p = positions(Canes {
            sticks: true,
            ..cane()
        });
        for (i, q) in p.iter().enumerate() {
            assert_close(*q, Vec3::new(0.0, i as f32, 0.0));
        }
    }

    #[test]
    fn alternate_nodes_go_up_every_other_spot_and_back_down() {
        let p = positions(Canes {
            alternate_nodes: true,
            ..cane()
        });
        assert_close(p[1], Vec3::new(-1.5, 2.0, 0.0));
        assert_close(p[4], Vec3::new(1.5, 5.0, 0.0));
        assert_close(p[5], Vec3::new(0.75, HOOK_TOP, 0.0));
        assert_close(p[8], Vec3::new(-1.5, 1.0, 0.0));
    }

    #[test]
    fn height_scales_the_canes_and_hooks_and_cane_height_stretches_them() {
        let tall = positions(Canes {
            height: 2.0,
            ..cane()
        });
        assert_close(tall[5], Vec3::new(-1.5, 10.0, 0.0));
        assert_close(tall[8], Vec3::new(4.5, 10.0, 0.0));
        let stretched = positions(Canes {
            cane_height: 2.0,
            ..cane()
        });
        assert_close(stretched[5], Vec3::new(-1.5, 10.0, 0.0));
        assert_close(stretched[7], Vec3::new(0.75, 2.0 * HOOK_TOP, 0.0));
    }

    #[test]
    fn skew_leans_each_cane_about_its_foot() {
        let p = positions(Canes {
            skew_deg: 90.0,
            ..cane()
        });
        assert_close(p[0], Vec3::new(-1.5, 0.0, 0.0));
        assert_close(p[2], Vec3::new(-3.5, 0.0, 0.0));
        assert_close(p[8], Vec3::new(-6.5, 3.0, 0.0));
    }

    #[test]
    fn no_canes_or_no_pixels_is_empty() {
        assert!(positions(Canes { canes: 0, ..cane() }).is_empty());
        assert!(
            positions(Canes {
                nodes_per_cane: 0,
                ..cane()
            })
            .is_empty()
        );
    }
}
