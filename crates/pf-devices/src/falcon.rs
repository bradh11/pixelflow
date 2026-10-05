//! Falcon pixel controllers (F16V4/F48V4, F16V5/F48V5/F32V5) — read-only.
//!
//! Identity comes from `GET /status.xml`; configuration from the JSON API at `POST /api` with
//! *query* requests only (`"T":"Q"`), following xLights' `Falcon.cpp`. The status reply also
//! carries Wi-Fi fields (`WS`, `WP`, `CP`); they are never read or kept.

use crate::config::{DeviceConfig, DeviceInput, PortConfig, StringConfig};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::http::Http;
use pf_model::ColorOrder;
use serde_json::Value;
use std::collections::BTreeMap;

/// Most `SP` pages to read before giving up (each holds a few strings).
const MAX_PAGES: u32 = 64;

/// Model name for the `p` product code in `/status.xml`.
fn model_for_product(code: u32) -> Option<&'static str> {
    Some(match code {
        1..=3 => "F16v2",
        4 => "F4v2",
        5 => "F16v3",
        6 => "F4v3",
        7 => "F48",
        128 => "F16v4",
        129 => "F48v4",
        130 => "F16v5",
        131 => "F48v5",
        132 => "F32v5",
        _ => return None,
    })
}

struct Status {
    name: String,
    firmware: String,
    product: u32,
}

fn read_status_xml(http: &dyn Http, host: &str) -> Result<Status, DeviceError> {
    let body = http.get(host, "/status.xml")?;
    let doc = roxmltree::Document::parse(&body)
        .map_err(|e| DeviceError::bad(host, "/status.xml", e.to_string()))?;
    let field = |name: &str| {
        doc.root_element()
            .children()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
            .unwrap_or_default()
    };
    let firmware = [field("fv"), field("v")]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    Ok(Status {
        name: field("n"),
        firmware,
        product: field("p").parse().unwrap_or(0),
    })
}

/// One page of a JSON API query: the payload `P` and whether it was the final page.
fn query(http: &dyn Http, host: &str, method: &str, batch: u32) -> Result<(Value, bool), DeviceError> {
    let body = format!(r#"{{"T":"Q","M":"{method}","B":{batch},"E":0,"I":0,"P":{{}}}}"#);
    let text = http.post_json(host, "/api", &body)?;
    let reply: Value =
        serde_json::from_str(&text).map_err(|e| DeviceError::bad(host, "/api", e.to_string()))?;
    if reply["R"].as_i64() != Some(200) {
        return Err(DeviceError::bad(
            host,
            "/api",
            format!("{method} query was refused ({})", reply["R"]),
        ));
    }
    Ok((reply["P"].clone(), reply["F"].as_i64() == Some(1)))
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// Identifies a Falcon from `/status.xml`, adding the name and firmware string from the JSON
/// status on V4/V5 boards.
pub fn probe(http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
    let status = read_status_xml(http, host)?;
    let mut model = model_for_product(status.product).unwrap_or("Falcon").to_string();
    let (mut name, mut firmware) = (status.name, status.firmware);
    if status.product >= 128
        && let Ok((p, _)) = query(http, host, "ST", 0)
    {
        // Read only the identity fields; Wi-Fi fields in the same payload are ignored.
        if let Some(n) = p["N"].as_str().filter(|s| !s.is_empty()) {
            name = n.to_string();
        }
        if let Some(v) = p["V"].as_str().filter(|s| !s.is_empty()) {
            firmware = v.to_string();
        }
        if let Some(ports) = p["BR"].as_i64().filter(|b| *b > 0) {
            let generation = if status.product >= 130 { 5 } else { 4 };
            model = format!("F{ports}v{generation}");
        }
    }
    Ok(Device {
        address: host.to_string(),
        kind: DeviceKind::Falcon,
        name: if name.is_empty() {
            format!("Falcon {host}")
        } else {
            name
        },
        model,
        firmware,
        mode: None,
        found_by: Vec::new(),
    })
}

fn mode_name(code: i64) -> &'static str {
    match code {
        0 => "E1.31/Art-Net",
        1 => "ZCPP",
        2 => "DDP",
        3 => "FPP remote",
        4 => "FPP master",
        5 => "FPP player",
        _ => "unknown",
    }
}

/// Falcon color-order codes: 0–5 are 3-channel orders; +6 means white first (WRGB…).
fn color_order(code: i64) -> Option<ColorOrder> {
    Some(match code {
        0 => ColorOrder::Rgb,
        1 => ColorOrder::Rbg,
        2 => ColorOrder::Grb,
        3 => ColorOrder::Gbr,
        4 => ColorOrder::Brg,
        5 => ColorOrder::Bgr,
        _ => return None,
    })
}

/// Reads the controller mode, input universes, and every string port (V4/V5 JSON API).
pub fn read_config(http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
    let mut notes = Vec::new();
    let (settings, _) = query(http, host, "ST", 1)?;
    let mode = int(&settings, "O");
    let input = match mode {
        0 => {
            let (inputs, _) = query(http, host, "IN", 0)?;
            let entries = inputs["A"].as_array().cloned().unwrap_or_default();
            match entries.first() {
                Some(first) if first["p"].as_str() == Some("e") => DeviceInput::Sacn {
                    start_universe: u16::try_from(int(first, "u")).unwrap_or(1),
                    channels_per_universe: u16::try_from(int(first, "c")).unwrap_or(510),
                    universe_count: u16::try_from(entries.iter().map(|e| int(e, "uc").max(1)).sum::<i64>())
                        .unwrap_or(1),
                },
                Some(_) => DeviceInput::Unsupported {
                    description: "Art-Net input".to_string(),
                },
                None => DeviceInput::Unsupported {
                    description: "E1.31 mode with no universes configured".to_string(),
                },
            }
        }
        2 => DeviceInput::Ddp,
        other => DeviceInput::Unsupported {
            description: format!("{} mode", mode_name(other)),
        },
    };
    if let DeviceInput::Unsupported { description } = &input {
        notes.push(format!(
            "The controller is in {description}; PixelFlow will send DDP. Switch the controller to DDP (or E1.31) mode to see live output."
        ));
    }

    let mut ports: BTreeMap<i64, Vec<(i64, i64, StringConfig)>> = BTreeMap::new();
    let mut page = 0;
    loop {
        let (payload, last) = query(http, host, "SP", page)?;
        for s in payload["A"].as_array().into_iter().flatten() {
            let pixels = int(s, "n");
            if pixels <= 0 {
                continue;
            }
            let port = int(s, "p");
            let order_code = int(s, "o");
            let color_order = color_order(order_code).unwrap_or_else(|| {
                notes.push(format!(
                    "Port {}: white-first color order (code {order_code}) isn't supported yet; using RGB.",
                    port + 1
                ));
                ColorOrder::Rgb
            });
            if int(s, "gp") > 1 {
                notes.push(format!(
                    "Port {}: pixel grouping isn't supported yet; imported ungrouped.",
                    port + 1
                ));
            }
            if int(s, "z") > 0 {
                notes.push(format!(
                    "Port {}: zig-zag isn't supported yet; imported straight.",
                    port + 1
                ));
            }
            let smart = u8::try_from(int(s, "r")).ok().filter(|r| *r > 0);
            let name = s["nm"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from);
            let config = StringConfig {
                name,
                pixels: u32::try_from(pixels).unwrap_or(0),
                color_order,
                null_pixels: u32::try_from(int(s, "ns")).unwrap_or(0),
                reverse: int(s, "v") == 1,
                brightness: u8::try_from(int(s, "b").clamp(0, 100)).unwrap_or(100),
                gamma: (int(s, "g").max(1) as f32) / 10.0,
                smart_receiver: smart,
            };
            ports
                .entry(port)
                .or_default()
                .push((int(s, "r"), int(s, "s"), config));
        }
        page += 1;
        if last || page >= MAX_PAGES {
            break;
        }
    }
    let ports = ports
        .into_iter()
        .map(|(port, mut strings)| {
            strings.sort_by_key(|(remote, index, _)| (*remote, *index));
            PortConfig {
                number: u16::try_from(port + 1).unwrap_or(0),
                strings: strings.into_iter().map(|(_, _, s)| s).collect(),
            }
        })
        .collect();
    notes.sort();
    notes.dedup();
    Ok(DeviceConfig {
        input,
        ports,
        destinations: Vec::new(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_codes_and_color_orders() {
        assert_eq!(model_for_product(130), Some("F16v5"));
        assert_eq!(model_for_product(132), Some("F32v5"));
        assert_eq!(model_for_product(99), None);
        assert_eq!(color_order(2), Some(ColorOrder::Grb));
        assert_eq!(color_order(8), None);
        assert_eq!(mode_name(2), "DDP");
    }
}
