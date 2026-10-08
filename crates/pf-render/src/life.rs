//! The Life effect, as xLights draws it (`LifeEffect` in `src-core/effects/LifeEffect.cpp` and
//! `ispc/LifeFunctions.ispc`): the Game of Life on the target's grid, wrapping around its edges.
//!
//! Random cells in colors blended from the palette start it off. xLights counts `speed` a
//! frame-worth of time and moves to the next generation each time the count passes a multiple of
//! 20 (on a 400 cycle), so at most once a frame; newborn cells take a random blend of the palette.
//! Generations follow from the ones before, so Life is worked out frame by frame from the start
//! (see `sim.rs`). Randomness is keyed to the effect's seed and the frame.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, Rng, Shade, hash, hash01};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{LifeParams, LifeRules};
use std::sync::Arc;

const SEED: u64 = 0x11_FE;
const BIRTH: u64 = 0xB1_47;

/// The grid between frames: each cell's color while it lives.
#[derive(Debug, Clone)]
pub(crate) struct Colony {
    width: i32,
    height: i32,
    /// Shared with the frame's shader rather than copied.
    cells: Arc<Vec<Option<[f32; 3]>>>,
    /// Where the generation count was at the last frame (xLights' `LastLifeState`).
    last_state: i64,
}

/// Whether a cell lives on (it was alive) or is born (it was dead) with `n` live neighbors.
fn lives(rules: LifeRules, alive: bool, n: u32) -> bool {
    match (rules, alive) {
        (LifeRules::Classic, true) => n == 2 || n == 3,
        (LifeRules::Classic, false) => n == 3,
        (LifeRules::B35S236, true) => matches!(n, 2 | 3 | 6),
        (LifeRules::B35S236, false) => matches!(n, 3 | 5),
        (LifeRules::Amoeba, true) => matches!(n, 1 | 3 | 5 | 8),
        (LifeRules::Amoeba, false) => matches!(n, 3 | 5 | 7),
        (LifeRules::Coagulations, true) => n == 2 || n == 3 || n >= 5,
        (LifeRules::Coagulations, false) => matches!(n, 3 | 7 | 8),
        (LifeRules::B25678S5678, true) => n >= 5,
        (LifeRules::B25678S5678, false) => n == 2 || n >= 5,
    }
}

impl Colony {
    /// The starting cells: `density`% of half the grid's cells (plus one) picked at random, some
    /// picked twice.
    pub fn seed(p: &LifeParams, colors: &Colors, seed: u64, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let mut cells = vec![None; (width * height) as usize];
        let count = i64::from(width) * i64::from(height) * i64::from(p.density.min(100)) / 200 + 1;
        let mut rng = Rng::new(hash(seed, SEED, 0));
        for _ in 0..count {
            let x = rng.int(0, width - 1);
            let y = rng.int(0, height - 1);
            let color = colors.ramp(rng.unit() as f32);
            cells[(y * width + x) as usize] = Some(color);
        }
        Self {
            width,
            height,
            cells: Arc::new(cells),
            last_state: 0,
        }
    }

    /// Frame `frame` of the effect (0 the first), with the settings then: a new generation when
    /// the count moves on.
    pub fn step(&mut self, p: &LifeParams, colors: &Colors, seed: u64, frame: u64, frame_ms: u32) {
        let state = (frame as i64)
            .saturating_mul(i64::from(p.speed.clamp(1, 30)))
            .saturating_mul(i64::from(frame_ms))
            / 50;
        let generation = state % 400 / 20;
        if generation == self.last_state {
            return;
        }
        self.last_state = generation;
        let (w, h) = (self.width, self.height);
        let (wu, hu) = (w as usize, h as usize);
        let alive: Vec<u8> = self.cells.iter().map(|c| u8::from(c.is_some())).collect();
        let mut next = vec![None; self.cells.len()];
        for y in 0..hu {
            // The rows above and below, wrapping around.
            let rows = [(y + hu - 1) % hu * wu, y * wu, (y + 1) % hu * wu];
            for x in 0..wu {
                let i = y * wu + x;
                let (left, right) = ((x + wu - 1) % wu, (x + 1) % wu);
                let mut n = 0;
                for (k, &row) in rows.iter().enumerate() {
                    n += alive[row + left] + alive[row + right];
                    if k != 1 {
                        n += alive[row + x];
                    }
                }
                let n = u32::from(n);
                let alive = alive[i] == 1;
                if lives(p.rules, alive, n) {
                    next[i] = if alive {
                        self.cells[i]
                    } else {
                        Some(colors.ramp(hash01(seed ^ BIRTH, frame, i as u64)))
                    };
                }
            }
        }
        self.cells = Arc::new(next);
    }
}

pub struct Life {
    colony: Colony,
}

impl Life {
    pub(crate) fn new(colony: &Colony) -> Self {
        Self {
            colony: colony.clone(),
        }
    }
}

impl Shade for Life {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let c = &self.colony;
        let (x, y) = cell_of(px, c.width, c.height);
        c.cells[(y * c.width + x) as usize].map_or(Rgba::CLEAR, Rgba::opaque)
    }
}
