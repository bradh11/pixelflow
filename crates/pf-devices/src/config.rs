//! A vendor-neutral view of a controller's configuration.

use pf_model::ColorOrder;
use serde::Serialize;

/// What a device is set up to receive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DeviceInput {
    Ddp,
    Sacn {
        start_universe: u16,
        channels_per_universe: u16,
        universe_count: u16,
    },
    /// Something PixelFlow can't send to yet (Art-Net, ZCPP, FPP sync modes…).
    Unsupported {
        description: String,
    },
}

/// One pixel string on a port (or on a smart receiver attached to it), in wiring order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringConfig {
    pub name: Option<String>,
    pub pixels: u32,
    pub color_order: ColorOrder,
    pub null_pixels: u32,
    pub reverse: bool,
    /// Percent, 0–100.
    pub brightness: u8,
    pub gamma: f32,
    /// Smart receiver (1 = A, 2 = B, …), if the string hangs off one.
    pub smart_receiver: Option<u8>,
}

/// A physical output port.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortConfig {
    /// Port number as printed on the controller (1-based).
    pub number: u16,
    pub strings: Vec<StringConfig>,
}

/// Another controller this device sends data to (FPP's channel outputs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub address: String,
    pub description: String,
    /// "DDP", "sACN unicast", "sACN multicast", "Art-Net", …
    pub protocol: String,
    pub channels: u32,
}

/// A device's configuration as PixelFlow understands it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfig {
    pub input: DeviceInput,
    pub ports: Vec<PortConfig>,
    pub destinations: Vec<Destination>,
    /// Anything that couldn't be read exactly, in plain language.
    pub notes: Vec<String>,
}

/// Maps a color-order name like "GRB" or "RGBW" to PixelFlow's, or `None` if unsupported.
pub(crate) fn color_order_from_name(name: &str) -> Option<ColorOrder> {
    match name.trim().to_ascii_uppercase().as_str() {
        "RGB" => Some(ColorOrder::Rgb),
        "RBG" => Some(ColorOrder::Rbg),
        "GRB" => Some(ColorOrder::Grb),
        "GBR" => Some(ColorOrder::Gbr),
        "BRG" => Some(ColorOrder::Brg),
        "BGR" => Some(ColorOrder::Bgr),
        "RGBW" => Some(ColorOrder::Rgbw),
        "GRBW" => Some(ColorOrder::Grbw),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_order_names() {
        assert_eq!(color_order_from_name("grb"), Some(ColorOrder::Grb));
        assert_eq!(color_order_from_name("RGBW"), Some(ColorOrder::Rgbw));
        assert_eq!(color_order_from_name("WRGB"), None);
    }

    #[test]
    fn input_serializes_with_a_type_tag() {
        let input = DeviceInput::Sacn {
            start_universe: 1,
            channels_per_universe: 510,
            universe_count: 4,
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["type"], "sacn");
        assert_eq!(json["startUniverse"], 1);
    }
}
