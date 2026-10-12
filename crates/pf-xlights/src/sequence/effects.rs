//! Translating one xLights effect (its name, settings, and palette) into a PixelFlow effect.
//!
//! Settings keys and defaults follow xLights' `src-core/effects/*Effect.cpp`; speeds that xLights
//! counts per effect ("cycles") become per-second rates using the effect's length.
//!
//! Settings that change over the effect (value curves) are translated the same way at each point
//! where they change, and every PixelFlow setting that changes as a result gets a curve through
//! those values (see [`param_curves`]).

use super::curves::{Driven, STEPS, XlCurve};
use super::list;
use super::look::{ByVerdict, LookGroup, Verdict};
use super::settings::{ParsedPalette, Settings};
use pf_model::{BufferTransform, RenderStyle};
use pf_sequence::{
    Axis, BarsMotion, BarsParams, Blend, ButterflyColors, ButterflyParams, ChaseParams, CirclesLook,
    CirclesParams, ColorWashParams, Curve, CurveShape, Direction, EffectParams, FaceColorSource, FaceEyes,
    FacesParams, FanParams, FillDirection, FillParams, FireLocation, FireParams, FireStyle, GarlandShape,
    GarlandsDirection, GarlandsParams, Gradient, LifeParams, LifeRules, LightningParams, LinesParams,
    MAX_CURVE_CYCLES, MIN_CURVE_CYCLES, MeteorDirection, MeteorsParams, MorphParams, OffParams, OnParams,
    Palette, PictureFit, PictureMovement, PictureParams, PictureResize, PictureTiming, PinwheelParams,
    PinwheelShading, PinwheelStyle, PlasmaColors, PlasmaParams, Rgb, RippleParams, SettingRange, ShapeObject,
    ShapeParams, ShimmerParams, ShockwaveParams, SnowflakeShape, SnowflakesMotion, SnowflakesParams,
    SpiralParams, StrobeParams, TendrilMovement, TendrilParams, TextCountdown, TextMovement, TextOrientation,
    TextParams, TwinkleParams, VuMeterParams, VuMeterShape, VuMeterType, WaveParams,
};
use std::collections::BTreeMap;

/// Brightness of a placeholder (an effect PixelFlow can't draw yet), so it reads as a stand-in.
pub const PLACEHOLDER_LEVEL: f32 = 0.25;

/// What the importer carried over of an xLights effect. How the result is expected to look is
/// a [`Verdict`], worked out from this (see [`super::look`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Fidelity {
    /// Every setting that changes its look has a PixelFlow setting.
    Exact,
    /// What a viewer would see differ, in a few words each ("'Expand' direction shown as
    /// straight movement").
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
    pub music_sparkles: bool,
    /// PixelFlow's blur (xLights' Blur minus one).
    pub blur: u32,
    /// The color panel's hue, saturation, and value shifts (−1 to 1), brightness (1 = as drawn),
    /// and contrast.
    pub hue_adjust: f32,
    pub saturation_adjust: f32,
    pub value_adjust: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub render_style: RenderStyle,
    pub buffer_transform: BufferTransform,
    /// Settings that change over the effect, by PixelFlow setting key.
    pub curves: BTreeMap<String, Curve>,
    /// The curves that follow a timing track, by setting key, and the track's name (the
    /// importer finds the track: see `Builder::effect`).
    pub curve_tracks: Vec<(String, String)>,
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

/// A PixelFlow curve from `from` to `to` that follows what `curve` (a music or timing-track
/// curve) follows, and the timing track it follows, by name.
fn driven_curve(curve: &XlCurve, from: f32, to: f32, diff: &mut Diff) -> (Curve, Option<String>) {
    let shaped = |shape| Curve::shaped(shape, from, to, 1.0);
    let c = match curve.kind() {
        "Music" => Curve::music(from, to, curve.param(3), false),
        "Inverted Music" => Curve::music(from, to, curve.param(3), true),
        "Music Trigger Fade" => Curve {
            trigger: curve.param(3),
            fade: curve.param(4),
            ..shaped(CurveShape::MusicTrigger)
        },
        "Timing Track Toggle" => shaped(CurveShape::TimingToggle),
        "Timing Track Fade Fixed" => Curve {
            fade: curve.param(3),
            ..shaped(CurveShape::TimingFade)
        },
        _ => Curve {
            fade: curve.param(3),
            ..shaped(CurveShape::TimingFadeSpan)
        },
    };
    if curve.other_audio().is_some() {
        diff.add("settings that follow another audio track follow the sequence's music");
    }
    let track = match curve.driven() {
        Some(Driven::TimingTrack) => {
            if curve.label_filter().is_some() {
                diff.add("settings that follow a timing track follow every mark, not only the labels chosen");
            }
            curve.timing_track().map(str::to_string)
        }
        _ => None,
    };
    (c, track)
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
/// `convert` turning each value (slider units) into PixelFlow's: the value at the start, the
/// curve when it changes, and the timing track it follows, by name.
fn setting_curve(
    curve: &XlCurve,
    convert: impl Fn(f64) -> f32,
    diff: &mut Diff,
) -> (f32, Option<Curve>, Option<String>) {
    if curve.driven().is_some() {
        let (lo, hi) = curve.driven_ends();
        let (c, track) = driven_curve(curve, convert(lo), convert(hi), diff);
        return (convert(curve.middle()), Some(c), track);
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
    (samples[0].1, curve_through(&samples), None)
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
                diff.add("stays as it starts where xLights changes it over the effect");
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

/// xLights' On (`OnEffect::Render`): the color dimmed to its level and still solid, see-through
/// only by its transparency, and with shimmer on, every other frame the second color (or
/// nothing).
fn on(r: &Reader) -> EffectParams {
    let start = r.get("Eff_On_Start", 100.0, 0.0, 100.0);
    let end = r.get("Eff_On_End", 100.0, 0.0, 100.0);
    EffectParams::On(OnParams {
        gradient: Gradient::None,
        start_level: unit(start / 100.0),
        end_level: unit(end / 100.0),
        solid: true,
        transparency: unit(r.get("On_Transparency", 0.0, 0.0, 100.0) / 100.0),
        shimmer: r.check("On_Shimmer"),
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
        diff.add("the color cycle stops at its last color instead of running on into the first");
    }
    EffectParams::ColorWash(ColorWashParams {
        cycles: cycles as f32,
        gradient: Gradient::None,
    })
}

/// xLights' Bars (`BarsEffect::Render`), which PixelFlow's Bars draws the same way: the palette
/// `repeats` times over in bars with no gaps, moving a palette's length each cycle (a bar at a
/// time: the whole prop's).
fn bars(r: &Reader, n_colors: f64, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    use BarsMotion::{Slide, Split, Step};
    let repeats = r.get("Bars_BarCount", 1.0, 1.0, 5.0);
    let cycles = r.get("Bars_Cycles", 1.0, 0.0, 30.0);
    let center = r.get("Bars_Center", 0.0, -100.0, 100.0);
    let center_moves = r.s.curve_active("E_VALUECURVE_Bars_Center");
    let way = r.choice("Bars_Direction", "up");
    let (axis, reverse, motion, moving) = match way {
        "up" => (Axis::Vertical, false, Slide, true),
        "down" => (Axis::Vertical, true, Slide, true),
        "Right" => (Axis::Horizontal, false, Slide, true),
        "Left" => (Axis::Horizontal, true, Slide, true),
        "expand" => (Axis::Vertical, false, Split, true),
        "compress" => (Axis::Vertical, true, Split, true),
        "H-expand" => (Axis::Horizontal, false, Split, true),
        "H-compress" => (Axis::Horizontal, true, Split, true),
        "Alternate Up" => (Axis::Vertical, false, Step, true),
        "Alternate Down" => (Axis::Vertical, true, Step, true),
        "Alternate Right" => (Axis::Horizontal, false, Step, true),
        "Alternate Left" => (Axis::Horizontal, true, Step, true),
        // Bars that stand still, set along the prop by the center.
        "Custom Horz" | "Custom Vert" => {
            if center != 0.0 || center_moves {
                diff.add(format!(
                    "'{way}' bars not moved along the prop by the center setting"
                ));
            }
            let axis = if way == "Custom Horz" {
                Axis::Horizontal
            } else {
                Axis::Vertical
            };
            (axis, false, Slide, false)
        }
        other => {
            // "Custom": the closest of up, down, left, and right to the angle.
            diff.add(format!("'{other}' direction shown as straight movement"));
            let angle = r.get("Bars_Angle", 90.0, -180.0, 180.0);
            if angle.abs() <= 45.0 {
                (Axis::Horizontal, false, Slide, true)
            } else if angle.abs() >= 135.0 {
                (Axis::Horizontal, true, Slide, true)
            } else {
                (Axis::Vertical, angle < 0.0, Slide, true)
            }
        }
    };
    // xLights reads a center that changes by where the bars are in their cycle, not the effect.
    if motion == Split && center_moves && (cycles - 1.0).abs() > 1e-6 {
        diff.add("a center that changes follows the effect, not each cycle of the bars");
    }
    let highlight = r.check("Bars_Highlight");
    if highlight && r.check("Bars_UseFirstColorForHighlight") && n_colors > 1.0 {
        diff.add("highlight in the palette's first color shown white, with that color as a bar");
    }
    // On a layer that isn't Normal, xLights takes the color out of an up-and-down bar's highlight
    // rather than making it white.
    if highlight && axis == Axis::Vertical && r.s.text("T_CHOICE_LayerMethod", "Normal") != "Normal" {
        diff.add("highlight shown white rather than the bar's color greyed");
    }
    EffectParams::Bars(BarsParams {
        count: count(repeats * n_colors, 1, 1000),
        speed: match motion {
            _ if !moving => 0.0,
            Step => rate(cycles, duration_ms),
            Slide | Split => rate(cycles / repeats, duration_ms),
        },
        axis,
        direction: direction(reverse),
        motion,
        fill: 1.0,
        center: if motion == Split { center as f32 } else { 0.0 },
        shaded: r.check("Bars_3D"),
        highlight,
        gradient: r.check("Bars_Gradient"),
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
                ..ChaseParams::default()
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
                diff.add("starts at the end of the prop, not part-way along");
            }
            if !r.choice("SingleStrand_TimingTrack", "").is_empty() {
                diff.add("steps a band on each timing mark instead of chasing between marks");
            }
            Kind::Params(EffectParams::Chase(ChaseParams {
                speed: if moving { rate(rotations, duration_ms) } else { 0.0 },
                width: unit(size / 100.0 * chases),
                bands: count(chases, 1, 20),
                direction: direction(reverse),
                bounce,
                ..ChaseParams::default()
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
        ..ChaseParams::default()
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
        diff.add("drawn across the middle of the prop, not moved up or down");
    }
    EffectParams::Wave(WaveParams {
        cycles: cycles as f32,
        speed: speed as f32,
        height: unit(r.get("Wave_Height", 50.0, 0.0, 100.0) / 100.0),
        thickness: unit(r.get("Thickness_Percentage", 5.0, 0.0, 100.0) / 100.0),
        direction: direction(r.choice("Wave_Direction", "Right to Left") == "Right to Left"),
    })
}

/// The share of the pixels a strobing twinkle lights in any one frame, as a multiple of its
/// count (a share of the pixels) over its steps (frames): see [`twinkle`].
const STROBING_TWINKLE_SHARE: f64 = 1.72;

/// How far over one flash a frame a strobing twinkle's flashes come, so that every frame falls
/// just inside a flash of its own and never, by rounding, at the end of the one before (which
/// has gone dark by then). Small enough that four hours of the fastest frames stay inside.
const OVER_A_FRAME: f64 = 2e-7;

fn twinkle(r: &Reader, frame: f64, diff: &mut Diff) -> EffectParams {
    let steps = r.get("Twinkle_Steps", 30.0, 2.0, 400.0);
    let count = r.get("Twinkle_Count", 3.0, 2.0, 100.0) / 100.0;
    if r.check("Twinkle_Strobe") {
        // What xLights shows, measured on frames it rendered (its newer way of drawing
        // twinkles, count 20, steps 98, 50 ms frames, on the pixels nothing else lit): no pixel
        // fades up or down. In any one frame 0.35% of them are lit (593 flashes in 168,724
        // pixel-frames), each at full brightness and for that frame alone, and when a pixel
        // flashes is chance (how many times each flashed in 330 frames follows the Poisson
        // distribution for that rate). That is PixelFlow's Strobe with a flash a frame. The
        // share is taken to rise with the count and fall with the steps, as a soft twinkle's
        // does.
        if r.choice("Twinkle_Style", "") == "New Render Method" {
            return EffectParams::Strobe(StrobeParams {
                rate: (1000.0 / frame * (1.0 + OVER_A_FRAME)) as f32,
                density: unit(STROBING_TWINKLE_SHARE * count / steps),
            });
        }
        diff.add("strobing twinkles shown as soft twinkles");
    }
    EffectParams::Twinkle(TwinkleParams {
        density: unit(count),
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
    diff.add("about as many flashes, not exactly as many");
    if r.get("Strobe_Type", 1.0, 1.0, 4.0) > 1.0 {
        diff.add("flash shapes shown as single pixels");
    }
    if r.check("Strobe_Music") {
        diff.add("flashes don't follow the music");
    }
    EffectParams::Strobe(StrobeParams {
        // A new set of flashes every `frames` frames.
        rate: (1000.0 / (frames * frame)) as f32,
        density: unit(r.get("Number_Strobes", 3.0, 1.0, 300.0) / 100.0).max(0.01),
    })
}

/// Lightning (xLights' `LightningEffect`). xLights grows one bolt from the bottom (or down from
/// the top) over its first `Number_Bolts` frames, redrawing its jags (up to `Number_Segments`
/// cells sideways) every frame, and holds it to the end; PixelFlow strikes again and again, each
/// strike flickering and fading, so the bolt count becomes its jags.
fn lightning(r: &Reader, diff: &mut Diff) -> EffectParams {
    diff.add("shown as strikes that flicker and fade instead of one bolt held for the effect");
    if r.choice("Lightning_Direction", "Up") != "Down" {
        diff.add("bolts strike down from the top");
    }
    if r.get("Lightning_TopX", 0.0, -50.0, 50.0) != 0.0
        || r.get("Lightning_TopY", 0.0, 0.0, 100.0) != 0.0
        || r.get("Lightning_BOTX", 0.0, -50.0, 50.0) != 0.0
    {
        diff.add("bolts start at the middle of the top and don't drift");
    }
    EffectParams::Lightning(LightningParams {
        density: 4.0,
        branches: if r.check("ForkedLightning") { 0.5 } else { 0.0 },
        thickness: count(r.get("Lightning_WIDTH", 1.0, 1.0, 7.0), 1, 10),
        segments: count(r.get("Number_Bolts", 10.0, 1.0, 50.0), 2, 30),
        ..LightningParams::default()
    })
}

fn spirals(r: &Reader, n_colors: f64, duration_ms: u64) -> EffectParams {
    let repeats = r.get("Spirals_Count", 1.0, 1.0, 5.0);
    let movement = r.get("Spirals_Movement", 1.0, -20.0, 20.0);
    EffectParams::Spiral(SpiralParams {
        // One arm per palette color, repeated.
        count: count(repeats * n_colors, 1, 1000),
        speed: rate(movement.abs(), duration_ms),
        thickness: unit(r.get("Spirals_Thickness", 50.0, 0.0, 100.0) / 100.0),
        // xLights shifts each row by `y * Rotation / height` cells, Rotation being the slider's
        // number as it is (Spirals stores its slider, not the text box, and `SpiralsEffect`
        // reads it undivided): cells across the height, not turns. It leans the other way from
        // PixelFlow's.
        twist: 0.0,
        lean: -r
            .s
            .num("E_SLIDER_Spirals_Rotation")
            .or_else(|| r.s.num("E_TEXTCTRL_Spirals_Rotation"))
            .unwrap_or(0.0)
            .clamp(-300.0, 300.0) as f32,
        direction: direction(movement < 0.0),
        shaded: r.check("Spirals_3D"),
        gradient: r.check("Spirals_Blend"),
        grow: r.check("Spirals_Grow"),
        shrink: r.check("Spirals_Shrink"),
    })
}

/// xLights' Fire (`FireEffect::Render`), which PixelFlow's flickering flames draw the same way.
/// Where each flame lands is random on both sides.
fn fire(r: &Reader) -> EffectParams {
    let hue_shift = r.get("Fire_HueShift", 0.0, 0.0, 100.0);
    EffectParams::Fire(FireParams {
        height: unit(r.get("Fire_Height", 50.0, 1.0, 100.0) / 100.0),
        style: FireStyle::Flicker,
        location: match r.choice("Fire_Location", "Bottom") {
            "Top" => FireLocation::Top,
            "Left" => FireLocation::Left,
            "Right" => FireLocation::Right,
            _ => FireLocation::Bottom,
        },
        hue_shift: unit(hue_shift.trunc() / 100.0),
        growth_cycles: r.get("Fire_GrowthCycles", 0.0, 0.0, 20.0) as f32,
        grow_with_music: r.check("Fire_GrowWithMusic"),
        ..FireParams::default()
    })
}

/// A hue-shifted fire's dim flames are both darker and see-through in xLights on a layer blended
/// Normal, and only darker on any other, where PixelFlow's stay both.
fn dim_flames(params: &EffectParams, curves: &BTreeMap<String, Curve>, blend: Blend, diff: &mut Diff) {
    let EffectParams::Fire(p) = params else {
        return;
    };
    if blend != Blend::Normal && (p.hue_shift > 0.0 || curves.contains_key("hueShift")) {
        diff.add("a hue-shifted fire's dim flames are dimmer than xLights' under this layer blending");
    }
}

fn meteors(r: &Reader, diff: &mut Diff) -> EffectParams {
    diff.add("about as many meteors at about the speed");
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

/// xLights' Shockwave (`ShockwaveEffect::Render`), which PixelFlow's Shockwave draws the same
/// way. A setting xLights leaves out of the file has the value its panel starts with.
fn shockwave(r: &Reader, diff: &mut Diff) -> EffectParams {
    let cycles = r.get("Shockwave_Cycles", 1.0, 1.0, 100.0);
    // xLights reads its value curves at the ring's place in its own cycle; PixelFlow's curves
    // run once over the effect.
    let moving =
        r.s.keys()
            .any(|k| k.starts_with("E_VALUECURVE_Shockwave_") && r.s.curve_active(k));
    if cycles >= 2.0 && moving {
        diff.add("settings that change over each ring change once over the effect");
    }
    if !r.choice("Shockwave_TimingTrack", "").is_empty() {
        diff.add("rings fired by a timing track shown one after another over the effect");
    }
    EffectParams::Shockwave(ShockwaveParams {
        center_x: r.get("Shockwave_CenterX", 50.0, 0.0, 100.0) as f32,
        center_y: r.get("Shockwave_CenterY", 50.0, 0.0, 100.0) as f32,
        start_radius: r.get("Shockwave_Start_Radius", 1.0, 0.0, 750.0) as f32,
        end_radius: r.get("Shockwave_End_Radius", 10.0, 0.0, 750.0) as f32,
        start_width: r.get("Shockwave_Start_Width", 5.0, 0.0, 255.0) as f32,
        end_width: r.get("Shockwave_End_Width", 10.0, 0.0, 255.0) as f32,
        cycles: count(cycles, 1, 100),
        acceleration: r.get("Shockwave_Accel", 0.0, -10.0, 10.0) as f32,
        blend_edges: r.check_or("Shockwave_Blend_Edges", true),
        scale: r.check_or("Shockwave_Scale", true),
    })
}

/// xLights' Fill (`FillEffect::Render`), which PixelFlow's Fill draws the same way.
fn fill(r: &Reader) -> EffectParams {
    EffectParams::Fill(FillParams {
        position: r.get("Fill_Position", 100.0, 0.0, 100.0) as f32,
        direction: match r.choice("Fill_Direction", "Up") {
            "Down" => FillDirection::Down,
            "Left" => FillDirection::Left,
            "Right" => FillDirection::Right,
            _ => FillDirection::Up,
        },
        band_size: r.get("Fill_Band_Size", 0.0, 0.0, 250.0) as f32,
        skip_size: r.get("Fill_Skip_Size", 0.0, 0.0, 250.0) as f32,
        offset: r.get("Fill_Offset", 0.0, 0.0, 100.0) as f32,
        offset_in_cells: r.check_or("Fill_Offset_In_Pixels", true),
        wrap: r.check_or("Fill_Wrap", true),
        color_by_time: r.check("Fill_Color_Time"),
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

/// A note for [`pictures`] of how the xLights that saved the sequence resized a picture. Not a
/// setting of xLights' own.
const PICTURES_RESIZED: &str = "PIXELFLOW_Pictures_Resized";

/// Settings xLights changes when it opens a file saved by an older version (each effect's
/// `adjustSettings`), so the effect looks as it does in xLights today. A picture is the
/// exception: xLights has resized pictures four ways, and an old sequence keeps the way of the
/// version that saved it, which is the one its lights were last seen with.
pub fn adjust_for_version(name: &str, s: &mut Settings, version: &str) {
    match effect_key(name).as_str() {
        // xLights resized with `wxImage::Rescale` until 2026.04, and what that does changed with
        // the wxWidgets it was built on: the nearest pixel until 2024.16; then four pixels mixed
        // when shrinking, with the nearest pixel still for enlarging (taken from each cell's
        // middle from 2025.04); from 2025.06 a box average. Since 2026.04 it filters.
        "pictures" => {
            let how = if version_older("2024.16", version) {
                "nearest"
            } else if version_older("2025.04", version) {
                "mixed"
            } else if version_older("2025.06", version) {
                "mixed, middle"
            } else {
                return;
            };
            s.set(PICTURES_RESIZED, how.into());
        }
        // Fan radii became a share of the prop in 2025.04; older fans keep them in pixels.
        "fan" if version_older("2025.04", version) => s.set("E_CHECKBOX_Fan_Scale", "0".into()),
        // So did a Shockwave's sizes.
        "shockwave" if version_older("2025.04", version) => {
            s.set("E_CHECKBOX_Shockwave_Scale", "0".into());
        }
        // The first Fills were colored by time (until 2016.41) and always wrapped (until 2016.53).
        "fill" => {
            if version_older("2016.41", version) {
                s.set("E_CHECKBOX_Fill_Color_Time", "1".into());
            }
            if version_older("2016.53", version) {
                s.set("E_CHECKBOX_Fill_Wrap", "1".into());
            }
        }
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
    let fire_on_music = !use_timing && r.check("Shape_UseMusic");
    if use_timing && !r.s.text("E_TEXTCTRL_Shape_FilterLabel", "").is_empty() {
        diff.add("shapes appear on every mark, not only the labels chosen");
    }
    let random_movement = r.check("Shapes_RandomMovement");
    if random_movement && (frame - 50.0).abs() > 0.5 {
        diff.add("shapes drift at about the speed, not exactly");
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
        fire_on_music,
        trigger_level: r.get("Shape_Sensitivity", 50.0, 0.0, 100.0) as f32,
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
        diff.add("plasma and fading circles don't drift the way bubbles do");
    }
    if r.check("Circles_Random_m") && !rings {
        diff.add("circles keep to straight paths instead of wandering");
    }
    if !rings && r.s.curve_active("E_VALUECURVE_Circles_Speed") {
        diff.add("circles that speed up or slow down do so roughly");
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
///
/// Its settings come across number for number, but for the thickness of smooth arms, which is
/// carried over as the width such an arm measures in xLights' picture (see [`smooth_thickness`]).
/// Spokes keep their number: PixelFlow counts their rays as that picture shows them.
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
    let arms = count(r.get("Pinwheel_Arms", 3.0, 1.0, 20.0), 1, 20);
    let style = if r.choice("Pinwheel_Style", "New Render Method") == "New Render Method" {
        PinwheelStyle::Smooth
    } else {
        PinwheelStyle::Spokes
    };
    let thickness = r.get("Pinwheel_Thickness", 0.0, 0.0, 100.0);
    let thickness = match style {
        PinwheelStyle::Smooth => smooth_thickness(thickness, arms),
        PinwheelStyle::Spokes => thickness,
    };
    EffectParams::Pinwheel(PinwheelParams {
        arms,
        arm_size: r.get("Pinwheel_ArmSize", 100.0, 0.0, 400.0) as f32,
        twist: r.get("Pinwheel_Twist", 0.0, -360.0, 360.0) as f32,
        thickness: thickness as f32,
        speed: r.get("Pinwheel_Speed", 10.0, 0.0, 50.0) as f32,
        counterclockwise: r.check_or("Pinwheel_Rotation", true),
        shading,
        offset: r.get("Pinwheel_Offset", 0.0, 0.0, 360.0) as f32,
        center_x: r.get("PinwheelXC", 0.0, -100.0, 100.0) as f32,
        center_y: r.get("PinwheelYC", 0.0, -100.0, 100.0) as f32,
        style,
    })
}

/// The thickness that makes PixelFlow's smooth Pinwheel arm as wide as the one xLights shows for
/// `thickness` with `arms` arms.
///
/// PixelFlow's arm is exactly the share of its sector its thickness says. Measured on xLights'
/// picture over ten arm counts and every thickness, an arm there is wider, and comes in whole
/// degrees: the thickness counts in whole percent, and under 1 as 1; that share of the sector
/// (360 degrees over the arms) is an angle; the arm is that angle's whole degrees and one more,
/// up to arms that touch. So 5% of one arm's circle, 18 degrees, shows as 19; 1% of four arms'
/// 90-degree sectors, 0.9 degrees, shows as 1; and no thickness at all shows as 4, 2, and 2
/// degrees for one, two, and three arms.
fn smooth_thickness(thickness: f64, arms: u32) -> f64 {
    let arms = u64::from(arms.max(1));
    let percent = (thickness.clamp(0.0, 100.0) as u64).max(1);
    // Whole degrees in `percent` of a sector of 360 / arms degrees, and one more.
    let degrees = percent * 360 / (100 * arms) + 1;
    (degrees as f64 * arms as f64 / 3.6).min(100.0)
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

/// A Tendril (xLights' `TendrilEffect`).
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
        "Music Line" => M::MusicLine,
        "Music Circle" => M::MusicCircle,
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

/// What a VU Meter draws, from xLights' name for it.
fn vu_type(name: &str, diff: &mut Diff) -> VuMeterType {
    use VuMeterType as T;
    match name {
        "Spectrogram" => T::Spectrogram,
        "Spectrogram Peak" => T::SpectrogramPeak,
        "Spectrogram Line" => T::SpectrogramLine,
        "Spectrogram Circle Line" => T::SpectrogramCircleLine,
        "Volume Bars" => T::VolumeBars,
        "Waveform" => T::Waveform,
        "On" => T::On,
        "Color On" => T::ColorOn,
        "Dominant Frequency Colour" => T::DominantFrequencyColor,
        "Dominant Frequency Colour Gradient" => T::DominantFrequencyColorGradient,
        "Intensity Wave" => T::IntensityWave,
        "Pulse" => T::Pulse,
        "Level Bar" => T::LevelBar,
        "Level Random Bar" => T::LevelRandomBar,
        "Level Color" => T::LevelColor,
        "Level Pulse" => T::LevelPulse,
        "Level Pulse Color" => T::LevelPulseColor,
        "Level Jump" => T::LevelJump,
        "Level Jump 100" => T::LevelJump100,
        "Level Shape" => T::LevelShape,
        "Timing Event Bar" => T::TimingEventBar,
        "Timing Event Bar Bounce" => T::TimingEventBarBounce,
        "Timing Event Random Bar" => T::TimingEventRandomBar,
        "Timing Event Bars" => T::TimingEventBars,
        "Timing Event Spike" => T::TimingEventSpike,
        "Timing Event Sweep" => T::TimingEventSweep,
        "Timing Event Sweep 2" => T::TimingEventSweep2,
        "Timing Event Timed Sweep" => T::TimingEventTimedSweep,
        "Timing Event Timed Sweep 2" => T::TimingEventTimedSweep2,
        "Timing Event Alternate Timed Sweep" => T::TimingEventAlternateTimedSweep,
        "Timing Event Alternate Timed Sweep 2" => T::TimingEventAlternateTimedSweep2,
        "Timing Event Timed Chase From Middle" => T::TimingEventChaseFromMiddle,
        "Timing Event Timed Chase To Middle" => T::TimingEventChaseToMiddle,
        "Timing Event Color" => T::TimingEventColor,
        "Timing Event Jump" => T::TimingEventJump,
        "Timing Event Jump 100" => T::TimingEventJump100,
        "Timing Event Pulse" => T::TimingEventPulse,
        "Timing Event Pulse Color" => T::TimingEventPulseColor,
        "Note On" => T::NoteOn,
        "Note Level Pulse" => T::NoteLevelPulse,
        "Note Level Jump" => T::NoteLevelJump,
        "Note Level Jump 100" => T::NoteLevelJump100,
        "Note Level Bar" => T::NoteLevelBar,
        "Note Level Random Bar" => T::NoteLevelRandomBar,
        // xLights draws the waveform within each frame from the raw samples, which the audio
        // track doesn't keep.
        "Frame Waveform" => {
            diff.add("'Frame Waveform' shown as a waveform across frames");
            T::Waveform
        }
        // An unknown type draws volume bars in xLights.
        other => {
            diff.add(format!("'{other}' shown as volume bars"));
            T::VolumeBars
        }
    }
}

/// The shape a VU Meter's Level Shape draws.
fn vu_shape(name: &str, diff: &mut Diff) -> VuMeterShape {
    use VuMeterShape as S;
    match name {
        "Circle" => S::Circle,
        "Filled Circle" => S::FilledCircle,
        "Square" => S::Square,
        "Filled Square" => S::FilledSquare,
        "Diamond" => S::Diamond,
        "Filled Diamond" => S::FilledDiamond,
        "Star" => S::Star,
        "Filled Star" => S::FilledStar,
        "Tree" => S::Tree,
        "Filled Tree" => S::FilledTree,
        "Crucifix" => S::Crucifix,
        "Filled Crucifix" => S::FilledCrucifix,
        "Present" => S::Present,
        "Filled Present" => S::FilledPresent,
        "Candy Cane" => S::CandyCane,
        "Snowflake" => S::Snowflake,
        "Heart" => S::Heart,
        "Filled Heart" => S::FilledHeart,
        other => {
            diff.add(format!("'{other}' level shape shown as a circle"));
            S::Circle
        }
    }
}

/// A VU Meter (xLights' `VUMeterEffect`). Its timing track is looked up by name when the effect is
/// placed (see `Builder::effect`).
fn vu_meter(r: &Reader, diff: &mut Diff) -> EffectParams {
    let meter = vu_type(r.choice("VUMeter_Type", "Waveform"), diff);
    let shape = r.choice("VUMeter_Shape", "Circle");
    let shape = if meter == VuMeterType::LevelShape {
        vu_shape(shape, diff)
    } else {
        vu_shape(shape, &mut Diff(Vec::new()))
    };
    let filter = r.s.text("E_TEXTCTRL_Filter", "").to_string();
    if !filter.is_empty() && r.check("Regex") && meter.uses_marks() {
        diff.add("label filter read as plain text, not a pattern");
    }
    let audio = r.choice("VUMeter_AudioTrack", "");
    if !audio.is_empty() && audio != "Main" {
        diff.add("follows the sequence's music, not another audio track");
    }
    let note = |id: &str, default: f64| count(r.get(id, default, 0.0, 127.0), 0, 126);
    EffectParams::VuMeter(VuMeterParams {
        meter,
        bars: count(r.get("VUMeter_Bars", 6.0, 1.0, 100.0), 1, 100),
        sensitivity: count(r.get("VUMeter_Sensitivity", 70.0, 0.0, 100.0), 0, 100),
        gain: r.get("VUMeter_Gain", 0.0, -100.0, 100.0) as f32,
        timing_track: None,
        shape,
        slow_falls: r.check_or("VUMeter_SlowDownFalls", true),
        start_note: note("VUMeter_StartNote", 36.0),
        end_note: note("VUMeter_EndNote", 84.0),
        log_x: r.check("VUMeter_LogarithmicX"),
        x_offset: r.get("VUMeter_XOffset", 0.0, -100.0, 100.0) as f32,
        y_offset: r.get("VUMeter_YOffset", 0.0, -100.0, 100.0) as f32,
        filter,
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
/// when the font doesn't say, where xLights takes the size of the computer's own text: 13 on a
/// Mac).
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
    let size = font_height(xl_font, os_font);
    diff.add(match size {
        Some(_) => format!("'{font}' font shown in PixelFlow's pixel font"),
        None => format!(
            "'{font}' font shown in PixelFlow's pixel font, 10 pixels high where xLights uses the \
             computer's own text size"
        ),
    });
    let size = size.unwrap_or(10.0);
    // The pixel font has the printable ASCII characters; xLights draws the rest (emoji too)
    // with the computer's fonts.
    if words.chars().any(|c| !matches!(c, ' '..='~' | '\n')) {
        diff.add("characters PixelFlow's pixel font doesn't have (accents, emoji) shown as boxes");
    }
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

/// The picture file a Pictures effect names: xLights has stored it under three controls over the
/// years (the newest first).
fn picture_file(s: &Settings) -> &str {
    [
        "E_TEXTCTRL_Pictures_Filename",
        "E_FILEPICKERCTRL_Pictures_Filename",
        "E_FILEPICKER_Pictures_Filename",
    ]
    .iter()
    .map(|key| s.text(key, "").trim())
    .find(|file| !file.is_empty())
    .unwrap_or("")
}

/// How a Pictures effect sizes its picture: xLights' `Scaling` choice, or the "Scale to fit"
/// checkbox sequences older than 2017.6 have instead.
fn picture_fit(r: &Reader, diff: &mut Diff) -> PictureFit {
    let old = if r.check("Pictures_ScaleToFit") {
        "Scale To Fit"
    } else {
        "No Scaling"
    };
    match r.choice("Scaling", old) {
        "No Scaling" => PictureFit::Actual,
        "Scale To Fit" => PictureFit::Stretch,
        "Scale Keep Aspect Ratio" => PictureFit::Fit,
        "Scale Keep Aspect Ratio Crop" => PictureFit::Fill,
        other => {
            diff.add(format!("'{other}' scaling shown pixel for pixel"));
            PictureFit::Actual
        }
    }
}

/// Pictures (xLights' `PicturesEffect`): the file as the sequence names it (the importer then
/// looks for it on this computer: see `find_pictures`), sized, placed, and scrolled as xLights
/// does. What changes over the effect besides its value curves (a size from start to end, a
/// "vector" move) becomes curves in [`picture_curves`].
///
/// - Sizing, offsets, black made clear, and scrolling left, right, up, and down match.
/// - An animated GIF plays at its own speed when xLights loops it, and is spread over the
///   effect when it doesn't. PixelFlow keeps each frame's share of the pass where xLights gives
///   every frame the same time: the same thing for a GIF whose frames are equally long.
/// - Diagonal moves, peekaboo, wiggle, flag wave, and tiling have no match and are noted.
fn pictures(r: &Reader, duration_ms: u64, diff: &mut Diff) -> EffectParams {
    let file = picture_file(r.s);
    let direction = r.choice("Pictures_Direction", "none");
    let speed = r.get("Pictures_Speed", 1.0, 0.0, 20.0);
    let wrap_x = r.check("Pictures_WrapX");
    use PictureMovement as M;
    let (movement, wrap) = match direction {
        "none" | "vector" => (M::None, wrap_x),
        "left" => (M::Left, wrap_x),
        "right" => (M::Right, wrap_x),
        "up" => (M::Up, false),
        "down" => (M::Down, false),
        "up once" | "down once" => {
            if speed > 1.0 + 1e-6 {
                diff.add(format!("'{direction}' crosses again and again instead of once"));
            }
            (if direction == "up once" { M::Up } else { M::Down }, false)
        }
        "up-left" | "down-left" | "up-right" | "down-right" => {
            let (way, movement) = if direction.ends_with("left") {
                ("left", M::Left)
            } else {
                ("right", M::Right)
            };
            diff.add(format!("'{direction}' movement shown moving {way}"));
            (movement, wrap_x)
        }
        "zoom in" => {
            diff.add("'zoom in' grows from the middle to the picture's size, not from the corner to the whole prop");
            if (speed - 1.0).abs() > 1e-6 {
                diff.add("repeated zooming shown as one zoom over the effect");
            }
            (M::ZoomIn, false)
        }
        "tile-left" | "tile-right" | "tile-up" | "tile-down" => {
            diff.add(format!(
                "'{direction}' shown as one picture wrapping round rather than tiles, at a speed of its own"
            ));
            let movement = match direction {
                "tile-left" => M::Left,
                "tile-right" => M::Right,
                "tile-up" => M::Up,
                _ => M::Down,
            };
            (movement, true)
        }
        other => {
            // Peekaboo (four ways), wiggle, and flag wave.
            diff.add(format!("'{other}' movement not shown: the picture stays still"));
            (M::None, wrap_x)
        }
    };
    if wrap_x && !wrap && !matches!(movement, M::None | M::Left | M::Right) {
        diff.add("wrapping across not shown on a picture that isn't moving across");
    }
    let scrolls = matches!(movement, M::Left | M::Right | M::Up | M::Down);
    let fit = picture_fit(r, diff);
    // xLights only scales a picture shown pixel for pixel.
    let scale = match fit {
        PictureFit::Actual => r.get("Pictures_StartScale", 100.0, 0.0, 1000.0),
        _ => 100.0,
    };
    // Which pixels a resized picture takes depends on the xLights that saved the sequence (see
    // `adjust_for_version`). PixelFlow has the two oldest ways; for the rest it averages.
    let (resize, differs) = match r.s.text(PICTURES_RESIZED, "") {
        "nearest" => (PictureResize::XlightsNearest, None),
        "mixed" => (PictureResize::XlightsMixed, None),
        "mixed, middle" => (
            PictureResize::XlightsMixed,
            Some(
                "enlarged from each cell's corner pixel, where xLights 2025.04 and 2025.05 took its middle one",
            ),
        ),
        _ => (
            PictureResize::Own,
            Some("resized by averaging its pixels, where xLights uses another filter"),
        ),
    };
    let resized = fit != PictureFit::Actual
        || scale != 100.0
        || r.get("Pictures_EndScale", 100.0, 0.0, 1000.0) != 100.0;
    if let (true, Some(differs)) = (resized, differs) {
        diff.add(differs);
    }
    // A scrolling picture's offset up is taken downward by xLights.
    let up = r.get("PicturesYC", 0.0, -100.0, 100.0);
    let y_offset = if scrolls || movement == M::ZoomIn { -up } else { up };
    let stem = file
        .rsplit(['/', '\\'])
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map_or("", |(stem, _)| stem);
    if stem.ends_with("-1") {
        diff.add("numbered picture files (name-1, name-2, and so on) shown as the first picture alone");
    }
    if r.check("Pictures_Shimmer") {
        diff.add("shimmer not shown");
    }
    if !r.check_or("SuppressGIFBackground", true) {
        diff.add("a GIF's background color not drawn");
    }
    EffectParams::Picture(PictureParams {
        file: file.to_string(),
        fit,
        timing: if r.check("LoopGIF") {
            PictureTiming::Loop
        } else {
            PictureTiming::Stretch
        },
        play_speed: r.get("Pictures_FrameRateAdj", 1.0, 0.0, 100.0) as f32,
        movement,
        move_speed: if scrolls {
            rate(speed, duration_ms)
        } else {
            PictureParams::default().move_speed
        },
        wrap,
        x_offset: r.get("PicturesXC", 0.0, -100.0, 100.0) as f32,
        y_offset: y_offset as f32,
        scale: scale as f32,
        black_transparent: r.check("Pictures_TransparentBlack"),
        // xLights' level is red + green + blue (0-765); PixelFlow's a share of full white.
        black_level: (r.get("Pictures_TransparentBlack", 0.0, 0.0, 765.0) / 7.65) as f32,
        pixel_offsets: r.check("Pictures_PixelOffsets"),
        resize,
        ..PictureParams::default()
    })
}

/// What an xLights Pictures effect changes over the effect besides its value curves: its size
/// from the start scale to the end scale (shown pixel for pixel), and a "vector" move from its
/// offsets to its end offsets. xLights repeats both as often as the effect's speed says.
fn picture_curves(
    s: &Settings,
    params: &EffectParams,
    curves: &mut BTreeMap<String, Curve>,
    diff: &mut Diff,
) {
    let EffectParams::Picture(p) = params else {
        return;
    };
    let r = Reader { s };
    let direction = r.choice("Pictures_Direction", "none");
    let speed = r.get("Pictures_Speed", 1.0, 0.0, 20.0) as f32;
    let mut ramps: Vec<(&str, f32, f32)> = Vec::new();
    if p.fit == PictureFit::Actual {
        let end = r.get("Pictures_EndScale", 100.0, 0.0, 1000.0) as f32;
        ramps.push(("scale", p.scale, end.clamp(1.0, 1000.0)));
    }
    if direction == "vector" {
        // xLights reads a "vector" move's offsets from the sliders alone: a value curve on them
        // does nothing.
        curves.remove("xOffset");
        curves.remove("yOffset");
        ramps.push((
            "xOffset",
            p.x_offset,
            r.get("PicturesEndXC", 0.0, -100.0, 100.0) as f32,
        ));
        ramps.push((
            "yOffset",
            p.y_offset,
            r.get("PicturesEndYC", 0.0, -100.0, 100.0) as f32,
        ));
    }
    ramps.retain(|(_, from, to)| (from - to).abs() > 1e-6);
    if ramps.is_empty() {
        return;
    }
    let once = (speed - 1.0).abs() <= 1e-6 || direction.ends_with("once");
    let repeats = (MIN_CURVE_CYCLES..=MAX_CURVE_CYCLES).contains(&speed);
    if !once && !repeats {
        diff.add("a size or position that changes over and over shown changing once over the effect");
    }
    for (key, from, to) in ramps {
        let curve = if once || !repeats {
            Curve::ramp(from, to)
        } else {
            Curve::shaped(CurveShape::Saw, from, to, speed)
        };
        curves.insert(key.to_string(), curve);
    }
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
        "on" => on(&r),
        "off" => EffectParams::Off(OffParams {}),
        "colorwash" => color_wash(&r, colors, diff),
        "bars" => bars(&r, n_colors, duration_ms, diff),
        "singlestrand" => return single_strand(&r, duration_ms, diff),
        "marquee" => marquee(&r, diff),
        "wave" => wave(&r, diff),
        "twinkle" => twinkle(&r, frame, diff),
        "shimmer" => shimmer(&r, duration_ms, diff),
        "strobe" => strobe(&r, frame, diff),
        "spirals" => spirals(&r, n_colors, duration_ms),
        "fire" => fire(&r),
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
        "vumeter" => vu_meter(&r, diff),
        "pictures" => pictures(&r, duration_ms, diff),
        "shockwave" => shockwave(&r, diff),
        "fill" => fill(&r),
        // No direct equivalent: the closest PixelFlow effect, with its default settings.
        "fireworks" => closest("twinkles", EffectParams::Twinkle(TwinkleParams::default())),
        "snowstorm" => closest("falling meteors", EffectParams::Meteors(MeteorsParams::default())),
        "galaxy" => closest("a spiral", EffectParams::Spiral(SpiralParams::default())),
        "curtain" => closest("moving bars", EffectParams::Bars(BarsParams::default())),
        "lightning" => lightning(&r, diff),
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
        diff.add("mixed with the layers below at full strength, not part-way");
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
        diff.add(format!(
            "drawn across the whole target, not in xLights' '{name}' arrangement"
        ));
        RenderStyle::Default
    });
    let name = s.text("B_CHOICE_BufferTransform", "None");
    let transform = BufferTransform::from_xlights(name).unwrap_or_else(|| {
        diff.add(format!("not turned or flipped the way '{name}' does"));
        BufferTransform::None
    });
    let changed = |key: &str, neutral: f64| s.num(key).is_some_and(|v| (v - neutral).abs() > 1e-6);
    // Rotation, zoom, and pivot curves move the buffer too.
    let moving = s
        .keys()
        .any(|k| k.starts_with("B_VALUECURVE_") && k != "B_VALUECURVE_Blur" && s.curve_active(k));
    if changed("B_SLIDER_Rotation", 0.0) || changed("B_SLIDER_Zoom", 1.0) || moving {
        diff.add("not rotated or zoomed");
    }
    if !s.text("B_CUSTOM_SubBuffer", "").is_empty() {
        diff.add("drawn over the whole prop, not just the part xLights draws on");
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
/// the start, the curve, and the timing track it follows.
fn blur_curve(s: &Settings, diff: &mut Diff) -> Option<(u32, Option<Curve>, Option<String>)> {
    let curve = XlCurve::parse_in(s.get("B_VALUECURVE_Blur")?, 1.0, 15.0)?;
    let max = f64::from(pf_sequence::MAX_BLUR + 1);
    let (start, curve, track) = setting_curve(&curve, |v| (v.trunc().clamp(1.0, max) - 1.0) as f32, diff);
    Some((start as u32, curve, track))
}

/// The sparkles' value curve (`C_VALUECURVE_SparkleFrequency`, 0-200): the value at the start, the
/// curve, and the timing track it follows.
fn sparkles_curve(palette: &ParsedPalette, diff: &mut Diff) -> Option<(u32, Option<Curve>, Option<String>)> {
    let curve = XlCurve::parse_in(palette.sparkles_curve.as_deref()?, 0.0, 200.0)?;
    let max = f64::from(pf_sequence::MAX_SPARKLES);
    let (start, curve, track) = setting_curve(&curve, |v| v.trunc().clamp(0.0, max) as f32, diff);
    Some((start as u32, curve, track))
}

/// The color panel's adjustments in PixelFlow's terms: the hue, saturation, and value shifts and
/// the contrast as fractions, the brightness as a factor.
struct Adjustments {
    hue: f32,
    saturation: f32,
    value: f32,
    brightness: f32,
    contrast: f32,
}

/// The palette's color adjustments (`PixelBufferClass::SetLayerSettings` reads the sliders as
/// whole percents), each at its value at the start, with a curve in `curves` where its value
/// curve changes it over the effect (the brightness curve's values are whole percents too).
fn adjustments(
    palette: &ParsedPalette,
    curves: &mut BTreeMap<String, Curve>,
    curve_tracks: &mut Vec<(String, String)>,
    diff: &mut Diff,
) -> Adjustments {
    let shift = |percent: f64| (percent / 100.0).clamp(-1.0, 1.0) as f32;
    let factor =
        |percent: f64| (percent.trunc() / 100.0).clamp(0.0, f64::from(pf_sequence::MAX_BRIGHTNESS)) as f32;
    let brightness = if palette.brightness.is_finite() {
        palette.brightness
    } else {
        100.0
    };
    let mut adjust = Adjustments {
        hue: shift(palette.hue_adjust),
        saturation: shift(palette.saturation_adjust),
        value: shift(palette.value_adjust),
        brightness: factor(brightness),
        contrast: shift(palette.contrast),
    };
    // A setting's value curve, over the range xLights gives it: `setting` starts where it does.
    let mut follow = |key: &str,
                      text: &Option<String>,
                      setting: &mut f32,
                      (least, most): (f32, f32),
                      convert: &dyn Fn(f64) -> f32| {
        let Some(curve) = text.as_deref().and_then(|t| XlCurve::parse_in(t, least, most)) else {
            return;
        };
        let (start, curve, track) = setting_curve(&curve, convert, diff);
        *setting = start;
        curves.extend(curve.map(|c| (key.to_string(), c)));
        curve_tracks.extend(track.map(|t| (key.to_string(), t)));
    };
    const SHIFT: (f32, f32) = (-100.0, 100.0);
    follow("hueAdjust", &palette.hue_curve, &mut adjust.hue, SHIFT, &shift);
    follow(
        "saturationAdjust",
        &palette.saturation_curve,
        &mut adjust.saturation,
        SHIFT,
        &shift,
    );
    follow(
        "valueAdjust",
        &palette.value_curve,
        &mut adjust.value,
        SHIFT,
        &shift,
    );
    follow(
        "brightness",
        &palette.brightness_curve,
        &mut adjust.brightness,
        (0.0, 400.0),
        &factor,
    );
    adjust
}

/// The palette's colors, as they are: its brightness and other adjustments are the effect's own
/// settings (see [`adjustments`]).
fn palette_colors(palette: &ParsedPalette, diff: &mut Diff) -> Vec<Rgb> {
    if palette.color_curves > 0 {
        diff.add("color curves shown as their first color");
    }
    if palette.unreadable > 0 {
        diff.add("unreadable palette colors left out");
    }
    for extra in &palette.extras {
        diff.add(format!("drawn without xLights' {extra}"));
    }
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
        .copied()
        .collect()
}

/// Translates one xLights effect lasting `duration_ms` (the length the file gives it, even when
/// the sequence ends before it does) in a sequence with `frame_ms` frames. `None` for effects
/// that are left out ([`Fidelity::Skipped`]).
pub fn translate(
    name: &str,
    s: &Settings,
    palette: &ParsedPalette,
    duration_ms: u64,
    frame_ms: u32,
) -> Option<Translated> {
    let mut diff = Diff(Vec::new());
    let colors = palette_colors(palette, &mut diff);
    let (mut sparkles, sparkle_color) = (palette.sparkles, palette.sparkle_color);
    // xLights' music sparkles: the count times the music's peak (nothing to follow without any).
    let music_sparkles = palette.music_sparkles && palette.sparkles > 0;
    let blend = blend(s, &mut diff);
    let (fade_in_ms, fade_out_ms) = fades(s, duration_ms, &mut diff);
    let Buffer {
        mut blur,
        style: render_style,
        transform: buffer_transform,
    } = buffer(s, &mut diff);
    let mut curves = BTreeMap::new();
    let mut curve_tracks = Vec::new();
    if let Some((start, curve, track)) = sparkles_curve(palette, &mut diff) {
        sparkles = start;
        curves.extend(curve.map(|c| ("sparkles".to_string(), c)));
        curve_tracks.extend(track.map(|t| ("sparkles".to_string(), t)));
    }
    if let Some((start, curve, track)) = blur_curve(s, &mut diff) {
        blur = start;
        curves.extend(curve.map(|c| ("blur".to_string(), c)));
        curve_tracks.extend(track.map(|t| ("blur".to_string(), t)));
    }
    let adjust = adjustments(palette, &mut curves, &mut curve_tracks, &mut diff);
    // Settings that change over the effect start at their first value; music and timing-track
    // ones at their middle (where they sit without the music or the track), and get curves that
    // follow them (see `follow`).
    let mut start = s.clone();
    let mut moving = Vec::new();
    let mut driven = Vec::new();
    for (id, curve) in effect_curves(s) {
        if curve.driven().is_some() {
            put(&mut start, &id, curve.middle());
            driven.push((id, curve));
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
                picture_curves(&start, &clamped, &mut curves, &mut diff);
                dim_flames(&clamped, &curves, blend, &mut diff);
                for (id, curve) in &driven {
                    let at = |value: f64| {
                        let mut at = start.clone();
                        put(&mut at, id, value);
                        match effect_params(name, &at, &colors, duration_ms, frame_ms, &mut Diff(Vec::new()))
                        {
                            Kind::Params(p) if p.kind() == clamped.kind() => Some(p.sanitized()),
                            _ => None,
                        }
                    };
                    let (lo, hi) = curve.driven_ends();
                    let (Some(low), Some(high), Some(mid)) = (at(lo), at(hi), at((lo + hi) / 2.0)) else {
                        diff.add(
                            "settings that follow the music or a timing track held at their middle value",
                        );
                        continue;
                    };
                    for spec in clamped.kind().settings() {
                        let whole = match spec.range {
                            SettingRange::Number { .. } => false,
                            SettingRange::Int { .. } => true,
                            _ => continue,
                        };
                        let (Some(a), Some(b), Some(m)) =
                            (low.number(spec.key), high.number(spec.key), mid.number(spec.key))
                        else {
                            continue;
                        };
                        if (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0) {
                            continue;
                        }
                        if curves.contains_key(spec.key) {
                            diff.add("a setting that also changes over the effect doesn't follow the music or timing track too");
                            continue;
                        }
                        // PixelFlow's curve is straight between the two ends: exact when the
                        // setting translates straight across (most do).
                        let off = (m - (a + b) / 2.0).abs();
                        if off > if whole { 0.51 } else { 1e-3 * (b - a).abs() } {
                            diff.add(
                                "a setting that follows the music or a timing track does so approximately",
                            );
                        }
                        let (c, track) = driven_curve(curve, a, b, &mut diff);
                        curves.insert(spec.key.to_string(), c);
                        curve_tracks.extend(track.map(|t| (spec.key.to_string(), t)));
                    }
                }
                Translated {
                    params: clamped,
                    palette: Palette::new(colors),
                    blend,
                    fade_in_ms,
                    fade_out_ms,
                    sparkles,
                    sparkle_color,
                    music_sparkles,
                    blur,
                    hue_adjust: adjust.hue,
                    saturation_adjust: adjust.saturation,
                    value_adjust: adjust.value,
                    brightness: adjust.brightness,
                    contrast: adjust.contrast,
                    render_style,
                    buffer_transform,
                    curves,
                    curve_tracks,
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
                    ..OnParams::default()
                }),
                palette: Palette::new(colors.first().copied().map_or_else(Vec::new, |c| vec![c])),
                blend,
                fade_in_ms,
                fade_out_ms,
                sparkles,
                sparkle_color,
                music_sparkles,
                blur,
                hue_adjust: adjust.hue,
                saturation_adjust: adjust.saturation,
                value_adjust: adjust.value,
                brightness: adjust.brightness,
                contrast: adjust.contrast,
                render_style,
                buffer_transform,
                curves,
                curve_tracks,
                fidelity: Fidelity::Placeholder,
            },
        },
    )
}

/// What a placeholder shows, for the report.
pub const PLACEHOLDER_LOOK: &str = "shown as a dim fill in the effect's first color";

/// How each kind of xLights effect came across, for the report.
#[derive(Debug, Default)]
pub struct Tally {
    /// Per verdict and xLights effect name: how many, for how long, and what differs.
    groups: BTreeMap<(Verdict, String), Group>,
    /// Effects left out, by name.
    skipped: BTreeMap<String, usize>,
}

#[derive(Debug, Default)]
struct Group {
    effects: usize,
    ms: u64,
    reasons: BTreeMap<String, usize>,
}

impl Tally {
    /// Counts an effect `ms` long that came across as `fidelity`. Returns how it's expected to
    /// look; `None` for one left out.
    pub fn record(&mut self, name: &str, fidelity: &Fidelity, ms: u64) -> Option<Verdict> {
        let name = if name.is_empty() { "(unnamed)" } else { name };
        if *fidelity == Fidelity::Skipped {
            *self.skipped.entry(name.to_string()).or_default() += 1;
            return None;
        }
        let (verdict, why) = super::look::expected(name, fidelity);
        let group = self.groups.entry((verdict, name.to_string())).or_default();
        group.effects += 1;
        group.ms += ms;
        let noted: &[String] = match fidelity {
            Fidelity::Approximate(reasons) => reasons,
            _ => &[],
        };
        for reason in noted.iter().map(String::as_str).chain(why) {
            *group.reasons.entry(reason.to_string()).or_default() += 1;
        }
        if verdict == Verdict::NoEquivalent {
            group.reasons.insert(PLACEHOLDER_LOOK.to_string(), group.effects);
        }
        Some(verdict)
    }

    /// Effects, and how long they run added up, by verdict.
    pub fn counts(&self) -> (ByVerdict<usize>, ByVerdict<u64>) {
        let (mut effects, mut ms) = (ByVerdict::default(), ByVerdict::default());
        for ((verdict, _), group) in &self.groups {
            effects.add(*verdict, group.effects);
            ms.add(*verdict, group.ms);
        }
        (effects, ms)
    }

    /// The effects that don't come across the same, worst first, and within a verdict the kind
    /// with most effects first.
    pub fn looks(&self) -> Vec<LookGroup> {
        let mut looks: Vec<LookGroup> = self
            .groups
            .iter()
            .filter(|((verdict, _), _)| *verdict != Verdict::Matches)
            .map(|((verdict, name), group)| {
                let mut reasons: Vec<(&String, &usize)> = group.reasons.iter().collect();
                reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                LookGroup {
                    verdict: *verdict,
                    effect: name.clone(),
                    effects: group.effects,
                    ms: group.ms,
                    reasons: reasons
                        .iter()
                        .map(|(reason, count)| {
                            if **count == group.effects {
                                (*reason).clone()
                            } else {
                                format!("{reason} ({count})")
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        looks.sort_by(|a, b| {
            b.verdict
                .cmp(&a.verdict)
                .then(b.effects.cmp(&a.effects))
                .then(a.effect.cmp(&b.effect))
        });
        looks
    }

    /// A note on the effects left out, when there are any.
    pub fn skipped_note(&self) -> Option<String> {
        let skipped: Vec<String> = self.skipped.iter().map(|(n, c)| format!("{n} ({c})")).collect();
        (!skipped.is_empty()).then(|| {
            format!(
                "These xLights effects change the layers below them or move fixtures, so they weren't imported (a stand-in would light the prop wrongly): {}.",
                list(&skipped)
            )
        })
    }
}

/// `looks` as sentences: the effects with no PixelFlow effect together, then one for each kind
/// that looks different or close.
pub fn look_notes(looks: &[LookGroup]) -> Vec<String> {
    let mut notes = Vec::new();
    let missing: Vec<&LookGroup> = looks
        .iter()
        .filter(|g| g.verdict == Verdict::NoEquivalent)
        .collect();
    if !missing.is_empty() {
        let names: Vec<String> = missing
            .iter()
            .map(|g| format!("{} ({})", g.effect, g.effects))
            .collect();
        let total: usize = missing.iter().map(|g| g.effects).sum();
        notes.push(format!(
            "No PixelFlow effect yet for {}, so {} shown as a dim fill in {} first color: {}.",
            if missing.len() == 1 {
                "this xLights effect"
            } else {
                "these xLights effects"
            },
            if total == 1 { "it is" } else { "they are" },
            if total == 1 { "its" } else { "each one's" },
            list(&names)
        ));
    }
    notes.extend(
        looks
            .iter()
            .filter(|g| g.verdict != Verdict::NoEquivalent)
            .map(LookGroup::sentence),
    );
    notes
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
                ..OnParams::default()
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
        // The palette keeps its colors; its brightness is the effect's.
        assert_eq!(t.palette.colors, vec![Rgb::new(200, 100, 50)]);
        assert_eq!(t.brightness, 0.5);
        assert_eq!(t.fidelity, Fidelity::Exact);
    }

    #[test]
    fn the_color_panels_adjustments_translate_exactly_with_their_curves() {
        let mut p = palette(&[Rgb::RED]);
        p.hue_adjust = -25.0;
        p.saturation_adjust = 10.0;
        p.value_adjust = -5.0;
        p.contrast = 13.0;
        p.brightness = 250.9;
        let t = translate("On", &Settings::default(), &p, 1000, 25).unwrap();
        assert_eq!(
            (
                t.hue_adjust,
                t.saturation_adjust,
                t.value_adjust,
                t.contrast,
                t.brightness
            ),
            (-0.25, 0.1, -0.05, 0.13, 2.5),
            "whole percents, and brightness can go past 100%"
        );
        assert!(t.curves.is_empty());
        assert_eq!(t.fidelity, Fidelity::Exact);
        let plain = translate("On", &Settings::default(), &palette(&[Rgb::RED]), 1000, 25).unwrap();
        assert_eq!(
            (
                plain.hue_adjust,
                plain.saturation_adjust,
                plain.value_adjust,
                plain.contrast,
                plain.brightness
            ),
            (0.0, 0.0, 0.0, 0.0, 1.0)
        );
        // Value curves: the shifts as fractions, the brightness in whole percents.
        let mut p = palette(&[Rgb::RED]);
        p.hue_curve = Some("Active=TRUE|Id=ID_VALUECURVE_Color_HueAdjust|Type=Ramp|Min=-100.00|Max=100.00|P1=-50.00|P2=75.00|RV=TRUE|".into());
        p.value_curve = Some("Active=TRUE|Id=ID_VALUECURVE_Color_ValueAdjust|Type=Ramp|Min=-100.00|Max=100.00|P1=0.00|P2=-34.00|RV=TRUE|".into());
        p.brightness_curve = Some("Active=TRUE|Id=ID_VALUECURVE_Brightness|Type=Ramp|Min=0.00|Max=400.00|P1=100.00|P2=300.00|RV=TRUE|".into());
        let t = translate("On", &Settings::default(), &p, 1000, 25).unwrap();
        assert_eq!(t.curves["hueAdjust"], Curve::ramp(-0.5, 0.75));
        assert_eq!(t.curves["valueAdjust"], Curve::ramp(0.0, -0.34));
        assert_eq!((t.hue_adjust, t.value_adjust, t.brightness), (-0.5, 0.0, 1.0));
        let Some(brightness) = t.curves.get("brightness") else {
            panic!("a brightness curve")
        };
        assert_eq!((brightness.value_at(0.0), brightness.value_at(1.0)), (1.0, 3.0));
        assert!(!t.curves.contains_key("saturationAdjust"));
        assert_eq!(t.fidelity, Fidelity::Exact);
        // What PixelFlow doesn't apply still says so.
        let mut p = palette(&[Rgb::RED]);
        p.extras = vec!["chroma key"];
        assert_eq!(
            translate("On", &Settings::default(), &p, 1000, 25)
                .unwrap()
                .fidelity,
            Fidelity::Approximate(vec!["drawn without xLights' chroma key".into()])
        );
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
            (t.sparkle_color, t.brightness),
            (Rgb::new(200, 100, 0), 0.5),
            "the effect's brightness dims its sparkles as it's drawn"
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
        // Sparkles that follow the music (a curve on them, and the music checkbox) follow it.
        assert_eq!(t.sparkles, 100, "halfway without the music");
        assert_eq!(t.curves["sparkles"], Curve::music(0.0, 200.0, 0.0, false));
        assert!(t.music_sparkles);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["blur above 15 shown at 15".into()])
        );
    }

    #[test]
    fn effect_settings_that_change_get_curves_through_the_translation() {
        // Spirals' rotation (cells across the height, on the slider) on xLights' sine: two
        // cycles, starting three quarters in, between -90 and 90 cells. PixelFlow's lean counts
        // the other way.
        let s = Settings::parse(
            "E_SLIDER_Spirals_Rotation=20,E_TEXTCTRL_Spirals_Movement=1.0,\
             E_VALUECURVE_Spirals_Rotation=Active=TRUE|Id=ID_VALUECURVE_Spirals_Rotation|Type=Sine|Min=-90.00|Max=90.00|P1=75.00|P2=90.00|P3=20.00|RV=TRUE|",
        );
        let t = translate("Spirals", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        let lean = &t.curves["lean"];
        assert_eq!(lean.shape, CurveShape::Custom);
        assert_eq!((lean.from, lean.to), (-90.0, 90.0));
        let EffectParams::Spiral(p) = t.params else {
            panic!("{:?}", t.params)
        };
        assert_eq!(
            (p.twist, p.lean),
            (0.0, 90.0),
            "the setting holds the curve's start"
        );
        for time in [0.0f64, 0.1, 0.125, 0.25, 0.4, 0.5, 0.75] {
            let r = time * 2.0 * std::f64::consts::TAU + 0.75 * std::f64::consts::TAU;
            let want = (-90.0 * r.sin()) as f32;
            assert!((lean.value_at(time as f32) - want).abs() < 0.1, "{time}: {want}");
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

        // Curves on settings that do nothing change nothing: bars going one way have no center.
        let s = Settings::parse(
            "E_VALUECURVE_Bars_Center=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-100.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Bars", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert!(t.curves.is_empty());
        // Bars from the center follow it.
        let s = Settings::parse(
            "E_CHOICE_Bars_Direction=expand,\
             E_VALUECURVE_Bars_Center=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-100.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Bars", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.curves["center"], Curve::ramp(-100.0, 100.0));
        assert_eq!(t.fidelity, Fidelity::Exact);
    }

    #[test]
    fn spirals_come_across_with_their_looks() {
        let s = Settings::parse(
            "E_CHECKBOX_Spirals_3D=1,E_CHECKBOX_Spirals_Blend=1,E_CHECKBOX_Spirals_Grow=1,\
             E_SLIDER_Spirals_Count=1,E_SLIDER_Spirals_Rotation=20,E_SLIDER_Spirals_Thickness=50,\
             E_TEXTCTRL_Spirals_Movement=1.0",
        );
        let t = translate(
            "Spirals",
            &s,
            &palette(&[Rgb::RED, Rgb::GREEN, Rgb::BLUE]),
            5000,
            50,
        )
        .unwrap();
        assert_eq!(
            t.params,
            EffectParams::Spiral(SpiralParams {
                count: 3,
                speed: 0.2,
                thickness: 0.5,
                twist: 0.0,
                lean: -20.0,
                direction: Direction::Forward,
                shaded: true,
                gradient: true,
                grow: true,
                shrink: false,
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
    }

    #[test]
    fn bars_come_across_with_their_movement_and_looks() {
        let bars = |settings: &str| {
            let t = translate(
                "Bars",
                &Settings::parse(settings),
                &palette(&[Rgb::RED, Rgb::GREEN]),
                4000,
                50,
            )
            .unwrap();
            match t.params {
                EffectParams::Bars(p) => (p, t.fidelity),
                other => panic!("{other:?}"),
            }
        };
        // xLights' defaults: one palette of bars with no gaps, going up a palette's length (the
        // whole prop) once over the effect.
        let (p, fidelity) = bars("");
        assert_eq!(
            p,
            BarsParams {
                count: 2,
                speed: 0.25,
                axis: Axis::Vertical,
                direction: Direction::Forward,
                motion: BarsMotion::Slide,
                fill: 1.0,
                center: 0.0,
                shaded: false,
                highlight: false,
                gradient: false,
            }
        );
        assert_eq!(fidelity, Fidelity::Exact);

        // Three palettes: each of the 6 cycles moves a third of the prop. 'compress' closes in on
        // the center; the looks come along.
        let (p, fidelity) = bars(
            "E_CHOICE_Bars_Direction=H-compress,E_SLIDER_Bars_BarCount=3,E_TEXTCTRL_Bars_Cycles=6.0,\
             E_TEXTCTRL_Bars_Center=57,E_CHECKBOX_Bars_3D=1,E_CHECKBOX_Bars_Gradient=1,E_CHECKBOX_Bars_Highlight=1",
        );
        assert_eq!(
            p,
            BarsParams {
                count: 6,
                speed: 0.5,
                axis: Axis::Horizontal,
                direction: Direction::Reverse,
                motion: BarsMotion::Split,
                fill: 1.0,
                center: 57.0,
                shaded: true,
                highlight: true,
                gradient: true,
            }
        );
        assert_eq!(fidelity, Fidelity::Exact);
        let (p, _) = bars("E_CHOICE_Bars_Direction=expand");
        assert_eq!(
            (p.axis, p.direction, p.motion),
            (Axis::Vertical, Direction::Forward, BarsMotion::Split)
        );

        // A bar at a time crosses the whole prop each cycle, however many palettes there are.
        let (p, fidelity) = bars(
            "E_CHOICE_Bars_Direction=Alternate Left,E_SLIDER_Bars_BarCount=3,E_TEXTCTRL_Bars_Cycles=6.0",
        );
        assert_eq!(
            (p.axis, p.direction, p.motion, p.speed),
            (Axis::Horizontal, Direction::Reverse, BarsMotion::Step, 1.5)
        );
        assert_eq!(fidelity, Fidelity::Exact);

        // 'Custom Horz' bars stand still where the center puts them: whole only in the middle.
        let (p, fidelity) = bars("E_CHOICE_Bars_Direction=Custom Horz,E_TEXTCTRL_Bars_Cycles=6.0");
        assert_eq!(
            (p.axis, p.direction, p.motion, p.speed),
            (Axis::Horizontal, Direction::Forward, BarsMotion::Slide, 0.0)
        );
        assert_eq!(fidelity, Fidelity::Exact);
        let approximate = |settings: &str, note: &str| match bars(settings).1 {
            Fidelity::Approximate(notes) => assert_eq!(notes, [note], "{settings}"),
            other => panic!("{settings}: {other:?}"),
        };
        approximate(
            "E_CHOICE_Bars_Direction=Custom Vert,E_TEXTCTRL_Bars_Center=20",
            "'Custom Vert' bars not moved along the prop by the center setting",
        );
        // What PixelFlow's bars don't do is said.
        approximate(
            "E_CHECKBOX_Bars_Highlight=1,E_CHECKBOX_Bars_UseFirstColorForHighlight=1",
            "highlight in the palette's first color shown white, with that color as a bar",
        );
        approximate(
            "E_CHECKBOX_Bars_Highlight=1,T_CHOICE_LayerMethod=Additive",
            "highlight shown white rather than the bar's color greyed",
        );
        approximate(
            "E_CHOICE_Bars_Direction=compress,E_TEXTCTRL_Bars_Cycles=2.0,\
             E_VALUECURVE_Bars_Center=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-100.00|P2=100.00|RV=TRUE|",
            "a center that changes follows the effect, not each cycle of the bars",
        );
    }

    #[test]
    fn music_and_timing_track_curves_follow_them_through_the_translation() {
        // Twinkle's count (2-100) is its density in percent: between 10 and 50 is 0.1 to 0.5.
        let s = Settings::parse(
            "E_SLIDER_Twinkle_Count=3,E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Inverted Music|Min=2.00|Max=100.00|P1=10.00|P2=50.00|P3=25.00|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact);
        let EffectParams::Twinkle(p) = &t.params else {
            panic!("{:?}", t.params)
        };
        assert!(
            (p.density - 0.3).abs() < 1e-6,
            "halfway without the music: {}",
            p.density
        );
        let density = &t.curves["density"];
        assert_eq!((density.shape, density.gain), (CurveShape::InvertedMusic, 25.0));
        assert!((density.from - 0.1).abs() < 1e-6 && (density.to - 0.5).abs() < 1e-6);
        assert!(t.curve_tracks.is_empty());

        let s = Settings::parse(
            "E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Music Trigger Fade|Min=2.00|Max=100.00|P1=10.00|P2=50.00|P3=60.00|P4=8.00|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        let density = &t.curves["density"];
        assert_eq!(
            (density.shape, density.trigger, density.fade),
            (CurveShape::MusicTrigger, 60.0, 8.0)
        );

        let s = Settings::parse(
            "E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Timing Track Fade Fixed|Min=2.00|Max=100.00|P1=10.00|P2=50.00|P3=12.00|TT=Beats|FT=kick|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        let density = &t.curves["density"];
        assert_eq!((density.shape, density.fade), (CurveShape::TimingFade, 12.0));
        assert_eq!(t.curve_tracks, vec![("density".to_string(), "Beats".to_string())]);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "settings that follow a timing track follow every mark, not only the labels chosen".into()
            ])
        );

        // A setting PixelFlow translates unevenly follows only approximately: Twinkle's steps
        // (frames a twinkle takes) become a rate, one over them.
        let s = Settings::parse(
            "E_VALUECURVE_Twinkle_Steps=Active=TRUE|Type=Music|Min=2.00|Max=200.00|P1=10.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Twinkle", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.curves["rate"].shape, CurveShape::Music);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "a setting that follows the music or a timing track does so approximately".into()
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
    fn on_comes_across_solid_with_its_shimmer_and_transparency() {
        // xLights' On dims its color and still covers what's below; only its transparency
        // lets that through.
        let s = Settings::parse(
            "E_TEXTCTRL_Eff_On_Start=80,E_TEXTCTRL_Eff_On_End=20,E_CHECKBOX_On_Shimmer=1,E_TEXTCTRL_On_Transparency=30",
        );
        let t = translate("On", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 1000, 50).unwrap();
        assert_eq!(
            t.params,
            EffectParams::On(OnParams {
                gradient: Gradient::None,
                start_level: 0.8,
                end_level: 0.2,
                solid: true,
                transparency: 0.3,
                shimmer: true,
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        // A transparency that follows the music becomes a curve that does.
        let s = Settings::parse(
            "E_VALUECURVE_On_Transparency=Active=TRUE|Id=ID_VALUECURVE_On_Transparency|Type=Music|Min=0.00|Max=100.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("On", &s, &palette(&[Rgb::RED]), 1000, 50).unwrap();
        assert_eq!(t.curves["transparency"], Curve::music(0.0, 1.0, 0.0, false));
        assert_eq!(t.fidelity, Fidelity::Exact);
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
                fire_on_music: false,
                trigger_level: 50.0,
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
            "E_CHOICE_Shape_ObjectToDraw=Emoji,E_CHECKBOX_Shape_UseMusic=1,E_CHECKBOX_Shapes_RandomMovement=1,\
             E_SLIDER_Shape_Sensitivity=35",
        );
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["'Emoji' shapes shown as circles".into()])
        );
        // Fired by the music, at its trigger level.
        let EffectParams::Shape(p) = &t.params else {
            unreachable!()
        };
        assert_eq!((p.fire_on_music, p.trigger_level), (true, 35.0));
        // A timing track wins over the music, as in xLights.
        let timed = Settings::parse(
            "E_CHECKBOX_Shape_UseMusic=1,E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Beats",
        );
        let EffectParams::Shape(p) = translate("Shape", &timed, &palette(&[]), 4000, 50)
            .unwrap()
            .params
        else {
            unreachable!()
        };
        assert!(!p.fire_on_music);
        let EffectParams::Shape(p) = t.params else {
            unreachable!()
        };
        assert_eq!(p.shape, ShapeObject::Circle);
        // Random drift is 20 pixels a frame at most in xLights: exact at 50 ms frames only.
        let t = translate("Shape", &s, &palette(&[Rgb::RED]), 4000, 25).unwrap();
        let Fidelity::Approximate(notes) = t.fidelity else {
            unreachable!()
        };
        assert!(notes.contains(&"shapes drift at about the speed, not exactly".to_string()));
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
                "plasma and fading circles don't drift the way bubbles do".into()
            ])
        );
        assert_eq!(
            look("E_CHECKBOX_Circles_Random_m=1").2,
            Fidelity::Approximate(vec!["circles keep to straight paths instead of wandering".into()])
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
            "E_CHECKBOX_Pinwheel_Rotation=0,E_CHOICE_Pinwheel_3D=Sweep,E_CHOICE_Pinwheel_Style=Old Render Method,\
             E_SLIDER_PinwheelXC=25,E_SLIDER_PinwheelYC=-10,E_SLIDER_Pinwheel_ArmSize=400,E_SLIDER_Pinwheel_Arms=12,\
             E_SLIDER_Pinwheel_Offset=45,E_SLIDER_Pinwheel_Speed=6,E_SLIDER_Pinwheel_Thickness=50,\
             E_SLIDER_Pinwheel_Twist=-360",
        );
        let t = translate("Pinwheel", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        let spokes = PinwheelParams {
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
            style: PinwheelStyle::Spokes,
        };
        assert_eq!(t.params, EffectParams::Pinwheel(spokes));
        // The new method's arms come across as wide as xLights shows them (here 16 degrees for
        // the 15 that half a 30-degree sector is: 53.33 points of thickness), and everything
        // else the same.
        let mut s = s;
        s.set("E_CHOICE_Pinwheel_Style", "New Render Method".into());
        let t = translate("Pinwheel", &s, &palette(&[Rgb::RED, Rgb::BLUE]), 4000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        assert_eq!(
            t.params,
            EffectParams::Pinwheel(PinwheelParams {
                thickness: (16.0_f64 / 30.0 * 100.0) as f32,
                style: PinwheelStyle::Smooth,
                ..spokes
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
    fn smooth_pinwheel_arms_come_across_as_wide_as_xlights_shows_them() {
        let translated = |settings: &str| {
            let t = translate("Pinwheel", &Settings::parse(settings), &palette(&[]), 4000, 25).unwrap();
            let EffectParams::Pinwheel(p) = t.params else {
                unreachable!()
            };
            (p, t.curves.get("thickness").cloned(), t.fidelity)
        };
        // The arm's width in degrees, for a thickness and a number of arms.
        let degrees = |thickness: &str, arms: u32| {
            let (p, _, fidelity) = translated(&format!(
                "E_SLIDER_Pinwheel_Arms={arms},E_TEXTCTRL_Pinwheel_Thickness={thickness}"
            ));
            assert_eq!(fidelity, Fidelity::Exact, "{arms} arms, thickness {thickness}");
            f64::from(p.thickness) * 3.6 / f64::from(arms)
        };
        let near = |got: f64, want: f64| (got - want).abs() < 1e-3;
        // The thickness's share of the sector, in whole degrees, and one more: 18 degrees show
        // as 19 and 45 as 46, but 0.9 as 1, 22.5 as 23, and 1.2 as 2.
        for (thickness, arms, want) in [
            ("5", 1, 19.0),
            ("50", 4, 46.0),
            ("1", 4, 1.0),
            ("25", 4, 23.0),
            ("3", 12, 1.0),
            ("5", 20, 1.0),
            ("1", 3, 2.0),
            ("96", 7, 50.0),
        ] {
            assert!(
                near(degrees(thickness, arms), want),
                "{arms} arms at {thickness}: {}",
                degrees(thickness, arms)
            );
        }
        // A fraction of a percent is dropped, and anything under 1 counts as 1: no thickness at
        // all shows as 4, 2, and 2 degrees for one, two, and three arms.
        assert!(near(degrees("3.7", 1), degrees("3", 1)) && near(degrees("3", 1), 11.0));
        for (arms, want) in [(1, 4.0), (2, 2.0), (3, 2.0), (4, 1.0)] {
            assert!(
                near(degrees("0", arms), want),
                "{arms} arms: {}",
                degrees("0", arms)
            );
            assert!(near(degrees("0.9", arms), want));
        }
        // Arms that touch, or whose extra degree closes the gap between them, touch.
        for (thickness, arms) in [("100", 3), ("99", 4), ("100", 7)] {
            assert_eq!(
                translated(&format!(
                    "E_SLIDER_Pinwheel_Arms={arms},E_TEXTCTRL_Pinwheel_Thickness={thickness}"
                ))
                .0
                .thickness,
                100.0
            );
        }
        assert!(near(degrees("99", 3), 119.0));
        // Spokes keep their number.
        let (spokes, _, _) = translated(
            "E_CHOICE_Pinwheel_Style=Old Render Method,E_SLIDER_Pinwheel_Arms=4,E_SLIDER_Pinwheel_Thickness=50",
        );
        assert_eq!(spokes.thickness, 50.0);

        // A thickness that changes over the effect takes the width xLights shows at each moment
        // along it: whole degrees, so it climbs in steps.
        let (start, curve, fidelity) = translated(
            "E_SLIDER_Pinwheel_Arms=4,E_SLIDER_Pinwheel_Thickness=10,\
             E_VALUECURVE_Pinwheel_Thickness=Active=TRUE|Type=Ramp|Min=0.00|Max=100.00|P1=10.00|P2=30.00|RV=TRUE|",
        );
        assert_eq!(fidelity, Fidelity::Exact);
        let curve = curve.expect("a curve on the thickness");
        let width = |thickness: f32| f64::from(thickness) * 0.9;
        assert!(near(width(start.thickness), 10.0), "{}", start.thickness);
        assert!(
            near(width(curve.value_at(0.0)), 10.0) && near(width(curve.value_at(1.0)), 28.0),
            "{curve:?}"
        );
        let mut last = 0.0;
        for step in 0..=100 {
            let here = width(curve.value_at(step as f32 / 100.0));
            assert!(here >= last - 1e-3 && here <= 28.0 + 1e-3, "{curve:?}");
            last = here;
        }
        // And so does a thickness that stays put while the number of arms changes: 20% is 37
        // degrees of two arms' sectors and 13 of six arms'.
        let (start, curve, _) = translated(
            "E_SLIDER_Pinwheel_Arms=2,E_SLIDER_Pinwheel_Thickness=20,\
             E_VALUECURVE_Pinwheel_Arms=Active=TRUE|Type=Ramp|Min=1.00|Max=20.00|P1=2.00|P2=6.00|RV=TRUE|",
        );
        let curve = curve.expect("a curve on the thickness");
        assert!(
            near(f64::from(start.thickness) * 1.8, 37.0),
            "{}",
            start.thickness
        );
        assert!(near(f64::from(curve.value_at(0.0)) * 1.8, 37.0), "{curve:?}");
        assert!(near(f64::from(curve.value_at(1.0)) * 0.6, 13.0), "{curve:?}");

        // A thickness that follows the music is as wide as xLights shows it at both ends, and
        // straight between: exact where the widths in between fall on that line...
        let (_, curve, fidelity) = translated(
            "E_SLIDER_Pinwheel_Arms=4,E_SLIDER_Pinwheel_Thickness=10,\
             E_VALUECURVE_Pinwheel_Thickness=Active=TRUE|Type=Music|Min=0.00|Max=100.00|P1=10.00|P2=50.00|RV=TRUE|",
        );
        let curve = curve.expect("a curve on the thickness");
        assert!(
            near(width(curve.from), 10.0) && near(width(curve.to), 46.0),
            "{curve:?}"
        );
        assert_eq!(fidelity, Fidelity::Exact);
        // ...and approximate where they don't (below 1% xLights' arm stops getting thinner),
        // which the import says.
        let (_, curve, fidelity) = translated(
            "E_SLIDER_Pinwheel_Arms=4,E_SLIDER_Pinwheel_Thickness=10,\
             E_VALUECURVE_Pinwheel_Thickness=Active=TRUE|Type=Music|Min=0.00|Max=100.00|P1=0.00|P2=50.00|RV=TRUE|",
        );
        let curve = curve.expect("a curve on the thickness");
        assert!(
            near(width(curve.from), 1.0) && near(width(curve.to), 46.0),
            "{curve:?}"
        );
        assert_eq!(
            fidelity,
            Fidelity::Approximate(vec![
                "a setting that follows the music or a timing track does so approximately".into()
            ])
        );
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
    fn life_and_tendril_translate_with_their_music_movements() {
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
        for (name, movement) in [
            ("Music Circle", TendrilMovement::MusicCircle),
            ("Music Line", TendrilMovement::MusicLine),
        ] {
            let music = Settings::parse(&format!("E_CHOICE_Tendril_Movement={name}"));
            let t = translate("Tendril", &music, &palette(&[]), 4000, 50).unwrap();
            assert_eq!(t.fidelity, Fidelity::Exact);
            let EffectParams::Tendril(p) = t.params else {
                unreachable!()
            };
            assert_eq!(p.movement, movement);
        }
    }

    #[test]
    fn vu_meters_translate_every_setting() {
        let s = Settings::parse(
            "E_CHOICE_VUMeter_Type=Level Shape,E_CHOICE_VUMeter_Shape=Filled Star,E_SLIDER_VUMeter_Bars=12,\
             E_SLIDER_VUMeter_Sensitivity=40,E_SLIDER_VUMeter_Gain=-25,E_CHECKBOX_VUMeter_SlowDownFalls=0,\
             E_SLIDER_VUMeter_StartNote=40,E_SLIDER_VUMeter_EndNote=127,E_CHECKBOX_VUMeter_LogarithmicX=1,\
             E_SLIDER_VUMeter_XOffset=10,E_SLIDER_VUMeter_YOffset=-20,E_TEXTCTRL_Filter=kick,\
             E_VALUECURVE_VUMeter_Gain=Active=TRUE|Type=Ramp|Min=-100.00|Max=100.00|P1=-50.00|P2=50.00|RV=TRUE|",
        );
        let t = translate("VU Meter", &s, &palette(&[Rgb::RED]), 2000, 25).unwrap();
        assert_eq!(t.fidelity, Fidelity::Exact, "{:?}", t.fidelity);
        assert_eq!(
            t.params,
            EffectParams::VuMeter(VuMeterParams {
                meter: VuMeterType::LevelShape,
                bars: 12,
                sensitivity: 40,
                gain: -50.0,
                timing_track: None,
                shape: VuMeterShape::FilledStar,
                slow_falls: false,
                start_note: 40,
                end_note: 126,
                log_x: true,
                x_offset: 10.0,
                y_offset: -20.0,
                filter: "kick".into(),
            })
        );
        assert_eq!(t.curves["gain"], Curve::ramp(-50.0, 50.0));
        // xLights' defaults: a waveform.
        let plain = translate("VUMeter", &Settings::default(), &palette(&[]), 2000, 25).unwrap();
        let EffectParams::VuMeter(p) = plain.params else {
            unreachable!()
        };
        assert_eq!(
            (p.meter, p.bars, p.sensitivity, p.slow_falls),
            (VuMeterType::Waveform, 6, 70, true)
        );
        // What PixelFlow can't do, it says.
        let s = Settings::parse(
            "E_CHOICE_VUMeter_Type=Frame Waveform,E_CHOICE_VUMeter_AudioTrack=Vocals,\
             E_CHOICE_VUMeter_Shape=SVG",
        );
        let t = translate("VU Meter", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "'Frame Waveform' shown as a waveform across frames".into(),
                "follows the sequence's music, not another audio track".into(),
            ])
        );
        let s = Settings::parse(
            "E_CHOICE_VUMeter_Type=Level Shape,E_CHOICE_VUMeter_Shape=SVG,E_TEXTCTRL_Filter=a.*,E_CHECKBOX_Regex=1",
        );
        let t = translate("VU Meter", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["'SVG' level shape shown as a circle".into()])
        );
        let s = Settings::parse(
            "E_CHOICE_VUMeter_Type=Timing Event Color,E_TEXTCTRL_Filter=a.*,E_CHECKBOX_Regex=1",
        );
        let t = translate("VU Meter", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["label filter read as plain text, not a pattern".into()])
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
        // A system font that doesn't give its size is drawn at the size of the computer's own
        // text in xLights, and characters outside the pixel font are boxes here.
        let s = Settings::parse(
            "E_CHOICE_Text_Font=Use OS Fonts,E_FONTPICKER_Text_Font=bold 'arial narrow' utf-8,\
             E_TEXTCTRL_Text=No\u{eb}l \u{2744}",
        );
        let t = translate("Text", &s, &palette(&[Rgb::RED]), 4000, 50).unwrap();
        let EffectParams::Text(p) = &t.params else {
            unreachable!()
        };
        assert_eq!(p.size, 10);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "'arial narrow' font shown in PixelFlow's pixel font, 10 pixels high where xLights uses the \
                 computer's own text size"
                    .into(),
                "characters PixelFlow's pixel font doesn't have (accents, emoji) shown as boxes".into(),
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
            Fidelity::Approximate(vec!["not turned or flipped the way 'Twist' does".into()])
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
                "drawn across the whole target, not in xLights' 'Vertical Per Strand' arrangement",
                "not rotated or zoomed",
            ]
        );
    }

    #[test]
    fn fire_comes_across_as_xlights_flickering_flames() {
        // xLights' defaults: half height, from the bottom, steady.
        let t = translate("Fire", &Settings::parse(""), &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.params,
            EffectParams::Fire(FireParams {
                height: 0.5,
                style: FireStyle::Flicker,
                ..FireParams::default()
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        let s = Settings::parse(
            "E_SLIDER_Fire_Height=70,E_SLIDER_Fire_HueShift=40,E_CHOICE_Fire_Location=Right,\
             E_TEXTCTRL_Fire_GrowthCycles=3.5,E_CHECKBOX_Fire_GrowWithMusic=1",
        );
        let t = translate("Fire", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.params,
            EffectParams::Fire(FireParams {
                height: 0.7,
                style: FireStyle::Flicker,
                location: FireLocation::Right,
                hue_shift: 0.4,
                growth_cycles: 3.5,
                grow_with_music: true,
                ..FireParams::default()
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        // The height as a curve (slider units), and growth cycles as one (tenths).
        let s = Settings::parse(
            "E_VALUECURVE_Fire_Height=Active=TRUE|Id=ID_VALUECURVE_Fire_Height|Type=Ramp|Min=1.00|Max=100.00|P1=20.00|P2=80.00|RV=TRUE|,\
             E_VALUECURVE_Fire_GrowthCycles=Active=TRUE|Id=ID_VALUECURVE_Fire_GrowthCycles|Type=Ramp|Min=0.00|Max=200.00|P1=0.00|P2=100.00|RV=TRUE|",
        );
        let t = translate("Fire", &s, &palette(&[]), 2000, 25).unwrap();
        assert_eq!(t.curves["height"], Curve::ramp(0.2, 0.8));
        assert_eq!(t.curves["growthCycles"], Curve::ramp(0.0, 10.0));
        assert_eq!(t.fidelity, Fidelity::Exact);
    }

    #[test]
    fn a_hue_shifted_fire_under_another_layer_blend_says_its_dim_flames_differ() {
        let s = Settings::parse("E_SLIDER_Fire_HueShift=40,T_CHOICE_LayerMethod=Additive");
        assert_eq!(
            translate("Fire", &s, &palette(&[]), 2000, 25).unwrap().fidelity,
            Fidelity::Approximate(vec![
                "a hue-shifted fire's dim flames are dimmer than xLights' under this layer blending".into()
            ])
        );
        // Without a hue shift the two draw the same under any blend.
        let s = Settings::parse("T_CHOICE_LayerMethod=Additive");
        assert_eq!(
            translate("Fire", &s, &palette(&[]), 2000, 25).unwrap().fidelity,
            Fidelity::Exact
        );
    }

    #[test]
    fn shockwaves_translate_exactly_and_old_ones_keep_their_sizes_in_cells() {
        // What xLights' panel starts with, for settings the file leaves out.
        let t = translate("Shockwave", &Settings::parse(""), &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.params,
            EffectParams::Shockwave(ShockwaveParams {
                center_x: 50.0,
                center_y: 50.0,
                start_radius: 1.0,
                end_radius: 10.0,
                start_width: 5.0,
                end_width: 10.0,
                cycles: 1,
                acceleration: 0.0,
                blend_edges: true,
                scale: true,
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        // A file from before 2025.04 measures in cells, whatever it says.
        let text = "E_CHECKBOX_Shockwave_Blend_Edges=0,E_SLIDER_Shockwave_Accel=-7,\
                    E_SLIDER_Shockwave_CenterX=25,E_SLIDER_Shockwave_CenterY=75,\
                    E_SLIDER_Shockwave_End_Radius=170,E_SLIDER_Shockwave_End_Width=118,\
                    E_SLIDER_Shockwave_Start_Radius=3,E_SLIDER_Shockwave_Start_Width=7,\
                    E_SLIDER_Shockwave_Cycles=4,E_CHECKBOX_Shockwave_Scale=1";
        let expected = ShockwaveParams {
            center_x: 25.0,
            center_y: 75.0,
            start_radius: 3.0,
            end_radius: 170.0,
            start_width: 7.0,
            end_width: 118.0,
            cycles: 4,
            acceleration: -7.0,
            blend_edges: false,
            scale: false,
        };
        for (version, scale) in [("2024.19", false), ("2025.04", true)] {
            let mut s = Settings::parse(text);
            adjust_for_version("Shockwave", &mut s, version);
            let t = translate("Shockwave", &s, &palette(&[]), 2000, 25).unwrap();
            assert_eq!(
                t.params,
                EffectParams::Shockwave(ShockwaveParams { scale, ..expected }),
                "{version}"
            );
            assert_eq!(t.fidelity, Fidelity::Exact);
        }
        // A radius that changes gets a curve; with several rings xLights restarts it each ring.
        let curve = "E_VALUECURVE_Shockwave_End_Radius=Active=TRUE|Id=ID_VALUECURVE_Shockwave_End_Radius|Type=Ramp|Min=0.00|Max=750.00|P1=10.00|P2=20.00|RV=TRUE|";
        let t = translate("Shockwave", &Settings::parse(curve), &palette(&[]), 2000, 25).unwrap();
        assert_eq!(t.curves["endRadius"], Curve::ramp(10.0, 20.0));
        assert_eq!(t.fidelity, Fidelity::Exact);
        let s = Settings::parse(&format!("{curve},E_SLIDER_Shockwave_Cycles=3"));
        assert_eq!(
            translate("Shockwave", &s, &palette(&[]), 2000, 25)
                .unwrap()
                .fidelity,
            Fidelity::Approximate(vec![
                "settings that change over each ring change once over the effect".into()
            ])
        );
    }

    #[test]
    fn fills_translate_exactly_with_their_first_versions_rules() {
        let t = translate("Fill", &Settings::parse(""), &palette(&[]), 2000, 25).unwrap();
        assert_eq!(
            t.params,
            EffectParams::Fill(FillParams {
                position: 100.0,
                direction: FillDirection::Up,
                band_size: 0.0,
                skip_size: 0.0,
                offset: 0.0,
                offset_in_cells: true,
                wrap: true,
                color_by_time: false,
            })
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        let text = "E_CHECKBOX_Fill_Color_Time=0,E_CHECKBOX_Fill_Offset_In_Pixels=0,E_CHECKBOX_Fill_Wrap=0,\
                    E_CHOICE_Fill_Direction=Left,E_SLIDER_Fill_Band_Size=216,E_SLIDER_Fill_Offset=79,\
                    E_SLIDER_Fill_Position=40,E_SLIDER_Fill_Skip_Size=218";
        let t = translate("Fill", &Settings::parse(text), &palette(&[]), 2000, 25).unwrap();
        let expected = FillParams {
            position: 40.0,
            direction: FillDirection::Left,
            band_size: 216.0,
            skip_size: 218.0,
            offset: 79.0,
            offset_in_cells: false,
            wrap: false,
            color_by_time: false,
        };
        assert_eq!(t.params, EffectParams::Fill(expected));
        assert_eq!(t.fidelity, Fidelity::Exact);
        // Fills from before 2016.41 were colored by time, and before 2016.53 always wrapped.
        for (version, color_by_time, wrap) in [("2016.40", true, true), ("2016.52", false, true)] {
            let mut s = Settings::parse(text);
            adjust_for_version("Fill", &mut s, version);
            assert_eq!(
                translate("Fill", &s, &palette(&[]), 2000, 25).unwrap().params,
                EffectParams::Fill(FillParams {
                    color_by_time,
                    wrap,
                    ..expected
                }),
                "{version}"
            );
        }
        // The position usually has a curve: that's what makes the fill move.
        let curve = "E_VALUECURVE_Fill_Position=Active=TRUE|Id=ID_VALUECURVE_Fill_Position|Type=Ramp|Min=0.00|Max=100.00|P1=0.00|P2=100.00|RV=TRUE|";
        let t = translate("Fill", &Settings::parse(curve), &palette(&[]), 2000, 25).unwrap();
        assert_eq!(t.curves["position"], Curve::ramp(0.0, 100.0));
    }

    /// Draws `params` on a grid of 2,000 pixels for 400 frames `frame_ms` apart, and says what
    /// share of the pixels was lit in a frame and what share of those was still lit in the
    /// next. Every lit pixel has to be at full brightness.
    fn flashes(params: &EffectParams, frame_ms: u32) -> (f64, f64) {
        use pf_render::{Canvas, Colors, EffectTime, Pixel, Shade, Shader};
        let canvas = Canvas {
            columns: 50,
            rows: 40,
        };
        let (pixels, frames) = (canvas.columns * canvas.rows, 400u64);
        let frame_of = |frame: u64| -> Vec<bool> {
            let at = 1000 + frame * u64::from(frame_ms);
            let time =
                EffectTime::within(1000, 1000 + frames * u64::from(frame_ms), at).with_frame_ms(frame_ms);
            let shader = Shader::new(params, &time, Colors::new(&[Rgb::WHITE]), 7, canvas);
            (0..pixels)
                .map(|index| {
                    let color = shader.shade(&Pixel {
                        u: (index % canvas.columns) as f32 / (canvas.columns - 1) as f32,
                        v: (index / canvas.columns) as f32 / (canvas.rows - 1) as f32,
                        index,
                        count: pixels,
                    });
                    assert!(color.a == 0.0 || color.to_rgb8() == [255; 3], "{color:?}");
                    color.a > 0.0
                })
                .collect()
        };
        let (mut lit, mut stayed) = (0u64, 0u64);
        let mut before = frame_of(0);
        for frame in 1..=frames {
            let now = frame_of(frame);
            lit += before.iter().filter(|&&on| on).count() as u64;
            stayed += before.iter().zip(&now).filter(|&(&a, &b)| a && b).count() as u64;
            before = now;
        }
        (
            lit as f64 / (frames * u64::from(pixels)) as f64,
            stayed as f64 / lit.max(1) as f64,
        )
    }

    #[test]
    fn a_strobing_twinkle_is_a_few_pixels_flashing_for_a_frame_each() {
        // Measured on frames xLights rendered (count 20, steps 98, 50 ms frames, a group of
        // 4,910 pixels): of the pixels nothing else lit, 0.35% were lit in any one frame, each
        // for that frame alone and at full brightness, and when a pixel flashed was a matter
        // of chance. A soft twinkle of the same count lights some forty times as many.
        let strobing = |count: u32, steps: u32| {
            Settings::parse(&format!(
                "E_CHECKBOX_Twinkle_Strobe=1,E_CHOICE_Twinkle_Style=New Render Method,\
                 E_SLIDER_Twinkle_Count={count},E_SLIDER_Twinkle_Steps={steps}"
            ))
        };
        for frame_ms in [50, 25, 20, 33] {
            let t = translate(
                "Twinkle",
                &strobing(20, 98),
                &palette(&[Rgb::WHITE]),
                30_000,
                frame_ms,
            )
            .unwrap();
            assert_eq!(t.fidelity, Fidelity::Exact, "{frame_ms} ms");
            let (lit, stayed) = flashes(&t.params, frame_ms);
            assert!(
                (0.0031..0.0039).contains(&lit),
                "{frame_ms} ms: {lit} of the pixels lit in a frame"
            );
            assert!(
                stayed < 0.02,
                "{frame_ms} ms: {stayed} of them still lit a frame later"
            );
        }
        // More of them with a higher count, fewer when a twinkle takes more steps.
        let share = |count, steps| {
            let t = translate(
                "Twinkle",
                &strobing(count, steps),
                &palette(&[Rgb::WHITE]),
                30_000,
                50,
            )
            .unwrap();
            flashes(&t.params, 50).0
        };
        let (twice, half) = (share(40, 98), share(20, 196));
        assert!((0.0062..0.0078).contains(&twice), "{twice}");
        assert!((0.0014..0.0021).contains(&half), "{half}");
        // The older way of drawing twinkles hasn't been measured strobing: it keeps its note.
        let old = Settings::parse(
            "E_CHECKBOX_Twinkle_Strobe=1,E_CHOICE_Twinkle_Style=Old Render Method,E_SLIDER_Twinkle_Count=20",
        );
        let t = translate("Twinkle", &old, &palette(&[Rgb::WHITE]), 30_000, 50).unwrap();
        assert!(matches!(t.params, EffectParams::Twinkle(_)), "{:?}", t.params);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec!["strobing twinkles shown as soft twinkles".into()])
        );
        // And a twinkle that doesn't strobe is the soft twinkle it was.
        let soft = Settings::parse("E_CHOICE_Twinkle_Style=New Render Method,E_SLIDER_Twinkle_Count=20");
        let t = translate("Twinkle", &soft, &palette(&[Rgb::WHITE]), 30_000, 50).unwrap();
        assert!(matches!(t.params, EffectParams::Twinkle(p) if (p.density - 0.2).abs() < 1e-6));
    }

    #[test]
    fn settings_beyond_pixelflows_ranges_are_clamped_and_reported() {
        // 20 turns in half a second is faster than PixelFlow's spiral turns.
        let s = Settings::parse("E_SLIDER_Spirals_Rotation=250,E_TEXTCTRL_Spirals_Movement=20.0");
        let t = translate("Spirals", &s, &palette(&[Rgb::RED]), 500, 25).unwrap();
        let EffectParams::Spiral(p) = t.params else {
            panic!("{:?}", t.params)
        };
        assert_eq!((p.speed, p.lean), (20.0, -250.0));
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
            "Lightning",
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

    fn picture(settings: &str, duration_ms: u64) -> (PictureParams, Translated) {
        let t = translate(
            "Pictures",
            &Settings::parse(settings),
            &palette(&[]),
            duration_ms,
            25,
        )
        .unwrap();
        let EffectParams::Picture(p) = t.params.clone() else {
            panic!("{:?}", t.params)
        };
        (p, t)
    }

    fn notes(t: &Translated) -> Vec<String> {
        match &t.fidelity {
            Fidelity::Approximate(notes) => notes.clone(),
            Fidelity::Exact => Vec::new(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pictures_keep_their_file_size_place_and_scrolling() {
        // A looping GIF scrolling left twice over 10 s, wrapping, black made clear.
        let (p, t) = picture(
            r"E_TEXTCTRL_Pictures_Filename=C:\Users\me\xlights\Images\santa dancing.gif,E_CHOICE_Scaling=Scale Keep Aspect Ratio,E_CHOICE_Pictures_Direction=left,E_TEXTCTRL_Pictures_Speed=2,E_SLIDER_PicturesXC=10,E_SLIDER_PicturesYC=-20,E_CHECKBOX_Pictures_WrapX=1,E_CHECKBOX_LoopGIF=1,E_TEXTCTRL_Pictures_FrameRateAdj=1.5,E_CHECKBOX_Pictures_TransparentBlack=1,E_TEXTCTRL_Pictures_TransparentBlack=153",
            10_000,
        );
        // Everything comes across; only the resizing isn't today's xLights'.
        assert_eq!(
            notes(&t),
            ["resized by averaging its pixels, where xLights uses another filter"]
        );
        assert!(
            (p.black_level - 20.0).abs() < 1e-4,
            "153 of 765 is a fifth: {}",
            p.black_level
        );
        assert_eq!(
            PictureParams {
                black_level: 20.0,
                ..p
            },
            PictureParams {
                file: r"C:\Users\me\xlights\Images\santa dancing.gif".into(),
                fit: PictureFit::Fit,
                timing: PictureTiming::Loop,
                play_speed: 1.5,
                movement: PictureMovement::Left,
                // Two trips in ten seconds.
                move_speed: 0.2,
                wrap: true,
                x_offset: 10.0,
                // xLights takes a scrolling picture's offset the other way up.
                y_offset: 20.0,
                black_transparent: true,
                black_level: 20.0,
                ..PictureParams::default()
            }
        );
        assert!(t.curves.is_empty());
        // The palette's brightness dims a picture as the effect's own; its colors don't color it.
        let mut dim = palette(&[Rgb::RED, Rgb::BLUE]);
        dim.brightness = 40.0;
        let t = translate("Pictures", &Settings::default(), &dim, 1000, 25).unwrap();
        let EffectParams::Picture(p) = &t.params else {
            panic!("{:?}", t.params)
        };
        assert!(!p.tint);
        assert_eq!(t.brightness, 0.4);
        // Nothing set: no file yet, pixel for pixel, still, its animation spread over the effect.
        let (p, t) = picture("", 4_000);
        assert_eq!(
            p,
            PictureParams {
                fit: PictureFit::Actual,
                timing: PictureTiming::Stretch,
                ..PictureParams::default()
            }
        );
        assert_eq!(t.fidelity, Fidelity::Exact);
        assert_eq!(t.palette.colors, vec![Rgb::WHITE]);
        // Each way of scaling, and the checkbox old sequences have instead.
        for (scaling, fit) in [
            ("No Scaling", PictureFit::Actual),
            ("Scale To Fit", PictureFit::Stretch),
            ("Scale Keep Aspect Ratio", PictureFit::Fit),
            ("Scale Keep Aspect Ratio Crop", PictureFit::Fill),
        ] {
            let (p, t) = picture(
                &format!("E_CHOICE_Scaling={scaling},E_SLIDER_PicturesYC=30"),
                1000,
            );
            assert_eq!((p.fit, p.y_offset), (fit, 30.0), "{scaling}");
            // Only a picture at its own size is the same pixels as xLights'.
            let same = notes(&t).is_empty();
            assert_eq!(same, fit == PictureFit::Actual, "{scaling}: {:?}", t.fidelity);
        }
        let (old, t) = picture(
            "E_FILEPICKER_Pictures_Filename=tree.png,E_CHECKBOX_Pictures_ScaleToFit=1,E_CHECKBOX_Pictures_PixelOffsets=1",
            1000,
        );
        assert_eq!(
            (old.file.as_str(), old.fit, old.pixel_offsets),
            ("tree.png", PictureFit::Stretch, true)
        );
        assert_eq!(
            notes(&t),
            ["resized by averaging its pixels, where xLights uses another filter"]
        );
        let (older, _) = picture("E_FILEPICKERCTRL_Pictures_Filename= star.bmp ", 1000);
        assert_eq!(older.file, "star.bmp");
        // Up and down scroll too (xLights never wraps them); a frame rate out of range is noted.
        let (up, t) = picture(
            "E_CHOICE_Pictures_Direction=up,E_CHECKBOX_Pictures_WrapX=1,E_TEXTCTRL_Pictures_Speed=1,E_TEXTCTRL_Pictures_FrameRateAdj=40",
            2_000,
        );
        assert_eq!(
            (up.movement, up.move_speed, up.wrap, up.play_speed),
            (PictureMovement::Up, 0.5, false, 10.0)
        );
        assert_eq!(
            notes(&t),
            [
                "wrapping across not shown on a picture that isn't moving across",
                BEYOND_RANGE
            ]
        );
    }

    #[test]
    fn a_picture_is_resized_the_way_the_xlights_that_saved_it_resized() {
        let text = "E_TEXTCTRL_Pictures_Filename=star.png,E_CHOICE_Scaling=Scale To Fit";
        let from = |version: &str| {
            let mut s = Settings::parse(text);
            adjust_for_version("Pictures", &mut s, version);
            let t = translate("Pictures", &s, &palette(&[]), 1000, 25).unwrap();
            let EffectParams::Picture(p) = &t.params else {
                panic!("{:?}", t.params)
            };
            (p.resize, notes(&t))
        };
        let none: Vec<String> = Vec::new();
        // The nearest pixel until 2024.16; then four mixed when shrinking.
        assert_eq!(from("2023.20"), (PictureResize::XlightsNearest, none.clone()));
        assert_eq!(from("2024.15"), (PictureResize::XlightsNearest, none.clone()));
        assert_eq!(from("2024.16"), (PictureResize::XlightsMixed, none.clone()));
        assert_eq!(from("2025.03"), (PictureResize::XlightsMixed, none.clone()));
        // Two releases enlarged from each cell's middle instead.
        assert_eq!(
            from("2025.05"),
            (
                PictureResize::XlightsMixed,
                vec![
                    "enlarged from each cell's corner pixel, where xLights 2025.04 and 2025.05 took its middle one"
                        .to_string()
                ]
            )
        );
        // Since then xLights averages or filters, which PixelFlow's own averaging only comes near.
        let averaged = vec!["resized by averaging its pixels, where xLights uses another filter".to_string()];
        assert_eq!(from("2025.06"), (PictureResize::Own, averaged.clone()));
        assert_eq!(from("2026.12"), (PictureResize::Own, averaged.clone()));
        assert_eq!(from("not a version"), (PictureResize::Own, averaged));
        // A picture shown at its own size isn't resized, whatever the version.
        let mut s = Settings::parse("E_TEXTCTRL_Pictures_Filename=star.png,E_CHOICE_Scaling=No Scaling");
        adjust_for_version("Pictures", &mut s, "2026.12");
        assert_eq!(
            translate("Pictures", &s, &palette(&[]), 1000, 25)
                .unwrap()
                .fidelity,
            Fidelity::Exact
        );
    }

    #[test]
    fn picture_movements_without_a_match_are_noted() {
        let moved = |direction: &str, more: &str| {
            let (p, t) = picture(&format!("E_CHOICE_Pictures_Direction={direction},{more}"), 5_000);
            (p.movement, p.wrap, notes(&t))
        };
        use PictureMovement as M;
        assert_eq!(moved("down", ""), (M::Down, false, vec![]));
        assert_eq!(moved("right", ""), (M::Right, false, vec![]));
        assert_eq!(
            moved("up-left", ""),
            (
                M::Left,
                false,
                vec!["'up-left' movement shown moving left".to_string()]
            )
        );
        assert_eq!(
            moved("down-right", "").2,
            ["'down-right' movement shown moving right"]
        );
        for still in [
            "peekaboo",
            "peekaboo 90",
            "peekaboo 180",
            "peekaboo 270",
            "wiggle",
            "flag wave",
        ] {
            assert_eq!(
                moved(still, ""),
                (
                    M::None,
                    false,
                    vec![format!("'{still}' movement not shown: the picture stays still")]
                )
            );
        }
        assert_eq!(
            moved("zoom in", "E_TEXTCTRL_Pictures_Speed=3"),
            (
                M::ZoomIn,
                false,
                vec![
                    "'zoom in' grows from the middle to the picture's size, not from the corner to the whole prop"
                        .to_string(),
                    "repeated zooming shown as one zoom over the effect".to_string()
                ]
            )
        );
        assert_eq!(
            moved("tile-up", ""),
            (
                M::Up,
                true,
                vec![
                    "'tile-up' shown as one picture wrapping round rather than tiles, at a speed of its own"
                        .to_string()
                ]
            )
        );
        assert_eq!(moved("tile-left", "").0, M::Left);
        // Crossing once is exact until it's asked to cross faster than the effect lasts.
        assert_eq!(moved("up once", ""), (M::Up, false, vec![]));
        assert_eq!(
            moved("down once", "E_TEXTCTRL_Pictures_Speed=4"),
            (
                M::Down,
                false,
                vec!["'down once' crosses again and again instead of once".to_string()]
            )
        );
        // What else isn't drawn says so.
        let (_, t) = picture(
            "E_TEXTCTRL_Pictures_Filename=frames/clip-1.jpg,E_CHECKBOX_Pictures_Shimmer=1,E_CHECKBOX_SuppressGIFBackground=0",
            1000,
        );
        assert_eq!(
            notes(&t),
            [
                "numbered picture files (name-1, name-2, and so on) shown as the first picture alone",
                "shimmer not shown",
                "a GIF's background color not drawn"
            ]
        );
    }

    #[test]
    fn a_pictures_size_and_position_that_change_get_curves() {
        // Pixel for pixel, growing from half size to double over the effect.
        let (p, t) = picture(
            "E_CHOICE_Scaling=No Scaling,E_SLIDER_Pictures_StartScale=50,E_SLIDER_Pictures_EndScale=200",
            4_000,
        );
        assert_eq!(p.scale, 50.0);
        assert_eq!(t.curves["scale"], Curve::ramp(50.0, 200.0));
        assert_eq!(
            notes(&t),
            ["resized by averaging its pixels, where xLights uses another filter"]
        );
        // At its own size the whole time, nothing is resized.
        let (_, t) = picture("E_CHOICE_Scaling=No Scaling", 4_000);
        assert_eq!(t.fidelity, Fidelity::Exact);
        // Three times over, with the movement's speed.
        let (_, t) = picture(
            "E_SLIDER_Pictures_StartScale=50,E_SLIDER_Pictures_EndScale=200,E_TEXTCTRL_Pictures_Speed=3,E_CHOICE_Pictures_Direction=left",
            4_000,
        );
        assert_eq!(
            t.curves["scale"],
            Curve::shaped(CurveShape::Saw, 50.0, 200.0, 3.0)
        );
        // xLights doesn't scale a picture it fits, whatever the sliders say.
        let (p, t) = picture(
            "E_CHOICE_Scaling=Scale To Fit,E_SLIDER_Pictures_StartScale=50,E_SLIDER_Pictures_EndScale=200",
            4_000,
        );
        assert_eq!(p.scale, 100.0);
        assert!(t.curves.is_empty());
        // A "vector" move: from its offsets to its end offsets, standing still otherwise.
        let (p, t) = picture(
            "E_CHOICE_Pictures_Direction=vector,E_SLIDER_PicturesXC=-50,E_SLIDER_PicturesYC=10,E_SLIDER_PicturesEndXC=50,E_SLIDER_PicturesEndYC=10",
            4_000,
        );
        assert_eq!(
            (p.movement, p.x_offset, p.y_offset),
            (PictureMovement::None, -50.0, 10.0)
        );
        assert_eq!(t.curves["xOffset"], Curve::ramp(-50.0, 50.0));
        assert!(!t.curves.contains_key("yOffset"), "it doesn't move up or down");
        assert_eq!(t.fidelity, Fidelity::Exact);
        // An offset on a value curve follows it.
        let (p, t) = picture(
            "E_CHOICE_Scaling=Scale To Fit,E_VALUECURVE_PicturesXC=Active=TRUE|Id=ID_VALUECURVE_PicturesXC|Type=Ramp|Min=-100.00|Max=100.00|P1=-50.00|P2=50.00|RV=TRUE|",
            4_000,
        );
        assert_eq!(p.x_offset, -50.0, "the setting holds the curve's start");
        assert_eq!(t.curves["xOffset"], Curve::ramp(-50.0, 50.0));
        assert_eq!(t.curves.len(), 1, "{:?}", t.curves.keys());
    }

    #[test]
    fn the_tally_groups_effects_by_how_they_look_and_says_what_differs() {
        let mut tally = Tally::default();
        for _ in 0..12 {
            tally.record("Butterfly", &Fidelity::Placeholder, 1000);
        }
        for _ in 0..5 {
            tally.record("Plasma", &Fidelity::Placeholder, 1000);
        }
        let close = Fidelity::Approximate(vec!["a".into(), "b".into()]);
        assert_eq!(tally.record("Bars", &close, 2000), Some(Verdict::Close));
        tally.record("Bars", &Fidelity::Approximate(vec!["a".into()]), 500);
        assert_eq!(tally.record("On", &Fidelity::Exact, 4000), Some(Verdict::Matches));
        assert_eq!(tally.record("DMX", &Fidelity::Skipped, 4000), None);

        let (effects, ms) = tally.counts();
        assert_eq!(
            (
                effects.matches,
                effects.close,
                effects.different,
                effects.no_equivalent
            ),
            (1, 2, 0, 17)
        );
        assert_eq!((ms.matches, ms.close, ms.no_equivalent), (4000, 2500, 17_000));
        // Worst first, and nothing about what matches.
        let looks = tally.looks();
        let listed: Vec<(Verdict, &str, usize)> = looks
            .iter()
            .map(|g| (g.verdict, g.effect.as_str(), g.effects))
            .collect();
        assert_eq!(
            listed,
            [
                (Verdict::NoEquivalent, "Butterfly", 12),
                (Verdict::NoEquivalent, "Plasma", 5),
                (Verdict::Close, "Bars", 2),
            ]
        );
        assert_eq!(looks[0].reasons, [PLACEHOLDER_LOOK]);
        assert_eq!(
            look_notes(&looks),
            vec![
                "No PixelFlow effect yet for these xLights effects, so they are shown as a dim fill in each one's first color: Butterfly (12), Plasma (5).",
                "Bars (2 effects) look close: a, b (1).",
            ]
        );
        assert!(tally.skipped_note().is_some_and(|n| n.ends_with("DMX (1).")));
    }
}
