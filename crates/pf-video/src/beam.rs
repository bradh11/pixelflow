//! Moving heads' beams and servos' arms, drawn the way the 2D previews draw them (the window's
//! `app/src/lib/beams.ts` and `stampPixels`, from the same numbers, to the same bits: the scenes
//! in `app/src/api/contracts/pixels/` hold the two to each other).
//!
//! A beam runs from the head's cell the way its pan and tilt point it (seen from the front),
//! `length` layout units out, widening to `length · tan(angle / 2)` either side (never narrower
//! than `NARROWEST` screen pixels): `along` of the way out and `r` of the way from its middle line
//! to its edge, it is `BEAM_STRENGTH × (1 - r²)² × (1 - BEAM_FADE × along)` solid in the head's
//! color, with a round cap past its end. A servo's arm turns from its cell in the fixture's front
//! plane, `ARM_LENGTH` layout units long (never under `SHORTEST_ARM` screen pixels), in
//! `ARM_COLOR`.

use crate::dots::View;
use pf_engine::{PreviewAxis, PreviewMotion};
use pf_model::{arm_direction, beam_direction, cos_deg, sin_deg};

/// How solid a beam is at its start.
pub(crate) const BEAM_STRENGTH: f64 = 0.5;
/// How much of its strength a beam has lost at its end.
const BEAM_FADE: f64 = 0.6;
/// The narrowest a beam is drawn, either side of its middle, in screen pixels.
const NARROWEST: f64 = 1.5;
/// A servo's arm: its length in layout units, its shortest in screen pixels, and half its width
/// in screen pixels.
const ARM_LENGTH: f64 = 0.4;
const SHORTEST_ARM: f64 = 10.0;
const ARM_HALF_WIDTH: f64 = 1.0;
/// A servo's arm: red, green, blue.
pub(crate) const ARM_COLOR: [u8; 3] = [200, 200, 208];

/// Where an axis points, in degrees from Home, in `frame`.
fn degrees(axis: &PreviewAxis, frame: &[u8]) -> f64 {
    axis.degrees(Some(frame))
}

/// Which way a head points in the layout with `frame`.
pub(crate) fn head_direction(motion: &PreviewMotion, frame: &[u8]) -> [f64; 3] {
    let beam = motion.beam.as_ref();
    let pan = beam
        .and_then(|b| b.pan.as_ref())
        .map_or(0.0, |a| degrees(a, frame));
    let tilt = beam
        .and_then(|b| b.tilt.as_ref())
        .map_or(0.0, |a| degrees(a, frame));
    beam_direction(pan, tilt, &motion.basis)
}

/// Which way servo `k` points in the layout with `frame`.
pub(crate) fn servo_direction(motion: &PreviewMotion, k: usize, frame: &[u8]) -> [f64; 3] {
    arm_direction(degrees(&motion.servos[k], frame), &motion.basis)
}

/// A beam in a picture: where it starts, how far and which way it runs, how wide its end is, and
/// how narrow it may be, in picture pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BeamShape {
    ax: f64,
    ay: f64,
    ex: f64,
    ey: f64,
    w: f64,
    narrow: f64,
}

/// A beam from the picture point (`ax`, `ay`) pointing `d` (layout), `length` long and
/// `angle_deg` wide, in `view`.
pub(crate) fn beam_shape(
    ax: f64,
    ay: f64,
    d: [f64; 3],
    length: f64,
    angle_deg: f64,
    view: &View,
) -> BeamShape {
    let scale = length * view.zoom * view.ratio;
    let narrow = NARROWEST * view.ratio;
    let spread = sin_deg(angle_deg / 2.0) / cos_deg(angle_deg / 2.0);
    BeamShape {
        ax,
        ay,
        ex: d[0] * scale,
        ey: -d[1] * scale,
        w: (scale * spread).max(narrow),
        narrow,
    }
}

/// The picture pixels a shape `reach` wide either side of the line from (`ax`, `ay`) by
/// (`ex`, `ey`) can touch, in a picture `width` by `height`: columns `x0..x1`, rows `y0..y1`.
fn shape_box(
    ax: f64,
    ay: f64,
    ex: f64,
    ey: f64,
    reach: f64,
    width: usize,
    height: usize,
) -> (usize, usize, usize, usize) {
    let low = |v: f64| v.floor().max(0.0) as usize;
    let high = |v: f64, most: usize| (v.ceil().max(0.0) as usize).min(most);
    (
        low(ax.min(ax + ex) - reach),
        high(ax.max(ax + ex) + reach, width),
        low(ay.min(ay + ey) - reach),
        high(ay.max(ay + ey) + reach, height),
    )
}

impl BeamShape {
    /// The picture pixels it can touch.
    pub(crate) fn bounds(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        shape_box(self.ax, self.ay, self.ex, self.ey, self.w + 1.0, width, height)
    }

    /// How solid it is at the middle of the picture pixel (`px`, `py`), as a share of its
    /// strength (0 outside it).
    pub(crate) fn at(&self, px: usize, py: usize) -> f64 {
        let qx = px as f64 + 0.5 - self.ax;
        let qy = py as f64 + 0.5 - self.ay;
        let ee = self.ex * self.ex + self.ey * self.ey;
        let t = if ee > 0.0 {
            (qx * self.ex + qy * self.ey) / ee
        } else {
            1.0
        };
        if t < 0.0 {
            return 0.0;
        }
        let (r2, fade) = if t <= 1.0 {
            let rx = qx - t * self.ex;
            let ry = qy - t * self.ey;
            let half = (t * self.w).max(self.narrow);
            ((rx * rx + ry * ry) / (half * half), 1.0 - BEAM_FADE * t)
        } else {
            let rx = qx - self.ex;
            let ry = qy - self.ey;
            ((rx * rx + ry * ry) / (self.w * self.w), 1.0 - BEAM_FADE)
        };
        if r2 >= 1.0 {
            return 0.0;
        }
        let s = 1.0 - r2;
        s * s * fade
    }
}

/// A servo's arm in a picture: its pivot, how far and which way it reaches, and half its width,
/// in picture pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArmShape {
    ax: f64,
    ay: f64,
    ex: f64,
    ey: f64,
    half: f64,
}

/// A servo's arm pivoting at the picture point (`ax`, `ay`), pointing `d` (layout), in `view`.
pub(crate) fn arm_shape(ax: f64, ay: f64, d: [f64; 3], view: &View) -> ArmShape {
    let len = (ARM_LENGTH * view.zoom).max(SHORTEST_ARM) * view.ratio;
    ArmShape {
        ax,
        ay,
        ex: d[0] * len,
        ey: -d[1] * len,
        half: ARM_HALF_WIDTH * view.ratio,
    }
}

impl ArmShape {
    /// The picture pixels it can touch.
    pub(crate) fn bounds(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        shape_box(self.ax, self.ay, self.ex, self.ey, self.half + 1.0, width, height)
    }

    /// How much of the picture pixel (`px`, `py`) it covers, 0 to 1.
    pub(crate) fn at(&self, px: usize, py: usize) -> f64 {
        let qx = px as f64 + 0.5 - self.ax;
        let qy = py as f64 + 0.5 - self.ay;
        let ee = self.ex * self.ex + self.ey * self.ey;
        let t = if ee > 0.0 {
            ((qx * self.ex + qy * self.ey) / ee).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rx = qx - t * self.ex;
        let ry = qy - t * self.ey;
        (self.half + 0.5 - (rx * rx + ry * ry).sqrt()).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            cx: 0.0,
            cy: 0.0,
            zoom: 10.0,
            width: 100.0,
            height: 100.0,
            ratio: 1.0,
        }
    }

    #[test]
    fn a_beam_up_is_bright_on_its_middle_line_fades_along_and_ends_in_a_cap() {
        let b = beam_shape(50.5, 80.0, [0.0, 1.0, 0.0], 6.0, 30.0, &view());
        // 60 pixels up; at the far end 60 × tan 15° ≈ 16 either side.
        assert!(b.at(50, 79) > 0.95);
        let middle = b.at(50, 49);
        assert!((middle - (1.0 - 0.6 * 30.5 / 60.0)).abs() < 0.05, "{middle}");
        assert_eq!(b.at(50, 85), 0.0, "nothing behind the head");
        assert_eq!(b.at(80, 49), 0.0, "nothing past its edge");
        assert!(b.at(50, 15) > 0.0, "a round cap past its end");
        let (x0, x1, y0, y1) = b.bounds(100, 100);
        assert!(x0 <= 33 && x1 >= 67 && y0 <= 3 && y1 >= 81);
    }

    #[test]
    fn a_beam_at_the_viewer_is_a_pool_round_the_head() {
        let b = beam_shape(50.0, 50.0, [0.0, 0.0, 1.0], 6.0, 30.0, &view());
        assert!(b.at(49, 49) > 0.3);
        assert!(b.at(60, 49) > 0.0);
        assert_eq!(b.at(70, 49), 0.0);
    }

    #[test]
    fn an_arm_covers_its_line_two_pixels_wide() {
        let a = arm_shape(50.0, 50.0, [1.0, 0.0, 0.0], &view());
        assert_eq!(a.at(55, 49), 1.0);
        assert_eq!(a.at(60, 52), 0.0);
        assert_eq!(a.at(50, 49), 1.0);
        assert_eq!(a.at(95, 49), 0.0, "10 pixels long, at the least");
    }
}
