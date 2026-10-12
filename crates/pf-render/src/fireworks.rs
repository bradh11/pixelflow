//! The Fireworks effect: bursts of sparks that fly out from a point, slow down, fall, and fade.
//!
//! No spark is ever stored, and nothing is carried from one frame to the next. A frame is worked
//! out from the settings, the palette, the effect's seed, the target's size, and the time, in
//! two steps.
//!
//! 1. **Which bursts are alight** (once a frame, in `new`). A burst is alight from the moment it
//!    goes off until its sparks have faded, so only the moments within one glow time of now can
//!    matter, and those are found without looking at any other:
//!    - *At random moments*: time is cut into equal stretches, as many a second as the rate
//!      asks, and one burst goes off in each at a moment drawn from the seed and the stretch's
//!      number. The first goes off as the effect starts, so an effect of any length shows one.
//!    - *On a timing track*: one burst at each mark.
//!    - *With the music*: one on the frame the music rises past the trigger level, and one more
//!      every half second it stays there.
//!
//!    Each burst's place, color, and sparks come from the seed and the stretch, mark, or frame
//!    that set it off, so the same burst is the same on every frame. How far its sparks have
//!    flown, how far they have dropped, and how bright they still are follow from its age.
//!    At most [`MAX_BURSTS`] are drawn at once, the newest.
//!
//! 2. **What a pixel shows** (in `shade`). For each burst alight, a pixel further from the
//!    burst than its fastest spark has flown, or nearer than its slowest, shows nothing of it,
//!    and is done. For the rest, the burst's sparks fly along rays spread evenly round the
//!    circle, one spark to a ray, each at its own place within its ray's share of the circle
//!    and at its own speed. So the pixel's direction from the burst names the share it is in,
//!    and only the sparks of that share and of the shares near enough on either side can light
//!    the pixel: one or two as a rule, never more than seven, for five sparks or two hundred.
//!    Dropping moves a burst's sparks together, so it is the burst's center that drops.
//!
//! A spark is at least one cell across. A soft one is a round dot with an edge one cell soft: one
//! cell wide and in the middle of a cell it lights that pixel alone, and as it moves on its
//! light passes to the next pixel without the sum of it changing by much. A crisp one is a
//! square of whole pixels, those whose middles it covers: one cell wide, it lights exactly one
//! pixel at a time. With tails, a streak runs from the spark back toward the burst's center.
//! Where sparks or bursts overlap, the brightest shows.
//!
//! **On a line** (a target fewer than [`THIN`] cells across) a burst has no room to open: its
//! sparks fly both ways along the line instead, spread evenly by speed, and a pixel looks at
//! the three whose speed would have brought them nearest.
//!
//! The arctangent is a short polynomial and nothing else here is more than arithmetic and a
//! square root, so a frame is the same bits on every machine and every thread.

use crate::audio::RenderContext;
use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade, hash};
use crate::geometry::Pixel;
use pf_sequence::{FireworksColors, FireworksParams, MAX_PALETTE_COLORS};
use std::f32::consts::{FRAC_PI_2, SQRT_2, TAU};

/// The most bursts drawn at once (the newest). Past this many alight together the target is a
/// wash of light whichever are drawn.
pub const MAX_BURSTS: usize = 12;

/// A target fewer cells than this across (or up) is a line: sparks fly along it.
pub const THIN: u32 = 4;

/// The slowest spark's speed, as a share of the fastest's: none stays at the center.
const SLOWEST: f32 = 0.15;

/// How wide a soft spark's edge is, in cells: coverage goes from all to none across it.
const SOFT: f32 = 1.0;

/// With the music, a burst goes off again this often while the music stays past the trigger.
const AGAIN_MS: u64 = 500;

/// The most shares of the circle either side of a pixel's own whose sparks it looks at.
const NEAR: usize = 3;

/// What each random number is for (the second number of the hash).
const WHEN: u64 = 1;
const PLACE: u64 = 2;
const COLOR: u64 = 3;
const SPARKS: u64 = 4;

/// One burst alight this frame.
#[derive(Debug, Clone, Copy)]
struct Burst {
    /// Where its sparks are flying out from, in cells: where it went off, less how far its
    /// sparks have dropped.
    center: [f32; 2],
    /// How far its fastest spark has flown, in cells.
    reach: f32,
    /// A pixel this far from the center or further (squared) shows nothing of it, nor does one
    /// nearer than `inside` (squared).
    outside: f32,
    inside: f32,
    /// How bright its sparks still are.
    level: f32,
    /// Its color, when its sparks share one.
    color: [f32; 3],
    /// What its sparks' own numbers are drawn from.
    seed: u32,
}

impl Burst {
    const NONE: Burst = Burst {
        center: [0.0; 2],
        reach: 0.0,
        outside: 0.0,
        inside: 0.0,
        level: 0.0,
        color: [0.0; 3],
        seed: 0,
    };
}

/// How a burst opens on the target.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Opens {
    /// All round its center.
    Round,
    /// Both ways along a line across the target.
    Across,
    /// Both ways along a line up the target.
    Up,
}

pub struct Fireworks {
    /// What a full step across or up the target is in cells: its columns and rows, less one.
    cells: [f32; 2],
    opens: Opens,
    /// Sparks in a burst.
    sparks: i32,
    /// One spark's share of the circle, in radians.
    sector: f32,
    /// How far from a spark's line a pixel is at the least, for each cell out from the center
    /// and each share of the circle round from it, when it is up to one, two, or three shares
    /// round.
    apart: [f32; NEAR],
    /// Half a spark's width, in cells.
    half: f32,
    /// Whether sparks have soft edges; otherwise they light whole pixels.
    soft: bool,
    /// How far from a spark's middle, or from the line of its tail, anything of it can show: for
    /// a soft spark half its width and half its soft edge, for a crisp one out to its corners
    /// (and that squared).
    edge: f32,
    edge_squared: f32,
    /// A spark's tail, as a share of how far the spark has flown.
    trail: f32,
    /// The palette's colors (red, green, and blue, of 255) when each spark takes one of its
    /// own, and how many there are: none when a burst's sparks share its color.
    each: [[u8; 3]; MAX_PALETTE_COLORS],
    each_count: u8,
    bursts: [Burst; MAX_BURSTS],
    alight: u8,
}

/// A fraction in `0.0..1.0` from sixteen bits of a hash.
#[inline]
fn share(bits: u64) -> f32 {
    (bits & 0xFFFF) as f32 * (1.0 / 65_536.0)
}

/// A pixel's place on one axis, with anything that isn't a number counted as 0.
#[inline]
fn place(at: f32) -> f32 {
    if at.is_finite() { at } else { 0.0 }
}

/// The direction of the point `x` across and `y` up from the origin, as a share of a turn
/// counterclockwise from pointing right (0 to 1; the origin itself points right). The angle
/// within an eighth of a turn is a polynomial (Abramowitz and Stegun, 4.4.47: within 1.2e-5
/// radians), unfolded by which side of the diagonal and of each axis the point is on.
#[inline]
fn turn(x: f32, y: f32) -> f32 {
    const fn turns(c: f32) -> f32 {
        c / TAU
    }
    const C: [f32; 5] = [
        turns(0.999_866),
        turns(-0.330_299_5),
        turns(0.180_141),
        turns(-0.085_133),
        turns(0.020_835_1),
    ];
    let (wide, tall) = (x.abs(), y.abs());
    let steep = tall > wide;
    let (short, long) = if steep { (wide, tall) } else { (tall, wide) };
    if long <= 0.0 {
        return 0.0;
    }
    let t = short / long;
    let z = t * t;
    let mut round = t * (C[0] + z * (C[1] + z * (C[2] + z * (C[3] + z * C[4]))));
    if steep {
        round = 0.25 - round;
    }
    if x < 0.0 {
        round = 0.5 - round;
    }
    if y < 0.0 {
        round = 1.0 - round;
    }
    if round < 1.0 { round } else { 0.0 }
}

/// The sine of an angle of a quarter turn at most either way (`squared` is the angle squared).
#[inline]
fn sine(angle: f32, squared: f32) -> f32 {
    angle * (1.0 + squared * (-1.0 / 6.0 + squared * (1.0 / 120.0 - squared * (1.0 / 5040.0))))
}

/// Its cosine.
#[inline]
fn cosine(squared: f32) -> f32 {
    1.0 + squared * (-0.5 + squared * (1.0 / 24.0 - squared * (1.0 / 720.0)))
}

/// A spark's speed as a share of the fastest's, from a fraction drawn for it: spread so that a
/// burst is as full of sparks near its middle as near its rim.
#[inline]
fn speed(drawn: f32) -> f32 {
    (SLOWEST * SLOWEST + (1.0 - SLOWEST * SLOWEST) * drawn).sqrt()
}

impl Fireworks {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(
        p: &FireworksParams,
        time: &EffectTime,
        colors: Colors,
        seed: u64,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        let (columns, rows) = (canvas.columns.max(1), canvas.rows.max(1));
        let cells = [(columns - 1) as f32, (rows - 1) as f32];
        let opens = if rows < THIN && rows <= columns {
            Opens::Across
        } else if columns < THIN {
            Opens::Up
        } else {
            Opens::Round
        };

        // A setting within its range, whatever it was handed.
        let within = |v: f32, least: f32, most: f32| {
            if v.is_finite() {
                v.clamp(least, most)
            } else {
                least
            }
        };
        // Sizes in cells: a burst size of 100 reaches from the middle of the longer side to its
        // ends, and a spark is a share of that side.
        let longer = columns.max(rows) as f32;
        let (size, spark) = if p.scale {
            (
                within(p.size, 0.0, 500.0) / 100.0 * longer / 2.0,
                within(p.spark_size, 0.0, 25.0) / 100.0 * longer,
            )
        } else {
            (within(p.size, 0.0, 500.0), within(p.spark_size, 0.0, 25.0))
        };
        let half = spark.max(1.0) / 2.0;
        let edge = if p.soft { half + SOFT / 2.0 } else { half * SQRT_2 };
        let trail = within(p.trail, 0.0, 1.0);
        let slowing = within(p.slowing, 0.0, 1.0);
        let fall = within(p.fall, 0.0, 2.0);
        // Whole milliseconds, so a glow time of 0.3 seconds is over 300 ms after a burst.
        let life_ms = (f64::from(within(p.life, 0.001, 10.0)) * 1000.0).round().max(1.0);

        let sparks = p.sparks.clamp(1, 200) as i32;
        let sector = TAU / sparks as f32;
        let apart = [1.0, 2.0, 3.0].map(|shares: f32| {
            let angle = (shares * sector).min(FRAC_PI_2);
            sine(angle, angle * angle) / shares
        });

        let mut bursts = [Burst::NONE; MAX_BURSTS];
        let mut alight = 0;
        // A burst set off by `key` (a stretch of time, a mark, or a frame), `age_ms` ago. False
        // once no more can be drawn.
        let mut add = |key: u64, age_ms: f64| {
            let Some(slot) = bursts.get_mut(alight) else {
                return false;
            };
            let age = (age_ms / life_ms) as f32;
            let reach = size * age * (1.0 + slowing * (1.0 - age));
            let at = hash(seed, key, PLACE);
            let (across, up) = if p.random_location {
                (share(at), share(at >> 16))
            } else {
                (
                    within(p.center_x, 0.0, 100.0) / 100.0,
                    within(p.center_y, 0.0, 100.0) / 100.0,
                )
            };
            let outside = reach + edge;
            let inside = (SLOWEST * reach * (1.0 - trail) - edge).max(0.0);
            *slot = Burst {
                center: [across * cells[0], up * cells[1] - fall * size * age * age],
                reach,
                outside: outside * outside,
                inside: inside * inside,
                level: 1.0 - age,
                color: match p.colors {
                    FireworksColors::Fade => colors.ramp(age),
                    _ => colors.get(hash(seed, key, COLOR) >> 16),
                },
                seed: hash(seed, key, SPARKS) as u32,
            };
            alight += 1;
            alight < MAX_BURSTS
        };

        let mut each = [[0; 3]; MAX_PALETTE_COLORS];
        let mut each_count = 0;
        if p.colors == FireworksColors::Spark {
            for slot in each.iter_mut().take(colors.len()) {
                *slot = colors.get(u64::from(each_count)).map(|v| (v * 255.0 + 0.5) as u8);
                each_count += 1;
            }
        }

        let (from, now) = (time.start_ms, time.start_ms.saturating_add(time.elapsed_ms));
        if let Some(track) = p.timing_track {
            // One at each mark since the effect started, newest first.
            let marks = cx.marks(Some(track)).unwrap_or_default();
            let passed = marks.partition_point(|m| m.start_ms <= now);
            for mark in marks[..passed].iter().rev() {
                let age = (now - mark.start_ms) as f64;
                if mark.start_ms < from || age >= life_ms || !add(mark.start_ms, age) {
                    break;
                }
            }
        } else if p.fire_on_music {
            if let Some(audio) = cx.audio {
                let frame_ms = u64::from(time.frame_ms.max(1));
                let trigger = within(p.trigger_level, 0.0, 100.0) / 100.0;
                let loud = |frame: u64| audio.peak(frame) > trigger;
                let again = (AGAIN_MS / frame_ms).max(1);
                let first = from / frame_ms;
                let mut frame = now / frame_ms;
                loop {
                    let age = (now - (frame * frame_ms).max(from)) as f64;
                    if age >= life_ms {
                        break;
                    }
                    // The frame the music gets loud on (or the effect's first, if it is loud
                    // already), and every so often while it has stayed loud since.
                    let goes_off = loud(frame)
                        && (frame == first
                            || !loud(frame - 1)
                            || (frame % again == 0
                                && frame - first >= again
                                && (frame - again..frame).all(loud)));
                    if (goes_off && !add(frame, age)) || frame == first {
                        break;
                    }
                    frame -= 1;
                }
            }
        } else {
            // One in each stretch of time, newest first: the first as the effect starts, the
            // rest at a moment of their own within their stretch.
            let stretch_ms = 1000.0 / f64::from(within(p.rate, 0.01, 20.0));
            let t = time.elapsed_ms as f64;
            let mut stretch = (t / stretch_ms) as u64;
            loop {
                let begins = stretch as f64 * stretch_ms;
                if t - (begins + stretch_ms) >= life_ms {
                    break;
                }
                let moment = if stretch == 0 {
                    0.0
                } else {
                    (hash(seed, stretch, WHEN) >> 11) as f64 / (1u64 << 53) as f64
                };
                let age = t - (begins + moment * stretch_ms);
                if (age >= 0.0 && age < life_ms && !add(stretch, age)) || stretch == 0 {
                    break;
                }
                stretch -= 1;
            }
        }

        Self {
            cells,
            opens,
            sparks,
            sector,
            apart,
            half,
            soft: p.soft,
            edge,
            edge_squared: edge * edge,
            trail,
            each,
            each_count,
            bursts,
            alight: alight as u8,
        }
    }

    /// How much of a pixel a spark or its tail covers when the nearest of it is `away` cells
    /// from the pixel's middle.
    #[inline]
    fn within(&self, away: f32) -> f32 {
        if self.soft {
            unit((self.edge - away) * (1.0 / SOFT))
        } else {
            f32::from(u8::from(away < self.half))
        }
    }

    /// How much of a pixel one spark covers: the pixel is `beside` cells off the line the spark
    /// flies along and `ahead` cells further out than the spark (behind it when negative), and
    /// the spark has flown `flown` cells. `from` is where the pixel is from the spark, across
    /// and up the target, which a crisp spark is square to.
    #[inline]
    fn spark(&self, beside: f32, ahead: f32, flown: f32, from: impl FnOnce() -> [f32; 2]) -> f32 {
        let away = beside * beside + ahead * ahead;
        let dot = if away >= self.edge_squared {
            0.0
        } else if self.soft {
            self.within(away.sqrt())
        } else {
            // The pixels whose middles the square covers: a middle on its edge belongs to the
            // pixel on one side only.
            let inside = |from: f32| -self.half <= from && from < self.half;
            let [across, up] = from();
            f32::from(u8::from(inside(across) && inside(up)))
        };
        // The tail: from the spark back toward the center, fading to nothing at its end.
        let tail = self.trail * flown;
        if ahead < 0.0 && -ahead < tail {
            dot.max(self.within(beside.abs()) * (1.0 + ahead / tail))
        } else {
            dot
        }
    }

    /// How many shares of the circle on one side of a pixel's own hold sparks that could reach
    /// it: the pixel is `r` cells from the center and `from` of a share from that side of its
    /// own.
    #[inline]
    fn shares_near(&self, r: f32, from: f32) -> i32 {
        let mut shares = 0;
        while shares < NEAR && (from + shares as f32) * r * self.apart[shares] < self.edge {
            shares += 1;
        }
        shares as i32
    }

    /// How much of a pixel `x` cells across and `y` up from a burst's center the burst's sparks
    /// cover, and the number the spark covering most of it draws its color from.
    #[inline]
    fn round(&self, burst: &Burst, x: f32, y: f32) -> (f32, u64) {
        let squared = x * x + y * y;
        if squared >= burst.outside || squared < burst.inside {
            return (0.0, 0);
        }
        let r = squared.sqrt();
        // The pixel's place round the burst, in shares of the circle: the whole part names the
        // share it is in, the fraction says how far through it.
        let round = turn(x, y) * self.sparks as f32;
        let own = (round as i32).min(self.sparks - 1);
        let through = round - own as f32;
        // The shares either side whose sparks could reach the pixel: all of them, when there
        // are few.
        let (below, above) = if self.sparks <= 2 * NEAR as i32 + 1 {
            (self.sparks / 2, self.sparks - 1 - self.sparks / 2)
        } else {
            (self.shares_near(r, through), self.shares_near(r, 1.0 - through))
        };
        let half_round = self.sparks as f32 * 0.5;
        let mut most = (0.0f32, 0);
        for other in -below..=above {
            let number = hash(
                u64::from(burst.seed),
                (own + other).rem_euclid(self.sparks) as u64,
                0,
            );
            // Each spark has its own place within its share of the circle, and its own speed.
            let flown = speed(share(number >> 16)) * burst.reach;
            // How far round from the spark's line the pixel is, the short way. One more than a
            // quarter turn round counts as a quarter turn round: as far from the line, and
            // level with the center.
            let mut round = through - other as f32 - share(number);
            if round > half_round {
                round -= self.sparks as f32;
            } else if round < -half_round {
                round += self.sparks as f32;
            }
            let angle = (round * self.sector).clamp(-FRAC_PI_2, FRAC_PI_2);
            let squared = angle * angle;
            let (sin, cos) = (sine(angle, squared), cosine(squared));
            let cover = self.spark(r * sin, r * cos - flown, flown, || {
                // The spark's line is the pixel's direction (the center itself points right)
                // turned back by the angle between them, and the spark is `flown` along it.
                let out = if r > 0.0 { [x / r, y / r] } else { [1.0, 0.0] };
                let line = [out[0] * cos + out[1] * sin, out[1] * cos - out[0] * sin];
                [x - flown * line[0], y - flown * line[1]]
            });
            if cover > most.0 {
                most = (cover, number >> 32);
            }
        }
        most
    }

    /// The same for a pixel `x` cells along a line from a burst's center.
    #[inline]
    fn along(&self, burst: &Burst, x: f32) -> (f32, u64) {
        let away = x.abs();
        if away * away >= burst.outside || away * away < burst.inside {
            return (0.0, 0);
        }
        // Half the sparks fly each way (the odd one forward), spread evenly by speed from the
        // slowest to the fastest, each at its own place within its share of that.
        let back = i32::from(x < 0.0);
        let count = (self.sparks + 1 - back) / 2;
        let span = (1.0 - SLOWEST) * burst.reach;
        let nearest = if span > 0.0 {
            (((away - SLOWEST * burst.reach) / span * count as f32) as i32).clamp(-1, count)
        } else {
            0
        };
        let mut most = (0.0f32, 0);
        for spark in (nearest - 1).max(0)..(nearest + 2).min(count) {
            let number = hash(u64::from(burst.seed), (2 * spark + back) as u64, 0);
            let speed = SLOWEST + (1.0 - SLOWEST) * (spark as f32 + share(number)) / count as f32;
            let flown = speed * burst.reach;
            let cover = self.spark(0.0, away - flown, flown, || [x - x.signum() * flown, 0.0]);
            if cover > most.0 {
                most = (cover, number >> 32);
            }
        }
        most
    }
}

impl Shade for Fireworks {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let at = [place(px.u) * self.cells[0], place(px.v) * self.cells[1]];
        let mut shown = Rgba::CLEAR;
        for burst in &self.bursts[..usize::from(self.alight)] {
            // The bursts are newest first, so brightest first: once the pixel shows as much as
            // a burst could give it, no later one can show more.
            if burst.level <= shown.a {
                break;
            }
            let (x, y) = (at[0] - burst.center[0], at[1] - burst.center[1]);
            let (cover, number) = match self.opens {
                Opens::Round => self.round(burst, x, y),
                Opens::Across => self.along(burst, x),
                Opens::Up => self.along(burst, y),
            };
            let level = cover * burst.level;
            if level > shown.a {
                let color = if self.each_count > 0 {
                    self.each[(number % u64::from(self.each_count)) as usize]
                        .map(|v| f32::from(v) * (1.0 / 255.0))
                } else {
                    burst.color
                };
                shown = Rgba::with_alpha(color, level);
            }
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::Rgb;

    const RIG: Canvas = Canvas {
        columns: 121,
        rows: 121,
    };

    /// One burst in the middle of the rig, sizes in cells, `ms` into the second it glows.
    fn one(p: FireworksParams, ms: u64) -> Fireworks {
        let p = FireworksParams {
            rate: 0.01,
            life: 1.0,
            fall: 0.0,
            random_location: false,
            scale: false,
            ..p
        };
        Fireworks::new(
            &p,
            &EffectTime::within(0, 60_000, ms),
            Colors::new(&[Rgb::WHITE]),
            0xB0057,
            RIG,
            &RenderContext::default(),
        )
    }

    /// What every spark of the burst covers of a pixel, each worked out from its own place:
    /// the most any covers.
    fn every_spark(fireworks: &Fireworks, burst: &Burst, x: f32, y: f32) -> f32 {
        let r = f64::from(x).hypot(f64::from(y));
        (0..fireworks.sparks)
            .map(|spark| {
                let number = hash(u64::from(burst.seed), spark as u64, 0);
                let flown = f64::from(speed(share(number >> 16)) * burst.reach);
                let turns = (f64::from(spark) + f64::from(share(number))) / f64::from(fireworks.sparks);
                let (sin, cos) = (turns * std::f64::consts::TAU).sin_cos();
                let out = f64::from(x) * cos + f64::from(y) * sin;
                let (beside, ahead) = if out < 0.0 {
                    // More than a quarter turn round from the spark's line.
                    (r, -flown)
                } else {
                    (f64::from(y) * cos - f64::from(x) * sin, out - flown)
                };
                let from = [f64::from(x) - flown * cos, f64::from(y) - flown * sin];
                fireworks.spark(beside as f32, ahead as f32, flown as f32, || {
                    from.map(|v| v as f32)
                })
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn the_sparks_a_pixel_looks_at_are_all_that_could_reach_it() {
        let mut compared = 0;
        let mut lit = 0;
        // Pixels the edge of a crisp spark or of its tail passes within a hair of: lit one way
        // and not the other.
        let mut on_an_edge = 0;
        for sparks in [1, 2, 5, 7, 8, 13, 50, 200] {
            for (spark_size, soft) in [(1.0, true), (5.0, true), (1.0, false), (4.0, false)] {
                for trail in [0.0, 0.5, 1.0] {
                    for ms in [200, 700] {
                        let p = FireworksParams {
                            sparks,
                            spark_size,
                            soft,
                            trail,
                            size: 55.0,
                            ..FireworksParams::default()
                        };
                        let fireworks = one(p, ms);
                        let burst = fireworks.bursts[0];
                        assert_eq!(fireworks.alight, 1);
                        // Out to where more shares than are looked at hold sparks that could
                        // reach a pixel, the sums can only miss some: never find more. Nearer
                        // the center than a spark is wide, one on the far side of it is drawn
                        // as if a quarter turn round.
                        let crowded = if sparks <= 7 {
                            0.0
                        } else {
                            fireworks.edge / (NEAR as f32 * fireworks.apart[NEAR - 1])
                        };
                        let exact_from = crowded.max(if soft { 0.0 } else { 2.0 * fireworks.edge });
                        for row in (0..121).step_by(2) {
                            for column in (0..121).step_by(2) {
                                // Not on the grid's own lines, to try every part of a cell.
                                let x = column as f32 - 60.0 + 0.37 * (row % 3) as f32;
                                let y = row as f32 - 60.0 - 0.21 * (column % 5) as f32;
                                let (cover, _) = fireworks.round(&burst, x, y);
                                let all = every_spark(&fireworks, &burst, x, y);
                                let what = || {
                                    format!(
                                        "{sparks} sparks {spark_size} wide (soft: {soft}), tails {trail}, at {ms}: ({x}, {y})"
                                    )
                                };
                                compared += 1;
                                lit += usize::from(all > 0.0);
                                if !soft && (cover - all).abs() >= 5e-3 && x.hypot(y) >= exact_from {
                                    on_an_edge += 1;
                                } else if x.hypot(y) >= exact_from {
                                    assert!((cover - all).abs() < 5e-3, "{}: {cover} for {all}", what());
                                } else {
                                    assert!(cover < all + 5e-3 || !soft, "{}: {cover} for {all}", what());
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(compared > 600_000 && lit > 20_000, "{lit} of {compared}");
        assert!(on_an_edge * 2000 < lit, "{on_an_edge} of {lit}");
    }

    #[test]
    fn the_polynomial_is_the_arctangent() {
        let mut worst = 0.0f64;
        for step in 0..20_000 {
            let angle = step as f64 / 20_000.0 * std::f64::consts::TAU;
            for r in [0.01, 1.0, 37.5, 4000.0] {
                let (x, y) = ((r * angle.cos()) as f32, (r * angle.sin()) as f32);
                let exact = (f64::from(y).atan2(f64::from(x)) / std::f64::consts::TAU).rem_euclid(1.0);
                let off = (f64::from(turn(x, y)) - exact).abs();
                worst = worst.max(off.min(1.0 - off));
            }
        }
        // 1.2e-5 radians.
        assert!(worst < 2e-6, "{worst} of a turn");
        assert_eq!(turn(0.0, 0.0), 0.0);
        assert_eq!(turn(3.0, 0.0), 0.0);
        assert_eq!(turn(0.0, 2.0), 0.25);
        assert_eq!(turn(-1.0, 0.0), 0.5);
        assert_eq!(turn(0.0, -5.0), 0.75);
        for (x, y) in [(1.0, -1e-30), (1e30, -1e-30), (f32::MAX, f32::MIN_POSITIVE)] {
            assert!((0.0..1.0).contains(&turn(x, y)));
        }
    }

    #[test]
    fn no_more_bursts_than_the_most_are_drawn_and_they_are_the_newest() {
        // Twenty a second, each alight for ten: two hundred at once.
        let p = FireworksParams {
            rate: 20.0,
            life: 10.0,
            ..FireworksParams::default()
        };
        let time = EffectTime::within(0, 60_000, 30_000);
        let fireworks = Fireworks::new(&p, &time, Colors::new(&[]), 7, RIG, &RenderContext::default());
        assert_eq!(usize::from(fireworks.alight), MAX_BURSTS);
        let levels: Vec<f32> = fireworks.bursts.iter().map(|b| b.level).collect();
        assert!(levels.windows(2).all(|w| w[0] >= w[1]), "{levels:?}");
        // The newest went off within the last twentieth of a second, the oldest drawn within
        // the last twelve of those.
        assert!(
            levels[0] > 1.0 - 0.005 && levels[MAX_BURSTS - 1] > 1.0 - 0.085,
            "{levels:?}"
        );
    }

    #[test]
    fn a_burst_is_the_same_burst_on_every_frame_of_its_life() {
        let p = FireworksParams {
            rate: 1.0,
            life: 0.5,
            fall: 0.0,
            ..FireworksParams::default()
        };
        let at = |ms: u64| {
            Fireworks::new(
                &p,
                &EffectTime::within(0, 60_000, ms),
                Colors::new(&[Rgb::RED, Rgb::GREEN, Rgb::BLUE]),
                7,
                RIG,
                &RenderContext::default(),
            )
        };
        // Every burst seen, by what its sparks are drawn from: where it is, its color, and how
        // far its sparks had flown when it was last seen.
        let mut seen: std::collections::HashMap<u32, ([f32; 2], [f32; 3], f32)> = Default::default();
        for ms in (0..20_000).step_by(10) {
            let fireworks = at(ms);
            for burst in &fireworks.bursts[..usize::from(fireworks.alight)] {
                let was = seen.insert(burst.seed, (burst.center, burst.color, burst.reach));
                if let Some((center, color, reach)) = was {
                    assert_eq!((burst.center, burst.color), (center, color), "at {ms}");
                    assert!(burst.reach > reach, "at {ms}");
                }
            }
        }
        // One a second, each somewhere of its own.
        assert_eq!(seen.len(), 20);
        let places: std::collections::HashSet<[u32; 2]> = seen
            .values()
            .map(|(center, ..)| center.map(f32::to_bits))
            .collect();
        assert_eq!(places.len(), 20);
    }

    #[test]
    fn a_target_too_thin_for_a_burst_to_open_on_is_a_line() {
        let opens = |columns, rows| {
            Fireworks::new(
                &FireworksParams::default(),
                &EffectTime::within(0, 1000, 0),
                Colors::new(&[]),
                0,
                Canvas { columns, rows },
                &RenderContext::default(),
            )
            .opens
        };
        assert_eq!(opens(50, 1), Opens::Across);
        assert_eq!(opens(50, 3), Opens::Across);
        assert_eq!(opens(1, 50), Opens::Up);
        assert_eq!(opens(3, 50), Opens::Up);
        assert_eq!(opens(2, 3), Opens::Up);
        assert_eq!(opens(0, 0), Opens::Across);
        assert_eq!(opens(4, 4), Opens::Round);
        assert_eq!(opens(50, 4), Opens::Round);
        assert_eq!(opens(16, 50), Opens::Round);
    }
}
