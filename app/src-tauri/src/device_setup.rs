//! "Compare with this device" and "Send setup to this device…": a controller in the show side by
//! side with the device itself. Comparing only reads; taking differences into the show is one undo
//! step. Sending changes the device, so it happens only from the user's Send click after the
//! changes were shown, and only if the device still reads as it did then.
//!
//! Before the first write, a copy of the device's setup is kept on disk, filed under the device
//! itself (its FPP uuid or WLED MAC), not its address. The copy kept is the *oldest* one not yet
//! put back: a later send doesn't replace it, so the setup from before PixelFlow first changed the
//! device (the last one known to work) is never lost to a half-written one. It stays until it's
//! put back or the user forgets it, across restarts. Put back first identifies the device at the
//! address and refuses another device, then shows what it will change and writes only when those
//! rows are confirmed.

use crate::devices::off_thread;
use crate::{AppState, Reply};
use pf_devices::adapter::{
    DeviceAdapter, DeviceIdentity, RestoreReport, SendReport, Snapshot, adapter_for, restore_setup,
    send_setup_with,
};
use pf_devices::setup::{Change, Setup, compare, one_string_per_port, show_setup, take_from_device};
use pf_devices::{Device, DeviceConfig, DeviceKind, Http};
use pf_engine::{Edit, ShowSnapshot};
use pf_model::{AdapterKind, Controller, PropId, Show};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tauri::State;

/// What was last read from each device (by address), for taking differences and sending.
#[derive(Default)]
pub(crate) struct SetupSessions {
    compared: Mutex<HashMap<String, (Device, DeviceConfig)>>,
    sends: Mutex<HashMap<String, SendSession>>,
    restores: Mutex<HashMap<String, RestoreSession>>,
    /// Controllers a send or a Put back is running for: one at a time each.
    busy: Mutex<HashSet<String>>,
    copies: Arc<RestorePoints>,
}

impl SetupSessions {
    pub(crate) fn in_dir(dir: PathBuf) -> Self {
        Self {
            copies: Arc::new(RestorePoints {
                dir: Some(dir),
                ..RestorePoints::default()
            }),
            ..Self::default()
        }
    }
}

/// A send shown to the user: the device, the snapshot and target it was planned from, and the
/// rows they saw.
struct SendSession {
    identity: DeviceIdentity,
    shown: Snapshot,
    target: Setup,
    change_ids: Vec<String>,
}

/// A Put back shown to the user: the device, what it held then (when it could be read), and the
/// rows they saw.
struct RestoreSession {
    identity: DeviceIdentity,
    shown: Option<Snapshot>,
    change_ids: Vec<String>,
}

/// A controller's setup from before PixelFlow changed it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestorePoint {
    identity: DeviceIdentity,
    device_name: String,
    taken_at_ms: u64,
    snapshot: Snapshot,
}

/// What a plan says about the kept copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestorePointInfo {
    /// What forgetting it takes.
    key: String,
    device_name: String,
    /// The address it was read from.
    address: String,
    taken_at_ms: u64,
}

impl From<&RestorePoint> for RestorePointInfo {
    fn from(point: &RestorePoint) -> Self {
        Self {
            key: point.identity.key(),
            device_name: point.device_name.clone(),
            address: point.snapshot.address.clone(),
            taken_at_ms: point.taken_at_ms,
        }
    }
}

/// The kept copies, by device ([`DeviceIdentity::key`]): in memory, and as files in `dir` when
/// there is one. Keys are distinct for distinct devices, so no copy overwrites another's.
#[derive(Default)]
pub(crate) struct RestorePoints {
    dir: Option<PathBuf>,
    cache: Mutex<HashMap<String, Option<RestorePoint>>>,
}

/// Whether `key` is a key a device has ([`DeviceIdentity::key`]): its kind, then its id in hex.
/// A key names a file, and one comes back from the window to forget a copy, so anything else
/// (a path, say) names no file at all.
fn is_device_key(key: &str) -> bool {
    key.split_once('-').is_some_and(|(kind, id)| {
        matches!(kind, "fpp" | "falcon" | "wled") && id.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

impl RestorePoints {
    fn file(&self, key: &str) -> Option<PathBuf> {
        if !is_device_key(key) {
            return None;
        }
        Some(self.dir.as_ref()?.join(format!("{key}.json")))
    }

    fn get(&self, identity: &DeviceIdentity) -> Option<RestorePoint> {
        let key = identity.key();
        let mut cache = lock(&self.cache);
        cache
            .entry(key.clone())
            .or_insert_with(|| {
                let text = std::fs::read_to_string(self.file(&key)?).ok()?;
                serde_json::from_str::<RestorePoint>(&text)
                    .ok()
                    .filter(|p| p.identity.same_device(identity))
            })
            .clone()
    }

    /// Keeps `point` unless a copy of that device is already kept (the older one is the one to
    /// put back).
    fn keep_oldest(&self, point: RestorePoint) -> Result<(), String> {
        if self.get(&point.identity).is_some() {
            return Ok(());
        }
        let key = point.identity.key();
        if let Some(file) = self.file(&key) {
            let write = || -> std::io::Result<()> {
                std::fs::create_dir_all(file.parent().expect("a folder"))?;
                let partial = file.with_extension("json.partial");
                std::fs::write(&partial, serde_json::to_vec_pretty(&point)?)?;
                std::fs::rename(&partial, &file)
            };
            write().map_err(|e| format!("it couldn't be saved on disk: {e}"))?;
        }
        lock(&self.cache).insert(key, Some(point));
        Ok(())
    }

    fn forget(&self, key: &str) {
        if !is_device_key(key) {
            return;
        }
        if let Some(file) = self.file(key) {
            let _ = std::fs::remove_file(file);
        }
        lock(&self.cache).insert(key.to_string(), None);
    }
}

/// Marks a send or Put back to `address` as running until dropped.
struct Running<'a> {
    busy: &'a Mutex<HashSet<String>>,
    address: String,
}

impl<'a> Running<'a> {
    fn start(busy: &'a Mutex<HashSet<String>>, address: &str) -> Reply<Self> {
        if !lock(busy).insert(address.to_string()) {
            return Err(
                "PixelFlow is already sending to this controller. Wait for it to finish.".to_string(),
            );
        }
        Ok(Self {
            busy,
            address: address.to_string(),
        })
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        lock(self.busy).remove(&self.address);
    }
}

fn identity_of(adapter: &dyn DeviceAdapter, http: &dyn Http, host: &str) -> Reply<DeviceIdentity> {
    adapter.identity(http, host).map_err(|e| {
        format!("PixelFlow can't tell which controller answers at {host}, so it won't change it. {e}")
    })
}

fn other_device(host: &str, now: &DeviceIdentity, expected: &DeviceIdentity) -> String {
    format!(
        "The controller at {host} is now {} ({}), not {}, the one this was planned for. Nothing was changed.",
        if now.name.is_empty() {
            "another device"
        } else {
            now.name.as_str()
        },
        now.id,
        if expected.name.is_empty() {
            expected.id.as_str()
        } else {
            expected.name.as_str()
        },
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The show's controller for a device at `address` (one with ports, not a placeholder first).
fn controller_at<'a>(show: &'a Show, address: &str) -> Option<&'a Controller> {
    let mut at = show.controllers.iter().filter(|c| c.address == address);
    let first = at.clone().next();
    at.find(|c| !pf_devices::is_placeholder(c)).or(first)
}

fn hint(controller: Option<&Controller>) -> Option<DeviceKind> {
    match controller?.adapter {
        AdapterKind::Fpp => Some(DeviceKind::Fpp),
        AdapterKind::Falcon => Some(DeviceKind::Falcon),
        AdapterKind::Wled => Some(DeviceKind::Wled),
        AdapterKind::Generic => None,
    }
}

fn controller_for(show: &Show, address: &str) -> Reply<Controller> {
    controller_at(show, address)
        .cloned()
        .ok_or_else(|| not_in_show(address))
}

fn not_in_show(address: &str) -> String {
    format!("No controller at {address} is in your show. Add it from the Controllers screen first.")
}

/// What differs between the show's controller and the device.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceComparison {
    device: Device,
    controller_name: String,
    changes: Vec<Change>,
    notes: Vec<String>,
}

/// Reads the device and compares it with the show's controller at its address (changes nothing).
#[tauri::command]
pub(crate) async fn compare_device(state: State<'_, AppState>, address: String) -> Reply<DeviceComparison> {
    let kind = {
        let engine = state.engine();
        let controller = controller_at(engine.show(), &address).ok_or_else(|| not_in_show(&address))?;
        hint(Some(controller))
    };
    let http = Arc::clone(&state.devices.config_http);
    let host = address.clone();
    let (device, config) = off_thread(move || {
        let device = pf_devices::identify(http.as_ref(), &host, kind).map_err(|e| e.to_string())?;
        let adapter = adapter_for(device.kind);
        let config = adapter
            .read_config(http.as_ref(), &host)
            .map_err(|e| e.to_string())?;
        Ok((device, config))
    })
    .await?;
    let show = state.engine().show().clone();
    let controller = controller_at(&show, &address).ok_or_else(|| not_in_show(&address))?;
    let comparison = compare(&show, controller, device.kind, &config);
    let result = DeviceComparison {
        device: device.clone(),
        controller_name: controller.name.clone(),
        changes: comparison.changes,
        notes: comparison.notes,
    };
    lock(&state.devices.setup.compared).insert(address, (device, config));
    Ok(result)
}

/// Takes the picked differences from the device (as last compared) into the show, as one undo
/// step. `use_props` wires existing props to new strings, by string key.
#[tauri::command]
pub(crate) async fn take_from_device_setup(
    state: State<'_, AppState>,
    address: String,
    picks: Vec<String>,
    use_props: Option<BTreeMap<String, PropId>>,
) -> Reply<ShowSnapshot> {
    let (device, config) = lock(&state.devices.setup.compared)
        .get(&address)
        .cloned()
        .ok_or_else(|| "Compare with the controller first.".to_string())?;
    if picks.is_empty() {
        return Err("Pick at least one difference to take into your show.".to_string());
    }
    let mut engine = state.engine();
    let show = engine.show().clone();
    let controller = controller_at(&show, &address).ok_or_else(|| not_in_show(&address))?;
    let taken = take_from_device(
        &show,
        controller,
        device.kind,
        &config,
        &picks,
        &use_props.unwrap_or_default(),
    )?;
    let mut edits: Vec<Edit> = taken
        .new_props
        .into_iter()
        .map(|prop| Edit::AddProp { prop })
        .collect();
    edits.extend(
        taken
            .changed_props
            .into_iter()
            .map(|prop| Edit::UpdateProp { prop }),
    );
    edits.push(Edit::UpdateController {
        controller: taken.controller,
    });
    engine.apply(edits).map_err(|e| e.to_string())
}

/// What sending the show's setup to a device would change.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendPlan {
    device: Device,
    controller_name: String,
    /// Device (before) → show (after).
    changes: Vec<Change>,
    notes: Vec<String>,
    /// Why this can't be sent as it is (the controller wouldn't load it, for instance).
    problems: Vec<String>,
    /// What the device is busy with that sending would interrupt.
    busy: Option<String>,
    can_send: bool,
    /// Why it can't be sent, when it can't.
    reason: Option<String>,
    /// The kept copy of this device's setup, which Put back sends.
    restore_point: Option<RestorePointInfo>,
}

/// Reads the device's setup and plans sending the show's (changes nothing). The reading is kept:
/// sending checks the device still matches it.
#[tauri::command]
pub(crate) async fn plan_device_setup(state: State<'_, AppState>, address: String) -> Reply<SendPlan> {
    let show = state.engine().show().clone();
    let controller = controller_for(&show, &address)?;
    let kind = hint(Some(&controller));
    let http = Arc::clone(&state.devices.config_http);
    let copies = Arc::clone(&state.devices.setup.copies);
    let host = address.clone();
    let (device, restore_point, read) = off_thread(move || {
        let device = pf_devices::identify(http.as_ref(), &host, kind).map_err(|e| e.to_string())?;
        let adapter = adapter_for(device.kind);
        let wanted = show_setup(
            &show,
            &controller_for(&show, &host)?,
            one_string_per_port(device.kind),
        );
        if let Err(reason) = adapter.can_send() {
            return Ok((device, None, Err(reason)));
        }
        let identity = match identity_of(adapter.as_ref(), http.as_ref(), &host) {
            Ok(identity) => identity,
            Err(reason) => return Ok((device, None, Err(reason))),
        };
        let restore_point = copies.get(&identity).as_ref().map(RestorePointInfo::from);
        let snapshot = match adapter.snapshot(http.as_ref(), &host) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                let reason = format!(
                    "PixelFlow couldn't read {}'s current setup, so it won't send anything to it. {e}",
                    device.name
                );
                return Ok((device, restore_point, Err(reason)));
            }
        };
        let plan = adapter
            .plan_config(&snapshot, &wanted)
            .map_err(|e| e.to_string())?;
        let busy = adapter.status(http.as_ref(), &host).ok().and_then(|s| s.busy);
        Ok((
            device,
            restore_point,
            Ok((identity, snapshot, plan, busy, wanted)),
        ))
    })
    .await?;
    let mut sends = lock(&state.devices.setup.sends);
    match read {
        Err(reason) => {
            sends.remove(&address);
            Ok(SendPlan {
                device,
                controller_name: controller.name,
                changes: Vec::new(),
                notes: Vec::new(),
                problems: Vec::new(),
                busy: None,
                can_send: false,
                reason: Some(reason),
                restore_point,
            })
        }
        Ok((identity, snapshot, plan, busy, target)) => {
            let change_ids: Vec<String> = plan.changes.iter().map(|c| c.id.clone()).collect();
            let can_send = !plan.writes.is_empty();
            let reason = if !plan.problems.is_empty() {
                Some("PixelFlow won't send this until the problems below are fixed.".to_string())
            } else {
                (!can_send).then(|| "The controller already matches your show.".to_string())
            };
            sends.insert(
                address,
                SendSession {
                    identity,
                    shown: snapshot,
                    target,
                    change_ids,
                },
            );
            Ok(SendPlan {
                device,
                controller_name: controller.name,
                reason,
                changes: plan.changes,
                notes: plan.notes,
                problems: plan.problems,
                busy,
                can_send,
                restore_point,
            })
        }
    }
}

/// Sends the setup [`plan_device_setup`] showed: `expected` is the ids of the rows shown. Changes
/// the device, only from the user's Send click. Nothing is sent if the device at the address, the
/// show, the device's setup, or the rows changed since, or if a copy of its setup can't be kept
/// first.
#[tauri::command]
pub(crate) async fn send_device_setup(
    state: State<'_, AppState>,
    address: String,
    expected: Vec<String>,
) -> Reply<SendReport> {
    let running = Running::start(&state.devices.setup.busy, &address)?;
    let (identity, shown, target) = {
        let mut sends = lock(&state.devices.setup.sends);
        let session = sends
            .get_mut(&address)
            .ok_or_else(|| "Review what will change before sending.".to_string())?;
        if session.change_ids != expected {
            return Err("What will change isn't what was shown. Review the changes again.".to_string());
        }
        // Used up now, under the same lock: a second Send can't go out with it.
        session.change_ids = vec!["(sending)".to_string()];
        (
            session.identity.clone(),
            session.shown.clone(),
            session.target.clone(),
        )
    };
    {
        let engine = state.engine();
        let controller = controller_at(engine.show(), &address).ok_or_else(|| not_in_show(&address))?;
        if show_setup(engine.show(), controller, one_string_per_port(identity.kind)) != target {
            return Err("Your show changed since you looked. Review the changes again.".to_string());
        }
    }
    let http: Arc<dyn Http> = Arc::clone(&state.devices.config_http);
    let copies = Arc::clone(&state.devices.setup.copies);
    let host = address.clone();
    let report = off_thread(move || {
        let adapter = adapter_for(identity.kind);
        let now = identity_of(adapter.as_ref(), http.as_ref(), &host)?;
        if !now.same_device(&identity) {
            return Err(other_device(&host, &now, &identity));
        }
        let outcome = send_setup_with(
            adapter.as_ref(),
            http.as_ref(),
            &host,
            &shown,
            &target,
            &expected,
            &mut |snapshot| {
                copies.keep_oldest(RestorePoint {
                    identity: identity.clone(),
                    device_name: identity.name.clone(),
                    taken_at_ms: now_ms(),
                    snapshot: snapshot.clone(),
                })
            },
        );
        Ok(outcome.report)
    })
    .await;
    drop(running);
    report
}

/// What putting the kept copy back would change.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestorePlan {
    device: Device,
    copy: RestorePointInfo,
    /// The device now (before) → the copy (after).
    changes: Vec<Change>,
    can_restore: bool,
    /// Why it can't be put back, when it can't.
    reason: Option<String>,
}

/// Identifies the device at `address`, finds the kept copy of *its* setup, and says what putting
/// it back would change (changes nothing). Putting it back must confirm these rows.
#[tauri::command]
pub(crate) async fn plan_device_restore(state: State<'_, AppState>, address: String) -> Reply<RestorePlan> {
    let hint = hint(controller_at(state.engine().show(), &address));
    let http = Arc::clone(&state.devices.config_http);
    let copies = Arc::clone(&state.devices.setup.copies);
    let host = address.clone();
    let (identity, device, point, now, changes) = off_thread(move || {
        // With no controller at this address in the show (after a restart, say), try each kind
        // that can be sent to.
        let kinds = hint.map_or(vec![DeviceKind::Fpp, DeviceKind::Wled], |k| vec![k]);
        let device = kinds
            .iter()
            .find_map(|k| pf_devices::identify(http.as_ref(), &host, Some(*k)).ok())
            .ok_or_else(|| format!("No controller PixelFlow can set up answers at {host}."))?;
        let adapter = adapter_for(device.kind);
        let identity = identity_of(adapter.as_ref(), http.as_ref(), &host)?;
        let point = copies
            .get(&identity)
            .ok_or_else(|| "There's no earlier setup of this controller to put back.".to_string())?;
        let now = adapter.snapshot(http.as_ref(), &host).ok();
        let changes = match &now {
            Some(now) => adapter.restore_changes(now, &point.snapshot),
            None => vec![unreadable_row()],
        };
        Ok((identity, device, point, now, changes))
    })
    .await?;
    let can_restore = !changes.is_empty();
    lock(&state.devices.setup.restores).insert(
        address,
        RestoreSession {
            identity,
            shown: now,
            change_ids: changes.iter().map(|c| c.id.clone()).collect(),
        },
    );
    Ok(RestorePlan {
        device,
        copy: RestorePointInfo::from(&point),
        reason: (!can_restore).then(|| "The controller already holds the kept setup.".to_string()),
        changes,
        can_restore,
    })
}

/// The row Put back shows when the device's setup can't be read now.
fn unreadable_row() -> Change {
    Change {
        id: "restore/unreadable".to_string(),
        port: None,
        kind: pf_devices::setup::ChangeKind::Setting,
        subject: String::new(),
        what: "Setup".to_string(),
        before: "can't be read".to_string(),
        after: "as in the copy".to_string(),
        warning: Some(
            "PixelFlow can't read what the controller holds now, so it can't show each change.".to_string(),
        ),
        can_take: false,
        why_not: None,
    }
}

/// Puts the kept copy back, as [`plan_device_restore`] showed it: `expected` is the ids of the
/// rows shown. Changes the device, only from the user's click, and only if it's still the device
/// the copy came from and still holds what was shown. Once it's back, the copy is let go.
#[tauri::command]
pub(crate) async fn restore_device_setup(
    state: State<'_, AppState>,
    address: String,
    expected: Vec<String>,
) -> Reply<RestoreReport> {
    let running = Running::start(&state.devices.setup.busy, &address)?;
    let (identity, shown) = {
        let restores = lock(&state.devices.setup.restores);
        let session = restores
            .get(&address)
            .ok_or_else(|| "Look at what Put back will change first.".to_string())?;
        (session.identity.clone(), session.shown.clone())
    };
    let http = Arc::clone(&state.devices.config_http);
    let copies = Arc::clone(&state.devices.setup.copies);
    let host = address.clone();
    let shown_ids = lock(&state.devices.setup.restores)
        .get(&address)
        .map(|s| s.change_ids.clone())
        .unwrap_or_default();
    if shown_ids != expected {
        return Err("What Put back will change isn't what was shown. Look again.".to_string());
    }
    let report = off_thread(move || {
        let adapter = adapter_for(identity.kind);
        let now_identity = identity_of(adapter.as_ref(), http.as_ref(), &host)?;
        if !now_identity.same_device(&identity) {
            return Err(other_device(&host, &now_identity, &identity));
        }
        let point = copies
            .get(&identity)
            .ok_or_else(|| "There's no earlier setup of this controller to put back.".to_string())?;
        let now = adapter.snapshot(http.as_ref(), &host).ok();
        let changes = match (&now, &shown) {
            (Some(now), Some(shown)) if !adapter.same_setup(now, shown) => {
                return Err(
                    "The controller's setup changed since you looked, so nothing was put back. Look again."
                        .to_string(),
                );
            }
            (Some(now), _) => adapter.restore_changes(now, &point.snapshot),
            (None, _) => vec![unreadable_row()],
        };
        if changes.iter().map(|c| &c.id).ne(expected.iter()) {
            return Err("What Put back will change isn't what was shown. Look again.".to_string());
        }
        let report = restore_setup(adapter.as_ref(), http.as_ref(), &host, &point.snapshot);
        if report.restored {
            copies.forget(&identity.key());
        }
        Ok(report)
    })
    .await;
    lock(&state.devices.setup.restores).remove(&address);
    drop(running);
    report
}

/// Dismisses a kept copy (by its key): Put back is no longer offered for that device.
#[tauri::command]
pub(crate) fn forget_device_setup_copy(state: State<'_, AppState>, key: String) {
    state.devices.setup.copies.forget(&key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{app, app_in, call};
    use serde_json::{Value, json};
    use tauri::WebviewWindow;
    use tauri::test::MockRuntime;

    #[test]
    fn forgetting_a_copy_removes_only_a_devices_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let copies_dir = dir.path().join("device-setups");
        std::fs::create_dir(&copies_dir).unwrap();
        let copies = RestorePoints {
            dir: Some(copies_dir.clone()),
            ..RestorePoints::default()
        };
        let key = DeviceIdentity {
            kind: DeviceKind::Wled,
            id: "aa:bb:cc".into(),
            name: "Porch".into(),
        }
        .key();
        let kept = copies_dir.join(format!("{key}.json"));
        std::fs::write(&kept, "{}").unwrap();
        // Files a key from the window must never reach: beside the folder, and anywhere at all.
        let beside = dir.path().join("show.pixelflow.json");
        std::fs::write(&beside, "{}").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let far = elsewhere.path().join("other.json");
        std::fs::write(&far, "{}").unwrap();
        for key in [
            "../show.pixelflow".to_string(),
            far.with_extension("").to_string_lossy().into_owned(),
            "wled-../../show.pixelflow".to_string(),
            "wled-zz".to_string(),
            "printer-aa".to_string(),
            String::new(),
        ] {
            assert!(!is_device_key(&key), "{key}");
            copies.forget(&key);
        }
        assert!(beside.exists() && far.exists() && kept.exists());
        // The device's own key removes its copy.
        assert!(is_device_key(&key), "{key}");
        copies.forget(&key);
        assert!(!kept.exists());
    }

    /// The pixel hat's strings: port 1 has "Roof Line" (150 pixels) and "Gutter" (50).
    fn fake_hat() -> pf_devices::testing::FakeFpp {
        let mut doc: Value = serde_json::from_str(include_str!(
            "../../../crates/pf-devices/fixtures/fpp-hat/api_channel_output_co-pixelStrings.json"
        ))
        .unwrap();
        doc.as_object_mut().unwrap().remove("status");
        pf_devices::testing::FakeFpp::start().with_pixel_strings(doc)
    }

    /// Adds the device at `host` to the show as an import would (one undo step).
    fn add_device(webview: &WebviewWindow<MockRuntime>, host: &str) -> Value {
        let http = pf_devices::HttpClient::new(std::time::Duration::from_secs(5));
        let device = pf_devices::identify(&http, host, Some(pf_devices::DeviceKind::Fpp)).unwrap();
        let config = pf_devices::read_config(&http, &device).unwrap();
        let plan = pf_devices::plan_import(&device, &config, &pf_model::Show::new("t"));
        let mut edits: Vec<Edit> = plan
            .props
            .into_iter()
            .map(|prop| Edit::AddProp { prop })
            .collect();
        edits.push(Edit::AddController {
            controller: plan.controller,
        });
        call(webview, "apply_edits", json!({ "edits": edits })).unwrap()
    }

    /// Resizes the line prop `name` to `nodes` pixels.
    fn resize(webview: &WebviewWindow<MockRuntime>, snapshot: &Value, name: &str, nodes: u32) -> Value {
        let mut prop = snapshot["show"]["props"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()
            .clone();
        prop["shape"]["nodes"] = json!(nodes);
        call(
            webview,
            "apply_edits",
            json!({ "edits": [{ "type": "updateProp", "prop": prop }] }),
        )
        .unwrap()
    }

    /// Puts back the kept copy as the dialog does: look at what it changes, then confirm those rows.
    fn put_back(webview: &WebviewWindow<MockRuntime>, host: &str) -> Result<Value, Value> {
        let plan = call(webview, "plan_device_restore", json!({ "address": host }))?;
        call(
            webview,
            "restore_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
    }

    fn ids(changes: &Value) -> Vec<String> {
        changes
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn comparing_takes_picked_differences_into_the_show_as_one_undo_step() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        add_device(&webview, &host);
        let error = call(
            &webview,
            "take_from_device_setup",
            json!({ "address": host, "picks": ["x"] }),
        )
        .unwrap_err();
        assert_eq!(error, json!("Compare with the controller first."));
        // On the FPP's own page, Gutter grows to 60 pixels.
        fpp.state().pixel_strings.as_mut().unwrap()["channelOutputs"][0]["outputs"][0]["virtualStrings"][1]
            ["pixelCount"] = json!(60);
        let comparison = call(&webview, "compare_device", json!({ "address": host })).unwrap();
        assert_eq!(comparison["controllerName"], "FakeFPP");
        assert_eq!(ids(&comparison["changes"]), vec!["port1/string2/pixels"]);
        assert_eq!(comparison["changes"][0]["before"], "50");
        assert_eq!(comparison["changes"][0]["after"], "60");

        let snapshot = call(
            &webview,
            "take_from_device_setup",
            json!({ "address": host, "picks": ["port1/string2/pixels"] }),
        )
        .unwrap();
        assert_eq!(snapshot["summary"]["pixels"], 210);
        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["pixels"], 200, "one undo step");
        // Comparing never writes to the device.
        assert!(fpp.state().config_writes.is_empty());
    }

    #[test]
    fn sending_a_setup_shows_it_first_then_sends_checks_and_can_put_it_back() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let original = fpp.state().pixel_strings.clone().unwrap();
        let snapshot = add_device(&webview, &host);
        let error = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": [] }),
        )
        .unwrap_err();
        assert_eq!(error, json!("Review what will change before sending."));

        let snapshot = resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(plan["canSend"], true);
        assert_eq!(ids(&plan["changes"]), vec!["port1/string2/pixels"]);
        assert!(
            plan["changes"][0]["warning"]
                .as_str()
                .unwrap()
                .contains("go dark")
        );
        assert!(fpp.state().config_writes.is_empty(), "planning sends nothing");

        let error = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ["other"] }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("isn't what was shown"),
            "{error}"
        );
        let report = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ["port1/string2/pixels"] }),
        )
        .unwrap();
        assert_eq!(report["status"], "sent", "{report}");
        let saved = fpp.state().pixel_strings.clone().unwrap();
        assert_eq!(
            saved["channelOutputs"][0]["outputs"][0]["virtualStrings"][1]["pixelCount"],
            30
        );
        // The plan is used up.
        let error = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ["port1/string2/pixels"] }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("isn't what was shown"),
            "{error}"
        );

        // A send that fails partway can be undone with one click.
        resize(&webview, &snapshot, "Gutter", 20);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        fpp.state().fail_config_writes = Some((500, "{}".to_string()));
        let report = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["canRestore"], true);
        fpp.state().fail_config_writes = None;
        // The copy kept is the one from before the first send: the last setup known to work.
        assert_ne!(fpp.state().pixel_strings.clone().unwrap(), original);
        let restored = put_back(&webview, &host).unwrap();
        assert_eq!(restored["restored"], true, "{restored}");
        assert_eq!(fpp.state().pixel_strings.clone().unwrap(), original);
        assert_ne!(saved, original);
        // Once it's back on the device, the copy is done with.
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(plan["restorePoint"], json!(null), "{plan}");
        let error = call(&webview, "plan_device_restore", json!({ "address": host })).unwrap_err();
        assert_eq!(
            error,
            json!("There's no earlier setup of this controller to put back.")
        );
    }

    #[test]
    fn put_back_shows_what_it_changes_and_needs_those_rows_confirmed() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        let restore = call(&webview, "plan_device_restore", json!({ "address": host })).unwrap();
        assert_eq!(restore["canRestore"], true, "{restore}");
        assert_eq!(ids(&restore["changes"]), vec!["port1/string2/pixels"]);
        assert_eq!(restore["changes"][0]["before"], "30");
        assert_eq!(restore["changes"][0]["after"], "50");
        let writes = fpp.state().config_writes.len();
        let error = call(
            &webview,
            "restore_device_setup",
            json!({ "address": host, "expected": ["other"] }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("isn't what was shown"),
            "{error}"
        );
        assert_eq!(fpp.state().config_writes.len(), writes, "nothing written");
        let restored = call(
            &webview,
            "restore_device_setup",
            json!({ "address": host, "expected": ["port1/string2/pixels"] }),
        )
        .unwrap();
        assert_eq!(restored["restored"], true, "{restored}");
    }

    #[test]
    fn put_back_refuses_a_different_device_at_the_same_address() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        let restore = call(&webview, "plan_device_restore", json!({ "address": host })).unwrap();
        // Another FPP now answers at this address (between looking and confirming).
        fpp.state().uuid = "M1-SOMEONE-ELSE".to_string();
        fpp.state().host_name = "OtherFPP".to_string();
        let writes = fpp.state().config_writes.len();
        let error = call(
            &webview,
            "restore_device_setup",
            json!({ "address": host, "expected": ids(&restore["changes"]) }),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("OtherFPP"), "{error}");
        assert_eq!(fpp.state().config_writes.len(), writes, "nothing written");
        // Looking again, the copy isn't this device's, so it isn't offered.
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(plan["restorePoint"], json!(null));
        let error = call(&webview, "plan_device_restore", json!({ "address": host })).unwrap_err();
        assert_eq!(
            error,
            json!("There's no earlier setup of this controller to put back.")
        );
    }

    #[test]
    fn copies_of_two_devices_are_kept_apart() {
        let (_app, webview, _dir) = app();
        let (a, b) = (fake_hat(), fake_hat());
        b.state().uuid = "M1-FAKE-0002".to_string();
        b.state().pixel_strings.as_mut().unwrap()["channelOutputs"][0]["outputs"][0]["virtualStrings"][1]["description"] =
            json!("Gutter B");
        for (fpp, gutter) in [(&a, "Gutter"), (&b, "Gutter B")] {
            let host = fpp.address().to_string();
            let snapshot = add_device(&webview, &host);
            resize(&webview, &snapshot, gutter, 30);
            let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
            call(
                &webview,
                "send_device_setup",
                json!({ "address": host, "expected": ids(&plan["changes"]) }),
            )
            .unwrap();
        }
        for fpp in [&a, &b] {
            let restored = put_back(&webview, fpp.address()).unwrap();
            assert_eq!(restored["restored"], true, "{restored}");
        }
    }

    #[test]
    fn put_back_stays_after_a_send_reported_as_sent_and_across_a_restart() {
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let before = fpp.state().pixel_strings.clone().unwrap();
        let (app, webview, dir) = app_in(tempfile::tempdir().unwrap());
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(plan["restorePoint"], json!(null));
        let report = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        assert_eq!(report["status"], "sent", "{report}");
        assert_eq!(report["canRestore"], true);
        // Looking again keeps the copy from before the send, not the device as it is now.
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert!(plan["restorePoint"]["takenAtMs"].as_u64().unwrap() > 0, "{plan}");
        drop((webview, app));

        // After a restart (no show open), the copy is still there and puts the FPP back.
        let (_app, webview, _dir) = app_in(dir);
        let restored = put_back(&webview, &host).unwrap();
        assert_eq!(restored["restored"], true, "{restored}");
        assert_eq!(fpp.state().pixel_strings.clone().unwrap(), before);
    }

    #[test]
    fn a_failed_put_back_can_be_tried_again() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        fpp.state().fail_config_writes = Some((500, "{}".to_string()));
        let restored = put_back(&webview, &host).unwrap();
        assert_eq!(restored["restored"], false);
        fpp.state().fail_config_writes = None;
        let restored = put_back(&webview, &host).unwrap();
        assert_eq!(restored["restored"], true, "{restored}");
    }

    #[test]
    fn a_plan_fppd_would_not_load_cant_be_sent() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 2000);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(plan["canSend"], false);
        assert!(plan["problems"][0].as_str().unwrap().contains("1,600"), "{plan}");
        let report = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap();
        assert_eq!(report["status"], "refused");
        assert!(fpp.state().config_writes.is_empty());
    }

    #[test]
    fn nothing_is_sent_when_the_show_changed_after_the_plan_was_shown() {
        let (_app, webview, _dir) = app();
        let fpp = fake_hat();
        let host = fpp.address().to_string();
        let snapshot = add_device(&webview, &host);
        resize(&webview, &snapshot, "Gutter", 30);
        let plan = call(&webview, "plan_device_setup", json!({ "address": host })).unwrap();
        resize(&webview, &snapshot, "Gutter", 25);
        let error = call(
            &webview,
            "send_device_setup",
            json!({ "address": host, "expected": ids(&plan["changes"]) }),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("Your show changed"), "{error}");
        assert!(fpp.state().config_writes.is_empty());
    }

    #[test]
    fn a_controller_not_in_the_show_cant_be_compared() {
        let (_app, webview, _dir) = app();
        let falcon = pf_devices::testing::FALCON;
        let error = call(&webview, "compare_device", json!({ "address": falcon })).unwrap_err();
        assert!(error.as_str().unwrap().contains("No controller at"), "{error}");
    }
}
