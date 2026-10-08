//! The Snowflakes effect, as xLights draws it (`SnowflakesEffect` in
//! `src-core/effects/SnowflakesEffect.cpp`).
//!
//! Blowing snow (xLights' Driving) scatters the flakes once, then shows that field twice, offset,
//! sliding diagonally across the target's grid and wrapping around: each frame follows from the
//! time alone. Falling snow moves each flake down a row (or diagonally past one in its way) every
//! few frames, adding new ones along the top as flakes leave the bottom or, piling up, come to
//! rest; that is worked out frame by frame from the start (see `sim.rs`). Randomness is keyed to
//! the effect's seed, the frame, and what it decides.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Rng, Shade, hash};
use crate::geometry::Pixel;
use crate::raster::{Raster, cell_of, grid_size};
use pf_sequence::{SnowflakeShape, SnowflakesMotion, SnowflakesParams};

/// Salts for the effect's random numbers, so each use draws its own.
const SCATTER: u64 = 0x5_C477;
const MOVE: u64 = 0x5_3073;
const TURN: u64 = 0x5_7055;

/// A flake's look as xLights numbers it (0 a dot to 8 an X; 9 draws nothing).
fn look(shape: SnowflakeShape, rng: &mut Rng) -> u8 {
    match shape {
        SnowflakeShape::Random => rng.int(0, 8) as u8,
        SnowflakeShape::Dot => 0,
        SnowflakeShape::Cross => 1,
        SnowflakeShape::Bar => 2,
        SnowflakeShape::BigCross => 3,
        SnowflakeShape::Star => 4,
        SnowflakeShape::Square => 5,
        SnowflakeShape::Plus => 6,
        SnowflakeShape::Diamond => 7,
        SnowflakeShape::X => 8,
    }
}

/// The flakes' first and second colors (the second white with a one-color palette, as xLights
/// gives white for a color the palette doesn't have).
fn flake_colors(colors: &Colors) -> ([f32; 3], [f32; 3]) {
    let second = if colors.len() > 1 { colors.get(1) } else { [1.0; 3] };
    (colors.get(0), second)
}

/// A grid of cells, each empty (0) or holding something (1 and up).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cells {
    pub width: i32,
    pub height: i32,
    cells: Vec<u8>,
}

impl Cells {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            cells: vec![0; (width.max(0) * height.max(0)) as usize],
        }
    }

    /// What's at a cell (0 outside the grid).
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.cells[(y * self.width + x) as usize]
        } else {
            0
        }
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, value: u8) {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.cells[(y * self.width + x) as usize] = value;
        }
    }
}

/// Where a scattered flake lands (`AdvanceState`'s first frame): one of four bands up the grid by
/// its number, at an empty cell if one turns up in 20 tries.
fn scatter_place(cells: &Cells, n: u32, rng: &mut Rng) -> (i32, i32) {
    let (width, height) = (cells.width, cells.height);
    let mut band = height / 4;
    let y0 = (n % 4) as i32 * band;
    if y0 + band > height {
        band = height - y0;
    }
    let band = band.max(1);
    let (mut x, mut y) = (0, 0);
    for _ in 0..20 {
        x = rng.int(0, width - 1);
        y = y0 + rng.int(0, band - 1);
        if cells.get(x, y) == 0 {
            break;
        }
    }
    (x, y)
}

/// Keeps a flake of `look` whole on the grid, as xLights nudges it in from the edges.
fn nudge(look: u8, (mut x, mut y): (i32, i32), width: i32, height: i32) -> (i32, i32) {
    let reach = match look {
        1 | 2 | 6 | 8 => 1,
        3 | 4 | 7 => 2,
        _ => 0,
    };
    if reach > 0 {
        if x < reach {
            x += reach;
        }
        if y < reach {
            y += reach;
        }
        if x > width - 1 - reach {
            x -= reach;
        }
        if y > height - 1 - reach {
            y -= reach;
        }
    } else if look == 5 {
        if x > width - 2 {
            x -= 1;
        }
        if y > height - 2 {
            y -= 1;
        }
    }
    (x, y)
}

/// The cells of a flake of `look` at (x, y): its center and the rest in the first color
/// (`true`) or the second.
fn flake_cells(look: u8, x: i32, y: i32, horizontal: bool) -> Vec<(i32, i32, bool)> {
    let mut out = vec![(x, y, true)];
    let mut second = |cells: &[(i32, i32)]| out.extend(cells.iter().map(|&(dx, dy)| (x + dx, y + dy, false)));
    match look {
        1 => second(&[(-1, 0), (1, 0), (0, -1), (0, 1)]),
        2 if horizontal => second(&[(-1, 0), (1, 0)]),
        2 => second(&[(0, -1), (0, 1)]),
        3 => second(&[(-1, 0), (1, 0), (0, -1), (0, 1), (-2, 0), (2, 0), (0, -2), (0, 2)]),
        4 => second(&[
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, 2),
            (1, 2),
            (-1, -2),
            (1, -2),
            (2, -1),
            (2, 1),
            (-2, -1),
            (-2, 1),
        ]),
        5 => out.extend([(1, 0), (1, 1), (0, 1)].map(|(dx, dy)| (x + dx, y + dy, true))),
        6 => out.extend([(1, 0), (0, 1), (-1, 0), (0, -1)].map(|(dx, dy)| (x + dx, y + dy, true))),
        7 => out.extend(
            [
                (0, 2),
                (-1, 1),
                (0, 1),
                (1, 1),
                (-2, 0),
                (-1, 0),
                (1, 0),
                (2, 0),
                (-1, -1),
                (0, -1),
                (1, -1),
                (0, -2),
            ]
            .map(|(dx, dy)| (x + dx, y + dy, true)),
        ),
        8 => out.extend([(1, 1), (-1, 1), (-1, -1), (1, -1)].map(|(dx, dy)| (x + dx, y + dy, true))),
        9 => out.clear(),
        _ => {}
    }
    out
}

pub struct Snowflakes {
    look: Look,
}

enum Look {
    /// The scattered field, slid across by whole cells (each pixel looks its cell up).
    Blowing {
        field: Cells,
        shift_x: i64,
        shift_y: i64,
        first: [f32; 3],
        second: [f32; 3],
    },
    /// Falling snow, drawn flake by flake.
    Drawn(Raster),
}

impl Snowflakes {
    /// Blowing snow at `time`: the scattered field, shown twice and slid across.
    pub(crate) fn blowing(
        p: &SnowflakesParams,
        time: &EffectTime,
        colors: Colors,
        seed: u64,
        canvas: Canvas,
    ) -> Self {
        let (width, height) = grid_size(canvas);
        // The field, scattered afresh whenever the number of flakes changes (as xLights does).
        let mut field = Cells::new(width, height);
        let mut rng = Rng::new(hash(seed, SCATTER, u64::from(p.count)));
        for n in 0..p.count.min(100) {
            let at = scatter_place(&field, n, &mut rng);
            let kind = look(p.flake, &mut rng);
            let (x, y) = nudge(kind, at, width, height);
            let horizontal = kind == 2 && rng.int(0, 99) > 50;
            for (cx, cy, first) in flake_cells(kind, x, y, horizontal) {
                field.set(cx, cy, if first { 1 } else { 2 });
            }
        }
        let (first, second) = flake_colors(&colors);
        let speed = i64::from((p.speed as i32).clamp(0, 50));
        let elapsed = time.frame() as i64 * i64::from(time.frame_ms);
        let movement = elapsed.saturating_mul(speed) / 50;
        Self {
            look: Look::Blowing {
                field,
                shift_x: movement / 20,
                shift_y: movement / 10,
                first,
                second,
            },
        }
    }

    /// Falling snow as it lies: each flake drawn in its look (a three-dot flake still falling
    /// turns at random each frame; one at rest lies flat).
    pub(crate) fn falling(fall: &Fall, colors: Colors, seed: u64, frame: u64, canvas: Canvas) -> Self {
        let mut raster = Raster::new(canvas);
        let flakes = &fall.flakes;
        let (first, second) = flake_colors(&colors);
        let (first, second) = (Rgba::opaque(first), Rgba::opaque(second));
        let mut rng = Rng::new(hash(seed, TURN, frame));
        for y in 0..flakes.height {
            for x in 0..flakes.width {
                let held = flakes.get(x, y);
                if held == 0 {
                    continue;
                }
                let kind = held - 1;
                let horizontal = kind != 2 || at_rest(flakes, x, y) || rng.int(0, 99) > 50;
                for (cx, cy, is_first) in flake_cells(kind, x, y, horizontal) {
                    if is_first {
                        raster.set(cx, cy, first);
                    } else if flakes.get(cx, cy) == 0 {
                        // Arms don't cover other flakes.
                        raster.set(cx, cy, second);
                    }
                }
            }
        }
        Self {
            look: Look::Drawn(raster),
        }
    }
}

/// A three-dot flake is at rest when the column under it (all but the row just below) is full.
fn at_rest(flakes: &Cells, x: i32, y: i32) -> bool {
    (0..y - 1).all(|below| flakes.get(x, below) != 0)
}

impl Shade for Snowflakes {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        match &self.look {
            Look::Blowing {
                field,
                shift_x,
                shift_y,
                first,
                second,
            } => {
                // The field shows twice: slid up and right, and slid up and left half a grid
                // higher, where the first copy has nothing.
                let (w, h) = (i64::from(field.width), i64::from(field.height));
                let (x, y) = cell_of(px, field.width, field.height);
                let (x, y) = (i64::from(x), i64::from(y));
                let y1 = (y + shift_y).rem_euclid(h);
                let mut at = field.get((x + shift_x).rem_euclid(w) as i32, y1 as i32);
                if at == 0 {
                    at = field.get((x - shift_x).rem_euclid(w) as i32, ((y1 + h / 2) % h) as i32);
                }
                match at {
                    1 => Rgba::opaque(*first),
                    2 => Rgba::opaque(*second),
                    _ => Rgba::CLEAR,
                }
            }
            Look::Drawn(raster) => raster.at(px),
        }
    }
}

/// Falling snow between frames: where each flake is (its look plus one), and how many are
/// counted as falling (`effectState`).
#[derive(Debug, Clone)]
pub(crate) struct Fall {
    pub flakes: Cells,
    falling: i32,
}

/// Which of the three cells below (left, under, right; wrapping across) are empty, as bits 1, 2,
/// and 4. Nothing moves below the bottom row.
fn ways_down(flakes: &Cells, x: i32, y: i32) -> u8 {
    if y == 0 {
        return 0;
    }
    let w = flakes.width;
    let left = if x - 1 < 0 { x - 1 + w } else { x - 1 };
    let right = if x + 1 >= w { x + 1 - w } else { x + 1 };
    u8::from(flakes.get(left, y - 1) == 0)
        | u8::from(flakes.get(x, y - 1) == 0) << 1
        | u8::from(flakes.get(right, y - 1) == 0) << 2
}

impl Fall {
    /// The first frame's flakes, scattered up the grid.
    pub fn scatter(p: &SnowflakesParams, seed: u64, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let mut flakes = Cells::new(width, height);
        let mut rng = Rng::new(hash(seed, SCATTER, u64::MAX));
        let mut falling = 0;
        for n in 0..p.count.min(100) {
            let (x, y) = scatter_place(&flakes, n, &mut rng);
            if flakes.get(x, y) == 0 {
                falling += 1;
            }
            let kind = look(p.flake, &mut rng);
            let (x, y) = nudge(kind, (x, y), width, height);
            flakes.set(x, y, kind + 1);
        }
        Self { flakes, falling }
    }

    /// One frame on (frame `frame` of the effect, 0 the first), with the settings then.
    pub fn step(&mut self, p: &SnowflakesParams, seed: u64, frame: u64) {
        let speed = i64::from((p.speed as i32).clamp(0, 50)) + 1;
        let moves_on = |f: i64| (f * speed) / 30 != ((f - 1) * speed) / 30;
        if frame == 0 {
            // A head start: the falling done before the effect, at the same pace.
            for i in 0..i64::from(p.warmup.min(100)) {
                if moves_on(i) {
                    self.fall(p, &mut Rng::new(hash(seed, MOVE, u64::MAX - i as u64)));
                }
            }
        } else if moves_on(frame as i64) {
            self.fall(p, &mut Rng::new(hash(seed, MOVE, frame)));
        }
    }

    /// Every flake that can move down a row (`MoveFlakes`), then new flakes along the top.
    fn fall(&mut self, p: &SnowflakesParams, rng: &mut Rng) {
        let piling = p.motion == SnowflakesMotion::PilingUp;
        let falling_away = p.motion == SnowflakesMotion::Falling;
        let (width, height) = (self.flakes.width, self.flakes.height);
        let start = i32::from(piling);
        for x in 0..width {
            for y in start..height {
                let held = self.flakes.get(x, y);
                if held == 0 {
                    continue;
                }
                let moves = ways_down(&self.flakes, x, y);
                if moves == 0 && !(falling_away && y == 0) {
                    continue;
                }
                let mut x0 = match rng.int(0, 8) {
                    0 => {
                        if moves & 1 != 0 {
                            x - 1
                        } else if moves & 2 != 0 {
                            x
                        } else {
                            x + 1
                        }
                    }
                    1 => {
                        if moves & 4 != 0 {
                            x + 1
                        } else if moves & 2 != 0 {
                            x
                        } else {
                            x - 1
                        }
                    }
                    // Straight down more often, to look less jittery.
                    _ => {
                        if moves & 2 != 0 {
                            x
                        } else if moves & 5 == 4 {
                            x + 1
                        } else if moves & 5 == 1 {
                            x - 1
                        } else if rng.int(0, 1) == 0 {
                            x + 1
                        } else {
                            x - 1
                        }
                    }
                };
                if x0 < 0 {
                    x0 += width;
                } else if x0 >= width {
                    x0 -= width;
                }
                let y0 = y - 1;
                self.flakes.set(x, y, 0);
                if y0 >= 0 {
                    self.flakes.set(x0, y0, held);
                    if piling && ways_down(&self.flakes, x0, y0) == 0 {
                        // At rest: room for another at the top.
                        self.falling -= 1;
                    }
                } else {
                    self.falling -= 1;
                }
            }
        }
        // New flakes along the top.
        let count = p.count.min(100) as i32;
        let mut full = 0;
        let mut tries = 0;
        while self.falling < count && tries < 20 {
            let x = rng.int(0, width - 1);
            if self.flakes.get(x, height - 1) == 0 {
                self.falling += 1;
                let kind = look(p.flake, rng);
                self.flakes.set(x, height - 1, kind + 1);
                if ways_down(&self.flakes, x, height - 1) == 0 {
                    full += 1;
                }
            }
            tries += 1;
        }
        self.falling -= full;
    }
}
