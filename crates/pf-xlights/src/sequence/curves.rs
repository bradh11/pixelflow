//! xLights value curves (`Active=TRUE|Type=Ramp|Min=0.00|Max=360.00|P1=…|RV=TRUE|`): settings
//! that change over an effect.
//!
//! Follows xLights' `ValueCurve.cpp`. Every type but the music and timing-track ones is worked
//! out there into points on a grid of 200 steps across the effect, joined by straight lines (or
//! held, for a wrapped point), and read at the effect's progress shifted by the time offset. The
//! same is done here, so [`XlCurve::ticks`] gives the curve's exact value either side of each
//! grid step: between steps it's a straight line, so those values are the whole curve. The music
//! and timing-track types need the song's loudness or the timing marks while rendering;
//! [`XlCurve::middle`] is the value they're held at instead.

/// Grid steps across an effect (xLights' `VC_X_POINTS`).
pub const STEPS: usize = 200;

/// What a curve that can't be worked out ahead follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driven {
    Music,
    TimingTrack,
}

/// One active xLights value curve, as written.
#[derive(Debug, Clone, PartialEq)]
pub struct XlCurve {
    kind: String,
    id: String,
    min: f32,
    max: f32,
    /// P1–P4, in the setting's units when `real_values`, else 0–100 of its range.
    p: [f32; 4],
    real_values: bool,
    wrap: bool,
    /// P3 and P4 are start and end levels (`SE=Y`, for the parabolic, logarithmic, and
    /// exponential types).
    start_end: bool,
    /// Percent of the effect the curve is read ahead by (`TO`).
    time_offset: i32,
    /// Custom and Random points (`Values=x:y;…`).
    values: Vec<(f32, f32)>,
}

/// One point of xLights' curve: `x` on the grid, `y` 0–1, and whether it was wrapped (the line
/// into it is held at its value instead).
#[derive(Debug, Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    wrapped: bool,
}

/// xLights' `vcSortablePoint::Normalise`: `x` to the nearest grid step.
fn grid(x: f32) -> f32 {
    (x * STEPS as f32).round() / STEPS as f32
}

fn safe01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Whether parameter `n` (1–4) of `kind` is in the setting's own range (xLights' `MINVOID`) or
/// a fixed one, given as `(low, high)`.
fn param_range(n: usize, kind: &str) -> Option<(f32, f32)> {
    const OWN: [&str; 13] = [
        "Flat",
        "Random",
        "Ramp",
        "Ramp Up/Down",
        "Ramp Up/Down Hold",
        "Saw Tooth",
        "Timing Track Toggle",
        "Timing Track Fade Fixed",
        "Timing Track Fade Proportional",
        "Music",
        "Inverted Music",
        "Music Trigger Fade",
        "Square",
    ];
    const LEVELS: [&str; 6] = [
        "Parabolic Down",
        "Parabolic Up",
        "Logarithmic Up",
        "Logarithmic Down",
        "Exponential Up",
        "Exponential Down",
    ];
    let own = match n {
        1 => {
            if kind == "Custom" {
                return Some((1.0, 10.0));
            }
            OWN.contains(&kind)
        }
        2 => {
            (OWN.contains(&kind) && kind != "Flat")
                || LEVELS.contains(&kind)
                || matches!(kind, "Sine" | "Abs Sine")
        }
        3 => match kind {
            "Ramp Up/Down" => true,
            "Random" => return Some((1.0, STEPS as f32)),
            "Music" | "Inverted Music" => return Some((-100.0, 100.0)),
            "Timing Track Fade Fixed" => return Some((1.0, 1000.0)),
            "Timing Track Fade Proportional" => return Some((1.0, 100.0)),
            _ => LEVELS.contains(&kind),
        },
        _ => LEVELS.contains(&kind) || matches!(kind, "Sine" | "Abs Sine"),
    };
    if own { None } else { Some((0.0, 100.0)) }
}

impl XlCurve {
    /// Reads a value curve; `None` unless it's active (as `ValueCurve::Deserialise` reads it).
    pub fn parse(text: &str) -> Option<Self> {
        Self::parse_in(text, 0.0, 100.0)
    }

    /// [`XlCurve::parse`] for a setting whose range is `min` to `max` when the curve doesn't
    /// say.
    pub fn parse_in(text: &str, min: f32, max: f32) -> Option<Self> {
        if !text.contains('|') {
            return None;
        }
        let mut c = XlCurve {
            kind: "Flat".into(),
            id: String::new(),
            min,
            max,
            p: [0.0; 4],
            real_values: false,
            wrap: false,
            start_end: false,
            time_offset: 0,
            values: Vec::new(),
        };
        let num = |v: &str| super::leading_number(v).unwrap_or(0.0) as f32;
        let mut active = true;
        for part in text.split('|') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            match key {
                "Active" => active = value != "FALSE",
                "Id" => c.id = value.to_string(),
                "Type" => c.kind = value.to_string(),
                "Min" => c.min = num(value),
                "Max" => c.max = num(value),
                "P1" => c.p[0] = num(value),
                "P2" => c.p[1] = num(value),
                "P3" => c.p[2] = num(value),
                "P4" => c.p[3] = num(value),
                "TO" => c.time_offset = super::leading_number(value).unwrap_or(0.0) as i32,
                "WRAP" => c.wrap = true,
                "RV" => c.real_values = true,
                "SE" => c.start_end = true,
                "Values" => {
                    c.values = value
                        .split(';')
                        .filter_map(|v| {
                            let (x, y) = v.split_once(':')?;
                            Some((grid(num(x)), num(y)))
                        })
                        .take(4 * STEPS)
                        .collect();
                }
                _ => {}
            }
        }
        if !active || !c.min.is_finite() || !c.max.is_finite() || c.max <= c.min {
            return None;
        }
        // Old files store parameters as 0–100 of the setting's range: put them in its units, as
        // `FixChangedScale` does when xLights opens them.
        if !c.real_values {
            let levels = matches!(
                c.kind.as_str(),
                "Exponential Up"
                    | "Exponential Down"
                    | "Logarithmic Up"
                    | "Logarithmic Down"
                    | "Parabolic Down"
                    | "Parabolic Up"
            );
            for n in 1..=4 {
                if param_range(n, &c.kind).is_none() && (n < 3 || !levels || c.start_end) {
                    c.p[n - 1] = c.p[n - 1] * (c.max - c.min) / 100.0 + c.min;
                }
            }
        }
        Some(c)
    }

    /// What the curve follows, when it can't be worked out ahead.
    pub fn driven(&self) -> Option<Driven> {
        match self.kind.as_str() {
            "Music" | "Inverted Music" | "Music Trigger Fade" => Some(Driven::Music),
            k if k.starts_with("Timing Track") => Some(Driven::TimingTrack),
            _ => None,
        }
    }

    /// Where a music or timing-track curve's value goes, as a level (0–1) of the range: between
    /// P1 and P2.
    fn driven_levels(&self) -> (f32, f32) {
        let level = |v: f32| safe01((v - self.min) / (self.max - self.min));
        (level(self.p[0]), level(self.p[1]))
    }

    /// The middle of a music or timing-track curve's values, in the setting's units (what it's
    /// held at).
    pub fn middle(&self) -> f64 {
        let (a, b) = self.driven_levels();
        self.output((a + b) / 2.0)
    }

    /// A level (0–1) in the setting's units (xLights' `GetOutputValueAt`).
    fn output(&self, level: f32) -> f64 {
        f64::from(self.min) + f64::from(self.max - self.min) * f64::from(level)
    }

    /// Parameter `n` (1–4) as 0–100 (xLights' `Normalise`).
    fn normalised(&self, n: usize) -> f32 {
        let (low, high) = param_range(n, &self.kind).unwrap_or((self.min, self.max));
        let v = self.p[n - 1];
        let res = if low != 0.0 || high != 100.0 {
            (v - low) * 100.0 / (high - low)
        } else {
            v
        };
        res.clamp(0.0, 100.0)
    }

    /// The curve's points, as xLights' `RenderType` builds them (sorted by `x`, keeping the
    /// order of points that share one).
    fn points(&self) -> Vec<Point> {
        let [p1, p2, p3, p4] = [1, 2, 3, 4].map(|n| self.normalised(n));
        let [raw1, raw2, raw3, raw4] = self.p;
        let mut points = Vec::new();
        let mut push = |x: f64, y: f32, wrapped: bool| {
            points.push(Point {
                x: grid(x as f32),
                y,
                wrapped,
            })
        };
        // Wraps `y` into 0–1 when the curve wraps; true if it did.
        let wrap = |y: &mut f32| {
            let mut wrapped = false;
            if self.wrap {
                while *y > 1.0 {
                    *y -= 1.0;
                    wrapped = true;
                }
                while *y < 0.0 {
                    *y += 1.0;
                    wrapped = true;
                }
            }
            wrapped
        };
        // The formula types step through the effect in `step`s (xLights adds in doubles, so the
        // last step lands just past 1 and is pulled back).
        let steps = |step: f64| {
            let mut xs = Vec::new();
            let mut i = 0.0f64;
            while i <= 1.01 {
                if i > 1.0 {
                    i = 1.0;
                }
                xs.push(i);
                i += step;
            }
            xs
        };
        let levels = |fy: f32| {
            if self.start_end {
                let (sn, en) = (p3 / 100.0, p4 / 100.0);
                safe01(sn + fy * (en - sn))
            } else {
                fy
            }
        };
        // xLights' own (shortened) value of 2π, so the points come out the same.
        #[allow(clippy::approx_constant)]
        const PI2: f64 = 6.283185307;
        match self.kind.as_str() {
            "Flat" => {
                push(0.0, p1 / 100.0, false);
                push(1.0, p1 / 100.0, false);
            }
            "Ramp" => {
                push(0.0, p1 / 100.0, false);
                push(1.0, p2 / 100.0, false);
            }
            "Ramp Up/Down" => {
                push(0.0, p1 / 100.0, false);
                push(0.5, p2 / 100.0, false);
                push(1.0, p3 / 100.0, false);
            }
            "Ramp Up/Down Hold" => {
                push(0.0, p1 / 100.0, false);
                push(0.5 - (0.5 * f64::from(p3)) / 100.0, p2 / 100.0, false);
                push(0.5 + (0.5 * f64::from(p3)) / 100.0, p2 / 100.0, false);
                push(1.0, p1 / 100.0, false);
            }
            "Saw Tooth" => {
                let count = (raw3 as i32).max(1);
                let per = 1.0f32 / count as f32;
                push(0.0, p1 / 100.0, false);
                for i in 0..count {
                    push(f64::from(i as f32 * per + per / 2.0), p2 / 100.0, false);
                    push(f64::from((i + 1) as f32 * per), p1 / 100.0, false);
                }
            }
            "Square" => {
                let count = (raw3 as i32).max(1);
                let per = 1.0f32 / (2 * count) as f32;
                let mut low = true;
                for i in 0..2 * count {
                    let (mut f1, mut f2) = (i as f32 * per - 0.0001, i as f32 * per);
                    if grid(f1) != grid(f2) {
                        f1 = i as f32 * per;
                        f2 = i as f32 * per + 0.0001;
                    }
                    let (f1, f2) = (f64::from(f1), f64::from(f2));
                    if low {
                        if i != 0 {
                            push(f1, p2 / 100.0, false);
                        }
                        push(f2, p1 / 100.0, false);
                    } else {
                        push(f1, p1 / 100.0, false);
                        push(f2, p2 / 100.0, false);
                    }
                    low = !low;
                }
                push(1.0, p2 / 100.0, false);
            }
            "Parabolic Down" | "Parabolic Up" => {
                // Upside down for "Parabolic Up"; xLights takes the whole part of P1.
                let a = match (self.kind == "Parabolic Up", raw1 == 0.0) {
                    (false, true) => 1,
                    (false, false) => raw1 as i32,
                    (true, true) => -1,
                    (true, false) => (-raw1) as i32,
                };
                for i in steps(0.05) {
                    let mut y = (f64::from(a) * (i - 0.5) * (i - 0.5) + f64::from(p2) / 100.0) as f32;
                    let wrapped = wrap(&mut y);
                    push(i, levels(safe01(y)), wrapped);
                }
            }
            "Logarithmic Up" => {
                let a = if raw1 == 0.0 { 0.04 } else { p1 / 25.0 };
                for i in steps(0.05) {
                    let a = f64::from(a);
                    let mut y = ((f64::from(p2) - 50.0) / 50.0 + (a + a * i).ln() + 1.0) as f32;
                    let wrapped = wrap(&mut y);
                    push(i, levels(safe01(y)), wrapped);
                }
            }
            "Logarithmic Down" => {
                let a = if raw1 == 0.0 { 0.1 } else { p1 / 10.0 };
                for i in steps(0.05) {
                    let mut y =
                        ((f64::from(p2) - 50.0) / 50.0 + 1.5 - (f64::from(a) * i - 1.0).exp2()) as f32;
                    let wrapped = wrap(&mut y);
                    push(i, levels(safe01(y)), wrapped);
                }
            }
            "Exponential Up" | "Exponential Down" => {
                let a = f64::from(if raw1 == 0.0 { 0.1 } else { p1 / 10.0 });
                for i in steps(0.05) {
                    let rise = ((a * i).exp() - 1.0) / (a.exp() - 1.0);
                    let rise = if self.kind == "Exponential Up" {
                        rise
                    } else {
                        1.0 - rise
                    };
                    let mut y = ((f64::from(p2) - 50.0) / 50.0 + rise) as f32;
                    let wrapped = wrap(&mut y);
                    push(i, levels(safe01(y)), wrapped);
                }
            }
            "Sine" | "Abs Sine" => {
                let maxx = (PI2 * f64::from((p3 / 10.0).max(0.1))) as f32;
                for i in steps(0.025) {
                    let r = (i * f64::from(maxx) + (f64::from(p1) * PI2) / 100.0) as f32;
                    let mut y = if self.kind == "Sine" {
                        ((f64::from(p4) - 50.0) / 50.0 + f64::from(r.sin() * (p2.max(1.0) / 200.0)) + 0.5)
                            as f32
                    } else {
                        ((f64::from(p4) - 50.0) / 50.0 + f64::from((r.sin() * (p2.max(1.0) / 100.0)).abs()))
                            as f32
                    };
                    let wrapped = wrap(&mut y);
                    push(i, safe01(y), wrapped);
                }
            }
            "Decaying Sine" => {
                // xLights reads this one's parameters as written, not as 0–100.
                let maxx = (PI2 * f64::from((raw3 / 10.0).max(0.1))) as f32;
                for i in steps(0.025) {
                    let r = (i * f64::from(maxx) + (f64::from(raw1) * PI2) / 100.0) as f32;
                    let exponent = (-0.1 * i * f64::from(maxx)).exp() as f32;
                    let mut y = ((f64::from(raw4) - 50.0) / 50.0
                        + f64::from(exponent * r.cos() * (raw2.max(1.0) / 200.0))
                        + 0.5) as f32;
                    let wrapped = wrap(&mut y);
                    push(i, safe01(y), wrapped);
                }
            }
            "Random" if self.values.is_empty() => self.random_points(p1, p2, raw3, &mut push),
            "Random" | "Custom" => {
                for &(x, y) in &self.values {
                    push(f64::from(x), y, false);
                }
            }
            _ => {}
        }
        // Stable: points sharing an `x` keep their order.
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        points
    }

    /// A Random curve without saved points: xLights' generator, seeded from the curve's id and
    /// parameters so every render agrees.
    fn random_points(&self, p1: f32, p2: f32, raw3: f32, push: &mut impl FnMut(f64, f32, bool)) {
        let mut seed: u64 = 0xCBF2_9CE4_8422_2325;
        let mut mix = |bytes: &[u8]| {
            for &b in bytes {
                seed ^= u64::from(b);
                seed = seed.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        mix(self.id.as_bytes());
        for p in &self.p[..3] {
            mix(&p.to_ne_bytes());
        }
        let mut next01 = || {
            seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            (z >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
        };
        let (min, max) = (f64::from(p1) / 100.0, f64::from(p2) / 100.0);
        let points = (raw3.round() as i32).clamp(1, STEPS as i32);
        let value = |r: f64| (r * (max - min) + min) as f32;
        if points == 1 {
            let v = value(next01());
            push(0.0, v, false);
            push(1.0, v, false);
            return;
        }
        // xLights' check for a taken position walks the points in the order they were added and
        // stops at the first one past it.
        let mut xs = vec![0.0f32, 1.0];
        let v0 = value(next01());
        push(0.0, v0, false);
        let v1 = value(next01());
        push(1.0, v1, false);
        for _ in 2..points {
            let taken = |x: f32, xs: &[f32]| xs.iter().take_while(|&&p| p <= x).any(|&p| p == x);
            let mut x = grid(next01() as f32);
            while taken(x, &xs) {
                x = grid(next01() as f32);
            }
            xs.push(x);
            let v = value(next01());
            push(f64::from(x), v, false);
        }
    }

    /// The curve's value either side of each grid step across the effect (`STEPS + 1` of them),
    /// as levels (0–1) of its range: `(just before, from then on)`. Between steps the curve is a
    /// straight line. Music and timing-track curves are held at their middle.
    pub fn levels(&self) -> Vec<(f32, f32)> {
        if self.driven().is_some() {
            let (a, b) = self.driven_levels();
            return vec![((a + b) / 2.0, (a + b) / 2.0); STEPS + 1];
        }
        let shape = Shape::new(&self.points());
        // Read ahead by the time offset, wrapping past the end (xLights' `GetValueAt`).
        let shift = self.time_offset.saturating_mul(2);
        let n = STEPS as i32;
        (0..=n)
            .map(|o| {
                let q = o.saturating_add(shift);
                let before = if q > n {
                    shape.before(q - n)
                } else {
                    shape.before(q.max(0))
                };
                let after = if q >= n && o < n {
                    shape.after(q - n)
                } else {
                    shape.after(q.clamp(0, n))
                };
                (safe01(before), safe01(after))
            })
            .collect()
    }

    /// [`XlCurve::levels`] in the setting's units (xLights' slider units, before any divisor).
    pub fn values(&self) -> Vec<(f64, f64)> {
        self.levels()
            .into_iter()
            .map(|(a, b)| (self.output(a), self.output(b)))
            .collect()
    }
}

/// xLights' curve read the way `GetValueAt` reads its points, by grid step: the value just
/// before and from each step on, straight between steps.
struct Shape {
    /// `(step, just before, from then on)` at each step that has points, in order.
    knots: Vec<(i32, f32, f32)>,
}

impl Shape {
    fn new(points: &[Point]) -> Self {
        let mut knots: Vec<(i32, f32, f32)> = Vec::new();
        if points.len() < 2 {
            // xLights reads a curve without two points as 1.
            return Self {
                knots: vec![(0, 1.0, 1.0), (STEPS as i32, 1.0, 1.0)],
            };
        }
        let step = |p: &Point| (p.x * STEPS as f32).round() as i32;
        let mut i = 0;
        while i < points.len() {
            let at = step(&points[i]);
            let mut j = i;
            while j + 1 < points.len() && step(&points[j + 1]) == at {
                j += 1;
            }
            // Into this step: toward its first point. On from it: from its last point toward
            // the next step's first, or held at that one when it was wrapped.
            let after = match points.get(j + 1) {
                Some(next) if next.wrapped => next.y,
                _ => points[j].y,
            };
            knots.push((at, points[i].y, after));
            i = j + 1;
        }
        // Before the first point xLights carries on the line from the first point to the next.
        if let [(a, _, ay), (b, by, _), ..] = knots[..]
            && a > 0
        {
            let start = ay - (by - ay) * a as f32 / (b - a) as f32;
            knots.insert(0, (0, start, start));
        }
        Self { knots }
    }

    /// The value just before step `q`.
    fn before(&self, q: i32) -> f32 {
        self.read(q, true)
    }

    /// The value from step `q` on.
    fn after(&self, q: i32) -> f32 {
        self.read(q, false)
    }

    fn read(&self, q: i32, before: bool) -> f32 {
        let at = self.knots.partition_point(|k| k.0 < q);
        if let Some(&(s, into, on)) = self.knots.get(at)
            && s == q
        {
            return if before { into } else { on };
        }
        match (at.checked_sub(1).map(|i| self.knots[i]), self.knots.get(at)) {
            (Some((a, _, av)), Some(&(b, bv, _))) => av + (bv - av) * (q - a) as f32 / (b - a) as f32,
            (Some((_, _, av)), None) => av,
            (None, Some(&(_, bv, _))) => bv,
            (None, None) => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(text: &str) -> XlCurve {
        XlCurve::parse(text).unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    /// The value from step `o` on, in the setting's units.
    fn at(c: &XlCurve, o: usize) -> f64 {
        c.values()[o].1
    }

    #[test]
    fn inactive_and_broken_curves_are_left_out() {
        assert!(XlCurve::parse("Active=FALSE|").is_none());
        assert!(XlCurve::parse("ID_VALUECURVE_X").is_none());
        assert!(XlCurve::parse("Active=TRUE|Type=Ramp|Min=5|Max=5|").is_none());
        assert!(XlCurve::parse("Active=TRUE|Type=Ramp|Min=0.00|Max=10.00|").is_some());
    }

    #[test]
    fn ramps_go_between_p1_and_p2_in_real_or_old_units() {
        let real =
            curve("Active=TRUE|Id=ID_VALUECURVE_X|Type=Ramp|Min=0.00|Max=360.00|P1=90.00|P2=270.00|RV=TRUE|");
        assert!(
            close(at(&real, 0), 90.0) && close(at(&real, 100), 180.0) && close(real.values()[200].0, 270.0)
        );
        // Before real values, P1 and P2 were 0–100 of the range.
        let old = curve("Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=25.00|P2=75.00|");
        assert!(close(at(&old, 0), -50.0) && close(old.values()[200].0, 50.0));
        let up_down = curve("Active=TRUE|Type=Ramp Up/Down|Min=0|Max=100|P1=0|P2=100|P3=50|RV=TRUE|");
        assert!(
            close(at(&up_down, 50), 50.0)
                && close(at(&up_down, 100), 100.0)
                && close(at(&up_down, 150), 75.0)
        );
    }

    #[test]
    fn sine_matches_xlights_formula() {
        // The user's kind of curve: two cycles, starting at three quarters of a cycle, full range.
        let c = curve("Active=TRUE|Type=Sine|Min=-300.00|Max=300.00|P1=75.00|P2=300.00|P3=20.00|RV=TRUE|");
        for o in [0, 5, 25, 40, 65, 120, 200] {
            let i = o as f64 / 200.0;
            let r = i * 2.0 * std::f64::consts::TAU + 0.75 * std::f64::consts::TAU;
            let level = 0.5 + 0.5 * r.sin();
            assert!(
                (at(&c, o) - (-300.0 + 600.0 * level)).abs() < 0.01,
                "step {o}: {} vs {}",
                at(&c, o),
                -300.0 + 600.0 * level
            );
        }
        // Between xLights' points (every 5 steps) it's a straight line.
        let (a, b, mid) = (at(&c, 5), at(&c, 10), at(&c, 7));
        assert!(close(mid, a + (b - a) * 0.4));
    }

    #[test]
    fn square_and_saw_tooth_keep_their_sharp_edges() {
        let square = curve("Active=TRUE|Type=Square|Min=0|Max=10|P1=2|P2=8|P3=2|RV=TRUE|");
        let v = square.values();
        // Two cycles: 2 for a quarter, 8 for a quarter, and so on, stepping at the grid step.
        assert!(close(v[0].1, 2.0) && close(v[49].1, 2.0));
        assert!(
            close(v[50].0, 2.0) && close(v[50].1, 8.0),
            "a step at 50: {:?}",
            v[50]
        );
        assert!(close(v[99].1, 8.0) && close(v[100].1, 2.0) && close(v[150].1, 8.0));
        // xLights' "Saw Tooth" climbs to P2 halfway through each cycle and back down.
        let saw = curve("Active=TRUE|Type=Saw Tooth|Min=0|Max=100|P1=0|P2=100|P3=2|RV=TRUE|");
        let v = saw.values();
        assert!(close(v[0].1, 0.0) && close(v[25].1, 50.0) && close(v[50].1, 100.0));
        assert!(close(v[100].1, 0.0) && close(v[150].1, 100.0));
    }

    #[test]
    fn formula_types_follow_xlights() {
        let exp = curve("Active=TRUE|Type=Exponential Up|Min=0|Max=100|P1=10|P2=50|RV=TRUE|");
        let want = |i: f64| ((i).exp() - 1.0) / (1.0f64.exp() - 1.0) * 100.0;
        assert!(close(at(&exp, 0), 0.0) && (at(&exp, 100) - want(0.5)).abs() < 0.01);
        let para = curve("Active=TRUE|Type=Parabolic Down|Min=0|Max=100|P1=4|P2=0|RV=TRUE|");
        // y = 4(x - 0.5)²: 1 at the ends, 0 in the middle.
        assert!(close(at(&para, 0), 100.0) && close(at(&para, 100), 0.0) && close(at(&para, 50), 25.0));
        let flat = curve("Active=TRUE|Min=0|Max=200|P1=50|RV=TRUE|");
        assert!(
            flat.values()
                .iter()
                .all(|&(a, b)| close(a, 50.0) && close(b, 50.0))
        );
        let custom =
            curve("Active=TRUE|Type=Custom|Min=0|Max=10|Values=0.00:0.00;0.50:1.00;1.00:0.50|RV=TRUE|");
        assert!(close(at(&custom, 50), 5.0) && close(at(&custom, 100), 10.0) && close(at(&custom, 150), 7.5));
    }

    #[test]
    fn time_offset_reads_ahead_and_wraps() {
        let c = curve("Active=TRUE|Type=Ramp|Min=0|Max=100|P1=0|P2=100|TO=25|RV=TRUE|");
        let v = c.values();
        assert!(close(v[0].1, 25.0), "starts a quarter in");
        assert!(
            close(v[150].0, 100.0) && close(v[150].1, 0.0),
            "wraps back to the start"
        );
        assert!(close(v[200].0, 25.0));
    }

    #[test]
    fn wrapped_curves_hold_into_each_wrapped_point() {
        // Exponential up shifted up by half wraps past the top; xLights holds into such points.
        let c = curve("Active=TRUE|Type=Exponential Up|Min=0|Max=100|P1=10|P2=75|WRAP=TRUE|RV=TRUE|");
        let v = c.values();
        assert!(
            v.iter()
                .all(|&(a, b)| (0.0..=100.0).contains(&a) && (0.0..=100.0).contains(&b))
        );
        assert!(
            v.windows(2).any(|w| w[1].1 < w[0].1),
            "drops back down where it wraps"
        );
    }

    #[test]
    fn music_and_timing_curves_hold_their_middle() {
        let music = curve("Active=TRUE|Type=Music|Min=0.00|Max=200.00|P2=200.00|RV=TRUE|");
        assert_eq!(music.driven(), Some(Driven::Music));
        assert!(close(music.middle(), 100.0));
        assert!(
            music
                .values()
                .iter()
                .all(|&(a, b)| close(a, 100.0) && close(b, 100.0))
        );
        let timing = curve("Active=TRUE|Type=Timing Track Toggle|Min=0|Max=100|P1=20|P2=40|RV=TRUE|");
        assert_eq!(timing.driven(), Some(Driven::TimingTrack));
        assert!(close(timing.middle(), 30.0));
    }

    #[test]
    fn random_curves_are_the_same_every_time() {
        let text = "Active=TRUE|Id=ID_VALUECURVE_R|Type=Random|Min=0|Max=100|P1=10|P2=90|P3=8|RV=TRUE|";
        assert_eq!(curve(text).values(), curve(text).values());
        assert!(
            curve(text)
                .values()
                .iter()
                .all(|&(a, b)| (10.0..=90.0).contains(&a) && (10.0..=90.0).contains(&b))
        );
        // Saved points are used as they are.
        let saved = curve("Active=TRUE|Type=Random|Min=0|Max=100|P3=2|RV=TRUE|Values=0.00:0.20;1.00:0.60|");
        assert!(close(at(&saved, 100), 40.0));
    }
}
