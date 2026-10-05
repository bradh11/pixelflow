//! WLED over its JSON API — read-only (`/json/info` and `/json/cfg`; never `/wsec.json`).

use crate::config::{DeviceConfig, DeviceInput, PortConfig, StringConfig};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::http::Http;
use pf_model::ColorOrder;
use serde_json::Value;

/// WLED bus types that are RGBW (SK6812 RGBW, TM1814).
const RGBW_BUS_TYPES: [i64; 2] = [30, 31];

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
    let ports = cfg["hw"]["led"]["ins"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, bus)| {
            let pixels = bus["len"].as_i64().unwrap_or(0);
            if pixels <= 0 {
                return None;
            }
            let number = u16::try_from(i + 1).unwrap_or(0);
            let rgbw = RGBW_BUS_TYPES.contains(&bus["type"].as_i64().unwrap_or(22));
            let order = bus["order"].as_i64().unwrap_or(0);
            let color_order = color_order(order, rgbw).unwrap_or_else(|| {
                notes.push(format!(
                    "Output {number}: color order code {order} isn't supported yet; using GRB."
                ));
                ColorOrder::Grb
            });
            Some(PortConfig {
                number,
                strings: vec![StringConfig {
                    name: None,
                    pixels: u32::try_from(pixels).unwrap_or(0),
                    color_order,
                    null_pixels: u32::try_from(bus["skip"].as_i64().unwrap_or(0)).unwrap_or(0),
                    reverse: bus["rev"].as_bool().unwrap_or(false),
                    brightness: 100,
                    gamma: 1.0,
                    smart_receiver: None,
                }],
            })
        })
        .collect();
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
