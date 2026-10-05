//! Falcon Player (FPP) over its REST API — read-only.
//!
//! Only these endpoints are ever read: `/api/system/info`, `/api/fppd/multiSyncSystems`, and
//! `/api/channel/output/{universeOutputs,co-pixelStrings,co-bbbStrings}`. Never
//! `/api/system/status`, `/api/network/interface/*`, `/api/configfile/*`, or backups: they
//! return Wi-Fi and other passwords.

use crate::config::{
    Destination, DeviceConfig, DeviceInput, PortConfig, StringConfig, color_order_from_name,
};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::http::Http;
use pf_model::ColorOrder;
use serde_json::Value;

fn get_json(http: &dyn Http, host: &str, path: &str) -> Result<Value, DeviceError> {
    let body = http.get(host, path)?;
    serde_json::from_str(&body).map_err(|e| DeviceError::bad(host, path, e.to_string()))
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn int_field(v: &Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|x| {
            x.as_i64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .unwrap_or(0)
}

/// Identifies an FPP from `/api/system/info`.
pub fn probe(http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
    let info = get_json(http, host, "/api/system/info")?;
    let name = str_field(&info, "HostName");
    if name.is_empty() && str_field(&info, "Version").is_empty() {
        return Err(DeviceError::Unrecognized(host.to_string()));
    }
    let model = [str_field(&info, "Variant"), str_field(&info, "Platform")]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("FPP");
    Ok(Device {
        address: host.to_string(),
        kind: DeviceKind::Fpp,
        name: if name.is_empty() {
            host.to_string()
        } else {
            name.to_string()
        },
        model: model.to_string(),
        firmware: format!("FPP {}", str_field(&info, "Version")),
        mode: Some(str_field(&info, "Mode").to_string()).filter(|m| !m.is_empty()),
        found_by: Vec::new(),
    })
}

/// Other controllers this FPP knows about: its MultiSync peers and the destinations of its
/// channel outputs. Returns `(address, description)` pairs, excluding the FPP itself.
pub fn peers(http: &dyn Http, host: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    if let Ok(sync) = get_json(http, host, "/api/fppd/multiSyncSystems") {
        for system in sync["systems"].as_array().into_iter().flatten() {
            if int_field(system, "local") == 1 {
                continue;
            }
            let address = str_field(system, "address");
            if !address.is_empty() {
                found.push((address.to_string(), str_field(system, "hostname").to_string()));
            }
        }
    }
    for destination in read_destinations(http, host).unwrap_or_default() {
        found.push((destination.address, destination.description));
    }
    found.retain(|(address, _)| address != host);
    found.sort();
    found.dedup_by(|a, b| a.0 == b.0);
    found
}

fn universe_protocol(kind: i64) -> &'static str {
    match kind {
        0 => "sACN multicast",
        1 => "sACN unicast",
        2 | 3 | 9 => "Art-Net",
        4 | 5 => "DDP",
        6 | 7 => "KiNet",
        8 => "Twinkly",
        _ => "unknown",
    }
}

fn read_destinations(http: &dyn Http, host: &str) -> Result<Vec<Destination>, DeviceError> {
    let doc = get_json(http, host, "/api/channel/output/universeOutputs")?;
    let mut destinations = Vec::new();
    for output in doc["channelOutputs"].as_array().into_iter().flatten() {
        if int_field(output, "enabled") == 0 {
            continue;
        }
        for universe in output["universes"].as_array().into_iter().flatten() {
            let address = str_field(universe, "address");
            if int_field(universe, "active") == 0 || address.is_empty() {
                continue;
            }
            destinations.push(Destination {
                address: address.to_string(),
                description: str_field(universe, "description").to_string(),
                protocol: universe_protocol(int_field(universe, "type")).to_string(),
                channels: u32::try_from(int_field(universe, "channelCount")).unwrap_or(0),
            });
        }
    }
    Ok(destinations)
}

/// Reads the FPP's own pixel ports (from a pixel cape/hat) and where it sends data.
pub fn read_config(http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
    let info = get_json(http, host, "/api/system/info")?;
    let file = if str_field(&info, "Platform").contains("Beagle") {
        "co-bbbStrings"
    } else {
        "co-pixelStrings"
    };
    let mut notes = Vec::new();
    let mut ports = Vec::new();
    if let Ok(strings) = get_json(http, host, &format!("/api/channel/output/{file}")) {
        for output in strings["channelOutputs"].as_array().into_iter().flatten() {
            if int_field(output, "enabled") == 0 {
                continue;
            }
            for port in output["outputs"].as_array().into_iter().flatten() {
                let number = u16::try_from(int_field(port, "portNumber") + 1).unwrap_or(0);
                let strings: Vec<StringConfig> = port["virtualStrings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|vs| int_field(vs, "pixelCount") > 0)
                    .map(|vs| {
                        let order_name = str_field(vs, "colorOrder");
                        let color_order = color_order_from_name(order_name).unwrap_or_else(|| {
                            notes.push(format!(
                                "Port {number}: color order {order_name} isn't supported yet; using RGB."
                            ));
                            ColorOrder::Rgb
                        });
                        StringConfig {
                            name: Some(str_field(vs, "description").to_string()).filter(|s| !s.is_empty()),
                            pixels: u32::try_from(int_field(vs, "pixelCount")).unwrap_or(0),
                            color_order,
                            null_pixels: u32::try_from(int_field(vs, "nullNodes")).unwrap_or(0),
                            reverse: int_field(vs, "reverse") != 0,
                            brightness: u8::try_from(int_field(vs, "brightness").clamp(0, 100))
                                .unwrap_or(100),
                            gamma: str_field(vs, "gamma")
                                .parse()
                                .ok()
                                .or_else(|| vs["gamma"].as_f64().map(|g| g as f32))
                                .unwrap_or(1.0),
                            smart_receiver: None,
                        }
                    })
                    .collect();
                if !strings.is_empty() {
                    ports.push(PortConfig { number, strings });
                }
            }
        }
    }
    let destinations = read_destinations(http, host).unwrap_or_default();
    if ports.is_empty() {
        notes.push(if destinations.is_empty() {
            "This FPP has no pixel outputs of its own.".to_string()
        } else {
            "This FPP has no pixel outputs of its own; it sends to the controllers listed below. Import those instead."
                .to_string()
        });
    }
    let mode = str_field(&info, "Mode");
    if !ports.is_empty() && mode != "bridge" {
        notes.push(format!(
            "FPP is in {mode} mode. Switch it to bridge mode to show PixelFlow's live output."
        ));
    }
    Ok(DeviceConfig {
        input: DeviceInput::Ddp,
        ports,
        destinations,
        notes,
    })
}
