//! Moving heads and servos: how a position in degrees becomes a motor's channel values and back,
//! and which way a head's beam (or a servo's arm) points.
//!
//! A motor (a head's pan or tilt, or a servo) turns `range_deg` degrees as its value goes from 0
//! to its highest: 255 with one channel, 65535 with a fine channel after it (coarse × 256 +
//! fine). Home is `home_deg` degrees from the value-0 end, and every position is in degrees from
//! Home. A reversed motor turns the other way as its value rises. Limits clamp a position before
//! it becomes a value:
//!
//! - position `p` to value: `x = (home + s·clamp(p)) / range`, value `floor(x·full + 0.5)`
//!   (`s` is -1 reversed, else 1);
//! - value to position: `p = s·(value/full·range - home)`.
//!
//! A head points, in its own frame (x right, y up, z toward the street), along
//! `(sin tilt·sin pan, cos tilt, sin tilt·cos pan)`: Home is straight up, tilt leans the beam
//! forward toward the street, and pan swings that lean to the right as seen from the street. The
//! prop's rotation turns the head's frame into the layout's (a head hung upside down is turned
//! half round in the front plane). A servo's arm turns in the head's front plane, along
//! `(sin p, cos p, 0)`: up at Home, clockwise as seen from the street.
//!
//! The sine and cosine here are PixelFlow's own (`sin_deg`, `cos_deg`): the window's previews
//! work them out with the same steps (`app/src/lib/beams.ts`), so a preview and an exported video
//! draw a beam to the same bits.

use serde::{Deserialize, Serialize};

/// The most degrees a motor's whole travel may be.
pub const MAX_MOTOR_RANGE: f32 = 1080.0;

/// How a motor (a head's pan or tilt, or a servo) turns with its value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxMotor {
    /// Degrees it turns as its value goes from 0 to its highest.
    pub range_deg: f32,
    /// Where Home is (a head: facing forward, pointing up), in degrees from the value-0 end.
    pub home_deg: f32,
    /// The farthest it may turn from Home one way (0 or less); left out, the end of its travel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_deg: Option<f32>,
    /// The farthest it may turn from Home the other way (0 or more); left out, the end of its
    /// travel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_deg: Option<f32>,
    /// It turns the other way as its value rises.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
}

impl DmxMotor {
    /// A motor turning `range_deg` with Home in the middle, no limits.
    pub fn centred(range_deg: f32) -> Self {
        Self {
            range_deg,
            home_deg: range_deg / 2.0,
            min_deg: None,
            max_deg: None,
            reverse: false,
        }
    }

    /// The usual motor for a channel of `role`: pan 540°, tilt 270°, a servo 180°, each with
    /// Home in the middle.
    pub fn default_for(role: crate::DmxRole) -> Self {
        use crate::DmxRole;
        match role {
            DmxRole::Pan | DmxRole::PanFine => Self::centred(540.0),
            DmxRole::Tilt | DmxRole::TiltFine => Self::centred(270.0),
            _ => Self::centred(180.0),
        }
    }

    fn sign(&self) -> f64 {
        if self.reverse { -1.0 } else { 1.0 }
    }

    /// Where its travel ends each way, in degrees from Home (low, high).
    pub fn travel(&self) -> (f64, f64) {
        let (range, home) = (f64::from(self.range_deg), f64::from(self.home_deg));
        if self.reverse {
            (home - range, home)
        } else {
            (-home, range - home)
        }
    }

    /// The farthest it may turn each way, in degrees from Home: its limits within its travel.
    pub fn limits(&self) -> (f64, f64) {
        let (low, high) = self.travel();
        let lo = self.min_deg.map_or(low, |m| f64::from(m).max(low));
        let hi = self.max_deg.map_or(high, |m| f64::from(m).min(high));
        if lo <= hi { (lo, hi) } else { (hi, hi) }
    }

    /// The value it takes for `degrees` from Home (kept within its limits), as its coarse and
    /// fine bytes: 16-bit when `fine` (the fine byte is 0 otherwise).
    pub fn values(&self, degrees: f64, fine: bool) -> (u8, u8) {
        let (lo, hi) = self.limits();
        let p = if degrees.is_nan() {
            0.0
        } else {
            degrees.clamp(lo, hi)
        };
        let range = f64::from(self.range_deg);
        let x = if range > 0.0 {
            (f64::from(self.home_deg) + self.sign() * p) / range
        } else {
            0.0
        };
        let full = if fine { 65535.0 } else { 255.0 };
        let v = (x * full + 0.5).floor().clamp(0.0, full) as u32;
        if fine {
            ((v >> 8) as u8, (v & 255) as u8)
        } else {
            (v as u8, 0)
        }
    }

    /// Where it is, in degrees from Home, at the value `coarse` (and `fine`, for a 16-bit motor).
    pub fn degrees(&self, coarse: u8, fine: Option<u8>) -> f64 {
        let x = match fine {
            Some(f) => f64::from(u32::from(coarse) * 256 + u32::from(f)) / 65535.0,
            None => f64::from(coarse) / 255.0,
        };
        self.sign() * (x * f64::from(self.range_deg) - f64::from(self.home_deg))
    }

    /// Why these settings can't be used, in plain words.
    pub fn problem(&self) -> Option<String> {
        let finite = [
            Some(self.range_deg),
            Some(self.home_deg),
            self.min_deg,
            self.max_deg,
        ]
        .into_iter()
        .flatten()
        .all(f32::is_finite);
        if !finite {
            return Some("A motor's degrees must be numbers.".into());
        }
        if !(self.range_deg > 0.0 && self.range_deg <= MAX_MOTOR_RANGE) {
            return Some(format!(
                "A motor turns more than 0 and at most {MAX_MOTOR_RANGE}°, so {}° won't work.",
                self.range_deg
            ));
        }
        if !(0.0..=self.range_deg).contains(&self.home_deg) {
            return Some(format!(
                "Home is within the motor's {}°, so {}° won't work.",
                self.range_deg, self.home_deg
            ));
        }
        if self.min_deg.is_some_and(|m| m > 0.0) || self.max_deg.is_some_and(|m| m < 0.0) {
            return Some(
                "A motor's limits are on either side of Home: the lower 0° or less, the upper 0° or more."
                    .into(),
            );
        }
        None
    }
}

/// A slot of a colour wheel or a gobo wheel: the value that selects it, and its colour or name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxSlot {
    pub value: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<crate::Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The most slots one wheel may have.
pub const MAX_DMX_SLOTS: usize = 64;

/// A moving head's beam as the previews draw it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxBeam {
    /// How wide the beam spreads, in degrees (the fixture's beam angle).
    pub angle_deg: f32,
    /// How far it is drawn, in layout units.
    pub length: f32,
}

impl Default for DmxBeam {
    fn default() -> Self {
        Self {
            angle_deg: 15.0,
            length: 6.0,
        }
    }
}

impl DmxBeam {
    /// Why these settings can't be used, in plain words.
    pub fn problem(&self) -> Option<String> {
        if !(self.angle_deg.is_finite() && (1.0..=120.0).contains(&self.angle_deg)) {
            return Some("A beam is 1° to 120° wide.".into());
        }
        if !(self.length.is_finite() && (0.5..=100.0).contains(&self.length)) {
            return Some("A beam is drawn 0.5 to 100 long.".into());
        }
        None
    }
}

/// How a zoom channel widens a head's beam: its width at value 0 and at 255, in degrees, and in
/// between in proportion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxZoom {
    pub from_deg: f32,
    pub to_deg: f32,
}

impl Default for DmxZoom {
    /// Narrow at 0 and wide at 255, as most zooming heads are.
    fn default() -> Self {
        Self {
            from_deg: 5.0,
            to_deg: 45.0,
        }
    }
}

impl DmxZoom {
    /// How wide the beam is at `value`, in degrees.
    pub fn angle(&self, value: u8) -> f64 {
        let (from, to) = (f64::from(self.from_deg), f64::from(self.to_deg));
        from + (to - from) * f64::from(value) / 255.0
    }

    /// Why these settings can't be used, in plain words.
    pub fn problem(&self) -> Option<String> {
        let ok = |a: f32| a.is_finite() && (1.0..=120.0).contains(&a);
        (!(ok(self.from_deg) && ok(self.to_deg))).then(|| "A zoom widens a beam to 1° to 120°.".into())
    }
}

/// The sine of `degrees`, by PixelFlow's own steps (the window's `sinDeg` takes the same ones):
/// brought to within 45° of a quarter turn, then a fixed series. Within 2e-9 of the true sine.
pub fn sin_deg(degrees: f64) -> f64 {
    let (q, t) = quarter(degrees);
    match q {
        0 => sin_series(t),
        1 => cos_series(t),
        2 => -sin_series(t),
        _ => -cos_series(t),
    }
}

/// The cosine of `degrees`, by PixelFlow's own steps (see [`sin_deg`]).
pub fn cos_deg(degrees: f64) -> f64 {
    let (q, t) = quarter(degrees);
    match q {
        0 => cos_series(t),
        1 => -sin_series(t),
        2 => -cos_series(t),
        _ => sin_series(t),
    }
}

/// `degrees` as the nearest quarter turn (0-3) and the radians left over (within ±45°).
fn quarter(degrees: f64) -> (u8, f64) {
    if !degrees.is_finite() {
        return (0, 0.0);
    }
    let turns = (degrees / 90.0 + 0.5).floor();
    let rest = degrees - turns * 90.0;
    let q = (turns - (turns / 4.0).floor() * 4.0) as u8;
    (q, rest * (std::f64::consts::PI / 180.0))
}

fn sin_series(t: f64) -> f64 {
    let t2 = t * t;
    t * (1.0 - t2 / 6.0 * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0 * (1.0 - t2 / 72.0 * (1.0 - t2 / 110.0)))))
}

fn cos_series(t: f64) -> f64 {
    let t2 = t * t;
    1.0 - t2 / 2.0
        * (1.0 - t2 / 12.0 * (1.0 - t2 / 30.0 * (1.0 - t2 / 56.0 * (1.0 - t2 / 90.0 * (1.0 - t2 / 132.0)))))
}

/// The fixture's own frame in the layout: where its right, up and front (toward the street)
/// point, as three columns of a row-major 3 × 3 matrix.
pub type Basis = [f64; 9];

/// The frame of a fixture that isn't turned.
pub const UPRIGHT: Basis = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn turn(b: &Basis, v: [f64; 3]) -> [f64; 3] {
    [
        b[0] * v[0] + b[1] * v[1] + b[2] * v[2],
        b[3] * v[0] + b[4] * v[1] + b[5] * v[2],
        b[6] * v[0] + b[7] * v[1] + b[8] * v[2],
    ]
}

/// Which way a head points in the layout for `pan` and `tilt` (degrees from Home), its frame
/// being `basis`.
pub fn beam_direction(pan: f64, tilt: f64, basis: &Basis) -> [f64; 3] {
    let (sp, cp, st, ct) = (sin_deg(pan), cos_deg(pan), sin_deg(tilt), cos_deg(tilt));
    turn(basis, [st * sp, ct, st * cp])
}

/// Which way a servo's arm points in the layout at `degrees` from Home, its frame being `basis`.
pub fn arm_direction(degrees: f64, basis: &Basis) -> [f64; 3] {
    turn(basis, [sin_deg(degrees), cos_deg(degrees), 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_become_values_and_back_on_a_16_bit_pan() {
        let pan = DmxMotor::centred(540.0);
        assert_eq!(pan.values(0.0, true), (128, 0), "Home is the middle");
        assert_eq!(pan.values(90.0, true), (170, 170));
        assert_eq!(pan.values(-270.0, true), (0, 0));
        assert_eq!(pan.values(270.0, true), (255, 255));
        assert_eq!(pan.values(999.0, true), (255, 255), "past the end of its travel");
        assert_eq!(pan.values(f64::NAN, true), (128, 0));
        for degrees in [-270.0, -100.25, 0.0, 33.3, 270.0] {
            let (c, f) = pan.values(degrees, true);
            let back = pan.degrees(c, Some(f));
            assert!(
                (back - degrees).abs() <= 540.0 / 65535.0,
                "{degrees} came back as {back}"
            );
        }
    }

    #[test]
    fn an_8_bit_tilt_reversed_turns_the_other_way() {
        let mut tilt = DmxMotor::centred(270.0);
        assert_eq!(tilt.values(0.0, false), (128, 0));
        assert_eq!(tilt.values(45.0, false), (170, 0));
        tilt.reverse = true;
        assert_eq!(tilt.values(45.0, false), (85, 0));
        assert_eq!(tilt.travel(), (-135.0, 135.0));
        assert!((tilt.degrees(85, None) - 45.0).abs() < 0.6);
    }

    #[test]
    fn limits_hold_a_position_within_them() {
        let servo = DmxMotor {
            min_deg: Some(-45.0),
            max_deg: Some(45.0),
            ..DmxMotor::centred(180.0)
        };
        assert_eq!(servo.limits(), (-45.0, 45.0));
        assert_eq!(servo.values(90.0, false), (191, 0));
        assert_eq!(servo.values(-90.0, false), (64, 0));
        // Limits past the travel are the travel; a home off-centre moves the travel.
        let off = DmxMotor {
            home_deg: 30.0,
            min_deg: Some(-90.0),
            ..DmxMotor::centred(180.0)
        };
        assert_eq!(off.limits(), (-30.0, 150.0));
        assert_eq!(off.values(-60.0, false), (0, 0));
    }

    #[test]
    fn motor_problems_are_explained() {
        assert_eq!(DmxMotor::centred(540.0).problem(), None);
        assert!(DmxMotor::centred(0.0).problem().is_some());
        assert!(DmxMotor::centred(2000.0).problem().is_some());
        let mut m = DmxMotor::centred(180.0);
        m.home_deg = 200.0;
        assert!(m.problem().unwrap().contains("Home"));
        m.home_deg = 90.0;
        m.min_deg = Some(10.0);
        assert!(m.problem().unwrap().contains("limits"));
        m.min_deg = Some(f32::NAN);
        assert!(m.problem().is_some());
    }

    #[test]
    fn the_sine_and_cosine_are_true_to_two_billionths() {
        for k in -2000..=2000 {
            let d = f64::from(k) * 0.37;
            let r = d.to_radians();
            assert!((sin_deg(d) - r.sin()).abs() < 2e-9, "sin {d}");
            assert!((cos_deg(d) - r.cos()).abs() < 2e-9, "cos {d}");
        }
        assert_eq!(sin_deg(0.0), 0.0);
        assert_eq!(cos_deg(0.0), 1.0);
        assert_eq!(sin_deg(90.0), 1.0);
        assert_eq!(cos_deg(180.0), -1.0);
        assert_eq!(sin_deg(f64::INFINITY), 0.0);
    }

    #[test]
    fn a_head_points_up_at_home_and_leans_toward_the_street_then_right() {
        let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(close(beam_direction(0.0, 0.0, &UPRIGHT), [0.0, 1.0, 0.0]));
        assert!(close(beam_direction(123.0, 0.0, &UPRIGHT), [0.0, 1.0, 0.0]));
        assert!(close(beam_direction(0.0, 90.0, &UPRIGHT), [0.0, 0.0, 1.0]));
        assert!(close(beam_direction(90.0, 90.0, &UPRIGHT), [1.0, 0.0, 0.0]));
        assert!(close(beam_direction(0.0, -90.0, &UPRIGHT), [0.0, 0.0, -1.0]));
        // Hung upside down: turned half round in the front plane.
        let hung: Basis = [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0];
        assert!(close(beam_direction(0.0, 0.0, &hung), [0.0, -1.0, 0.0]));
        assert!(close(beam_direction(90.0, 90.0, &hung), [-1.0, 0.0, 0.0]));
        assert!(close(arm_direction(90.0, &UPRIGHT), [1.0, 0.0, 0.0]));
        assert!(close(arm_direction(0.0, &UPRIGHT), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn a_zoom_widens_the_beam_in_proportion_to_its_value() {
        let zoom = DmxZoom::default();
        assert_eq!(zoom.angle(0), 5.0);
        assert_eq!(zoom.angle(255), 45.0);
        assert!((zoom.angle(128) - 25.08).abs() < 0.01);
        // A zoom may narrow as its value rises.
        let narrowing = DmxZoom {
            from_deg: 60.0,
            to_deg: 10.0,
        };
        assert_eq!(narrowing.angle(255), 10.0);
        assert_eq!(zoom.problem(), None);
        assert!(
            DmxZoom {
                from_deg: 0.0,
                to_deg: 10.0
            }
            .problem()
            .is_some()
        );
        assert!(
            DmxZoom {
                from_deg: 5.0,
                to_deg: f32::NAN
            }
            .problem()
            .is_some()
        );
    }

    #[test]
    fn settings_save_only_what_they_set() {
        let json = serde_json::to_value(DmxMotor::centred(540.0)).unwrap();
        assert_eq!(json, serde_json::json!({ "rangeDeg": 540.0, "homeDeg": 270.0 }));
        let slot = DmxSlot {
            value: 16,
            color: Some(crate::Rgb::RED),
            name: None,
        };
        let json = serde_json::to_value(&slot).unwrap();
        assert_eq!(json, serde_json::json!({ "value": 16, "color": "#ff0000" }));
        assert_eq!(serde_json::from_value::<DmxSlot>(json).unwrap(), slot);
    }
}
