//! Effects: what lights up, when, and how it mixes with what's underneath.

use crate::settings::{SettingSpec, choices, effect_params};
use crate::{EffectId, Rgb, TimingTrackId};
use serde::{Deserialize, Serialize};

/// The colors an effect draws with. An empty palette draws white.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    #[serde(default)]
    pub colors: Vec<Rgb>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            colors: vec![Rgb::WHITE],
        }
    }
}

impl Palette {
    pub fn new(colors: impl Into<Vec<Rgb>>) -> Self {
        Self {
            colors: colors.into(),
        }
    }
}

/// How an effect's colors combine with the layers below it **on the same row**. Rows don't
/// blend with each other: a later row covers an earlier one by coverage (where it's lit).
/// Layer 0 is the bottom layer (the opposite of xLights, where layer 1 is drawn on top).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Blend {
    /// Covers what's below (where the effect is lit).
    #[default]
    Normal,
    /// Adds its light to what's below.
    Add,
    /// Keeps the brighter of the two, channel by channel.
    Max,
    /// Tints what's below by its colors (white leaves it unchanged, black hides it).
    Multiply,
}

/// One timed effect on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub id: EffectId,
    /// When the effect starts, in milliseconds from the start of the sequence.
    pub start_ms: u64,
    /// When it ends (exclusive).
    pub end_ms: u64,
    /// The effect's kind and settings (`params.kind` names the kind).
    pub params: EffectParams,
    #[serde(default)]
    pub palette: Palette,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default)]
    pub fade_in_ms: u32,
    #[serde(default)]
    pub fade_out_ms: u32,
}

impl Effect {
    /// A new effect of `kind` with default settings and a white palette.
    pub fn new(kind: EffectKind, start_ms: u64, end_ms: u64) -> Self {
        Self {
            id: EffectId::new(),
            start_ms,
            end_ms,
            params: EffectParams::default_for(kind),
            palette: Palette::default(),
            blend: Blend::default(),
            fade_in_ms: 0,
            fade_out_ms: 0,
        }
    }

    pub fn kind(&self) -> EffectKind {
        self.params.kind()
    }

    pub fn with_palette(mut self, colors: impl Into<Vec<Rgb>>) -> Self {
        self.palette = Palette::new(colors);
        self
    }

    pub fn with_params(mut self, params: EffectParams) -> Self {
        self.params = params;
        self
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    /// True when the effect is lit at `t_ms` (`start <= t < end`).
    pub fn is_active_at(&self, t_ms: u64) -> bool {
        self.start_ms <= t_ms && t_ms < self.end_ms
    }

    /// True when the two effects share any time.
    pub fn overlaps(&self, other: &Effect) -> bool {
        self.start_ms < other.end_ms && other.start_ms < self.end_ms
    }
}

/// The kinds of effect PixelFlow can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    On,
    Off,
    ColorWash,
    Fade,
    Chase,
    Bars,
    Wave,
    Twinkle,
    Shimmer,
    Strobe,
    Spiral,
    Fire,
    Meteors,
    Ripple,
    Faces,
}

impl EffectKind {
    pub const ALL: [EffectKind; 15] = [
        EffectKind::On,
        EffectKind::Off,
        EffectKind::ColorWash,
        EffectKind::Fade,
        EffectKind::Chase,
        EffectKind::Bars,
        EffectKind::Wave,
        EffectKind::Twinkle,
        EffectKind::Shimmer,
        EffectKind::Strobe,
        EffectKind::Spiral,
        EffectKind::Fire,
        EffectKind::Meteors,
        EffectKind::Ripple,
        EffectKind::Faces,
    ];

    /// The name people see.
    pub fn label(self) -> &'static str {
        match self {
            EffectKind::On => "On",
            EffectKind::Off => "Off",
            EffectKind::ColorWash => "Color Wash",
            EffectKind::Fade => "Fade",
            EffectKind::Chase => "Chase",
            EffectKind::Bars => "Bars",
            EffectKind::Wave => "Wave",
            EffectKind::Twinkle => "Twinkle",
            EffectKind::Shimmer => "Shimmer",
            EffectKind::Strobe => "Strobe",
            EffectKind::Spiral => "Spiral",
            EffectKind::Fire => "Fire",
            EffectKind::Meteors => "Meteors",
            EffectKind::Ripple => "Ripple",
            EffectKind::Faces => "Faces",
        }
    }

    /// What the effect looks like, in a sentence.
    pub fn description(self) -> &'static str {
        match self {
            EffectKind::On => {
                "A solid color, or the palette spread across the prop, at a steady or changing brightness."
            }
            EffectKind::Off => "Black: hides the layers below.",
            EffectKind::ColorWash => "Blends through the palette over the effect.",
            EffectKind::Fade => "The first palette color fading in or out.",
            EffectKind::Chase => "Bands of light moving along the pixels in wiring order.",
            EffectKind::Bars => "Stripes in the palette colors sliding across or up the prop.",
            EffectKind::Wave => "A wavy line rolling across the prop.",
            EffectKind::Twinkle => "Random pixels softly brightening and dimming.",
            EffectKind::Shimmer => "The whole prop flickering on and off.",
            EffectKind::Strobe => "Random pixels flashing, a new set each flash.",
            EffectKind::Spiral => "Diagonal stripes turning around the prop (made for trees and matrices).",
            EffectKind::Fire => "Flames rising from the bottom of the prop.",
            EffectKind::Meteors => "Streaks of light with fading tails.",
            EffectKind::Ripple => "Rings spreading out from the center of the prop.",
            EffectKind::Faces => {
                "A singing face: the prop's face mouths the words on a timing track, with eyes that blink."
            }
        }
    }

    /// The kind's settings table: keys, labels, ranges (see [`crate::effect_catalog`]).
    pub fn settings(self) -> &'static [SettingSpec] {
        match self {
            EffectKind::On => OnParams::SETTINGS,
            EffectKind::Off => OffParams::SETTINGS,
            EffectKind::ColorWash => ColorWashParams::SETTINGS,
            EffectKind::Fade => FadeParams::SETTINGS,
            EffectKind::Chase => ChaseParams::SETTINGS,
            EffectKind::Bars => BarsParams::SETTINGS,
            EffectKind::Wave => WaveParams::SETTINGS,
            EffectKind::Twinkle => TwinkleParams::SETTINGS,
            EffectKind::Shimmer => ShimmerParams::SETTINGS,
            EffectKind::Strobe => StrobeParams::SETTINGS,
            EffectKind::Spiral => SpiralParams::SETTINGS,
            EffectKind::Fire => FireParams::SETTINGS,
            EffectKind::Meteors => MeteorsParams::SETTINGS,
            EffectKind::Ripple => RippleParams::SETTINGS,
            EffectKind::Faces => FacesParams::SETTINGS,
        }
    }
}

/// Settings for each kind of effect. Missing settings take their defaults, so
/// `{ "kind": "chase" }` is a complete chase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EffectParams {
    On(OnParams),
    Off(OffParams),
    ColorWash(ColorWashParams),
    Fade(FadeParams),
    Chase(ChaseParams),
    Bars(BarsParams),
    Wave(WaveParams),
    Twinkle(TwinkleParams),
    Shimmer(ShimmerParams),
    Strobe(StrobeParams),
    Spiral(SpiralParams),
    Fire(FireParams),
    Meteors(MeteorsParams),
    Ripple(RippleParams),
    Faces(FacesParams),
}

impl EffectParams {
    pub fn kind(&self) -> EffectKind {
        match self {
            EffectParams::On(_) => EffectKind::On,
            EffectParams::Off(_) => EffectKind::Off,
            EffectParams::ColorWash(_) => EffectKind::ColorWash,
            EffectParams::Fade(_) => EffectKind::Fade,
            EffectParams::Chase(_) => EffectKind::Chase,
            EffectParams::Bars(_) => EffectKind::Bars,
            EffectParams::Wave(_) => EffectKind::Wave,
            EffectParams::Twinkle(_) => EffectKind::Twinkle,
            EffectParams::Shimmer(_) => EffectKind::Shimmer,
            EffectParams::Strobe(_) => EffectKind::Strobe,
            EffectParams::Spiral(_) => EffectKind::Spiral,
            EffectParams::Fire(_) => EffectKind::Fire,
            EffectParams::Meteors(_) => EffectKind::Meteors,
            EffectParams::Ripple(_) => EffectKind::Ripple,
            EffectParams::Faces(_) => EffectKind::Faces,
        }
    }

    /// Default settings for a kind.
    pub fn default_for(kind: EffectKind) -> Self {
        match kind {
            EffectKind::On => EffectParams::On(OnParams::default()),
            EffectKind::Off => EffectParams::Off(OffParams::default()),
            EffectKind::ColorWash => EffectParams::ColorWash(ColorWashParams::default()),
            EffectKind::Fade => EffectParams::Fade(FadeParams::default()),
            EffectKind::Chase => EffectParams::Chase(ChaseParams::default()),
            EffectKind::Bars => EffectParams::Bars(BarsParams::default()),
            EffectKind::Wave => EffectParams::Wave(WaveParams::default()),
            EffectKind::Twinkle => EffectParams::Twinkle(TwinkleParams::default()),
            EffectKind::Shimmer => EffectParams::Shimmer(ShimmerParams::default()),
            EffectKind::Strobe => EffectParams::Strobe(StrobeParams::default()),
            EffectKind::Spiral => EffectParams::Spiral(SpiralParams::default()),
            EffectKind::Fire => EffectParams::Fire(FireParams::default()),
            EffectKind::Meteors => EffectParams::Meteors(MeteorsParams::default()),
            EffectKind::Ripple => EffectParams::Ripple(RippleParams::default()),
            EffectKind::Faces => EffectParams::Faces(FacesParams::default()),
        }
    }

    /// Pulls every setting into the range its kind's table allows (NaN becomes the default).
    /// Files are clamped like this when they open, and the renderer draws clamped settings.
    pub fn sanitize(&mut self) {
        match self {
            EffectParams::On(p) => p.sanitize(),
            EffectParams::Off(p) => p.sanitize(),
            EffectParams::ColorWash(p) => p.sanitize(),
            EffectParams::Fade(p) => p.sanitize(),
            EffectParams::Chase(p) => p.sanitize(),
            EffectParams::Bars(p) => p.sanitize(),
            EffectParams::Wave(p) => p.sanitize(),
            EffectParams::Twinkle(p) => p.sanitize(),
            EffectParams::Shimmer(p) => p.sanitize(),
            EffectParams::Strobe(p) => p.sanitize(),
            EffectParams::Spiral(p) => p.sanitize(),
            EffectParams::Fire(p) => p.sanitize(),
            EffectParams::Meteors(p) => p.sanitize(),
            EffectParams::Ripple(p) => p.sanitize(),
            EffectParams::Faces(p) => p.sanitize(),
        }
    }

    /// A clamped copy (see [`EffectParams::sanitize`]).
    pub fn sanitized(&self) -> Self {
        let mut copy = self.clone();
        copy.sanitize();
        copy
    }

    /// The first setting outside its range, in plain words (e.g. "Speed is 70; use 0 to 50").
    pub fn setting_problem(&self) -> Option<String> {
        let found = match self {
            EffectParams::On(p) => p.setting_problem(),
            EffectParams::Off(p) => p.setting_problem(),
            EffectParams::ColorWash(p) => p.setting_problem(),
            EffectParams::Fade(p) => p.setting_problem(),
            EffectParams::Chase(p) => p.setting_problem(),
            EffectParams::Bars(p) => p.setting_problem(),
            EffectParams::Wave(p) => p.setting_problem(),
            EffectParams::Twinkle(p) => p.setting_problem(),
            EffectParams::Shimmer(p) => p.setting_problem(),
            EffectParams::Strobe(p) => p.setting_problem(),
            EffectParams::Spiral(p) => p.setting_problem(),
            EffectParams::Fire(p) => p.setting_problem(),
            EffectParams::Meteors(p) => p.setting_problem(),
            EffectParams::Ripple(p) => p.setting_problem(),
            EffectParams::Faces(p) => p.setting_problem(),
        };
        found.map(|(spec, why)| format!("{} {why}", spec.label))
    }
}

/// Spreads the palette across the prop instead of using one color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Gradient {
    #[default]
    None,
    /// Left to right.
    Horizontal,
    /// Bottom to top.
    Vertical,
}

choices!(Gradient { "none" => "None", "horizontal" => "Left to right", "vertical" => "Bottom to top" });

/// Which way something moves along the pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Toward higher pixel numbers, rightward, or upward.
    #[default]
    Forward,
    Reverse,
}

choices!(Direction { "forward" => "Forward", "reverse" => "Reverse" });

/// Across (horizontal) or up and down (vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

choices!(Axis { "horizontal" => "Across", "vertical" => "Up and down" });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FadeDirection {
    #[default]
    In,
    Out,
}

choices!(FadeDirection { "in" => "Fade in", "out" => "Fade out" });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum MeteorDirection {
    #[default]
    Down,
    Up,
    Left,
    Right,
}

choices!(MeteorDirection { "down" => "Down", "up" => "Up", "left" => "Left", "right" => "Right" });

/// A singing face's eyes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FaceEyes {
    Open,
    /// Open, blinking every few seconds.
    #[default]
    Auto,
    Closed,
}

choices!(FaceEyes { "open" => "Open", "auto" => "Open, blinking", "closed" => "Closed" });

/// Where a singing face's colors come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FaceColorSource {
    /// The colors the face was made with (the palette when it has none).
    #[default]
    Face,
    /// The palette: mouth, eyes, outline.
    Palette,
}

choices!(FaceColorSource { "face" => "The face's own", "palette" => "The palette (mouth, eyes, outline)" });

// Each settings struct below is the single table for its kind: type, default, JSON key, label,
// and range (see `settings.rs`). The renderer clamps to these ranges, files are clamped to them
// on open, and edits outside them are refused.

effect_params! {
    /// Solid color (the first palette color), or the palette spread across the prop, with the
    /// brightness going from `start_level` to `end_level` over the effect.
    #[derive(Copy)]
    pub struct OnParams {
        /// Spread the palette across the prop instead of using the first color.
        gradient: Gradient = Gradient::None => "gradient", "Gradient", choice;
        /// Brightness at the start of the effect (0–1).
        start_level: f32 = 1.0 => "startLevel", "Start brightness", number(0.0, 1.0, 0.01);
        /// Brightness at the end of the effect (0–1).
        end_level: f32 = 1.0 => "endLevel", "End brightness", number(0.0, 1.0, 0.01);
    }
}

effect_params! {
    /// Black: hides the layers below.
    #[derive(Copy)]
    pub struct OffParams {}
}

effect_params! {
    /// Blends through the palette over the effect, `cycles` times; with a gradient, the colors
    /// also spread across the prop.
    #[derive(Copy)]
    pub struct ColorWashParams {
        /// Times through the palette over the effect.
        cycles: f32 = 1.0 => "cycles", "Cycles", number(0.0, 100.0, 0.1);
        /// Also spread the colors across the prop.
        gradient: Gradient = Gradient::None => "gradient", "Gradient", choice;
    }
}

effect_params! {
    /// The first palette color fading in (or out) over the effect.
    #[derive(Copy)]
    pub struct FadeParams {
        /// Fade in or fade out.
        direction: FadeDirection = FadeDirection::In => "direction", "Direction", choice;
    }
}

effect_params! {
    /// Bands of light moving along the pixels in wiring order (several bands make a marquee).
    #[derive(Copy)]
    pub struct ChaseParams {
        /// Trips along the whole prop per second.
        speed: f32 = 1.0 => "speed", "Speed", number(0.0, 50.0, 0.1, "per second");
        /// Each band's length, as a fraction of the space between bands (0–1).
        width: f32 = 0.2 => "width", "Band width", number(0.0, 1.0, 0.01);
        /// Bands spread evenly along the prop; they take the palette colors in turn.
        bands: u32 = 1 => "bands", "Bands", int(1, 1000);
        /// Which way the bands move along the wiring.
        direction: Direction = Direction::Forward => "direction", "Direction", choice;
        /// Go back and forth instead of wrapping around.
        bounce: bool = false => "bounce", "Bounce", toggle;
    }
}

effect_params! {
    /// Stripes in the palette colors sliding across or up the prop.
    #[derive(Copy)]
    pub struct BarsParams {
        /// Bars visible at once.
        count: u32 = 4 => "count", "Bars", int(1, 100);
        /// Prop widths (or heights) moved per second.
        speed: f32 = 0.5 => "speed", "Speed", number(0.0, 20.0, 0.05, "per second");
        /// Bars sliding across or up and down.
        axis: Axis = Axis::Vertical => "axis", "Axis", choice;
        /// Which way the bars slide.
        direction: Direction = Direction::Forward => "direction", "Direction", choice;
    }
}

effect_params! {
    /// A sine wave line across the prop, rolling sideways.
    #[derive(Copy)]
    pub struct WaveParams {
        /// Waves across the prop.
        cycles: f32 = 1.0 => "cycles", "Waves", number(0.0, 20.0, 0.1);
        /// Waves passing per second.
        speed: f32 = 1.0 => "speed", "Speed", number(0.0, 20.0, 0.1, "per second");
        /// Peak-to-peak height, as a fraction of the prop's height (0–1).
        height: f32 = 0.8 => "height", "Height", number(0.0, 1.0, 0.01);
        /// Line thickness, as a fraction of the prop's height.
        thickness: f32 = 0.2 => "thickness", "Thickness", number(0.0, 1.0, 0.01);
        /// Which way the wave rolls.
        direction: Direction = Direction::Forward => "direction", "Direction", choice;
    }
}

effect_params! {
    /// Random pixels softly brightening and dimming.
    #[derive(Copy)]
    pub struct TwinkleParams {
        /// Fraction of pixels lit at any moment (0–1).
        density: f32 = 0.3 => "density", "Density", number(0.0, 1.0, 0.01);
        /// Twinkles per second for each pixel.
        rate: f32 = 1.0 => "rate", "Rate", number(0.0, 50.0, 0.1, "per second");
    }
}

effect_params! {
    /// The whole prop flickering on and off.
    #[derive(Copy)]
    pub struct ShimmerParams {
        /// Flickers per second.
        rate: f32 = 10.0 => "rate", "Rate", number(0.0, 60.0, 0.5, "per second");
        /// Fraction of each flicker spent on (0–1).
        duty: f32 = 0.5 => "duty", "On time", number(0.0, 1.0, 0.01);
    }
}

effect_params! {
    /// Random pixels flashing, a new set each flash.
    #[derive(Copy)]
    pub struct StrobeParams {
        /// Flashes per second.
        rate: f32 = 10.0 => "rate", "Rate", number(0.0, 60.0, 0.5, "per second");
        /// Fraction of pixels in each flash (0–1).
        density: f32 = 0.2 => "density", "Density", number(0.0, 1.0, 0.01);
    }
}

effect_params! {
    /// Diagonal stripes rotating around the prop (made for trees and matrices).
    #[derive(Copy)]
    pub struct SpiralParams {
        /// Stripes around the prop.
        count: u32 = 3 => "count", "Stripes", int(1, 100);
        /// Turns per second.
        speed: f32 = 0.5 => "speed", "Speed", number(0.0, 20.0, 0.05, "turns per second");
        /// Fraction of each stripe's space that is lit (0–1).
        thickness: f32 = 0.5 => "thickness", "Thickness", number(0.0, 1.0, 0.01);
        /// How steeply the stripes slant (0 = straight up; negative slants the other way).
        twist: f32 = 1.0 => "twist", "Twist", number(-10.0, 10.0, 0.1);
        /// Which way the stripes turn.
        direction: Direction = Direction::Forward => "direction", "Direction", choice;
    }
}

effect_params! {
    /// Flames rising from the bottom of the prop.
    #[derive(Copy)]
    pub struct FireParams {
        /// How high the flames reach, as a fraction of the prop's height (0–1).
        height: f32 = 0.8 => "height", "Height", number(0.0, 1.0, 0.01);
        /// How often new flames flare up (0–1).
        sparks: f32 = 0.6 => "sparks", "Sparks", number(0.0, 1.0, 0.01);
    }
}

effect_params! {
    /// Streaks of light with fading tails.
    #[derive(Copy)]
    pub struct MeteorsParams {
        /// Meteors on the prop at once.
        count: u32 = 5 => "count", "Meteors", int(1, 100);
        /// Prop heights (or widths) travelled per second.
        speed: f32 = 1.0 => "speed", "Speed", number(0.01, 20.0, 0.01, "per second");
        /// Tail length, as a fraction of the prop's height (or width).
        length: f32 = 0.25 => "length", "Tail length", number(0.01, 1.0, 0.01);
        /// Which way the meteors fall.
        direction: MeteorDirection = MeteorDirection::Down => "direction", "Direction", choice;
    }
}

effect_params! {
    /// Rings spreading out from the center of the prop.
    #[derive(Copy)]
    pub struct RippleParams {
        /// How fast rings grow: center-to-corner distances per second.
        speed: f32 = 0.5 => "speed", "Speed", number(0.0, 20.0, 0.05, "per second");
        /// Distance between rings (center-to-corner = 1).
        spacing: f32 = 0.4 => "spacing", "Spacing", number(0.01, 4.0, 0.01);
        /// Ring thickness (center-to-corner = 1).
        thickness: f32 = 0.12 => "thickness", "Thickness", number(0.01, 1.0, 0.01);
    }
}

effect_params! {
    /// A singing face: the mouth shape for the sound under the playhead on a timing track (a
    /// phoneme track, or words and lyrics read letter by letter), with eyes and an outline.
    pub struct FacesParams {
        /// Which of the prop's faces sings (blank: its first face).
        face: String = String::new() => "face", "Face", face;
        /// The phonemes, words, or lyrics the face sings.
        timing_track: Option<TimingTrackId> = None => "timingTrack", "Timing track", timing_track;
        /// Open, open and blinking every few seconds, or closed.
        eyes: FaceEyes = FaceEyes::Auto => "eyes", "Eyes", choice;
        /// The face's own colors, or the palette (first color the mouth, second the eyes, third
        /// the outline).
        colors: FaceColorSource = FaceColorSource::Face => "colors", "Colors", choice;
        /// Light the face's outline too.
        outline: bool = false => "outline", "Show outline", toggle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_settings_read_and_write_with_defaults() {
        let params: EffectParams = serde_json::from_str(r#"{ "kind": "faces", "face": "Singer" }"#).unwrap();
        let EffectParams::Faces(faces) = &params else {
            panic!("faces")
        };
        assert_eq!(faces.face, "Singer");
        assert_eq!(
            (faces.timing_track, faces.eyes, faces.colors, faces.outline),
            (None, FaceEyes::Auto, FaceColorSource::Face, false)
        );
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["timingTrack"], serde_json::Value::Null);
        assert_eq!(json["eyes"], "auto");
        assert_eq!(json["colors"], "face");
    }

    #[test]
    fn params_are_tagged_by_kind_and_missing_settings_take_defaults() {
        let params: EffectParams = serde_json::from_str(r#"{ "kind": "chase", "bands": 3 }"#).unwrap();
        assert_eq!(
            params,
            EffectParams::Chase(ChaseParams {
                bands: 3,
                ..ChaseParams::default()
            })
        );
        assert_eq!(params.kind(), EffectKind::Chase);
        let off: EffectParams = serde_json::from_str(r#"{ "kind": "off" }"#).unwrap();
        assert_eq!(off.kind(), EffectKind::Off);
        let json = serde_json::to_value(EffectParams::default_for(EffectKind::ColorWash)).unwrap();
        assert_eq!(json["kind"], "colorWash");
        assert_eq!(json["gradient"], "none");
    }

    #[test]
    fn every_kind_has_defaults_that_round_trip() {
        for kind in EffectKind::ALL {
            let params = EffectParams::default_for(kind);
            assert_eq!(params.kind(), kind);
            let text = serde_json::to_string(&params).unwrap();
            assert_eq!(
                serde_json::from_str::<EffectParams>(&text).unwrap(),
                params,
                "{text}"
            );
            assert!(!kind.label().is_empty());
        }
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        let err = serde_json::from_str::<EffectParams>(r#"{ "kind": "lasers" }"#).unwrap_err();
        assert!(err.to_string().contains("lasers"), "{err}");
    }

    #[test]
    fn effect_timing_helpers() {
        let a = Effect::new(EffectKind::On, 100, 200);
        assert!(a.is_active_at(100) && a.is_active_at(199) && !a.is_active_at(200));
        assert!(a.overlaps(&Effect::new(EffectKind::On, 150, 250)));
        assert!(!a.overlaps(&Effect::new(EffectKind::On, 200, 250)));
        assert_eq!(a.duration_ms(), 100);
        assert_eq!(Effect::new(EffectKind::On, 300, 200).duration_ms(), 0);
    }
}
