//! Effects: what lights up, when, and how it mixes with what's underneath.

use crate::settings::{SettingRange, SettingSpec, choices, effect_params};
use crate::{Curve, CurveInputs, CurveTime, EffectId, Rgb, TimingTrackId};
use pf_model::{BufferTransform, RenderStyle};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

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
// The lowest effect drawn on a row at any moment always covers, whatever its blend, as in xLights
// (there is nothing below it to mix with). Apart from Normal, the blends work on the colors as
// they show (dimmed by coverage and fades), as xLights' layer methods do, and "lit" means
// brighter than black on an 8-bit pixel. Each variant notes xLights' name for it, where "1" is
// this effect and "2" the layers below it. Not here: xLights' "Effect 1" and "Effect 2" (they
// cross-fade by the layer mix amount, which PixelFlow doesn't have). The notes are plain comments
// so the AI tools' schema stays small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Blend {
    /// Covers what's below where lit.
    #[default]
    Normal,
    /// Adds its light.
    // xLights: Additive.
    Add,
    /// Takes its light away.
    // xLights: Subtractive.
    Subtract,
    /// The brighter of the two, per channel.
    Max,
    /// The darker of the two, per channel.
    Min,
    /// Tints what's below by its colors.
    Multiply,
    /// Averages the two where both are lit.
    Average,
    /// Shows where lit; what's below elsewhere.
    // xLights: 1 reveals 2.
    Over,
    /// Shows only where what's below is dark.
    // xLights: 2 reveals 1, and Layered (the same).
    Behind,
    /// Blacks out what's below where lit.
    // xLights: 1 is Mask.
    Mask,
    /// What's below only where lit; black elsewhere.
    // xLights: 1 is True Unmask.
    Reveal,
    /// What's below at its brightness where lit; black elsewhere.
    // xLights: 1 is Unmask.
    RevealBrightness,
    /// Shows only where what's below is dark; black elsewhere.
    // xLights: 2 is Mask.
    CutOut,
    /// Shows only where what's below is lit; black elsewhere.
    // xLights: 2 is True Unmask.
    Clip,
    /// Its colors at the brightness of what's below; black where that's dark.
    // xLights: 2 is Unmask.
    ClipBrightness,
    /// Shifts the hue of what's below where lit.
    // xLights: Shadow 1 on 2.
    Shadow,
    /// Its colors, hue shifted by what's below.
    // xLights: Shadow 2 on 1.
    ShadowBelow,
    /// Shows only where both are lit.
    Highlight,
    /// Adds its light only where what's below is lit.
    // xLights: Highlight Vibrant.
    HighlightAdd,
    /// Shows on the bottom half; what's below on the top half.
    // xLights: Bottom-Top.
    BottomHalf,
    /// Shows on the left half; what's below on the right half.
    // xLights: Left-Right.
    LeftHalf,
}

impl Blend {
    pub const ALL: [Blend; 21] = [
        Blend::Normal,
        Blend::Add,
        Blend::Subtract,
        Blend::Max,
        Blend::Min,
        Blend::Multiply,
        Blend::Average,
        Blend::Over,
        Blend::Behind,
        Blend::Mask,
        Blend::Reveal,
        Blend::RevealBrightness,
        Blend::CutOut,
        Blend::Clip,
        Blend::ClipBrightness,
        Blend::Shadow,
        Blend::ShadowBelow,
        Blend::Highlight,
        Blend::HighlightAdd,
        Blend::BottomHalf,
        Blend::LeftHalf,
    ];

    /// The blend's name in files (`"cutOut"`).
    pub fn key(self) -> &'static str {
        match self {
            Blend::Normal => "normal",
            Blend::Add => "add",
            Blend::Subtract => "subtract",
            Blend::Max => "max",
            Blend::Min => "min",
            Blend::Multiply => "multiply",
            Blend::Average => "average",
            Blend::Over => "over",
            Blend::Behind => "behind",
            Blend::Mask => "mask",
            Blend::Reveal => "reveal",
            Blend::RevealBrightness => "revealBrightness",
            Blend::CutOut => "cutOut",
            Blend::Clip => "clip",
            Blend::ClipBrightness => "clipBrightness",
            Blend::Shadow => "shadow",
            Blend::ShadowBelow => "shadowBelow",
            Blend::Highlight => "highlight",
            Blend::HighlightAdd => "highlightAdd",
            Blend::BottomHalf => "bottomHalf",
            Blend::LeftHalf => "leftHalf",
        }
    }
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
    /// Sparkles on lit pixels: 0 (none) to 200.
    // xLights' palette Sparkles slider: each lit pixel flashes `sparkle_color` once every
    // `208 - sparkles` frames, at its own random moment (see pf-render's sparkles). Up to
    // [`crate::MAX_SPARKLES`].
    #[serde(default)]
    pub sparkles: u32,
    #[serde(default = "white")]
    pub sparkle_color: Rgb,
    /// Sparkles follow the music: as many as its loudness, up to `sparkles`.
    // xLights' "Music" sparkles checkbox: the count times the music's peak in the frame. Without
    // the music, the steady count.
    #[serde(default, skip_serializing_if = "is_false")]
    pub music_sparkles: bool,
    /// Softens the effect: 0 (none) to 14.
    // Before it mixes with the layers below; xLights' Blur setting minus one, up to
    // [`crate::MAX_BLUR`].
    #[serde(default)]
    pub blur: u32,
    /// How the target's pixels are laid out for the effect (the target's own layout by default).
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    pub render_style: RenderStyle,
    /// Turns or flips the layout the effect draws on.
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    pub buffer_transform: BufferTransform,
    /// Settings that change over the effect, by key (a `params` number setting, `sparkles`, or
    /// `blur`); each takes the place of that setting's value while the effect plays.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub curves: BTreeMap<String, Curve>,
}

fn white() -> Rgb {
    Rgb::WHITE
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A number setting a curve can change: its range, and whether it's a whole number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveRange {
    pub min: f32,
    pub max: f32,
    pub whole: bool,
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
            sparkles: 0,
            sparkle_color: Rgb::WHITE,
            music_sparkles: false,
            blur: 0,
            render_style: RenderStyle::Default,
            buffer_transform: BufferTransform::None,
            curves: BTreeMap::new(),
        }
    }

    /// The range of the number setting `key` a curve can change on an effect of `kind`: one of
    /// the kind's number settings, `sparkles`, or `blur`. `None` for any other key.
    pub fn curve_range(kind: EffectKind, key: &str) -> Option<CurveRange> {
        let whole = |max: u32| CurveRange {
            min: 0.0,
            max: max as f32,
            whole: true,
        };
        match key {
            "sparkles" => return Some(whole(crate::MAX_SPARKLES)),
            "blur" => return Some(whole(crate::MAX_BLUR)),
            _ => {}
        }
        match kind.settings().iter().find(|s| s.key == key)?.range {
            SettingRange::Number { min, max, .. } => Some(CurveRange {
                min,
                max,
                whole: false,
            }),
            SettingRange::Int { min, max, .. } => Some(CurveRange {
                min: min as f32,
                max: max as f32,
                whole: true,
            }),
            _ => None,
        }
    }

    /// Pulls the settings every effect has into range: sparkles, blur, and curves (see
    /// [`EffectParams::sanitize`] for the kind's own). Curves on settings the kind doesn't have
    /// are dropped.
    pub fn sanitize(&mut self) {
        self.params.sanitize();
        self.sparkles = self.sparkles.min(crate::MAX_SPARKLES);
        self.blur = self.blur.min(crate::MAX_BLUR);
        let kind = self.kind();
        self.curves
            .retain(|key, curve| match Self::curve_range(kind, key) {
                Some(range) => {
                    curve.sanitize(range.min, range.max);
                    true
                }
                None => false,
            });
    }

    /// Where the effect is in its own time at `t_ms`: 0 at its start, approaching 1 at its end.
    pub fn progress(&self, t_ms: u64) -> f32 {
        let length = self.duration_ms().max(1);
        let elapsed = t_ms.saturating_sub(self.start_ms);
        (elapsed as f64 / length as f64).clamp(0.0, 1.0) as f32
    }

    /// The effect as it plays at `t_ms`: each curve's value at that moment in place of its
    /// setting (the effect itself when it has no curves). Curves that follow the music or a
    /// timing track sit halfway (see [`Effect::at_with`]).
    pub fn at(&self, t_ms: u64) -> Cow<'_, Effect> {
        self.at_with(t_ms, &CurveInputs::default())
    }

    /// [`Effect::at`] with the music and timing tracks that curves (and music sparkles) follow.
    pub fn at_with(&self, t_ms: u64, inputs: &CurveInputs) -> Cow<'_, Effect> {
        let music = self.music_sparkles.then_some(inputs.peak).flatten();
        if self.curves.is_empty() && music.is_none() {
            return Cow::Borrowed(self);
        }
        let at = CurveTime {
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            t_ms,
        };
        let mut now = Effect {
            id: self.id,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            params: self.params.clone(),
            palette: self.palette.clone(),
            blend: self.blend,
            fade_in_ms: self.fade_in_ms,
            fade_out_ms: self.fade_out_ms,
            sparkles: self.sparkles,
            sparkle_color: self.sparkle_color,
            music_sparkles: self.music_sparkles,
            blur: self.blur,
            render_style: self.render_style,
            buffer_transform: self.buffer_transform,
            curves: BTreeMap::new(),
        };
        for (key, curve) in &self.curves {
            let value = curve.value_in(at, inputs);
            // Saturating casts: NaN becomes 0, and the limits clamp the rest.
            match key.as_str() {
                "sparkles" => now.sparkles = (value.round() as u32).min(crate::MAX_SPARKLES),
                "blur" => now.blur = (value.round() as u32).min(crate::MAX_BLUR),
                _ => {
                    now.params.set_number(key, value);
                }
            }
        }
        if let Some(peak) = music {
            // xLights' `(int)(factor * count)`.
            let frame = t_ms / u64::from(if inputs.frame_ms == 0 { 50 } else { inputs.frame_ms });
            now.sparkles = (peak(frame).clamp(0.0, 1.0) * now.sparkles as f32) as u32;
        }
        Cow::Owned(now)
    }

    /// The first setting outside its range, in plain words (see [`EffectParams::setting_problem`]).
    pub fn setting_problem(&self) -> Option<String> {
        if let Some(problem) = self.params.setting_problem() {
            return Some(problem);
        }
        if self.sparkles > crate::MAX_SPARKLES {
            return Some(format!(
                "Sparkles is {}; use 0 to {}",
                self.sparkles,
                crate::MAX_SPARKLES
            ));
        }
        if self.blur > crate::MAX_BLUR {
            return Some(format!("Blur is {}; use 0 to {}", self.blur, crate::MAX_BLUR));
        }
        let kind = self.kind();
        for (key, curve) in &self.curves {
            let Some(range) = Self::curve_range(kind, key) else {
                return Some(format!(
                    "'{key}' can't change over the effect: the {} effect has no number setting by that name",
                    kind.label()
                ));
            };
            if let Some(why) = curve.problem(range.min, range.max) {
                let label = match key.as_str() {
                    "sparkles" => "Sparkles",
                    "blur" => "Blur",
                    _ => kind
                        .settings()
                        .iter()
                        .find(|s| s.key == key)
                        .map_or("A", |s| s.label),
                };
                return Some(format!("{label}'s curve {why}"));
            }
        }
        None
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
    Shape,
    Fan,
    Morph,
    Circles,
    Pinwheel,
    Snowflakes,
    Plasma,
    Butterfly,
    Garlands,
    Lines,
    Life,
    Tendril,
    Text,
    Faces,
    VuMeter,
}

impl EffectKind {
    pub const ALL: [EffectKind; 29] = [
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
        EffectKind::Shape,
        EffectKind::Fan,
        EffectKind::Morph,
        EffectKind::Circles,
        EffectKind::Pinwheel,
        EffectKind::Snowflakes,
        EffectKind::Plasma,
        EffectKind::Butterfly,
        EffectKind::Garlands,
        EffectKind::Lines,
        EffectKind::Life,
        EffectKind::Tendril,
        EffectKind::Text,
        EffectKind::Faces,
        EffectKind::VuMeter,
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
            EffectKind::Shape => "Shape",
            EffectKind::Fan => "Fan",
            EffectKind::Morph => "Morph",
            EffectKind::Circles => "Circles",
            EffectKind::Pinwheel => "Pinwheel",
            EffectKind::Snowflakes => "Snowflakes",
            EffectKind::Plasma => "Plasma",
            EffectKind::Butterfly => "Butterfly",
            EffectKind::Garlands => "Garlands",
            EffectKind::Lines => "Lines",
            EffectKind::Life => "Life",
            EffectKind::Tendril => "Tendril",
            EffectKind::Text => "Text",
            EffectKind::Faces => "Faces",
            EffectKind::VuMeter => "VU Meter",
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
            EffectKind::Shape => {
                "Shapes (stars, hearts, snowflakes, and more) appearing, growing, and fading."
            }
            EffectKind::Fan => "Blades of color spinning out from a center point.",
            EffectKind::Morph => "A line sweeping from one place to another, with a head and a fading tail.",
            EffectKind::Circles => "Balls of color moving around the prop, or rings spreading from a point.",
            EffectKind::Pinwheel => "Arms of color turning around a center point, like a pinwheel.",
            EffectKind::Snowflakes => "Snowflakes drifting across the prop, or falling and piling up.",
            EffectKind::Plasma => "Swirling waves of color flowing over the prop.",
            EffectKind::Butterfly => "Shifting patterns of color, like a butterfly's wings.",
            EffectKind::Garlands => "Rows of swags stacking up the prop, one after another.",
            EffectKind::Lines => "Lines bouncing around the prop, with fading trails.",
            EffectKind::Life => "Cells living and dying by the rules of the Game of Life.",
            EffectKind::Tendril => "Tendrils trailing after a point that moves around the prop.",
            EffectKind::Text => "Words scrolling across the prop, or standing still.",
            EffectKind::Faces => {
                "A singing face: the prop's face mouths the words on a timing track, with eyes that blink."
            }
            EffectKind::VuMeter => "Bars, levels, and flashes that follow the music or a timing track.",
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
            EffectKind::Shape => ShapeParams::SETTINGS,
            EffectKind::Fan => FanParams::SETTINGS,
            EffectKind::Morph => MorphParams::SETTINGS,
            EffectKind::Circles => CirclesParams::SETTINGS,
            EffectKind::Pinwheel => PinwheelParams::SETTINGS,
            EffectKind::Snowflakes => SnowflakesParams::SETTINGS,
            EffectKind::Plasma => PlasmaParams::SETTINGS,
            EffectKind::Butterfly => ButterflyParams::SETTINGS,
            EffectKind::Garlands => GarlandsParams::SETTINGS,
            EffectKind::Lines => LinesParams::SETTINGS,
            EffectKind::Life => LifeParams::SETTINGS,
            EffectKind::Tendril => TendrilParams::SETTINGS,
            EffectKind::Text => TextParams::SETTINGS,
            EffectKind::Faces => FacesParams::SETTINGS,
            EffectKind::VuMeter => VuMeterParams::SETTINGS,
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
    Shape(ShapeParams),
    Fan(FanParams),
    Morph(MorphParams),
    Circles(CirclesParams),
    Pinwheel(PinwheelParams),
    Snowflakes(SnowflakesParams),
    Plasma(PlasmaParams),
    Butterfly(ButterflyParams),
    Garlands(GarlandsParams),
    Lines(LinesParams),
    Life(LifeParams),
    Tendril(TendrilParams),
    Text(TextParams),
    Faces(FacesParams),
    VuMeter(VuMeterParams),
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
            EffectParams::Shape(_) => EffectKind::Shape,
            EffectParams::Fan(_) => EffectKind::Fan,
            EffectParams::Morph(_) => EffectKind::Morph,
            EffectParams::Circles(_) => EffectKind::Circles,
            EffectParams::Pinwheel(_) => EffectKind::Pinwheel,
            EffectParams::Snowflakes(_) => EffectKind::Snowflakes,
            EffectParams::Plasma(_) => EffectKind::Plasma,
            EffectParams::Butterfly(_) => EffectKind::Butterfly,
            EffectParams::Garlands(_) => EffectKind::Garlands,
            EffectParams::Lines(_) => EffectKind::Lines,
            EffectParams::Life(_) => EffectKind::Life,
            EffectParams::Tendril(_) => EffectKind::Tendril,
            EffectParams::Text(_) => EffectKind::Text,
            EffectParams::Faces(_) => EffectKind::Faces,
            EffectParams::VuMeter(_) => EffectKind::VuMeter,
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
            EffectKind::Shape => EffectParams::Shape(ShapeParams::default()),
            EffectKind::Fan => EffectParams::Fan(FanParams::default()),
            EffectKind::Morph => EffectParams::Morph(MorphParams::default()),
            EffectKind::Circles => EffectParams::Circles(CirclesParams::default()),
            EffectKind::Pinwheel => EffectParams::Pinwheel(PinwheelParams::default()),
            EffectKind::Snowflakes => EffectParams::Snowflakes(SnowflakesParams::default()),
            EffectKind::Plasma => EffectParams::Plasma(PlasmaParams::default()),
            EffectKind::Butterfly => EffectParams::Butterfly(ButterflyParams::default()),
            EffectKind::Garlands => EffectParams::Garlands(GarlandsParams::default()),
            EffectKind::Lines => EffectParams::Lines(LinesParams::default()),
            EffectKind::Life => EffectParams::Life(LifeParams::default()),
            EffectKind::Tendril => EffectParams::Tendril(TendrilParams::default()),
            EffectKind::Text => EffectParams::Text(TextParams::default()),
            EffectKind::Faces => EffectParams::Faces(FacesParams::default()),
            EffectKind::VuMeter => EffectParams::VuMeter(VuMeterParams::default()),
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
            EffectParams::Shape(p) => p.sanitize(),
            EffectParams::Fan(p) => p.sanitize(),
            EffectParams::Morph(p) => p.sanitize(),
            EffectParams::Circles(p) => p.sanitize(),
            EffectParams::Pinwheel(p) => p.sanitize(),
            EffectParams::Snowflakes(p) => p.sanitize(),
            EffectParams::Plasma(p) => p.sanitize(),
            EffectParams::Butterfly(p) => p.sanitize(),
            EffectParams::Garlands(p) => p.sanitize(),
            EffectParams::Lines(p) => p.sanitize(),
            EffectParams::Life(p) => p.sanitize(),
            EffectParams::Tendril(p) => p.sanitize(),
            EffectParams::Text(p) => p.sanitize(),
            EffectParams::Faces(p) => p.sanitize(),
            EffectParams::VuMeter(p) => p.sanitize(),
        }
    }

    /// A clamped copy (see [`EffectParams::sanitize`]).
    pub fn sanitized(&self) -> Self {
        let mut copy = self.clone();
        copy.sanitize();
        copy
    }

    /// The number setting `key`; `None` when the kind has no number setting by that name.
    pub fn number(&self, key: &str) -> Option<f32> {
        match self {
            EffectParams::On(p) => p.number(key),
            EffectParams::Off(p) => p.number(key),
            EffectParams::ColorWash(p) => p.number(key),
            EffectParams::Fade(p) => p.number(key),
            EffectParams::Chase(p) => p.number(key),
            EffectParams::Bars(p) => p.number(key),
            EffectParams::Wave(p) => p.number(key),
            EffectParams::Twinkle(p) => p.number(key),
            EffectParams::Shimmer(p) => p.number(key),
            EffectParams::Strobe(p) => p.number(key),
            EffectParams::Spiral(p) => p.number(key),
            EffectParams::Fire(p) => p.number(key),
            EffectParams::Meteors(p) => p.number(key),
            EffectParams::Ripple(p) => p.number(key),
            EffectParams::Shape(p) => p.number(key),
            EffectParams::Fan(p) => p.number(key),
            EffectParams::Morph(p) => p.number(key),
            EffectParams::Circles(p) => p.number(key),
            EffectParams::Pinwheel(p) => p.number(key),
            EffectParams::Snowflakes(p) => p.number(key),
            EffectParams::Plasma(p) => p.number(key),
            EffectParams::Butterfly(p) => p.number(key),
            EffectParams::Garlands(p) => p.number(key),
            EffectParams::Lines(p) => p.number(key),
            EffectParams::Life(p) => p.number(key),
            EffectParams::Tendril(p) => p.number(key),
            EffectParams::Text(p) => p.number(key),
            EffectParams::Faces(p) => p.number(key),
            EffectParams::VuMeter(p) => p.number(key),
        }
    }

    /// Sets the number setting `key` to `value` (a whole number rounded; not clamped); false when
    /// the kind has no number setting by that name.
    pub fn set_number(&mut self, key: &str, value: f32) -> bool {
        match self {
            EffectParams::On(p) => p.set_number(key, value),
            EffectParams::Off(p) => p.set_number(key, value),
            EffectParams::ColorWash(p) => p.set_number(key, value),
            EffectParams::Fade(p) => p.set_number(key, value),
            EffectParams::Chase(p) => p.set_number(key, value),
            EffectParams::Bars(p) => p.set_number(key, value),
            EffectParams::Wave(p) => p.set_number(key, value),
            EffectParams::Twinkle(p) => p.set_number(key, value),
            EffectParams::Shimmer(p) => p.set_number(key, value),
            EffectParams::Strobe(p) => p.set_number(key, value),
            EffectParams::Spiral(p) => p.set_number(key, value),
            EffectParams::Fire(p) => p.set_number(key, value),
            EffectParams::Meteors(p) => p.set_number(key, value),
            EffectParams::Ripple(p) => p.set_number(key, value),
            EffectParams::Shape(p) => p.set_number(key, value),
            EffectParams::Fan(p) => p.set_number(key, value),
            EffectParams::Morph(p) => p.set_number(key, value),
            EffectParams::Circles(p) => p.set_number(key, value),
            EffectParams::Pinwheel(p) => p.set_number(key, value),
            EffectParams::Snowflakes(p) => p.set_number(key, value),
            EffectParams::Plasma(p) => p.set_number(key, value),
            EffectParams::Butterfly(p) => p.set_number(key, value),
            EffectParams::Garlands(p) => p.set_number(key, value),
            EffectParams::Lines(p) => p.set_number(key, value),
            EffectParams::Life(p) => p.set_number(key, value),
            EffectParams::Tendril(p) => p.set_number(key, value),
            EffectParams::Text(p) => p.set_number(key, value),
            EffectParams::Faces(p) => p.set_number(key, value),
            EffectParams::VuMeter(p) => p.set_number(key, value),
        }
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
            EffectParams::Shape(p) => p.setting_problem(),
            EffectParams::Fan(p) => p.setting_problem(),
            EffectParams::Morph(p) => p.setting_problem(),
            EffectParams::Circles(p) => p.setting_problem(),
            EffectParams::Pinwheel(p) => p.setting_problem(),
            EffectParams::Snowflakes(p) => p.setting_problem(),
            EffectParams::Plasma(p) => p.setting_problem(),
            EffectParams::Butterfly(p) => p.setting_problem(),
            EffectParams::Garlands(p) => p.setting_problem(),
            EffectParams::Lines(p) => p.setting_problem(),
            EffectParams::Life(p) => p.setting_problem(),
            EffectParams::Tendril(p) => p.setting_problem(),
            EffectParams::Text(p) => p.setting_problem(),
            EffectParams::Faces(p) => p.setting_problem(),
            EffectParams::VuMeter(p) => p.setting_problem(),
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

/// What the Shape effect draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ShapeObject {
    #[default]
    Circle,
    Ellipse,
    Triangle,
    Square,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Heart,
    Tree,
    Snowflake,
    CandyCane,
    Crucifix,
    Present,
    /// A different shape (not an ellipse) each time one appears.
    Random,
}

choices!(ShapeObject {
    "circle" => "Circle",
    "ellipse" => "Ellipse",
    "triangle" => "Triangle",
    "square" => "Square",
    "pentagon" => "Pentagon",
    "hexagon" => "Hexagon",
    "octagon" => "Octagon",
    "star" => "Star",
    "heart" => "Heart",
    "tree" => "Tree",
    "snowflake" => "Snowflake",
    "candyCane" => "Candy cane",
    "crucifix" => "Cross",
    "present" => "Present",
    "random" => "A random shape each time",
});

/// How the Circles effect draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CirclesLook {
    /// Solid balls.
    #[default]
    Solid,
    /// Balls bright in the middle, fading to their edge.
    Fading,
    /// Outlines drifting the same way.
    Bubbles,
    /// Glowing blobs that merge where they meet.
    Plasma,
    /// Rings of the palette colors spreading from a point.
    Radial,
    /// Rainbow rings spreading from a point.
    RainbowRadial,
}

choices!(CirclesLook {
    "solid" => "Solid balls",
    "fading" => "Fading balls",
    "bubbles" => "Bubbles",
    "plasma" => "Plasma blobs",
    "radial" => "Rings from a point",
    "rainbowRadial" => "Rainbow rings from a point",
});

effect_params! {
    /// Shapes appearing at random places (or one place), growing and fading over their lifetime;
    /// a new one takes each one's place as it goes, so `count` show at once. With a timing track,
    /// one appears at each of its marks instead.
    pub struct ShapeParams {
        /// The shape drawn.
        shape: ShapeObject = ShapeObject::Circle => "shape", "Shape", choice;
        /// Shapes on the prop at once.
        count: u32 = 5 => "count", "Shapes", int(1, 100);
        /// How long each shape lasts, as a share of the effect.
        lifetime: f32 = 5.0 => "lifetime", "Lifetime", number(1.0, 100.0, 1.0, "% of the effect");
        /// Each shape's size when it appears: its radius in pixels.
        start_size: f32 = 1.0 => "startSize", "Start size", number(0.0, 100.0, 1.0, "pixels");
        /// How much each shape's radius grows over its lifetime (negative shrinks it).
        growth: f32 = 10.0 => "growth", "Growth", number(-100.0, 100.0, 1.0, "pixels");
        /// Line thickness.
        thickness: u32 = 1 => "thickness", "Thickness", int(1, 100, "pixels");
        /// Dim each shape as it ages.
        fade: bool = true => "fade", "Fade away", toggle;
        /// Put each shape somewhere new at random, instead of at the center below.
        random_location: bool = true => "randomLocation", "Random places", toggle;
        /// Turns the shapes (circles and candy canes don't turn).
        rotation: f32 = 0.0 => "rotation", "Rotation", number(0.0, 360.0, 1.0, "degrees"), more;
        /// A star's points; for an ellipse, its height as tenths of its width.
        points: u32 = 5 => "points", "Points", int(2, 9), more;
        /// Where the shapes appear without random places, from the left (0) to the right (100).
        center_x: f32 = 50.0 => "centerX", "Center across", number(0.0, 100.0, 1.0, "%"), more;
        /// Where the shapes appear without random places, from the bottom (0) to the top (100).
        center_y: f32 = 50.0 => "centerY", "Center up", number(0.0, 100.0, 1.0, "%"), more;
        /// How fast each shape drifts.
        speed: f32 = 0.0 => "speed", "Drift speed", number(0.0, 1000.0, 1.0, "pixels per second"), more;
        /// Which way the shapes drift: 0 is right, 90 up.
        direction: f32 = 90.0 => "direction", "Drift direction", number(0.0, 359.0, 1.0, "degrees"), more;
        /// Each shape drifts at its own random speed and direction.
        random_movement: bool = false => "randomMovement", "Random drift", toggle, more;
        /// Start the first shapes part way through their lifetimes, so they don't all appear at once.
        random_start: bool = true => "randomStart", "Staggered start", toggle, more;
        /// Make a shape appear at each mark on this timing track, instead of keeping `count` shown.
        timing_track: Option<TimingTrackId> = None => "timingTrack", "Appear on marks of", timing_track, more;
        /// Make a shape appear when the music gets louder than the trigger level (and every 21
        /// frames while it stays louder), instead of keeping `count` shown.
        fire_on_music: bool = false => "fireOnMusic", "Appear with the music", toggle, more;
        /// How loud the music must get to make a shape appear.
        trigger_level: f32 = 50.0 => "triggerLevel", "Music trigger level", number(0.0, 100.0, 1.0, "%"), more;
    }
}

effect_params! {
    /// Blades of color spinning around a center point, growing out at the start and shrinking
    /// away at the end. Each blade takes the palette colors side by side.
    #[derive(Copy)]
    pub struct FanParams {
        /// The center, from the left (0) to the right (100).
        center_x: f32 = 50.0 => "centerX", "Center across", number(0.0, 100.0, 1.0, "%");
        /// The center, from the bottom (0) to the top (100).
        center_y: f32 = 50.0 => "centerY", "Center up", number(0.0, 100.0, 1.0, "%");
        /// Where the blades start (100 reaches the edge of the prop's longer side).
        start_radius: f32 = 1.0 => "startRadius", "Inner radius", number(0.0, 2500.0, 1.0);
        /// Where the blades end (100 reaches the edge of the prop's longer side).
        end_radius: f32 = 50.0 => "endRadius", "Outer radius", number(0.0, 2500.0, 1.0);
        /// Number of blades.
        blades: u32 = 3 => "blades", "Blades", int(1, 16);
        /// How much of each blade's slice of the circle it fills.
        blade_width: f32 = 50.0 => "bladeWidth", "Blade width", number(5.0, 100.0, 1.0, "%");
        /// Turns over the effect.
        revolutions: f32 = 2.0 => "revolutions", "Turns", number(0.0, 10.0, 0.05);
        /// How far the blades curve from center to tip (0 is straight).
        blade_angle: f32 = 90.0 => "bladeAngle", "Blade curve", number(-360.0, 360.0, 1.0, "degrees");
        /// How much of the effect the blades are at full length (they grow before and shrink after).
        duration: f32 = 80.0 => "duration", "Full length for", number(0.0, 100.0, 1.0, "%"), more;
        /// Where the first blade points at the start.
        start_angle: f32 = 0.0 => "startAngle", "Start angle", number(0.0, 360.0, 1.0, "degrees"), more;
        /// Stripes each color is split into across a blade.
        elements: u32 = 1 => "elements", "Stripes per color", int(1, 4), more;
        /// How much of its space each stripe fills.
        element_width: f32 = 100.0 => "elementWidth", "Stripe width", number(5.0, 100.0, 1.0, "%"), more;
        /// Speeds up (positive) or slows down (negative) the spin over the effect.
        acceleration: f32 = 0.0 => "acceleration", "Acceleration", number(-10.0, 10.0, 1.0), more;
        /// Which way the blades turn.
        direction: Direction = Direction::Forward => "direction", "Direction", choice, more;
        /// Soften each stripe toward its edges.
        blend_edges: bool = true => "blendEdges", "Soft edges", toggle, more;
        /// Radii as a share of the prop (100 reaches the edge of its longer side) instead of pixels.
        scale: bool = true => "scale", "Radius in % of the prop", toggle, more;
    }
}

effect_params! {
    /// A line from (startX1, startY1) to (startX2, startY2) sweeping to the line from (endX1,
    /// endY1) to (endX2, endY2): a head in the first palette colors, then a fading tail in the
    /// rest. Positions run 0–100 from the left and from the bottom.
    #[derive(Copy)]
    pub struct MorphParams {
        /// Where the line starts: its first end, from the left.
        start_x1: f32 = 0.0 => "startX1", "Start X1", number(0.0, 100.0, 1.0, "%");
        /// Where the line starts: its first end, from the bottom.
        start_y1: f32 = 0.0 => "startY1", "Start Y1", number(0.0, 100.0, 1.0, "%");
        /// Where the line starts: its second end, from the left.
        start_x2: f32 = 100.0 => "startX2", "Start X2", number(0.0, 100.0, 1.0, "%");
        /// Where the line starts: its second end, from the bottom.
        start_y2: f32 = 0.0 => "startY2", "Start Y2", number(0.0, 100.0, 1.0, "%");
        /// Where the line ends up: its first end, from the left.
        end_x1: f32 = 0.0 => "endX1", "End X1", number(0.0, 100.0, 1.0, "%");
        /// Where the line ends up: its first end, from the bottom.
        end_y1: f32 = 100.0 => "endY1", "End Y1", number(0.0, 100.0, 1.0, "%");
        /// Where the line ends up: its second end, from the left.
        end_x2: f32 = 100.0 => "endX2", "End X2", number(0.0, 100.0, 1.0, "%");
        /// Where the line ends up: its second end, from the bottom.
        end_y2: f32 = 100.0 => "endY2", "End Y2", number(0.0, 100.0, 1.0, "%");
        /// How much of the effect the head takes to cross; the tail follows it out.
        head_duration: f32 = 20.0 => "headDuration", "Head time", number(0.0, 100.0, 1.0, "% of the effect");
        /// The head's length as it sets off.
        start_length: f32 = 1.0 => "startLength", "Start head length",
            number(0.0, 100.0, 1.0, "pixels"), more;
        /// The head's length as it arrives.
        end_length: f32 = 1.0 => "endLength", "End head length", number(0.0, 100.0, 1.0, "pixels"), more;
        /// Speeds up (positive) or slows down (negative) the sweep over the effect.
        acceleration: f32 = 0.0 => "acceleration", "Acceleration", number(-10.0, 10.0, 1.0), more;
        /// Extra copies of the line, side by side.
        repeats: u32 = 0 => "repeats", "Repeats", int(0, 250), more;
        /// Space between the copies.
        repeat_spacing: u32 = 1 => "repeatSpacing", "Repeat spacing", int(1, 100, "pixels"), more;
        /// Starts the copies one after another instead of together (negative: last copy first).
        stagger: f32 = 0.0 => "stagger", "Stagger", number(-100.0, 100.0, 1.0), more;
        /// Show the whole head at its start before it sets off.
        head_at_start: bool = false => "headAtStart", "Show head at start", toggle, more;
        /// As many copies as fill the prop (instead of Repeats).
        auto_repeat: bool = false => "autoRepeat", "Repeat to fill", toggle, more;
    }
}

effect_params! {
    /// Balls of the palette colors moving around the prop (wrapping around its edges, or
    /// bouncing off them), or rings spreading from a point.
    #[derive(Copy)]
    pub struct CirclesParams {
        /// Number of balls (rings repeat the palette this many times across the rings).
        count: u32 = 3 => "count", "Circles", int(1, 10);
        /// Each ball's radius (for rings: how thin each color band is).
        size: u32 = 5 => "size", "Size", int(1, 20, "pixels");
        /// How fast the balls move or the rings spread (10 moves a ball 50 to 150 pixels a second).
        speed: f32 = 10.0 => "speed", "Speed", number(1.0, 30.0, 1.0);
        /// Solid or fading balls, bubbles, plasma, or rings.
        look: CirclesLook = CirclesLook::Solid => "look", "Look", choice;
        /// Bounce off the prop's edges instead of wrapping around to the other side.
        bounce: bool = false => "bounce", "Bounce", toggle;
        /// Where the rings spread from, left (-50) to right (50) of the center.
        center_x: f32 = 0.0 => "centerX", "Rings center across", number(-50.0, 50.0, 1.0), more;
        /// Where the rings spread from, below (-50) to above (50) the center.
        center_y: f32 = 0.0 => "centerY", "Rings center up", number(-50.0, 50.0, 1.0), more;
    }
}

/// How a Pinwheel's arms are shaded across their width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum PinwheelShading {
    #[default]
    Flat,
    /// Brightest down the middle of each arm (xLights' 3D).
    Raised,
    /// Brightest at each arm's edges (xLights' 3D Inverted).
    Sunken,
    /// Fading from one edge of each arm to the other (xLights' Sweep).
    Sweep,
}

choices!(PinwheelShading {
    "flat" => "Flat",
    "raised" => "Bright middle",
    "sunken" => "Bright edges",
    "sweep" => "Fading sweep",
});

/// How a Pinwheel's arms are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum PinwheelStyle {
    /// Every pixel within an arm lit (xLights' new render method).
    #[default]
    Smooth,
    /// Each arm drawn as a bundle of spokes (xLights' old render method).
    Spokes,
}

choices!(PinwheelStyle { "smooth" => "Smooth arms", "spokes" => "Spokes (older xLights)" });

effect_params! {
    /// Arms of the palette colors (from the second color on) turning around a center point,
    /// bending as they go out with twist.
    #[derive(Copy)]
    pub struct PinwheelParams {
        /// Number of arms.
        arms: u32 = 3 => "arms", "Arms", int(1, 20);
        /// How far the arms reach: 100 reaches the prop's corners.
        arm_size: f32 = 100.0 => "armSize", "Arm length", number(0.0, 400.0, 1.0, "%");
        /// How far the arms bend from center to tip.
        twist: f32 = 0.0 => "twist", "Twist", number(-360.0, 360.0, 1.0, "degrees");
        /// How much of each arm's slice of the circle it fills (0 is a thin line).
        thickness: f32 = 0.0 => "thickness", "Thickness", number(0.0, 100.0, 1.0, "%");
        /// How fast the arms turn (10 turns them 200 degrees a second).
        speed: f32 = 10.0 => "speed", "Speed", number(0.0, 50.0, 1.0);
        /// Turn counterclockwise (xLights' Rotation box) instead of clockwise.
        counterclockwise: bool = true => "counterclockwise", "Counterclockwise", toggle;
        /// Flat arms, or shaded across their width.
        shading: PinwheelShading = PinwheelShading::Flat => "shading", "Shading", choice;
        /// Where the first arm points at the start.
        offset: f32 = 0.0 => "offset", "Start angle", number(0.0, 360.0, 1.0, "degrees"), more;
        /// The center, left (-100) to right (100) of the middle.
        center_x: f32 = 0.0 => "centerX", "Center across", number(-100.0, 100.0, 1.0), more;
        /// The center, below (-100) to above (100) the middle.
        center_y: f32 = 0.0 => "centerY", "Center up", number(-100.0, 100.0, 1.0), more;
        /// Smooth arms, or arms drawn as spokes as older xLights did.
        style: PinwheelStyle = PinwheelStyle::Smooth => "style", "Drawing", choice, more;
    }
}

/// What each snowflake looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SnowflakeShape {
    /// A different look for each flake.
    Random,
    #[default]
    Dot,
    Cross,
    Bar,
    BigCross,
    Star,
    Square,
    Plus,
    Diamond,
    X,
}

choices!(SnowflakeShape {
    "random" => "A random look each",
    "dot" => "Dot",
    "cross" => "Small cross (two colors)",
    "bar" => "Three dots (two colors)",
    "bigCross" => "Large cross (two colors)",
    "star" => "Star (two colors)",
    "square" => "Square",
    "plus" => "Plus",
    "diamond" => "Diamond",
    "x" => "X",
});

/// How snowflakes move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum SnowflakesMotion {
    /// Sliding diagonally across the prop, wrapping around (xLights' Driving).
    #[default]
    Blowing,
    /// Falling to the bottom and away, new ones starting at the top.
    Falling,
    /// Falling and piling up at the bottom.
    PilingUp,
}

choices!(SnowflakesMotion {
    "blowing" => "Blowing",
    "falling" => "Falling",
    "pilingUp" => "Falling and piling up",
});

effect_params! {
    /// Snowflakes in the first palette color (the two-color looks take the second for their
    /// arms), blowing across the prop or falling down it.
    #[derive(Copy)]
    pub struct SnowflakesParams {
        /// Snowflakes on the prop at once.
        count: u32 = 5 => "count", "Flakes", int(1, 100);
        /// What each flake looks like.
        flake: SnowflakeShape = SnowflakeShape::Dot => "flake", "Flake", choice;
        /// How fast they move.
        speed: f32 = 10.0 => "speed", "Speed", number(0.0, 50.0, 1.0);
        /// Blowing across, falling, or falling and piling up.
        motion: SnowflakesMotion = SnowflakesMotion::Blowing => "motion", "Motion", choice;
        /// Frames of falling done before the effect starts, so the flakes are already spread out.
        warmup: u32 = 0 => "warmup", "Head start", int(0, 100, "frames"), more;
    }
}

/// Where Plasma's colors come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum PlasmaColors {
    #[default]
    Palette,
    RedGreen,
    BlueGreen,
    Rainbow,
    White,
}

choices!(PlasmaColors {
    "palette" => "The palette",
    "redGreen" => "Red and green",
    "blueGreen" => "Blue and green",
    "rainbow" => "Rainbow",
    "white" => "Shades of white",
});

effect_params! {
    /// Waves of color flowing over the prop, worked out from several moving sine waves.
    #[derive(Copy)]
    pub struct PlasmaParams {
        /// The palette blended across the waves, or a fixed color scheme.
        colors: PlasmaColors = PlasmaColors::Palette => "colors", "Colors", choice;
        /// Tightens the circular waves.
        twist: u32 = 1 => "twist", "Twist", int(1, 10);
        /// How many bands of color the waves are split into.
        density: u32 = 1 => "density", "Line density", int(1, 10);
        /// How fast the waves flow.
        speed: f32 = 10.0 => "speed", "Speed", number(0.0, 100.0, 1.0);
    }
}

/// Where Butterfly's colors come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ButterflyColors {
    #[default]
    Rainbow,
    Palette,
}

choices!(ButterflyColors { "rainbow" => "Rainbow", "palette" => "The palette" });

effect_params! {
    /// Patterns worked out from each pixel's place, shifting over time: butterfly wings
    /// (patterns 1 to 5) or plasma looks (6 to 10).
    #[derive(Copy)]
    pub struct ButterflyParams {
        /// Patterns 1 to 5 are wings; 6 to 9 plasmas in fixed colors, and 10 a plasma in the palette.
        pattern: u32 = 1 => "pattern", "Pattern", int(1, 10);
        /// A rainbow, or the palette blended across the pattern.
        colors: ButterflyColors = ButterflyColors::Rainbow => "colors", "Colors", choice;
        /// How fast the pattern shifts.
        speed: f32 = 10.0 => "speed", "Speed", number(0.0, 100.0, 1.0);
        /// Which way the pattern shifts.
        direction: Direction = Direction::Forward => "direction", "Direction", choice;
        /// Splits the pattern into bands (for plasmas, more bands of color).
        chunks: u32 = 1 => "chunks", "Chunks", int(1, 10), more;
        /// With more than one chunk, every band this many apart is left dark.
        skip: u32 = 2 => "skip", "Dark every", int(2, 10), more;
    }
}

/// The shape of each garland.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum GarlandShape {
    #[default]
    Straight,
    SmallSwags,
    Swags,
    DeepSwags,
    DoubleDips,
}

choices!(GarlandShape {
    "straight" => "Straight",
    "smallSwags" => "Small swags",
    "swags" => "Swags",
    "deepSwags" => "Deep swags",
    "doubleDips" => "Double dips",
});

/// Which way garlands stack up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum GarlandsDirection {
    #[default]
    Up,
    Down,
    Left,
    Right,
    UpThenDown,
    DownThenUp,
    LeftThenRight,
    RightThenLeft,
}

choices!(GarlandsDirection {
    "up" => "Up",
    "down" => "Down",
    "left" => "Left",
    "right" => "Right",
    "upThenDown" => "Up, then down",
    "downThenUp" => "Down, then up",
    "leftThenRight" => "Left, then right",
    "rightThenLeft" => "Right, then left",
});

effect_params! {
    /// Rows of garlands, one per row of pixels, sliding into place one after another until they
    /// fill the prop, `cycles` times over the effect. Colors run through the palette from the
    /// last row to the first.
    #[derive(Copy)]
    pub struct GarlandsParams {
        /// Straight rows, or swags of different depths.
        shape: GarlandShape = GarlandShape::Straight => "shape", "Garland", choice;
        /// How far apart the garlands start, as a share of the prop.
        spacing: f32 = 10.0 => "spacing", "Spacing", number(1.0, 100.0, 1.0, "%");
        /// Times the garlands stack up over the effect.
        cycles: f32 = 1.0 => "cycles", "Cycles", number(0.0, 20.0, 0.1);
        /// Which way they stack.
        direction: GarlandsDirection = GarlandsDirection::Up => "direction", "Stack direction", choice;
    }
}

effect_params! {
    /// Lines joining points that bounce around the prop, each line in the next palette color,
    /// with copies trailing behind.
    #[derive(Copy)]
    pub struct LinesParams {
        /// Number of lines.
        count: u32 = 2 => "count", "Lines", int(1, 20);
        /// Points in each line (more than two make a closed shape).
        points: u32 = 3 => "points", "Points", int(2, 6);
        /// Line thickness.
        thickness: u32 = 1 => "thickness", "Thickness", int(1, 10, "pixels");
        /// How far the points move each frame.
        speed: f32 = 1.0 => "speed", "Speed", number(0.0, 10.0, 0.1, "pixels a frame");
        /// Copies of each line left behind it.
        trails: u32 = 0 => "trails", "Trails", int(0, 10);
        /// Dim the trails, oldest the dimmest.
        fade_trails: bool = true => "fadeTrails", "Fade trails", toggle, more;
    }
}

/// The rules Life's cells live by: how many neighbors bring a cell to life (B) and keep it
/// alive (S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LifeRules {
    #[default]
    Classic,
    B35S236,
    Amoeba,
    Coagulations,
    B25678S5678,
}

choices!(LifeRules {
    "classic" => "Classic (B3/S23)",
    "b35S236" => "B35/S236",
    "amoeba" => "Amoeba (B357/S1358)",
    "coagulations" => "Coagulations (B378/S235678)",
    "b25678S5678" => "B25678/S5678",
});

effect_params! {
    /// Conway's Game of Life on the prop's grid: random cells in the palette colors to start,
    /// then a new generation every so often. The grid wraps around at its edges.
    #[derive(Copy)]
    pub struct LifeParams {
        /// How many cells are alive at the start (100 fills about half the grid).
        density: u32 = 50 => "density", "Starting cells", int(0, 100, "%");
        /// Which neighbors bring cells to life and keep them alive.
        rules: LifeRules = LifeRules::Classic => "rules", "Rules", choice;
        /// How often a new generation comes (10 is two a second; 20 or more, one every frame).
        speed: u32 = 10 => "speed", "Speed", int(1, 30);
    }
}

/// How the point a Tendril follows moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TendrilMovement {
    Random,
    Square,
    #[default]
    Circle,
    HorizontalZigZag,
    HorizontalZigZagReturn,
    VerticalZigZag,
    VerticalZigZagReturn,
    /// Back and forth across, as high as the music is loud.
    MusicLine,
    /// Around a circle as wide as the music is loud.
    MusicCircle,
    /// Held at one point (`manualX`, `manualY`).
    Manual,
}

choices!(TendrilMovement {
    "random" => "Random",
    "square" => "Around a square",
    "circle" => "Around a circle",
    "horizontalZigZag" => "Zig zag across",
    "horizontalZigZagReturn" => "Zig zag across and back",
    "verticalZigZag" => "Zig zag up",
    "verticalZigZagReturn" => "Zig zag up and back",
    "musicLine" => "Across, as high as the music",
    "musicCircle" => "Around a circle sized by the music",
    "manual" => "Held at a point",
});

effect_params! {
    /// Springy tendrils trailing after a point that moves around the prop, in the palette
    /// colors blended over the effect.
    #[derive(Copy)]
    pub struct TendrilParams {
        /// How the point the tendrils follow moves.
        movement: TendrilMovement = TendrilMovement::Circle => "movement", "Movement", choice;
        /// How far the point moves each step.
        movement_size: f32 = 10.0 => "movementSize", "Movement size", number(0.0, 20.0, 1.0);
        /// How thick the tendrils are.
        thickness: f32 = 3.0 => "thickness", "Thickness", number(1.0, 20.0, 1.0, "pixels");
        /// Tendrils following the point, each a little springier.
        tendrils: u32 = 1 => "tendrils", "Tendrils", int(1, 20);
        /// Joints in each tendril: longer tendrils trail further.
        length: u32 = 60 => "length", "Length", int(5, 100);
        /// How often the point moves (10 every frame, 9 every other, and so on).
        speed: u32 = 10 => "speed", "Speed", int(1, 10);
        /// How quickly the tendrils slow down.
        friction: u32 = 10 => "friction", "Friction", int(0, 20), more;
        /// How much of the movement of the joint before it each joint carries on.
        dampening: u32 = 10 => "dampening", "Dampening", int(0, 20), more;
        /// How stiff the tendrils are toward their ends.
        tension: u32 = 20 => "tension", "Tension", int(0, 39), more;
        /// Moves the path left (negative) or right, as a share of the prop.
        offset_x: f32 = 0.0 => "offsetX", "Offset across", number(-100.0, 100.0, 1.0, "%"), more;
        /// Moves the path down (negative) or up, as a share of the prop.
        offset_y: f32 = 0.0 => "offsetY", "Offset up", number(-100.0, 100.0, 1.0, "%"), more;
        /// Where a held point is, from the left.
        manual_x: f32 = 0.0 => "manualX", "Point across", number(0.0, 100.0, 1.0, "%"), more;
        /// Where a held point is, from the bottom.
        manual_y: f32 = 0.0 => "manualY", "Point up", number(0.0, 100.0, 1.0, "%"), more;
    }
}

/// How text moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TextMovement {
    #[default]
    None,
    Left,
    Right,
    Up,
    Down,
    UpLeft,
    DownLeft,
    UpRight,
    DownRight,
    /// From the start position to the end position over the effect.
    Vector,
    /// Left, bobbing up and down.
    Wavy,
    /// Left, then back right.
    LeftRight,
    /// Up, then back down.
    UpDown,
}

choices!(TextMovement {
    "none" => "Still",
    "left" => "Left",
    "right" => "Right",
    "up" => "Up",
    "down" => "Down",
    "upLeft" => "Up and left",
    "downLeft" => "Down and left",
    "upRight" => "Up and right",
    "downRight" => "Down and right",
    "vector" => "From start to end",
    "wavy" => "Left, bobbing",
    "leftRight" => "Back and forth",
    "upDown" => "Up and back down",
});

/// How text's letters are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TextOrientation {
    #[default]
    Across,
    /// One letter under the other, the first at the top.
    StackedDown,
    /// One letter above the other, the first at the bottom.
    StackedUp,
}

choices!(TextOrientation {
    "across" => "Across",
    "stackedDown" => "Stacked, reading down",
    "stackedUp" => "Stacked, reading up",
});

/// A countdown shown in place of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TextCountdown {
    #[default]
    None,
    /// The text is a number of seconds, counting down to 0.
    Seconds,
    /// The text is a number of seconds, shown as minutes and seconds counting down.
    MinutesSeconds,
}

choices!(TextCountdown {
    "none" => "None",
    "seconds" => "Seconds",
    "minutesSeconds" => "Minutes and seconds",
});

/// What a VU Meter draws (xLights' VU Meter types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum VuMeterType {
    #[default]
    Spectrogram,
    SpectrogramPeak,
    SpectrogramLine,
    SpectrogramCircleLine,
    VolumeBars,
    Waveform,
    On,
    ColorOn,
    DominantFrequencyColor,
    DominantFrequencyColorGradient,
    IntensityWave,
    Pulse,
    LevelBar,
    LevelRandomBar,
    LevelColor,
    LevelPulse,
    LevelPulseColor,
    LevelJump,
    LevelJump100,
    LevelShape,
    TimingEventBar,
    TimingEventBarBounce,
    TimingEventRandomBar,
    TimingEventBars,
    TimingEventSpike,
    TimingEventSweep,
    TimingEventSweep2,
    TimingEventTimedSweep,
    TimingEventTimedSweep2,
    TimingEventAlternateTimedSweep,
    TimingEventAlternateTimedSweep2,
    TimingEventChaseFromMiddle,
    TimingEventChaseToMiddle,
    TimingEventColor,
    TimingEventJump,
    TimingEventJump100,
    TimingEventPulse,
    TimingEventPulseColor,
    NoteOn,
    NoteLevelPulse,
    NoteLevelJump,
    NoteLevelJump100,
    NoteLevelBar,
    NoteLevelRandomBar,
}

choices!(VuMeterType {
    "spectrogram" => "Spectrogram",
    "spectrogramPeak" => "Spectrogram with peaks",
    "spectrogramLine" => "Spectrogram line",
    "spectrogramCircleLine" => "Spectrogram circle",
    "volumeBars" => "Volume bars",
    "waveform" => "Waveform",
    "on" => "On with the level",
    "colorOn" => "Color by level",
    "dominantFrequencyColor" => "Color by the loudest note",
    "dominantFrequencyColorGradient" => "Blend by the loudest note",
    "intensityWave" => "Intensity wave",
    "pulse" => "Pulse on marks",
    "levelBar" => "Level bar",
    "levelRandomBar" => "Level random bar",
    "levelColor" => "Level color",
    "levelPulse" => "Level pulse",
    "levelPulseColor" => "Level pulse color",
    "levelJump" => "Level jump",
    "levelJump100" => "Level jump to the top",
    "levelShape" => "Level shape",
    "timingEventBar" => "Bar on marks",
    "timingEventBarBounce" => "Bouncing bar on marks",
    "timingEventRandomBar" => "Random bar on marks",
    "timingEventBars" => "Bars on marks",
    "timingEventSpike" => "Spike on marks",
    "timingEventSweep" => "Sweep on marks",
    "timingEventSweep2" => "Sweep on marks 2",
    "timingEventTimedSweep" => "Timed sweep",
    "timingEventTimedSweep2" => "Timed sweep 2",
    "timingEventAlternateTimedSweep" => "Alternating timed sweep",
    "timingEventAlternateTimedSweep2" => "Alternating timed sweep 2",
    "timingEventChaseFromMiddle" => "Timed chase from the middle",
    "timingEventChaseToMiddle" => "Timed chase to the middle",
    "timingEventColor" => "Color on marks",
    "timingEventJump" => "Jump on marks",
    "timingEventJump100" => "Jump to the top on marks",
    "timingEventPulse" => "Pulse up on marks",
    "timingEventPulseColor" => "Pulse color on marks",
    "noteOn" => "Notes on",
    "noteLevelPulse" => "Note level pulse",
    "noteLevelJump" => "Note level jump",
    "noteLevelJump100" => "Note level jump to the top",
    "noteLevelBar" => "Note level bar",
    "noteLevelRandomBar" => "Note level random bar",
});

impl VuMeterType {
    /// Whether the type follows a timing track's marks.
    pub fn uses_marks(self) -> bool {
        use VuMeterType as T;
        matches!(
            self,
            T::Pulse
                | T::TimingEventBar
                | T::TimingEventBarBounce
                | T::TimingEventRandomBar
                | T::TimingEventBars
                | T::TimingEventSpike
                | T::TimingEventSweep
                | T::TimingEventSweep2
                | T::TimingEventTimedSweep
                | T::TimingEventTimedSweep2
                | T::TimingEventAlternateTimedSweep
                | T::TimingEventAlternateTimedSweep2
                | T::TimingEventChaseFromMiddle
                | T::TimingEventChaseToMiddle
                | T::TimingEventColor
                | T::TimingEventJump
                | T::TimingEventJump100
                | T::TimingEventPulse
                | T::TimingEventPulseColor
        )
    }
}

/// The shape a VU Meter's Level Shape grows and shrinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum VuMeterShape {
    #[default]
    Circle,
    FilledCircle,
    Square,
    FilledSquare,
    Diamond,
    FilledDiamond,
    Star,
    FilledStar,
    Tree,
    FilledTree,
    Crucifix,
    FilledCrucifix,
    Present,
    FilledPresent,
    CandyCane,
    Snowflake,
    Heart,
    FilledHeart,
}

choices!(VuMeterShape {
    "circle" => "Circle",
    "filledCircle" => "Filled circle",
    "square" => "Square",
    "filledSquare" => "Filled square",
    "diamond" => "Diamond",
    "filledDiamond" => "Filled diamond",
    "star" => "Star",
    "filledStar" => "Filled star",
    "tree" => "Tree",
    "filledTree" => "Filled tree",
    "crucifix" => "Cross",
    "filledCrucifix" => "Filled cross",
    "present" => "Present",
    "filledPresent" => "Filled present",
    "candyCane" => "Candy cane",
    "snowflake" => "Snowflake",
    "heart" => "Heart",
    "filledHeart" => "Filled heart",
});

effect_params! {
    /// xLights' VU Meter: bars, levels, shapes, and flashes worked out from the music's loudness
    /// and spectrum each frame, or from a timing track's marks. The palette colors the bars from
    /// bottom to top (the last color marks a spectrogram's peaks).
    pub struct VuMeterParams {
        /// What it draws.
        meter: VuMeterType = VuMeterType::Spectrogram => "meter", "Type", choice;
        /// Bars across (spectrograms, volume bars, waveforms), or frames a pulse or jump lasts.
        bars: u32 = 6 => "bars", "Bars", int(1, 100);
        /// The level (0-100) that triggers the level and note types; how long spectrogram peaks
        /// hold; a Level Shape's size.
        sensitivity: u32 = 70 => "sensitivity", "Sensitivity", int(0, 100);
        /// Boosts (or cuts) the music's level.
        gain: f32 = 0.0 => "gain", "Gain", number(-100.0, 100.0, 1.0, "%");
        /// The marks the timing types follow.
        timing_track: Option<TimingTrackId> = None => "timingTrack", "Timing track", timing_track;
        /// The shape a Level Shape draws.
        shape: VuMeterShape = VuMeterShape::Circle => "shape", "Shape", choice, more;
        /// Let bars and shapes fall back slowly.
        slow_falls: bool = true => "slowFalls", "Slow falls", toggle, more;
        /// The lowest MIDI note the spectrogram and note types read.
        start_note: u32 = 36 => "startNote", "Lowest note", int(0, 126), more;
        /// The highest.
        end_note: u32 = 84 => "endNote", "Highest note", int(0, 126), more;
        /// Spread the low notes wider than the high ones.
        log_x: bool = false => "logX", "Logarithmic across", toggle, more;
        /// Moves the drawing left (-) or right (+), as a share of the prop.
        x_offset: f32 = 0.0 => "xOffset", "Offset across", number(-100.0, 100.0, 1.0, "%"), more;
        /// Moves the drawing down (-) or up (+).
        y_offset: f32 = 0.0 => "yOffset", "Offset up", number(-100.0, 100.0, 1.0, "%"), more;
        /// Only marks with this label (one word of it) count.
        filter: String = String::new() => "filter", "Only marks labeled", text, more;
    }
}

effect_params! {
    /// Text in PixelFlow's built-in pixel font, still or moving, in the first palette color (or
    /// a palette color per letter or word when the palette has several).
    pub struct TextParams {
        /// What it says (a new line starts with \n).
        text: String = "Hello".to_string() => "text", "Text", text;
        /// Still, or moving.
        movement: TextMovement = TextMovement::None => "movement", "Movement", choice;
        /// How fast it moves.
        speed: u32 = 10 => "speed", "Speed", int(0, 100);
        /// How tall the letters are (8 is the font's own size; others scale it).
        size: u32 = 8 => "size", "Letter height", int(4, 100, "pixels");
        /// Letters across, or stacked one above another.
        orientation: TextOrientation = TextOrientation::Across => "orientation", "Letters", choice;
        /// Stop when the text reaches the middle.
        to_center: bool = false => "toCenter", "Stop in the middle", toggle, more;
        /// Cross the prop once instead of again and again.
        no_repeat: bool = false => "noRepeat", "Cross once", toggle, more;
        /// Where the text sits (or starts, moving from start to end), left (-) or right (+) of the middle.
        start_x: f32 = 0.0 => "startX", "Start across", number(-200.0, 200.0, 1.0), more;
        /// Where the text sits (or starts), below (-) or above (+) the middle.
        start_y: f32 = 0.0 => "startY", "Start up", number(-200.0, 200.0, 1.0), more;
        /// Where text moving from start to end ends, left (-) or right (+) of the middle.
        end_x: f32 = 0.0 => "endX", "End across", number(-200.0, 200.0, 1.0), more;
        /// Where text moving from start to end ends, below (-) or above (+) the middle.
        end_y: f32 = 0.0 => "endY", "End up", number(-200.0, 200.0, 1.0), more;
        /// Positions in pixels instead of a share of the prop.
        pixel_offsets: bool = false => "pixelOffsets", "Positions in pixels", toggle, more;
        /// A palette color per word instead of per letter.
        color_per_word: bool = false => "colorPerWord", "A color per word", toggle, more;
        /// Show a countdown from the number in the text instead.
        countdown: TextCountdown = TextCountdown::None => "countdown", "Countdown", choice, more;
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
    fn curves_take_the_place_of_their_settings_as_the_effect_plays() {
        let mut e = Effect::new(EffectKind::Chase, 1000, 3000);
        assert!(matches!(e.at(1500), Cow::Borrowed(_)), "no curves, no copy");
        e.curves.insert("speed".into(), Curve::ramp(0.0, 10.0));
        e.curves.insert("bands".into(), Curve::ramp(1.0, 4.0));
        e.curves.insert("sparkles".into(), Curve::ramp(0.0, 300.0));
        let now = e.at(1500);
        let EffectParams::Chase(p) = &now.params else {
            unreachable!()
        };
        assert_eq!((p.speed, p.bands), (2.5, 2), "a whole-number setting is rounded");
        assert_eq!(now.sparkles, 75);
        assert!(now.curves.is_empty());
        assert_eq!(e.at(2999).sparkles, 200, "kept to the most sparkles");
        assert_eq!(e.progress(0), 0.0);
        assert_eq!(e.progress(5000), 1.0);
        // Curves only fit number settings.
        assert!(Effect::curve_range(EffectKind::Chase, "speed").is_some());
        assert_eq!(Effect::curve_range(EffectKind::Chase, "direction"), None);
        assert_eq!(Effect::curve_range(EffectKind::Faces, "face"), None);
        assert!(Effect::curve_range(EffectKind::Off, "blur").is_some_and(|r| r.whole && r.max == 14.0));
        let mut params = EffectParams::default_for(EffectKind::Spiral);
        assert!(params.set_number("twist", -3.5));
        assert_eq!(params.number("twist"), Some(-3.5));
        assert_eq!(params.number("count"), Some(3.0));
        assert_eq!(params.number("direction"), None);
        assert!(!params.set_number("direction", 1.0));
        assert!(!params.set_number("nothing", 1.0));
    }

    #[test]
    fn music_sparkles_and_curves_follow_the_peak() {
        let mut e = Effect::new(EffectKind::Twinkle, 0, 1000);
        e.sparkles = 100;
        e.music_sparkles = true;
        let peak = |frame: u64| if frame < 4 { 0.25 } else { 0.999 };
        let inputs = CurveInputs {
            peak: Some(&peak),
            frame_ms: 25,
            tracks: &[],
        };
        assert_eq!(
            e.at_with(50, &inputs).sparkles,
            25,
            "a quarter as loud, a quarter as many"
        );
        assert_eq!(e.at_with(100, &inputs).sparkles, 99, "rounded down, as xLights");
        assert_eq!(e.at(50).sparkles, 100, "without the music, the steady count");
        e.curves
            .insert("density".into(), Curve::music(0.0, 1.0, 0.0, false));
        let EffectParams::Twinkle(p) = &e.at_with(50, &inputs).params else {
            unreachable!()
        };
        assert_eq!(p.density, 0.25);
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["musicSparkles"], true);
        assert!(serde_json::to_value(Effect::new(EffectKind::On, 0, 1)).unwrap()["musicSparkles"].is_null());
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
