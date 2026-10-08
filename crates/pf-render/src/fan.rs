//! The Fan effect, as xLights draws it (`FanEffect::Render` in `src-core/effects/FanEffect.cpp`):
//! every cell of the target's grid inside the ring between the two radii lights when its angle
//! around the center falls on a blade. Each blade holds the palette colors side by side, each
//! split into stripes; the blades turn over the effect and bend toward their tips.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade, accelerate};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{Direction, FanParams};

pub struct Fan {
    colors: Colors,
    width: i32,
    height: i32,
    /// The center, in cells from the grid's middle.
    center: (i32, i32),
    /// The lit ring, in cells.
    inner: f64,
    outer: f64,
    /// The radius the blade curve is measured against (never 0).
    max_radius: f64,
    blade_angle: f64,
    start_angle: f64,
    /// How far the blades have turned, in degrees.
    turned: f64,
    reverse: bool,
    blend_edges: bool,
    /// Each blade's slice of the circle, the part of it the blade fills, each color's share, each
    /// stripe's share, and the part of that the stripe fills (all degrees).
    blade_slice: f64,
    blade_width: f64,
    color_width: f64,
    stripe_slice: f64,
    stripe_width: f64,
}

impl Fan {
    pub fn new(p: &FanParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let progress = accelerate(f64::from(time.t_norm), f64::from(p.acceleration));
        let mut inner = f64::from(p.start_radius);
        let mut outer = f64::from(p.end_radius);
        let mut max_radius = inner.max(outer);
        if p.scale {
            // 100 reaches from the center to the edge of the longer side.
            let scale = f64::from(width.max(height)) / 200.0;
            inner *= scale;
            outer *= scale;
            max_radius *= scale;
        }
        // The blades grow out from the inner radius at the start and shrink back to the outer
        // one at the end, over the part of the effect they aren't at full length.
        let full = f64::from(p.duration) / 100.0;
        let ramp = (1.0 - full) / 2.0;
        if full < 1.0 {
            let delta = (outer - inner).abs();
            if progress < ramp {
                let left = 1.0 - progress / ramp;
                outer = if outer > inner {
                    outer - delta * left
                } else {
                    outer + delta * left
                };
            } else if progress > 1.0 - ramp {
                let left = (1.0 - progress) / ramp;
                inner = if outer > inner {
                    outer - delta * left
                } else {
                    outer + delta * left
                };
            }
        }
        if inner > outer {
            std::mem::swap(&mut inner, &mut outer);
        }
        let mut blade_angle = f64::from(p.blade_angle);
        if max_radius <= 0.0 {
            blade_angle = 0.0;
            max_radius = 1.0;
        }
        let blade_slice = 360.0 / f64::from(p.blades.max(1));
        let blade_width = blade_slice * f64::from(p.blade_width) / 100.0;
        let color_width = blade_width / colors.len() as f64;
        let stripe_slice = color_width / f64::from(p.elements.max(1));
        Self {
            colors,
            width,
            height,
            center: (
                ((f64::from(p.center_x) - 50.0) * f64::from(width) / 100.0) as i32,
                ((f64::from(p.center_y) - 50.0) * f64::from(height) / 100.0) as i32,
            ),
            inner,
            outer,
            max_radius,
            blade_angle,
            start_angle: f64::from(p.start_angle),
            turned: progress * f64::from(p.revolutions) * 360.0,
            reverse: p.direction == Direction::Reverse,
            blend_edges: p.blend_edges,
            blade_slice,
            blade_width,
            color_width,
            stripe_slice,
            stripe_width: stripe_slice * f64::from(p.element_width) / 100.0,
        }
    }
}

impl Shade for Fan {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.width, self.height);
        let x1 = f64::from(x - self.center.0 - self.width / 2);
        let y1 = f64::from(y - self.center.1 - self.height / 2);
        let r = x1.hypot(y1);
        if r < self.inner || r > self.outer {
            return Rgba::CLEAR;
        }
        let twist = r / self.max_radius * self.blade_angle;
        let mut theta = x1.atan2(y1).to_degrees() + twist + self.start_angle;
        theta = if self.reverse {
            self.turned - theta + 180.0
        } else {
            theta + 180.0 + self.turned
        };
        if theta < 0.0 {
            theta += 360.0;
        }
        // xLights takes whole blades and stripes toward zero.
        let in_blade = theta - (theta / self.blade_slice).trunc() * self.blade_slice;
        if in_blade > self.blade_width {
            return Rgba::CLEAR;
        }
        let in_stripe = in_blade - (in_blade / self.stripe_slice).trunc() * self.stripe_slice;
        if in_stripe > self.stripe_width {
            return Rgba::CLEAR;
        }
        let k = (in_blade / self.color_width).trunc() as i64;
        let color = self.colors.get(k.rem_euclid(self.colors.len() as i64) as u64);
        if self.blend_edges {
            let level = 1.0 - (in_stripe - self.stripe_width / 2.0).abs() * 2.0 / self.stripe_width;
            Rgba::with_alpha(color, level.clamp(0.0, 1.0) as f32)
        } else {
            Rgba::opaque(color)
        }
    }
}
