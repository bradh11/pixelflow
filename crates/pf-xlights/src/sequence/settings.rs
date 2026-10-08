//! xLights settings strings (`KEY=VALUE,KEY=VALUE`), color palettes, and the second entity
//! decode xLights applies to names and labels.

use pf_sequence::Rgb;
use std::collections::HashMap;

/// Control-type prefixes xLights writes after `B_`/`C_`/`T_`/`E_` (used to split values that an
/// old xLights bug fused with the next key).
const CONTROL_TYPES: [&str; 13] = [
    "SLIDER_",
    "VALUECURVE_",
    "CHOICE_",
    "CHECKBOX_",
    "TEXTCTRL_",
    "SPINCTRL_",
    "TOGGLEBUTTON_",
    "FILEPICKER_",
    "0FILEPICKER_",
    "FONTPICKER_",
    "CUSTOM_",
    "NOTEBOOK_",
    "PANEL_",
];

/// Most settings read from one string (a real effect has well under 100).
const MAX_SETTINGS: usize = 2_000;

/// One effect's (or palette's) settings, keyed as written in the file (`E_SLIDER_Bars_BarCount`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    map: HashMap<String, String>,
}

/// Where an embedded `X_CONTROL_Name=` key starts inside `value` (not at its start), if any.
fn embedded_key(value: &str) -> Option<usize> {
    if !value.contains('=') {
        return None;
    }
    let bytes = value.as_bytes();
    (1..bytes.len().saturating_sub(2)).find(|&i| {
        matches!(bytes[i], b'B' | b'C' | b'T' | b'E') && bytes[i + 1] == b'_' && {
            let rest = &value[i + 2..];
            CONTROL_TYPES.iter().any(|t| {
                rest.strip_prefix(t).is_some_and(|name| {
                    let end = name.find('=').unwrap_or(0);
                    end > 0
                        && name[..end]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                })
            })
        }
    })
}

impl Settings {
    /// Parses a settings string the way xLights' `SettingsMap::Parse` does: split on commas,
    /// then at the first `=`; `&comma;` and `&amp;` in values are unescaped. A value with
    /// another key fused into it (an old xLights bug) is split back apart.
    pub fn parse(text: &str) -> Self {
        let mut map = HashMap::new();
        let mut pending: Vec<String> = Vec::new();
        let mut rest = text;
        loop {
            let token = if let Some(t) = pending.pop() {
                t
            } else if rest.is_empty() {
                break;
            } else {
                let (token, after) = rest.split_once(',').unwrap_or((rest, ""));
                rest = after;
                token.to_string()
            };
            if map.len() >= MAX_SETTINGS {
                break;
            }
            let (key, value) = token.split_once('=').unwrap_or((token.as_str(), ""));
            let mut value = value.to_string();
            if let Some(at) = embedded_key(&value) {
                pending.push(value[at..].to_string());
                value.truncate(at);
            }
            let value = value.replace("&comma;", ",").replace("&amp;", "&");
            if !key.is_empty() {
                map.entry(key.to_string()).or_insert(value);
            }
        }
        Self { map }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The text of `key`, or `default` (like `SettingsMap::Get`).
    pub fn text<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    /// A number (an empty or unparseable value counts as missing, like xLights' `GetFloat`).
    pub fn num(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(leading_number).filter(|v| v.is_finite())
    }

    pub fn num_or(&self, key: &str, default: f64) -> f64 {
        self.num(key).unwrap_or(default)
    }

    /// A checkbox: "1"/"T…" are on (like xLights' `GetBool`); missing is `default`.
    pub fn flag(&self, key: &str, default: bool) -> bool {
        match self.get(key) {
            None => default,
            Some(v) => v.starts_with('1') || v.starts_with('T'),
        }
    }

    /// Every key, in no particular order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    /// Sets `key` to `value`, replacing what was there.
    pub fn set(&mut self, key: &str, value: String) {
        self.map.insert(key.to_string(), value);
    }

    /// True when `key`'s value is an active value curve (the setting changes over the effect).
    pub fn curve_active(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| v.contains("Active=TRUE"))
    }
}

/// The leading number in `text`, the way C's `strtod` reads it (`"25 ms"` is 25).
pub fn leading_number(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let bytes = t.as_bytes();
    let mut end = 0;
    if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
        end += 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end == digits_start || (end == digits_start + 1 && bytes[digits_start] == b'.') {
        return None;
    }
    // An exponent, only when complete.
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut e = end + 1;
        if e < bytes.len() && (bytes[e] == b'-' || bytes[e] == b'+') {
            e += 1;
        }
        let exp_digits = e;
        while e < bytes.len() && bytes[e].is_ascii_digit() {
            e += 1;
        }
        if e > exp_digits {
            end = e;
        }
    }
    t[..end].parse().ok()
}

/// The second entity decode xLights applies to names and labels (`UnXmlSafe`): `&amp;`,
/// `&lt;`, `&gt;`, `&apos;`, `&quot;`, and `&#N;` character references. Unknown entities are
/// left as written.
pub fn unxml_safe(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "apos" => Some('\''),
                "quot" => Some('"'),
                _ => entity
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, end + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A palette color as xLights writes it: `#rrggbb` (or `0xrrggbb`).
fn parse_color(text: &str) -> Option<Rgb> {
    let t = text.trim();
    let hex = t
        .strip_prefix('#')
        .or_else(|| t.strip_prefix("0x"))
        .or_else(|| t.strip_prefix("0X"))?;
    Rgb::from_hex(hex.get(..6)?)
}

/// The colors of an xLights color palette, and what didn't come across exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPalette {
    /// The enabled colors, in palette order.
    pub colors: Vec<Rgb>,
    /// Enabled colors that change over the effect (color curves); their first color is used.
    pub color_curves: usize,
    /// Enabled colors that couldn't be read.
    pub unreadable: usize,
    /// The palette's brightness (`C_SLIDER_Brightness`, percent, 100 when missing).
    pub brightness: f64,
    /// Sparkles (`C_SLIDER_SparkleFrequency`, 0 = none, up to 200).
    pub sparkles: u32,
    /// The sparkles' color (`C_COLOURPICKERCTRL_SparklesColour`, white when missing).
    pub sparkle_color: Rgb,
    /// Sparkles that follow the music (`C_CHECKBOX_MusicSparkles`).
    pub music_sparkles: bool,
    /// The sparkles' value curve (`C_VALUECURVE_SparkleFrequency`), when active.
    pub sparkles_curve: Option<String>,
    /// Other color settings PixelFlow can't apply (hue/saturation/value shifts, sparkles...).
    pub extras: Vec<&'static str>,
}

/// No colors, at full brightness, without sparkles (their color white, xLights' default).
impl Default for ParsedPalette {
    fn default() -> Self {
        Self {
            colors: Vec::new(),
            color_curves: 0,
            unreadable: 0,
            brightness: 100.0,
            sparkles: 0,
            sparkle_color: Rgb::WHITE,
            music_sparkles: false,
            sparkles_curve: None,
            extras: Vec::new(),
        }
    }
}

/// Color settings that change how an effect looks, with the name people know them by.
const COLOR_EXTRAS: [(&str, &str, f64); 6] = [
    ("C_SLIDER_Color_HueAdjust", "hue shift", 0.0),
    ("C_SLIDER_Color_SaturationAdjust", "saturation shift", 0.0),
    ("C_SLIDER_Color_ValueAdjust", "brightness shift", 0.0),
    ("C_SLIDER_Contrast", "contrast", 0.0),
    ("C_SLIDER_Color_HueShift", "hue shift", 0.0),
    ("C_SLIDER_Saturation", "saturation", 0.0),
];

/// Reads a palette string (`C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1,...`): the colors
/// whose checkbox is on, in order. Missing palettes have no colors.
pub fn parse_palette(text: &str) -> ParsedPalette {
    let settings = Settings::parse(text);
    let mut palette = ParsedPalette {
        brightness: settings.num_or("C_SLIDER_Brightness", 100.0),
        // xLights reads the slider as a whole number (`GetInt`).
        sparkles: settings.num("C_SLIDER_SparkleFrequency").map_or(0, |v| {
            v.trunc().clamp(0.0, f64::from(pf_sequence::MAX_SPARKLES)) as u32
        }),
        sparkle_color: parse_color(settings.text("C_COLOURPICKERCTRL_SparklesColour", "#FFFFFF"))
            .unwrap_or(Rgb::WHITE),
        music_sparkles: settings.flag("C_CHECKBOX_MusicSparkles", false),
        ..ParsedPalette::default()
    };
    if settings.curve_active("C_VALUECURVE_SparkleFrequency") {
        palette.sparkles_curve = settings.get("C_VALUECURVE_SparkleFrequency").map(str::to_string);
    }
    if settings.curve_active("C_VALUECURVE_Brightness") {
        palette.extras.push("brightness curve");
    }
    for n in 1..=8 {
        if !settings.flag(&format!("C_CHECKBOX_Palette{n}"), false) {
            continue;
        }
        let value = settings.text(&format!("C_BUTTON_Palette{n}"), "");
        if value.contains("Active=TRUE") {
            // A color curve: `Values=x=0.000^c=#ff0000;x=1.000^c=#0000ff`; keep its first color.
            let first = value
                .split(['^', ';', '|'])
                .find_map(|part| part.strip_prefix("c=").and_then(parse_color));
            match first {
                Some(c) => {
                    palette.colors.push(c);
                    palette.color_curves += 1;
                }
                None => palette.unreadable += 1,
            }
        } else {
            match parse_color(value) {
                Some(c) => palette.colors.push(c),
                None => palette.unreadable += 1,
            }
        }
    }
    for (key, label, neutral) in COLOR_EXTRAS {
        let changed = settings.num(key).is_some_and(|v| (v - neutral).abs() > 1e-6)
            || settings.curve_active(&key.replace("SLIDER", "VALUECURVE"));
        if changed && !palette.extras.contains(&label) {
            palette.extras.push(label);
        }
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_split_on_commas_and_unescape_values() {
        let s = Settings::parse(
            "E_SLIDER_Bars_BarCount=3,E_TEXTCTRL_Text=a&comma;b &amp; c,E_CHECKBOX_On=1,Novalue,=x,E_X=1=2",
        );
        assert_eq!(s.num("E_SLIDER_Bars_BarCount"), Some(3.0));
        assert_eq!(s.get("E_TEXTCTRL_Text"), Some("a,b & c"));
        assert!(s.flag("E_CHECKBOX_On", false));
        assert_eq!(s.get("Novalue"), Some(""));
        assert_eq!(s.get("E_X"), Some("1=2"));
        assert!(!s.contains(""));
    }

    #[test]
    fn fused_keys_are_split_back_apart() {
        let s = Settings::parse("T_TEXTCTRL_Fadeout=0.50B_SLIDER_Blur=3,E_SLIDER_Speed=10");
        assert_eq!(s.get("T_TEXTCTRL_Fadeout"), Some("0.50"));
        assert_eq!(s.get("B_SLIDER_Blur"), Some("3"));
        assert_eq!(s.num("E_SLIDER_Speed"), Some(10.0));
    }

    #[test]
    fn numbers_read_like_strtod_and_flags_like_xlights() {
        assert_eq!(leading_number("25 ms"), Some(25.0));
        assert_eq!(leading_number(" -1.5e2x"), Some(-150.0));
        assert_eq!(leading_number("3e"), Some(3.0));
        assert_eq!(leading_number("ms"), None);
        assert_eq!(leading_number("."), None);
        let s = Settings::parse("A=True,B=0,C=,D=nan");
        assert!(s.flag("A", false) && !s.flag("B", true) && !s.flag("C", true) && s.flag("Z", true));
        assert_eq!(s.num("C"), None);
        assert_eq!(s.num("D"), None);
    }

    #[test]
    fn value_curves_are_detected() {
        let s = Settings::parse(
            "E_VALUECURVE_Bars_Cycles=Active=TRUE|Id=ID_VALUECURVE_Bars_Cycles|Type=Ramp|Min=0.00|Max=300.00|,E_VALUECURVE_X=Active=FALSE|",
        );
        assert!(s.curve_active("E_VALUECURVE_Bars_Cycles"));
        assert!(!s.curve_active("E_VALUECURVE_X") && !s.curve_active("E_VALUECURVE_Y"));
    }

    #[test]
    fn names_get_a_second_entity_decode() {
        assert_eq!(unxml_safe("Tom &amp;amp; Jerry"), "Tom &amp; Jerry");
        assert_eq!(
            unxml_safe("A &amp; B &lt;3 &quot;x&quot; &apos;y&apos;"),
            "A & B <3 \"x\" 'y'"
        );
        assert_eq!(unxml_safe("tab&#9;nl&#10;hex&#x41;"), "tab\tnl\nhexA");
        assert_eq!(
            unxml_safe("R&D & more &bogus; &#xZZ; &"),
            "R&D & more &bogus; &#xZZ; &"
        );
        assert_eq!(unxml_safe("plain"), "plain");
    }

    #[test]
    fn palettes_keep_enabled_colors_in_order() {
        let p = parse_palette(
            "C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1,C_BUTTON_Palette2=#00FF00,C_CHECKBOX_Palette2=0,\
             C_BUTTON_Palette3=#0000FF,C_CHECKBOX_Palette3=1,C_BUTTON_Palette4=#FFFFFF,\
             C_BUTTON_Palette5=oops,C_CHECKBOX_Palette5=1,C_SLIDER_Brightness=50",
        );
        assert_eq!(p.colors, vec![Rgb::RED, Rgb::BLUE]);
        assert_eq!(p.unreadable, 1);
        assert_eq!(p.brightness, 50.0);
        assert!(p.extras.is_empty());
    }

    #[test]
    fn color_curves_use_their_first_color_and_extras_are_named() {
        let p = parse_palette(
            "C_BUTTON_Palette1=Active=TRUE|Id=ID_BUTTON_Palette1|Values=x=0.000^c=#ff8000;x=1.000^c=#0000ff|,\
             C_CHECKBOX_Palette1=1,C_SLIDER_SparkleFrequency=20,C_SLIDER_Color_HueAdjust=0,C_SLIDER_Contrast=5",
        );
        assert_eq!(p.colors, vec![Rgb::new(255, 128, 0)]);
        assert_eq!(p.color_curves, 1);
        assert_eq!(p.extras, vec!["contrast"]);
        assert_eq!(parse_palette("").colors, vec![]);
        assert_eq!(parse_palette("").brightness, 100.0);
    }

    #[test]
    fn sparkles_are_read_with_their_color() {
        let p = parse_palette(
            "C_SLIDER_SparkleFrequency=54,C_COLOURPICKERCTRL_SparklesColour=#00FF00,C_CHECKBOX_MusicSparkles=1",
        );
        assert_eq!(
            (p.sparkles, p.sparkle_color, p.music_sparkles),
            (54, Rgb::GREEN, true)
        );
        assert!(p.extras.is_empty());
        let none = parse_palette("C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1");
        assert_eq!(
            (none.sparkles, none.sparkle_color, none.music_sparkles),
            (0, Rgb::WHITE, false)
        );
        assert_eq!(parse_palette("C_SLIDER_SparkleFrequency=999").sparkles, 200);
        let curve = parse_palette(
            "C_SLIDER_SparkleFrequency=10,C_VALUECURVE_SparkleFrequency=Active=TRUE|Type=Ramp|",
        );
        assert!(curve.extras.is_empty());
        assert_eq!(curve.sparkles_curve.as_deref(), Some("Active=TRUE|Type=Ramp|"));
    }
}
