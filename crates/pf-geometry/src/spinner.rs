//! Spinners as xLights lays them out (`SpinnerModel::SetSpinnerCoord`, one light per node),
//! scaled so the outermost pixel is the prop's radius from the middle.

use pf_model::Vec3;

/// The settings of a spinner (see `pf_model::Generator::Spinner`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spinner {
    pub arms: u32,
    pub nodes_per_arm: u32,
    pub hollow: u32,
    pub start_angle: f32,
    pub arc: f32,
    pub zig_zag: bool,
    pub alternate: bool,
    pub from_center: bool,
    pub clockwise: bool,
    pub radius: f32,
}

/// The arms' angles step in single precision as xLights' do, so a spinner of many arms keeps
/// xLights' tiny drift and lands exactly where xLights draws it.
pub(crate) fn positions(s: Spinner) -> Vec<Vec3> {
    let (arms, npa) = (s.arms, s.nodes_per_arm);
    if arms == 0 || npa == 0 {
        return Vec::new();
    }
    let pi = std::f32::consts::PI;
    let mut angle = (pi * 2.0 * (270.0 + s.start_angle)) / 360.0;
    let incr = if s.arc < 360.0 && arms > 1 {
        (pi * 2.0 * s.arc) / ((arms as f32 - 1.0) * 360.0)
    } else {
        (pi * 2.0 * s.arc) / (arms as f32 * 360.0)
    };
    // In pixel steps: the first pixel half a step out from the hollow middle.
    let hollow = f64::from(s.hollow) * 2.0 * f64::from(npa) / 100.0;
    let unit = f64::from(s.radius) / (f64::from(npa) - 0.5 + hollow);
    let mut out = Vec::with_capacity(arms as usize * npa as usize);
    for a in 0..arms {
        let (sin, cos) = f64::from(angle).sin_cos();
        let outward = s.from_center != (s.zig_zag && a % 2 == 1);
        for n in 0..npa {
            let step = if s.alternate {
                if n < npa.div_ceil(2) {
                    2 * n
                } else {
                    (npa - (n + 1)) * 2 + 1
                }
            } else if outward {
                n
            } else {
                npa - n - 1
            };
            let r = (0.5 + f64::from(step) + hollow) * unit;
            out.push(Vec3::new((r * cos) as f32, (r * sin) as f32, 0.0));
        }
        if s.clockwise {
            angle -= incr;
        } else {
            angle += incr;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    /// Two arms of three pixels, no hollow middle, sized so a pixel step is one unit.
    fn spinner() -> Spinner {
        Spinner {
            arms: 2,
            nodes_per_arm: 3,
            hollow: 0,
            start_angle: 0.0,
            arc: 360.0,
            zig_zag: false,
            alternate: false,
            from_center: false,
            clockwise: false,
            radius: 2.5,
        }
    }

    fn assert_all(p: &[Vec3], want: &[(f32, f32)]) {
        assert_eq!(p.len(), want.len());
        for (a, &(x, y)) in p.iter().zip(want) {
            assert_close(*a, Vec3::new(x, y, 0.0));
        }
    }

    #[test]
    fn the_first_arm_points_down_and_runs_in_from_its_tip() {
        assert_all(
            &positions(spinner()),
            &[
                (0.0, -2.5),
                (0.0, -1.5),
                (0.0, -0.5),
                (0.0, 2.5),
                (0.0, 1.5),
                (0.0, 0.5),
            ],
        );
    }

    #[test]
    fn pixels_can_start_in_the_middle_and_every_other_arm_can_zig_zag() {
        let p = positions(Spinner {
            from_center: true,
            ..spinner()
        });
        assert_close(p[0], Vec3::new(0.0, -0.5, 0.0));
        assert_close(p[3], Vec3::new(0.0, 0.5, 0.0));
        let p = positions(Spinner {
            zig_zag: true,
            ..spinner()
        });
        assert_close(p[0], Vec3::new(0.0, -2.5, 0.0));
        assert_close(p[3], Vec3::new(0.0, 0.5, 0.0));
    }

    #[test]
    fn alternate_goes_out_every_other_spot_and_comes_back_in() {
        let p = positions(Spinner {
            arms: 1,
            nodes_per_arm: 5,
            alternate: true,
            radius: 4.5,
            ..spinner()
        });
        assert_all(
            &p,
            &[(0.0, -0.5), (0.0, -2.5), (0.0, -4.5), (0.0, -3.5), (0.0, -1.5)],
        );
    }

    #[test]
    fn the_hollow_middle_pushes_the_arms_out() {
        // 50% of twice three pixels: three steps of hollow, so the outermost pixel is 5.5 out.
        let p = positions(Spinner {
            hollow: 50,
            from_center: true,
            radius: 5.5,
            ..spinner()
        });
        assert_close(p[0], Vec3::new(0.0, -3.5, 0.0));
        assert_close(p[2], Vec3::new(0.0, -5.5, 0.0));
        // Sized down: the same layout, smaller.
        let p = positions(Spinner {
            hollow: 50,
            from_center: true,
            radius: 1.1,
            ..spinner()
        });
        assert_close(p[0], Vec3::new(0.0, -0.7, 0.0));
    }

    #[test]
    fn arms_spread_over_the_arc_from_the_start_angle_either_way_round() {
        let fan = Spinner {
            arms: 3,
            nodes_per_arm: 1,
            arc: 180.0,
            radius: 1.0,
            ..spinner()
        };
        // Less than a full turn: the last arm ends the arc.
        assert_all(&positions(fan), &[(0.0, -1.0), (1.0, 0.0), (0.0, 1.0)]);
        assert_all(
            &positions(Spinner {
                clockwise: true,
                ..fan
            }),
            &[(0.0, -1.0), (-1.0, 0.0), (0.0, 1.0)],
        );
        assert_all(
            &positions(Spinner {
                start_angle: 90.0,
                ..fan
            }),
            &[(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)],
        );
        // A full turn: the last arm stops a step short of the first.
        let p = positions(Spinner { arc: 360.0, ..fan });
        assert_close(p[1], Vec3::new(0.866_025, 0.5, 0.0));
    }

    #[test]
    fn no_arms_or_no_pixels_is_empty() {
        assert!(positions(Spinner { arms: 0, ..spinner() }).is_empty());
        assert!(
            positions(Spinner {
                nodes_per_arm: 0,
                ..spinner()
            })
            .is_empty()
        );
    }
}
