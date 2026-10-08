//! Translating one xLights effect (its name, settings, and palette) into a PixelFlow effect.
//!
//! Settings keys and defaults follow xLights' `src-core/effects/*Effect.cpp`; speeds that xLights
//! counts per effect ("cycles") become per-second rates using the effect's length.
//!
//! Settings that change over the effect (value curves) are translated the same way at each point
//! where they change, and every PixelFlow setting that changes as a result gets a curve through
//! those values (see [`param_curves`]).

use super::curves::{Driven, STEPS, XlCurve};
use super::settings::{ParsedPalette, Settings};
use super::{list, plural};
use pf_model::{BufferTransform, RenderStyle};
use pf_sequence::{
    Axis, BarsParams, Blend, ButterflyColors, ButterflyParams, ChaseParams, CirclesLook, CirclesParams,
    ColorWashParams, Curve, CurveShape, Direction, EffectParams, FaceColorSource, FaceEyes, FacesParams,
    FanParams, FireParams, GarlandShape, GarlandsDirection, GarlandsParams, Gradient, LifeParams, LifeRules,
    LinesParams, MAX_CURVE_CYCLES, MIN_CURVE_CYCLES, MeteorDirection, MeteorsParams, MorphParams, OffParams,
    OnParams, Palette, PinwheelParams, PinwheelShading, PinwheelStyle, PlasmaColors, PlasmaParams, Rgb,
    RippleParams, SettingRange, ShapeObject, ShapeParams, ShimmerParams, SnowflakeShape, SnowflakesMotion,
    SnowflakesParams, SpiralParams, StrobeParams, TendrilMovement, TendrilParams, TextCountdown,
    TextMovement, TextOrientation, TextParams, TwinkleParams, WaveParams,
};
use std::collections::BTreeMap;

/// Brightness of a placeholder (an effect PixelFlow can't draw yet), so it reads as a stand-in.
pub const PLACEHOLDER_LEVEL: f32 = 0.25;

/// How closely the PixelFlow effect matches the xLights one.
#[derive(Debug, Clone, PartialEq)]
pub enum Fidelity {
    Exact,
    /// What differs, in a few words each ("'Expand' direction shown as moving up").
    Approximate(Vec<String>),
    Placeholder,
    /// Left out: it changes other layers or drives fixtures, so a stand-in would be wrong.
    Skipped,
}

/// A translated effect, ready to place on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Translated {
    pub params: EffectParams,
    pub palette: Palette,
    pub blend: Blend,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
    pub sparkles: u32,
    pub sparkle_color: Rgb,
    /// PixelFlow's blur (xLights' Blur minus one).
    pub blur: u32,
    pub render_style: RenderStyle,
    pub buffer_transform: BufferTransform,
    /// Settings that change over the effect, by PixelFlow setting key.
    pub curves: BTreeMap<String, Curve>,
    pub fidelity: Fidelity,
}

/// Collects what differs while translating one effect.
struct Diff(Vec<String>);

impl Diff {
    fn add(&mut self, what: impl Into<String>) {
        let what = what.into();
        if !self.0.contains(&what) {
            self.0.push(what);
        }
    }
}

/// `a / b` for rates, 0 when `b` is 0.
fn per_second(cycles: f64, duration_ms: u64) -> f32 {
    if duration_ms == 0 {
        0.0
    } else {
        (cycles / (duration_ms as f64 / 1000.0)) as f32
    }
}

fn unit(v: f64) -> f32 {
    v.clamp(0.0, 1.0) as f32
}

/// A whole-number count from a setting, at least `min`.
fn count(v: f64, min: u32, max: u32) -> u32 {
    if v.is_finite() {
        (v.round().clamp(f64::from(min), f64::from(max))) as u32
    } else {
        min
    }
}

/// xLights settings whose text box shows the slider's value divided (`Bars_Cycles` 0-300 on the
/// slider is 0-30 cycles in the box), from xLights' effect metadata. Value curves run in slider
/// units.
const DIVISORS: [(&str, f64); 18] = [
    ("Bars_Cycles", 10.0),
    ("ColorWash_Cycles", 10.0),
    ("Fire_GrowthCycles", 10.0),
    ("Ripple_Outline", 10.0),
    ("Ripple_Spacing", 10.0),
    ("Ripple_Cycles", 10.0),
    ("Ripple_Twist", 10.0),
    ("Ripple_Velocity", 10.0),
    ("Shimmer_Cycles", 10.0),
    ("Chase_Rotations", 10.0),
    ("Chase_Offset", 10.0),
    ("Spirals_Rotation", 10.0),
    ("Spirals_Movement", 10.0),
    ("Number_Waves", 360.0),
    ("Wave_Speed", 100.0),
    ("Fan_Revolutions", 360.0),
    ("Garlands_Cycles", 10.0),
    ("Lines_Speed", 10.0),
];
/// Divided settings that xLights stores as the slider all the same.
const STORED_AS_SLIDER: [&str; 1] = ["Spirals_Rotation"];

/// Puts a value curve's value (slider units) in the effect setting `id`, under whichever keys the
/// file stores it as (the slider undivided, the text box divided).
fn put(s: &mut Settings, id: &str, value: f64) {
    let divisor = DIVISORS.iter().find(|(k, _)| *k == id).map_or(1.0, |(_, d)| *d);
    let [slider, text, spin] = ["E_SLIDER_", "E_TEXTCTRL_", "E_SPINCTRL_"].map(|p| format!("{p}{id}"));
    let mut stored = false;
    for (key, v) in [(&slider, value), (&text, value / divisor), (&spin, value)] {
        if s.contains(key) {
            s.set(key, v.to_string());
            stored = true;
        }
    }
    if !stored {
        if divisor != 1.0 && !STORED_AS_SLIDER.contains(&id) {
            s.set(&text, (value / divisor).to_string());
        } else {
            s.set(&slider, value.to_string());
        }
    }
}

fn driven_note(driven: Driven) -> &'static str {
    match driven {
        Driven::Music => "settings that follow the music held at their middle value",
        Driven::TimingTrack => "settings that follow a timing track held at their middle value",
    }
}

/// When to read curves (each a value just before and from each grid step on): the start, and
/// each step, twice where any of them jumps there. `(step, from it on)`.
fn sample_times(curves: &[&[(f64, f64)]]) -> Vec<(usize, bool)> {
    let mut times = vec![(0, true)];
    for step in 1..=STEPS {
        times.push((step, false));
        if step < STEPS && curves.iter().any(|c| c[step].0 != c[step].1) {
            times.push((step, true));
        }
    }
    times
}

/// A PixelFlow curve through `(time, value)` samples in time order (two at one time are a jump),
/// or `None` when the value never changes. Points on a straight line between their neighbours
/// are left out; a straight line from start to end is a ramp.
fn curve_through(samples: &[(f32, f32)]) -> Option<Curve> {
    // To 5 decimals, which is past what a setting shows and keeps files tidy.
    let samples: Vec<(f32, f32)> = samples
        .iter()
        .map(|&(t, v)| (t, ((f64::from(v) * 1e5).round() / 1e5) as f32))
        .collect();
    let (lo, hi) = samples
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &(_, v)| {
            (lo.min(v), hi.max(v))
        });
    let spread = hi - lo;
    if spread.is_nan() || spread <= 1e-6 * hi.abs().max(1.0) {
        return None;
    }
    let eps = (hi - lo) * 1e-5;
    let mut kept: Vec<(f32, f32)> = Vec::new();
    for &p in &samples {
        if kept.last() == Some(&p) {
            continue;
        }
        while let [.., a, b] = kept[..] {
            let on_line = a.0 < b.0 && b.0 < p.0 && {
                let along = a.1 + (p.1 - a.1) * (b.0 - a.0) / (p.0 - a.0);
                (b.1 - along).abs() <= eps
            };
            if !on_line {
                break;
            }
            kept.pop();
        }
        kept.push(p);
    }
    if let [(t0, a), (t1, b)] = kept[..]
        && t0 == 0.0
        && t1 == 1.0
    {
        return Some(Curve::ramp(a, b));
    }
    let level = |v: f32| ((v - lo) / (hi - lo) * 10_000.0).round() / 10_000.0;
    Some(Curve::custom(
        lo,
        hi,
        kept.iter().map(|&(t, v)| [t, level(v)]).collect(),
    ))
}

/// The curve for one effect-wide setting (sparkles, blur) from the xLights value curve on it,
/// `convert` turning each value (slider units) into PixelFlow's: the value at the start, and the
/// curve when it changes.
fn setting_curve(curve: &XlCurve, convert: impl Fn(f64) -> f32, diff: &mut Diff) -> (f32, Option<Curve>) {
    if let Some(driven) = curve.driven() {
        diff.add(driven_note(driven));
        return (convert(curve.middle()), None);
    }
    let values = curve.values();
    let samples: Vec<(f32, f32)> = sample_times(&[&values])
        .into_iter()
        .map(|(step, after)| {
            let (before, on) = values[step];
            (
                step as f32 / STEPS as f32,
                convert(if after { on } else { before }),
            )
        })
        .collect();
    (samples[0].1, curve_through(&samples))
}

/// The effect's own settings that change over it (`E_VALUECURVE_<id>`), by setting id.
fn effect_curves(s: &Settings) -> Vec<(String, XlCurve)> {
    let mut found: Vec<(String, XlCurve)> = s
        .keys()
        .filter_map(|key| {
            let id = key.strip_prefix("E_VALUECURVE_")?;
            Some((id.to_string(), XlCurve::parse(s.get(key)?)?))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// The curves for an effect whose xLights settings change over it (`moving`: setting id and its
/// values at each grid step; `s` holds their values at the start). The effect is translated
/// again at each step, and each PixelFlow number setting that changes gets a curve through its
/// values there. Notes from every step are kept.
#[allow(clippy::too_many_arguments)]
fn param_curves(
    name: &str,
    s: &Settings,
    moving: &[(String, Vec<(f64, f64)>)],
    colors: &[Rgb],
    duration_ms: u64,
    frame_ms: u32,
    base: &EffectParams,
    diff: &mut Diff,
) -> BTreeMap<String, Curve> {
    let kind = base.kind();
    let keys: Vec<&str> = kind
        .settings()
        .iter()
        .filter(|spec| matches!(spec.range, SettingRange::Number { .. } | SettingRange::Int { .. }))
        .map(|spec| spec.key)
        .collect();
    let values: Vec<&[(f64, f64)]> = moving.iter().map(|(_, v)| v.as_slice()).collect();
    let mut series: Vec<Vec<(f32, f32)>> = vec![Vec::new(); keys.len()];
    for (step, after) in sample_times(&values) {
        let mut at = s.clone();
        for (id, v) in moving {
            put(&mut at, id, if after { v[step].1 } else { v[step].0 });
        }
        let params = match effect_params(name, &at, colors, duration_ms, frame_ms, diff) {
            Kind::Params(p) if p.kind() == kind => p,
            _ => {
                diff.add("settings that change over the effect kept at one value");
                return BTreeMap::new();
            }
        };
        let clamped = params.sanitized();
        if clamped != params {
            diff.add(BEYOND_RANGE);
        }
        for (key, points) in keys.iter().zip(&mut series) {
            if let Some(v) = clamped.number(key) {
                points.push((step as f32 / STEPS as f32, v));
            }
        }
    }
    keys.iter()
        .zip(&series)
        .filter_map(|(key, points)| Some((key.to_string(), curve_through(points)?)))
        .collect()
}

/// xLights' On repeats its brightness ramp `On_Cycles` times over the effect: a saw from the
/// start brightness to the end, on both (so the brightness is the saw's).
fn on_cycles(s: &Settings, params: &EffectParams, curves: &mut BTreeMap<String, Curve>, diff: &mut Diff) {
    let EffectParams::On(p) = params else {
        return;
    };
    let cycles = Reader { s }.get("On_Cycles", 1.0, 0.0, 100.0) as f32;
    let ramps = (p.start_level - p.end_level).abs() > 1e-6;
    if !ramps || (cycles - 1.0).abs() <= 1e-6 {
        return;
    }
    if !(MIN_CURVE_CYCLES..=MAX_CURVE_CYCLES).contains(&cycles)
        || curves.contains_key("startLevel")
        || curves.contains_key("endLevel")
    {
        diff.add("repeating brightness ramp shown once");
        return;
    }
    let saw = Curve::shaped(CurveShape::Saw, p.start_level, p.end_level, cycles);
    curves.insert("startLevel".into(), saw.clone());
    curves.insert("endLevel".into(), saw);
}

const BEYOND_RANGE: &str = "settings beyond PixelFlow's range set to the nearest it allows";

/// What an xLights effect becomes.
enum Kind {
    Params(EffectParams),
    /// No PixelFlow equivalent: a dim placeholder.
    Placeholder,
    /// An effect that changes other layers or drives fixtures (DMX, servos): a fill would light
    /// the prop wrongly, so it's left out.
    Skip,
}

/// Reads an xLights effect's settings by control name (`Bars_Cycles`), whichever control type
/// the file stored it under. Values are in xLights' own units (its renderer reads them as is).
struct Reader<'s> {
    s: &'s Settings,
}

impl Reader<'_> {
    fn num(&self, id: &str) -> Option<f64> {
        ["E_SLIDER_", "E_TEXTCTRL_", "E_SPINCTRL_"]
            .iter()
            .find_map(|p| self.s.num(&format!("{p}{id}")))
    }

    /// A number clamped to a sane range for the setting (`default` when missing).
    fn get(&self, id: &str, default: f64, min: f64, max: f64) -> f64 {
        self.num(id).unwrap_or(default).clamp(min, max)
    }

    fn choice<'a>(&'a self, id: &str, default: &'a str) -> &'a str {
        self.s.text(&format!("E_CHOICE_{id}"), default)
    }

    fn check(&self, id: &str) -> bool {
        self.s.flag(&format!("E_CHECKBOX_{id}"), false)
    }

    /// A checkbox that's on unless the file says otherwise.
    fn check_or(&self, id: &str, default: bool) -> bool {
        self.s.flag(&format!("E_CHECKBOX_{id}"), default)
    }
}

/// `cycles` over the effect, as a per-second rate (finite and bounded).
fn rate(cycles: f64, duration_ms: u64) -> f32 {
    per_second(cycles, duration_ms).clamp(-1000.0, 1000.0)
}

fn direction(reverse: bool) -> Direction {
    if reverse {
        Direction::Reverse
    } else {
        Direction::Forward
    }
}

/// The xLights effect name without spaces, lowercased ("Color Wash" and "ColorWash" match).
fn effect_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

fn on(r: &Reader, diff: &mut Diff) -> EffectParams {
    let start = r.get("Eff_On_Start", 100.0, 0.0, 100.0);
    let end = r.get("Eff_On_End", 100.0, 0.0, 100.0);
    if r.check("On_Shimmer") {
        diff.add("shimmer not shown");
    }
    if r.get("On_Transparency", 0.0, 0.0, 100.0) > 0.0 {
        diff.add("transparency not applied");
    }
    EffectParams::On(OnParams {
        gradient: Gradient::None,
        start_level: unit(start / 100.0),
        end_level: unit(end / 100.0),
    })
}

fn color_wash(r: &Reader, colors: &[Rgb], diff: &mut Diff) -> EffectParams {
    let cycles = r.get("ColorWash_Cycles", 1.0, 0.1, 20.0);
    if cycles > 1.0 + 1e-6 && colors.len() > 1 {
        diff.add("repeated color cycles go back and forth instead of restarting");
    }
    if r.check("ColorWash_HFade") || r.check("ColorWash_VFade") {
        diff.add("fade toward the edges not shown");
    }
    if r.check("ColorWash_Shimmer") {
        diff.add("shimmer not shown");
    }
    if r.check("ColorWash_CircularPalette") {
        diff.add("circular palette not applied");
    }
    EffectParams::ColorWash(ColorWashParams {
        cycles: cycles as f32,
        gradient: Gradient::None,
    })
}

fn bars(r: &Reader, n_colors: f64, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    let repeats = r.get("Bars_BarCount", 1.0, 1.0, 5.0);
    let cycles = r.get("Bars_Cycles", 1.0, 0.0, 30.0);
    diff.add("bars have gaps between them");
    let (axis, reverse, moving) = match r.choice("Bars_Direction", "up") {
        "up" => (Axis::Vertical, false, true),
        "down" => (Axis::Vertical, true, true),
        "Right" => (Axis::Horizontal, false, true),
        "Left" => (Axis::Horizontal, true, true),
        other => {
            diff.add(format!("'{other}' direction shown as straight movement"));
            match other {
                "expand" | "Alternate Up" => (Axis::Vertical, false, true),
                "compress" | "Alternate Down" => (Axis::Vertical, true, true),
                "H-expand" | "Alternate Right" => (Axis::Horizontal, false, true),
                "H-compress" | "Alternate Left" => (Axis::Horizontal, true, true),
                "Custom Horz" => (Axis::Horizontal, false, false),
                "Custom Vert" => (Axis::Vertical, false, false),
                _ => {
                    // "Custom": the closest of up, down, left, and right to the angle.
                    let angle = r.get("Bars_Angle", 90.0, -180.0, 180.0);
                    if angle.abs() <= 45.0 {
                        (Axis::Horizontal, false, true)
                    } else if angle.abs() >= 135.0 {
                        (Axis::Horizontal, true, true)
                    } else {
                        (Axis::Vertical, angle < 0.0, true)
                    }
                }
            }
        }
    };
    if r.check("Bars_Highlight") || r.check("Bars_3D") || r.check("Bars_Gradient") {
        diff.add("highlight, 3D, or gradient look not shown");
    }
    EffectParams::Bars(BarsParams {
        count: count(repeats * n_colors, 1, 1000),
        speed: if moving { rate(cycles, duration_ms) } else { 0.0 },
        axis,
        direction: direction(reverse),
    })
}

fn single_strand(r: &Reader, duration_ms: u64, diff: &mut Diff) -> Kind {
    match r.s.text("E_NOTEBOOK_SSEFFECT_TYPE", "Chase") {
        "Skips" => {
            let band = r.get("Skips_BandSize", 1.0, 1.0, 20.0);
            let skip = r.get("Skips_SkipSize", 1.0, 0.0, 20.0);
            let advance = r.get("Skips_Advance", 0.0, 0.0, 100.0);
            let way = r.choice("Skips_Direction", "Left");
            diff.add("skips pattern shown as an evenly spaced chase");
            if way != "Left" && way != "Right" {
                diff.add("mirrored skips not shown");
            }
            Kind::Params(EffectParams::Chase(ChaseParams {
                speed: rate(advance / 10.0, duration_ms),
                width: unit(band / (band + skip)),
                bands: 10,
                direction: direction(way == "Right"),
                bounce: false,
            }))
        }
        "FX" => Kind::Placeholder,
        _ => {
            let chases = r.get("Number_Chases", 1.0, 1.0, 20.0);
            let size = r.get("Color_Mix1", 10.0, 1.0, 100.0);
            let rotations = r.get("Chase_Rotations", 1.0, 0.0, 50.0);
            let (reverse, bounce, moving) = match r.choice("Chase_Type1", "Left-Right") {
                "Left-Right" => (false, false, true),
                "Right-Left" => (true, false, true),
                "Bounce from Left" => (false, true, true),
                "Bounce from Right" => (true, true, true),
                other => {
                    diff.add(format!("'{other}' chase shown as a one-way chase"));
                    (other.contains("Right-Left"), false, !other.starts_with("Static"))
                }
            };
            if r.choice("SingleStrand_Colors", "Palette") == "Rainbow" {
                diff.add("rainbow colors shown in the palette colors");
            }
            if r.choice("Fade_Type", "None") != "None" || r.check("Chase_3dFade1") {
                diff.add("fading tail not shown");
            }
            if r.get("Chase_Offset", 0.0, -500.0, 500.0) != 0.0 {
                diff.add("start offset not applied");
            }
            if !r.choice("SingleStrand_TimingTrack", "").is_empty() {
                diff.add("timing-track pacing not applied");
            }
            Kind::Params(EffectParams::Chase(ChaseParams {
                speed: if moving { rate(rotations, duration_ms) } else { 0.0 },
                width: unit(size / 100.0 * chases),
                bands: count(chases, 1, 20),
                direction: direction(reverse),
                bounce,
            }))
        }
    }
}

fn marquee(r: &Reader, diff: &mut Diff) -> EffectParams {
    let band = r.get("Marquee_Band_Size", 3.0, 1.0, 100.0);
    let skip = r.get("Marquee_Skip_Size", 0.0, 0.0, 100.0);
    diff.add("shown as a chase along the pixels");
    EffectParams::Chase(ChaseParams {
        speed: (r.get("Marquee_Speed", 3.0, 0.0, 50.0) / 10.0) as f32,
        width: unit(if skip > 0.0 { band / (band + skip) } else { 0.5 }),
        bands: 10,
        direction: direction(r.check("Marquee_Reverse")),
        bounce: false,
    })
}

fn wave(r: &Reader, diff: &mut Diff) -> EffectParams {
    // Newer files store waves as cycles; older ones as degrees (900 = 2.5 waves).
    let cycles = match r.s.num("E_TEXTCTRL_Number_Waves") {
        Some(c) => c,
        None => r.s.num("E_SLIDER_Number_Waves").map_or(2.5, |d| d / 360.0),
    }
    .clamp(0.5, 10.0);
    // xLights moves the wave `speed` degrees every 50 ms.
    let speed = r.get("Wave_Speed", 10.0, 0.0, 50.0) * 20.0 / 360.0;
    let shape = r.choice("Wave_Type", "Sine");
    if shape != "Sine" {
        diff.add(format!("'{shape}' wave shown as a sine wave"));
    }
    if r.choice("Fill_Colors", "None") != "None" {
        diff.add("fill below the wave not shown");
    }
    if r.check("Mirror_Wave") {
        diff.add("mirrored wave not shown");
    }
    if r.get("Wave_YOffset", 0.0, -250.0, 250.0) != 0.0 {
        diff.add("vertical offset not applied");
    }
    EffectParams::Wave(WaveParams {
        cycles: cycles as f32,
        speed: speed as f32,
        height: unit(r.get("Wave_Height", 50.0, 0.0, 100.0) / 100.0),
        thickness: unit(r.get("Thickness_Percentage", 5.0, 0.0, 100.0) / 100.0),
        direction: direction(r.choice("Wave_Direction", "Right to Left") == "Right to Left"),
    })
}

fn twinkle(r: &Reader, frame: f64, diff: &mut Diff) -> EffectParams {
    let steps = r.get("Twinkle_Steps", 30.0, 2.0, 400.0);
    if r.check("Twinkle_Strobe") {
        diff.add("strobing twinkles shown as soft twinkles");
    }
    EffectParams::Twinkle(TwinkleParams {
        density: unit(r.get("Twinkle_Count", 3.0, 2.0, 100.0) / 100.0),
        // One twinkle (up and down) takes `steps` frames.
        rate: (1000.0 / (steps * frame)) as f32,
    })
}

fn shimmer(r: &Reader, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    if r.check("Shimmer_Use_All_Colors") {
        diff.add("a random color per pixel shown as one color at a time");
    }
    EffectParams::Shimmer(ShimmerParams {
        rate: rate(r.get("Shimmer_Cycles", 1.0, 0.0, 600.0), duration_ms),
        duty: unit(r.get("Shimmer_Duty_Factor", 50.0, 1.0, 100.0) / 100.0),
    })
}

fn strobe(r: &Reader, frame: f64, diff: &mut Diff) -> EffectParams {
    let frames = r.get("Strobe_Duration", 10.0, 1.0, 100.0);
    diff.add("number of flashes approximated");
    if r.get("Strobe_Type", 1.0, 1.0, 4.0) > 1.0 {
        diff.add("flash shapes shown as single pixels");
    }
    if r.check("Strobe_Music") {
        diff.add("music-driven flashes not applied");
    }
    EffectParams::Strobe(StrobeParams {
        // A new set of flashes every `frames` frames.
        rate: (1000.0 / (frames * frame)) as f32,
        density: unit(r.get("Number_Strobes", 3.0, 1.0, 300.0) / 100.0).max(0.01),
    })
}

fn spirals(r: &Reader, n_colors: f64, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    let repeats = r.get("Spirals_Count", 1.0, 1.0, 5.0);
    let movement = r.get("Spirals_Movement", 1.0, -20.0, 20.0);
    if r.check("Spirals_Blend") || r.check("Spirals_3D") {
        diff.add("blended or 3D look not shown");
    }
    if r.check("Spirals_Grow") || r.check("Spirals_Shrink") {
        diff.add("growing or shrinking arms not shown");
    }
    EffectParams::Spiral(SpiralParams {
        // One arm per palette color, repeated.
        count: count(repeats * n_colors, 1, 1000),
        speed: rate(movement.abs(), duration_ms),
        thickness: unit(r.get("Spirals_Thickness", 50.0, 0.0, 100.0) / 100.0),
        // Wraps around the prop from bottom to top. Unlike most effects, Spirals stores its
        // slider (tenths of a wrap), not the text box.
        twist: match r.s.num("E_SLIDER_Spirals_Rotation") {
            Some(tenths) => tenths / 10.0,
            None => r.s.num("E_TEXTCTRL_Spirals_Rotation").unwrap_or(2.0),
        }
        .clamp(-30.0, 30.0) as f32,
        direction: direction(movement < 0.0),
    })
}

fn fire(r: &Reader, diff: &mut Diff) -> EffectParams {
    if r.get("Fire_HueShift", 0.0, 0.0, 100.0) > 0.0 {
        diff.add("hue shift not applied");
    }
    if r.get("Fire_GrowthCycles", 0.0, 0.0, 20.0) > 0.0 || r.check("Fire_GrowWithMusic") {
        diff.add("growing flames shown at a steady height");
    }
    let location = r.choice("Fire_Location", "Bottom");
    if location != "Bottom" {
        diff.add(format!(
            "fire from the {} shown rising from the bottom",
            location.to_lowercase()
        ));
    }
    EffectParams::Fire(FireParams {
        height: unit(r.get("Fire_Height", 50.0, 1.0, 100.0) / 100.0),
        ..FireParams::default()
    })
}

fn meteors(r: &Reader, diff: &mut Diff) -> EffectParams {
    diff.add("meteor count and speed approximated");
    let direction = match r.choice("Meteors_Effect", "Down") {
        "Down" => MeteorDirection::Down,
        "Up" => MeteorDirection::Up,
        "Left" => MeteorDirection::Left,
        "Right" => MeteorDirection::Right,
        other => {
            diff.add(format!("'{other}' meteors shown falling down"));
            MeteorDirection::Down
        }
    };
    match r.choice("Meteors_Type", "Rainbow") {
        "Palette" => {}
        "Rainbow" => diff.add("rainbow meteors shown in the palette colors"),
        _ => diff.add("colors from the palette range shown as the palette colors"),
    }
    if r.get("Meteors_Swirl_Intensity", 0.0, 0.0, 20.0) > 0.0 {
        diff.add("swirl not shown");
    }
    EffectParams::Meteors(MeteorsParams {
        count: count(r.get("Meteors_Count", 10.0, 1.0, 100.0), 1, 100),
        speed: (r.get("Meteors_Speed", 10.0, 0.0, 50.0) / 10.0).max(0.05) as f32,
        length: unit(r.get("Meteors_Length", 25.0, 1.0, 100.0) / 100.0),
        direction,
    })
}

fn ripple(r: &Reader, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    let shape = r.choice("Ripple_Object_To_Draw", "Circle");
    if shape != "Circle" {
        diff.add(format!("'{shape}' ripples shown as circles"));
    }
    let movement = r.choice("Ripple_Movement", "Explode");
    if movement != "Explode" {
        diff.add(format!("'{movement}' movement shown as rings spreading out"));
    }
    let style = r.choice("Ripple_Draw_Style", "Old");
    if !style.starts_with("Old") && !style.starts_with("Lines") {
        diff.add("solid or highlighted rings shown as lines");
    }
    if r.check("Ripple3D") || r.get("Ripple_Twist", 0.0, -45.0, 45.0) != 0.0 {
        diff.add("3D or twist not shown");
    }
    if r.get("Ripple_XC", 0.0, -100.0, 100.0) != 0.0
        || r.get("Ripple_YC", 0.0, -100.0, 100.0) != 0.0
        || r.get("Ripple_Velocity", 0.0, 0.0, 30.0) != 0.0
    {
        diff.add("off-center or moving ripples shown from the center");
    }
    // Ripple_Thickness is how many rings show at once.
    let rings = r.get("Ripple_Thickness", 3.0, 1.0, 100.0);
    let cycles = if movement == "None" {
        0.0
    } else {
        r.get("Ripple_Cycles", 1.0, 0.0, 30.0)
    };
    EffectParams::Ripple(RippleParams {
        speed: rate(cycles, duration_ms),
        spacing: (1.0 / rings) as f32,
        ..RippleParams::default()
    })
}

/// True when the xLights version `version` ("2024.19") is older than `than` ("2025.04"), as
/// xLights' `IsVersionOlder` compares them. An unreadable version counts as current.
fn version_older(than: &str, version: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u32>> { v.trim().split('.').map(|p| p.parse().ok()).collect() };
    match (parts(than), parts(version)) {
        (Some(than), Some(version)) if !version.is_empty() => version < than,
        _ => false,
    }
}

/// Settings xLights changes when it opens a file saved by an older version (each effect's
/// `adjustSettings`), so the effect looks as it does in xLights today.
pub fn adjust_for_version(name: &str, s: &mut Settings, version: &str) {
    match effect_key(name).as_str() {
        // Fan radii became a share of the prop in 2025.04; older fans keep them in pixels.
        "fan" if version_older("2025.04", version) => s.set("E_CHECKBOX_Fan_Scale", "0".into()),
        // Pinwheels from before the new render method (2026.06) keep the old one.
        "pinwheel" if version_older("2026.06", version) && !s.contains("E_CHOICE_Pinwheel_Style") => {
            s.set("E_CHOICE_Pinwheel_Style", "Old Render Method".into());
        }
        // Lines' speed became a number with tenths in 2026.07; the old whole-number slider holds
        // the same speed.
        "lines" if version_older("2026.07", version) && !s.contains("E_TEXTCTRL_Lines_Speed") => {
            if let Some(speed) = s.get("E_SLIDER_Lines_Speed").map(str::to_string) {
                s.set("E_TEXTCTRL_Lines_Speed", speed);
            }
        }
        _ => {}
    }
}

/// What a Shape effect draws, from xLights' name for it.
fn shape_object(name: &str, diff: &mut Diff) -> ShapeObject {
    match name {
        "Circle" => ShapeObject::Circle,
        "Ellipse" => ShapeObject::Ellipse,
        "Triangle" => ShapeObject::Triangle,
        "Square" => ShapeObject::Square,
        "Pentagon" => ShapeObject::Pentagon,
        "Hexagon" => ShapeObject::Hexagon,
        "Octagon" => ShapeObject::Octagon,
        "Star" => ShapeObject::Star,
        "Heart" => ShapeObject::Heart,
        "Tree" => ShapeObject::Tree,
        "Snowflake" => ShapeObject::Snowflake,
        "Candy Cane" => ShapeObject::CandyCane,
        "Crucifix" => ShapeObject::Crucifix,
        "Present" => ShapeObject::Present,
        "Random" => ShapeObject::Random,
        other => {
            diff.add(format!("'{other}' shapes shown as circles"));
            ShapeObject::Circle
        }
    }
}

/// Shapes (xLights' `ShapeEffect`). xLights moves shapes so many pixels a frame; PixelFlow, a
/// second. A timing track is looked up by name when the effect is placed (see `Builder::effect`).
fn shape(r: &Reader, frame: f64, diff: &mut Diff) -> EffectParams {
    let use_timing = r.check("Shape_FireTiming") && !r.choice("Shape_FireTimingTrack", "").trim().is_empty();
    if !use_timing && r.check("Shape_UseMusic") {
        diff.add("shapes fired by the music shown as a steady stream");
    }
    if use_timing && !r.s.text("E_TEXTCTRL_Shape_FilterLabel", "").is_empty() {
        diff.add("shapes appear on every mark, not only the labels chosen");
    }
    let random_movement = r.check("Shapes_RandomMovement");
    if random_movement && (frame - 50.0).abs() > 0.5 {
        diff.add("random drift speeds approximated");
    }
    EffectParams::Shape(ShapeParams {
        shape: shape_object(r.choice("Shape_ObjectToDraw", "Circle"), diff),
        count: count(r.get("Shape_Count", 5.0, 1.0, 100.0), 1, 100),
        lifetime: r.get("Shape_Lifetime", 5.0, 1.0, 100.0) as f32,
        start_size: r.get("Shape_StartSize", 1.0, 0.0, 100.0) as f32,
        growth: r.get("Shape_Growth", 10.0, -100.0, 100.0) as f32,
        thickness: count(r.get("Shape_Thickness", 1.0, 1.0, 100.0), 1, 100),
        fade: r.check_or("Shape_FadeAway", true),
        random_location: r.check_or("Shape_RandomLocation", true),
        rotation: r.get("Shape_Rotation", 0.0, 0.0, 360.0) as f32,
        points: count(r.get("Shape_Points", 5.0, 2.0, 9.0), 2, 9),
        center_x: r.get("Shape_CentreX", 50.0, 0.0, 100.0) as f32,
        center_y: r.get("Shape_CentreY", 50.0, 0.0, 100.0) as f32,
        speed: (r.get("Shapes_Velocity", 0.0, 0.0, 20.0) * 1000.0 / frame) as f32,
        direction: r.get("Shapes_Direction", 90.0, 0.0, 359.0) as f32,
        random_movement,
        random_start: r.check_or("Shape_RandomInitial", true),
        timing_track: None,
    })
}

/// A Fan (xLights' `FanEffect`). Revolutions are stored in 360ths on the slider.
fn fan(r: &Reader) -> EffectParams {
    let revolutions = match r.s.num("E_SLIDER_Fan_Revolutions") {
        Some(ticks) => ticks / 360.0,
        None => r.s.num("E_TEXTCTRL_Fan_Revolutions").unwrap_or(2.0),
    };
    EffectParams::Fan(FanParams {
        center_x: r.get("Fan_CenterX", 50.0, 0.0, 100.0) as f32,
        center_y: r.get("Fan_CenterY", 50.0, 0.0, 100.0) as f32,
        start_radius: r.get("Fan_Start_Radius", 1.0, 0.0, 2500.0) as f32,
        end_radius: r.get("Fan_End_Radius", 50.0, 0.0, 2500.0) as f32,
        blades: count(r.get("Fan_Num_Blades", 3.0, 1.0, 16.0), 1, 16),
        blade_width: r.get("Fan_Blade_Width", 50.0, 5.0, 100.0) as f32,
        revolutions: revolutions.clamp(0.0, 10.0) as f32,
        blade_angle: r.get("Fan_Blade_Angle", 90.0, -360.0, 360.0) as f32,
        duration: r.get("Fan_Duration", 80.0, 0.0, 100.0) as f32,
        start_angle: r.get("Fan_Start_Angle", 0.0, 0.0, 360.0) as f32,
        elements: count(r.get("Fan_Num_Elements", 1.0, 1.0, 4.0), 1, 4),
        element_width: r.get("Fan_Element_Width", 100.0, 5.0, 100.0) as f32,
        acceleration: r.get("Fan_Accel", 0.0, -10.0, 10.0) as f32,
        direction: direction(r.check("Fan_Reverse")),
        blend_edges: r.check_or("Fan_Blend_Edges", true),
        scale: r.check_or("Fan_Scale", true),
    })
}

/// A Morph (xLights' `MorphEffect`). Linked points put the line's second end on its first.
fn morph(r: &Reader) -> EffectParams {
    let at = |id: &str, default: f64| r.get(id, default, 0.0, 100.0) as f32;
    let (start_x1, start_y1) = (at("Morph_Start_X1", 0.0), at("Morph_Start_Y1", 0.0));
    let (end_x1, end_y1) = (at("Morph_End_X1", 0.0), at("Morph_End_Y1", 100.0));
    let (start_x2, start_y2) = if r.check("Morph_Start_Link") {
        (start_x1, start_y1)
    } else {
        (at("Morph_Start_X2", 100.0), at("Morph_Start_Y2", 0.0))
    };
    let (end_x2, end_y2) = if r.check("Morph_End_Link") {
        (end_x1, end_y1)
    } else {
        (at("Morph_End_X2", 100.0), at("Morph_End_Y2", 100.0))
    };
    EffectParams::Morph(MorphParams {
        start_x1,
        start_y1,
        start_x2,
        start_y2,
        end_x1,
        end_y1,
        end_x2,
        end_y2,
        head_duration: at("MorphDuration", 20.0),
        start_length: at("MorphStartLength", 1.0),
        end_length: at("MorphEndLength", 1.0),
        acceleration: r.get("MorphAccel", 0.0, -10.0, 10.0) as f32,
        repeats: count(r.get("Morph_Repeat_Count", 0.0, 0.0, 250.0), 0, 250),
        repeat_spacing: count(r.get("Morph_Repeat_Skip", 1.0, 1.0, 100.0), 1, 100),
        stagger: r.get("Morph_Stagger", 0.0, -100.0, 100.0) as f32,
        head_at_start: r.check("ShowHeadAtStart"),
        auto_repeat: r.check("Morph_AutoRepeat"),
    })
}

/// Circles (xLights' `CirclesEffect`). Its checkboxes pick one look, in xLights' order: rings
/// (rainbow first), then plasma, then fading, then bubbles. Collide is Bounce, as xLights treats
/// it now.
fn circles(r: &Reader, diff: &mut Diff) -> EffectParams {
    let bubbles = r.check("Circles_Bubbles");
    let look = if r.check("Circles_Radial_3D") {
        CirclesLook::RainbowRadial
    } else if r.check("Circles_Radial") {
        CirclesLook::Radial
    } else if r.check("Circles_Plasma") {
        CirclesLook::Plasma
    } else if r.check("Circles_Linear_Fade") {
        CirclesLook::Fading
    } else if bubbles {
        CirclesLook::Bubbles
    } else {
        CirclesLook::Solid
    };
    let rings = matches!(look, CirclesLook::Radial | CirclesLook::RainbowRadial);
    if bubbles && matches!(look, CirclesLook::Plasma | CirclesLook::Fading) {
        diff.add("bubbles' drift not applied to plasma or fading circles");
    }
    if r.check("Circles_Random_m") && !rings {
        diff.add("random motion not applied");
    }
    if !rings && r.s.curve_active("E_VALUECURVE_Circles_Speed") {
        diff.add("circles moving at a changing speed approximated");
    }
    EffectParams::Circles(CirclesParams {
        count: count(r.get("Circles_Count", 3.0, 1.0, 10.0), 1, 10),
        size: count(r.get("Circles_Size", 5.0, 1.0, 20.0), 1, 20),
        speed: r.get("Circles_Speed", 10.0, 1.0, 30.0) as f32,
        look,
        bounce: r.check("Circles_Bounce") || r.check("Circles_Collide"),
        center_x: r.get("Circles_XC", 0.0, -50.0, 50.0) as f32,
        center_y: r.get("Circles_YC", 0.0, -50.0, 50.0) as f32,
    })
}

/// A setting the file stores as the text box (the slider holding it times `divisor`), from
/// whichever it has.
fn divided(r: &Reader, id: &str, divisor: f64, default: f64) -> f64 {
    r.s.num(&format!("E_TEXTCTRL_{id}"))
        .or_else(|| r.s.num(&format!("E_SLIDER_{id}")).map(|v| v / divisor))
        .unwrap_or(default)
}

/// A Pinwheel (xLights' `PinwheelEffect`). Its Rotation box turns the arms counterclockwise.
fn pinwheel(r: &Reader, diff: &mut Diff) -> EffectParams {
    let shading = match r.choice("Pinwheel_3D", "None") {
        "None" => PinwheelShading::Flat,
        "3D" => PinwheelShading::Raised,
        "3D Inverted" => PinwheelShading::Sunken,
        "Sweep" => PinwheelShading::Sweep,
        other => {
            diff.add(format!("'{other}' shading shown flat"));
            PinwheelShading::Flat
        }
    };
    EffectParams::Pinwheel(PinwheelParams {
        arms: count(r.get("Pinwheel_Arms", 3.0, 1.0, 20.0), 1, 20),
        arm_size: r.get("Pinwheel_ArmSize", 100.0, 0.0, 400.0) as f32,
        twist: r.get("Pinwheel_Twist", 0.0, -360.0, 360.0) as f32,
        thickness: r.get("Pinwheel_Thickness", 0.0, 0.0, 100.0) as f32,
        speed: r.get("Pinwheel_Speed", 10.0, 0.0, 50.0) as f32,
        counterclockwise: r.check_or("Pinwheel_Rotation", true),
        shading,
        offset: r.get("Pinwheel_Offset", 0.0, 0.0, 360.0) as f32,
        center_x: r.get("PinwheelXC", 0.0, -100.0, 100.0) as f32,
        center_y: r.get("PinwheelYC", 0.0, -100.0, 100.0) as f32,
        style: if r.choice("Pinwheel_Style", "New Render Method") == "New Render Method" {
            PinwheelStyle::Smooth
        } else {
            PinwheelStyle::Spokes
        },
    })
}

/// Snowflakes (xLights' `SnowflakesEffect`). The Type slider picks the look, 0 a random one.
fn snowflakes(r: &Reader, diff: &mut Diff) -> EffectParams {
    let flake = match r.get("Snowflakes_Type", 1.0, 0.0, 9.0).round() as u32 {
        0 => SnowflakeShape::Random,
        1 => SnowflakeShape::Dot,
        2 => SnowflakeShape::Cross,
        3 => SnowflakeShape::Bar,
        4 => SnowflakeShape::BigCross,
        5 => SnowflakeShape::Star,
        6 => SnowflakeShape::Square,
        7 => SnowflakeShape::Plus,
        8 => SnowflakeShape::Diamond,
        _ => SnowflakeShape::X,
    };
    let motion = match r.s.text("E_CHOICE_Falling", "Driving") {
        "Driving" => SnowflakesMotion::Blowing,
        "Falling" => SnowflakesMotion::Falling,
        "Falling & Accumulating" => SnowflakesMotion::PilingUp,
        other => {
            diff.add(format!("'{other}' snow shown blowing"));
            SnowflakesMotion::Blowing
        }
    };
    EffectParams::Snowflakes(SnowflakesParams {
        count: count(r.get("Snowflakes_Count", 5.0, 1.0, 100.0), 1, 100),
        flake,
        speed: r.get("Snowflakes_Speed", 10.0, 0.0, 50.0) as f32,
        motion,
        warmup: count(r.get("Snowflakes_WarmupFrames", 0.0, 0.0, 100.0), 0, 100),
    })
}

/// Plasma (xLights' `PlasmaEffect`). Its Style slider adds twist.
fn plasma(r: &Reader, diff: &mut Diff) -> EffectParams {
    let colors = match r.choice("Plasma_Color", "Normal") {
        "Normal" => PlasmaColors::Palette,
        "Preset Colors 1" => PlasmaColors::RedGreen,
        "Preset Colors 2" => PlasmaColors::BlueGreen,
        "Preset Colors 3" => PlasmaColors::Rainbow,
        "Preset Colors 4" => PlasmaColors::White,
        other => {
            diff.add(format!("'{other}' colors shown in the palette"));
            PlasmaColors::Palette
        }
    };
    EffectParams::Plasma(PlasmaParams {
        colors,
        twist: count(r.get("Plasma_Style", 1.0, 1.0, 10.0), 1, 10),
        density: count(r.get("Plasma_Line_Density", 1.0, 1.0, 10.0), 1, 10),
        speed: r.get("Plasma_Speed", 10.0, 0.0, 100.0) as f32,
    })
}

/// A Butterfly (xLights' `ButterflyEffect`). Its Style slider picks the pattern.
fn butterfly(r: &Reader) -> EffectParams {
    EffectParams::Butterfly(ButterflyParams {
        pattern: count(r.get("Butterfly_Style", 1.0, 1.0, 10.0), 1, 10),
        colors: if r.choice("Butterfly_Colors", "Rainbow") == "Palette" {
            ButterflyColors::Palette
        } else {
            ButterflyColors::Rainbow
        },
        speed: r.get("Butterfly_Speed", 10.0, 0.0, 100.0) as f32,
        direction: direction(r.choice("Butterfly_Direction", "Normal") == "Reverse"),
        chunks: count(r.get("Butterfly_Chunks", 1.0, 1.0, 10.0), 1, 10),
        skip: count(r.get("Butterfly_Skip", 2.0, 2.0, 10.0), 2, 10),
    })
}

/// Garlands (xLights' `GarlandsEffect`). Cycles are stored in tenths on the slider.
fn garlands(r: &Reader, diff: &mut Diff) -> EffectParams {
    let shape = match r.get("Garlands_Type", 0.0, 0.0, 4.0).round() as u32 {
        0 => GarlandShape::Straight,
        1 => GarlandShape::SmallSwags,
        2 => GarlandShape::Swags,
        3 => GarlandShape::DeepSwags,
        _ => GarlandShape::DoubleDips,
    };
    use GarlandsDirection as D;
    let direction = match r.choice("Garlands_Direction", "Up") {
        "Up" => D::Up,
        "Down" => D::Down,
        "Left" => D::Left,
        "Right" => D::Right,
        "Up then Down" => D::UpThenDown,
        "Down then Up" => D::DownThenUp,
        "Left then Right" => D::LeftThenRight,
        "Right then Left" => D::RightThenLeft,
        other => {
            diff.add(format!("'{other}' garlands shown stacking up"));
            D::Up
        }
    };
    EffectParams::Garlands(GarlandsParams {
        shape,
        spacing: r.get("Garlands_Spacing", 10.0, 1.0, 100.0) as f32,
        cycles: divided(r, "Garlands_Cycles", 10.0, 1.0).clamp(0.0, 20.0) as f32,
        direction,
    })
}

/// Lines (xLights' `LinesEffect`). Speed is stored in tenths on the slider.
fn lines(r: &Reader) -> EffectParams {
    EffectParams::Lines(LinesParams {
        count: count(r.get("Lines_Objects", 2.0, 1.0, 20.0), 1, 20),
        points: count(r.get("Lines_Segments", 3.0, 2.0, 6.0), 2, 6),
        thickness: count(r.get("Lines_Thickness", 1.0, 1.0, 10.0), 1, 10),
        speed: divided(r, "Lines_Speed", 10.0, 1.0).clamp(0.0, 10.0) as f32,
        trails: count(r.get("Lines_Trails", 0.0, 0.0, 10.0), 0, 10),
        fade_trails: r.check_or("Lines_FadeTrails", true),
    })
}

/// Life (xLights' `LifeEffect`). Its Type slider picks the rules.
fn life(r: &Reader) -> EffectParams {
    let rules = match r.get("Life_Seed", 0.0, 0.0, 4.0).round() as u32 {
        0 => LifeRules::Classic,
        1 => LifeRules::B35S236,
        2 => LifeRules::Amoeba,
        3 => LifeRules::Coagulations,
        _ => LifeRules::B25678S5678,
    };
    EffectParams::Life(LifeParams {
        density: count(r.get("Life_Count", 50.0, 0.0, 100.0), 0, 100),
        rules,
        speed: count(r.get("Life_Speed", 10.0, 1.0, 30.0), 1, 30),
    })
}

/// A Tendril (xLights' `TendrilEffect`). The two movements that follow the music move as the
/// closest one that doesn't.
fn tendril(r: &Reader, diff: &mut Diff) -> EffectParams {
    use TendrilMovement as M;
    let movement = match r.choice("Tendril_Movement", "Circle") {
        "Random" => M::Random,
        "Square" => M::Square,
        "Circle" => M::Circle,
        "Horizontal Zig Zag" => M::HorizontalZigZag,
        "Horiz. Zig Zag Return" => M::HorizontalZigZagReturn,
        "Vertical Zig Zag" => M::VerticalZigZag,
        "Vert. Zig Zag Return" => M::VerticalZigZagReturn,
        "Manual" => M::Manual,
        "Music Line" => {
            diff.add("movement that follows the music shown as a zig zag");
            M::VerticalZigZag
        }
        "Music Circle" => {
            diff.add("movement that follows the music shown as a circle");
            M::Circle
        }
        other => {
            diff.add(format!("'{other}' movement shown as random"));
            M::Random
        }
    };
    EffectParams::Tendril(TendrilParams {
        movement,
        movement_size: r.get("Tendril_TuneMovement", 10.0, 0.0, 20.0) as f32,
        thickness: r.get("Tendril_Thickness", 3.0, 1.0, 20.0) as f32,
        tendrils: count(r.get("Tendril_Trails", 1.0, 1.0, 20.0), 1, 20),
        length: count(r.get("Tendril_Length", 60.0, 5.0, 100.0), 5, 100),
        speed: count(r.get("Tendril_Speed", 10.0, 1.0, 10.0), 1, 10),
        friction: count(r.get("Tendril_Friction", 10.0, 0.0, 20.0), 0, 20),
        dampening: count(r.get("Tendril_Dampening", 10.0, 0.0, 20.0), 0, 20),
        tension: count(r.get("Tendril_Tension", 20.0, 0.0, 39.0), 0, 39),
        offset_x: r.get("Tendril_XOffset", 0.0, -100.0, 100.0) as f32,
        offset_y: r.get("Tendril_YOffset", 0.0, -100.0, 100.0) as f32,
        manual_x: r.get("Tendril_ManualX", 0.0, 0.0, 100.0) as f32,
        manual_y: r.get("Tendril_ManualY", 0.0, 0.0, 100.0) as f32,
    })
}

/// A font's letter height in pixels: from xLights' own font's name ("7-7x9 Thin" is 9 tall), or
/// the point size in a system font's description; `None` when it doesn't say.
fn font_height(xl_font: &str, os_font: &str) -> Option<f64> {
    if xl_font != "Use OS Fonts" {
        let size = xl_font.split_whitespace().next()?.split('-').nth(1)?;
        return size.split('x').nth(1)?.parse().ok();
    }
    os_font
        .split_whitespace()
        .filter_map(|word| word.trim_matches('\'').parse::<f64>().ok())
        .find(|&points| points > 0.0)
}

/// Text (xLights' `TextEffect`), in PixelFlow's pixel font at about the font's size (10 pixels
/// when the font doesn't say).
fn text(r: &Reader, diff: &mut Diff) -> EffectParams {
    let words = r.s.text("E_TEXTCTRL_Text", "");
    if words.is_empty()
        && (!r.s.text("E_CHOICE_Text_LyricTrack", "").is_empty()
            || !r.s.text("E_FILEPICKERCTRL_Text_File", "").is_empty())
    {
        diff.add("words from a lyrics track or a file not shown");
    }
    if words.contains("${") {
        diff.add("song details in the text shown as written");
    }
    let xl_font = r.choice("Text_Font", "Use OS Fonts");
    let os_font = r.s.text("E_FONTPICKER_Text_Font", "");
    let font = if xl_font != "Use OS Fonts" {
        xl_font
    } else {
        os_font.split('\'').nth(1).unwrap_or("default")
    };
    diff.add(format!("'{font}' font shown in PixelFlow's pixel font"));
    let size = font_height(xl_font, os_font).unwrap_or(10.0);
    use TextMovement as M;
    let movement = match r.choice("Text_Dir", "none") {
        "none" => M::None,
        "left" => M::Left,
        "right" => M::Right,
        "up" => M::Up,
        "down" => M::Down,
        "up-left" => M::UpLeft,
        "down-left" => M::DownLeft,
        "up-right" => M::UpRight,
        "down-right" => M::DownRight,
        "vector" => M::Vector,
        "wavey" => M::Wavy,
        "left-right" => M::LeftRight,
        "up-down" => M::UpDown,
        other => {
            diff.add(format!("'{other}' movement shown still"));
            M::None
        }
    };
    let orientation = match r.choice("Text_Effect", "normal") {
        "normal" => TextOrientation::Across,
        "vert text up" => TextOrientation::StackedUp,
        "vert text down" => TextOrientation::StackedDown,
        other => {
            diff.add(format!("'{other}' text shown upright"));
            TextOrientation::Across
        }
    };
    let countdown = match r.choice("Text_Count", "none") {
        "none" => TextCountdown::None,
        "seconds" => TextCountdown::Seconds,
        "minutes seconds" => TextCountdown::MinutesSeconds,
        other => {
            diff.add(format!("countdown '{other}' shown as the text itself"));
            TextCountdown::None
        }
    };
    let at = |id: &str| r.get(id, 0.0, -200.0, 200.0) as f32;
    EffectParams::Text(TextParams {
        text: words.to_string(),
        movement,
        speed: count(r.get("Text_Speed", 10.0, 0.0, 100.0), 0, 100),
        size: count(size, 4, 100),
        orientation,
        to_center: r.check("TextToCenter"),
        no_repeat: r.check("TextNoRepeat"),
        start_x: at("Text_XStart"),
        start_y: at("Text_YStart"),
        end_x: at("Text_XEnd"),
        end_y: at("Text_YEnd"),
        pixel_offsets: r.check("Text_PixelOffsets"),
        color_per_word: r.check("Text_Color_PerWord"),
        countdown,
    })
}

/// A singing face (xLights' `FacesEffect` on a node-range face). The timing track is looked up
/// by name when the effect is placed (see `Builder::effect`).
fn faces(r: &Reader, diff: &mut Diff) -> EffectParams {
    let face = r.choice("Faces_FaceDefinition", "Default").trim();
    // "Default" (or nothing) is the model's first face; which one that is depends on the row,
    // so it's worked out where the effect is placed (`face_named`).
    let face = if face == "Default" { "" } else { face };
    let eyes = match r.choice("Faces_Eyes", "Auto") {
        "Open" => FaceEyes::Open,
        "Closed" => FaceEyes::Closed,
        "Auto" => FaceEyes::Auto,
        other => {
            diff.add(format!("eyes '{other}' shown closed"));
            FaceEyes::Closed
        }
    };
    if r.choice("Faces_EyeBlinkFrequency", "Normal") != "Normal"
        || r.choice("Faces_EyeBlinkDuration", "Normal") != "Normal"
    {
        diff.add("blinks at PixelFlow's usual pace");
    }
    if r.choice("Faces_TimingTrack", "").trim().is_empty() && !r.choice("Faces_Phoneme", "").trim().is_empty()
    {
        diff.add("a fixed mouth shape shown at rest");
    }
    if r.check("Faces_SuppressWhenNotSinging") || r.check("Faces_Fade") {
        diff.add("shown while not singing too");
    }
    if !r.choice("Faces_UseState", "").trim().is_empty() {
        diff.add("states on the outline not shown");
    }
    EffectParams::Faces(FacesParams {
        face: face.to_string(),
        timing_track: None,
        eyes,
        colors: FaceColorSource::Face,
        outline: r.check("Faces_Outline"),
    })
}

/// The effect-specific translation of the xLights effect `name`.
fn effect_params(
    name: &str,
    s: &Settings,
    colors: &[Rgb],
    duration_ms: u64,
    frame_ms: u32,
    diff: &mut Diff,
) -> Kind {
    let r = Reader { s };
    let n_colors = colors.len().max(1) as f64;
    let frame = f64::from(frame_ms.max(1));
    let mut closest = |what: &str, params: EffectParams| {
        diff.add(format!("shown as {what}"));
        params
    };
    Kind::Params(match effect_key(name).as_str() {
        "on" => on(&r, diff),
        "off" => EffectParams::Off(OffParams {}),
        "colorwash" => color_wash(&r, colors, diff),
        "bars" => bars(&r, n_colors, duration_ms, diff),
        "singlestrand" => return single_strand(&r, duration_ms, diff),
        "marquee" => marquee(&r, diff),
        "wave" => wave(&r, diff),
        "twinkle" => twinkle(&r, frame, diff),
        "shimmer" => shimmer(&r, duration_ms, diff),
        "strobe" => strobe(&r, frame, diff),
        "spirals" => spirals(&r, n_colors, duration_ms, diff),
        "fire" => fire(&r, diff),
        "meteors" => meteors(&r, diff),
        "ripple" => ripple(&r, duration_ms, diff),
        "faces" => faces(&r, diff),
        "shape" => shape(&r, frame, diff),
        "fan" => fan(&r),
        "morph" => morph(&r),
        "circles" => circles(&r, diff),
        "pinwheel" => pinwheel(&r, diff),
        "snowflakes" => snowflakes(&r, diff),
        "plasma" => plasma(&r, diff),
        "butterfly" => butterfly(&r),
        "garlands" => garlands(&r, diff),
        "lines" => lines(&r),
        "life" => life(&r),
        "tendril" => tendril(&r, diff),
        "text" => text(&r, diff),
        // No direct equivalent: the closest PixelFlow effect, with its default settings.
        "fireworks" => closest("twinkles", EffectParams::Twinkle(TwinkleParams::default())),
        "snowstorm" => closest("falling meteors", EffectParams::Meteors(MeteorsParams::default())),
        "shockwave" => closest("a ripple", EffectParams::Ripple(RippleParams::default())),
        "galaxy" => closest("a spiral", EffectParams::Spiral(SpiralParams::default())),
        "fill" | "curtain" => closest("moving bars", EffectParams::Bars(BarsParams::default())),
        "lightning" => closest("a strobe", EffectParams::Strobe(StrobeParams::default())),
        "adjust" | "warp" | "duplicate" | "dmx" | "servo" | "movinghead" => return Kind::Skip,
        _ => return Kind::Placeholder,
    })
}

/// Layer blending (`T_CHOICE_LayerMethod`) in PixelFlow terms. xLights mixes each layer with the
/// ones below it ("1" is the layer, "2" what's below), and PixelFlow's layers are imported in
/// reverse (xLights' layer 1 on top), so each method maps onto the effect's own blend.
fn blend(s: &Settings, diff: &mut Diff) -> Blend {
    let method = s.text("T_CHOICE_LayerMethod", "Normal");
    let blend = match method {
        "Normal" => Blend::Normal,
        "Additive" => Blend::Add,
        "Subtractive" => Blend::Subtract,
        "Max" => {
            diff.add("'Max' layer blending keeps the layers below where the effect is unlit");
            Blend::Max
        }
        "Min" => Blend::Min,
        "Average" => Blend::Average,
        "1 reveals 2" => Blend::Over,
        "2 reveals 1" | "Layered" => Blend::Behind,
        "1 is Mask" => Blend::Mask,
        "1 is True Unmask" => Blend::Reveal,
        "1 is Unmask" => Blend::RevealBrightness,
        "2 is Mask" => Blend::CutOut,
        "2 is True Unmask" => Blend::Clip,
        "2 is Unmask" => Blend::ClipBrightness,
        "Shadow 1 on 2" => Blend::Shadow,
        "Shadow 2 on 1" => Blend::ShadowBelow,
        "Highlight" => Blend::Highlight,
        "Highlight Vibrant" => Blend::HighlightAdd,
        "Bottom-Top" => Blend::BottomHalf,
        "Left-Right" => Blend::LeftHalf,
        "Brightness" => {
            diff.add("'Brightness' layer blending shown as Tint");
            Blend::Multiply
        }
        // "Effect 1" and "Effect 2" cross-fade by the layer mix amount, which PixelFlow doesn't
        // have (and xLights treats names it doesn't know as "Effect 1").
        other => {
            diff.add(format!("'{other}' layer blending shown as Normal"));
            Blend::Normal
        }
    };
    if s.num_or("T_SLIDER_EffectLayerMix", 0.0) > 0.0 || s.flag("T_CHECKBOX_LayerMorph", false) {
        diff.add("layer mix amount not applied");
    }
    blend
}

/// Fade in/out (`T_TEXTCTRL_Fadein`/`Fadeout`, seconds) and the transition shapes.
fn fades(s: &Settings, duration_ms: u64, diff: &mut Diff) -> (u32, u32) {
    let ms = |key: &str| {
        let seconds = s.num_or(key, 0.0);
        ((seconds.max(0.0) * 1000.0).round().min(duration_ms as f64)) as u32
    };
    let (fade_in, fade_out) = (ms("T_TEXTCTRL_Fadein"), ms("T_TEXTCTRL_Fadeout"));
    for (fade, key) in [
        (fade_in, "T_CHOICE_In_Transition_Type"),
        (fade_out, "T_CHOICE_Out_Transition_Type"),
    ] {
        let kind = s.text(key, "Fade");
        if fade > 0 && kind != "Fade" {
            diff.add(format!("'{kind}' transition shown as a fade"));
        }
    }
    (fade_in, fade_out)
}

/// The render-buffer settings (`B_*`) PixelFlow keeps.
struct Buffer {
    /// PixelFlow's blur (xLights' Blur minus one).
    blur: u32,
    style: RenderStyle,
    transform: BufferTransform,
}

/// Render-buffer settings (`B_*`) that change how an effect is laid over the prop: the render
/// style, its rotation or flip, and the blur (`B_SLIDER_Blur`, 1 = none, answered in PixelFlow's
/// terms: one less).
fn buffer(s: &Settings, diff: &mut Diff) -> Buffer {
    let name = s.text("B_CHOICE_BufferStyle", "Default");
    let style = RenderStyle::from_xlights(name).unwrap_or_else(|| {
        diff.add(format!("'{name}' render style not applied"));
        RenderStyle::Default
    });
    let name = s.text("B_CHOICE_BufferTransform", "None");
    let transform = BufferTransform::from_xlights(name).unwrap_or_else(|| {
        diff.add(format!("'{name}' buffer transformation not applied"));
        BufferTransform::None
    });
    let changed = |key: &str, neutral: f64| s.num(key).is_some_and(|v| (v - neutral).abs() > 1e-6);
    // Rotation, zoom, and pivot curves move the buffer too.
    let moving = s
        .keys()
        .any(|k| k.starts_with("B_VALUECURVE_") && k != "B_VALUECURVE_Blur" && s.curve_active(k));
    if changed("B_SLIDER_Rotation", 0.0) || changed("B_SLIDER_Zoom", 1.0) || moving {
        diff.add("rotation or zoom not applied");
    }
    if !s.text("B_CUSTOM_SubBuffer", "").is_empty() {
        diff.add("sub-buffer (part of the prop) not applied");
    }
    // xLights reads the slider as a whole number (`GetInt`); past 15 it blurs as 15.
    let blur = s.num_or("B_SLIDER_Blur", 1.0).trunc();
    if blur > f64::from(pf_sequence::MAX_BLUR + 1) {
        diff.add("blur above 15 shown at 15");
    }
    Buffer {
        blur: (blur.clamp(1.0, f64::from(pf_sequence::MAX_BLUR + 1)) as u32) - 1,
        style,
        transform,
    }
}

/// The blur's value curve (`B_VALUECURVE_Blur`, xLights' 1-15), as PixelFlow's blur: its value at
/// the start and the curve.
fn blur_curve(s: &Settings, diff: &mut Diff) -> Option<(u32, Option<Curve>)> {
    let curve = XlCurve::parse_in(s.get("B_VALUECURVE_Blur")?, 1.0, 15.0)?;
    let max = f64::from(pf_sequence::MAX_BLUR + 1);
    let (start, curve) = setting_curve(&curve, |v| (v.trunc().clamp(1.0, max) - 1.0) as f32, diff);
    Some((start as u32, curve))
}

/// The sparkles' value curve (`C_VALUECURVE_SparkleFrequency`, 0-200): the value at the start and
/// the curve.
fn sparkles_curve(palette: &ParsedPalette, diff: &mut Diff) -> Option<(u32, Option<Curve>)> {
    let curve = XlCurve::parse_in(palette.sparkles_curve.as_deref()?, 0.0, 200.0)?;
    let max = f64::from(pf_sequence::MAX_SPARKLES);
    let (start, curve) = setting_curve(&curve, |v| v.trunc().clamp(0.0, max) as f32, diff);
    Some((start as u32, curve))
}

/// The palette's sparkles and their color (dimmed by the palette's brightness, as xLights applies
/// brightness after sparkles).
fn sparkles(palette: &ParsedPalette, diff: &mut Diff) -> (u32, Rgb) {
    if palette.sparkles > 0 && palette.music_sparkles {
        diff.add("music-driven sparkles shown at a steady rate");
    }
    let c = palette.sparkle_color;
    let scale = brightness_scale(palette);
    let f = |v: u8| (f64::from(v) * scale).round() as u8;
    (palette.sparkles, Rgb::new(f(c.r), f(c.g), f(c.b)))
}

/// The palette's brightness as a factor (0–1; above 100% counts as 100%).
fn brightness_scale(palette: &ParsedPalette) -> f64 {
    let brightness = if palette.brightness.is_finite() {
        palette.brightness.max(0.0)
    } else {
        100.0
    };
    (brightness / 100.0).min(1.0)
}

/// Palette colors with the palette's brightness applied (xLights' `C_SLIDER_Brightness`).
fn palette_colors(palette: &ParsedPalette, diff: &mut Diff) -> Vec<Rgb> {
    if palette.color_curves > 0 {
        diff.add("color curves shown as their first color");
    }
    if palette.unreadable > 0 {
        diff.add("unreadable palette colors left out");
    }
    for extra in &palette.extras {
        diff.add(format!("{extra} not applied"));
    }
    if palette.brightness.is_finite() && palette.brightness > 100.0 {
        diff.add("brightness above 100% shown at 100%");
    }
    let scale = brightness_scale(palette);
    // With no colors enabled xLights draws white, so say so explicitly (PixelFlow's renderer
    // also draws an empty palette white, but the timeline shows what's in the palette).
    let colors: &[Rgb] = if palette.colors.is_empty() {
        &[Rgb::WHITE]
    } else {
        &palette.colors
    };
    colors
        .iter()
        .take(pf_sequence::MAX_PALETTE_COLORS)
        .map(|c| {
            let f = |v: u8| (f64::from(v) * scale).round() as u8;
            Rgb::new(f(c.r), f(c.g), f(c.b))
        })
        .collect()
}

/// Translates one xLights effect lasting `duration_ms` in a sequence with `frame_ms` frames.
/// `None` for effects that are left out ([`Fidelity::Skipped`]).
pub fn translate(
    name: &str,
    s: &Settings,
    palette: &ParsedPalette,
    duration_ms: u64,
    frame_ms: u32,
) -> Option<Translated> {
    let mut diff = Diff(Vec::new());
    let colors = palette_colors(palette, &mut diff);
    let (mut sparkles, sparkle_color) = sparkles(palette, &mut diff);
    let blend = blend(s, &mut diff);
    let (fade_in_ms, fade_out_ms) = fades(s, duration_ms, &mut diff);
    let Buffer {
        mut blur,
        style: render_style,
        transform: buffer_transform,
    } = buffer(s, &mut diff);
    let mut curves = BTreeMap::new();
    if let Some((start, curve)) = sparkles_curve(palette, &mut diff) {
        sparkles = start;
        curves.extend(curve.map(|c| ("sparkles".to_string(), c)));
    }
    if let Some((start, curve)) = blur_curve(s, &mut diff) {
        blur = start;
        curves.extend(curve.map(|c| ("blur".to_string(), c)));
    }
    // Settings that change over the effect start at their first value; music and timing-track
    // ones stay at their middle.
    let mut start = s.clone();
    let mut moving = Vec::new();
    for (id, curve) in effect_curves(s) {
        if let Some(driven) = curve.driven() {
            diff.add(driven_note(driven));
            put(&mut start, &id, curve.middle());
        } else {
            let values = curve.values();
            put(&mut start, &id, values[0].1);
            moving.push((id, values));
        }
    }
    Some(
        match effect_params(name, &start, &colors, duration_ms, frame_ms, &mut diff) {
            Kind::Skip => return None,
            Kind::Params(params) => {
                // PixelFlow's settings table has the final say on ranges (evaluated before
                // `fidelity`, so a clamp counts as an approximation).
                let clamped = params.sanitized();
                if clamped != params {
                    diff.add(BEYOND_RANGE);
                }
                if !moving.is_empty() {
                    curves.extend(param_curves(
                        name,
                        &start,
                        &moving,
                        &colors,
                        duration_ms,
                        frame_ms,
                        &clamped,
                        &mut diff,
                    ));
                }
                on_cycles(&start, &clamped, &mut curves, &mut diff);
                Translated {
                    params: clamped,
                    palette: Palette::new(colors),
                    blend,
                    fade_in_ms,
                    fade_out_ms,
                    sparkles,
                    sparkle_color,
                    blur,
                    render_style,
                    buffer_transform,
                    curves,
                    fidelity: if diff.0.is_empty() {
                        Fidelity::Exact
                    } else {
                        Fidelity::Approximate(diff.0)
                    },
                }
            }
            Kind::Placeholder => Translated {
                params: EffectParams::On(OnParams {
                    gradient: Gradient::None,
                    start_level: PLACEHOLDER_LEVEL,
                    end_level: PLACEHOLDER_LEVEL,
                }),
                palette: Palette::new(colors.first().copied().map_or_else(Vec::new, |c| vec![c])),
                blend,
                fade_in_ms,
                fade_out_ms,
                sparkles,
                sparkle_color,
                blur,
                render_style,
                buffer_transform,
                curves,
                fidelity: Fidelity::Placeholder,
            },
        },
    )
}

/// What happened to each kind of xLights effect, for the report.
#[derive(Debug, Default)]
pub struct Tally {
    /// Per xLights effect name: placeholders, and approximations by what differs.
    by_name: BTreeMap<String, NameTally>,
}

#[derive(Debug, Default)]
struct NameTally {
    placeholders: usize,
    skipped: usize,
    approximate: usize,
    reasons: BTreeMap<String, usize>,
}

impl Tally {
    pub fn record(&mut self, name: &str, fidelity: &Fidelity) {
        let name = if name.is_empty() { "(unnamed)" } else { name };
        match fidelity {
            Fidelity::Exact => {}
            Fidelity::Placeholder => {
                self.by_name.entry(name.to_string()).or_default().placeholders += 1;
            }
            Fidelity::Skipped => {
                self.by_name.entry(name.to_string()).or_default().skipped += 1;
            }
            Fidelity::Approximate(reasons) => {
                let t = self.by_name.entry(name.to_string()).or_default();
                t.approximate += 1;
                for r in reasons {
                    *t.reasons.entry(r.clone()).or_default() += 1;
                }
            }
        }
    }

    /// Notes: placeholders first (most effects first), then approximations per effect.
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        let mut placeholders: Vec<(&String, usize)> = self
            .by_name
            .iter()
            .filter(|(_, t)| t.placeholders > 0)
            .map(|(n, t)| (n, t.placeholders))
            .collect();
        placeholders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        if !placeholders.is_empty() {
            let names: Vec<String> = placeholders.iter().map(|(n, c)| format!("{n} ({c})")).collect();
            let total: usize = placeholders.iter().map(|(_, c)| c).sum();
            notes.push(format!(
                "PixelFlow has no matching effect yet for {}, so {} shown as a dim fill in {} first color: {}.",
                if placeholders.len() == 1 { "this xLights effect" } else { "these xLights effects" },
                if total == 1 { "it is" } else { "they are" },
                if total == 1 { "its" } else { "each one's" },
                list(&names)
            ));
        }
        let skipped: Vec<String> = self
            .by_name
            .iter()
            .filter(|(_, t)| t.skipped > 0)
            .map(|(n, t)| format!("{n} ({})", t.skipped))
            .collect();
        if !skipped.is_empty() {
            notes.push(format!(
                "These xLights effects change the layers below them or move fixtures, so they weren't imported (a stand-in would light the prop wrongly): {}.",
                list(&skipped)
            ));
        }
        let mut approximated: Vec<(&String, &NameTally)> =
            self.by_name.iter().filter(|(_, t)| t.approximate > 0).collect();
        approximated.sort_by(|a, b| b.1.approximate.cmp(&a.1.approximate).then(a.0.cmp(b.0)));
        for (name, t) in approximated {
            let mut reasons: Vec<(&String, &usize)> = t.reasons.iter().collect();
            reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let reasons: Vec<String> = reasons
                .iter()
                .map(|(r, c)| {
                    if **c == t.approximate {
                        (*r).clone()
                    } else {
                        format!("{r} ({c})")
                    }
                })
                .collect();
            notes.push(format!(
                "{name} ({}) approximated: {}.",
                plural(t.approximate, "effect"),
                list(&reasons)
            ));
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette(colors: &[Rgb]) -> ParsedPalette {
        ParsedPalette {
            colors: colors.to_vec(),
            brightness: 100.0,
            ..ParsedPalette::default()
        }
    }

    #[test]
    fn faces_keep_their_face_eyes_and_outline() {
        let settings = Settings::parse(
            "E_CHECKBOX_Faces_Outline=1,E_CHOICE_Faces_Eyes=(off),E_CHOICE_Faces_FaceDefinition=Elf,E_CHOICE_Faces_Phoneme=AI",
        );
        let t = translate("Faces", &settings, &palette(&[Rgb::RED]), 1000, 25).unwrap();
        assert_eq!(
            t.params,
            EffectParams::Faces(FacesParams {
                face: "Elf".into(),
                timing_track: None,
                eyes: FaceEyes::Closed,
                colors: FaceColorSource::Face,
                outline: true,
            })
        );
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "eyes '(off)' shown closed".into(),
                "a fixed mouth shape shown at rest".into()
            ])
        );
        let plain = translate("Faces", &Settings::default(), &palette(&[]), 1000, 25).unwrap();
        assert_eq!(
            plain.params,
            EffectParams::Faces(FacesParams::default()),
            "Default: the first face"
        );
        assert_eq!(plain.fidelity, Fidelity::Exact);
    }

    #[test]
    fn unknown_effects_become_dim_placeholders_in_their_first_color() {
        let t = translate(
            "Candle",
            &Settings::default(),
            &palette(&[Rgb::RED, Rgb::BLUE]),
            1000,
            25,
        )
        .unwrap();
        assert_eq!(t.fidelity, Fidelity::Placeholder);
        assert_eq!(t.palette.colors, vec![Rgb::RED]);
        assert_eq!(
            t.params,
            EffectParams::On(OnParams {
                gradient: Gradient::None,
                start_level: PLACEHOLDER_LEVEL,
                end_level: PLACEHOLDER_LEVEL,
            })
        );
    }

    #[test]
    fn an_empty_palette_is_white_like_in_xlights() {
        let t = translate("Color Wash", &Settings::default(), &palette(&[]), 1000, 25).unwrap();
        assert_eq!(t.palette.colors, vec![Rgb::WHITE]);
        assert_eq!(t.fidelity, Fidelity::Exact);
        let t = translate("Faces", &Settings::default(), &palette(&[]), 1000, 25).unwrap();
        assert_eq!(t.palette.colors, vec![Rgb::WHITE], "placeholders too");
    }

    #[test]
    fn blends_fades_and_brightness_translate() {
        let s = Settings::parse(
            "T_CHOICE_LayerMethod=Additive,T_TEXTCTRL_Fadein=0.25,T_TEXTCTRL_Fadeout=1.5,T_CHOICE_Out_Transition_Type=Fade",
        );
        let mut p = palette(&[Rgb::new(200, 100, 50)]);
        p.brightness = 50.0;
        let t = translate("Off", &s, &p, 1000, 25).unwrap();
        assert_eq!(t.blend, Blend::Add);
        assert_eq!(
            (t.fade_in_ms, t.fade_out_ms),
            (250, 1000),
            "fades fit in the effect"
        );
        assert_eq!(t.palette.colors, vec![Rgb::new(100, 50, 25)]);
        assert_eq!(t.fidelity, Fidelity::Exact);
    }

    #[test]
    fn every_xlights_layer_method_but_the_cross_fades_translates_exactly() {
        let methods = [
            ("Normal", Blend::Normal),
            ("Additive", Blend::Add),
            ("Subtractive", Blend::Subtract),
            ("Min", Blend::Min),
            ("Average", Blend::Average),
            ("1 reveals 2", Blend::Over),
            ("2 reveals 1", Blend::Behind),
            ("Layered", Blend::Behind),
            ("1 is Mask", Blend::Mask),
            ("1 is True Unmask", Blend::Reveal),
            ("1 is Unmask", Blend::RevealBrightness),
            ("2 is Mask", Blend::CutOut),
            ("2 is True Unmask", Blend::Clip),
            ("2 is Unmask", Blend::ClipBrightness),
            ("Shadow 1 on 2", Blend::Shadow),
            ("Shadow 2 on 1", Blend::ShadowBelow),
            ("Highlight", Blend::Highlight),
            ("Highlight Vibrant", Blend::HighlightAdd),
            ("Bottom-Top", Blend::BottomHalf),
            ("Left-Right", Blend::LeftHalf),
        ];
        for (method, want) in methods {
            let s = Settings::parse(&format!("T_CHOICE_LayerMethod={method}"));
            let t = translate("Color Wash", &s, &palette(&[Rgb::RED]), 1000, 25).unwrap();
            assert_eq!((t.blend, t.fidelity), (want, Fidelity::Exact), "{method}");
        }
        for (method, want, note) in [
            (
                "Effect 1",
                Blend::Normal,
                "'Effect 1' layer blending shown as Normal",
            ),
            (
                "Brightness",
                Blend::Multiply,
                "'Brightness' layer blending shown as Tint",
            ),
            (
                "Max",
                Blend::Max,
                "'Max' layer blending keeps the layers below where the effect is unlit",
            ),
        ] {
            let s = Settings::parse(&format!("T_CHOICE_LayerMethod={method}"));
            let t = translate("Color Wash", &s, &palette(&[Rgb::RED]), 1000, 25).unwrap();
            assert_eq!(t.blend, want, "{method}");
            assert_eq!(t.fidelity, Fidelity::Approximate(vec![note.into()]), "{method}");
        }
    }

    #[test]
    fn sparkles_and_blur_translate_exactly() {
        let s = Settings::parse("B_SLIDER_Blur=8");
        let mut p = palette(&[Rgb::RED]);
        p.sparkles = 54;
        p.sparkle_color = Rgb::new(200, 100, 0);
        p.brightness = 50.0;
        let t = translate("Color Wash", &s, &p, 1000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!((t.sparkles, t.blur), (54, 7));
        assert_eq!(
            t.sparkle_color,
            Rgb::new(100, 50, 0),
            "dimmed by the palette's brightness"
        );
        let plain = translate(
            "Color Wash",
            &Settings::default(),
            &palette(&[Rgb::RED]),
            1000,
            25,
        )
        .unwrap();
        assert_eq!(
            (plain.sparkles, plain.sparkle_color, plain.blur),
            (0, Rgb::WHITE, 0)
        );
        // Placeholders keep them too.
        let lines = translate("Candle", &s, &p, 1000, 25).unwrap();
        assert_eq!(lines.fidelity, Fidelity::Placeholder);
        assert_eq!((lines.sparkles, lines.blur), (54, 7));
    }

    #[test]
    fn sparkles_and_blur_that_change_over_the_effect_get_curves() {
        let s = Settings::parse(
            "B_SLIDER_Blur=40,B_VALUECURVE_Blur=Active=TRUE|Type=Ramp|Min=1.00|Max=15.00|P1=1.00|P2=15.00|RV=TRUE|",
        );
        let mut p = palette(&[Rgb::RED]);
        p.sparkles = 10;
        p.music_sparkles = true;
        p.sparkles_curve = Some("Active=TRUE|Type=Music|Min=0.00|Max=200.00|P2=200.00|RV=TRUE|".into());
        let t = translate("Color Wash", &s, &p, 1000, 25).unwrap();
        // The blur goes from none to the most; xLights takes the whole part of each value, so
        // it climbs in steps.
        assert_eq!(t.blur, 0, "the curve's start, not the slider");
        let blur = &t.curves["blur"];
        assert_eq!((blur.from, blur.to), (0.0, 14.0));
        assert_eq!(blur.value_at(0.5).round(), 7.0);
        // Sparkles that follow the music are held at their middle.
        assert_eq!(t.sparkles, 100);
        assert!(!t.curves.contains_key("sparkles"));
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "music-driven sparkles shown at a steady rate".into(),
                "blur above 15 shown at 15".into(),
                "settings that follow the music held at their middle value".into(),
            ])
        );
    }

    #[test]
    fn effect_settings_that_change_get_curves_through_the_translation() {
        // Spirals' rotation (tenths of a wrap on the slider) on xLights' sine: two cycles,
        // starting three quarters in, between -9 and 9 wraps.
        let s = Settings::parse(
            "E_SLIDER_Spirals_Rotation=20,E_TEXTCTRL_Spirals_Movement=1.0,\
             E_VALUECURVE_Spirals_Rotation=Active=TRUE|Id=ID_VALUECURVE_Spirals_Rotation|Type=Sine|Min=-90.00|Max=90.00|P1=75.00|P2=90.00|P3=20.00|RV=TRUE|",
        );
        let t = translate("Spirals", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        let twist = &t.curves["twist"];
        assert_eq!(twist.shape, CurveShape::Custom);
        assert_eq!((twist.from, twist.to), (-9.0, 9.0));
        let EffectParams::Spiral(p) = t.params else {
            panic!("{:?}", t.params)
        };
        assert_eq!(p.twist, -9.0, "the setting holds the curve's start");
        for time in [0.0f64, 0.1, 0.125, 0.25, 0.4, 0.5, 0.75] {
            let r = time * 2.0 * std::f64::consts::TAU + 0.75 * std::f64::consts::TAU;
            let want = (9.0 * r.sin()) as f32;
            assert!(
                (twist.value_at(time as f32) - want).abs() < 0.01,
                "{time}: {want}"
            );
        }
        assert!(t.curves.len() == 1, "nothing else changes: {:?}", t.curves.keys());

        // A ramp on Bars' cycles (the text box shows tenths of the slider) becomes a ramp of
        // its speed, per second over the 2 s effect.
        let s = Settings::parse(
            "E_CHOICE_Bars_Direction=up,E_TEXTCTRL_Bars_Cycles=1.0,\
             E_VALUECURVE_Bars_Cycles=Active=TRUE|Type=Ramp|Min=0.00|Max=300.00|P1=20.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Bars", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.curves["speed"], Curve::ramp(1.0, 5.0));

        // A square wave on a whole-number setting keeps its sharp edges.
        let s = Settings::parse(
            "E_VALUECURVE_Meteors_Count=Active=TRUE|Type=Square|Min=1.00|Max=100.00|P1=10.00|P2=40.00|P3=2.00|RV=TRUE|",
        );
        let t = translate("Meteors", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        let count = &t.curves["count"];
        assert_eq!(
            [0.1, 0.249, 0.25, 0.4, 0.5, 0.8].map(|t| count.value_at(t).round()),
            [10.0, 10.0, 40.0, 40.0, 10.0, 40.0]
        );

        // Curves on settings PixelFlow doesn't use change nothing.
        let s = Settings::parse(
            "E_VALUECURVE_Bars_Center=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-100.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Bars", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert!(t.curves.is_empty());
    }

    #[test]
    fn music_and_timing_track_curves_hold_their_middle_and_say_so() {
        let s = Settings::parse(
            "E_SLIDER_Twinkle_Count=3,E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Inverted Music|Min=2.00|Max=100.00|P1=10.00|P2=50.00|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        let EffectParams::Twinkle(p) = t.params else {
            panic!("{:?}", t.params)
        };
        assert!((p.density - 0.3).abs() < 1e-6, "between 10 and 50: {}", p.density);
        assert!(t.curves.is_empty());
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "settings that follow the music held at their middle value".into()
            ])
        );
        let s = Settings::parse(
            "E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Timing Track Toggle|Min=2.00|Max=100.00|P1=10.00|P2=50.00|TT=Beats|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "settings that follow a timing track held at their middle value".into()
            ])
        );
    }

    #[test]
    fn on_repeats_its_brightness_ramp() {
        let s =
            Settings::parse("E_TEXTCTRL_Eff_On_Start=100,E_TEXTCTRL_Eff_On_End=0,E_TEXTCTRL_On_Cycles=3.0");
        let t = translate("On", &s, &palette(&[Rgb::RED]), 3000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        let saw = Curve::shaped(CurveShape::Saw, 1.0, 0.0, 3.0);
        assert_eq!(t.curves["startLevel"], saw);
        assert_eq!(t.curves["endLevel"], saw);
        // Once through is the plain ramp.
        let s = Settings::parse("E_TEXTCTRL_Eff_On_Start=100,E_TEXTCTRL_Eff_On_End=0");
        assert!(
            translate("On", &s, &palette(&[Rgb::RED]), 3000, 25)
                .unwrap()
                .curves
                .is_empty()
        );
    }

    #[test]
    fn fans_translate_exactly() {
        // As xLights 2024 writes them: revolutions in 360ths on the slider.
        let s = Settings::parse(
            "E_CHECKBOX_Fan_Blend_Edges=1,E_CHECKBOX_Fan_Reverse=1,E_NOTEBOOK_Fan=Position,E_SLIDER_Fan_Accel=2,\
             E_SLIDER_Fan_Blade_Angle=45,E_SLIDER_Fan_Blade_Width=60,E_SLIDER_Fan_CenterX=40,E_SLIDER_Fan_CenterY=55,\
             E_SLIDER_Fan_Duration=70,E_SLIDER_Fan_Element_Width=80,E_SLIDER_Fan_End_Radius=120,\
             E_SLIDER_Fan_Num_Blades=4,E_SLIDER_Fan_Num_Elements=2,E_SLIDER_Fan_Revolutions=540,\
             E_SLIDER_Fan_Start_Angle=30,E_SLIDER_Fan_Start_Radius=5,E_CHECKBOX_Fan_Scale=0",
        );
        let t = translate("Fan", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Fan(FanParams {
                center_x: 40.0,
                center_y: 55.0,
                start_radius: 5.0,
                end_radius: 120.0,
                blades: 4,
                blade_width: 60.0,
                revolutions: 1.5,
                blade_angle: 45.0,
                duration: 70.0,
                start_angle: 30.0,
                elements: 2,
                element_width: 80.0,
                acceleration: 2.0,
                direction: Direction::Reverse,
                blend_edges: true,
                scale: false,
            })
        );
        // Nothing set: xLights' defaults, revolutions from the text box when that's all there is.
        let plain = translate("Fan", &Settings::default(), &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(plain.params, EffectParams::Fan(FanParams::default()));
        let text = Settings::parse("E_TEXTCTRL_Fan_Revolutions=3.5");
        let EffectParams::Fan(p) = translate("Fan", &text, &palette(&[]), 4000, 25).unwrap().params else {
            unreachable!()
        };
        assert_eq!(p.revolutions, 3.5);
    }

    #[test]
    fn fans_from_before_2025_keep_their_radii_in_pixels() {
        for (version, scale) in [
            ("2024.19", false),
            ("2025.04", true),
            ("2026.1", true),
            ("", true),
        ] {
            let mut s = Settings::default();
            adjust_for_version("Fan", &mut s, version);
            let EffectParams::Fan(p) = translate("Fan", &s, &palette(&[]), 1000, 25).unwrap().params else {
                unreachable!()
            };
            assert_eq!(p.scale, scale, "{version}");
        }
        // Other effects are left alone.
        let mut s = Settings::default();
        adjust_for_version("Shape", &mut s, "2020.1");
        assert!(s.is_empty());
    }

    #[test]
    fn fan_settings_that_change_get_curves() {
        // A sine on the start angle, and revolutions (in 360ths) ramping from 1 to 3.
        let s = Settings::parse(
            "E_SLIDER_Fan_Start_Angle=0,E_SLIDER_Fan_Revolutions=720,\
             E_VALUECURVE_Fan_Start_Angle=Active=TRUE|Id=ID_VALUECURVE_Fan_Start_Angle|Type=Sine|Min=0.00|Max=360.00|P1=0.00|P2=360.00|P3=10.00|P4=180.00|RV=TRUE|,\
             E_VALUECURVE_Fan_Revolutions=Active=TRUE|Type=Ramp|Min=0.00|Max=3600.00|P1=360.00|P2=1080.00|RV=TRUE|",
        );
        let t = translate("Fan", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        assert_eq!(t.curves["revolutions"], Curve::ramp(1.0, 3.0));
        let angle = &t.curves["startAngle"];
        assert!(
            (angle.value_at(0.25) - 360.0).abs() < 1.0,
            "{}",
            angle.value_at(0.25)
        );
        assert!(angle.value_at(0.75).abs() < 1.0);
    }

    #[test]
    fn shapes_translate_with_speeds_per_second() {
        let s = Settings::parse(
            "E_CHECKBOX_Shape_FadeAway=1,E_CHECKBOX_Shape_FireTiming=0,E_CHECKBOX_Shape_HoldColour=1,\
             E_CHECKBOX_Shape_RandomInitial=0,E_CHECKBOX_Shape_RandomLocation=0,E_CHECKBOX_Shape_UseMusic=0,\
             E_CHECKBOX_Shapes_RandomMovement=0,E_CHOICE_Shape_ObjectToDraw=Star,E_SLIDER_Shape_CentreX=30,\
             E_SLIDER_Shape_CentreY=70,E_SLIDER_Shape_Growth=25,E_SLIDER_Shape_Lifetime=40,E_SLIDER_Shape_Points=6,\
             E_SLIDER_Shape_Rotation=15,E_SLIDER_Shape_StartSize=3,E_SLIDER_Shape_Thickness=2,\
             E_SLIDER_Shapes_Direction=180,E_SLIDER_Shapes_Velocity=2,E_TEXTCTRL_Shape_Count=8,\
             E_TEXTCTRL_Shapes_Direction=180,E_TEXTCTRL_Shapes_Velocity=2",
        );
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Shape(ShapeParams {
                shape: ShapeObject::Star,
                count: 8,
                lifetime: 40.0,
                start_size: 3.0,
                growth: 25.0,
                thickness: 2,
                fade: true,
                random_location: false,
                rotation: 15.0,
                points: 6,
                center_x: 30.0,
                center_y: 70.0,
                // 2 pixels a frame at 25 ms frames.
                speed: 80.0,
                direction: 180.0,
                random_movement: false,
                random_start: false,
                timing_track: None,
            })
        );
        // Unset checkboxes take xLights' defaults: fading, random places, staggered start.
        let plain = translate("Shape", &Settings::default(), &palette(&[]), 4000, 25).unwrap();
        assert_eq!(plain.params, EffectParams::Shape(ShapeParams::default()));
        for (object, want) in [
            ("Candy Cane", ShapeObject::CandyCane),
            ("Crucifix", ShapeObject::Crucifix),
            ("Random", ShapeObject::Random),
            ("Ellipse", ShapeObject::Ellipse),
        ] {
            let s = Settings::parse(&format!("E_CHOICE_Shape_ObjectToDraw={object}"));
            let EffectParams::Shape(p) = translate("Shape", &s, &palette(&[]), 1000, 25).unwrap().params
            else {
                unreachable!()
            };
            assert_eq!(p.shape, want);
        }
    }

    #[test]
    fn shapes_pixelflow_cant_draw_say_so() {
        let s = Settings::parse(
            "E_CHOICE_Shape_ObjectToDraw=Emoji,E_CHECKBOX_Shape_UseMusic=1,E_CHECKBOX_Shapes_RandomMovement=1",
        );
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "shapes fired by the music shown as a steady stream".into(),
                "'Emoji' shapes shown as circles".into(),
            ])
        );
        let EffectParams::Shape(p) = t.params else {
            unreachable!()
        };
        assert_eq!(p.shape, ShapeObject::Circle);
        // Random drift is 20 pixels a frame at most in xLights: exact at 50 ms frames only.
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        let Fidelity::Approximate(notes) = t.fidelity else {
            unreachable!()
        };
        assert!(notes.contains(&"random drift speeds approximated".to_string()));
        // On a timing track, a label filter isn't applied.
        let s = Settings::parse(
            "E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Beats,E_TEXTCTRL_Shape_FilterLabel=1",
        );
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "shapes appear on every mark, not only the labels chosen".into()
            ])
        );
    }

    #[test]
    fn morphs_translate_exactly_with_linked_points() {
        let s = Settings::parse(
            "E_CHECKBOX_Morph_End_Link=0,E_CHECKBOX_Morph_Start_Link=1,E_CHECKBOX_ShowHeadAtStart=1,\
             E_NOTEBOOK_Morph=Start,E_SLIDER_MorphAccel=-3,E_SLIDER_MorphDuration=40,E_SLIDER_MorphEndLength=10,\
             E_SLIDER_MorphStartLength=5,E_SLIDER_Morph_End_X1=10,E_SLIDER_Morph_End_X2=90,\
             E_SLIDER_Morph_End_Y1=100,E_SLIDER_Morph_End_Y2=80,E_SLIDER_Morph_Repeat_Count=4,\
             E_SLIDER_Morph_Repeat_Skip=3,E_SLIDER_Morph_Stagger=-20,E_SLIDER_Morph_Start_X1=50,\
             E_SLIDER_Morph_Start_X2=75,E_SLIDER_Morph_Start_Y1=0,E_SLIDER_Morph_Start_Y2=25",
        );
        let t = translate("Morph", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Morph(MorphParams {
                start_x1: 50.0,
                start_y1: 0.0,
                // Linked: the second end sits on the first.
                start_x2: 50.0,
                start_y2: 0.0,
                end_x1: 10.0,
                end_y1: 100.0,
                end_x2: 90.0,
                end_y2: 80.0,
                head_duration: 40.0,
                start_length: 5.0,
                end_length: 10.0,
                acceleration: -3.0,
                repeats: 4,
                repeat_spacing: 3,
                stagger: -20.0,
                head_at_start: true,
                auto_repeat: false,
            })
        );
        let plain = translate("Morph", &Settings::default(), &palette(&[]), 4000, 25).unwrap();
        assert_eq!(plain.params, EffectParams::Morph(MorphParams::default()));
    }

    #[test]
    fn circles_pick_one_look_and_collide_bounces() {
        let look = |checks: &str| {
            let s = Settings::parse(checks);
            let t = translate("Circles", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
            let EffectParams::Circles(p) = t.params else {
                unreachable!()
            };
            (p.look, p.bounce, t.fidelity)
        };
        assert_eq!(
            look(
                "E_CHECKBOX_Circles_Bounce=1,E_CHECKBOX_Circles_Bubbles=0,E_CHECKBOX_Circles_Collide=0,\
                 E_CHECKBOX_Circles_Linear_Fade=0,E_CHECKBOX_Circles_Plasma=0,E_CHECKBOX_Circles_Radial=0,\
                 E_CHECKBOX_Circles_Radial_3D=0,E_CHECKBOX_Circles_Random_m=0,E_SLIDER_Circles_Count=5,\
                 E_SLIDER_Circles_Size=8,E_SLIDER_Circles_Speed=12"
            ),
            (CirclesLook::Solid, true, Fidelity::Exact)
        );
        assert_eq!(
            look("E_CHECKBOX_Circles_Collide=1"),
            (CirclesLook::Solid, true, Fidelity::Exact)
        );
        assert_eq!(
            look("E_CHECKBOX_Circles_Radial=1,E_CHECKBOX_Circles_Radial_3D=1,E_CHECKBOX_Circles_Plasma=1").0,
            CirclesLook::RainbowRadial
        );
        assert_eq!(look("E_CHECKBOX_Circles_Radial=1").0, CirclesLook::Radial);
        assert_eq!(
            look("E_CHECKBOX_Circles_Plasma=1,E_CHECKBOX_Circles_Linear_Fade=1").0,
            CirclesLook::Plasma
        );
        assert_eq!(look("E_CHECKBOX_Circles_Linear_Fade=1").0, CirclesLook::Fading);
        assert_eq!(look("E_CHECKBOX_Circles_Bubbles=1").0, CirclesLook::Bubbles);
        assert_eq!(
            look("E_CHECKBOX_Circles_Bubbles=1,E_CHECKBOX_Circles_Plasma=1").2,
            Fidelity::Approximate(vec![
                "bubbles' drift not applied to plasma or fading circles".into()
            ])
        );
        assert_eq!(
            look("E_CHECKBOX_Circles_Random_m=1").2,
            Fidelity::Approximate(vec!["random motion not applied".into()])
        );
        let s = Settings::parse(
            "E_SLIDER_Circles_Count=5,E_SLIDER_Circles_Size=8,E_SLIDER_Circles_Speed=12,E_SLIDER_Circles_XC=-20,E_SLIDER_Circles_YC=10",
        );
        assert_eq!(
            translate("Circles", &s, &palette(&[]), 4000, 25).unwrap().params,
            EffectParams::Circles(CirclesParams {
                count: 5,
                size: 8,
                speed: 12.0,
                look: CirclesLook::Solid,
                bounce: false,
                center_x: -20.0,
                center_y: 10.0,
            })
        );
    }

    #[test]
    fn pinwheels_translate_exactly_and_old_ones_keep_the_old_method() {
        let s = Settings::parse(
            "E_CHECKBOX_Pinwheel_Rotation=0,E_CHOICE_Pinwheel_3D=Sweep,E_CHOICE_Pinwheel_Style=New Render Method,\
             E_SLIDER_PinwheelXC=25,E_SLIDER_PinwheelYC=-10,E_SLIDER_Pinwheel_ArmSize=400,E_SLIDER_Pinwheel_Arms=12,\
             E_SLIDER_Pinwheel_Offset=45,E_SLIDER_Pinwheel_Speed=6,E_SLIDER_Pinwheel_Thickness=50,\
             E_SLIDER_Pinwheel_Twist=-360",
        );
        let t = translate("Pinwheel", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        assert_eq!(
            t.params,
            EffectParams::Pinwheel(PinwheelParams {
                arms: 12,
                arm_size: 400.0,
                twist: -360.0,
                thickness: 50.0,
                speed: 6.0,
                counterclockwise: false,
                shading: PinwheelShading::Sweep,
                offset: 45.0,
                center_x: 25.0,
                center_y: -10.0,
                style: PinwheelStyle::Smooth,
            })
        );
        // Files from before xLights 2026.06 without a style keep the old method; newer ones the
        // new one.
        for (version, style) in [
            ("2024.19", PinwheelStyle::Spokes),
            ("2026.06", PinwheelStyle::Smooth),
        ] {
            let mut s = Settings::default();
            adjust_for_version("Pinwheel", &mut s, version);
            let EffectParams::Pinwheel(p) =
                translate("Pinwheel", &s, &palette(&[]), 1000, 25).unwrap().params
            else {
                unreachable!()
            };
            assert_eq!(p.style, style, "{version}");
        }
        // Curves on the center and twist.
        let s = Settings::parse(
            "E_SLIDER_PinwheelXC=25,\
             E_VALUECURVE_PinwheelXC=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-50.00|P2=50.00|RV=TRUE|",
        );
        let t = translate("Pinwheel", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(t.curves["centerX"], Curve::ramp(-50.0, 50.0));
    }

    #[test]
    fn snowflakes_translate_their_look_and_motion() {
        let s = Settings::parse(
            "E_CHOICE_Falling=Falling & Accumulating,E_SLIDER_Snowflakes_Count=69,E_SLIDER_Snowflakes_Speed=34,\
             E_SLIDER_Snowflakes_Type=8,E_SLIDER_Snowflakes_WarmupFrames=12",
        );
        let t = translate("Snowflakes", &s, &palette(&[Rgb::WHITE]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Snowflakes(SnowflakesParams {
                count: 69,
                flake: SnowflakeShape::Diamond,
                speed: 34.0,
                motion: SnowflakesMotion::PilingUp,
                warmup: 12,
            })
        );
        for (setting, flake, motion) in [
            (
                "E_SLIDER_Snowflakes_Type=0",
                SnowflakeShape::Random,
                SnowflakesMotion::Blowing,
            ),
            (
                "E_SLIDER_Snowflakes_Type=2,E_CHOICE_Falling=Falling",
                SnowflakeShape::Cross,
                SnowflakesMotion::Falling,
            ),
            (
                "E_SLIDER_Snowflakes_Type=9",
                SnowflakeShape::X,
                SnowflakesMotion::Blowing,
            ),
        ] {
            let EffectParams::Snowflakes(p) =
                translate("Snowflakes", &Settings::parse(setting), &palette(&[]), 1000, 50)
                    .unwrap()
                    .params
            else {
                unreachable!()
            };
            assert_eq!((p.flake, p.motion), (flake, motion), "{setting}");
        }
        let plain = translate("Snowflakes", &Settings::default(), &palette(&[]), 1000, 50).unwrap();
        assert_eq!(
            plain.params,
            EffectParams::Snowflakes(SnowflakesParams::default())
        );
    }

    #[test]
    fn plasma_and_butterfly_translate_exactly() {
        let s = Settings::parse(
            "E_CHOICE_Plasma_Color=Preset Colors 3,E_SLIDER_Plasma_Line_Density=6,E_SLIDER_Plasma_Speed=16,\
             E_SLIDER_Plasma_Style=2",
        );
        let t = translate("Plasma", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Plasma(PlasmaParams {
                colors: PlasmaColors::Rainbow,
                twist: 2,
                density: 6,
                speed: 16.0,
            })
        );
        let s = Settings::parse(
            "E_CHOICE_Butterfly_Colors=Palette,E_CHOICE_Butterfly_Direction=Reverse,E_SLIDER_Butterfly_Chunks=3,\
             E_SLIDER_Butterfly_Skip=4,E_SLIDER_Butterfly_Speed=90,E_SLIDER_Butterfly_Style=7,\
             E_VALUECURVE_Butterfly_Speed=Active=TRUE|Type=Ramp|Min=0.00|Max=100.00|P1=10.00|P2=90.00|RV=TRUE|",
        );
        let t = translate("Butterfly", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Butterfly(ButterflyParams {
                pattern: 7,
                colors: ButterflyColors::Palette,
                speed: 10.0,
                direction: Direction::Reverse,
                chunks: 3,
                skip: 4,
            })
        );
        assert_eq!(t.curves["speed"], Curve::ramp(10.0, 90.0));
    }

    #[test]
    fn garlands_and_lines_read_tenths_from_either_control() {
        let s = Settings::parse(
            "E_CHOICE_Garlands_Direction=Left then Right,E_SLIDER_Garlands_Spacing=75,E_SLIDER_Garlands_Type=1,\
             E_TEXTCTRL_Garlands_Cycles=2.5",
        );
        let t = translate("Garlands", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Garlands(GarlandsParams {
                shape: GarlandShape::SmallSwags,
                spacing: 75.0,
                cycles: 2.5,
                direction: GarlandsDirection::LeftThenRight,
            })
        );
        let slider = Settings::parse("E_SLIDER_Garlands_Cycles=30");
        let EffectParams::Garlands(p) = translate("Garlands", &slider, &palette(&[]), 4000, 50)
            .unwrap()
            .params
        else {
            unreachable!()
        };
        assert_eq!(p.cycles, 3.0);
        // Lines from before xLights 2026.07 keep their whole-number speed on the slider.
        let mut s = Settings::parse(
            "E_CHECKBOX_Lines_FadeTrails=0,E_SLIDER_Lines_Objects=6,E_SLIDER_Lines_Segments=2,\
             E_SLIDER_Lines_Speed=3,E_SLIDER_Lines_Thickness=4,E_SLIDER_Lines_Trails=5",
        );
        adjust_for_version("Lines", &mut s, "2024.19");
        let t = translate("Lines", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Lines(LinesParams {
                count: 6,
                points: 2,
                thickness: 4,
                speed: 3.0,
                trails: 5,
                fade_trails: false,
            })
        );
        let newer = Settings::parse("E_TEXTCTRL_Lines_Speed=2.5");
        let EffectParams::Lines(p) = translate("Lines", &newer, &palette(&[]), 4000, 50)
            .unwrap()
            .params
        else {
            unreachable!()
        };
        assert_eq!(p.speed, 2.5);
    }

    #[test]
    fn life_and_tendril_translate_and_music_movements_say_so() {
        let s = Settings::parse("E_SLIDER_Life_Count=100,E_SLIDER_Life_Seed=3,E_SLIDER_Life_Speed=3");
        let t = translate("Life", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Life(LifeParams {
                density: 100,
                rules: LifeRules::Coagulations,
                speed: 3,
            })
        );
        let s = Settings::parse(
            "E_CHOICE_Tendril_Movement=Vertical Zig Zag,E_TEXTCTRL_Tendril_Dampening=15,\
             E_TEXTCTRL_Tendril_Friction=11,E_TEXTCTRL_Tendril_Length=97,E_TEXTCTRL_Tendril_ManualX=5,\
             E_TEXTCTRL_Tendril_ManualY=6,E_TEXTCTRL_Tendril_Speed=10,E_TEXTCTRL_Tendril_Tension=37,\
             E_TEXTCTRL_Tendril_Thickness=9,E_TEXTCTRL_Tendril_Trails=2,E_TEXTCTRL_Tendril_TuneMovement=3,\
             E_TEXTCTRL_Tendril_XOffset=-16,E_TEXTCTRL_Tendril_YOffset=-9",
        );
        let t = translate("Tendril", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(
            t.params,
            EffectParams::Tendril(TendrilParams {
                movement: TendrilMovement::VerticalZigZag,
                movement_size: 3.0,
                thickness: 9.0,
                tendrils: 2,
                length: 97,
                speed: 10,
                friction: 11,
                dampening: 15,
                tension: 37,
                offset_x: -16.0,
                offset_y: -9.0,
                manual_x: 5.0,
                manual_y: 6.0,
            })
        );
        let music = Settings::parse("E_CHOICE_Tendril_Movement=Music Circle");
        let t = translate("Tendril", &music, &palette(&[]), 4000, 50).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["movement that follows the music shown as a circle".into()])
        );
    }

    #[test]
    fn text_translates_with_its_font_noted() {
        let s = Settings::parse(
            "E_CHECKBOX_TextNoRepeat=1,E_CHECKBOX_TextToCenter=0,E_CHECKBOX_Text_Color_PerWord=1,\
             E_CHOICE_Text_Count=none,E_CHOICE_Text_Dir=up,E_CHOICE_Text_Effect=vert text down,\
             E_CHOICE_Text_Font=Use OS Fonts,E_FONTPICKER_Text_Font=bold 'arial narrow' 12 utf-8,\
             E_SLIDER_Text_XStart=10,E_SLIDER_Text_YStart=-20,E_TEXTCTRL_Text=Happy&comma; days,\
             E_TEXTCTRL_Text_Speed=12",
        );
        let t = translate("Text", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["'arial narrow' font shown in PixelFlow's pixel font".into()])
        );
        assert_eq!(
            t.params,
            EffectParams::Text(TextParams {
                text: "Happy, days".into(),
                movement: TextMovement::Up,
                speed: 12,
                size: 12,
                orientation: TextOrientation::StackedDown,
                to_center: false,
                no_repeat: true,
                start_x: 10.0,
                start_y: -20.0,
                end_x: 0.0,
                end_y: 0.0,
                pixel_offsets: false,
                color_per_word: true,
                countdown: TextCountdown::None,
            })
        );
        // xLights' own fonts give their height; what PixelFlow can't show says so.
        let s = Settings::parse(
            "E_CHOICE_Text_Font=7-7x9 Bold,E_CHOICE_Text_Dir=word-flip,E_CHOICE_Text_Effect=rotate up 45,\
             E_CHOICE_Text_Count=to date 's',E_TEXTCTRL_Text=${TITLE}",
        );
        let t = translate("Text", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        let EffectParams::Text(p) = &t.params else {
            unreachable!()
        };
        assert_eq!(p.size, 9);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "song details in the text shown as written".into(),
                "'7-7x9 Bold' font shown in PixelFlow's pixel font".into(),
                "'word-flip' movement shown still".into(),
                "'rotate up 45' text shown upright".into(),
                "countdown 'to date 's'' shown as the text itself".into(),
            ])
        );
    }

    #[test]
    fn curve_through_keeps_only_the_points_it_needs() {
        let line: Vec<(f32, f32)> = (0..=10).map(|i| (i as f32 / 10.0, i as f32 * 2.0)).collect();
        assert_eq!(curve_through(&line), Some(Curve::ramp(0.0, 20.0)));
        assert_eq!(curve_through(&[(0.0, 3.0), (0.5, 3.0), (1.0, 3.0)]), None);
        let step = curve_through(&[(0.0, 0.0), (0.5, 0.0), (0.5, 4.0), (1.0, 4.0)]).unwrap();
        assert_eq!(
            step,
            Curve::custom(0.0, 4.0, vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [1.0, 1.0]])
        );
    }

    #[test]
    fn render_styles_and_transforms_carry_over() {
        let s = Settings::parse(
            "B_CHOICE_BufferStyle=Per Model Default,B_CHOICE_BufferTransform=Rotate CC 90 Flip Horizontal",
        );
        let t = translate("On", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(t.render_style, RenderStyle::PerModelDefault);
        assert_eq!(t.buffer_transform, BufferTransform::RotateCcw90FlipHorizontal);
        let s = Settings::parse("B_CHOICE_BufferStyle=Overlay - Centered,B_CHOICE_BufferTransform=Twist");
        let t = translate("On", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.render_style, RenderStyle::OverlayCentered);
        assert_eq!(t.buffer_transform, BufferTransform::None);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["'Twist' buffer transformation not applied".into()])
        );
    }

    #[test]
    fn unsupported_blends_transitions_and_buffers_are_approximations() {
        let s = Settings::parse(
            "T_CHOICE_LayerMethod=Effect 2,T_TEXTCTRL_Fadein=1,T_CHOICE_In_Transition_Type=Wipe,B_CHOICE_BufferStyle=Vertical Per Strand,B_SLIDER_Rotation=45",
        );
        let t = translate("Off", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(t.blend, Blend::Normal);
        let Fidelity::Approximate(reasons) = t.fidelity else {
            panic!("{:?}", t.fidelity)
        };
        assert_eq!(
            reasons,
            vec![
                "'Effect 2' layer blending shown as Normal",
                "'Wipe' transition shown as a fade",
                "'Vertical Per Strand' render style not applied",
                "rotation or zoom not applied",
            ]
        );
    }

    #[test]
    fn settings_beyond_pixelflows_ranges_are_clamped_and_reported() {
        // 25 wraps is more twist than PixelFlow's spiral allows.
        let s = Settings::parse("E_SLIDER_Spirals_Rotation=250,E_TEXTCTRL_Spirals_Movement=1.0");
        let t = translate("Spirals", &s, &palette(&[Rgb::RED]), 1000, 25).unwrap();
        let EffectParams::Spiral(p) = t.params else {
            panic!("{:?}", t.params)
        };
        assert_eq!(p.twist, 10.0);
        assert_eq!(t.params.setting_problem(), None);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "settings beyond PixelFlow's range set to the nearest it allows".into()
            ])
        );
        // Every kind, with hostile settings, comes out inside the table.
        let wild = Settings::parse(
            "E_SLIDER_Bars_BarCount=1e300,E_TEXTCTRL_Bars_Cycles=-1e300,E_TEXTCTRL_Chase_Rotations=1e300,\
             E_SLIDER_Twinkle_Steps=0,E_SLIDER_Strobe_Duration=0,E_TEXTCTRL_Wave_Speed=1e300,\
             E_SLIDER_Meteors_Speed=1e300,E_TEXTCTRL_Ripple_Cycles=1e300,E_SLIDER_Ripple_Thickness=0,\
             E_TEXTCTRL_Shape_Count=1e300,E_SLIDER_Shapes_Velocity=-1e300,E_SLIDER_Fan_Revolutions=1e300,\
             E_SLIDER_Fan_Num_Blades=-5,E_SLIDER_Morph_Repeat_Count=1e300,E_SLIDER_Circles_Size=1e300,\
             E_SLIDER_Pinwheel_Arms=1e300,E_SLIDER_Snowflakes_Type=-3,E_TEXTCTRL_Garlands_Cycles=1e300,\
             E_TEXTCTRL_Lines_Speed=-1e300,E_SLIDER_Life_Speed=0,E_TEXTCTRL_Tendril_Length=1e300,\
             E_TEXTCTRL_Text_Speed=1e300,E_FONTPICKER_Text_Font='x' 1e300",
        );
        for name in [
            "On",
            "Off",
            "Color Wash",
            "Bars",
            "Single Strand",
            "Marquee",
            "Wave",
            "Twinkle",
            "Shimmer",
            "Strobe",
            "Spirals",
            "Fire",
            "Meteors",
            "Ripple",
            "Plasma",
            "Faces",
            "Shape",
            "Fan",
            "Morph",
            "Circles",
            "Pinwheel",
            "Snowflakes",
            "Butterfly",
            "Garlands",
            "Lines",
            "Life",
            "Tendril",
            "Text",
        ] {
            for duration in [1, 25, 3_600_000] {
                let t = translate(name, &wild, &palette(&[Rgb::RED]), duration, 10).unwrap();
                assert_eq!(
                    t.params.setting_problem(),
                    None,
                    "{name} {duration}: {:?}",
                    t.params
                );
            }
        }
    }

    #[test]
    fn tally_notes_list_placeholders_with_counts_and_approximations_with_reasons() {
        let mut tally = Tally::default();
        for _ in 0..12 {
            tally.record("Butterfly", &Fidelity::Placeholder);
        }
        for _ in 0..5 {
            tally.record("Plasma", &Fidelity::Placeholder);
        }
        tally.record("Bars", &Fidelity::Approximate(vec!["a".into(), "b".into()]));
        tally.record("Bars", &Fidelity::Approximate(vec!["a".into()]));
        tally.record("On", &Fidelity::Exact);
        assert_eq!(
            tally.notes(),
            vec![
                "PixelFlow has no matching effect yet for these xLights effects, so they are shown as a dim fill in each one's first color: Butterfly (12), Plasma (5).",
                "Bars (2 effects) approximated: a, b (1).",
            ]
        );
    }
}
