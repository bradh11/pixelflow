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

/// Most RGB pixels one string port drives (WS2811 pixels), the board's own maximum, when known.
///
/// From xLights' Falcon definitions: `resources/controllers/falcon.xcontroller` gives each board a
/// `MaxPixelPortChannels` (2,040 channels on V2 boards, 3,072 on V3 and F48, and 3,072 or 2,112 on
/// V4/V5 depending on the board mode), counted three channels to a pixel; `Falcon.cpp`
/// (`V4_GetMaxPortPixels`) gives V4/V5 boards 1,024 WS2811 pixels a port with up to 32 ports in
/// use and 704 with more. This is the board's maximum, reached at about 20 frames a second; at
/// 40 frames xLights expects about 704 (V4/V5) or 680 (older boards) — the Wiring screen warns
/// about that from the show's frame rate.
///
/// `board_mode` is the V4/V5 `B` setting; when it's missing or unknown, the highest port in use
/// decides. RGBW pixels count as 1⅓, as the board counts channels (see `pf-mapping`).
pub fn pixels_per_port(product: u32, board_mode: Option<i64>, highest_port: u16) -> Option<u32> {
    match product {
        1..=4 => Some(680),
        5..=7 => Some(1024),
        128..=132 => {
            let ports = board_mode.and_then(board_ports).unwrap_or(highest_port);
            Some(if ports > 32 { 704 } else { 1024 })
        }
        _ => None,
    }
}

/// Pixel ports on a V4/V5 board in this board mode (xLights' `Falcon::V4_GetBoardPorts`).
fn board_ports(mode: i64) -> Option<u16> {
    Some(match mode {
        0 => 16,
        1 => 24,
        2 | 4 | 11 => 32,
        3 | 5 => 40,
        6..=10 => 48,
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

/// Most `ST` pages to read. An F16V5 on firmware Bld 32 sends everything in page 0 (marked final);
/// older V4 firmware splits it over pages 0 and 1, as xLights' `Falcon.cpp` records.
const MAX_SETTINGS_PAGES: u32 = 8;

/// The controller's settings (`ST`), merged from page 0 up to the page marked final, as xLights
/// reads them. Callers pick out only the fields they use; Wi-Fi fields are never read.
fn read_settings(http: &dyn Http, host: &str) -> Result<Value, DeviceError> {
    let mut settings = serde_json::Map::new();
    for page in 0..MAX_SETTINGS_PAGES {
        let (payload, last) = query(http, host, "ST", page)?;
        if let Value::Object(fields) = payload {
            settings.extend(fields);
        }
        if last {
            break;
        }
    }
    Ok(Value::Object(settings))
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// Identifies a Falcon from `/status.xml`, adding the name and firmware string from the JSON
/// status on V4/V5 boards.
///
/// The model comes from the product code alone. xLights also reads `BR` as the port count, but an
/// F16V5 on firmware Bld 32 reports `BR` 165, so it isn't used.
pub fn probe(http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
    let status = read_status_xml(http, host)?;
    let model = model_for_product(status.product).unwrap_or("Falcon").to_string();
    let (mut name, mut firmware) = (status.name, status.firmware);
    if status.product >= 128
        && let Ok(p) = read_settings(http, host)
    {
        // Read only the identity fields; Wi-Fi fields in the same payload are ignored.
        if let Some(n) = p["N"].as_str().filter(|s| !s.is_empty()) {
            name = n.to_string();
        }
        if let Some(v) = p["V"].as_str().filter(|s| !s.is_empty()) {
            firmware = v.to_string();
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

/// The controller mode (`O`), as xLights' `Falcon.cpp` numbers them. A real F16V5 receiving DDP
/// from an FPP reports 2.
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
    // Product codes below 128 (or a missing code) are pre-V4 boards, which don't speak the JSON API used below.
    if status.product < 128 {
        let name = if status.name.is_empty() {
            format!("Falcon {host}")
        } else {
            status.name
        };
        return Err(DeviceError::bad_plain(format!(
            "{name} is an older Falcon controller that PixelFlow can't read yet."
        )));
    }
    let settings = read_settings(http, host)?;
    let mode = int(&settings, "O");
    let board_mode = settings.get("B").and_then(Value::as_i64);
    // String start channels (`sc`) are 0-based. With absolute addressing (`A` 0) they count from the
    // controller's first channel `ps` (also 0-based), so a string at `sc` == `ps` takes the first
    // channel PixelFlow sends. With universe addressing (`A` 1) each `sc` is within its string's
    // universe (`u`), so the layout can't be checked from start channels alone.
    let absolute = int(&settings, "A") == 0;
    let first_channel = int(&settings, "ps");
    let input = match mode {
        0 => {
            let (inputs, _) = query(http, host, "IN", 0)?;
            let entries = inputs["A"].as_array().cloned().unwrap_or_default();
            // PixelFlow sends one run of same-sized universes from the first entry's universe.
            let even = entries.windows(2).all(|pair| {
                int(&pair[1], "u") == int(&pair[0], "u").saturating_add(int(&pair[0], "uc").max(1))
                    && int(&pair[1], "c") == int(&pair[0], "c")
            });
            if !even {
                notes.push(
                    "The controller's input universes aren't one continuous run of the same size, so check \
                     its universes before running a show."
                        .to_string(),
                );
            }
            match entries.first() {
                Some(first) if first["p"].as_str() == Some("e") => DeviceInput::Sacn {
                    start_universe: u16::try_from(int(first, "u")).unwrap_or(1),
                    channels_per_universe: u16::try_from(int(first, "c")).unwrap_or(510),
                    universe_count: u16::try_from(
                        entries
                            .iter()
                            .fold(0i64, |sum, e| sum.saturating_add(int(e, "uc").max(1))),
                    )
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
            "The controller is in {description}; PixelFlow will send DDP. Switch the controller to DDP mode to see live output."
        ));
    }

    // Strings per port, keyed by (smart receiver, index) so a repeated page can't add a string twice
    // and the order is the wiring order. Each also keeps the controller's start channel (`sc`).
    type Entry = (Option<i64>, StringConfig);
    let mut ports: BTreeMap<u16, BTreeMap<(i64, i64), Entry>> = BTreeMap::new();
    let mut page = 0;
    loop {
        let (payload, last) = query(http, host, "SP", page)?;
        for s in payload["A"].as_array().into_iter().flatten() {
            // Empty strings are skipped before anything else, so they never raise a note.
            if int(s, "n") <= 0 {
                continue;
            }
            let Some(number) = valid_port(int(s, "p").saturating_add(1), &mut notes) else {
                continue;
            };
            let label = format!("Port {number}");
            // `n` counts pixels that take data, not the null pixels (`ns`), which the controller adds
            // itself (xLights counts a port's length as `n` + `ns`), so each `sc` is the previous
            // string's `sc` plus `n` times the channels per pixel.
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
            // `r` 0 is the port itself; 1, 2, 3… are smart receivers A, B, C… (as in xLights).
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
                .entry(number)
                .or_default()
                .entry((int(s, "r"), int(s, "s")))
                .or_insert((s.get("sc").and_then(Value::as_i64).filter(|_| absolute), config));
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
        .map(|(number, strings)| {
            let strings: Vec<_> = strings.into_values().collect();
            placed.extend(strings.iter().map(|(start, s)| Placed {
                start: *start,
                pixels: s.pixels,
                color_order: s.color_order,
            }));
            PortConfig {
                number,
                strings: strings.into_iter().map(|(_, s)| s).collect(),
                max_pixels: None,
            }
        })
        .collect::<Vec<_>>();
    let highest = ports.iter().map(|p| p.number).max().unwrap_or(0);
    let max_pixels = pixels_per_port(status.product, board_mode, highest);
    let ports = ports
        .into_iter()
        .map(|p| PortConfig { max_pixels, ..p })
        .collect();
    if !layout_is_contiguous(&placed) {
        notes.push(LAYOUT_NOTE.to_string());
    }
    if !absolute && !placed.is_empty() {
        notes.push(
            "This Falcon places its strings by universe, so PixelFlow can't check their channel layout. \
             Check it on the controller before running a show."
                .to_string(),
        );
    }
    // PixelFlow sends the first string's data on the controller's first channel.
    if let Some(start) = placed
        .first()
        .and_then(|p| p.start)
        .map(|sc| sc.saturating_sub(first_channel))
        .filter(|offset| *offset != 0)
    {
        notes.push(format!(
            "This Falcon's strings start at its channel {}, but PixelFlow sends from channel 1. Set the first string to start at channel 1 on the Falcon, or the strings will show the wrong data.",
            start.saturating_add(1)
        ));
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

    #[test]
    fn port_limits_follow_xlights_falcon_definitions() {
        // V4/V5 (product codes 128–132): 1,024 pixels a port with up to 32 ports in use, 704 with more.
        for (product, mode, highest, want) in [
            (130, Some(0), 16, 1024),  // F16V5, 16 local ports
            (130, Some(4), 32, 1024),  // F16V5, 16 local + 4 smart receiver chains
            (128, Some(6), 16, 704),   // F16V4, 16 + 16 + 16: limited even if only 16 are used
            (131, Some(10), 48, 704),  // F48V5, 4 + 4 + 4 smart receiver chains
            (131, Some(11), 32, 1024), // F48V5, 4 + 4 smart receiver chains
            (132, None, 32, 1024),     // F32V5, mode not reported: by the ports in use
            (129, None, 40, 704),
            (129, Some(99), 40, 704), // unknown mode: by the ports in use
        ] {
            assert_eq!(
                pixels_per_port(product, mode, highest),
                Some(want),
                "{product} {mode:?} {highest}"
            );
        }
        // Older boards: V2 2,040 channels (680 pixels), V3 and F48 3,072 (1,024).
        assert_eq!(pixels_per_port(1, None, 16), Some(680));
        assert_eq!(pixels_per_port(4, None, 4), Some(680));
        assert_eq!(pixels_per_port(5, None, 16), Some(1024));
        assert_eq!(pixels_per_port(7, None, 48), Some(1024));
        assert_eq!(pixels_per_port(99, None, 16), None);
        assert_eq!(pixels_per_port(0, None, 16), None);
    }
}
