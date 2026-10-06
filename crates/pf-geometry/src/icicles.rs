//! Icicles as xLights lays them out (`IciclesModel::InitModel`), scaled to the prop's width and
//! drop height.

use pf_model::Vec3;

/// The settings of a run of icicles (see `pf_model::Generator::Icicles`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Icicles<'a> {
    pub strings: u32,
    pub lights_per_string: u32,
    pub drops: &'a [u32],
    pub width: f32,
    pub drop_height: f32,
    pub alternate_nodes: bool,
}

pub(crate) fn positions(ic: Icicles) -> Vec<Vec3> {
    if ic.strings == 0 || ic.lights_per_string == 0 {
        return Vec::new();
    }
    // A pattern without a drop that holds pixels would never place one; xLights' default is 5.
    let drops = if ic.drops.iter().all(|&d| d == 0) {
        &[5][..]
    } else {
        ic.drops
    };
    let longest = drops.iter().copied().max().unwrap_or(1);
    let spacing = ic.drop_height / longest.saturating_sub(1).max(1) as f32;

    // Columns and spots down the drop, each string starting a new column and the pattern over.
    let mut spots = Vec::with_capacity(ic.strings as usize * ic.lights_per_string as usize);
    let mut column: i64 = -1;
    for _ in 0..ic.strings {
        column += 1;
        let (mut y, mut d) = (0u32, 0usize);
        for _ in 0..ic.lights_per_string {
            while y >= drops[d] {
                column += 1;
                y = 0;
                d = (d + 1) % drops.len();
            }
            let n = drops[d];
            let spot = if !ic.alternate_nodes {
                y
            } else if y < n.div_ceil(2) {
                2 * y
            } else {
                (n - (y + 1)) * 2 + 1
            };
            spots.push((column, spot));
            y += 1;
        }
    }
    let last = column as f32;
    spots
        .into_iter()
        .map(|(col, spot)| {
            let x = if column == 0 {
                0.0
            } else {
                (col as f32 / last - 0.5) * ic.width
            };
            Vec3::new(x, -(spot as f32) * spacing, 0.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn icicles(strings: u32, lights_per_string: u32, drops: &[u32]) -> Vec<Vec3> {
        positions(Icicles {
            strings,
            lights_per_string,
            drops,
            width: 2.0,
            drop_height: 0.3,
            alternate_nodes: false,
        })
    }

    #[test]
    fn fills_each_drop_top_down_then_moves_a_column_right() {
        let p = icicles(1, 7, &[3, 4]);
        let want = [
            (-1.0, 0.0),
            (-1.0, -0.1),
            (-1.0, -0.2),
            (1.0, 0.0),
            (1.0, -0.1),
            (1.0, -0.2),
            (1.0, -0.3),
        ];
        assert_eq!(p.len(), want.len());
        for (q, (x, y)) in p.iter().zip(want) {
            assert_close(*q, Vec3::new(x, y, 0.0));
        }
    }

    #[test]
    fn each_string_starts_the_pattern_over_in_a_new_column() {
        // Columns 0..=3 over a width of 2: 2/3 apart.
        let p = icicles(2, 4, &[3]);
        assert_close(p[3], Vec3::new(-1.0 / 3.0, 0.0, 0.0));
        assert_close(p[4], Vec3::new(1.0 / 3.0, 0.0, 0.0));
        assert_close(p[6], Vec3::new(1.0 / 3.0, -0.3, 0.0));
        assert_close(p[7], Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn a_drop_of_none_leaves_a_gap() {
        let p = icicles(1, 4, &[2, 0, 2]);
        assert_close(p[1], Vec3::new(-1.0, -0.3, 0.0));
        assert_close(p[2], Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn alternate_nodes_go_down_every_other_spot_and_back_up() {
        let p = positions(Icicles {
            strings: 1,
            lights_per_string: 5,
            drops: &[5],
            width: 2.0,
            drop_height: 0.4,
            alternate_nodes: true,
        });
        let ys: Vec<f32> = p.iter().map(|q| q.y).collect();
        for (y, want) in ys.iter().zip([0.0, -0.2, -0.4, -0.3, -0.1]) {
            assert!((y - want).abs() < 1e-5, "{ys:?}");
        }
        // One column: it sits in the middle.
        assert!(p.iter().all(|q| q.x == 0.0));
    }

    #[test]
    fn drops_of_one_pixel_are_spaced_by_the_drop_height_and_an_empty_pattern_hangs_fives() {
        let p = icicles(1, 3, &[1]);
        assert_close(p[1], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(icicles(1, 6, &[]).len(), 6);
        // Drops of 5 (0.3 / 4 apart): the sixth pixel starts a second column.
        let fives = icicles(1, 6, &[0, 0]);
        assert_close(fives[4], Vec3::new(-1.0, -0.3, 0.0));
        assert_close(fives[5], Vec3::new(1.0, 0.0, 0.0));
        assert!(icicles(0, 6, &[3]).is_empty());
        assert!(icicles(2, 0, &[3]).is_empty());
    }
}
