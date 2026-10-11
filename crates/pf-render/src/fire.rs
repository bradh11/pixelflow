//! xLights' Fire (`FireEffect::Render`): flames built afresh every frame on the target's grid.
//!
//! The row along the edge the fire burns from is lit at random, hot and cooler cells in turn.
//! Every row after it takes the row before, each cell the mean of the three cells nearest it
//! there (the one straight behind counted twice), then flares up a step (one time in five) or
//! cools a step. The step is sized so the flames die out at the height asked for. xLights keeps
//! the grid from frame to frame but writes all of it each frame, so a frame depends on nothing
//! but its own random numbers; here those come from the effect's seed and the frame, and any
//! frame can be drawn on its own.

use crate::audio::RenderContext;
use crate::color::{Rgba, from_hsv};
use crate::effects::{Canvas, EffectTime, Rng, Shade, hash};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{FireLocation, FireParams};

/// Places in the fire's palette: 100 reds from black up, then 100 hues from red to yellow.
const SHADES: usize = 200;
/// How far the hue moves from one of the bright shades to the next.
const HUE_STEP: f64 = 0.001_666_66;
/// Keeps the fire's random numbers apart from the effect's others.
const SALT: u64 = 0xF1AE;

pub struct Flames {
    columns: i32,
    rows: i32,
    location: FireLocation,
    /// The fire's own grid: cells along the edge it burns from, and rows away from it.
    across: i32,
    away: i32,
    /// Each cell's place in the palette, the row along the edge first.
    heat: Vec<u8>,
    palette: Box<[Rgba; SHADES]>,
}

/// The height xLights builds the frame's flames for, in whole percent: the setting, grown and
/// shrunk `growth_cycles` times over the effect, or with the music from 10% (silence) to 100%.
fn height_percent(p: &FireParams, time: &EffectTime, cx: &RenderContext) -> i32 {
    let mut percent = (f64::from(p.height) * 100.0).round() as i32;
    if p.grow_with_music {
        let frame = time.start_ms / u64::from(time.frame_ms.max(1)) + time.frame();
        let peak = cx.audio.map_or(0.0, |a| a.peak(frame));
        percent = (10.0 + 90.0 * peak) as i32;
    } else if p.growth_cycles > 0.0 {
        let at = time.cycle_position(f64::from(p.growth_cycles));
        percent = (f64::from(percent) + (0.5 - (at - 0.5).abs()) * 100.0) as i32;
    }
    percent.max(1)
}

/// The fire's colors, moved round the color wheel by `hue_shift` percent (stopping at the
/// wheel's end). The dim reds are see-through rather than dark, as xLights draws them on a layer
/// that can be: with a hue shift it dims them as well.
fn palette(hue_shift: i32) -> Box<[Rgba; SHADES]> {
    let shift = f64::from(hue_shift) / 100.0;
    let mut shades = Box::new([Rgba::CLEAR; SHADES]);
    for (i, shade) in shades.iter_mut().enumerate() {
        let (hue, value) = if i < 100 {
            (0.0, i as f64 / 100.0)
        } else {
            ((i - 100) as f64 * HUE_STEP, 1.0)
        };
        let cover = if i < 100 {
            (i * 255 / 100) as f32 / 255.0
        } else {
            1.0
        };
        *shade = if hue_shift > 0 {
            let [r, g, b] = from_hsv([(hue + shift).min(1.0) as f32, 1.0, value as f32]);
            Rgba::new(r, g, b, cover)
        } else if i < 100 {
            Rgba::new(1.0, 0.0, 0.0, cover)
        } else {
            let [r, g, b] = from_hsv([hue as f32, 1.0, 1.0]);
            Rgba::new(r, g, b, 1.0)
        };
    }
    shades
}

impl Flames {
    pub fn new(p: &FireParams, time: &EffectTime, seed: u64, canvas: Canvas, cx: &RenderContext) -> Self {
        let (columns, rows) = grid_size(canvas);
        let (across, away) = match p.location {
            FireLocation::Bottom | FireLocation::Top => (columns, rows),
            FireLocation::Left | FireLocation::Right => (rows, columns),
        };
        // Whole numbers throughout, as xLights works them out.
        let step = 255 * 100 / away / height_percent(p, time, cx);
        let mut rng = Rng::new(hash(seed, time.frame(), SALT));
        let wide = across as usize;
        let mut heat = vec![0u8; wide * away as usize];
        for (x, cell) in heat[..wide].iter_mut().enumerate() {
            *cell = if x % 2 == 0 {
                190 + (rng.next() % 10) as u8
            } else {
                100 + (rng.next() % 50) as u8
            };
        }
        for y in 1..away as usize {
            let (before, row) = heat[(y - 1) * wide..(y + 1) * wide].split_at_mut(wide);
            let mut burning = false;
            for x in 0..wide {
                let (mut sum, mut cells) = (2 * i32::from(before[x]), 2);
                if x > 0 {
                    sum += i32::from(before[x - 1]);
                    cells += 1;
                }
                if x + 1 < wide {
                    sum += i32::from(before[x + 1]);
                    cells += 1;
                }
                let mut shade = sum / cells;
                if shade > 0 {
                    shade += if rng.next() % 100 < 20 { step } else { -step };
                    shade = shade.clamp(0, SHADES as i32 - 1);
                }
                row[x] = shade as u8;
                burning |= shade > 0;
            }
            // Nothing past a row that has gone out.
            if !burning {
                break;
            }
        }
        Self {
            columns,
            rows,
            location: p.location,
            across,
            away,
            heat,
            palette: palette((f64::from(p.hue_shift) * 100.0).round() as i32),
        }
    }

    /// The place in the palette (0 cold to 199 hottest) of the fire's cell `along` the edge it
    /// burns from and `away` from it.
    #[cfg(test)]
    fn heat(&self, along: i32, away: i32) -> i32 {
        i32::from(self.heat[(away * self.across + along) as usize])
    }
}

impl Shade for Flames {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.columns, self.rows);
        let (along, away) = match self.location {
            FireLocation::Bottom => (x, y),
            FireLocation::Top => (x, self.away - 1 - y),
            FireLocation::Left => (y, x),
            FireLocation::Right => (y, self.away - 1 - x),
        };
        self.palette[usize::from(self.heat[(away * self.across + along) as usize])]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Audio;
    use pf_sequence::FireStyle;

    const SEED: u64 = 0xF00D;

    fn canvas(columns: u32, rows: u32) -> Canvas {
        Canvas { columns, rows }
    }

    /// Frame `frame` of a 2-second effect at 20 frames a second.
    fn at(frame: u64) -> EffectTime {
        EffectTime::within(1000, 3000, 1000 + frame * 50).with_frame_ms(50)
    }

    fn fire(height: f32) -> FireParams {
        FireParams {
            height,
            style: FireStyle::Flicker,
            ..FireParams::default()
        }
    }

    fn flames(p: &FireParams, frame: u64, canvas: Canvas) -> Flames {
        Flames::new(p, &at(frame), SEED, canvas, &RenderContext::default())
    }

    /// The step a frame's flames were built with, read back from a one-cell-wide fire: every cell
    /// is the one before it, a step up or down (looking at the first step down that stays lit,
    /// which the palette's ends can't have cut short).
    fn step_of(f: &Flames) -> i32 {
        (1..f.away)
            .find(|&y| f.heat(0, y) > 0 && f.heat(0, y) < f.heat(0, y - 1))
            .map(|y| f.heat(0, y - 1) - f.heat(0, y))
            .expect("a step down")
    }

    fn cell(columns: u32, rows: u32, x: u32, y: u32) -> Pixel {
        Pixel {
            u: x as f32 / (columns - 1).max(1) as f32,
            v: y as f32 / (rows - 1).max(1) as f32,
            index: 0,
            count: 1,
        }
    }

    #[test]
    fn the_edge_row_is_hot_and_cooler_cells_in_turn() {
        for frame in 0..20 {
            let f = flames(&fire(0.5), frame, canvas(30, 10));
            for x in 0..30 {
                let heat = f.heat(x, 0);
                if x % 2 == 0 {
                    assert!((190..200).contains(&heat), "{x}: {heat}");
                } else {
                    assert!((100..150).contains(&heat), "{x}: {heat}");
                }
            }
        }
    }

    #[test]
    fn each_row_is_the_row_before_averaged_then_a_step_up_or_down() {
        // 255 × 100 / 40 rows / 50% = 12 (whole numbers).
        let f = flames(&fire(0.5), 3, canvas(16, 40));
        let (mut up, mut down) = (0, 0);
        for y in 1..40 {
            for x in 0..16 {
                // The cell straight behind counts twice; off the grid, a neighbor isn't counted.
                let mut behind = vec![f.heat(x, y - 1), f.heat(x, y - 1)];
                if x > 0 {
                    behind.push(f.heat(x - 1, y - 1));
                }
                if x < 15 {
                    behind.push(f.heat(x + 1, y - 1));
                }
                let mean = behind.iter().sum::<i32>() / behind.len() as i32;
                let heat = f.heat(x, y);
                if mean == 0 {
                    assert_eq!(heat, 0, "a cold cell stays cold");
                } else if heat == (mean + 12).min(199) {
                    up += 1;
                } else {
                    assert_eq!(heat, (mean - 12).max(0), "({x}, {y}) from {mean}");
                    down += 1;
                }
            }
        }
        // One cell in five flares up.
        let share = f64::from(up) / f64::from(up + down);
        assert!((0.12..0.28).contains(&share), "{up} up, {down} down");
    }

    #[test]
    fn the_step_is_sized_by_the_height_in_whole_numbers() {
        let column = |rows| canvas(1, rows);
        assert_eq!(step_of(&flames(&fire(0.5), 0, column(40))), 12);
        assert_eq!(step_of(&flames(&fire(1.0), 0, column(40))), 6);
        assert_eq!(step_of(&flames(&fire(0.86), 0, column(100))), 2);
        // A height of nothing counts as 1%.
        assert_eq!(step_of(&flames(&fire(0.0), 0, column(255))), 100);
        // Lower flames go out sooner.
        let lit = |height: f32| {
            let f = flames(&fire(height), 5, canvas(40, 60));
            (0..60).filter(|&y| (0..40).any(|x| f.heat(x, y) > 0)).count()
        };
        assert!(lit(0.2) < lit(0.5) && lit(0.5) < lit(1.0));
        assert!(lit(0.2) < 30, "{}", lit(0.2));
    }

    #[test]
    fn flames_grow_and_shrink_over_each_growth_cycle() {
        // Two cycles over 40 frames: each 20 frames, from the height to 50% more and back.
        let p = FireParams {
            growth_cycles: 2.0,
            ..fire(0.2)
        };
        let step = |frame| step_of(&flames(&p, frame, canvas(1, 100)));
        assert_eq!(step(0), 255 * 100 / 100 / 20);
        // Half way through a cycle of 20 frames (position 9/19 and 10/19): 20 + 47.
        assert_eq!(step(9), 255 * 100 / 100 / 67);
        assert_eq!(step(10), 255 * 100 / 100 / 67);
        assert_eq!(step(19), 255 * 100 / 100 / 20, "back down at the cycle's end");
        assert_eq!(step(20), 255 * 100 / 100 / 20, "and the next starts");
    }

    #[test]
    fn flames_follow_the_music_from_a_tenth_of_the_height() {
        let p = FireParams {
            grow_with_music: true,
            growth_cycles: 5.0,
            ..fire(0.9)
        };
        // Without music: 10%, whatever the height and growth cycles say.
        assert_eq!(step_of(&flames(&p, 7, canvas(1, 100))), 255 * 100 / 100 / 10);
        // One second loud, one second half as loud.
        let mut samples = vec![0.8f32; 44_100];
        samples.extend(std::iter::repeat_n(0.4, 44_100));
        let track = pf_analysis::audio_track(samples, 44_100, 50);
        let cx = RenderContext::new(Some(Audio::new(&track, 50)), &[], 50);
        let step = |effect_start_ms: u64| {
            let time = EffectTime::within(effect_start_ms, effect_start_ms + 500, effect_start_ms + 100)
                .with_frame_ms(50);
            step_of(&Flames::new(&p, &time, SEED, canvas(1, 100), &cx))
        };
        // The loudest frames: 10 + 90 × 1; at half the peak: 10 + 90 × ½.
        assert_eq!(step(200), 255 * 100 / 100 / 100);
        assert_eq!(step(1400), 255 * 100 / 100 / 55);
    }

    #[test]
    fn dim_shades_are_see_through_red_and_bright_ones_run_to_yellow() {
        let shades = palette(0);
        assert_eq!(shades[0], Rgba::new(1.0, 0.0, 0.0, 0.0), "cold: nothing drawn");
        assert_eq!(shades[50], Rgba::new(1.0, 0.0, 0.0, 127.0 / 255.0));
        assert_eq!(shades[100], Rgba::new(1.0, 0.0, 0.0, 1.0));
        let hottest = shades[199];
        assert!(hottest.r == 1.0 && (hottest.g - 0.99).abs() < 0.001 && hottest.b == 0.0 && hottest.a == 1.0);
        assert!(shades[150].g > 0.45 && shades[150].g < 0.55);
    }

    #[test]
    fn a_hue_shift_moves_every_shade_and_stops_at_the_end_of_the_wheel() {
        // A third of the way round: red becomes green.
        let shades = palette(33);
        let base = shades[100];
        assert!(base.g == 1.0 && base.r < 0.05 && base.b == 0.0, "{base:?}");
        // Dim shades are both darker and see-through.
        let dim = shades[50];
        assert!(
            (dim.g - 0.5).abs() < 1e-6 && (dim.a - 127.0 / 255.0).abs() < 1e-6,
            "{dim:?}"
        );
        // Past the end of the wheel the hue stays there, which xLights draws as magenta.
        let end = palette(98)[199];
        assert_eq!([end.r, end.g, end.b], [1.0, 0.0, 1.0]);
    }

    #[test]
    fn fire_burns_from_whichever_edge_is_chosen() {
        let from = |location| {
            let p = FireParams {
                location,
                ..fire(0.3)
            };
            flames(&p, 2, canvas(12, 20))
        };
        let lit = |f: &Flames, x: u32, y: u32| f.shade(&cell(12, 20, x, y)).a > 0.0;
        let bottom = from(FireLocation::Bottom);
        assert!((0..12).all(|x| lit(&bottom, x, 0)) && (0..12).all(|x| !lit(&bottom, x, 19)));
        let top = from(FireLocation::Top);
        assert!((0..12).all(|x| lit(&top, x, 19)) && (0..12).all(|x| !lit(&top, x, 0)));
        // From a side the fire's rows run up the prop: 20 cells along the edge, 12 away from it.
        let left = from(FireLocation::Left);
        assert!((0..20).all(|y| lit(&left, 0, y)) && (0..20).all(|y| !lit(&left, 11, y)));
        assert_eq!((left.across, left.away), (20, 12));
        let right = from(FireLocation::Right);
        assert!((0..20).all(|y| lit(&right, 11, y)) && (0..20).all(|y| !lit(&right, 0, y)));
        // The same flames, turned: top is bottom upside down.
        for x in 0..12 {
            for y in 0..20 {
                assert_eq!(
                    top.shade(&cell(12, 20, x, 19 - y)),
                    bottom.shade(&cell(12, 20, x, y))
                );
            }
        }
    }

    #[test]
    fn every_frame_is_new_and_any_frame_can_be_drawn_on_its_own() {
        let p = fire(0.5);
        let heat = |frame| flames(&p, frame, canvas(20, 20)).heat;
        assert_eq!(heat(7), heat(7));
        assert_ne!(heat(7), heat(8), "new flames every frame");
        // Within one frame nothing changes.
        let within = |ms: u64| {
            Flames::new(
                &p,
                &EffectTime::within(0, 2000, ms).with_frame_ms(50),
                SEED,
                canvas(20, 20),
                &RenderContext::default(),
            )
            .heat
        };
        assert_eq!(within(350), within(399));
        assert_ne!(within(350), within(400));
    }
}
