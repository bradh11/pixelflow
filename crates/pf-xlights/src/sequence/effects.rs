//! Translating one xLights effect (its name, settings, and palette) into a PixelFlow effect.
//!
//! Settings keys and defaults follow xLights' `src-core/effects/*Effect.cpp`; speeds that xLights
//! counts per effect ("cycles") become per-second rates using the effect's length.

use super::settings::{ParsedPalette, Settings};
use super::{list, plural};
use pf_sequence::{
    Axis, BarsParams, Blend, ChaseParams, ColorWashParams, Direction, EffectParams, FaceColorSource,
    FaceEyes, FacesParams, FireParams, Gradient, MeteorDirection, MeteorsParams, OffParams, OnParams,
    Palette, Rgb, RippleParams, ShimmerParams, SpiralParams, StrobeParams, TwinkleParams, WaveParams,
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

/// Notes the settings that change over the effect in xLights (value curves), which PixelFlow
/// imports at their fixed value.
fn curves(s: &Settings, keys: &[&str], diff: &mut Diff) {
    if keys.iter().any(|k| s.curve_active(&format!("E_VALUECURVE_{k}"))) {
        diff.add("settings that change over the effect kept at one value");
    }
}

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
    curves(r.s, &["Eff_On_Start", "Eff_On_End", "On_Transparency"], diff);
    if r.check("On_Shimmer") {
        diff.add("shimmer not shown");
    }
    if (r.get("On_Cycles", 1.0, 0.0, 100.0) - 1.0).abs() > 1e-6 && (start - end).abs() > 1e-6 {
        diff.add("repeating brightness ramp shown once");
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
    curves(r.s, &["ColorWash_Cycles"], diff);
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
    curves(
        r.s,
        &["Bars_BarCount", "Bars_Cycles", "Bars_Center", "Bars_Angle"],
        diff,
    );
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
            curves(
                r.s,
                &["Number_Chases", "Color_Mix1", "Chase_Rotations", "Chase_Offset"],
                diff,
            );
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
    curves(
        r.s,
        &[
            "Number_Waves",
            "Wave_Speed",
            "Thickness_Percentage",
            "Wave_Height",
            "Wave_YOffset",
        ],
        diff,
    );
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
    curves(r.s, &["Twinkle_Count", "Twinkle_Steps"], diff);
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
    curves(r.s, &["Shimmer_Duty_Factor", "Shimmer_Cycles"], diff);
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
    curves(
        r.s,
        &[
            "Spirals_Count",
            "Spirals_Rotation",
            "Spirals_Thickness",
            "Spirals_Movement",
        ],
        diff,
    );
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
    curves(r.s, &["Fire_Height", "Fire_HueShift", "Fire_GrowthCycles"], diff);
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
    curves(
        r.s,
        &[
            "Meteors_Count",
            "Meteors_Length",
            "Meteors_Speed",
            "Meteors_Swirl_Intensity",
        ],
        diff,
    );
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
    curves(
        r.s,
        &[
            "Ripple_Cycles",
            "Ripple_Thickness",
            "Ripple_Spacing",
            "Ripple_Outline",
        ],
        diff,
    );
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
        // No direct equivalent: the closest PixelFlow effect, with its default settings.
        "plasma" | "butterfly" => closest(
            "a color wash",
            EffectParams::ColorWash(ColorWashParams::default()),
        ),
        "fireworks" | "snowflakes" => closest("twinkles", EffectParams::Twinkle(TwinkleParams::default())),
        "snowstorm" => closest("falling meteors", EffectParams::Meteors(MeteorsParams::default())),
        "shockwave" => closest("a ripple", EffectParams::Ripple(RippleParams::default())),
        "pinwheel" | "galaxy" => closest("a spiral", EffectParams::Spiral(SpiralParams::default())),
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

/// Render-buffer settings (`B_*`) that change how an effect is laid over the prop. Answers the
/// blur (`B_SLIDER_Blur`, 1 = none) in PixelFlow's terms (one less).
fn buffer(s: &Settings, diff: &mut Diff) -> u32 {
    let style = s.text("B_CHOICE_BufferStyle", "Default");
    if !matches!(style, "Default" | "Per Preview" | "Single Line" | "") {
        diff.add(format!("'{style}' render style not applied"));
    }
    if !matches!(s.text("B_CHOICE_BufferTransform", "None"), "None" | "") {
        diff.add("buffer rotation or flip not applied");
    }
    let changed = |key: &str, neutral: f64| s.num(key).is_some_and(|v| (v - neutral).abs() > 1e-6);
    if changed("B_SLIDER_Rotation", 0.0) || changed("B_SLIDER_Zoom", 1.0) {
        diff.add("rotation or zoom not applied");
    }
    if !s.text("B_CUSTOM_SubBuffer", "").is_empty() {
        diff.add("sub-buffer (part of the prop) not applied");
    }
    if s.curve_active("B_VALUECURVE_Blur") {
        diff.add("blur that changes over the effect kept at one value");
    }
    // xLights reads the slider as a whole number (`GetInt`); past 15 it blurs as 15.
    let blur = s.num_or("B_SLIDER_Blur", 1.0).trunc();
    if blur > f64::from(pf_sequence::MAX_BLUR + 1) {
        diff.add("blur above 15 shown at 15");
    }
    (blur.clamp(1.0, f64::from(pf_sequence::MAX_BLUR + 1)) as u32) - 1
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
    let (sparkles, sparkle_color) = sparkles(palette, &mut diff);
    let blend = blend(s, &mut diff);
    let (fade_in_ms, fade_out_ms) = fades(s, duration_ms, &mut diff);
    let blur = buffer(s, &mut diff);
    Some(
        match effect_params(name, s, &colors, duration_ms, frame_ms, &mut diff) {
            Kind::Skip => return None,
            Kind::Params(params) => Translated {
                // PixelFlow's settings table has the final say on ranges (evaluated before
                // `fidelity`, so a clamp counts as an approximation).
                params: {
                    let clamped = params.sanitized();
                    if clamped != params {
                        diff.add("settings beyond PixelFlow's range set to the nearest it allows");
                    }
                    clamped
                },
                palette: Palette::new(colors),
                blend,
                fade_in_ms,
                fade_out_ms,
                sparkles,
                sparkle_color,
                blur,
                fidelity: if diff.0.is_empty() {
                    Fidelity::Exact
                } else {
                    Fidelity::Approximate(diff.0)
                },
            },
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
            "Text",
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
        let shape = translate("Shape", &s, &p, 1000, 25).unwrap();
        assert_eq!((shape.sparkles, shape.blur), (54, 7));
    }

    #[test]
    fn sparkles_and_blur_xlights_varies_are_noted() {
        let s = Settings::parse("B_SLIDER_Blur=40,B_VALUECURVE_Blur=Active=TRUE|Type=Ramp|");
        let mut p = palette(&[Rgb::RED]);
        p.sparkles = 10;
        p.music_sparkles = true;
        let t = translate("Color Wash", &s, &p, 1000, 25).unwrap();
        assert_eq!(t.blur, 14);
        assert_eq!(
            t.fidelity,
            Fidelity::Approximate(vec![
                "music-driven sparkles shown at a steady rate".into(),
                "blur that changes over the effect kept at one value".into(),
                "blur above 15 shown at 15".into(),
            ])
        );
    }

    #[test]
    fn unsupported_blends_transitions_and_buffers_are_approximations() {
        let s = Settings::parse(
            "T_CHOICE_LayerMethod=Effect 2,T_TEXTCTRL_Fadein=1,T_CHOICE_In_Transition_Type=Wipe,B_CHOICE_BufferStyle=Per Model Default,B_SLIDER_Rotation=45",
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
                "'Per Model Default' render style not applied",
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
             E_SLIDER_Meteors_Speed=1e300,E_TEXTCTRL_Ripple_Cycles=1e300,E_SLIDER_Ripple_Thickness=0",
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
