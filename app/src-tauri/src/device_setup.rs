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

impl RestorePoints {
    fn file(&self, key: &str) -> Option<PathBuf> {
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
