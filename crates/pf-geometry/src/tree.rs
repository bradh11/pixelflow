use crate::spread;
use pf_model::{Corner, TreeStyle, Vec3};

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
    pub start: Corner,
    pub strands_per_string: u32,
    pub alternate_nodes: bool,
    pub spiral_rotations: f32,
}

/// Strings running from the base (y = 0) to the top (y = height), string by string. Round: a
/// cone, as xLights wraps a tree (string `s` at `start_angle + s × step` round from the front,
/// the step spreading the strings over `degrees`, reaching both edges when that's under 350°),
/// spiralling `spiral_rotations` times round on the way up as xLights winds it. Flat and ribbon:
/// fanned out in the front view (xLights' `SetTreeCoord` for those styles).
///
/// Wiring is xLights' vertical matrix wiring: the first string on the left (or, from a right
/// corner, the last spot is the first string's), each running up (or down, from a top corner);
/// with `serpentine` every other string runs back, starting afresh every `strands_per_string`
/// strings when that's set; with `alternate_nodes` each string goes up every other spot and
/// comes back down the ones between.
pub(crate) fn positions(t: Tree) -> Vec<Vec3> {
    let n = t.strings;
    let per = t.nodes_per_string;
    let mut out = Vec::with_capacity(n as usize * per as usize);
    let step = if t.degrees < 350.0 && n > 1 {
        t.degrees / (n - 1) as f32
    } else {
        t.degrees / n.max(1) as f32
    };
    let from_right = matches!(t.start, Corner::BottomRight | Corner::TopRight);
    let from_top = matches!(t.start, Corner::TopLeft | Corner::TopRight);
    let spiral = (t.style == TreeStyle::Round && t.spiral_rotations != 0.0 && t.spiral_rotations.is_finite())
        .then(|| {
            // xLights' spiral, in its own units: the tree 3 per row tall.
            let unit = t.height / (3.0 * per as f32);
            (unit.is_finite() && unit != 0.0)
                .then(|| spiral_offsets(per, t.spiral_rotations, t.base_radius / unit, t.top_radius / unit))
        })
        .flatten();
    for s in 0..n {
        let spot = if from_right { n - 1 - s } else { s };
        let angle = t.start_angle + spot as f32 * step;
        // Flat and ribbon: the string's place across the base and the top, -1 to 1 of the width.
        let across = (spot as f32 + 0.5 - n as f32 / 2.0) / (n as f32 / 2.0);
        let (xb, xt) = (across * t.base_radius, across * t.top_radius);
        let slant = (t.height * t.height + (xt - xb) * (xt - xb)).sqrt();
        let fold = if t.strands_per_string > 0 {
            s % t.strands_per_string
        } else {
            s
        };
        for j in 0..per {
            let along = if t.alternate_nodes {
                interleave(j, per)
            } else if t.serpentine && fold % 2 == 1 {
                per - 1 - j
            } else {
                j
            };
            let row = if from_top { per - 1 - along } else { along };
            let (f, turn) = match &spiral {
                Some((heights, turns)) => (
                    if per > 1 {
                        heights[row as usize] / (per - 1) as f32
                    } else {
                        0.5
                    },
                    turns[row as usize],
                ),
                None => (spread(row, per), 0.0),
            };
            out.push(match t.style {
                TreeStyle::Round => {
                    let radius = t.base_radius + (t.top_radius - t.base_radius) * f;
                    let a = angle.to_radians() + turn;
                    Vec3::new(radius * a.sin(), f * t.height, radius * a.cos())
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

/// The row a string's `y`th pixel sits on when it goes up every other spot and comes back down
/// the ones between.
fn interleave(y: u32, n: u32) -> u32 {
    if y < n.div_ceil(2) {
        y * 2
    } else {
        (n - (y + 1)) * 2 + 1
    }
}

/// Each row's height (in rows) and turn (radians) up a spiral tree of `rows` rows, `radius` and
/// `top_radius` in xLights' units (the tree 3 a row tall), as xLights winds it: ten stretches,
/// each a share of the rows by its length round the cone.
pub(crate) fn spiral_offsets(rows: u32, spiral: f32, radius: f32, top_radius: f32) -> (Vec<f32>, Vec<f32>) {
    let n = rows as usize;
    let mut heights: Vec<f32> = (0..n).map(|x| x as f32).collect();
    let mut turns = vec![0.0f32; n];
    if spiral == 0.0 || n == 0 {
        return (heights, turns);
    }
    let bh = rows as f32;
    let gap = (radius - top_radius) / 10.0;
    let mut lengths = [0.0f32; 10];
    let mut total = 0.0f32;
    for (x, l) in lengths.iter_mut().enumerate() {
        *l = 2.0 * std::f32::consts::PI * (radius - gap * x as f32) - gap / 2.0;
        *l *= spiral / 10.0;
        *l = (*l * *l + bh / 10.0 * bh / 10.0).sqrt();
        total += *l;
    }
    for l in lengths.iter_mut() {
        *l /= total;
    }
    let mut stretch = 0usize;
    let mut in_stretch = (lengths[0] * bh).round();
    let mut done = 0.0f32;
    for x in 1..n {
        if done >= in_stretch {
            stretch = (stretch + 1).min(9);
            done = 0.0;
            in_stretch = if stretch == 9 {
                (n - x) as f32
            } else {
                (lengths[stretch] * bh).round()
            };
        }
        if in_stretch > 0.0 {
            heights[x] = heights[x - 1] + (f64::from(bh) / 10.0 / f64::from(in_stretch)) as f32;
            turns[x] = turns[x - 1] + spiral * 2.0 * std::f32::consts::PI / 10.0 / in_stretch;
        } else {
            heights[x] = heights[x - 1];
            turns[x] = turns[x - 1];
        }
        done += 1.0;
    }
    (heights, turns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn plain(strings: u32, nodes: u32) -> Tree {
        Tree {
            strings,
            nodes_per_string: nodes,
            height: 6.0,
            base_radius: 2.0,
            top_radius: 0.0,
            serpentine: false,
            style: TreeStyle::Round,
            degrees: 360.0,
            start_angle: 0.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
        }
    }

    fn tree(
        strings: u32,
        nodes: u32,
        style: TreeStyle,
        degrees: f32,
        start_angle: f32,
        serpentine: bool,
    ) -> Vec<Vec3> {
        positions(Tree {
            serpentine,
            style,
            degrees,
            start_angle,
            ..plain(strings, nodes)
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
            serpentine: true,
            ..plain(2, 3)
        });
        assert_close(p[3], Vec3::new(0.0, 6.0, -2.0));
        assert_close(p[5], Vec3::new(0.0, 0.0, -2.0));
    }

    #[test]
    fn wired_from_a_top_or_right_corner_strings_start_there() {
        // Four strings round, two pixels each: from the top right, the first string is the
        // last spot round (270°, on the left), running down.
        let p = positions(Tree {
            top_radius: 2.0,
            start: Corner::TopRight,
            ..plain(4, 2)
        });
        assert_close(p[0], Vec3::new(-2.0, 6.0, 0.0));
        assert_close(p[1], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, 6.0, -2.0));
    }

    #[test]
    fn zig_zag_starts_afresh_with_each_folded_string() {
        // Three strands to a string: up, down, up, then up again on the next string.
        let p = positions(Tree {
            top_radius: 2.0,
            serpentine: true,
            strands_per_string: 3,
            ..plain(4, 2)
        });
        let ups: Vec<bool> = p.chunks(2).map(|s| s[0].y < s[1].y).collect();
        assert_eq!(ups, [true, false, true, true]);
    }

    #[test]
    fn alternate_pixels_go_up_every_other_spot_and_come_back_down() {
        let p = positions(Tree {
            top_radius: 2.0,
            alternate_nodes: true,
            ..plain(1, 5)
        });
        let rows: Vec<f32> = p.iter().map(|q| q.y / 1.5).collect();
        for (a, b) in rows.iter().zip([0.0, 2.0, 4.0, 3.0, 1.0]) {
            assert!((a - b).abs() < 1e-5, "{rows:?}");
        }
    }

    #[test]
    fn a_spiral_tree_winds_round_as_it_goes_up() {
        let straight = positions(plain(1, 40));
        let wound = positions(Tree {
            spiral_rotations: 2.0,
            ..plain(1, 40)
        });
        assert_close(wound[0], straight[0]);
        // Part way up it has turned off the front.
        assert!(wound[10].x.abs() > 0.1, "{:?}", wound[10]);
        // xLights' winding: the rows climb in ten stretches, steadily, about two turns all told.
        let (heights, turns) = spiral_offsets(40, 2.0, 40.0 / 1.2, 40.0 / 1.2 / 6.0);
        assert!(heights.windows(2).all(|w| w[1] > w[0]), "{heights:?}");
        assert!(turns.windows(2).all(|w| w[1] > w[0]), "{turns:?}");
        let two_turns = 4.0 * std::f32::consts::PI;
        assert!(
            turns[39] > 0.8 * two_turns && turns[39] <= two_turns + 1e-3,
            "{turns:?}"
        );
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
