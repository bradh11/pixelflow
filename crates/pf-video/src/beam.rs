//! Moving heads' beams and servos' arms, drawn the way the 2D previews draw them (the window's
//! `app/src/lib/beams.ts` and `stampPixels`, from the same numbers, to the same bits: the scenes
//! in `app/src/api/contracts/pixels/` hold the two to each other).
//!
//! A beam is drawn as the light would look at night, from the color the head shows in the frame
//! (dark while its shutter is shut), as bright as that color is, in its hue. It runs from the
//! head's cell the way its pan and tilt point it (seen from the front), `length` layout units
//! out, widening to `length · tan(angle / 2)` either side (the angle its zoom sets, if it has a
//! zoom channel; never narrower than `NARROWEST` screen pixels): `along` of the way out and `r` of
//! the way from its middle line to its edge, it is `(1 - r²)² × beam_strength(half width)` solid,
//! strong where it is narrow and soft where it is wide. Past `FADE_FROM` of the way it fades out,
//! except as much as it points at the viewer (`toward²`), whose end then shows as a round glow. A
//! beam pointing down that reaches the ground (y = 0) stops there and lights a flat pool on it. A
//! servo's arm turns from its cell in the fixture's front plane, `ARM_LENGTH` layout units long
//! (never under `SHORTEST_ARM` screen pixels), in `ARM_COLOR`.

use crate::dots::View;
use pf_engine::{PreviewAxis, PreviewMotion};
use pf_model::{arm_direction, beam_direction, cos_deg, sin_deg};

/// How solid a beam is at its narrowest.
const BEAM_PEAK: f64 = 0.9;
/// A beam this wide either side of its middle (screen pixels) is half as solid as at its
/// narrowest.
const BEAM_SOFT: f64 = 10.0;
/// How far out (of its drawn length) a beam that lands nowhere starts fading to nothing.
const FADE_FROM: f64 = 0.7;
/// How solid a pool of light on the ground is at its smallest.
const POOL_PEAK: f64 = 0.85;
/// A pool this wide either side of its middle (screen pixels) is half as solid as at its
/// smallest.
const POOL_SOFT: f64 = 24.0;
/// A pool on the ground, seen from the front, is drawn this much as deep as it is wide.
const POOL_FLAT: f64 = 0.3;
/// The most a pool is stretched by a beam slanting onto the ground.
const GRAZING: f64 = 0.2;
/// The narrowest a beam is drawn, either side of its middle, in screen pixels.
const NARROWEST: f64 = 1.5;
/// A servo's arm: its length in layout units, its shortest in screen pixels, and half its width
/// in screen pixels.
const ARM_LENGTH: f64 = 0.4;
const SHORTEST_ARM: f64 = 10.0;
const ARM_HALF_WIDTH: f64 = 1.0;
/// A servo's arm: red, green, blue.
pub(crate) const ARM_COLOR: [u8; 3] = [200, 200, 208];

/// How solid a beam is on its middle line where it is `half` screen pixels wide either side.
fn beam_strength(half: f64) -> f64 {
    BEAM_PEAK * BEAM_SOFT / (BEAM_SOFT + half)
}

/// How solid a pool of light is at its middle when it is `half` screen pixels wide either side.
fn pool_strength(half: f64) -> f64 {
    POOL_PEAK * POOL_SOFT / (POOL_SOFT + half)
}

/// How much of a beam's light is left `t` of the way out.
fn end_fade(t: f64) -> f64 {
    if t <= FADE_FROM {
        return 1.0;
    }
    let u = (t - FADE_FROM) / (1.0 - FADE_FROM);
    1.0 - u * u * (3.0 - 2.0 * u)
}

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

/// A light's color as a beam draws it: its hue at full and how bright it is (0-1); `None` when
/// it's dark.
pub(crate) fn beam_light([r, g, b]: [u8; 3]) -> Option<([u32; 3], f64)> {
    let most = u32::from(r.max(g).max(b));
    if most == 0 {
        return None;
    }
    let half = most / 2;
    let full = |c: u8| (u32::from(c) * 255 + half) / most;
    Some(([full(r), full(g), full(b)], f64::from(most) / 255.0))
}

/// Where a beam pointing `d` from `head_y` (layout) meets the ground, as how far along it (layout
/// units), or `None` when it doesn't within `length`.
fn ground_reach(head_y: f64, d: [f64; 3], length: f64) -> Option<f64> {
    if d[1].is_nan() || d[1] >= 0.0 {
        return None;
    }
    let s = (if head_y > 0.0 { head_y } else { 0.0 }) / -d[1];
    (s < length).then_some(s)
}

/// A pool of light in a picture: its middle and how far it reaches across and down, in picture
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pool {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
}

/// A beam in a picture: where it starts, how far and which way it runs, how wide its end is, how
/// narrow it may be, picture pixels to a screen pixel, how much it points at the viewer
/// (squared), and its pool on the ground when it lands on it, in picture pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BeamShape {
    ax: f64,
    ay: f64,
    ex: f64,
    ey: f64,
    w: f64,
    narrow: f64,
    ratio: f64,
    toward: f64,
    pool: Option<Pool>,
}

/// A beam from the picture point (`ax`, `ay`), a head `head_y` above the ground (layout),
/// pointing `d` (layout), `length` long and `angle_deg` wide, in `view`.
pub(crate) fn beam_shape(
    (ax, ay): (f64, f64),
    head_y: f64,
    d: [f64; 3],
    length: f64,
    angle_deg: f64,
    view: &View,
) -> BeamShape {
    let scale = view.zoom * view.ratio;
    let narrow = NARROWEST * view.ratio;
    let wide = sin_deg(angle_deg / 2.0) / cos_deg(angle_deg / 2.0);
    let toward = if d[2] > 0.0 { d[2] } else { 0.0 };
    let landed = ground_reach(head_y, d, length);
    let reach = landed.unwrap_or(length);
    let out = reach * scale;
    let ex = d[0] * out;
    let ey = -d[1] * out;
    let pool = landed.map(|_| {
        // The cone's footprint: as wide as the cone where it lands, stretched along its slant.
        let radius = reach * wide;
        let stretch = radius / (-d[1]).max(GRAZING);
        let h = (d[0] * d[0] + d[2] * d[2]).sqrt();
        let hx = if h > 0.0 { d[0] / h } else { 1.0 };
        let hz = if h > 0.0 { d[2] / h } else { 0.0 };
        let (sx, sz, rx, rz) = (stretch * hx, stretch * hz, radius * hx, radius * hz);
        let across = (sx * sx + rz * rz).sqrt();
        let deep = (sz * sz + rx * rx).sqrt();
        Pool {
            cx: ax + ex,
            cy: ay + ey,
            rx: (across * scale).max(narrow),
            ry: (deep * scale * POOL_FLAT).max(view.ratio),
        }
    });
    BeamShape {
        ax,
        ay,
        ex,
        ey,
        w: (out * wide).max(narrow),
        narrow,
        ratio: view.ratio,
        toward: toward * toward,
        pool,
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
    edges_box(
        (
            ax.min(ax + ex) - reach,
            ax.max(ax + ex) + reach,
            ay.min(ay + ey) - reach,
            ay.max(ay + ey) + reach,
        ),
        width,
        height,
    )
}

/// The picture pixels within `left..right` and `top..bottom` (picture pixels), in a picture
/// `width` by `height`: columns `x0..x1`, rows `y0..y1`.
fn edges_box(
    (left, right, top, bottom): (f64, f64, f64, f64),
    width: usize,
    height: usize,
) -> (usize, usize, usize, usize) {
    let low = |v: f64| v.floor().max(0.0) as usize;
    let high = |v: f64, most: usize| (v.ceil().max(0.0) as usize).min(most);
    (low(left), high(right, width), low(top), high(bottom, height))
}

impl BeamShape {
    /// The picture pixels it (and its pool) can touch.
    pub(crate) fn bounds(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let reach = self.w + 1.0;
        let mut edges = (
            self.ax.min(self.ax + self.ex) - reach,
            self.ax.max(self.ax + self.ex) + reach,
            self.ay.min(self.ay + self.ey) - reach,
            self.ay.max(self.ay + self.ey) + reach,
        );
        if let Some(p) = self.pool {
            edges = (
                edges.0.min(p.cx - p.rx - 1.0),
                edges.1.max(p.cx + p.rx + 1.0),
                edges.2.min(p.cy - p.ry - 1.0),
                edges.3.max(p.cy + p.ry + 1.0),
            );
        }
        edges_box(edges, width, height)
    }

    /// How solid it (and its pool) is at the middle of the picture pixel (`px`, `py`), as a share
    /// of a full light's (0 outside it).
    pub(crate) fn at(&self, px: usize, py: usize) -> f64 {
        let qx = px as f64 + 0.5 - self.ax;
        let qy = py as f64 + 0.5 - self.ay;
        let ee = self.ex * self.ex + self.ey * self.ey;
        let t = if ee > 0.0 {
            (qx * self.ex + qy * self.ey) / ee
        } else {
            1.0
        };
        let mut lit = 0.0;
        if (0.0..=1.0).contains(&t) {
            let rx = qx - t * self.ex;
            let ry = qy - t * self.ey;
            let half = (t * self.w).max(self.narrow);
            let r2 = (rx * rx + ry * ry) / (half * half);
            if r2 < 1.0 {
                let s = 1.0 - r2;
                let along = if self.pool.is_some() {
                    1.0
                } else {
                    self.toward + (1.0 - self.toward) * end_fade(t)
                };
                lit = s * s * along * beam_strength(half / self.ratio);
            }
        } else if t > 1.0 && self.pool.is_none() && self.toward > 0.0 {
            let rx = qx - self.ex;
            let ry = qy - self.ey;
            let r2 = (rx * rx + ry * ry) / (self.w * self.w);
            if r2 < 1.0 {
                let s = 1.0 - r2;
                lit = s * s * self.toward * beam_strength(self.w / self.ratio);
            }
        }
        if let Some(p) = self.pool {
            let dx = (px as f64 + 0.5 - p.cx) / p.rx;
            let dy = (py as f64 + 0.5 - p.cy) / p.ry;
            let q = dx * dx + dy * dy;
            if q < 1.0 {
                let s = 1.0 - q;
                let pool = s * s * pool_strength(p.rx / self.ratio);
                lit = lit + pool - lit * pool;
            }
        }
        lit
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
    fn a_beam_up_is_strong_where_narrow_softer_where_wide_and_fades_out_at_its_end() {
        let b = beam_shape((50.5, 80.0), 1.0, [0.0, 1.0, 0.0], 6.0, 30.0, &view());
        // 60 pixels up; at the far end 60 × tan 15° ≈ 16 either side.
        let near = b.at(50, 79);
        assert!(near > 0.7, "{near}");
        let middle = b.at(50, 49);
        assert!(middle > 0.3 && middle < near, "{middle}");
        let late = b.at(50, 24);
        assert!(late < middle * 0.8, "fading out: {late}");
        assert_eq!(b.at(50, 85), 0.0, "nothing behind the head");
        assert_eq!(b.at(80, 49), 0.0, "nothing past its edge");
        assert_eq!(b.at(50, 15), 0.0, "nothing past its end");
        let (x0, x1, y0, y1) = b.bounds(100, 100);
        assert!(x0 <= 33 && x1 >= 67 && y0 <= 3 && y1 >= 81);
    }

    #[test]
    fn a_beam_at_the_viewer_is_a_glow_round_the_head() {
        let b = beam_shape((50.0, 50.0), 1.0, [0.0, 0.0, 1.0], 6.0, 30.0, &view());
        assert!(b.at(49, 49) > 0.2);
        assert!(b.at(60, 49) > 0.0);
        assert_eq!(b.at(70, 49), 0.0);
    }

    #[test]
    fn a_beam_down_stops_at_the_ground_and_lights_a_flat_pool_there() {
        // A head 2 up pointing straight down: 20 pixels to the ground at y 70.
        let b = beam_shape((50.0, 50.0), 2.0, [0.0, -1.0, 0.0], 6.0, 30.0, &view());
        assert!(b.at(49, 65) > 0.3, "the beam, full to the ground");
        assert!(b.at(49, 69) > 0.4, "the pool");
        assert!(b.at(53, 69) > 0.0, "wider than it is deep");
        assert_eq!(b.at(49, 75), 0.0, "nothing below the ground");
        assert_eq!(b.at(49, 66).min(1.0), b.at(49, 66));
        // A head on the ground pointing up lands nowhere.
        assert!(
            beam_shape((50.0, 50.0), 0.0, [0.0, 1.0, 0.0], 6.0, 30.0, &view())
                .pool
                .is_none()
        );
    }

    #[test]
    fn a_light_is_drawn_in_its_hue_as_bright_as_it_is() {
        assert_eq!(beam_light([0, 0, 0]), None);
        assert_eq!(beam_light([255, 128, 0]), Some(([255, 128, 0], 1.0)));
        assert_eq!(beam_light([100, 50, 0]), Some(([255, 128, 0], 100.0 / 255.0)));
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
