//! One interface for each kind of controller: identify it, read its status and configuration,
//! and (where supported) plan, send, check, and undo a new setup.
//!
//! Sending changes the controller, so the app only ever calls [`send_setup`] and
//! [`restore_setup`] from an explicit click, after showing [`ConfigPlan::changes`]. Every send
//! starts by reading the controller's setup again (the snapshot to put back); if that read fails,
//! or the setup has changed since it was shown, nothing is sent. Only pixel outputs and what the
//! controller receives are ever written: never network or Wi-Fi settings.
//!
//! - FPP: `GET`/`POST /api/channel/output/co-pixelStrings` (or `co-bbbStrings` on a
//!   BeagleBone), as xLights' `FPP::SetOutputs` does, keeping every field PixelFlow doesn't set.
//! - WLED: `GET`/`POST /json/cfg`, as xLights' `WLED::SetOutputs` does, but without the network,
//!   access point, security, and usermod sections, and without asking WLED to reboot.
//! - Falcon: read only for now (its V4/V5 string upload is paged and can reboot the board).

use crate::config::{DeviceConfig, color_order_from_name};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, opt_int_field, str_field};
use crate::http::Http;
use crate::setup::{Change, ChangeKind, Direction, Setup, SetupInput, SetupPort, SetupString, diff_ports};
use crate::{falcon, fpp, fpp_player, wled};
use pf_model::ColorOrder;
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Duration;

/// Anything going on at the controller that a new setup would interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    /// In plain words, when it's busy (an FPP playing a show).
    pub busy: Option<String>,
}

/// A controller's own settings document, exactly as read: what "Put back" sends.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub kind: DeviceKind,
    pub address: String,
    /// The API path it was read from (and is written back to).
    pub path: String,
    pub doc: Value,
}

/// One save to the controller.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub path: String,
    pub body: Value,
    /// What it saves, in words ("the LED outputs").
    pub what: String,
}

/// What sending the show's setup would change, before anything is sent.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigPlan {
    /// Device (before) → show (after), port by port.
    pub changes: Vec<Change>,
    /// What isn't sent, and why.
    pub notes: Vec<String>,
    /// The saves that make the changes, in order. Empty when nothing differs.
    pub writes: Vec<Write>,
}

/// A save that didn't go through: how many before it did.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyError {
    pub done: usize,
    pub total: usize,
    pub error: DeviceError,
}

/// Reading, and where supported changing, one kind of controller.
pub trait DeviceAdapter: Send + Sync {
    fn kind(&self) -> DeviceKind;
    /// Identifies the controller (changes nothing).
    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError>;
    /// What it's busy with (changes nothing).
    fn status(&self, http: &dyn Http, host: &str) -> Result<DeviceStatus, DeviceError>;
    /// Its configuration as PixelFlow understands it (changes nothing).
    fn read_config(&self, http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError>;
    /// Whether a setup can be sent to this kind of controller; the reason when it can't.
    fn can_send(&self) -> Result<(), String> {
        Ok(())
    }
    /// Reads exactly the settings a send would change, to put back later (changes nothing).
    fn snapshot(&self, http: &dyn Http, host: &str) -> Result<Snapshot, DeviceError>;
    /// What sending `target` over `snapshot` would change (changes nothing).
    fn plan_config(&self, snapshot: &Snapshot, target: &Setup) -> Result<ConfigPlan, DeviceError>;
    /// Makes the plan's saves, in order. Changes the controller.
    fn apply_config(&self, http: &dyn Http, host: &str, plan: &ConfigPlan) -> Result<(), ApplyError> {
        let total = plan.writes.len();
        for (done, write) in plan.writes.iter().enumerate() {
            post(http, host, write).map_err(|error| ApplyError { done, total, error })?;
        }
        Ok(())
    }
    /// What still differs from `target` when the controller is read back (empty when it all took).
    fn verify_config(&self, http: &dyn Http, host: &str, target: &Setup) -> Result<Vec<Change>, DeviceError> {
        let now = self.snapshot(http, host)?;
        Ok(self.plan_config(&now, target)?.changes)
    }
    /// Sends `snapshot` back. Changes the controller.
    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError>;
    /// Whether two snapshots hold the same settings (the parts a send changes).
    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool;
}

fn not_sendable(kind: &str) -> DeviceError {
    DeviceError::Message(format!("PixelFlow can't send a setup to {kind} controllers yet."))
}

fn post(http: &dyn Http, host: &str, write: &Write) -> Result<String, DeviceError> {
    let reply = http.post_json(host, &write.path, &write.body.to_string())?;
    // FPP answers `"status": "ERROR…"`, WLED `{"error": n}`, both with HTTP 200.
    if let Ok(doc) = serde_json::from_str::<Value>(&reply) {
        let fpp_error = doc
            .get("status")
            .and_then(Value::as_str)
            .filter(|s| s.to_ascii_uppercase().starts_with("ERROR"));
        if let Some(status) = fpp_error {
            return Err(DeviceError::Message(format!(
                "{host} refused the new setup: {status}"
            )));
        }
        if doc.get("error").is_some() {
            return Err(DeviceError::Message(format!(
                "{host} refused the new setup (WLED error {}). If its settings are locked with a PIN, unlock them first.",
                doc["error"]
            )));
        }
    }
    Ok(reply)
}

/// The adapter for `kind`.
pub fn adapter_for(kind: DeviceKind) -> Box<dyn DeviceAdapter> {
    match kind {
        DeviceKind::Fpp => Box::new(FppAdapter),
        DeviceKind::Wled => Box::new(WledAdapter::default()),
        DeviceKind::Falcon => Box::new(FalconAdapter),
    }
}

// ---------------------------------------------------------------------------------------------
// FPP

/// FPP over its REST API. Sends only its pixel string outputs (the cape or hat's strings).
#[derive(Debug, Clone, Copy, Default)]
pub struct FppAdapter;

/// `co-bbbStrings` on a BeagleBone, `co-pixelStrings` otherwise (xLights' `FPP.cpp` and
/// PixelFlow's own [`fpp::read_config`] choose the same way).
fn fpp_strings_path(http: &dyn Http, host: &str) -> Result<String, DeviceError> {
    let info = get_json(http, host, "/api/system/info")?;
    let file = if str_field(&info, "Platform").contains("Beagle") {
        "co-bbbStrings"
    } else {
        "co-pixelStrings"
    };
    Ok(format!("/api/channel/output/{file}"))
}

/// The pixel string driver in an FPP's string output file: the first enabled entry with outputs.
fn fpp_driver(doc: &Value) -> Option<usize> {
    doc["channelOutputs"]
        .as_array()?
        .iter()
        .position(|o| int_field(o, "enabled") != 0 && o["outputs"].is_array())
}

/// A virtual string that drives pixels (FPP keeps 0-pixel placeholders on empty ports).
fn fpp_real(vs: &Value) -> bool {
    int_field(vs, "pixelCount") > 0
}

/// The strings an FPP's string output file sets up, with where each starts (from channel 1).
fn fpp_setup(doc: &Value) -> Setup {
    let mut ports = Vec::new();
    if let Some(driver) = fpp_driver(doc) {
        for output in doc["channelOutputs"][driver]["outputs"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Ok(number) = u16::try_from(int_field(output, "portNumber").saturating_add(1)) else {
                continue;
            };
            let strings = output["virtualStrings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|vs| fpp_real(vs))
                .enumerate()
                .map(|(i, vs)| {
                    let color_order = color_order_from_name(str_field(vs, "colorOrder"));
                    SetupString {
                        name: Some(str_field(vs, "description").to_string())
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| format!("String {}", i + 1)),
                        pixels: u32::try_from(int_field(vs, "pixelCount")).unwrap_or(u32::MAX),
                        color_order,
                        start: opt_int_field(vs, "startChannel")
                            .and_then(|s| u32::try_from(s.saturating_add(1)).ok()),
                        channels_per_pixel: color_order.map_or(3, ColorOrder::channels_per_pixel),
                        slots: Vec::new(),
                    }
                })
                .collect();
            ports.push(SetupPort { number, strings });
        }
    }
    ports.sort_by_key(|p| p.number);
    Setup {
        input: SetupInput::Ddp,
        ports,
        notes: Vec::new(),
    }
}

fn order_text(order: ColorOrder) -> String {
    serde_json::to_value(order)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// A new virtual string with FPP's defaults (as xLights writes one).
fn fpp_new_string(string: &SetupString) -> Value {
    let order = string.color_order.map_or_else(
        || {
            if string.channels_per_pixel == 4 {
                "RGBW"
            } else {
                "RGB"
            }
            .to_string()
        },
        order_text,
    );
    json!({
        "description": string.name, "startChannel": 0, "pixelCount": 0, "groupCount": 0,
        "reverse": 0, "colorOrder": order, "nullNodes": 0, "endNulls": 0, "zigZag": 0,
        "brightness": 100, "gamma": "1.0"
    })
}

/// An empty port's placeholder, as FPP's own page and xLights write it.
fn fpp_placeholder() -> Value {
    json!({
        "description": "", "startChannel": 0, "pixelCount": 0, "groupCount": 0, "reverse": 0,
        "colorOrder": "RGB", "nullNodes": 0, "endNulls": 0, "zigZag": 0, "brightness": 100,
        "gamma": "1.0"
    })
}

/// The document as it's saved: without the status key FPP adds when it's read.
fn fpp_body(doc: &Value) -> Value {
    let mut body = doc.clone();
    if let Some(map) = body.as_object_mut() {
        map.remove("status");
    }
    body
}

impl DeviceAdapter for FppAdapter {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Fpp
    }

    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
        fpp::probe(http, host)
    }

    fn status(&self, http: &dyn Http, host: &str) -> Result<DeviceStatus, DeviceError> {
        let status = fpp_player::status(http, host)?;
        let what = status
            .sequence
            .clone()
            .or_else(|| status.playlist.clone())
            .unwrap_or_else(|| "a show".to_string());
        let busy = match status.state {
            fpp_player::PlayerState::Playing | fpp_player::PlayerState::Paused => Some(format!(
                "This FPP is playing {what}. Its lights may flicker or go dark while the new setup is saved."
            )),
            _ => None,
        };
        Ok(DeviceStatus { busy })
    }

    fn read_config(&self, http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
        fpp::read_config(http, host)
    }

    fn snapshot(&self, http: &dyn Http, host: &str) -> Result<Snapshot, DeviceError> {
        let path = fpp_strings_path(http, host)?;
        let doc = match get_json(http, host, &path) {
            Ok(doc) => doc,
            Err(DeviceError::Http { status: 404, .. }) => {
                return Err(DeviceError::Message(
                    "This FPP has no pixel outputs set up (no cape or hat), so there's nothing for PixelFlow to set up on it."
                        .to_string(),
                ));
            }
            Err(e) => return Err(e),
        };
        if fpp_driver(&doc).is_none() {
            return Err(DeviceError::Message(
                "This FPP's pixel outputs are turned off, so PixelFlow won't set them up. Turn them on from the FPP's own page first."
                    .to_string(),
            ));
        }
        Ok(Snapshot {
            kind: DeviceKind::Fpp,
            address: host.to_string(),
            path,
            doc,
        })
    }

    fn plan_config(&self, snapshot: &Snapshot, target: &Setup) -> Result<ConfigPlan, DeviceError> {
        let mut notes = target.notes.clone();
        let current = fpp_setup(&snapshot.doc);
        let has = |n: u16| current.ports.iter().any(|p| p.number == n);
        let mut wanted = target.clone();
        for port in &target.ports {
            if !has(port.number) && !port.strings.is_empty() {
                notes.push(format!(
                    "Port {} isn't one of this FPP's {} ports, so its strings aren't sent.",
                    port.number,
                    current.ports.len()
                ));
            }
        }
        wanted.ports.retain(|p| has(p.number));
        if let SetupInput::Sacn { .. } = target.input {
            notes.push(
                "PixelFlow sends sACN to this FPP. Its sACN inputs are set on the FPP's own page; PixelFlow doesn't change them."
                    .to_string(),
            );
        }
        let changes = diff_ports(&current, &wanted, Direction::ToDevice);
        if changes.is_empty() {
            return Ok(ConfigPlan {
                changes,
                notes,
                writes: Vec::new(),
            });
        }
        let mut doc = fpp_body(&snapshot.doc);
        let driver = fpp_driver(&doc).expect("a snapshot has a driver");
        for output in doc["channelOutputs"][driver]["outputs"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            let Ok(number) = u16::try_from(int_field(output, "portNumber").saturating_add(1)) else {
                continue;
            };
            let none = Vec::new();
            let strings = wanted.port(number).map_or(&none, |p| &p.strings);
            let existing: Vec<Value> = output["virtualStrings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|vs| fpp_real(vs))
                .cloned()
                .collect();
            if strings.is_empty() {
                if !existing.is_empty() {
                    output["virtualStrings"] = json!([fpp_placeholder()]);
                }
                continue;
            }
            let list: Vec<Value> = strings
                .iter()
                .enumerate()
                .map(|(i, string)| {
                    let mut vs = existing.get(i).cloned().unwrap_or_else(|| fpp_new_string(string));
                    vs["pixelCount"] = json!(string.pixels);
                    if let Some(start) = string.start {
                        vs["startChannel"] = json!(start.saturating_sub(1));
                    }
                    if let Some(order) = string.color_order {
                        vs["colorOrder"] = json!(order_text(order));
                    }
                    vs
                })
                .collect();
            output["virtualStrings"] = Value::Array(list);
        }
        notes.push(
            "If the lights don't change after sending, restart FPP's player (fppd) from the FPP's own page."
                .to_string(),
        );
        Ok(ConfigPlan {
            changes,
            notes,
            writes: vec![Write {
                path: snapshot.path.clone(),
                body: doc,
                what: "the pixel string outputs".to_string(),
            }],
        })
    }

    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError> {
        post(
            http,
            host,
            &Write {
                path: snapshot.path.clone(),
                body: fpp_body(&snapshot.doc),
                what: "the previous pixel string outputs".to_string(),
            },
        )
        .map(|_| ())
    }

    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool {
        a.path == b.path && fpp_body(&a.doc) == fpp_body(&b.doc)
    }
}

// ---------------------------------------------------------------------------------------------
// WLED

/// WLED over its JSON API. Sends its LED outputs' lengths, starts, and color orders, and its
/// realtime (DDP/E1.31) receive settings.
#[derive(Debug, Clone, Copy)]
pub struct WledAdapter {
    /// How long to wait before reading back: WLED sets up its outputs again after a save.
    settle: Duration,
}

impl Default for WledAdapter {
    fn default() -> Self {
        Self {
            settle: Duration::from_millis(1500),
        }
    }
}

impl WledAdapter {
    pub fn with_settle(settle: Duration) -> Self {
        Self { settle }
    }
}

/// Top-level `cfg.json` sections never sent: Wi-Fi and network, the access point, Ethernet,
/// the device's names, update and security settings, and usermods (which may hold passwords).
const WLED_NEVER_SENT: [&str; 7] = ["nw", "ap", "eth", "wifi", "id", "ota", "um"];

/// WLED's DMX modes that give every LED its own channels (`wled00/const.h`).
const WLED_MULTI_RGB: i64 = 4;
const WLED_MULTI_RGBW: i64 = 6;
const WLED_E131_PORT: i64 = 5568;

/// WLED's color order code (low nibble of `order`) for `order`, on an RGB or RGBW output.
fn wled_order_code(order: ColorOrder) -> Option<(i64, bool)> {
    Some(match order {
        ColorOrder::Grb => (0, false),
        ColorOrder::Rgb => (1, false),
        ColorOrder::Brg => (2, false),
        ColorOrder::Rbg => (3, false),
        ColorOrder::Bgr => (4, false),
        ColorOrder::Gbr => (5, false),
        ColorOrder::Grbw => (0, true),
        ColorOrder::Rgbw => (1, true),
    })
}

/// The body of a save: the whole configuration as read, less the sections never sent. WLED
/// resets some settings that a save leaves out (its frame rate, gamma, and ESP-NOW remotes,
/// `wled00/cfg.cpp`), so everything else goes back as it was.
fn wled_body(cfg: &Value) -> Value {
    let mut body = cfg.clone();
    if let Some(map) = body.as_object_mut() {
        for key in WLED_NEVER_SENT {
            map.remove(key);
        }
        if let Some(remotes) = cfg["nw"].get("linked_remote") {
            map.insert("nw".to_string(), json!({ "linked_remote": remotes }));
        }
    }
    body
}

/// One LED output: its port number (from 1), whether it drives pixels, and whether they're RGBW.
fn wled_bus_is_pixels(bus: &Value) -> (bool, bool) {
    let kind = bus["type"].as_i64().unwrap_or(22);
    let extra_white = [18, 19, 21, 28, 32, 34].contains(&kind);
    let pixels = ((16..=39).contains(&kind) || (48..=63).contains(&kind)) && !extra_white;
    (pixels, [29, 30, 31].contains(&kind))
}

fn wled_setup(cfg: &Value) -> Setup {
    let mut ports = Vec::new();
    for (i, bus) in cfg["hw"]["led"]["ins"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Ok(number) = u16::try_from(i + 1) else { break };
        let (pixels, rgbw) = wled_bus_is_pixels(bus);
        let len = bus["len"].as_i64().unwrap_or(0);
        let strings = if pixels && len > 0 {
            let code = bus["order"].as_i64().unwrap_or(0) & 0x0F;
            let color_order = [
                ColorOrder::Grb,
                ColorOrder::Rgb,
                ColorOrder::Brg,
                ColorOrder::Rbg,
                ColorOrder::Bgr,
                ColorOrder::Gbr,
                ColorOrder::Grbw,
                ColorOrder::Rgbw,
            ]
            .into_iter()
            .find(|o| wled_order_code(*o) == Some((code, rgbw)));
            vec![SetupString {
                name: format!("Output {number}"),
                pixels: u32::try_from(len).unwrap_or(u32::MAX),
                color_order,
                start: None,
                channels_per_pixel: if rgbw { 4 } else { 3 },
                slots: Vec::new(),
            }]
        } else {
            Vec::new()
        };
        ports.push(SetupPort { number, strings });
    }
    Setup {
        input: SetupInput::Ddp,
        ports,
        notes: Vec::new(),
    }
}

fn setting(
    id: &str,
    port: Option<u16>,
    subject: String,
    what: &str,
    before: String,
    after: String,
) -> Change {
    Change {
        id: id.to_string(),
        port,
        kind: ChangeKind::Setting,
        subject,
        what: what.to_string(),
        before,
        after,
        warning: None,
        can_take: false,
        why_not: None,
    }
}

impl DeviceAdapter for WledAdapter {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Wled
    }

    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
        wled::probe(http, host)
    }

    fn status(&self, http: &dyn Http, host: &str) -> Result<DeviceStatus, DeviceError> {
        let info = get_json(http, host, "/json/info")?;
        let busy = (info["live"].as_bool() == Some(true)).then(|| {
            "This WLED is showing live data now. Its lights may flicker while the new setup is saved."
                .to_string()
        });
        Ok(DeviceStatus { busy })
    }

    fn read_config(&self, http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
        wled::read_config(http, host)
    }

    fn snapshot(&self, http: &dyn Http, host: &str) -> Result<Snapshot, DeviceError> {
        let doc = get_json(http, host, "/json/cfg")?;
        if !doc["hw"]["led"]["ins"].is_array() {
            return Err(DeviceError::bad(host, "/json/cfg", "it lists no LED outputs"));
        }
        Ok(Snapshot {
            kind: DeviceKind::Wled,
            address: host.to_string(),
            path: "/json/cfg".to_string(),
            doc,
        })
    }

    fn plan_config(&self, snapshot: &Snapshot, target: &Setup) -> Result<ConfigPlan, DeviceError> {
        let cfg = &snapshot.doc;
        let mut notes = target.notes.clone();
        let current = wled_setup(cfg);
        let buses = cfg["hw"]["led"]["ins"].as_array().cloned().unwrap_or_default();
        // What the show asks of each output WLED has.
        let mut wanted = current.clone();
        for port in &target.ports {
            let Some(index) = usize::from(port.number)
                .checked_sub(1)
                .filter(|i| *i < buses.len())
            else {
                if !port.strings.is_empty() {
                    notes.push(format!(
                        "Output {} isn't set up on this WLED (it has {}). Add it in WLED's LED settings first: PixelFlow doesn't choose data pins.",
                        port.number,
                        buses.len()
                    ));
                }
                continue;
            };
            let (pixels, rgbw) = wled_bus_is_pixels(&buses[index]);
            let Some(mut string) = port.strings.first().cloned() else {
                if !current.ports[index].strings.is_empty() {
                    notes.push(format!(
                        "Output {} isn't wired in your show; PixelFlow leaves it as it is.",
                        port.number
                    ));
                }
                continue;
            };
            if !pixels {
                notes.push(format!(
                    "Output {} doesn't drive pixels; it's left as it is.",
                    port.number
                ));
                continue;
            }
            if let Some(order) = string.color_order
                && wled_order_code(order).map(|(_, w)| w) != Some(rgbw)
            {
                notes.push(format!(
                    "Output {}: its LEDs are {}, so color order {} can't be set; it's left as it is.",
                    port.number,
                    if rgbw { "RGBW" } else { "RGB" },
                    order_text(order)
                ));
                string.color_order = None;
            }
            string.start = None;
            wanted.ports[index].strings = vec![string];
        }
        for port in &current.ports {
            if !port.strings.is_empty() && target.port(port.number).is_none_or(|p| p.strings.is_empty()) {
                let note = format!(
                    "Output {} isn't wired in your show; PixelFlow leaves it as it is.",
                    port.number
                );
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
        }
        let mut changes = diff_ports(&current, &wanted, Direction::ToDevice);

        // The outputs as they'll be saved: lengths and orders from the show, starts back to back.
        let mut ins = buses.clone();
        let mut start = 0i64;
        for (i, bus) in ins.iter_mut().enumerate() {
            if let Some(string) = wanted.ports.get(i).and_then(|p| p.strings.first()) {
                bus["len"] = json!(string.pixels);
                if let Some((code, _)) = string.color_order.and_then(wled_order_code) {
                    let upper = bus["order"].as_i64().unwrap_or(0) & 0xF0;
                    bus["order"] = json!(upper | code);
                }
            }
            let was = bus["start"].as_i64().unwrap_or(0);
            if was != start {
                let number = u16::try_from(i + 1).unwrap_or(u16::MAX);
                changes.push(setting(
                    &format!("port{number}/firstLed"),
                    Some(number),
                    format!("Output {number}"),
                    "First LED",
                    was.to_string(),
                    start.to_string(),
                ));
            }
            bus["start"] = json!(start);
            start += bus["len"].as_i64().unwrap_or(0);
        }
        let outputs_change = !changes.is_empty();

        // Realtime receive.
        let live = &cfg["if"]["live"];
        let mut new_live = live.clone();
        let mut input = Vec::new();
        if live["en"].as_bool() != Some(true) {
            input.push(setting(
                "input/realtime",
                None,
                String::new(),
                "Realtime receive",
                "Off".to_string(),
                "On".to_string(),
            ));
            new_live["en"] = json!(true);
        }
        if let SetupInput::Sacn {
            start_universe,
            universe_size,
        } = target.input
        {
            let port = live["port"].as_i64().unwrap_or(WLED_E131_PORT);
            if port != WLED_E131_PORT {
                let mut change = setting(
                    "input/receives",
                    None,
                    String::new(),
                    "Receives",
                    match port {
                        6454 => "Art-Net".to_string(),
                        4048 => "DDP".to_string(),
                        other => format!("port {other}"),
                    },
                    "sACN (E1.31)".to_string(),
                );
                change.kind = ChangeKind::Receives;
                input.push(change);
                new_live["port"] = json!(WLED_E131_PORT);
            }
            if let Some(universe) = start_universe {
                let was = live["dmx"]["uni"].as_i64().unwrap_or(1);
                if was != i64::from(universe) {
                    let mut change = setting(
                        "input/startUniverse",
                        None,
                        String::new(),
                        "First universe",
                        was.to_string(),
                        universe.to_string(),
                    );
                    change.kind = ChangeKind::StartUniverse;
                    input.push(change);
                    new_live["dmx"]["uni"] = json!(universe);
                }
            }
            let rgbw = wanted
                .ports
                .iter()
                .flat_map(|p| &p.strings)
                .any(|s| s.channels_per_pixel == 4);
            let mode = live["dmx"]["mode"].as_i64().unwrap_or(WLED_MULTI_RGB);
            let want_mode = if rgbw { WLED_MULTI_RGBW } else { WLED_MULTI_RGB };
            if mode != want_mode {
                input.push(setting(
                    "input/dmxMode",
                    None,
                    String::new(),
                    "DMX mode",
                    format!("mode {mode}"),
                    if rgbw { "Multi RGBW" } else { "Multi RGB" }.to_string(),
                ));
                new_live["dmx"]["mode"] = json!(want_mode);
            }
            if live["dmx"]["addr"].as_i64().unwrap_or(1) != 1 {
                input.push(setting(
                    "input/dmxAddress",
                    None,
                    String::new(),
                    "First DMX address",
                    live["dmx"]["addr"].to_string(),
                    "1".to_string(),
                ));
                new_live["dmx"]["addr"] = json!(1);
            }
            let per_universe = if rgbw { 512 } else { 510 };
            if universe_size != per_universe {
                notes.push(format!(
                    "WLED fits {per_universe} channels in each universe. Set this controller to {per_universe} channels per universe in PixelFlow, or its colors will be off."
                ));
            }
        }
        let input_change = !input.is_empty();
        changes.extend(input);

        let mut writes = Vec::new();
        let mut body = wled_body(cfg);
        if outputs_change {
            body["hw"]["led"]["ins"] = Value::Array(ins);
            body["hw"]["led"]["total"] = json!(start);
            writes.push(Write {
                path: snapshot.path.clone(),
                body: body.clone(),
                what: "the LED outputs".to_string(),
            });
        }
        if input_change {
            body["if"]["live"] = new_live;
            writes.push(Write {
                path: snapshot.path.clone(),
                body,
                what: "the realtime receive settings".to_string(),
            });
        }
        Ok(ConfigPlan {
            changes,
            notes,
            writes,
        })
    }

    fn verify_config(&self, http: &dyn Http, host: &str, target: &Setup) -> Result<Vec<Change>, DeviceError> {
        // WLED applies new outputs on its next loop; read again once if they haven't shown yet.
        let mut left = Vec::new();
        for attempt in 0..2 {
            if !self.settle.is_zero() {
                std::thread::sleep(if attempt == 0 {
                    self.settle
                } else {
                    self.settle * 2
                });
            }
            let now = self.snapshot(http, host)?;
            left = self.plan_config(&now, target)?.changes;
            if left.is_empty() {
                break;
            }
        }
        Ok(left)
    }

    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError> {
        post(
            http,
            host,
            &Write {
                path: snapshot.path.clone(),
                body: wled_body(&snapshot.doc),
                what: "the previous setup".to_string(),
            },
        )?;
        // Give WLED a moment to set its outputs up again before it's read back.
        std::thread::sleep(self.settle);
        Ok(())
    }

    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool {
        let outputs = |s: &Snapshot| -> Vec<(Value, Value, Value, Value)> {
            s.doc["hw"]["led"]["ins"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|bus| {
                    (
                        bus["start"].clone(),
                        bus["len"].clone(),
                        bus["order"].clone(),
                        bus["type"].clone(),
                    )
                })
                .collect()
        };
        outputs(a) == outputs(b) && a.doc["if"]["live"] == b.doc["if"]["live"]
    }
}

// ---------------------------------------------------------------------------------------------
// Falcon

/// Falcon controllers: read only. Their V4/V5 string setup is sent in pages and some changes
/// reboot the board (xLights' `Falcon::V4_SendOutputs`, `V4_SendBoardMode`), so sending is left
/// for later.
#[derive(Debug, Clone, Copy, Default)]
pub struct FalconAdapter;

impl DeviceAdapter for FalconAdapter {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Falcon
    }

    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
        falcon::probe(http, host)
    }

    fn status(&self, _http: &dyn Http, _host: &str) -> Result<DeviceStatus, DeviceError> {
        Ok(DeviceStatus { busy: None })
    }

    fn read_config(&self, http: &dyn Http, host: &str) -> Result<DeviceConfig, DeviceError> {
        falcon::read_config(http, host)
    }

    fn can_send(&self) -> Result<(), String> {
        Err("PixelFlow can't send a setup to Falcon controllers yet. Use Compare to bring the Falcon's setup into your show, or set it on the Falcon's own page.".to_string())
    }

    fn snapshot(&self, _http: &dyn Http, _host: &str) -> Result<Snapshot, DeviceError> {
        Err(not_sendable("Falcon"))
    }

    fn plan_config(&self, _snapshot: &Snapshot, _target: &Setup) -> Result<ConfigPlan, DeviceError> {
        Err(not_sendable("Falcon"))
    }

    fn restore_config(&self, _http: &dyn Http, _host: &str, _snapshot: &Snapshot) -> Result<(), DeviceError> {
        Err(not_sendable("Falcon"))
    }

    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool {
        a == b
    }
}

// ---------------------------------------------------------------------------------------------
// Sending

/// How a send ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SendStatus {
    /// Saved, and reading it back matches.
    Sent,
    /// Saved, but reading it back doesn't match.
    Mismatch,
    /// A save failed partway: the controller may hold some of the new setup.
    Failed,
    /// Nothing was sent.
    Refused,
}

/// What happened, for the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReport {
    pub status: SendStatus,
    pub message: String,
    /// What still differs after reading back (device → show).
    pub mismatches: Vec<Change>,
    /// The setup from just before sending can be put back.
    pub can_restore: bool,
}

/// A send's report, and the snapshot taken just before it (to put back).
#[derive(Debug, Clone, PartialEq)]
pub struct SendOutcome {
    pub report: SendReport,
    pub snapshot: Option<Snapshot>,
}

fn refused(message: String) -> SendOutcome {
    SendOutcome {
        report: SendReport {
            status: SendStatus::Refused,
            message,
            mismatches: Vec::new(),
            can_restore: false,
        },
        snapshot: None,
    }
}

/// Sends the show's setup `target` to the controller, as shown to the user: `shown` is the
/// snapshot the plan was made from and `shown_changes` the ids of the rows they saw. Takes a new
/// snapshot first; sends nothing if it can't be read or differs from `shown`. Then saves, reads
/// back, and reports.
pub fn send_setup(
    adapter: &dyn DeviceAdapter,
    http: &dyn Http,
    host: &str,
    shown: &Snapshot,
    target: &Setup,
    shown_changes: &[String],
) -> SendOutcome {
    if let Err(reason) = adapter.can_send() {
        return refused(reason);
    }
    let snapshot = match adapter.snapshot(http, host) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            return refused(format!(
                "PixelFlow couldn't read the controller's current setup to keep a copy. Nothing was sent. {e}"
            ));
        }
    };
    if !adapter.same_setup(&snapshot, shown) {
        return refused(
            "The controller's setup changed since you looked, so nothing was sent. Review the changes again."
                .to_string(),
        );
    }
    let plan = match adapter.plan_config(&snapshot, target) {
        Ok(plan) => plan,
        Err(e) => return refused(format!("Nothing was sent. {e}")),
    };
    let ids: Vec<&str> = plan.changes.iter().map(|c| c.id.as_str()).collect();
    if ids != shown_changes.iter().map(String::as_str).collect::<Vec<_>>() {
        return refused(
            "Your show changed since you looked, so nothing was sent. Review the changes again.".to_string(),
        );
    }
    if plan.writes.is_empty() {
        return refused("The controller already matches your show; nothing was sent.".to_string());
    }
    if let Err(failure) = adapter.apply_config(http, host, &plan) {
        let message = if failure.total > 1 {
            format!(
                "Saving the new setup stopped after {} of {} steps: {} The controller may hold part of the new setup.",
                failure.done, failure.total, failure.error
            )
        } else {
            format!(
                "Saving the new setup failed: {} It may have been only partly saved.",
                failure.error
            )
        };
        return SendOutcome {
            report: SendReport {
                status: SendStatus::Failed,
                message,
                mismatches: Vec::new(),
                can_restore: true,
            },
            snapshot: Some(snapshot),
        };
    }
    let report = match adapter.verify_config(http, host, target) {
        Ok(left) if left.is_empty() => SendReport {
            status: SendStatus::Sent,
            message: "Sent. Reading it back, the controller matches your show.".to_string(),
            mismatches: Vec::new(),
            can_restore: false,
        },
        Ok(left) => SendReport {
            status: SendStatus::Mismatch,
            message: "Sent, but reading it back, the controller's setup doesn't match your show.".to_string(),
            mismatches: left,
            can_restore: true,
        },
        Err(e) => SendReport {
            status: SendStatus::Mismatch,
            message: format!("Sent, but PixelFlow couldn't read the setup back to check it. {e}"),
            mismatches: Vec::new(),
            can_restore: true,
        },
    };
    SendOutcome {
        report,
        snapshot: Some(snapshot),
    }
}

/// How putting a snapshot back went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub restored: bool,
    pub message: String,
}

/// Puts `snapshot` back on the controller and reads it back to check. Changes the controller:
/// only from the user's click.
pub fn restore_setup(
    adapter: &dyn DeviceAdapter,
    http: &dyn Http,
    host: &str,
    snapshot: &Snapshot,
) -> RestoreReport {
    if let Err(e) = adapter.restore_config(http, host, snapshot) {
        return RestoreReport {
            restored: false,
            message: format!("Putting the previous setup back failed: {e}"),
        };
    }
    match adapter.snapshot(http, host) {
        Ok(now) if adapter.same_setup(&now, snapshot) => RestoreReport {
            restored: true,
            message: "The previous setup is back on the controller.".to_string(),
        },
        Ok(_) => RestoreReport {
            restored: false,
            message: "The previous setup was sent, but reading it back, the controller doesn't match it."
                .to_string(),
        },
        Err(e) => RestoreReport {
            restored: false,
            message: format!(
                "The previous setup was sent, but PixelFlow couldn't read it back to check. {e}"
            ),
        },
    }
}
