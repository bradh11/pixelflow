//! WLED over its JSON API — read-only (`/json/info` and `/json/cfg`; never `/wsec.json`).

use crate::config::{DeviceConfig, DeviceInput, PortConfig, StringConfig, bounded_nulls, bounded_pixels};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::http::Http;
use pf_model::ColorOrder;
use serde_json::Value;

/// WLED bus types (`wled00/const.h`) that are RGBW: UCS8904, SK6812 RGBW, TM1814.
const RGBW_BUS_TYPES: [i64; 3] = [29, 30, 31];

/// Pixel buses with extra white channels (white-only, white + amber, RGB + CCT/WWA) that need
/// 5 or 6 channels per pixel, which PixelFlow can't send yet.
const EXTRA_WHITE_BUS_TYPES: [i64; 6] = [18, 19, 21, 28, 32, 34];

/// Pixel buses: one-wire types 16–39 and two-wire types 48–63.
fn is_pixel_bus(bus_type: i64) -> bool {
    (16..=39).contains(&bus_type) || (48..=63).contains(&bus_type)
}

fn get_json(http: &dyn Http, host: &str, path: &str) -> Result<Value, DeviceError> {
    let body = http.get(host, path)?;
    serde_json::from_str(&body).map_err(|e| DeviceError::bad(host, path, e.to_string()))
}

/// Identifies a WLED from `/json/info` (requires `brand == "WLED"`).
pub fn probe(http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
    let info = get_json(http, host, "/json/info")?;
    if info["brand"].as_str() != Some("WLED") {
        return Err(DeviceError::Unrecognized(host.to_string()));
    }
    let text = |key: &str| info[key].as_str().unwrap_or("").to_string();
    Ok(Device {
        address: host.to_string(),
        kind: DeviceKind::Wled,
        name: Some(text("name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("WLED {host}")),
        model: format!("WLED ({})", text("arch")),
        firmware: format!("WLED {}", text("ver")),
        mode: Some(text("lm")).filter(|m| !m.is_empty()),
        found_by: Vec::new(),
    })
}

/// WLED color order (low nibble of `order`): 0 GRB, 1 RGB, 2 BRG, 3 RBG, 4 BGR, 5 GBR.
fn color_order(order: i64, rgbw: bool) -> Option<ColorOrder> {
    Some(match (order & 0x0F, rgbw) {
        (0, false) => ColorOrder::Grb,
        (1, false) => ColorOrder::Rgb,
        (2, false) => ColorOrder::Brg,
        (3, false) => ColorOrder::Rbg,
        (4, false) => ColorOrder::Bgr,
        (5, false) => ColorOrder::Gbr,
        (0, true) => ColorOrder::Grbw,
        (1, true) => ColorOrder::Rgbw,
        _ => return None,
    })
}

/// Reads LED outputs (`hw.led.ins`) and realtime receive settings (`if.live`).
pub fn read_config(http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
    let cfg = get_json(http, host, "/json/cfg")?;
    let mut notes = Vec::new();
    let mut ports = Vec::new();
    for (i, bus) in cfg["hw"]["led"]["ins"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let raw_pixels = bus["len"].as_i64().unwrap_or(0);
        if raw_pixels <= 0 {
            continue;
        }
        let number =
            u16::try_from(i + 1).map_err(|_| DeviceError::bad(host, "/json/cfg", "too many LED outputs"))?;
        let bus_type = bus["type"].as_i64().unwrap_or(22);
        if EXTRA_WHITE_BUS_TYPES.contains(&bus_type) {
            notes.push(format!(
                "Output {number} uses LEDs with extra white channels that PixelFlow doesn't support yet; it was skipped."
            ));
            continue;
        }
        if !is_pixel_bus(bus_type) {
            notes.push(format!(
                "Output {number} isn't a pixel output (type {bus_type}); it was skipped."
            ));
            continue;
        }
        let label = format!("Output {number}");
        let Some(pixels) = bounded_pixels(&label, raw_pixels, &mut notes) else {
            continue;
        };
        let rgbw = RGBW_BUS_TYPES.contains(&bus_type);
        let order = bus["order"].as_i64().unwrap_or(0);
        let color_order = color_order(order, rgbw).unwrap_or_else(|| {
            notes.push(format!(
                "Output {number}: color order code {order} isn't supported yet; using GRB."
            ));
            ColorOrder::Grb
        });
        ports.push(PortConfig {
            number,
            strings: vec![StringConfig {
                name: None,
                pixels,
                color_order,
                null_pixels: bounded_nulls(&label, bus["skip"].as_i64().unwrap_or(0), &mut notes),
                reverse: bus["rev"].as_bool().unwrap_or(false),
                brightness: 100,
                gamma: 1.0,
                smart_receiver: None,
            }],
            max_pixels: None,
        });
    }
    if cfg["if"]["live"]["en"].as_bool() == Some(false) {
        notes.push("Realtime receive is turned off in WLED (Config → Sync Interfaces); turn it on to see PixelFlow's output.".to_string());
    }
    Ok(DeviceConfig {
        input: DeviceInput::Ddp,
        ports,
        destinations: Vec::new(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_orders_including_rgbw() {
        assert_eq!(color_order(0, false), Some(ColorOrder::Grb));
        assert_eq!(color_order(1, true), Some(ColorOrder::Rgbw));
        assert_eq!(color_order(0x21, false), Some(ColorOrder::Rgb));
        assert_eq!(color_order(4, true), None);
    }
}
