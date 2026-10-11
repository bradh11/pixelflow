//! xLights' Fill (`FillEffect::Render`): the target filled from one side up to a position, a
//! line of cells at a time, every line across it one color.
//!
//! The fill starts `offset` lines in and (wrapping) carries on round from the far side. A line's
//! color is its place along the fill in the palette, unless "color by time" is on: then the
//! whole fill is the palette color for the moment, or (with a band size) bands of the palette
//! colors in turn with black gaps. Sizes are whole lines, worked out as xLights works them out.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{FillDirection, FillParams};

pub struct Fill {
    columns: i32,
    rows: i32,
    /// Lines run across the target (the fill goes up or down) rather than up it.
    across: bool,
    /// The color of each line of cells along the way the fill goes.
    lines: Vec<Rgba>,
}

impl Fill {
    pub fn new(p: &FillParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (columns, rows) = grid_size(canvas);
        let across = matches!(p.direction, FillDirection::Up | FillDirection::Down);
        let size = if across { rows } else { columns };
        // xLights reads every setting as a whole number.
        let whole = |v: f32| v.trunc() as i32;
        let share = f64::from(whole(p.position)) / 100.0;
        let (band, skip) = (whole(p.band_size), whole(p.skip_size));
        let offset = if p.offset_in_cells {
            whole(p.offset) % size
        } else {
            (size - 1) * whole(p.offset) / 100
        } % size;
        // The palette along the fill runs from its first line to the last a full, wrapped fill
        // reaches.
        let span = size + offset - 1;
        let timed = Rgba::opaque(colors.ramp(time.cycle_position(1.0) as f32));
        let (mut within, mut band_color) = (0, 0u64);
        let mut lines = vec![Rgba::CLEAR; size as usize];
        let mut draw = |line: i32, along: f64| {
            let color = if !p.color_by_time {
                Rgba::opaque(colors.ramp(along as f32))
            } else if band == 0 {
                timed
            } else if within < band {
                Rgba::opaque(colors.get(band_color))
            } else {
                Rgba::BLACK
            };
            if let Some(cell) = lines.get_mut(line.rem_euclid(size) as usize)
                && (-size..2 * size).contains(&line)
            {
                *cell = color;
            }
            if band > 0 {
                within += 1;
                if within >= band + skip {
                    band_color += 1;
                    within = 0;
                }
            }
        };
        let (size_f, offset_f) = (f64::from(size), f64::from(offset));
        match p.direction {
            FillDirection::Up | FillDirection::Right => {
                let target = if p.wrap {
                    size_f * share + offset_f
                } else {
                    offset_f + f64::from(size - offset) * share
                } as i32;
                for line in offset..target {
                    let along = if span != 0 {
                        f64::from(line) / f64::from(span)
                    } else {
                        0.0
                    };
                    draw(line, along);
                }
            }
            FillDirection::Down | FillDirection::Left => {
                let target = if p.wrap {
                    size_f * (1.0 - share) - offset_f
                } else {
                    f64::from(size - offset) * (1.0 - share)
                } as i32;
                for line in (target..size - offset).rev() {
                    let along = if span != 0 {
                        1.0 - f64::from(line) / f64::from(span)
                    } else {
                        1.0
                    };
                    draw(line, along);
                }
            }
        }
        Self {
            columns,
            rows,
            across,
            lines,
        }
    }
}

impl Shade for Fill {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.columns, self.rows);
        self.lines[if self.across { y } else { x } as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::Rgb;

    const TALL: Canvas = Canvas { columns: 4, rows: 10 };
    const RED_BLUE: [Rgb; 2] = [Rgb::RED, Rgb::BLUE];

    /// Frame `frame` of an 11-frame effect (frame 5 is half way, frame 10 the end).
    fn at(frame: u64) -> EffectTime {
        EffectTime::within(0, 11 * 50, frame * 50).with_frame_ms(50)
    }

    fn fill(p: &FillParams, canvas: Canvas, palette: &[Rgb]) -> Fill {
        Fill::new(p, &at(0), Colors::new(palette), canvas)
    }

    /// Which lines are lit, first line first.
    fn lit(f: &Fill) -> Vec<usize> {
        (0..f.lines.len()).filter(|&i| f.lines[i].a > 0.0).collect()
    }

    fn rgb(c: Rgba) -> [f32; 3] {
        [c.r, c.g, c.b]
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn a_full_fill_spreads_the_palette_along_the_way_it_goes() {
        let up = fill(&FillParams::default(), TALL, &RED_BLUE);
        assert_eq!(lit(&up), (0..10).collect::<Vec<_>>());
        assert!(close(rgb(up.lines[0]), [1.0, 0.0, 0.0]));
        assert!(close(rgb(up.lines[9]), [0.0, 0.0, 1.0]));
        // Line 3 of 10 is 3/9 of the way.
        assert!(close(rgb(up.lines[3]), [2.0 / 3.0, 0.0, 1.0 / 3.0]));
        // Every cell across a line is the line's color.
        let px = |x: f32, y: f32| Pixel {
            u: x,
            v: y,
            index: 0,
            count: 1,
        };
        assert_eq!(up.shade(&px(0.0, 1.0 / 3.0)), up.shade(&px(1.0, 1.0 / 3.0)));
        // Down starts the palette at the top; left and right run across the columns.
        let down = fill(
            &FillParams {
                direction: FillDirection::Down,
                ..FillParams::default()
            },
            TALL,
            &RED_BLUE,
        );
        assert!(close(rgb(down.lines[9]), [1.0, 0.0, 0.0]) && close(rgb(down.lines[0]), [0.0, 0.0, 1.0]));
        let right = fill(
            &FillParams {
                direction: FillDirection::Right,
                ..FillParams::default()
            },
            TALL,
            &RED_BLUE,
        );
        assert_eq!(right.lines.len(), 4);
        assert!(!right.across);
        assert!(close(rgb(right.lines[0]), [1.0, 0.0, 0.0]) && close(rgb(right.lines[3]), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn the_position_is_how_far_the_fill_reaches_in_whole_lines() {
        let reach = |direction, position| {
            lit(&fill(
                &FillParams {
                    direction,
                    position,
                    ..FillParams::default()
                },
                TALL,
                &[],
            ))
        };
        assert_eq!(reach(FillDirection::Up, 50.0), [0, 1, 2, 3, 4]);
        assert_eq!(reach(FillDirection::Down, 50.0), [5, 6, 7, 8, 9]);
        assert_eq!(reach(FillDirection::Up, 0.0), Vec::<usize>::new());
        // 35% of 10 lines is 3 whole lines going up; going down the fill stops at line 6.5,
        // which leaves 4.
        assert_eq!(reach(FillDirection::Up, 35.0), [0, 1, 2]);
        assert_eq!(reach(FillDirection::Down, 35.0), [6, 7, 8, 9]);
        // The position itself is a whole percent.
        assert_eq!(reach(FillDirection::Up, 39.9), [0, 1, 2]);
        // xLights' arithmetic: 50 lines × 0.58 falls just short of 29.
        let tall = Canvas { columns: 1, rows: 50 };
        let p = FillParams {
            position: 58.0,
            ..FillParams::default()
        };
        assert_eq!(lit(&fill(&p, tall, &[])).len(), 28);
    }

    #[test]
    fn an_offset_starts_the_fill_part_way_and_wraps_round() {
        let p = FillParams {
            offset: 3.0,
            position: 50.0,
            ..FillParams::default()
        };
        assert_eq!(lit(&fill(&p, TALL, &[])), [3, 4, 5, 6, 7]);
        let full = fill(&FillParams { position: 100.0, ..p }, TALL, &RED_BLUE);
        assert_eq!(lit(&full), (0..10).collect::<Vec<_>>());
        // The palette runs over lines 0 to 12 (10 lines and the offset, less one): the fill's
        // first line, 3, is a quarter in, and its last, wrapped to line 2, is the end.
        assert!(close(rgb(full.lines[3]), [0.75, 0.0, 0.25]));
        assert!(close(rgb(full.lines[2]), [0.0, 0.0, 1.0]));
        assert!(close(rgb(full.lines[0]), [1.0 / 6.0, 0.0, 5.0 / 6.0]));
        // Without wrapping the fill covers only what's left past the offset.
        let unwrapped = FillParams { wrap: false, ..p };
        assert_eq!(
            lit(&fill(&unwrapped, TALL, &[])),
            [3, 4, 5],
            "half of 7 lines, whole"
        );
        // As a percent, the offset is of the lines less one: 50% of 9 is line 4.
        let percent = FillParams {
            offset: 50.0,
            offset_in_cells: false,
            position: 10.0,
            ..FillParams::default()
        };
        assert_eq!(lit(&fill(&percent, TALL, &[])), [4]);
        // In cells, an offset past the end goes round again.
        let past = FillParams {
            offset: 13.0,
            position: 10.0,
            ..FillParams::default()
        };
        assert_eq!(lit(&fill(&past, TALL, &[])), [3]);
        // Going down the offset counts from the top, and the fill wraps past the bottom.
        let down = FillParams {
            direction: FillDirection::Down,
            offset: 8.0,
            position: 50.0,
            ..FillParams::default()
        };
        assert_eq!(lit(&fill(&down, TALL, &[])), [0, 1, 7, 8, 9]);
    }

    #[test]
    fn color_by_time_gives_the_whole_fill_the_color_of_the_moment() {
        let p = FillParams {
            color_by_time: true,
            ..FillParams::default()
        };
        let color = |frame| {
            let f = Fill::new(&p, &at(frame), Colors::new(&RED_BLUE), TALL);
            assert!(f.lines.iter().all(|c| *c == f.lines[0]), "one color");
            rgb(f.lines[0])
        };
        assert!(close(color(0), [1.0, 0.0, 0.0]));
        assert!(close(color(5), [0.5, 0.0, 0.5]));
        assert!(close(color(10), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn bands_show_only_with_color_by_time_and_leave_black_gaps() {
        let p = FillParams {
            band_size: 2.0,
            skip_size: 1.0,
            color_by_time: true,
            ..FillParams::default()
        };
        let f = fill(&p, TALL, &RED_BLUE);
        let (red, blue) = (Rgba::opaque([1.0, 0.0, 0.0]), Rgba::opaque([0.0, 0.0, 1.0]));
        assert_eq!(
            f.lines,
            [
                red,
                red,
                Rgba::BLACK,
                blue,
                blue,
                Rgba::BLACK,
                red,
                red,
                Rgba::BLACK,
                blue
            ]
        );
        // Without it the lines take their place in the palette, band size or not.
        let plain = FillParams {
            color_by_time: false,
            ..p
        };
        assert_eq!(
            fill(&plain, TALL, &RED_BLUE).lines,
            fill(&FillParams::default(), TALL, &RED_BLUE).lines
        );
    }
}
