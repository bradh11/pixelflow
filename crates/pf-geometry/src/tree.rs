use crate::spread;
use pf_model::{TreeStyle, Vec3};

pub(crate) struct Tree {
    pub strings: u32,
    pub nodes_per_string: u32,
    pub height: f32,
    pub base_radius: f32,
    pub top_radius: f32,
    pub serpentine: bool,
    pub style: TreeStyle,
    pub degrees: f32,
    pub start_angle: f32,
}

/// Strings running from the base (y = 0) to the top (y = height), string by string; with
/// `serpentine`, odd strings run top to bottom. Round: a cone, as xLights wraps a tree (string
/// `s` at `start_angle + s × step` round from the front, the step spreading the strings over
/// `degrees`, reaching both edges when that's under 350°). Flat and ribbon: fanned out in the
/// front view (xLights' `SetTreeCoord` for those styles).
pub(crate) fn positions(t: Tree) -> Vec<Vec3> {
    let n = t.strings;
    let mut out = Vec::with_capacity(n as usize * t.nodes_per_string as usize);
    let step = if t.degrees < 350.0 && n > 1 {
        t.degrees / (n - 1) as f32
    } else {
        t.degrees / n.max(1) as f32
    };
    for s in 0..n {
        let angle = (t.start_angle + s as f32 * step).to_radians();
        // Flat and ribbon: the string's place across the base and the top, -1 to 1 of the width.
        let across = (s as f32 + 0.5 - n as f32 / 2.0) / (n as f32 / 2.0);
        let (xb, xt) = (across * t.base_radius, across * t.top_radius);
        let slant = (t.height * t.height + (xt - xb) * (xt - xb)).sqrt();
        for j in 0..t.nodes_per_string {
            let mut f = spread(j, t.nodes_per_string);
            if t.serpentine && s % 2 == 1 {
                f = 1.0 - f;
            }
            out.push(match t.style {
                TreeStyle::Round => {
                    let radius = t.base_radius + (t.top_radius - t.base_radius) * f;
                    Vec3::new(radius * angle.sin(), f * t.height, radius * angle.cos())
                }
                TreeStyle::Flat => Vec3::new(xb + (xt - xb) * f, f * t.height, 0.0),
                TreeStyle::Ribbon => {
                    // Every string is as long as the middle one would be: slanted ones end lower.
                    let y = if slant > 0.0 {
                        f * t.height * t.height / slant
                    } else {
                        0.0
                    };
                    Vec3::new(xb + (xt - xb) * f, y, 0.0)
                }
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn tree(
        strings: u32,
        nodes: u32,
        style: TreeStyle,
        degrees: f32,
        start_angle: f32,
        serpentine: bool,
    ) -> Vec<Vec3> {
        positions(Tree {
            strings,
            nodes_per_string: nodes,
            height: 6.0,
            base_radius: 2.0,
            top_radius: 0.0,
            serpentine,
            style,
            degrees,
            start_angle,
        })
    }

    #[test]
    fn strings_run_base_to_top_and_taper() {
        let p = tree(4, 3, TreeStyle::Round, 360.0, 0.0, false);
        assert_eq!(p.len(), 12);
        assert_close(p[0], Vec3::new(0.0, 0.0, 2.0));
        assert_close(p[2], Vec3::new(0.0, 6.0, 0.0));
        assert_close(p[3], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn serpentine_reverses_odd_strings() {
        let p = positions(Tree {
            top_radius: 2.0,
            ..Tree {
                strings: 2,
                nodes_per_string: 3,
                height: 6.0,
                base_radius: 2.0,
                top_radius: 0.0,
                serpentine: true,
                style: TreeStyle::Round,
                degrees: 360.0,
                start_angle: 0.0,
            }
        });
        assert_close(p[3], Vec3::new(0.0, 6.0, -2.0));
        assert_close(p[5], Vec3::new(0.0, 0.0, -2.0));
    }

    #[test]
    fn part_trees_reach_both_edges_from_the_start_angle() {
        // Half a tree, 3 strings: at -90°, 0° and 90° (left, front, right).
        let p = tree(3, 2, TreeStyle::Round, 180.0, -90.0, false);
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, 0.0, 2.0));
        assert_close(p[4], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn flat_and_ribbon_trees_fan_out_in_the_front_view() {
        // Two strings: a quarter and three quarters of the way across, meeting at the top.
        let flat = tree(2, 3, TreeStyle::Flat, 360.0, 0.0, false);
        assert_close(flat[0], Vec3::new(-1.0, 0.0, 0.0));
        assert_close(flat[1], Vec3::new(-0.5, 3.0, 0.0));
        assert_close(flat[2], Vec3::new(0.0, 6.0, 0.0));
        assert_close(flat[3], Vec3::new(1.0, 0.0, 0.0));
        // A ribbon's slanted string keeps its length: it ends below the full height.
        let ribbon = tree(2, 3, TreeStyle::Ribbon, 360.0, 0.0, false);
        let top = 36.0 / 37f32.sqrt();
        assert_close(ribbon[2], Vec3::new(0.0, top, 0.0));
        assert_close(ribbon[1], Vec3::new(-0.5, top / 2.0, 0.0));
    }
}
