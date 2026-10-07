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
//!   BeagleBone), as xLights' `FPP::UploadPixelOutputs` does, keeping every field PixelFlow
//!   doesn't set. FPP 9 runs `stripslashes()` over the body and doesn't check it
//!   (`channel.php` at 9.5.3), so no `"` or `\` is ever sent; and fppd refuses to load strings
//!   past its limits (`PixelString.cpp`), so such plans are refused here.
//! - WLED: one `POST /json/cfg` holding only `hw.led` (as read, outputs edited), `light` (as
//!   read), `if.live` (when the receive settings change), and `nw.linked_remote` (as read): the
//!   settings WLED resets or clears when a save leaves them out. Never Wi-Fi, never the color
//!   order overrides (`hw.com`, which WLED adds to rather than replaces), never a reboot.
//! - Falcon: read only for now (its V4/V5 string upload is paged and can reboot the board).

use crate::config::{DeviceConfig, color_order_from_name, with_commas};
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, opt_int_field, str_field};
use crate::http::Http;
use crate::setup::{
    Change, ChangeKind, Direction, MOVES_PIXELS, Setup, SetupInput, SetupPort, SetupString, diff_ports,
    string_key,
};
use crate::{falcon, fpp, fpp_player, wled};
use pf_model::ColorOrder;
use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// What isn't sent, and why, and what to check afterwards.
    pub notes: Vec<String>,
    /// Why this can't be sent as it is (the controller wouldn't load it, or it would run over
    /// outputs PixelFlow doesn't set up). When there are any, there are no writes.
    pub problems: Vec<String>,
    /// The saves that make the changes, in order. Empty when nothing differs or can't be sent.
    pub writes: Vec<Write>,
    /// What reading back should find once the writes are saved (adapter-specific).
    pub expect: Value,
}

impl ConfigPlan {
    fn nothing(notes: Vec<String>) -> Self {
        Self {
            changes: Vec::new(),
            notes,
            problems: Vec::new(),
            writes: Vec::new(),
            expect: Value::Null,
        }
    }
}

/// What a controller is, whatever its address: its kind and a stable hardware id (an FPP's
/// `uuid`, or its host name when it has none; a WLED's MAC). A kept copy of a setup belongs to
/// the device, and is only ever put back on that same device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    pub kind: DeviceKind,
    pub id: String,
    /// Its name, for messages.
    pub name: String,
}

impl DeviceIdentity {
    /// A file-safe key that no other identity shares: the kind, then the id's bytes in hex.
    pub fn key(&self) -> String {
        let kind = match self.kind {
            DeviceKind::Fpp => "fpp",
            DeviceKind::Falcon => "falcon",
            DeviceKind::Wled => "wled",
        };
        let hex: String = self.id.bytes().map(|b| format!("{b:02x}")).collect();
        format!("{kind}-{hex}")
    }

    /// Whether `other` is the same device.
    pub fn same_device(&self, other: &DeviceIdentity) -> bool {
        self.kind == other.kind && self.id == other.id
    }
}

/// What reading back after a send found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verification {
    /// What still differs from what was sent (empty when it all took).
    pub left: Vec<Change>,
    /// Things worth knowing that PixelFlow didn't cause (problems on ports it didn't change).
    pub notes: Vec<String>,
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
    /// Which device this is, whatever its address (changes nothing).
    fn identity(&self, http: &dyn Http, host: &str) -> Result<DeviceIdentity, DeviceError> {
        let _ = (http, host);
        Err(not_sendable("these"))
    }
    /// What putting `copy` back would change on a controller now holding `now`: now (before) →
    /// copy (after).
    fn restore_changes(&self, now: &Snapshot, copy: &Snapshot) -> Vec<Change> {
        let _ = (now, copy);
        Vec::new()
    }
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
    /// Whether a save's reply says it was saved properly.
    fn check_reply(&self, host: &str, reply: &str) -> Result<(), DeviceError> {
        let _ = (host, reply);
        Ok(())
    }
    /// Makes the plan's saves, in order. Changes the controller.
    fn apply_config(&self, http: &dyn Http, host: &str, plan: &ConfigPlan) -> Result<(), ApplyError> {
        let total = plan.writes.len();
        for (done, write) in plan.writes.iter().enumerate() {
            post(http, host, write)
                .and_then(|reply| self.check_reply(host, &reply))
                .map_err(|error| ApplyError { done, total, error })?;
        }
        Ok(())
    }
    /// What still differs once `plan` was saved and the controller is read back (nothing left
    /// when it all took). An error means it couldn't be read back.
    fn verify_config(
        &self,
        http: &dyn Http,
        host: &str,
        target: &Setup,
        plan: &ConfigPlan,
    ) -> Result<Verification, DeviceError> {
        let _ = plan;
        let now = self.snapshot(http, host)?;
        Ok(Verification {
            left: self.plan_config(&now, target)?.changes,
            notes: Vec::new(),
        })
    }
    /// Sends `snapshot` back. Changes the controller.
    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError>;
    /// Whether two snapshots hold the same settings (the parts a send changes).
    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool;
    /// Whether `now`, read after putting `snapshot` back, holds it.
    fn restored(&self, now: &Snapshot, snapshot: &Snapshot) -> bool {
        self.same_setup(now, snapshot)
    }
    /// What a send that read back as sent says.
    fn sent_message(&self) -> String {
        "Sent. Reading it back, the controller matches your show.".to_string()
    }
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

fn order_text(order: ColorOrder) -> String {
    serde_json::to_value(order)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn mismatch(
    id: String,
    port: Option<u16>,
    subject: String,
    what: &str,
    before: String,
    after: String,
) -> Change {
    Change {
        id,
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

// ---------------------------------------------------------------------------------------------
// FPP

/// FPP over its REST API. Sends only its pixel string outputs (the cape or hat's strings).
#[derive(Debug, Clone, Copy, Default)]
pub struct FppAdapter;

/// fppd's longest pixel string, nulls included (`MAX_PIXEL_STRING_LENGTH`, `PixelString.cpp`).
const FPP_MAX_STRING: i64 = 1600;
/// fppd's channel count (`FPPD_MAX_CHANNELS`, `Sequence.h` at 9.5.3).
const FPP_MAX_CHANNELS: i64 = 8192 * 1024;

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

/// Text FPP 9 can save: its `channel_save_output()` runs PHP's `stripslashes()` over the whole
/// body, which turns `"12\" Star"` into broken JSON. Quotes become two apostrophes, backslashes
/// slashes, and control characters (sent as `\n`, `\u0001`…) are dropped.
pub fn fpp_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '"' => "''".to_string(),
            '\\' => "/".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Makes every string in `value` safe for FPP 9 ([`fpp_text`]); how many changed.
fn fpp_clean(value: &mut Value) -> usize {
    match value {
        Value::String(text) => {
            let clean = fpp_text(text);
            if clean == *text {
                0
            } else {
                *text = clean;
                1
            }
        }
        Value::Array(items) => items.iter_mut().map(fpp_clean).sum(),
        Value::Object(map) => map.values_mut().map(fpp_clean).sum(),
        _ => 0,
    }
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
                .map(|vs| {
                    let color_order = color_order_from_name(str_field(vs, "colorOrder"));
                    SetupString {
                        name: str_field(vs, "description").to_string(),
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
        left_alone: Vec::new(),
    }
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
        "description": fpp_text(&string.name), "startChannel": 0, "pixelCount": 0, "groupCount": 0,
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

/// A number field FPP keeps as a number or a string ("gamma": "2.2").
fn fpp_number(vs: &Value, key: &str) -> f64 {
    vs.get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()))
        .unwrap_or(0.0)
}

/// Why fppd wouldn't load this virtual string (`PixelString.cpp` at 9.5.3, lines 601-617), in
/// plain words, or `None`.
fn fpp_load_problem(vs: &Value) -> Option<String> {
    let n = |key: &str| int_field(vs, key);
    let pixels = n("pixelCount");
    if pixels > FPP_MAX_STRING {
        return Some(format!(
            "an FPP string drives at most 1,600 pixels, and this one would have {}. Split it across strings or ports",
            with_commas(pixels)
        ));
    }
    if pixels <= 0 {
        return None;
    }
    let (nulls, end) = (n("nullNodes"), n("endNulls"));
    if nulls < 0 || end < 0 || n("groupCount") < 0 || n("zigZag") < 0 || n("startChannel") < 0 {
        return Some("one of its settings on the FPP is below zero".to_string());
    }
    if nulls + pixels + end > FPP_MAX_STRING {
        return Some(format!(
            "with the FPP's own {nulls} null pixels and {end} end nulls, {} pixels go past FPP's 1,600-pixel string",
            with_commas(pixels)
        ));
    }
    if n("groupCount") > pixels {
        return Some(format!(
            "the FPP groups it by {}, more than its {} pixels. Turn grouping off on the FPP's page first",
            n("groupCount"),
            with_commas(pixels)
        ));
    }
    if n("zigZag") > pixels {
        return Some(format!(
            "the FPP zig-zags it every {} pixels, more than its {}. Turn zig-zag off on the FPP's page first",
            n("zigZag"),
            with_commas(pixels)
        ));
    }
    if n("startChannel") > FPP_MAX_CHANNELS {
        return Some("it would start past FPP's last channel".to_string());
    }
    None
}

/// The settings a virtual string carries that PixelFlow doesn't set, when not FPP's defaults.
fn fpp_kept(vs: &Value) -> Vec<String> {
    let mut kept = Vec::new();
    if int_field(vs, "reverse") != 0 {
        kept.push("reversed".to_string());
    }
    let nulls = int_field(vs, "nullNodes");
    if nulls > 0 {
        kept.push(if nulls == 1 {
            "1 null pixel".to_string()
        } else {
            format!("{nulls} null pixels")
        });
    }
    let end = int_field(vs, "endNulls");
    if end > 0 {
        kept.push(format!("{end} end nulls"));
    }
    let brightness = vs.get("brightness").map_or(100, |_| int_field(vs, "brightness"));
    if brightness != 100 {
        kept.push(format!("{brightness}% brightness"));
    }
    let gamma = fpp_number(vs, "gamma");
    if gamma > 0.0 && (gamma - 1.0).abs() > 1e-6 {
        kept.push(format!("gamma {gamma}"));
    }
    if int_field(vs, "groupCount") > 1 {
        kept.push(format!("grouped by {}", int_field(vs, "groupCount")));
    }
    if int_field(vs, "zigZag") > 1 {
        kept.push(format!("zig-zag {}", int_field(vs, "zigZag")));
    }
    kept
}

fn fpp_string_rows(port: u16, index: usize, old: &Value, new: &Value) -> Vec<Change> {
    let (from, to) = (str_field(old, "description"), str_field(new, "description"));
    if from == to {
        return Vec::new();
    }
    let key = string_key(port, index);
    let subject = format!("String {}", index + 1);
    let shown = |name: &str| {
        if name.is_empty() {
            "None".to_string()
        } else {
            name.to_string()
        }
    };
    let mut rows = vec![mismatch(
        format!("{key}/name"),
        Some(port),
        subject.clone(),
        "Name",
        shown(from),
        shown(to),
    )];
    let kept = fpp_kept(old);
    if !kept.is_empty() {
        let list = kept.join(", ");
        let mut row = mismatch(
            format!("{key}/kept"),
            Some(port),
            subject,
            "Keeps",
            list.clone(),
            list,
        );
        row.warning = Some(format!(
            "These were set on the FPP for {}; they stay on this string for {}. Change them on the FPP's page if they don't suit it.",
            shown(from),
            shown(to)
        ));
        rows.push(row);
    }
    rows
}

impl DeviceAdapter for FppAdapter {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Fpp
    }

    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
        fpp::probe(http, host)
    }

    /// FPP's `uuid` (from the board's serial number), or its host name when it has none.
    fn identity(&self, http: &dyn Http, host: &str) -> Result<DeviceIdentity, DeviceError> {
        let info = get_json(http, host, "/api/system/info")?;
        let name = str_field(&info, "HostName").trim().to_string();
        let id = Some(str_field(&info, "uuid").trim().to_string())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| name.clone());
        if id.is_empty() {
            return Err(DeviceError::bad(
                host,
                "/api/system/info",
                "it gives no uuid or host name",
            ));
        }
        Ok(DeviceIdentity {
            kind: DeviceKind::Fpp,
            id,
            name,
        })
    }

    fn restore_changes(&self, now: &Snapshot, copy: &Snapshot) -> Vec<Change> {
        let mut rows = diff_ports(&fpp_setup(&now.doc), &fpp_setup(&copy.doc), Direction::ToDevice);
        if rows.is_empty() && !self.restored(now, copy) {
            rows.push(mismatch(
                "restore/other".to_string(),
                None,
                String::new(),
                "Other pixel string settings",
                "as they are now".to_string(),
                "as in the copy".to_string(),
            ));
        }
        rows
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
        if doc.get("channelOutputs").is_none() {
            return Err(DeviceError::Message(
                "FPP can't read its own pixel string outputs: the file holding them is damaged. Fix it on the FPP's page (Channel Outputs → Pixel Strings) before sending."
                    .to_string(),
            ));
        }
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
        let mut changes = diff_ports(&current, &wanted, Direction::ToDevice);
        // What fppd won't load, by port: a problem where PixelFlow changes the port, a note
        // where it doesn't.
        let mut port_problems: Vec<(u16, String)> = Vec::new();
        let mut doc = fpp_body(&snapshot.doc);
        let cleaned = fpp_clean(&mut doc);
        let driver = fpp_driver(&doc).expect("a snapshot has a driver");
        for output in doc["channelOutputs"][driver]["outputs"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            let Ok(number) = u16::try_from(int_field(output, "portNumber").saturating_add(1)) else {
                continue;
            };
            if target.left_alone.contains(&number) {
                continue;
            }
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
            let mut list = Vec::new();
            for (i, string) in strings.iter().enumerate() {
                let label = if string.name.is_empty() {
                    format!("Port {number} string {}", i + 1)
                } else {
                    format!("Port {number} string {} ({})", i + 1, string.name)
                };
                let mut vs = existing.get(i).cloned().unwrap_or_else(|| fpp_new_string(string));
                vs["pixelCount"] = json!(string.pixels);
                vs["description"] = json!(fpp_text(&string.name));
                if let Some(start) = string.start {
                    vs["startChannel"] = json!(start.saturating_sub(1));
                }
                if let Some(order) = string.color_order {
                    if order.channels_per_pixel() != string.channels_per_pixel {
                        port_problems.push((number, format!(
                            "{label}: color order {} takes {} channels a pixel, but its prop sends {}. Change the color order on the Wiring screen.",
                            order_text(order),
                            order.channels_per_pixel(),
                            string.channels_per_pixel
                        )));
                    }
                    vs["colorOrder"] = json!(order_text(order));
                }
                if let Some(problem) = fpp_load_problem(&vs) {
                    port_problems.push((number, format!("{label}: {problem}.")));
                }
                if let Some(old) = existing.get(i) {
                    changes.extend(fpp_string_rows(number, i, old, &vs));
                }
                list.push(vs);
            }
            output["virtualStrings"] = Value::Array(list);
        }
        let changed: Vec<u16> = changes.iter().filter_map(|c| c.port).collect();
        let mut problems = Vec::new();
        for (port, problem) in port_problems {
            if changed.contains(&port) {
                problems.push(problem);
            } else {
                notes.push(format!(
                    "{problem} PixelFlow doesn't change port {port}, but fppd won't load the FPP's strings until it's fixed on the FPP's page."
                ));
            }
        }
        if changes.is_empty() {
            return Ok(ConfigPlan::nothing(notes));
        }
        if cleaned > 0 {
            notes.push(format!(
                "{} on this FPP {} quotes or backslashes, which FPP 9 can't save; they're saved with '' and / instead.",
                if cleaned == 1 { "1 name".to_string() } else { format!("{cleaned} names") },
                if cleaned == 1 { "has" } else { "have" }
            ));
        }
        let writes = if problems.is_empty() {
            vec![Write {
                path: snapshot.path.clone(),
                body: doc.clone(),
                what: "the pixel string outputs".to_string(),
            }]
        } else {
            Vec::new()
        };
        Ok(ConfigPlan {
            changes,
            notes,
            problems,
            writes,
            expect: doc,
        })
    }

    /// FPP's save echoes the saved file back; without its `channelOutputs`, the file FPP wrote
    /// can't be read (on FPP 9, the reply is a bare `{"status":"OK"}`).
    fn check_reply(&self, host: &str, reply: &str) -> Result<(), DeviceError> {
        let ok = serde_json::from_str::<Value>(reply).is_ok_and(|doc| doc["channelOutputs"].is_array());
        if ok {
            Ok(())
        } else {
            Err(DeviceError::Message(format!(
                "{host} saved the pixel outputs, but it can't read them back: the file it wrote is damaged, and its strings would stay dark after its player restarts."
            )))
        }
    }

    fn verify_config(
        &self,
        http: &dyn Http,
        host: &str,
        target: &Setup,
        plan: &ConfigPlan,
    ) -> Result<Verification, DeviceError> {
        let now = self.snapshot(http, host)?;
        let mut left = self.plan_config(&now, target)?.changes;
        let mut notes = Vec::new();
        let changed: Vec<u16> = plan.changes.iter().filter_map(|c| c.port).collect();
        let read = fpp_body(&now.doc);
        if !plan.expect.is_null() && read != plan.expect {
            left.push(mismatch(
                "readBack".to_string(),
                None,
                String::new(),
                "Pixel string outputs",
                "as sent".to_string(),
                "read back differently".to_string(),
            ));
        }
        let current = fpp_setup(&read);
        if let Some(driver) = fpp_driver(&read) {
            for output in read["channelOutputs"][driver]["outputs"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let number =
                    u16::try_from(int_field(output, "portNumber").saturating_add(1)).unwrap_or(u16::MAX);
                let real = output["virtualStrings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|vs| fpp_real(vs));
                for (i, vs) in real.enumerate() {
                    if let Some(problem) = fpp_load_problem(vs) {
                        if !changed.contains(&number) {
                            notes.push(format!(
                                "Port {number} string {} ({}): fppd won't load it ({problem}). PixelFlow didn't change that port; fix it on the FPP's page.",
                                i + 1,
                                str_field(vs, "description")
                            ));
                            continue;
                        }
                        let name = current
                            .port(number)
                            .and_then(|p| p.strings.get(i))
                            .map_or("", |s| s.name.as_str());
                        let subject = if name.is_empty() {
                            format!("String {}", i + 1)
                        } else {
                            format!("String {} · {name}", i + 1)
                        };
                        left.push(mismatch(
                            format!("{}/load", string_key(number, i)),
                            Some(number),
                            subject,
                            "fppd",
                            "loads".to_string(),
                            format!("won't load: {problem}"),
                        ));
                    }
                }
            }
        }
        Ok(Verification { left, notes })
    }

    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError> {
        let mut body = fpp_body(&snapshot.doc);
        fpp_clean(&mut body);
        let reply = post(
            http,
            host,
            &Write {
                path: snapshot.path.clone(),
                body,
                what: "the previous pixel string outputs".to_string(),
            },
        )?;
        self.check_reply(host, &reply)
    }

    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool {
        a.path == b.path && fpp_body(&a.doc) == fpp_body(&b.doc)
    }

    /// Put back saves the snapshot with FPP-9-safe names, so that's what it reads back as.
    fn restored(&self, now: &Snapshot, snapshot: &Snapshot) -> bool {
        let mut was = fpp_body(&snapshot.doc);
        fpp_clean(&mut was);
        now.path == snapshot.path && fpp_body(&now.doc) == was
    }

    fn sent_message(&self) -> String {
        "Saved, and reading it back, the FPP's pixel outputs match your show. The lights use them once FPP's player (fppd) restarts: restart it from the FPP's own page, then check its warnings.".to_string()
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

/// A save's body: only `hw.led` (whole: WLED resets its frame rate and white mode when they're
/// missing), `light` (whole: older WLEDs turn color gamma off when it's missing), `if.live` when
/// given, and `nw.linked_remote` (WLED 16 clears ESP-NOW remotes a save leaves out). Without
/// `ins`, WLED doesn't set its outputs up again.
fn wled_body(cfg: &Value, led: Value, live: Option<Value>) -> Value {
    let mut body = json!({ "hw": { "led": led } });
    if let Some(light) = cfg.get("light") {
        body["light"] = light.clone();
    }
    if let Some(live) = live {
        body["if"] = json!({ "live": live });
    }
    if let Some(remotes) = cfg["nw"].get("linked_remote") {
        body["nw"] = json!({ "linked_remote": remotes });
    }
    body
}

/// Whether an LED output drives pixels, and whether they're RGBW.
fn wled_bus_is_pixels(bus: &Value) -> (bool, bool) {
    let kind = bus["type"].as_i64().unwrap_or(22);
    let extra_white = [18, 19, 21, 28, 32, 34].contains(&kind);
    let pixels = ((16..=39).contains(&kind) || (48..=63).contains(&kind)) && !extra_white;
    (pixels, [29, 30, 31].contains(&kind))
}

/// What kind of output a non-pixel one is, for messages.
fn wled_bus_kind(bus: &Value) -> &'static str {
    match bus["type"].as_i64().unwrap_or(22) {
        40..=47 => " (an on/off or PWM output)",
        80..=95 => " (a network output)",
        _ => "",
    }
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
        left_alone: Vec::new(),
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
    mismatch(id.to_string(), port, subject, what, before, after)
}

/// An LED range `[start, start + len)`.
fn wled_range(bus: &Value) -> (i64, i64) {
    let start = bus["start"].as_i64().unwrap_or(0);
    (start, start + bus["len"].as_i64().unwrap_or(1).max(0))
}

fn overlaps(a: (i64, i64), b: (i64, i64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// What reading an output back compares: everything WLED might change or drop.
fn wled_bus_fields(bus: &Value) -> [Value; 5] {
    [
        bus["type"].clone(),
        bus["pin"].clone(),
        bus["start"].clone(),
        bus["len"].clone(),
        bus["order"].clone(),
    ]
}

impl DeviceAdapter for WledAdapter {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Wled
    }

    fn probe(&self, http: &dyn Http, host: &str) -> Result<Device, DeviceError> {
        wled::probe(http, host)
    }

    /// WLED's MAC address (`mac` in `/json/info`).
    fn identity(&self, http: &dyn Http, host: &str) -> Result<DeviceIdentity, DeviceError> {
        let info = get_json(http, host, "/json/info")?;
        let id = str_field(&info, "mac").trim().to_ascii_lowercase();
        if id.is_empty() {
            return Err(DeviceError::bad(host, "/json/info", "it gives no MAC address"));
        }
        Ok(DeviceIdentity {
            kind: DeviceKind::Wled,
            id,
            name: str_field(&info, "name").to_string(),
        })
    }

    fn restore_changes(&self, now: &Snapshot, copy: &Snapshot) -> Vec<Change> {
        let outputs = |s: &Snapshot| s.doc["hw"]["led"]["ins"].as_array().cloned().unwrap_or_default();
        let (ours, theirs) = (outputs(now), outputs(copy));
        let text = |bus: Option<&Value>, other: Option<&Value>| match bus {
            None => "None".to_string(),
            Some(bus) => {
                let mut text = format!(
                    "{} LEDs from {}",
                    bus["len"].as_i64().unwrap_or(0),
                    bus["start"].as_i64().unwrap_or(0)
                );
                if other.is_some_and(|o| {
                    o["order"] != bus["order"] || o["type"] != bus["type"] || o["pin"] != bus["pin"]
                }) {
                    text.push_str(&format!(
                        ", order {}, type {}, pin {}",
                        bus["order"], bus["type"], bus["pin"]
                    ));
                }
                text
            }
        };
        let mut rows = Vec::new();
        for i in 0..ours.len().max(theirs.len()) {
            let (a, b) = (ours.get(i), theirs.get(i));
            if a.map(wled_bus_fields) == b.map(wled_bus_fields) {
                continue;
            }
            let number = u16::try_from(i + 1).unwrap_or(u16::MAX);
            rows.push(mismatch(
                format!("port{number}/restore"),
                Some(number),
                format!("Output {number}"),
                "Output",
                text(a, b),
                text(b, a),
            ));
        }
        if now.doc["if"]["live"] != copy.doc["if"]["live"] {
            rows.push(mismatch(
                "input/restore".to_string(),
                None,
                String::new(),
                "Receive settings",
                "as they are now".to_string(),
                "as in the copy".to_string(),
            ));
        }
        rows
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
        let mut problems = Vec::new();
        let current = wled_setup(cfg);
        let buses = cfg["hw"]["led"]["ins"].as_array().cloned().unwrap_or_default();
        // What the show asks of each output WLED has, and which outputs PixelFlow sets up.
        let mut wanted = current.clone();
        let mut owned = vec![false; buses.len()];
        // Owned outputs in the order the show sends their data (its first channel for each).
        let mut data_order: Vec<(u32, usize)> = Vec::new();
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
            data_order.push((string.start.unwrap_or(u32::MAX), index));
            string.start = None;
            wanted.ports[index].strings = vec![string];
            owned[index] = true;
        }
        for (i, port) in current.ports.iter().enumerate() {
            if !port.strings.is_empty() && !owned[i] {
                notes.push(format!(
                    "Output {} isn't wired in your show; PixelFlow leaves it as it is.",
                    port.number
                ));
            }
        }
        let mut changes = diff_ports(&current, &wanted, Direction::ToDevice);

        // The outputs PixelFlow sets up get the show's lengths and orders, back to back from LED
        // 0 in the show's order; every other output keeps its place.
        let mut ins = buses.clone();
        let mut start = 0i64;
        let mut moved = false;
        data_order.sort_unstable();
        for &(_, i) in &data_order {
            let bus = &mut ins[i];
            let number = u16::try_from(i + 1).unwrap_or(u16::MAX);
            let string = &wanted.ports[i].strings[0];
            bus["len"] = json!(string.pixels);
            if let Some((code, _)) = string.color_order.and_then(wled_order_code) {
                let upper = bus["order"].as_i64().unwrap_or(0) & 0xF0;
                bus["order"] = json!(upper | code);
            }
            let was = bus["start"].as_i64().unwrap_or(0);
            if was != start {
                moved = true;
                let mut change = setting(
                    &format!("port{number}/firstLed"),
                    Some(number),
                    format!("Output {number}"),
                    "First LED",
                    was.to_string(),
                    start.to_string(),
                );
                change.warning = Some(MOVES_PIXELS.to_string());
                changes.push(change);
            }
            bus["start"] = json!(start);
            start += i64::from(string.pixels);
        }
        // Outputs PixelFlow doesn't set up must not end up under ones it does.
        for (j, other) in ins.iter().enumerate() {
            if owned[j] || other["len"].as_i64().unwrap_or(0) <= 0 {
                continue;
            }
            let theirs = wled_range(other);
            if let Some(k) = (0..ins.len()).find(|&k| owned[k] && overlaps(wled_range(&ins[k]), theirs)) {
                problems.push(format!(
                    "Output {}{} isn't set up by PixelFlow but uses LEDs {}–{}, which output {} would need. Wire it in your show, or move it in WLED's LED settings first.",
                    j + 1,
                    wled_bus_kind(other),
                    theirs.0,
                    theirs.1 - 1,
                    k + 1
                ));
            }
        }
        // WLED's color order overrides (by LED number) win over an output's own order.
        let overrides: Vec<(i64, i64)> = cfg["hw"]["com"]
            .as_array()
            .into_iter()
            .flatten()
            .map(wled_range)
            .collect();
        for change in changes.iter_mut().filter(|c| c.kind == ChangeKind::ColorOrder) {
            let Some(index) = change.port.and_then(|p| usize::from(p).checked_sub(1)) else {
                continue;
            };
            let range = wled_range(&ins[index]);
            if let Some(o) = overrides.iter().find(|o| overlaps(**o, range)) {
                change.warning = Some(format!(
                    "WLED has a color order override for LEDs {}–{}, and that override wins over this. Remove it in WLED's LED settings (Color Order Override) for this to take effect.",
                    o.0,
                    o.1 - 1
                ));
            }
        }
        if moved && !overrides.is_empty() {
            notes.push(
                "WLED's color order overrides are set by LED number, so check them after the outputs move."
                    .to_string(),
            );
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
                notes.push(
                    "WLED starts listening on a new port only after it restarts: reboot it from its own page after sending."
                        .to_string(),
                );
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
            let widths: Vec<u8> = (0..ins.len())
                .filter(|&i| owned[i])
                .map(|i| wanted.ports[i].strings[0].channels_per_pixel)
                .collect();
            let rgbw = widths.contains(&4);
            if rgbw && widths.contains(&3) {
                notes.push(
                    "WLED uses one sACN mode for all its outputs, so RGB and RGBW outputs together won't line up: PixelFlow sets Multi RGBW, and the RGB outputs' colors will be off."
                        .to_string(),
                );
            }
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
        if changes.is_empty() {
            return Ok(ConfigPlan::nothing(notes));
        }
        let i2c = cfg["hw"]["if"]["i2c-pin"]
            .as_array()
            .is_some_and(|pins| pins.iter().any(|p| p.as_i64().is_some_and(|p| p >= 0)));
        if i2c {
            notes.push(
                "This WLED has I2C pins set. On an ESP32, WLED can turn I2C off when its settings are saved: check its I2C pins (and any I2C usermods) after sending."
                    .to_string(),
            );
        }
        let mut led = cfg["hw"]["led"].clone();
        if outputs_change {
            led["ins"] = Value::Array(ins.clone());
            led["total"] = json!(ins.iter().map(|b| b["len"].as_i64().unwrap_or(0)).sum::<i64>());
        } else if let Some(map) = led.as_object_mut() {
            map.remove("ins");
        }
        let body = wled_body(cfg, led, input_change.then(|| new_live.clone()));
        let writes = if problems.is_empty() {
            vec![Write {
                path: snapshot.path.clone(),
                body,
                what: "the LED outputs and receive settings".to_string(),
            }]
        } else {
            Vec::new()
        };
        Ok(ConfigPlan {
            changes,
            notes,
            problems,
            writes,
            expect: json!({ "ins": ins, "live": new_live }),
        })
    }

    fn verify_config(
        &self,
        http: &dyn Http,
        host: &str,
        target: &Setup,
        plan: &ConfigPlan,
    ) -> Result<Verification, DeviceError> {
        // WLED sets new outputs up on its next loop; read again once if they haven't shown yet.
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
            // Every output as written, field by field: WLED drops one it can't drive.
            let written = plan.expect["ins"].as_array().cloned().unwrap_or_default();
            let read = now.doc["hw"]["led"]["ins"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (i, bus) in written.iter().enumerate() {
                let number = u16::try_from(i + 1).unwrap_or(u16::MAX);
                let sent = format!(
                    "{} LEDs from {}",
                    bus["len"].as_i64().unwrap_or(0),
                    bus["start"].as_i64().unwrap_or(0)
                );
                let after = match read.get(i) {
                    None => "missing: WLED dropped it".to_string(),
                    Some(got) if wled_bus_fields(got) != wled_bus_fields(bus) => format!(
                        "{} LEDs from {}",
                        got["len"].as_i64().unwrap_or(0),
                        got["start"].as_i64().unwrap_or(0)
                    ),
                    Some(_) => continue,
                };
                left.push(mismatch(
                    format!("port{number}/readBack"),
                    Some(number),
                    format!("Output {number}"),
                    "Read back",
                    sent,
                    after,
                ));
            }
            if read.len() > written.len() && !written.is_empty() {
                left.push(mismatch(
                    "readBack/extra".to_string(),
                    None,
                    String::new(),
                    "LED outputs",
                    written.len().to_string(),
                    read.len().to_string(),
                ));
            }
            if !plan.expect["live"].is_null() && now.doc["if"]["live"] != plan.expect["live"] {
                left.push(mismatch(
                    "input/readBack".to_string(),
                    None,
                    String::new(),
                    "Receive settings",
                    "as sent".to_string(),
                    "read back differently".to_string(),
                ));
            }
            if left.is_empty() {
                break;
            }
        }
        Ok(Verification {
            left,
            notes: Vec::new(),
        })
    }

    fn restore_config(&self, http: &dyn Http, host: &str, snapshot: &Snapshot) -> Result<(), DeviceError> {
        let cfg = &snapshot.doc;
        let body = wled_body(cfg, cfg["hw"]["led"].clone(), Some(cfg["if"]["live"].clone()));
        post(
            http,
            host,
            &Write {
                path: snapshot.path.clone(),
                body,
                what: "the previous setup".to_string(),
            },
        )?;
        // Give WLED a moment to set its outputs up again before it's read back.
        std::thread::sleep(self.settle);
        Ok(())
    }

    fn same_setup(&self, a: &Snapshot, b: &Snapshot) -> bool {
        let outputs = |s: &Snapshot| -> Vec<[Value; 5]> {
            s.doc["hw"]["led"]["ins"]
                .as_array()
                .into_iter()
                .flatten()
                .map(wled_bus_fields)
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
    /// Nothing was sent, or the controller refused before changing anything.
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
    /// Worth knowing, but not caused by this send.
    pub notes: Vec<String>,
}

/// A send's report, and the snapshot taken just before it (to put back). The snapshot is only
/// given when something may have changed on the controller.
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
            notes: Vec::new(),
        },
        snapshot: None,
    }
}

/// A first save refused outright (a 4xx, or 503 busy) changed nothing.
fn refused_outright(failure: &ApplyError) -> Option<u16> {
    match failure.error {
        DeviceError::Http { status, .. }
            if failure.done == 0 && ((400..500).contains(&status) || status == 503) =>
        {
            Some(status)
        }
        _ => None,
    }
}

/// Sends the show's setup `target` to the controller, as shown to the user: `shown` is the
/// snapshot the plan was made from and `shown_changes` the ids of the rows they saw. Takes a new
/// snapshot first; sends nothing if it can't be read or differs from `shown`, or if the plan has
/// problems. Then saves, reads back, and reports.
pub fn send_setup(
    adapter: &dyn DeviceAdapter,
    http: &dyn Http,
    host: &str,
    shown: &Snapshot,
    target: &Setup,
    shown_changes: &[String],
) -> SendOutcome {
    send_setup_with(adapter, http, host, shown, target, shown_changes, &mut |_| Ok(()))
}

/// [`send_setup`], handing the snapshot to `keep` just before anything is written (to keep it
/// somewhere safe). If `keep` fails, nothing is sent.
pub fn send_setup_with(
    adapter: &dyn DeviceAdapter,
    http: &dyn Http,
    host: &str,
    shown: &Snapshot,
    target: &Setup,
    shown_changes: &[String],
    keep: &mut dyn FnMut(&Snapshot) -> Result<(), String>,
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
    if !plan.problems.is_empty() {
        return refused(format!("Nothing was sent. {}", plan.problems.join(" ")));
    }
    if plan.writes.is_empty() {
        return refused("The controller already matches your show; nothing was sent.".to_string());
    }
    if let Err(e) = keep(&snapshot) {
        return refused(format!(
            "Nothing was sent: PixelFlow couldn't keep a copy of the controller's setup first ({e})."
        ));
    }
    if let Err(failure) = adapter.apply_config(http, host, &plan) {
        if let Some(status) = refused_outright(&failure) {
            let pin = if status == 401 || status == 403 {
                " If its settings are locked with a PIN, unlock them first."
            } else {
                ""
            };
            return refused(format!(
                "{host} refused the new setup (HTTP {status}), so nothing was changed.{pin}"
            ));
        }
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
                notes: Vec::new(),
            },
            snapshot: Some(snapshot),
        };
    }
    let report = match adapter.verify_config(http, host, target, &plan) {
        Ok(found) if found.left.is_empty() => SendReport {
            status: SendStatus::Sent,
            message: adapter.sent_message(),
            mismatches: Vec::new(),
            can_restore: true,
            notes: found.notes,
        },
        Ok(found) => SendReport {
            status: SendStatus::Mismatch,
            message: "Sent, but reading it back, the controller's setup doesn't match your show.".to_string(),
            mismatches: found.left,
            can_restore: true,
            notes: found.notes,
        },
        Err(e) => SendReport {
            status: SendStatus::Failed,
            message: format!(
                "Sent, but PixelFlow can't read the setup back, so it may not have been saved properly. {e}"
            ),
            mismatches: Vec::new(),
            can_restore: true,
            notes: Vec::new(),
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
        Ok(now) if adapter.restored(&now, snapshot) => RestoreReport {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpp_text_has_nothing_stripslashes_would_break() {
        assert_eq!(fpp_text("12\" Star \\ big\n"), "12'' Star / big");
        let mut doc = json!({"a": ["x\"y", {"b": "ok"}], "c": 1});
        assert_eq!(fpp_clean(&mut doc), 1);
        assert_eq!(doc["a"][0], "x''y");
        assert!(!doc.to_string().contains('\\'));
    }
}
