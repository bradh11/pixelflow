//! The Garlands effect, as xLights draws it (`GarlandsEffect::Render` in
//! `src-core/effects/GarlandsEffect.cpp` and `ispc/GarlandsFunctions.ispc`): one garland per row
//! of the target's grid (per column when they stack sideways), spread `spacing` apart and sliding
//! into place one after another until they fill the grid. A garland can't fall below its own
//! row, so they pile up from the stacking side.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::Raster;
use pf_sequence::{GarlandShape, GarlandsDirection, GarlandsParams};

pub struct Garlands {
    raster: Raster,
}

/// How far below its line a garland hangs at column `x` (the swag pattern repeats every 5 or 6).
fn droop(shape: GarlandShape, x: i32) -> i32 {
    match shape {
        GarlandShape::Straight => 0,
        GarlandShape::SmallSwags => [0, 1, 2, 1, 0][x.rem_euclid(5) as usize],
        GarlandShape::Swags => [0, 2, 4, 2, 0][x.rem_euclid(5) as usize],
        GarlandShape::DeepSwags => [0, 2, 4, 6, 4, 2][x.rem_euclid(6) as usize],
        GarlandShape::DoubleDips => [0, 2, 0, 2, 0][x.rem_euclid(5) as usize],
    }
}

impl Garlands {
    pub fn new(p: &GarlandsParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let mut raster = Raster::new(canvas);
        let (width, height) = (raster.width, raster.height);
        let spacing = (p.spacing as i32).max(1);
        let mut position = time.cycle_position(f64::from(p.cycles));
        use GarlandsDirection as D;
        // 0 up, 1 down, 2 left, 3 right; the back-and-forth ones stack and then unstack.
        let dir = match p.direction {
            D::Up => 0,
            D::Down => 1,
            D::Left => 2,
            D::Right => 3,
            there_and_back => {
                position = if position > 0.5 {
                    (1.0 - position) * 2.0
                } else {
                    position * 2.0
                };
                match there_and_back {
                    D::UpThenDown => 0,
                    D::DownThenUp => 1,
                    D::LeftThenRight => 2,
                    _ => 3,
                }
            }
        };
        let (rings, across) = if dir > 1 { (width, height) } else { (height, width) };
        if rings < 1 || across < 1 {
            return Self { raster };
        }
        let pixel_spacing = (f64::from(spacing) * f64::from(rings) / 100.0).max(2.0);
        let total = f64::from(rings) * pixel_spacing - f64::from(rings) + 1.0;
        let position_offset = total * position;
        for ring in 0..rings {
            let ratio = f64::from(rings - ring - 1) / f64::from(rings);
            let color = Rgba::opaque(colors.ramp(ratio as f32));
            let line = (1.0 + f64::from(ring) * pixel_spacing - position_offset) as i32;
            for col in 0..across {
                let v = (line - droop(p.shape, col)).max(ring);
                if v >= rings {
                    continue;
                }
                let (x, y) = match dir {
                    0 => (col, v),
                    1 => (col, rings - v - 1),
                    2 => (rings - v - 1, col),
                    _ => (v, col),
                };
                raster.set(x, y, color);
            }
        }
        Self { raster }
    }
}

impl Shade for Garlands {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}
