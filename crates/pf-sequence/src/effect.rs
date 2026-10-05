//! Effects: what lights up, when, and how it mixes with what's underneath.

use crate::{EffectId, Rgb};
use serde::{Deserialize, Serialize};

/// The colors an effect draws with. An empty palette draws white.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// How an effect's colors combine with the layers below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
}

impl EffectKind {
    pub const ALL: [EffectKind; 14] = [
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
        }
    }
}

/// Settings for each kind of effect. Missing settings take their defaults, so
/// `{ "kind": "chase" }` is a complete chase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        }
    }
}

/// Spreads the palette across the prop instead of using one color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gradient {
    #[default]
    None,
    /// Left to right.
    Horizontal,
    /// Bottom to top.
    Vertical,
}

/// Which way something moves along the pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Toward higher pixel numbers, rightward, or upward.
    #[default]
    Forward,
    Reverse,
}

/// Across (horizontal) or up and down (vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FadeDirection {
    #[default]
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeteorDirection {
    #[default]
    Down,
    Up,
    Left,
    Right,
}

/// Solid color (the first palette color), or the palette spread across the prop, with the
/// brightness going from `start_level` to `end_level` over the effect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OnParams {
    pub gradient: Gradient,
    pub start_level: f32,
    pub end_level: f32,
}

impl Default for OnParams {
    fn default() -> Self {
        Self {
            gradient: Gradient::None,
            start_level: 1.0,
            end_level: 1.0,
        }
    }
}

/// Black: hides the layers below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OffParams {}

/// Blends through the palette over the effect, `cycles` times; with a gradient, the colors
/// also spread across the prop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorWashParams {
    pub cycles: f32,
    pub gradient: Gradient,
}

impl Default for ColorWashParams {
    fn default() -> Self {
        Self {
            cycles: 1.0,
            gradient: Gradient::None,
        }
    }
}

/// The first palette color fading in (or out) over the effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FadeParams {
    pub direction: FadeDirection,
}

/// Bands of light moving along the pixels in wiring order (several bands make a marquee).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChaseParams {
    /// Trips along the whole prop per second.
    pub speed: f32,
    /// Each band's length, as a fraction of the space between bands (0–1).
    pub width: f32,
    /// Bands spread evenly along the prop; they take the palette colors in turn.
    pub bands: u32,
    pub direction: Direction,
    /// Go back and forth instead of wrapping around.
    pub bounce: bool,
}

impl Default for ChaseParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            width: 0.2,
            bands: 1,
            direction: Direction::Forward,
            bounce: false,
        }
    }
}

/// Stripes in the palette colors sliding across or up the prop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BarsParams {
    /// Bars visible at once.
    pub count: u32,
    /// Prop widths (or heights) moved per second.
    pub speed: f32,
    pub axis: Axis,
    pub direction: Direction,
}

impl Default for BarsParams {
    fn default() -> Self {
        Self {
            count: 4,
            speed: 0.5,
            axis: Axis::Vertical,
            direction: Direction::Forward,
        }
    }
}

/// A sine wave line across the prop, rolling sideways.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WaveParams {
    /// Waves across the prop.
    pub cycles: f32,
    /// Waves passing per second.
    pub speed: f32,
    /// Peak-to-peak height, as a fraction of the prop's height (0–1).
    pub height: f32,
    /// Line thickness, as a fraction of the prop's height.
    pub thickness: f32,
    pub direction: Direction,
}

impl Default for WaveParams {
    fn default() -> Self {
        Self {
            cycles: 1.0,
            speed: 1.0,
            height: 0.8,
            thickness: 0.2,
            direction: Direction::Forward,
        }
    }
}

/// Random pixels softly brightening and dimming.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TwinkleParams {
    /// Fraction of pixels lit at any moment (0–1).
    pub density: f32,
    /// Twinkles per second for each pixel.
    pub rate: f32,
}

impl Default for TwinkleParams {
    fn default() -> Self {
        Self {
            density: 0.3,
            rate: 1.0,
        }
    }
}

/// The whole prop flickering on and off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShimmerParams {
    /// Flickers per second.
    pub rate: f32,
    /// Fraction of each flicker spent on (0–1).
    pub duty: f32,
}

impl Default for ShimmerParams {
    fn default() -> Self {
        Self {
            rate: 10.0,
            duty: 0.5,
        }
    }
}

/// Random pixels flashing, a new set each flash.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StrobeParams {
    /// Flashes per second.
    pub rate: f32,
    /// Fraction of pixels in each flash (0–1).
    pub density: f32,
}

impl Default for StrobeParams {
    fn default() -> Self {
        Self {
            rate: 10.0,
            density: 0.2,
        }
    }
}

/// Diagonal stripes rotating around the prop (made for trees and matrices).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpiralParams {
    /// Stripes around the prop.
    pub count: u32,
    /// Turns per second.
    pub speed: f32,
    /// Fraction of each stripe's space that is lit (0–1).
    pub thickness: f32,
    /// How steeply the stripes slant (0 = straight up).
    pub twist: f32,
    pub direction: Direction,
}

impl Default for SpiralParams {
    fn default() -> Self {
        Self {
            count: 3,
            speed: 0.5,
            thickness: 0.5,
            twist: 1.0,
            direction: Direction::Forward,
        }
    }
}

/// Flames rising from the bottom of the prop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FireParams {
    /// How high the flames reach, as a fraction of the prop's height (0–1).
    pub height: f32,
    /// How often new flames flare up (0–1).
    pub sparks: f32,
}

impl Default for FireParams {
    fn default() -> Self {
        Self {
            height: 0.8,
            sparks: 0.6,
        }
    }
}

/// Streaks of light with fading tails.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MeteorsParams {
    /// Meteors on the prop at once.
    pub count: u32,
    /// Prop heights (or widths) travelled per second.
    pub speed: f32,
    /// Tail length, as a fraction of the prop's height (or width).
    pub length: f32,
    pub direction: MeteorDirection,
}

impl Default for MeteorsParams {
    fn default() -> Self {
        Self {
            count: 5,
            speed: 1.0,
            length: 0.25,
            direction: MeteorDirection::Down,
        }
    }
}

/// Rings spreading out from the center of the prop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RippleParams {
    /// How fast rings grow: center-to-corner distances per second.
    pub speed: f32,
    /// Distance between rings (center-to-corner = 1).
    pub spacing: f32,
    /// Ring thickness (center-to-corner = 1).
    pub thickness: f32,
}

impl Default for RippleParams {
    fn default() -> Self {
        Self {
            speed: 0.5,
            spacing: 0.4,
            thickness: 0.12,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
