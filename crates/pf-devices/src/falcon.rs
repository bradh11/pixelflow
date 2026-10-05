//! Falcon pixel controllers (F16V4/F48V4, F16V5/F48V5/F32V5) — read-only.
//!
//! Identity comes from `GET /status.xml`; configuration from the JSON API at `POST /api` with
//! *query* requests only (`"T":"Q"`), following xLights' `Falcon.cpp`. The status reply also
//! carries Wi-Fi fields (`WS`, `WP`, `CP`); they are never read or kept.

use crate::config::{
    DeviceConfig, DeviceInput, LAYOUT_NOTE, Placed, PortConfig, StringConfig, bounded_nulls, bounded_pixels,
    layout_is_contiguous, valid_port,
};
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
    let status = read_status_xml(http, host)?;
    // Product codes below 128 are pre-V4 boards, which don't speak the JSON API used below.
    if (1..128).contains(&status.product) {
        let name = if status.name.is_empty() {
            format!("Falcon {host}")
        } else {
            status.name
        };
        return Err(DeviceError::bad_plain(format!(
            "{name} is an older Falcon controller that PixelFlow can't read yet."
        )));
    }
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

    // Strings per port, keyed by (smart receiver, index) so a repeated page can't add a string twice
    // and the order is the wiring order. Each also keeps the controller's start channel (`sc`).
    type Entry = (Option<i64>, StringConfig);
    let mut ports: BTreeMap<i64, BTreeMap<(i64, i64), Entry>> = BTreeMap::new();
    let mut page = 0;
    loop {
        let (payload, last) = query(http, host, "SP", page)?;
        for s in payload["A"].as_array().into_iter().flatten() {
            let port = int(s, "p");
            // Empty strings are skipped before anything else, so they never raise a note.
            if int(s, "n") <= 0 {
                continue;
            }
            let Some(number) = valid_port(port + 1, &mut notes) else {
                continue;
            };
            let label = format!("Port {number}");
            // `n` counts pixels that take data; the controller skips its null pixels (`ns`) itself, so
            // each `sc` is the previous string's `sc` plus `n` times the channels per pixel.
            let Some(pixels) = bounded_pixels(&label, int(s, "n"), &mut notes) else {
                continue;
            };
            let order_code = int(s, "o");
            let color_order = color_order(order_code).unwrap_or_else(|| {
                notes.push(format!(
                    "Port {number}: white-first color order (code {order_code}) isn't supported yet; using RGB."
                ));
                ColorOrder::Rgb
            });
            if int(s, "gp") > 1 {
                notes.push(format!(
                    "Port {number}: pixel grouping isn't supported yet; imported ungrouped."
                ));
            }
            if int(s, "z") > 0 {
                notes.push(format!(
                    "Port {number}: zig-zag isn't supported yet; imported straight."
                ));
            }
            let smart = u8::try_from(int(s, "r")).ok().filter(|r| *r > 0);
            let name = s["nm"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from);
            // Tenths of a gamma value; 0 or missing means no correction.
            let gamma = match int(s, "g") {
                g if g > 0 => g as f32 / 10.0,
                _ => 1.0,
            };
            let config = StringConfig {
                name,
                pixels,
                color_order,
                null_pixels: bounded_nulls(&label, int(s, "ns"), &mut notes),
                reverse: int(s, "v") == 1,
                // A missing brightness means full brightness, not off.
                brightness: u8::try_from(s.get("b").and_then(Value::as_i64).unwrap_or(100).clamp(0, 100))
                    .unwrap_or(100),
                gamma,
                smart_receiver: smart,
            };
            ports
                .entry(port)
                .or_default()
                .entry((int(s, "r"), int(s, "s")))
                .or_insert((s.get("sc").and_then(Value::as_i64), config));
        }
        page += 1;
        if last {
            break;
        }
        if page >= MAX_PAGES {
            notes.push(
                "The controller kept sending string pages; the list of strings may be incomplete."
                    .to_string(),
            );
            break;
        }
    }
    let mut placed = Vec::new();
    let ports = ports
        .into_iter()
        .filter_map(|(port, strings)| {
            let number = u16::try_from(port + 1).ok()?;
            let strings: Vec<_> = strings.into_values().collect();
            placed.extend(strings.iter().map(|(start, s)| Placed {
                start: *start,
                pixels: s.pixels,
                color_order: s.color_order,
            }));
            Some(PortConfig {
                number,
                strings: strings.into_iter().map(|(_, s)| s).collect(),
            })
        })
        .collect();
    if !layout_is_contiguous(&placed) {
        notes.push(LAYOUT_NOTE.to_string());
    }
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
