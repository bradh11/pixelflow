//! The show's pixels as crisp dots, drawn the way the 2D previews draw them.
//!
//! This is `stampPixels` of the window's `app/src/lib/pixelBatches.ts`, which says why the look is
//! what it is: the same steps in the same order with the same rounding, so a video frame and the
//! preview of the same moment are the same picture. A dot is sized from how far apart its prop's
//! pixels are in the picture; pixels too close along the wire for dots of their own run together,
//! each place showing the pixel nearest to it; pixels closer than a screen pixel are merged into
//! one light first; lit dots go over unlit ones; and a light too faint to see lets the dot of a
//! pixel that's off show through.
//!
//! The scenes in `app/src/api/contracts/pixels/` hold the two to each other: the window's tests
//! write the picture the preview draws of each, and the tests here fail unless this draws the
//! same.
//!
//! A video's view never changes, so everything that doesn't depend on the lights is worked out
//! once ([`Dots::new`]): how each prop is drawn, where each pixel's dot lands, and which pixels
//! merge. A frame ([`Dots::stamp`]) then only looks up colors and stamps.

use pf_engine::PreviewProp;
use std::ops::Range;

/// A pixel's dot in the layout: its radius, in layout units.
pub(crate) const DOT_RADIUS: f64 = 0.05;
/// How much of the way to its neighbor a dot may reach: the rest is the gap between them.
const FILL: f64 = 0.7;
/// The smallest dot, in screen pixels: smaller, a light is too thin to see what color it is.
const SMALLEST: f64 = 2.0;
/// Dots at least this wide (picture pixels) are placed to the nearest of `PLACES` places across a
/// picture pixel, so a grid of them comes out even. Smaller ones sit on whole pixels.
const FINE_FROM: i32 = 4;
const PLACES: i32 = 4;
/// A light dimmer than this (its brightest channel, of 255) lets the dot of a pixel that's off
/// show through, the more the dimmer it is.
const FAINT: f64 = 64.0;
/// How many of a prop's pixels are measured to find how far apart they are.
const SAMPLES: usize = 256;
/// How far to a pixel's side is looked at for other pixels, in layout units. The squares pixels
/// are sorted into are no smaller than a `REACH_SQUARES`th of it.
const REACH: f64 = 10.0 * DOT_RADIUS;
const REACH_SQUARES: i64 = 5;
/// A pixel is to another's side, not along its line, when the way to it is more than this off the
/// line to that pixel's nearest neighbor (as a cosine).
const SIDEWAYS: f64 = 0.8;

/// Rounds as the window's JavaScript does: a half goes up, also below zero (where `f64::round`
/// goes down).
pub(crate) fn round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}

/// What a picture shows of the layout, as a preview's canvas does: the layout point at its
/// middle, screen pixels to a layout unit, the canvas's size in screen pixels, and picture pixels
/// to a screen pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
    pub cx: f64,
    pub cy: f64,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
    pub ratio: f64,
}

impl View {
    /// Where a layout point is in the picture, in picture pixels (y down).
    pub(crate) fn place(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (self.width / 2.0 + (x - self.cx) * self.zoom) * self.ratio,
            (self.height / 2.0 - (y - self.cy) * self.zoom) * self.ratio,
        )
    }
}

/// How a prop's pixels are spread out, in layout units: how far apart they are (the distance from
/// a pixel to its nearest neighbor that a quarter of them are within), how much room they have to
/// their side (to the nearest pixel that isn't along the line to that neighbor), and whether they
/// have neighbors on every side, as a matrix's do and a string's don't.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spread {
    pub spacing: f64,
    pub across: f64,
    pub all_round: bool,
}

/// A prop with no two pixels apart.
const ALONE: Spread = Spread {
    spacing: f64::INFINITY,
    across: f64::INFINITY,
    all_round: false,
};

/// Measures how a prop's pixels (x, y pairs in wiring order) are spread out, from a sample of
/// them.
pub(crate) fn measure_spread(points: &[f32]) -> Spread {
    let n = points.len() / 2;
    if n < 2 {
        return ALONE;
    }
    let x = |i: usize| f64::from(points[2 * i]);
    let y = |i: usize| f64::from(points[2 * i + 1]);
    let picks = n.min(SAMPLES);
    let pick = |k: usize| (((k as f64 + 0.5) * n as f64) / picks as f64).floor() as usize;
    let between = |i: usize, j: usize| {
        let (dx, dy) = (x(i) - x(j), y(i) - y(j));
        (dx * dx + dy * dy).sqrt()
    };
    // From a pixel to the nearer of the pixels wired before and after it that aren't on its spot.
    let to_next = |i: usize| {
        let before = if i > 0 { between(i, i - 1) } else { 0.0 };
        let after = if i + 1 < n { between(i, i + 1) } else { 0.0 };
        if before > 0.0 && after > 0.0 {
            before.min(after)
        } else if before > 0.0 {
            before
        } else {
            after
        }
    };

    // Along the wiring, a pixel's nearest neighbor is no farther than the next pixel.
    let mut along: Vec<f64> = (0..picks)
        .map(|k| to_next(pick(k)))
        .filter(|d| *d > 0.0)
        .collect();
    if along.is_empty() {
        return ALONE;
    }
    along.sort_by(f64::total_cmp);
    let step = along[along.len() >> 1];

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for i in 0..n {
        min_x = min_x.min(x(i));
        max_x = max_x.max(x(i));
        min_y = min_y.min(y(i));
        max_y = max_y.max(y(i));
    }
    let (w, h) = (max_x - min_x, max_y - min_y);
    // Every pixel goes in a grid of squares, so a pixel's neighbors are found in the squares
    // around it. A square is as wide as the step along the wiring, unless the wiring jumps about
    // (then a few pixels wide, going by how many pixels share the prop's box), and wide enough
    // that a few squares each way reach as far as a pixel's side is looked at.
    let crowd = ((w * h) / n as f64).sqrt().max(w.max(h) / n as f64);
    let cell = (REACH / REACH_SQUARES as f64).max(step.min(4.0 * crowd));
    let mut buckets = 1usize;
    while buckets < 2 * n {
        buckets <<= 1;
    }
    let bucket_of = |column: i64, row: i64| {
        let hash = (column as i32).wrapping_mul(73_856_093) ^ (row as i32).wrapping_mul(19_349_663);
        hash as u32 as usize & (buckets - 1)
    };
    let column = |i: usize| ((x(i) - min_x) / cell).floor() as i64;
    let row = |i: usize| ((y(i) - min_y) / cell).floor() as i64;
    let mut starts = vec![0u32; buckets + 1];
    for i in 0..n {
        starts[bucket_of(column(i), row(i)) + 1] += 1;
    }
    for b in 0..buckets {
        starts[b + 1] += starts[b];
    }
    let mut filled = starts[..buckets].to_vec();
    let mut in_bucket = vec![0u32; n];
    for i in 0..n {
        let b = bucket_of(column(i), row(i));
        in_bucket[filled[b] as usize] = i as u32;
        filled[b] += 1;
    }

    let same = step * 1e-3;
    // Calls `each` with every other pixel in the squares `ring` squares out from pixel `i`'s own
    // (its own for 0), and how far from it each is.
    let around = |i: usize, ring: i64, each: &mut dyn FnMut(f64, usize)| {
        let (cx, cy) = (column(i), row(i));
        for dy in -ring..=ring {
            let edge = dy == -ring || dy == ring;
            let stride = if edge || ring == 0 { 1 } else { 2 * ring };
            let mut dx = -ring;
            while dx <= ring {
                let b = bucket_of(cx + dx, cy + dy);
                for at in starts[b]..starts[b + 1] {
                    let j = in_bucket[at as usize] as usize;
                    // Other squares can share the bucket.
                    if column(j) != cx + dx || row(j) != cy + dy {
                        continue;
                    }
                    let d = between(i, j);
                    if d > same {
                        each(d, j);
                    }
                }
                dx += stride;
            }
        }
    };

    let mut nearest = Vec::with_capacity(picks);
    let mut sideways = Vec::with_capacity(picks);
    for k in 0..picks {
        let i = pick(k);
        let (mut best, mut toward) = (f64::INFINITY, None);
        for ring in 0..=1 {
            around(i, ring, &mut |d, j| {
                if d < best {
                    (best, toward) = (d, Some(j));
                }
            });
        }
        let Some(toward) = toward else {
            if to_next(i) > 0.0 {
                nearest.push(to_next(i));
            }
            continue;
        };
        nearest.push(best);
        // The nearest pixel that isn't along the line to that one, looking a ring of squares
        // farther out until nothing nearer can be left to find.
        let (ux, uy) = ((x(toward) - x(i)) / best, (y(toward) - y(i)) / best);
        let mut side = f64::INFINITY;
        let mut ring = 0;
        while ring <= REACH_SQUARES && side > (ring - 1) as f64 * cell {
            around(i, ring, &mut |d, j| {
                if d >= side {
                    return;
                }
                let lined = ((x(j) - x(i)) * ux + (y(j) - y(i)) * uy).abs() / d;
                if lined < SIDEWAYS {
                    side = d;
                }
            });
            ring += 1;
        }
        if side <= REACH {
            sideways.push(side);
        }
    }
    if nearest.is_empty() {
        return ALONE;
    }
    nearest.sort_by(f64::total_cmp);
    let spacing = nearest[nearest.len() >> 2];
    sideways.sort_by(f64::total_cmp);
    // A quarter of the pixels looked at, not of those with something to their side.
    let across = if sideways.len() > picks >> 2 {
        sideways[picks >> 2]
    } else {
        f64::INFINITY
    };

    // Far enough to find every pixel within one and a half times the spacing.
    let reach = ((spacing * 1.5) / cell).ceil().min(3.0) as i64;
    let mut surrounded = 0;
    for k in 0..picks {
        let mut near = 0;
        for ring in 0..=reach {
            around(pick(k), ring, &mut |d, _| {
                if d <= spacing * 1.5 {
                    near += 1;
                }
            });
        }
        if near >= 4 {
            surrounded += 1;
        }
    }
    Spread {
        spacing,
        across,
        all_round: surrounded * 2 > picks,
    }
}

/// How much room a dot `size` picture pixels wide takes up: one placed finely spreads its soft
/// edge over one more.
fn footprint(size: f64) -> f64 {
    if size >= f64::from(FINE_FROM) {
        size + 1.0
    } else {
        size
    }
}

/// How wide a dot is, in picture pixels, for pixels `pitch` picture pixels apart: most of the way
/// to its neighbor, always leaving a pixel clear between them, but no smaller than `smallest` and
/// no larger than `largest`.
fn dot_size(pitch: f64, smallest: f64, largest: f64) -> f64 {
    // A hair more room than there is, so pixels a whole number of picture pixels apart don't
    // lose one to rounding.
    let room = pitch - 1.0 + 1e-6;
    let mut size = largest.min(room.floor()).min(round(pitch * FILL));
    if footprint(size) > room {
        size -= 1.0;
    }
    smallest.max(size)
}

/// A dot ready to stamp: the square of picture pixels it's stamped in (`side` each way) and, for
/// each of them row by row, how much of it the dot covers (0–255; none when it covers them all)
/// and how far it is from the dot's middle (1 for the nearest, up to 255 the farther).
#[derive(Debug, Clone)]
struct Shape {
    side: i32,
    cover: Option<Box<[u8]>>,
    apart: Box<[u8]>,
}

/// The kinds of shape a dot of one size comes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    /// Round, sitting on whole picture pixels.
    Whole,
    /// Round, one shape for each place it can sit across a picture pixel (row by row).
    Fine,
    /// Its whole square.
    Square,
}

/// The shapes of a round dot `size` picture pixels wide: solid in the middle with a soft edge
/// (under 3 wide it's the whole square: too small to round). One shape for a dot sitting on whole
/// pixels, or with `fine` one for each place it can sit across a pixel, its soft edge following.
fn round_shapes(size: i32, fine: bool) -> Vec<Shape> {
    let side = if fine { size + 1 } else { size };
    let places = if fine { PLACES * PLACES } else { 1 };
    let half = f64::from(size) / 2.0;
    (0..places)
        .map(|place| {
            let cx = f64::from(place % PLACES) / f64::from(PLACES) + half;
            let cy = f64::from(place / PLACES) / f64::from(PLACES) + half;
            let mut cover = Vec::new();
            let mut apart = Vec::with_capacity((side * side) as usize);
            for y in 0..side {
                for x in 0..side {
                    let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
                    let away = (dx * dx + dy * dy).sqrt();
                    apart.push((1.0 + round(away * away * 16.0)).min(255.0) as u8);
                    if size >= 3 {
                        cover.push(round(255.0 * (half + 0.5 - away).clamp(0.0, 1.0)) as u8);
                    }
                }
            }
            Shape {
                side,
                cover: (size >= 3).then(|| cover.into()),
                apart: apart.into(),
            }
        })
        .collect()
}

/// A color as a picture pixel: how solid it is, then blue, green and red, highest byte first.
fn pack(red: u32, green: u32, blue: u32, solid: u32) -> u32 {
    (solid << 24) | (blue << 16) | (green << 8) | red
}

/// Puts a color (red, green and blue as one number, and how solid it is of 255) over a picture
/// pixel.
fn blend(pixel: &mut u32, rgb: u32, alpha: u32) {
    let under = *pixel;
    let under_alpha = under >> 24;
    if under_alpha == 0 || alpha == 255 {
        *pixel = (alpha << 24) | rgb;
        return;
    }
    // Both colors weighted by how much of each shows, out of 255 × 255.
    let top = alpha * 255;
    let rest = under_alpha * (255 - alpha);
    let all = top + rest;
    let half = all >> 1;
    let mix = |shift: u32| (((rgb >> shift) & 255) * top + ((under >> shift) & 255) * rest + half) / all;
    *pixel = pack(mix(0), mix(8), mix(16), (all + 127) / 255);
}

/// The picture pixels of a dot's square that are in the picture: columns `left..right` of rows
/// `top..bottom`.
fn clip(x0: i32, y0: i32, side: i32, width: usize, height: usize) -> (i32, i32, i32, i32) {
    (
        x0.max(0),
        x0.saturating_add(side).min(width as i32),
        y0.max(0),
        y0.saturating_add(side).min(height as i32),
    )
}

/// A box of picture pixels: `x0`, `y0` inside it, `x1`, `y1` just past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

/// The box around the squares dots are stamped in, before it's cut to the picture.
#[derive(Debug, Clone, Copy)]
struct Reach {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl Reach {
    const NOWHERE: Reach = Reach {
        x0: i32::MAX,
        y0: i32::MAX,
        x1: i32::MIN,
        y1: i32::MIN,
    };

    fn grow(&mut self, x0: i32, y0: i32, side: i32) {
        self.x0 = self.x0.min(x0);
        self.y0 = self.y0.min(y0);
        self.x1 = self.x1.max(x0.saturating_add(side));
        self.y1 = self.y1.max(y0.saturating_add(side));
    }

    /// The box around this one and another.
    fn with(self, other: Reach) -> Reach {
        Reach {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// The part of it in a picture, when it has any.
    fn within(self, width: usize, height: usize) -> Option<PixelBox> {
        let clamp = |v: i32, most: usize| (v.max(0) as usize).min(most);
        let cut = PixelBox {
            x0: clamp(self.x0, width),
            y0: clamp(self.y0, height),
            x1: clamp(self.x1, width),
            y1: clamp(self.y1, height),
        };
        (cut.x0 < cut.x1 && cut.y0 < cut.y1).then_some(cut)
    }
}

/// How a prop's dots are drawn: standing apart, run together (each place showing the nearest
/// pixel), or merged into fewer lights first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Way {
    Apart,
    Crowded,
    Merged,
}

/// One prop's dots: its marks, how bright its lit dots are drawn (less than 1 when a glow adds
/// the rest back), and for a crowded prop its tag (see [`Layer::claim_dot`]).
#[derive(Debug, Clone)]
struct Drawn {
    way: Way,
    marks: Range<usize>,
    dim: f64,
    tag: u16,
}

/// One pixel's dot: the top left of its square in the picture, which shape is stamped there, and
/// where its color is in a show frame.
#[derive(Debug, Clone, Copy)]
struct Mark {
    x: i32,
    y: i32,
    shape: u32,
    at: usize,
}

/// One light merged from the pixels that share a screen pixel: where and how it's stamped, and
/// the pixels it's made of.
#[derive(Debug, Clone)]
struct Light {
    x: i32,
    y: i32,
    shape: u32,
    members: Range<usize>,
}

/// A pixel that's part of a merged light: where its color is in a show frame, and how bright it's
/// drawn.
#[derive(Debug, Clone, Copy)]
struct Member {
    at: usize,
    dim: f64,
}

/// The picture the dots are stamped into, kept between frames: for each picture pixel a color
/// (see [`pack`]), and for the dots that run together, whose dot is shown there and how near it is
/// (see [`Layer::claim_dot`]; 0 for none, as all are between frames).
#[derive(Debug, Clone, Default)]
pub(crate) struct Layer {
    pub pixels: Vec<u32>,
    nearest: Vec<u16>,
    width: usize,
    height: usize,
    /// Where the last frame's dots are.
    drawn: Option<PixelBox>,
}

impl Layer {
    /// Makes it a picture of this size, empty where the last frame drew.
    fn ready(&mut self, width: usize, height: usize, crowded: bool) {
        if (self.width, self.height) != (width, height) {
            *self = Layer {
                pixels: vec![0; width * height],
                width,
                height,
                ..Layer::default()
            };
        }
        if crowded && self.nearest.is_empty() {
            self.nearest = vec![0; width * height];
        }
        if let Some(old) = self.drawn.take() {
            for y in old.y0..old.y1 {
                self.pixels[y * width..][old.x0..old.x1].fill(0);
            }
        }
    }

    /// Stamps one dot with the top left of its square at `x0`, `y0`. A dot stamped `over` covers
    /// what's there; otherwise it's one of the dots under the lit ones, where two overlapping
    /// never add up to more than either alone.
    fn stamp_dot(&mut self, x0: i32, y0: i32, shape: &Shape, color: u32, over: bool) {
        let side = shape.side;
        let (left, right, top, bottom) = clip(x0, y0, side, self.width, self.height);
        if left >= right {
            return;
        }
        let (rgb, alpha) = (color & 0x00ff_ffff, color >> 24);
        for y in top..bottom {
            let row = &mut self.pixels[y as usize * self.width..][left as usize..right as usize];
            let Some(cover) = &shape.cover else {
                row.fill(color);
                continue;
            };
            let covers = &cover[((y - y0) * side + left - x0) as usize..];
            for (pixel, &covered) in row.iter_mut().zip(covers) {
                let covered = u32::from(covered);
                if covered == 255 {
                    *pixel = color;
                } else if covered > 0 {
                    let solid = (covered * alpha + 127) / 255;
                    if over {
                        blend(pixel, rgb, solid);
                    } else if solid > *pixel >> 24 {
                        *pixel = (solid << 24) | rgb;
                    }
                }
            }
        }
    }

    /// Stamps one dot of a crowded prop: only where no dot stamped so far is nearer. What's kept
    /// for each picture pixel is the `tag` of the dot shown there times 256, plus how far the
    /// pixel is from that dot's middle (see [`Shape`]). A tag is a prop's number (up to 127) times
    /// two, plus one for a lit pixel's dot. Between two props, a lit pixel's dot stays over one
    /// that isn't, whichever is nearer.
    fn claim_dot(&mut self, x0: i32, y0: i32, shape: &Shape, color: u32, tag: u16) {
        let side = shape.side;
        let (left, right, top, bottom) = clip(x0, y0, side, self.width, self.height);
        if left >= right {
            return;
        }
        let (rgb, alpha) = (color & 0x00ff_ffff, color >> 24);
        let mine = tag << 8;
        for y in top..bottom {
            let from = y as usize * self.width;
            let row = &mut self.pixels[from..][left as usize..right as usize];
            let shown_row = &mut self.nearest[from..][left as usize..right as usize];
            let k = ((y - y0) * side + left - x0) as usize;
            for (i, (pixel, shown)) in row.iter_mut().zip(shown_row).enumerate() {
                let covered = shape.cover.as_ref().map_or(255, |cover| u32::from(cover[k + i]));
                if covered == 0 {
                    continue;
                }
                let apart = u16::from(shape.apart[k + i]);
                if *shown != 0 {
                    let other = (*shown >> 8) ^ tag;
                    if other > 1 && other & 1 != 0 {
                        if tag & 1 == 0 {
                            continue;
                        }
                        // Over another prop's unlit dot, a soft edge lets it show through.
                        if covered < 255 {
                            *shown = mine | apart;
                            blend(pixel, rgb, (covered * alpha + 127) / 255);
                            continue;
                        }
                    } else if *shown & 255 <= apart {
                        continue;
                    }
                }
                *shown = mine | apart;
                *pixel = if covered == 255 {
                    color
                } else {
                    (((covered * alpha + 127) / 255) << 24) | rgb
                };
            }
        }
    }
}

/// Every pixel's dot for one view of a layout, ready to stamp frame after frame.
#[derive(Debug, Clone)]
pub(crate) struct Dots {
    width: usize,
    height: usize,
    /// The dot of a pixel that's off: its color as it is, how solid it is, and its red, green and
    /// blue as light (each times how solid the color is).
    dark: u32,
    dark_alpha: f64,
    dark_light: [f64; 3],
    shapes: Vec<Shape>,
    props: Vec<Drawn>,
    marks: Vec<Mark>,
    lights: Vec<Light>,
    members: Vec<Member>,
    /// Where dots that run together are stamped, and where any are.
    crowded: Option<PixelBox>,
    drawn: Option<PixelBox>,
    /// DMX floods' cells, each washing its color round it under every dot, as far as
    /// `wash_reach` (picture pixels).
    washes: Vec<crate::flood::Wash>,
    wash_reach: f64,
    /// Moving heads and servos: a beam or arms each, under every dot, after the washes.
    movers: Vec<Mover>,
    view: View,
}

/// A moving head or servos: its cells placed in the picture, how high its first cell is above
/// the ground (layout units), where its color is in a show frame, and how it moves (see
/// `beam.rs`).
#[derive(Debug, Clone)]
struct Mover {
    cells: Vec<(f64, f64)>,
    head_y: f64,
    offset: usize,
    motion: pf_engine::PreviewMotion,
}

impl Dots {
    /// The dots of `props` in `view`, in a picture `width` by `height` pixels. `radius` (screen
    /// pixels) is the largest a dot is drawn, `dark` the color of a pixel that's off (red, green,
    /// blue, and how solid it is), and `dim` how bright lit dots of a width (picture pixels) are
    /// drawn (1 unless a glow adds the rest back).
    pub(crate) fn new(
        props: &[PreviewProp],
        view: &View,
        (width, height): (usize, usize),
        radius: f64,
        dark: [u8; 4],
        dim: impl Fn(f64) -> f64,
    ) -> Self {
        let ratio = view.ratio;
        let screen_pixel = round(ratio).max(1.0);
        let largest = round(2.0 * radius * ratio).max(1.0);
        let smallest = largest.min(round(SMALLEST * ratio).max(1.0));
        // A line of pixels run together is this wide at most: one as wide as the view's whole
        // dot would weigh more than a row of those dots standing apart.
        let widest_line = smallest.max(round(largest * FILL));
        let (w, h) = (width as f64, height as f64);
        let dark_alpha = f64::from(dark[3]);
        let mut dots = Dots {
            width,
            height,
            dark: pack(dark[0].into(), dark[1].into(), dark[2].into(), dark[3].into()),
            dark_alpha,
            dark_light: [0, 1, 2].map(|c| (f64::from(dark[c]) * dark_alpha) / 255.0),
            shapes: Vec::new(),
            props: Vec::with_capacity(props.len()),
            marks: Vec::new(),
            lights: Vec::new(),
            members: Vec::new(),
            crowded: None,
            drawn: None,
            washes: crate::flood::washes(props, view),
            wash_reach: crate::flood::wash_reach(view),
            movers: props
                .iter()
                .filter_map(|p| {
                    let motion = p.motion.clone()?;
                    let cells = p
                        .points
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|xy| view.place(f64::from(xy[0]), f64::from(xy[1])))
                        .collect();
                    Some(Mover {
                        cells,
                        head_y: p.points.get(1).copied().map_or(0.0, f64::from),
                        offset: p.frame_offset,
                        motion,
                    })
                })
                .collect(),
            view: *view,
        };
        let mut washed = Reach::NOWHERE;
        for wash in &dots.washes {
            let (x0, x1, y0, y1) = crate::flood::wash_box(wash.x, wash.y, dots.wash_reach, width, height);
            if x0 < x1 && y0 < y1 {
                washed = washed.with(Reach {
                    x0: x0 as i32,
                    y0: y0 as i32,
                    x1: x1 as i32,
                    y1: y1 as i32,
                });
            }
        }
        let mut cut: Vec<(i32, Cut, u32)> = Vec::new();
        let mut shapes_of = |dots: &mut Dots, size: i32, kind: Cut| -> u32 {
            if let Some(found) = cut.iter().find(|(s, k, _)| (*s, *k) == (size, kind)) {
                return found.2;
            }
            let first = dots.shapes.len() as u32;
            match kind {
                Cut::Whole => dots.shapes.extend(round_shapes(size, false)),
                Cut::Fine => dots.shapes.extend(round_shapes(size, true)),
                Cut::Square => dots.shapes.extend(
                    round_shapes(size, false)
                        .into_iter()
                        .map(|shape| Shape { cover: None, ..shape }),
                ),
            }
            cut.push((size, kind, first));
            first
        };

        // Screen pixels across the picture, and the merged lights being gathered: which is in
        // each screen pixel (from 1; 0 for none), and for each the sums of where its pixels are,
        // the widest dot any of them is drawn with, and whether any is of a prop whose pixels
        // have neighbors on every side.
        let across = (w / screen_pixel).ceil() as usize;
        let down = (h / screen_pixel).ceil() as usize;
        let mut light_at: Vec<u32> = Vec::new();
        let mut gathered: Vec<(f64, f64, f64, bool)> = Vec::new();
        let mut joined: Vec<(u32, Member)> = Vec::new();
        let (mut crowded, mut apart) = (Reach::NOWHERE, Reach::NOWHERE);

        for (n, prop) in props.iter().enumerate() {
            let spread = measure_spread(&prop.points);
            // A hair more than it is, as in `dot_size`.
            let along = spread.spacing * view.zoom * ratio + 1e-6;
            // Nothing found to a pixel's side means nothing within `REACH`: that much room is
            // sure. A prop's dots are as big as its pixels' room along the wire allows, but no
            // thinner than the line they make where they run together.
            let line =
                dot_size(spread.across.min(REACH) * view.zoom * ratio, smallest, largest).min(widest_line);
            let dot = dot_size(along, smallest, largest).max(line);
            let way = if along < screen_pixel {
                Way::Merged
            } else if along < footprint(dot) + 1.0 {
                Way::Crowded
            } else {
                Way::Apart
            };
            let dim = dim(dot);
            let first = dots.marks.len();
            let pixels = prop.points.as_chunks::<2>().0.iter().enumerate();
            let at = |i: usize| prop.frame_offset + i * usize::from(prop.channels_per_pixel);
            if way == Way::Merged {
                // Pixels in the same screen pixel become one light, in the middle of them.
                if light_at.is_empty() {
                    light_at = vec![0; across * down];
                }
                for (i, p) in pixels {
                    let (x, y) = view.place(p[0].into(), p[1].into());
                    if x <= -dot || y <= -dot || x >= w + dot || y >= h + dot {
                        continue;
                    }
                    // A pixel just off the picture still reaches into it: it's a light of its own.
                    let inside = x >= 0.0 && y >= 0.0 && x < w && y < h;
                    let cell = inside.then(|| {
                        (y / screen_pixel).floor() as usize * across + (x / screen_pixel).floor() as usize
                    });
                    let light = match cell.map(|cell| light_at[cell]) {
                        Some(light) if light != 0 => light - 1,
                        _ => {
                            gathered.push((0.0, 0.0, 0.0, false));
                            if let Some(cell) = cell {
                                light_at[cell] = gathered.len() as u32;
                            }
                            gathered.len() as u32 - 1
                        }
                    };
                    let sums = &mut gathered[light as usize];
                    sums.0 += x;
                    sums.1 += y;
                    sums.2 = sums.2.max(dot);
                    sums.3 |= spread.all_round;
                    joined.push((light, Member { at: at(i), dim }));
                }
            } else {
                // Where a crowded prop's pixels have neighbors on every side and its dots overlap,
                // each is its whole square: round ones would leave the corners between four of
                // them half covered.
                let square =
                    way == Way::Crowded && spread.all_round && spread.spacing * view.zoom * ratio <= dot;
                let fine = !square && dot >= f64::from(FINE_FROM);
                let kind = if square {
                    Cut::Square
                } else if fine {
                    Cut::Fine
                } else {
                    Cut::Whole
                };
                let shapes = shapes_of(&mut dots, dot as i32, kind);
                let side = dots.shapes[shapes as usize].side;
                let half = dot / 2.0;
                let reach = if way == Way::Crowded {
                    &mut crowded
                } else {
                    &mut apart
                };
                for (i, p) in pixels {
                    let (x, y) = view.place(p[0].into(), p[1].into());
                    let (left, top) = (x - half, y - half);
                    // On whole picture pixels, or to the nearest place across one.
                    let (x0, y0, place) = if fine {
                        let (mut x0, mut y0) = (left.floor(), top.floor());
                        let mut over = round((left - x0) * f64::from(PLACES)) as i32;
                        let mut under = round((top - y0) * f64::from(PLACES)) as i32;
                        if over == PLACES {
                            over = 0;
                            x0 += 1.0;
                        }
                        if under == PLACES {
                            under = 0;
                            y0 += 1.0;
                        }
                        (x0 as i32, y0 as i32, (under * PLACES + over) as u32)
                    } else {
                        (round(left) as i32, round(top) as i32, 0)
                    };
                    if x0 >= width as i32 || y0 >= height as i32 || x0 + side <= 0 || y0 + side <= 0 {
                        continue;
                    }
                    reach.grow(x0, y0, side);
                    dots.marks.push(Mark {
                        x: x0,
                        y: y0,
                        shape: shapes + place,
                        at: at(i),
                    });
                }
            }
            dots.props.push(Drawn {
                way,
                marks: first..dots.marks.len(),
                dim,
                // 0 is the merged lights' number.
                tag: (1 + ((n + 1) % 127) as u16) << 1,
            });
        }

        // The merged lights, each with its pixels together in the order they joined it.
        let mut starts = vec![0usize; gathered.len() + 1];
        for (light, _) in &joined {
            starts[*light as usize + 1] += 1;
        }
        for light in 0..gathered.len() {
            starts[light + 1] += starts[light];
        }
        let mut filled = starts.clone();
        dots.members = vec![Member { at: 0, dim: 1.0 }; joined.len()];
        for (light, member) in joined {
            dots.members[filled[light as usize]] = member;
            filled[light as usize] += 1;
        }
        for (light, (x, y, dot, all_round)) in gathered.into_iter().enumerate() {
            let members = starts[light]..starts[light + 1];
            let count = members.len() as f64;
            let x0 = round(x / count - dot / 2.0) as i32;
            let y0 = round(y / count - dot / 2.0) as i32;
            let side = dot as i32;
            if x0 >= width as i32 || y0 >= height as i32 || x0 + side <= 0 || y0 + side <= 0 {
                continue;
            }
            let kind = if all_round { Cut::Square } else { Cut::Whole };
            let shape = shapes_of(&mut dots, side, kind);
            crowded.grow(x0, y0, side);
            dots.lights.push(Light {
                x: x0,
                y: y0,
                shape,
                members,
            });
        }

        dots.crowded = crowded.within(width, height);
        dots.drawn = apart.with(crowded).with(washed).within(width, height);
        // A beam can point anywhere: with any, each frame starts from an empty picture.
        if !dots.movers.is_empty() {
            dots.drawn = Reach {
                x0: 0,
                y0: 0,
                x1: width as i32,
                y1: height as i32,
            }
            .within(width, height);
        }
        dots
    }

    /// The part of the picture the dots are in, when they're in any of it.
    pub(crate) fn bounds(&self) -> Option<PixelBox> {
        self.drawn
    }

    /// The ways the props are drawn, in their order.
    #[cfg(test)]
    pub(crate) fn ways(&self) -> Vec<Way> {
        self.props.iter().map(|prop| prop.way).collect()
    }

    /// A lit pixel's color in the picture: its own, or over the dot of a pixel that's off when
    /// it's faint.
    fn lit(&self, [r, g, b]: [u8; 3], dim: f64) -> u32 {
        let most = r.max(g).max(b);
        if dim >= 1.0 && f64::from(most) >= FAINT {
            return pack(r.into(), g.into(), b.into(), 255);
        }
        let mut light = [f64::from(r), f64::from(g), f64::from(b)];
        if dim < 1.0 {
            light = light.map(|c| round(c * dim));
        }
        let most = f64::from(most);
        if most >= FAINT {
            return pack(light[0] as u32, light[1] as u32, light[2] as u32, 255);
        }
        let through = 1.0 - most / FAINT;
        let solid = most / FAINT + (through * self.dark_alpha) / 255.0;
        let [red, green, blue] =
            [0, 1, 2].map(|c| round((light[c] + through * self.dark_light[c]) / solid).min(255.0) as u32);
        pack(red, green, blue, round(solid * 255.0) as u32)
    }

    /// Stamps the dots into `layer` with their colors in `frame` (a show frame: RGB or RGBW per
    /// pixel; a pixel past its end is off), emptying what the last frame left there first.
    pub(crate) fn stamp(&self, frame: &[u8], layer: &mut Layer) {
        layer.ready(self.width, self.height, self.crowded.is_some());
        layer.drawn = self.drawn;
        let color_at = |at: usize| {
            frame
                .get(at..)
                .and_then(|rest| rest.first_chunk::<3>())
                .copied()
                .filter(|c| *c != [0; 3])
        };

        // Floods' washes go under every dot (see `flood.rs`).
        let reach2 = self.wash_reach * self.wash_reach;
        for wash in &self.washes {
            let Some([r, g, b]) = color_at(wash.offset) else {
                continue;
            };
            let rgb = pack(r.into(), g.into(), b.into(), 0);
            let (x0, x1, y0, y1) =
                crate::flood::wash_box(wash.x, wash.y, self.wash_reach, self.width, self.height);
            for py in y0..y1 {
                let dy = py as f64 + 0.5 - wash.y;
                for px in x0..x1 {
                    let dx = px as f64 + 0.5 - wash.x;
                    let level =
                        crate::flood::FLOOD_STRENGTH * crate::flood::wash_at((dx * dx + dy * dy) / reach2);
                    let alpha = (level * 255.0 + 0.5).floor() as u32;
                    if alpha > 0 {
                        blend(&mut layer.pixels[py * self.width + px], rgb, alpha);
                    }
                }
            }
        }

        // Then heads' beams and servos' arms, also under every dot (see `beam.rs`).
        for mover in &self.movers {
            let Some(&(ax, ay)) = mover.cells.first() else {
                continue;
            };
            if let Some(beam) = &mover.motion.beam
                && let Some(([r, g, b], level)) = color_at(mover.offset).and_then(crate::beam::beam_light)
            {
                let direction = crate::beam::head_direction(&mover.motion, frame);
                let shape = crate::beam::beam_shape(
                    (ax, ay),
                    mover.head_y,
                    direction,
                    beam.length,
                    beam.width(Some(frame)),
                    &self.view,
                );
                let rgb = pack(r, g, b, 0);
                let (x0, x1, y0, y1) = shape.bounds(self.width, self.height);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let alpha = (level * shape.at(px, py) * 255.0 + 0.5).floor() as u32;
                        if alpha > 0 {
                            blend(&mut layer.pixels[py * self.width + px], rgb, alpha);
                        }
                    }
                }
            }
            let [r, g, b] = crate::beam::ARM_COLOR;
            let rgb = pack(r.into(), g.into(), b.into(), 0);
            for k in 0..mover.motion.servos.len() {
                let (ax, ay) = mover.cells[k.min(mover.cells.len() - 1)];
                let direction = crate::beam::servo_direction(&mover.motion, k, frame);
                let arm = crate::beam::arm_shape(ax, ay, direction, &self.view);
                let (x0, x1, y0, y1) = arm.bounds(self.width, self.height);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let alpha = (arm.at(px, py) * 255.0 + 0.5).floor() as u32;
                        if alpha > 0 {
                            blend(&mut layer.pixels[py * self.width + px], rgb, alpha);
                        }
                    }
                }
            }
        }

        // Crowded props go under everything else: what's stamped over them covers them by the
        // usual rules. The merged lights are stamped first, each on whole picture pixels: together they
        // count as one prop.
        if let Some(crowded) = self.crowded {
            for light in &self.lights {
                let members = &self.members[light.members.clone()];
                let count = members.len() as f64;
                let (mut sums, mut shining, mut any) = ([0.0f64; 3], 0.0f64, false);
                for member in members {
                    if let Some(color) = color_at(member.at) {
                        for c in 0..3 {
                            sums[c] += f64::from(color[c]) * member.dim;
                        }
                        shining += (f64::from(color[0].max(color[1]).max(color[2])) / FAINT).min(1.0);
                        any = true;
                    }
                }
                // What isn't light is the dot of a pixel that's off, showing through.
                let through = count - shining;
                let solid = (shining + (through * self.dark_alpha) / 255.0) / count;
                let color = if solid > 0.0 {
                    let share = 1.0 / (count * solid);
                    let [red, green, blue] = [0, 1, 2]
                        .map(|c| round((sums[c] + through * self.dark_light[c]) * share).min(255.0) as u32);
                    pack(red, green, blue, round(solid * 255.0) as u32)
                } else {
                    0
                };
                let shape = &self.shapes[light.shape as usize];
                layer.claim_dot(light.x, light.y, shape, color, any.into());
            }
            for prop in self.props.iter().filter(|prop| prop.way == Way::Crowded) {
                for mark in &self.marks[prop.marks.clone()] {
                    let (color, tag) = match color_at(mark.at) {
                        Some(color) => (self.lit(color, prop.dim), prop.tag | 1),
                        None => (self.dark, prop.tag),
                    };
                    let shape = &self.shapes[mark.shape as usize];
                    layer.claim_dot(mark.x, mark.y, shape, color, tag);
                }
            }
            for y in crowded.y0..crowded.y1 {
                layer.nearest[y * self.width..][crowded.x0..crowded.x1].fill(0);
            }
        }
        // Then the dots that stand apart: those of pixels that are off (unless they have no color),
        // and the lit ones over them.
        for over in [false, true] {
            if !over && self.dark == 0 {
                continue;
            }
            for prop in self.props.iter().filter(|prop| prop.way == Way::Apart) {
                for mark in &self.marks[prop.marks.clone()] {
                    let color = match (color_at(mark.at), over) {
                        (Some(color), true) => self.lit(color, prop.dim),
                        (None, false) => self.dark,
                        _ => continue,
                    };
                    let shape = &self.shapes[mark.shape as usize];
                    layer.stamp_dot(mark.x, mark.y, shape, color, over);
                }
            }
        }
    }
}
