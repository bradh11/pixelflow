use crate::spread;
use pf_model::Vec3;

pub(crate) struct Arch<'a> {
    pub nodes: u32,
    pub width: f32,
    pub height: f32,
    pub arches: u32,
    pub arc: f32,
    pub gap: f32,
    pub skew_deg: f32,
    pub start_right: bool,
    pub layers: &'a [u32],
    pub hollow: u32,
    pub zig_zag: bool,
    pub start_inside: bool,
}

/// Arches as xLights lays them out (`ArchesModel::SetArchCoord` / `SetLayerdArchCoord`).
///
/// Each arch is the part of an ellipse `arc` degrees round about its top, its feet `width` apart
/// on y = 0 and its top `height` above them; pixels are spaced evenly by angle from the left foot
/// to the right one. Plain arches stand in a row, `gap` between one's right foot and the next
/// one's left foot, centered on the origin. A layered arch nests its layers about the ellipse's
/// center, the innermost `hollow` percent of the outermost's size, each layer's pixels on the
/// outermost layer's spots (rounded, as xLights does). The lean shifts each pixel left by its
/// height times sin(lean) and lowers it to its height times cos(lean).
pub(crate) fn positions(a: Arch) -> Vec<Vec3> {
    let theta = if a.arc.is_finite() {
        f64::from(a.arc).clamp(1.0, 180.0).to_radians()
    } else {
        std::f64::consts::PI
    };
    let half = theta / 2.0;
    // The ellipse: semi-axes `ea` across and `eb` up, its center `eb · cos(half)` below the feet.
    let ea = f64::from(a.width) / 2.0 / half.sin();
    let eb = f64::from(a.height) / (1.0 - half.cos());
    let drop = eb * half.cos();
    let (sin_skew, cos_skew) = f64::from(a.skew_deg).to_radians().sin_cos();
    let place = |x: f64, adj: f64, angle: f64| {
        let px = x + ea * adj * angle.sin();
        let py = eb * adj * angle.cos() - drop;
        Vec3::new((px - py * sin_skew) as f32, (py * cos_skew) as f32, 0.0)
    };
    if a.layers.is_empty() {
        let (n, w, gap) = (a.arches, f64::from(a.width), f64::from(a.gap));
        let total = f64::from(n) * w + f64::from(n.saturating_sub(1)) * gap;
        let mut out = Vec::with_capacity(n as usize * a.nodes as usize);
        for k in 0..n {
            let x = -total / 2.0 + w / 2.0 + f64::from(k) * (w + gap);
            for i in 0..a.nodes {
                let angle = -half + theta * f64::from(spread(i, a.nodes));
                out.push(place(x, 1.0, angle));
            }
        }
        // Wired from the right: the same spots, last first.
        if a.start_right {
            out.reverse();
        }
        out
    } else {
        layered(&a, theta, half)
            .into_iter()
            .map(|(angle, adj)| place(0.0, adj, angle))
            .collect()
    }
}

/// Each pixel of a layered arch: its angle and its layer's size (1 for the outermost).
fn layered(a: &Arch, theta: f64, half: f64) -> Vec<(f64, f64)> {
    let lc = a.layers.len();
    let max_len = i64::from(a.layers.iter().copied().max().unwrap_or(1));
    let nodes = a.nodes as usize;
    // xLights' buffer spot (along the outermost layer, and which layer) of each pixel.
    let mut spots = vec![(0i64, 0usize); nodes];
    let mut idx = 0usize;
    let mut forward = !a.start_right;
    for layer in 0..lc {
        if idx >= nodes {
            break;
        }
        let yy = if a.start_inside { layer } else { lc - layer - 1 };
        let it = a.layers[yy];
        if it == 1 {
            spots[idx] = (max_len / 2, yy);
            idx += 1;
        } else {
            let step = (max_len - 1) as f32 / (it as f32 - 1.0);
            for x in 0..it {
                // Past the last pixel, the rest of the layer changes nothing.
                if idx >= nodes {
                    break;
                }
                let mut xx = (x as f32 * step).round() as i64;
                if !forward {
                    xx = max_len - 1 - xx;
                }
                spots[idx] = (xx, yy);
                idx += 1;
            }
        }
        if a.zig_zag {
            forward = !forward;
        }
    }
    let midpt = (max_len - 1) as f64 / 2.0;
    let layer_gap = if lc > 1 {
        (1.0 - f64::from(a.hollow) / 100.0) / (lc - 1) as f64
    } else {
        0.0
    };
    spots
        .into_iter()
        .map(|(x, y)| {
            let angle = if midpt == 0.0 {
                0.0
            } else {
                -half + theta * x as f64 / midpt / 2.0
            };
            (angle, 1.0 - layer_gap * (lc - 1 - y) as f64)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn arch(nodes: u32, width: f32, height: f32) -> Arch<'static> {
        Arch {
            nodes,
            width,
            height,
            arches: 1,
            arc: 180.0,
            gap: 0.0,
            skew_deg: 0.0,
            start_right: false,
            layers: &[],
            hollow: 70,
            zig_zag: false,
            start_inside: false,
        }
    }

    #[test]
    fn runs_left_base_to_top_to_right_base() {
        let p = positions(arch(3, 4.0, 2.0));
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[1], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[2], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn a_part_arch_keeps_its_feet_and_top_and_spaces_pixels_by_angle() {
        // 90° of an ellipse: feet 4 apart, top 1 above them.
        let p = positions(Arch {
            arc: 90.0,
            ..arch(3, 4.0, 1.0)
        });
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[1], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[2], Vec3::new(2.0, 0.0, 0.0));
        // A quarter of the way round: 22.5° from the top.
        let q = positions(Arch {
            arc: 90.0,
            ..arch(5, 4.0, 1.0)
        });
        let (ea, eb) = (
            2.0 / 45f32.to_radians().sin(),
            1.0 / (1.0 - 45f32.to_radians().cos()),
        );
        let t = (-22.5f32).to_radians();
        assert_close(
            q[1],
            Vec3::new(ea * t.sin(), eb * (t.cos() - 45f32.to_radians().cos()), 0.0),
        );
    }

    #[test]
    fn arches_stand_in_a_row_with_gaps_and_can_start_on_the_right() {
        let p = positions(Arch {
            arches: 2,
            gap: 1.0,
            ..arch(3, 2.0, 1.0)
        });
        // Two arches 2 wide, 1 apart: 5 across, centered.
        assert_close(p[0], Vec3::new(-2.5, 0.0, 0.0));
        assert_close(p[2], Vec3::new(-0.5, 0.0, 0.0));
        assert_close(p[3], Vec3::new(0.5, 0.0, 0.0));
        assert_close(p[4], Vec3::new(1.5, 1.0, 0.0));
        let r = positions(Arch {
            arches: 2,
            gap: 1.0,
            start_right: true,
            ..arch(3, 2.0, 1.0)
        });
        assert_close(r[0], Vec3::new(2.5, 0.0, 0.0));
        assert_close(r[5], Vec3::new(-2.5, 0.0, 0.0));
    }

    #[test]
    fn a_leaning_arch_shifts_its_top_left_and_lowers_it() {
        let p = positions(Arch {
            skew_deg: 30.0,
            ..arch(3, 2.0, 2.0)
        });
        assert_close(p[0], Vec3::new(-1.0, 0.0, 0.0));
        assert_close(p[1], Vec3::new(-1.0, 3f32.sqrt(), 0.0));
    }

    #[test]
    fn layers_nest_inside_the_outermost_from_the_outside_or_the_inside() {
        // Layers of 3 (inside) and 5 (outside), the inside one half the size.
        let layers = [3, 5];
        let a = |start_inside, zig_zag| {
            positions(Arch {
                layers: &layers,
                hollow: 50,
                start_inside,
                zig_zag,
                ..arch(8, 4.0, 2.0)
            })
        };
        let p = a(false, false);
        assert_eq!(p.len(), 8);
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[4], Vec3::new(2.0, 0.0, 0.0));
        // The inner layer's pixels sit on the outer one's 1st, 3rd and 5th spots.
        assert_close(p[5], Vec3::new(-1.0, 0.0, 0.0));
        assert_close(p[6], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[7], Vec3::new(1.0, 0.0, 0.0));
        // From the right, every layer runs right to left.
        let r = positions(Arch {
            layers: &layers,
            hollow: 50,
            start_right: true,
            ..arch(8, 4.0, 2.0)
        });
        assert_close(r[0], Vec3::new(2.0, 0.0, 0.0));
        assert_close(r[5], Vec3::new(1.0, 0.0, 0.0));
        let q = a(true, true);
        assert_close(q[0], Vec3::new(-1.0, 0.0, 0.0));
        // Zig-zag: the outer layer runs back from the right.
        assert_close(q[3], Vec3::new(2.0, 0.0, 0.0));
        // Pixels beyond the layers sit where xLights puts them: the inner layer's left foot.
        let extra = positions(Arch {
            layers: &layers,
            hollow: 50,
            ..arch(10, 4.0, 2.0)
        });
        assert_close(extra[9], Vec3::new(-1.0, 0.0, 0.0));
    }
}
