//! Settings that change over an effect (xLights' value curves): a shape going between two values,
//! sampled each frame at the effect's own time (0 at its start, 1 at its end).

use serde::{Deserialize, Serialize};

/// The most points a custom curve has (an xLights curve has at most 201 positions, each of which
/// can be a step, so every xLights curve fits).
pub const MAX_CURVE_POINTS: usize = 512;
/// The fewest and most times a curve's shape repeats over its effect.
pub const MIN_CURVE_CYCLES: f32 = 0.1;
pub const MAX_CURVE_CYCLES: f32 = 100.0;

/// How a curve goes between its two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CurveShape {
    /// Straight from `from` to `to`.
    #[default]
    Ramp,
    /// From `from` to `to` and back, smoothly.
    Sine,
    /// `from` for half of each cycle, then `to`.
    Square,
    /// From `from` to `to`, then straight back.
    Saw,
    /// Through `points`.
    Custom,
}

impl CurveShape {
    pub const ALL: [CurveShape; 5] = [
        CurveShape::Ramp,
        CurveShape::Sine,
        CurveShape::Square,
        CurveShape::Saw,
        CurveShape::Custom,
    ];
}

/// A setting that changes over its effect.
// The value at time `t` is `from + (to - from) * level(t)`, where the shape gives `level` (0 to
// 1): a ramp is `t`; sine, square, and saw repeat `cycles` times; custom goes straight from point
// to point (`[t, level]`, sorted by `t`), holds the first and last levels before and after them,
// and two points at one time make a step (the later one counts from that time on).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Curve {
    pub shape: CurveShape,
    pub from: f32,
    pub to: f32,
    /// Repeats over the effect (sine, square, saw).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub cycles: f32,
    /// Custom: `[time 0-1, level 0-1]` pairs; level 0 is `from` and 1 is `to`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

impl Curve {
    /// A straight ramp from `from` to `to`.
    pub fn ramp(from: f32, to: f32) -> Self {
        Self::shaped(CurveShape::Ramp, from, to, 1.0)
    }

    /// `shape` between `from` and `to`, repeating `cycles` times.
    pub fn shaped(shape: CurveShape, from: f32, to: f32, cycles: f32) -> Self {
        Self {
            shape,
            from,
            to,
            cycles,
            points: Vec::new(),
        }
    }

    /// Through `points` (`[t, level]`), level 0 being `from` and 1 `to`.
    pub fn custom(from: f32, to: f32, points: Vec<[f32; 2]>) -> Self {
        Self {
            points,
            ..Self::shaped(CurveShape::Custom, from, to, 1.0)
        }
    }

    /// Where the curve is between `from` (0) and `to` (1) at time `t` (0–1 over the effect).
    pub fn level_at(&self, t: f32) -> f32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let cycles = if self.cycles.is_finite() {
            self.cycles.clamp(MIN_CURVE_CYCLES, MAX_CURVE_CYCLES)
        } else {
            1.0
        };
        let phase = || (f64::from(t) * f64::from(cycles)).fract() as f32;
        match self.shape {
            CurveShape::Ramp => t,
            CurveShape::Sine => {
                let x = f64::from(t) * f64::from(cycles) * std::f64::consts::TAU;
                (0.5 - 0.5 * x.cos()) as f32
            }
            CurveShape::Square => {
                if phase() < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            CurveShape::Saw => phase(),
            CurveShape::Custom => custom_level(&self.points, t),
        }
    }

    /// The setting's value at time `t` (0–1 over the effect).
    pub fn value_at(&self, t: f32) -> f32 {
        self.from + (self.to - self.from) * self.level_at(t)
    }

    /// Pulls the curve into a setting's range (`min` to `max`): values clamped (NaN becomes
    /// `min`), cycles kept to what's allowed, and points sorted, inside 0–1, and at most
    /// [`MAX_CURVE_POINTS`].
    pub fn sanitize(&mut self, min: f32, max: f32) {
        let fit = |v: f32| if v.is_nan() { min } else { v.clamp(min, max) };
        self.from = fit(self.from);
        self.to = fit(self.to);
        self.cycles = if self.cycles.is_nan() {
            1.0
        } else {
            self.cycles.clamp(MIN_CURVE_CYCLES, MAX_CURVE_CYCLES)
        };
        self.points.retain(|p| p.iter().all(|v| !v.is_nan()));
        for p in &mut self.points {
            *p = p.map(|v| v.clamp(0.0, 1.0));
        }
        // Stable, so the two points of a step keep their order.
        self.points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        self.points.truncate(MAX_CURVE_POINTS);
    }

    /// Why the curve doesn't fit a setting's range (`min` to `max`), if it doesn't.
    pub fn problem(&self, min: f32, max: f32) -> Option<String> {
        for (name, v) in [("starts", self.from), ("ends", self.to)] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Some(format!("{name} at {v}; use {min} to {max}"));
            }
        }
        if !(MIN_CURVE_CYCLES..=MAX_CURVE_CYCLES).contains(&self.cycles) {
            return Some(format!(
                "repeats {} times; use {MIN_CURVE_CYCLES} to {MAX_CURVE_CYCLES}",
                self.cycles
            ));
        }
        if self.points.len() > MAX_CURVE_POINTS {
            return Some(format!(
                "has {} points; use at most {MAX_CURVE_POINTS}",
                self.points.len()
            ));
        }
        if self.points.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
            return Some("has a point outside 0 to 1".into());
        }
        if self.points.windows(2).any(|w| w[1][0] < w[0][0]) {
            return Some("has points out of time order".into());
        }
        None
    }
}

/// A custom curve's level at `t`: straight between points, held before the first and after the
/// last; at a step (points sharing a time) the later point counts from that time on. No points:
/// level 0.
fn custom_level(points: &[[f32; 2]], t: f32) -> f32 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    // The last point at or before `t`.
    let after = points.partition_point(|p| p[0] <= t);
    if after == 0 {
        return first[1];
    }
    let a = points[after - 1];
    let Some(b) = points.get(after) else {
        return a[1];
    };
    let span = b[0] - a[0];
    if span <= 0.0 {
        return a[1];
    }
    a[1] + (b[1] - a[1]) * ((t - a[0]) / span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn shapes_go_between_from_and_to() {
        let ramp = Curve::ramp(10.0, 20.0);
        assert_eq!([0.0, 0.25, 1.0].map(|t| ramp.value_at(t)), [10.0, 12.5, 20.0]);
        let down = Curve::ramp(1.0, 0.0);
        assert!(close(down.value_at(0.75), 0.25));

        let sine = Curve::shaped(CurveShape::Sine, 0.0, 1.0, 2.0);
        assert!(close(sine.value_at(0.0), 0.0));
        assert!(close(sine.value_at(0.25), 1.0), "the top halfway through a cycle");
        assert!(close(sine.value_at(0.5), 0.0));
        assert!(close(sine.value_at(0.125), 0.5));

        let square = Curve::shaped(CurveShape::Square, 2.0, 8.0, 2.0);
        assert_eq!(
            [0.0, 0.2, 0.25, 0.49, 0.5, 0.8].map(|t| square.value_at(t)),
            [2.0, 2.0, 8.0, 8.0, 2.0, 8.0]
        );

        let saw = Curve::shaped(CurveShape::Saw, 0.0, 10.0, 4.0);
        assert!(close(saw.value_at(0.125), 5.0));
        assert!(close(saw.value_at(0.25), 0.0), "back down at each cycle");
    }

    #[test]
    fn custom_curves_go_point_to_point_and_step_where_points_share_a_time() {
        let c = Curve::custom(0.0, 100.0, vec![[0.2, 0.0], [0.5, 1.0], [0.5, 0.25], [1.0, 0.25]]);
        assert_eq!(c.value_at(0.0), 0.0, "held before the first point");
        assert!(close(c.value_at(0.35), 50.0));
        assert!(c.value_at(0.4999) > 99.0);
        assert_eq!(c.value_at(0.5), 25.0, "the later point counts at a step");
        assert_eq!(c.value_at(0.9), 25.0);
        assert_eq!(Curve::custom(3.0, 9.0, vec![]).value_at(0.5), 3.0);
    }

    #[test]
    fn sanitize_fits_a_curve_to_its_setting() {
        let mut c = Curve {
            shape: CurveShape::Custom,
            from: f32::NAN,
            to: 70.0,
            cycles: 0.0,
            points: vec![[0.5, 2.0], [f32::NAN, 0.0], [-1.0, 0.5]],
        };
        assert!(c.problem(0.0, 50.0).is_some());
        c.sanitize(0.0, 50.0);
        assert_eq!((c.from, c.to, c.cycles), (0.0, 50.0, MIN_CURVE_CYCLES));
        assert_eq!(c.points, vec![[0.0, 0.5], [0.5, 1.0]]);
        assert_eq!(c.problem(0.0, 50.0), None);
    }

    #[test]
    fn curves_read_and_write_compactly() {
        let json = serde_json::to_value(Curve::ramp(0.0, 1.0)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "shape": "ramp", "from": 0.0, "to": 1.0 })
        );
        let c: Curve =
            serde_json::from_str(r#"{ "shape": "custom", "from": 1, "to": 3, "points": [[0, 0], [1, 1]] }"#)
                .unwrap();
        assert_eq!(c.cycles, 1.0);
        assert_eq!(c.value_at(0.5), 2.0);
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Curve>(&text).unwrap(), c);
    }
}
