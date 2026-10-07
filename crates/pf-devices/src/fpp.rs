//! Falcon Player (FPP) over its REST API — read-only.
//!
//! Only these endpoints are ever read: `/api/system/info`, `/api/fppd/multiSyncSystems`, and
//! `/api/channel/output/{universeOutputs,co-pixelStrings,co-bbbStrings}`. Never
//! `/api/system/status`, `/api/network/interface/*`, `/api/configfile/*`, or backups: they
//! return Wi-Fi and other passwords.

use crate::config::{
    Destination, DeviceConfig, DeviceInput, LAYOUT_NOTE, Placed, PortConfig, StringConfig, bounded_nulls,
    bounded_pixels, color_order_from_name, layout_is_contiguous, valid_port,
};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::http::Http;
use pf_model::ColorOrder;
use serde_json::Value;

pub(crate) fn get_json(http: &dyn Http, host: &str, path: &str) -> Result<Value, DeviceError> {
    let body = http.get(host, path)?;
    serde_json::from_str(&body).map_err(|e| DeviceError::bad(host, path, e.to_string()))
}

pub(crate) fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn int_field(v: &Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|x| {
            x.as_i64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .unwrap_or(0)
}

pub(crate) fn opt_int_field(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

/// A listing that FPP doesn't have (HTTP 404) is empty; any other failure is a real error.
fn get_json_or_none(http: &dyn Http, host: &str, path: &str) -> Result<Option<Value>, DeviceError> {
    match get_json(http, host, path) {
        Ok(doc) => Ok(Some(doc)),
        Err(DeviceError::Http { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
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

/// The FPP major version, from `majorVersion` or the start of `Version`. Unknown reads as modern.
fn major_version(info: &Value) -> i64 {
    opt_int_field(info, "majorVersion")
        .or_else(|| {
            str_field(info, "Version")
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(i64::MAX)
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
    // Same address listed twice: keep the entry that has a description.
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
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
    let Some(doc) = get_json_or_none(http, host, "/api/channel/output/universeOutputs")? else {
        return Ok(Vec::new());
    };
    let mut destinations: Vec<Destination> = Vec::new();
    // For each destination, the universe that would continue its sACN run without a gap.
    let mut next_universe: Vec<Option<u32>> = Vec::new();
    for output in doc["channelOutputs"].as_array().into_iter().flatten() {
        if int_field(output, "enabled") == 0 {
            continue;
        }
        for universe in output["universes"].as_array().into_iter().flatten() {
            let address = str_field(universe, "address");
            if int_field(universe, "active") == 0 || address.is_empty() {
                continue;
            }
            let kind = int_field(universe, "type");
            let protocol = universe_protocol(kind);
            let start_channel = u32::try_from(int_field(universe, "startChannel").max(1)).unwrap_or(1);
            // For sACN entries, FPP's `id` is the universe number.
            let start_universe = matches!(kind, 0 | 1)
                .then(|| u16::try_from(int_field(universe, "id")).ok())
                .flatten()
                .filter(|u| *u > 0);
            // `channelCount` is per universe; `universeCount` universes run back to back.
            let per_universe = int_field(universe, "channelCount").max(0);
            let count = opt_int_field(universe, "universeCount").map_or(1, |c| c.max(1));
            let channels = u32::try_from(per_universe.saturating_mul(count)).unwrap_or(u32::MAX);
            let universe_size = matches!(kind, 0 | 1)
                .then(|| u16::try_from(per_universe).ok())
                .flatten();
            let following = start_universe
                .and_then(|u| u32::try_from(count).ok().map(|c| u32::from(u).saturating_add(c)));
            // One entry per address and protocol, however many universe ranges FPP lists.
            if let Some(index) = destinations
                .iter()
                .position(|d| d.address == address && d.protocol == protocol)
            {
                let existing = &mut destinations[index];
                if kind == 0 || kind == 1 {
                    let in_a_row = existing.universe_size == universe_size
                        && start_universe.map(u32::from) == next_universe[index];
                    existing.uneven_universes |= !in_a_row;
                    next_universe[index] = following;
                }
                existing.ddp_raw &= kind == 4;
                existing.channels = existing.channels.saturating_add(channels);
                existing.start_channel = existing.start_channel.min(start_channel);
                existing.start_universe = match (existing.start_universe, start_universe) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                if existing.description.is_empty() {
                    existing.description = str_field(universe, "description").to_string();
                }
                continue;
            }
            destinations.push(Destination {
                address: address.to_string(),
                description: str_field(universe, "description").to_string(),
                protocol: protocol.to_string(),
                channels,
                start_channel,
                start_universe,
                universe_size,
                // FPP: type 4 = "DDP Raw Channel Numbers", type 5 = "DDP One Based". In FPP's
                // DDP.cpp the first packet's offset is `startChannel - 1` for type 4 and 0 for
                // type 5 (each later packet adds the bytes already sent).
                ddp_raw: kind == 4,
                uneven_universes: false,
            });
            next_universe.push(following);
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
    // Every kept string in import order, for the channel layout check.
    let mut placed = Vec::new();
    let strings_doc = get_json_or_none(http, host, &format!("/api/channel/output/{file}"))?;
    for output in strings_doc
        .as_ref()
        .map(|d| &d["channelOutputs"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if int_field(output, "enabled") == 0 {
            continue;
        }
        for port in output["outputs"].as_array().into_iter().flatten() {
            let raw_number = int_field(port, "portNumber").saturating_add(1);
            let mut strings = Vec::new();
            for vs in port["virtualStrings"].as_array().into_iter().flatten() {
                if int_field(vs, "pixelCount") <= 0 {
                    continue;
                }
                let Some(number) = valid_port(raw_number, &mut notes) else {
                    continue;
                };
                let label = format!("Port {number}");
                let Some(pixels) = bounded_pixels(&label, int_field(vs, "pixelCount"), &mut notes) else {
                    continue;
                };
                let order_name = str_field(vs, "colorOrder");
                let color_order = color_order_from_name(order_name).unwrap_or_else(|| {
                    notes.push(format!(
                        "Port {number}: color order {order_name} isn't supported yet; using RGB."
                    ));
                    ColorOrder::Rgb
                });
                if int_field(vs, "groupCount") > 1 {
                    notes.push(format!(
                        "Port {number}: pixel grouping isn't supported yet; imported ungrouped."
                    ));
                }
                if int_field(vs, "zigZag") > 0 {
                    notes.push(format!(
                        "Port {number}: zig-zag isn't supported yet; imported straight."
                    ));
                }
                placed.push(Placed {
                    start: opt_int_field(vs, "startChannel"),
                    pixels,
                    color_order,
                });
                let gamma = str_field(vs, "gamma")
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .or_else(|| vs["gamma"].as_f64())
                    .filter(|g| g.is_finite() && *g > 0.0)
                    .map_or(1.0, |g| g as f32);
                strings.push(StringConfig {
                    name: Some(str_field(vs, "description").to_string()).filter(|s| !s.is_empty()),
                    pixels,
                    color_order,
                    null_pixels: bounded_nulls(&label, int_field(vs, "nullNodes"), &mut notes),
                    reverse: int_field(vs, "reverse") != 0,
                    // A missing brightness means full brightness, not off.
                    brightness: u8::try_from(opt_int_field(vs, "brightness").unwrap_or(100).clamp(0, 100))
                        .unwrap_or(100),
                    gamma,
                    smart_receiver: None,
                });
            }
            if !strings.is_empty() {
                // `strings` is non-empty only when `valid_port` passed.
                if let Ok(number) = u16::try_from(raw_number) {
                    ports.push(PortConfig {
                        number,
                        strings,
                        max_pixels: None,
                    });
                }
            }
        }
    }
    if !layout_is_contiguous(&placed) {
        notes.push(LAYOUT_NOTE.to_string());
    }
    let destinations = read_destinations(http, host)?;
    if ports.is_empty() {
        notes.push(if destinations.is_empty() {
            "This FPP has no pixel outputs of its own.".to_string()
        } else {
            "This FPP has no pixel outputs of its own; it sends to the controllers listed above. Add each one with its Add to show button."
                .to_string()
        });
    }
    if !ports.is_empty() {
        let mode = str_field(&info, "Mode");
        if major_version(&info) >= 5 {
            // Bridge mode was removed in FPP 5; an idle player accepts live data.
            notes.push(
                "If a playlist or sequence is running on this FPP, it overrides PixelFlow's live output; stop it while using PixelFlow."
                    .to_string(),
            );
        } else if mode != "bridge" {
            notes.push(if mode.is_empty() {
                "This FPP isn't in bridge mode. Switch it to bridge mode to show PixelFlow's live output."
                    .to_string()
            } else {
                format!("FPP is in {mode} mode. Switch it to bridge mode to show PixelFlow's live output.")
            });
        }
        // PixelFlow's DDP output always starts at channel 1; FPP's start channels are 0-based.
        if let Some(start) = placed.first().and_then(|p| p.start).filter(|s| *s != 0) {
            notes.push(format!(
                "This FPP's strings start at channel {}, but PixelFlow sends from channel 1. Set the first string to start at channel 1 on the FPP, or the strings will stay dark.",
                start.saturating_add(1)
            ));
        }
    }
    // Several strings can raise the same note; say each once, in the order first seen.
    let mut seen = std::collections::HashSet::new();
    notes.retain(|n| seen.insert(n.clone()));
    Ok(DeviceConfig {
        input: DeviceInput::Ddp,
        ports,
        destinations,
        notes,
    })
}
