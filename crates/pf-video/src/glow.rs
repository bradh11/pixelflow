//! The glow around lit pixels, drawn the way the 2D previews draw it.
//!
//! This is the window's `app/src/lib/pixelGlow.ts`, which says why the look is what it is, from
//! the same numbers. Around each lit dot is a soft halo in its color, a plain bell added onto
//! everything it reaches. So that a dot still comes out its own color under its own halo, lit dots
//! are drawn dimmer by what the halo adds back ([`core_dim`]). A halo is sized from the dot a
//! pixel has in the layout, not from the dot drawn for it, so the glow covers as much of the
//! display however the dots are sized.
//!
//! A halo is many times its dot's size, so every halo is stamped into one coarse picture, in
//! which a halo's bell is only a few pixels wide (a halo has no sharp edges to lose), and that
//! picture is added onto the frame enlarged and smoothed, as the previews' canvas enlarges
//! theirs.

use crate::dots::{PixelBox, View, round};
use pf_engine::PreviewProp;

/// A glow's width (its bell's width, in dot radii) and strength beside the dot, at none and at
/// full glow.
const GLOW_WIDTH: (f64, f64) = (1.2, 2.4);
const GLOW_STRENGTH: (f64, f64) = (0.0, 0.7);
/// Glow is drawn at the nearest of this many steps from none to full (the slider's steps of 5%).
const GLOW_STEPS: f64 = 20.0;
/// Halos whose bell is narrower than this (picture pixels) are lost under their dots: they
/// aren't drawn.
const NARROWEST_BELL: f64 = 1.0;
/// A halo's bell is at least this many pixels of the coarse picture wide,
const BELL_CELLS: f64 = 2.0;
/// unless that would give the coarse picture more pixels than this.
const MOST_CELLS: f64 = 500_000.0;
/// A dot sits at one of this many places across a pixel of the coarse picture (each way), so its
/// halo stays centered on it.
const PLACES: usize = 4;

/// The step (0 to `GLOW_STEPS`) a glow level (0–1) is drawn at; a level that isn't a number is
/// none.
fn glow_step(level: f32) -> f64 {
    if level.is_finite() {
        round(f64::from(level).clamp(0.0, 1.0) * GLOW_STEPS)
    } else {
        0.0
    }
}

/// A halo's bell width at a glow step, in dot radii.
fn bell_width(step: f64) -> f64 {
    GLOW_WIDTH.0 + (GLOW_WIDTH.1 - GLOW_WIDTH.0) * (step / GLOW_STEPS)
}

fn strength(step: f64) -> f64 {
    GLOW_STRENGTH.0 + (GLOW_STRENGTH.1 - GLOW_STRENGTH.0) * (step / GLOW_STEPS)
}

/// How bright to draw lit dots (0–1 of their color) so that with their own halo added they come
/// out their own color: 1 less the halo's strength averaged over the dot. `spread` is the size the
/// halo is made for, as a share of the dot's drawn size.
fn core_dim(step: f64, spread: f64) -> f64 {
    let w = bell_width(step) * spread;
    1.0 - strength(step) * w * w * (1.0 - (-1.0 / (w * w)).exp())
}

/// A halo's weights (out of 256) for the square of coarse pixels within `reach` of the one its
/// dot is in, row by row, and where each row's weights start and end: the rest are zero.
#[derive(Debug, Clone)]
struct Sprite {
    weights: Vec<u32>,
    rows: Vec<(usize, usize)>,
}

impl Sprite {
    /// The halo of a dot of `radius` (coarse pixels) at a glow step, for a dot `fx`, `fy` (0–1)
    /// across its pixel.
    fn new(radius: f64, step: f64, reach: usize, fx: f64, fy: f64) -> Self {
        let bell = radius * bell_width(step);
        let side = 2 * reach + 1;
        let peak = strength(step) * 256.0;
        let mut weights = Vec::with_capacity(side * side);
        let mut rows = vec![(side, 0); side];
        for (row, (from, to)) in rows.iter_mut().enumerate() {
            for col in 0..side {
                let x = col as f64 - reach as f64 + 0.5 - fx;
                let y = row as f64 - reach as f64 + 0.5 - fy;
                let weight = round(peak * (-(x * x + y * y) / (bell * bell)).exp()) as u32;
                weights.push(weight);
                if weight > 0 {
                    (*from, *to) = ((*from).min(col), col + 1);
                }
            }
        }
        Self { weights, rows }
    }
}

/// One pixel's halo: the coarse pixel its dot is in, which sprite is stamped around it, and where
/// its color is in a show frame.
#[derive(Debug, Clone, Copy)]
struct Mark {
    x: i32,
    y: i32,
    sprite: u8,
    at: usize,
}

/// Where the picture's rows (or columns) fall among a coarse picture's, for enlarging it. The
/// picture is cut into patches: patch `q` is between the middles of coarse rows `q - 1` and `q`
/// (the first and last patches are the edges, outside the outermost middles).
#[derive(Debug, Clone)]
struct Enlarging {
    /// For each patch and one more, the first of the picture's rows in it or after it.
    firsts: Vec<usize>,
    /// For each of the picture's rows, how far it is across its patch (of 256).
    toward: Vec<u32>,
}

impl Enlarging {
    /// For a coarse picture `cells` rows long, each `cell` of the picture's `length` rows.
    fn new(cells: usize, length: usize, cell: f64) -> Self {
        let mut firsts = vec![length; cells + 2];
        let mut toward = Vec::with_capacity(length);
        let mut next = 0;
        for at in 0..length {
            // How many coarse rows down from the first one's middle this row's middle is.
            let down = (at as f64 + 0.5) / cell - 0.5;
            let patch = ((down.floor() + 1.0).max(0.0) as usize).min(cells);
            while next <= patch {
                firsts[next] = at;
                next += 1;
            }
            // At an edge there's only the one coarse row to take from.
            let between = patch > 0 && patch < cells;
            toward.push(if between {
                round((down - down.floor()) * 256.0) as u32
            } else {
                0
            });
        }
        Self { firsts, toward }
    }
}

/// The coarse picture halos are added up in, kept between frames: for each of its pixels red,
/// green and blue times 256 (so halos can add past full brightness), and the same as colors.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sums {
    sums: Vec<[u32; 3]>,
    colors: Vec<[u8; 3]>,
}

/// Every pixel's halo for one view of a layout at one glow level, ready to add frame after frame.
#[derive(Debug, Clone)]
pub(crate) struct Halos {
    step: f64,
    /// The radius of the dot a halo is made for, in picture pixels.
    radius: f64,
    /// The coarse picture's size, and the picture's.
    width: usize,
    height: usize,
    picture: (usize, usize),
    reach: usize,
    sprites: Vec<Sprite>,
    marks: Vec<Mark>,
    columns: Enlarging,
    rows: Enlarging,
}

impl Halos {
    /// The halos of `props` in `view` for a glow level (0–1), in a picture `width` by `height`
    /// pixels, around dots of `radius` (screen pixels). None when there's no glow or the halos
    /// would be too small to see: then dots are drawn as they are.
    pub(crate) fn new(
        props: &[PreviewProp],
        view: &View,
        (width, height): (usize, usize),
        radius: f64,
        level: f32,
    ) -> Option<Self> {
        let step = glow_step(level);
        if step <= 0.0 {
            return None;
        }
        let bell = radius * view.ratio * bell_width(step);
        if bell.is_nan() || bell < NARROWEST_BELL {
            return None;
        }
        // Picture pixels across each pixel of the coarse picture, in quarters.
        let cell = ((4.0 * bell / BELL_CELLS).floor() / 4.0)
            .max(1.0)
            .max(((width * height) as f64 / MOST_CELLS).sqrt());
        let coarse = (
            (width as f64 / cell).ceil() as usize,
            (height as f64 / cell).ceil() as usize,
        );
        let scale = view.ratio / cell;
        let around = radius * scale;
        let reach = (around + 2.2 * around * bell_width(step)).ceil() as usize;
        let sprites = (0..PLACES * PLACES)
            .map(|place| {
                let across = |p: usize| (p as f64 + 0.5) / PLACES as f64;
                Sprite::new(
                    around,
                    step,
                    reach,
                    across(place % PLACES),
                    across(place / PLACES),
                )
            })
            .collect();
        let mut marks = Vec::new();
        for prop in props {
            for (i, p) in prop.points.as_chunks::<2>().0.iter().enumerate() {
                let (x, y) = view.place(p[0].into(), p[1].into());
                let (x, y) = (x / cell, y / cell);
                let (ix, iy) = (x.floor(), y.floor());
                let outside =
                    |at: f64, cells: usize| at + (reach as f64) < 0.0 || at - reach as f64 >= cells as f64;
                if !(x.is_finite() && y.is_finite()) || outside(ix, coarse.0) || outside(iy, coarse.1) {
                    continue;
                }
                let place = |f: f64| (f * PLACES as f64).floor() as usize;
                marks.push(Mark {
                    x: ix as i32,
                    y: iy as i32,
                    sprite: (place(y - iy) * PLACES + place(x - ix)) as u8,
                    at: prop.frame_offset + i * usize::from(prop.channels_per_pixel),
                });
            }
        }
        Some(Self {
            step,
            radius: radius * view.ratio,
            width: coarse.0,
            height: coarse.1,
            picture: (width, height),
            reach,
            sprites,
            marks,
            columns: Enlarging::new(coarse.0, width, cell),
            rows: Enlarging::new(coarse.1, height, cell),
        })
    }

    /// How bright to draw lit dots `dot` picture pixels wide, so that with their halo added they
    /// come out their own color.
    pub(crate) fn dim(&self, dot: f64) -> f64 {
        core_dim(self.step, (self.radius * 2.0) / dot)
    }

    /// Adds the halo of every lit pixel of `frame` onto `rgb`, the picture (red, green and blue
    /// for each pixel, none going past 255).
    pub(crate) fn add(&self, frame: &[u8], scratch: &mut Sums, rgb: &mut [u16]) {
        let (width, height) = (self.width, self.height);
        if scratch.sums.len() != width * height {
            scratch.sums = vec![[0; 3]; width * height];
            scratch.colors = vec![[0; 3]; width * height];
        }
        let Sums { sums, colors } = scratch;
        let reach = self.reach as i32;
        let side = 2 * self.reach + 1;
        // The box of the coarse picture that's added to.
        let mut lit: Option<PixelBox> = None;
        for mark in &self.marks {
            let Some(color) = frame.get(mark.at..).and_then(|rest| rest.first_chunk::<3>()) else {
                continue;
            };
            if *color == [0; 3] {
                continue;
            }
            let x0 = (mark.x - reach).max(0) as usize;
            let x1 = (mark.x + reach + 1).clamp(0, width as i32) as usize;
            let y0 = (mark.y - reach).max(0) as usize;
            let y1 = (mark.y + reach + 1).clamp(0, height as i32) as usize;
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let sprite = &self.sprites[usize::from(mark.sprite)];
            let color = color.map(u32::from);
            // The coarse column the sprite's first is over (it may be off the picture).
            let left = mark.x - reach;
            for y in y0..y1 {
                let row = (y as i32 - mark.y + reach) as usize;
                let (from, to) = sprite.rows[row];
                let a = x0.max((left + from as i32).max(0) as usize);
                let b = x1.min((left + to as i32).max(0) as usize);
                if a >= b {
                    continue;
                }
                let weights = &sprite.weights[row * side + (a as i32 - left) as usize..];
                for (sum, &weight) in sums[y * width + a..y * width + b].iter_mut().zip(weights) {
                    for c in 0..3 {
                        sum[c] = sum[c].saturating_add(color[c] * weight);
                    }
                }
            }
            let grown = lit.unwrap_or(PixelBox { x0, y0, x1, y1 });
            lit = Some(PixelBox {
                x0: grown.x0.min(x0),
                y0: grown.y0.min(y0),
                x1: grown.x1.max(x1),
                y1: grown.y1.max(y1),
            });
        }
        let Some(lit) = lit else { return };

        // As colors, each as bright as it adds up to; the sums are emptied for the next frame.
        for y in lit.y0..lit.y1 {
            let row = y * width + lit.x0..y * width + lit.x1;
            for (color, sum) in colors[row.clone()].iter_mut().zip(&mut sums[row]) {
                *color = sum.map(|c| (c >> 8).min(255) as u8);
                *sum = [0; 3];
            }
        }
        // Enlarged onto the picture: each of its pixels takes from the four coarse pixels around
        // it, by how near each is. A patch between four with no light in them is skipped.
        let (columns, rows) = (&self.columns, &self.rows);
        let at = |x: usize, y: usize| colors[y.min(height - 1) * width + x.min(width - 1)];
        for qy in lit.y0..=lit.y1 {
            for qx in lit.x0..=lit.x1 {
                // The coarse pixels at the patch's corners (the same one twice at an edge).
                let (cx, cy) = (qx.saturating_sub(1), qy.saturating_sub(1));
                let corners = [at(cx, cy), at(qx, cy), at(cx, qy), at(qx, qy)];
                if corners == [[0; 3]; 4] {
                    continue;
                }
                let corners = corners.map(|c| c.map(u32::from));
                let (left, right) = (columns.firsts[qx], columns.firsts[qx + 1]);
                for y in rows.firsts[qy]..rows.firsts[qy + 1] {
                    let ty = rows.toward[y];
                    let row = &mut rgb[(y * self.picture.0 + left) * 3..(y * self.picture.0 + right) * 3];
                    for (pixel, tx) in row
                        .as_chunks_mut::<3>()
                        .0
                        .iter_mut()
                        .zip(&columns.toward[left..right])
                    {
                        for c in 0..3 {
                            let top = corners[0][c] * (256 - tx) + corners[1][c] * tx;
                            let bottom = corners[2][c] * (256 - tx) + corners[3][c] * tx;
                            let light = (top * (256 - ty) + bottom * ty + (1 << 15)) >> 16;
                            pixel[c] = (u32::from(pixel[c]) + light).min(255) as u16;
                        }
                    }
                }
            }
        }
        for y in lit.y0..lit.y1 {
            colors[y * width + lit.x0..y * width + lit.x1].fill([0; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_is_drawn_at_the_nearest_of_twenty_steps() {
        assert_eq!(glow_step(0.0), 0.0);
        assert_eq!(glow_step(0.5), 10.0);
        assert_eq!(glow_step(0.52), 10.0);
        assert_eq!(glow_step(1.0), 20.0);
        // Out-of-range levels are kept in range rather than refused.
        assert_eq!(glow_step(-1.0), 0.0);
        assert_eq!(glow_step(9.0), 20.0);
        assert_eq!(glow_step(f32::NAN), 0.0);
    }

    #[test]
    fn a_dot_and_its_own_halo_add_up_to_its_color() {
        // The halo's weight averaged over the dot (a disc of radius 1, the halo's bell in the
        // same units), by adding it up finely.
        for step in [2.0, 10.0, 20.0] {
            for spread in [0.5, 1.0, 2.0] {
                let bell = bell_width(step) * spread;
                let (mut sum, mut count) = (0.0, 0.0);
                for iy in -200..=200 {
                    for ix in -200..=200 {
                        let (x, y) = (f64::from(ix) / 200.0, f64::from(iy) / 200.0);
                        if x * x + y * y <= 1.0 {
                            sum += strength(step) * (-(x * x + y * y) / (bell * bell)).exp();
                            count += 1.0;
                        }
                    }
                }
                let dim = core_dim(step, spread);
                assert!((dim + sum / count - 1.0).abs() < 2e-3, "{step} {spread}: {dim}");
            }
        }
    }

    #[test]
    fn enlarging_places_each_picture_row_between_two_coarse_ones() {
        // Coarse rows 2.5 picture rows deep: their middles are at 1.25, 3.75, 6.25 and 8.75.
        let rows = Enlarging::new(4, 10, 2.5);
        assert_eq!(rows.firsts, vec![0, 1, 4, 6, 9, 10]);
        // The row whose middle is at 2.5 is half way between the first two; the one at 1.5 is a
        // tenth of the way.
        assert_eq!(rows.toward[2], 128);
        assert_eq!(rows.toward[1], 26);
        // Before the first middle and after the last, a row takes the edge's color alone.
        assert_eq!((rows.toward[0], rows.toward[9]), (0, 0));
    }

    fn prop(points: &[f32]) -> PreviewProp {
        PreviewProp {
            prop: pf_model::PropId::new(),
            frame_offset: 0,
            channels_per_pixel: 3,
            points: points.to_vec(),
            flood: false,
        }
    }

    /// A picture 200 by 100 showing the layout at 100 screen pixels to a unit, two picture pixels
    /// to a screen pixel, with (0, 0) in the middle.
    const VIEW: View = View {
        cx: 0.0,
        cy: 0.0,
        zoom: 100.0,
        width: 100.0,
        height: 50.0,
        ratio: 2.0,
    };

    fn glowing(points: &[f32], frame: &[u8], level: f32) -> Vec<u16> {
        let mut rgb = vec![0u16; 200 * 100 * 3];
        if let Some(halos) = Halos::new(&[prop(points)], &VIEW, (200, 100), 4.0, level) {
            halos.add(frame, &mut Sums::default(), &mut rgb);
        }
        rgb
    }

    #[test]
    fn a_halo_is_a_bell_in_its_pixels_color_centered_on_it() {
        // A dot 4 screen pixels in radius is 8 picture pixels: at half glow its bell is 14.4 wide
        // and 0.35 strong at its middle (a little less, in a picture as coarse as it's added up
        // in).
        let rgb = glowing(&[0.0, 0.0], &[200, 100, 0], 0.5);
        let at = |x: usize, y: usize| [0, 1, 2].map(|c| rgb[(y * 200 + x) * 3 + c]);
        let middle = at(100, 50);
        assert!((60..=70).contains(&middle[0]), "{middle:?}");
        assert!((30..=35).contains(&middle[1]) && middle[2] == 0, "{middle:?}");
        // The same all round (to within the eighth of a coarse pixel a halo may be off its dot),
        // fading with distance to nothing.
        for (dx, dy) in [(14, 0), (0, 14), (10, 10)] {
            let ring = [at(100 + dx, 50 + dy), at(99 - dx, 49 - dy)];
            assert!(ring[0][0].abs_diff(ring[1][0]) <= 4, "{dx}, {dy}: {ring:?}");
            assert!((20..=32).contains(&ring[0][0]), "{dx}, {dy}: {ring:?}");
        }
        assert_eq!(at(160, 50), [0, 0, 0]);
        assert_eq!(at(100, 95), [0, 0, 0]);
        // A pixel that's off has none, and no glow draws none.
        assert!(glowing(&[0.0, 0.0], &[0, 0, 0], 0.5).iter().all(|c| *c == 0));
        assert!(glowing(&[0.0, 0.0], &[200, 100, 0], 0.0).iter().all(|c| *c == 0));
    }

    #[test]
    fn halos_add_up_and_stop_at_full_brightness() {
        let one = glowing(&[0.0, 0.0], &[100, 0, 0], 1.0);
        let pair = glowing(&[-0.02, 0.0, 0.02, 0.0], &[100, 0, 0, 100, 0, 0], 1.0);
        let middle = (50 * 200 + 100) * 3;
        assert!(
            pair[middle] > one[middle] + 20,
            "{} against {}",
            pair[middle],
            one[middle]
        );
        let pile: Vec<f32> = std::iter::repeat_n([0.0, 0.0], 40).flatten().collect();
        let bright = glowing(&pile, &[255; 120], 1.0);
        assert_eq!(bright[middle..middle + 3], [255, 255, 255]);
    }

    #[test]
    fn a_frame_leaves_nothing_behind_for_the_next() {
        let halos = Halos::new(&[prop(&[0.0, 0.0, 0.3, 0.1])], &VIEW, (200, 100), 4.0, 0.5).unwrap();
        let mut scratch = Sums::default();
        let draw = |frame: &[u8], scratch: &mut Sums| {
            let mut rgb = vec![0u16; 200 * 100 * 3];
            halos.add(frame, scratch, &mut rgb);
            rgb
        };
        let first = draw(&[0, 0, 0, 9, 200, 30], &mut scratch);
        draw(&[255, 255, 255, 0, 0, 0], &mut scratch);
        assert_eq!(draw(&[0, 0, 0, 9, 200, 30], &mut scratch), first);
    }

    #[test]
    fn halos_too_small_to_see_are_not_drawn() {
        // A dot a fifth of a picture pixel in radius: its bell is under a pixel wide.
        assert!(Halos::new(&[prop(&[0.0, 0.0])], &VIEW, (200, 100), 0.1, 1.0).is_none());
        assert!(Halos::new(&[prop(&[0.0, 0.0])], &VIEW, (200, 100), 4.0, 0.0).is_none());
        assert!(Halos::new(&[prop(&[0.0, 0.0])], &VIEW, (200, 100), 4.0, f32::NAN).is_none());
    }
}
