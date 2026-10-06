//! Colors (effect palettes, singing-face features), stored as `#rrggbb` text.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// An 8-bit RGB color. In files it is written as `"#rrggbb"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);
    pub const RED: Rgb = Rgb::new(255, 0, 0);
    pub const GREEN: Rgb = Rgb::new(0, 255, 0);
    pub const BLUE: Rgb = Rgb::new(0, 0, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `rrggbb`, with or without a leading `#`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self::new(byte(0)?, byte(2)?, byte(4)?))
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgb::from_hex(&text).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "'{text}' is not a color; use six hex digits, like #ff8000"
            ))
        })
    }
}

/// Colors are `"#rrggbb"` text in JSON (see the `Serialize` impl above).
#[cfg(feature = "schema")]
impl schemars::JsonSchema for Rgb {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Rgb".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^#?[0-9a-fA-F]{6}$",
            "description": "A color as six hex digits, like \"#ff8000\"."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_round_trip_as_hex_text() {
        let orange = Rgb::new(255, 128, 0);
        assert_eq!(serde_json::to_string(&orange).unwrap(), "\"#ff8000\"");
        assert_eq!(serde_json::from_str::<Rgb>("\"#FF8000\"").unwrap(), orange);
        assert_eq!(serde_json::from_str::<Rgb>("\"ff8000\"").unwrap(), orange);
    }

    #[test]
    fn bad_colors_are_explained() {
        let err = serde_json::from_str::<Rgb>("\"red\"").unwrap_err().to_string();
        assert!(err.contains("'red' is not a color"), "{err}");
        assert_eq!(Rgb::from_hex("#ff80"), None);
        assert_eq!(Rgb::from_hex("#gg8000"), None);
        assert_eq!(Rgb::from_hex("#ff800é"), None);
    }
}
