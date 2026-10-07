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
    /// Pixels that take data. The controller's own null pixels are not counted.
    pub pixels: u32,
    pub color_order: ColorOrder,
    /// Null pixels the controller skips itself (no input channels, not in `pixels`). Shown for review;
    /// not the same as PixelFlow's `PortSlot::null_pixels`, which are dark pixels PixelFlow sends.
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
    /// Most RGB pixels the port drives as the board is set up, when known (Falcon). Each smart
    /// receiver on the port has this limit for its own output.
    pub max_pixels: Option<u32>,
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
    /// The first sequence channel (1-based) sent to this destination.
    pub start_channel: u32,
    /// The first universe, for sACN destinations.
    pub start_universe: Option<u16>,
    /// Channels in each universe, for sACN destinations (FPP's `channelCount`); `None` for DDP.
    pub universe_size: Option<u16>,
    /// DDP only: FPP sends "DDP Raw Channel Numbers", so each packet's offset is the absolute
    /// channel (this destination's first channel minus one). Otherwise ("DDP One Based") the
    /// offset starts at 0 for the controller.
    pub ddp_raw: bool,
    /// Several sACN ranges were merged into this entry but they aren't one back-to-back run of
    /// universes of the same size.
    pub uneven_universes: bool,
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

/// Shown when a controller's own start channels don't line up with the order strings are imported in.
pub(crate) const LAYOUT_NOTE: &str = "This controller's strings don't use one continuous block of channels. PixelFlow sends each string's data right after the previous one, so check the channel layout on the controller before running a show.";

/// One string's place in the controller's own channel map, in import order.
pub(crate) struct Placed {
    /// The device's start channel for the string, if it reported one (any base).
    pub start: Option<i64>,
    /// Pixels that take data on the string. The controller skips its own null pixels, so they are not
    /// counted here and use no input channels.
    pub pixels: u32,
    pub color_order: ColorOrder,
}

/// True when every string that follows another starts exactly where the previous one ends.
/// Compared relatively, so it doesn't matter whether the device counts channels from 0 or 1.
/// Pairs where either start channel is missing are not checked.
pub(crate) fn layout_is_contiguous(strings: &[Placed]) -> bool {
    strings
        .windows(2)
        .all(|pair| match (pair[0].start, pair[1].start) {
            (Some(a), Some(b)) => {
                b.saturating_sub(a)
                    == i64::from(pair[0].pixels) * i64::from(pair[0].color_order.channels_per_pixel())
            }
            _ => true,
        })
}

/// 5000000 -> "5,000,000".
pub(crate) fn with_commas(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// A usable pixel count, or `None` (with a note) when it is more than PixelFlow supports.
/// Zero or negative counts are an empty string and are skipped without a note.
pub(crate) fn bounded_pixels(label: &str, raw: i64, notes: &mut Vec<String>) -> Option<u32> {
    if raw <= 0 {
        return None;
    }
    match u32::try_from(raw).ok().filter(|p| *p <= pf_model::MAX_PROP_NODES) {
        Some(pixels) => Some(pixels),
        None => {
            notes.push(format!(
                "{label} reports {} pixels, more than PixelFlow supports on one string; it was skipped.",
                with_commas(raw)
            ));
            None
        }
    }
}

/// Null pixels, clamped to what PixelFlow supports (with a note when clamped). Negative is 0.
pub(crate) fn bounded_nulls(label: &str, raw: i64, notes: &mut Vec<String>) -> u32 {
    let max = pf_model::MAX_NULL_PIXELS;
    match u32::try_from(raw.max(0)).ok().filter(|n| *n <= max) {
        Some(nulls) => nulls,
        None => {
            notes.push(format!(
                "{label} reports {} null pixels; PixelFlow keeps at most {}.",
                with_commas(raw),
                with_commas(i64::from(max))
            ));
            max
        }
    }
}

/// A port number that fits `u16` and is at least 1, or `None` (with a note).
pub(crate) fn valid_port(raw: i64, notes: &mut Vec<String>) -> Option<u16> {
    let number = u16::try_from(raw).ok().filter(|n| *n > 0);
    if number.is_none() {
        notes.push(format!(
            "A string on port number {raw} was skipped; PixelFlow can't use that port number."
        ));
    }
    number
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(start: Option<i64>, pixels: u32) -> Placed {
        Placed {
            start,
            pixels,
            color_order: ColorOrder::Rgb,
        }
    }

    #[test]
    fn layout_check_is_relative_and_skips_missing_starts() {
        assert!(layout_is_contiguous(&[
            placed(Some(1), 10),
            placed(Some(31), 5),
            placed(Some(46), 1)
        ]));
        assert!(
            !layout_is_contiguous(&[placed(Some(0), 10), placed(Some(40), 5)]),
            "gap"
        );
        assert!(
            !layout_is_contiguous(&[placed(Some(0), 10), placed(Some(0), 5)]),
            "overlap"
        );
        assert!(layout_is_contiguous(&[
            placed(Some(0), 10),
            placed(None, 5),
            placed(Some(99), 1)
        ]));
    }

    #[test]
    fn bounds_helpers_note_what_they_change() {
        let mut notes = Vec::new();
        assert_eq!(bounded_pixels("Port 3", 5_000_000, &mut notes), None);
        assert_eq!(bounded_pixels("Port 3", 0, &mut notes), None);
        assert_eq!(bounded_pixels("Port 3", 50, &mut notes), Some(50));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("Port 3 reports 5,000,000 pixels"));
        assert_eq!(
            bounded_nulls("Port 3", 5_000, &mut notes),
            pf_model::MAX_NULL_PIXELS
        );
        assert_eq!(notes.len(), 2);
        assert_eq!(valid_port(0, &mut notes), None);
        assert_eq!(valid_port(70_000, &mut notes), None);
        assert_eq!(valid_port(4, &mut notes), Some(4));
    }

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
