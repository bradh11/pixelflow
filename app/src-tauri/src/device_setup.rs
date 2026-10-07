//! "Compare with this device" and "Send setup to this device…": a controller in the show side by
//! side with the device itself. Comparing only reads; taking differences into the show is one undo
//! step. Sending changes the device, so it happens only from the user's Send click after the
//! changes were shown, and only if the device still reads as it did then. A copy of its setup is
//! taken first and kept on disk (one per controller) until the user dismisses it or a later send
//! replaces it, so one click puts it back, even after a send that read back fine, a failed Put
//! back, or a restart.

use crate::devices::off_thread;
use crate::{AppState, Reply};
use pf_devices::adapter::{RestoreReport, SendReport, Snapshot, adapter_for, restore_setup, send_setup};
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
    /// Controllers a send or a Put back is running for: one at a time each.
    busy: Mutex<HashSet<String>>,
    copies: RestorePoints,
}

impl SetupSessions {
    pub(crate) fn in_dir(dir: PathBuf) -> Self {
        Self {
            copies: RestorePoints {
                dir: Some(dir),
                ..RestorePoints::default()
            },
            ..Self::default()
        }
    }
}

/// A send shown to the user: the snapshot and target it was planned from, and the rows they saw.
struct SendSession {
    kind: DeviceKind,
    device_name: String,
    shown: Snapshot,
    target: Setup,
    change_ids: Vec<String>,
}

/// A controller's setup from just before a send.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestorePoint {
    device_name: String,
    taken_at_ms: u64,
    snapshot: Snapshot,
}

/// What a plan says about the kept copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestorePointInfo {
    device_name: String,
    taken_at_ms: u64,
}

/// The kept copies, by address: in memory, and as files in `dir` when there is one.
#[derive(Default)]
struct RestorePoints {
    dir: Option<PathBuf>,
    cache: Mutex<HashMap<String, Option<RestorePoint>>>,
}

impl RestorePoints {
    fn file(&self, address: &str) -> Option<PathBuf> {
        let name: String = address
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Some(self.dir.as_ref()?.join(format!("{name}.json")))
    }

    fn get(&self, address: &str) -> Option<RestorePoint> {
        let mut cache = lock(&self.cache);
        cache
            .entry(address.to_string())
            .or_insert_with(|| {
                let text = std::fs::read_to_string(self.file(address)?).ok()?;
                serde_json::from_str::<RestorePoint>(&text)
                    .ok()
                    .filter(|p| p.snapshot.address == address)
            })
            .clone()
    }

    fn save(&self, address: &str, point: RestorePoint) -> Result<(), String> {
        if let Some(file) = self.file(address) {
            let write = || -> std::io::Result<()> {
                std::fs::create_dir_all(file.parent().expect("a folder"))?;
                let partial = file.with_extension("json.partial");
                std::fs::write(&partial, serde_json::to_vec_pretty(&point)?)?;
                std::fs::rename(&partial, &file)
            };
            write().map_err(|e| {
                format!("PixelFlow couldn't keep the copy of the controller's setup on disk: {e}")
            })?;
        }
        lock(&self.cache).insert(address.to_string(), Some(point));
        Ok(())
    }

    fn forget(&self, address: &str) {
        if let Some(file) = self.file(address) {
            let _ = std::fs::remove_file(file);
        }
        lock(&self.cache).insert(address.to_string(), None);
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
    /// The copy of its setup kept from before an earlier send, which Put back sends.
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
    let host = address.clone();
    let (device, read) = off_thread(move || {
        let device = pf_devices::identify(http.as_ref(), &host, kind).map_err(|e| e.to_string())?;
        let adapter = adapter_for(device.kind);
        let wanted = show_setup(
            &show,
            &controller_for(&show, &host)?,
            one_string_per_port(device.kind),
        );
        if let Err(reason) = adapter.can_send() {
            return Ok((device, Err(reason)));
        }
        let snapshot = match adapter.snapshot(http.as_ref(), &host) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                let reason = format!(
                    "PixelFlow couldn't read {}'s current setup, so it won't send anything to it. {e}",
                    device.name
                );
                return Ok((device, Err(reason)));
            }
        };
        let plan = adapter
            .plan_config(&snapshot, &wanted)
            .map_err(|e| e.to_string())?;
        let busy = adapter.status(http.as_ref(), &host).ok().and_then(|s| s.busy);
        Ok((device, Ok((snapshot, plan, busy, wanted))))
    })
    .await?;
    let restore_point = state
        .devices
        .setup
        .copies
        .get(&address)
        .map(|p| RestorePointInfo {
            device_name: p.device_name,
            taken_at_ms: p.taken_at_ms,
        });
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
        Ok((snapshot, plan, busy, target)) => {
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
                    kind: device.kind,
                    device_name: device.name.clone(),
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
/// the device, only from the user's Send click. Nothing is sent if the show, the device, or the
/// rows changed since, or if the device's setup can't be read first to keep a copy.
#[tauri::command]
pub(crate) async fn send_device_setup(
    state: State<'_, AppState>,
    address: String,
    expected: Vec<String>,
) -> Reply<SendReport> {
    let running = Running::start(&state.devices.setup.busy, &address)?;
    let (kind, device_name, shown, target) = {
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
            session.kind,
            session.device_name.clone(),
            session.shown.clone(),
            session.target.clone(),
        )
    };
    {
        let engine = state.engine();
        let controller = controller_at(engine.show(), &address).ok_or_else(|| not_in_show(&address))?;
        if show_setup(engine.show(), controller, one_string_per_port(kind)) != target {
            return Err("Your show changed since you looked. Review the changes again.".to_string());
        }
    }
    let http: Arc<dyn Http> = Arc::clone(&state.devices.config_http);
    let host = address.clone();
    let outcome = off_thread(move || {
        let adapter = adapter_for(kind);
        Ok(send_setup(
            adapter.as_ref(),
            http.as_ref(),
            &host,
            &shown,
            &target,
            &expected,
        ))
    })
    .await?;
    let mut report = outcome.report;
    if let Some(snapshot) = outcome.snapshot {
        let point = RestorePoint {
            device_name,
            taken_at_ms: now_ms(),
            snapshot,
        };
        if let Err(e) = state.devices.setup.copies.save(&address, point) {
            report.message = format!("{} {e}", report.message);
        }
    }
    drop(running);
    Ok(report)
}

/// Puts back the setup the device had just before the last send. Changes the device, only from
/// the user's click.
#[tauri::command]
pub(crate) async fn restore_device_setup(
    state: State<'_, AppState>,
    address: String,
) -> Reply<RestoreReport> {
    let point = state
        .devices
        .setup
        .copies
        .get(&address)
        .ok_or_else(|| "There's no earlier setup to put back.".to_string())?;
    let running = Running::start(&state.devices.setup.busy, &address)?;
    let http = Arc::clone(&state.devices.config_http);
    let host = address.clone();
    let report = off_thread(move || {
        let adapter = adapter_for(point.snapshot.kind);
        Ok(restore_setup(
            adapter.as_ref(),
            http.as_ref(),
            &host,
            &point.snapshot,
        ))
    })
    .await;
    drop(running);
    report
}

/// Dismisses the kept copy of a controller's setup: Put back is no longer offered for it.
#[tauri::command]
pub(crate) fn forget_device_setup_copy(state: State<'_, AppState>, address: String) {
    state.devices.setup.copies.forget(&address);
}
