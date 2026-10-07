//! "Compare with this device" and "Send setup to this device…": a controller in the show side by
//! side with the device itself. Comparing only reads; taking differences into the show is one undo
//! step. Sending changes the device, so it happens only from the user's Send click after the
//! changes were shown, and only if the device still reads as it did then; a copy of its setup is
//! kept first, and one click puts it back.

use crate::devices::off_thread;
use crate::{AppState, Reply};
use pf_devices::adapter::{RestoreReport, SendReport, Snapshot, adapter_for, restore_setup, send_setup};
use pf_devices::setup::{Change, Setup, compare, one_string_per_port, show_setup, take_from_device};
use pf_devices::{Device, DeviceConfig, DeviceKind, Http};
use pf_engine::{Edit, ShowSnapshot};
use pf_model::{AdapterKind, Controller, PropId, Show};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tauri::State;

/// What was last read from each device (by address), for taking differences and sending.
#[derive(Default)]
pub(crate) struct SetupSessions {
    compared: Mutex<HashMap<String, (Device, DeviceConfig)>>,
    sends: Mutex<HashMap<String, SendSession>>,
}

/// A send shown to the user: the snapshot and target it was planned from, the rows they saw,
/// and, once sent, the setup from just before (to put back).
struct SendSession {
    kind: DeviceKind,
    shown: Snapshot,
    target: Setup,
    change_ids: Vec<String>,
    restore: Option<Snapshot>,
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
    /// What the device is busy with that sending would interrupt.
    busy: Option<String>,
    can_send: bool,
    /// Why it can't be sent, when it can't.
    reason: Option<String>,
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
        let snapshot = adapter.snapshot(http.as_ref(), &host).map_err(|e| {
            format!(
                "PixelFlow couldn't read {}'s current setup, so it won't send anything to it. {e}",
                device.name
            )
        })?;
        let plan = adapter
            .plan_config(&snapshot, &wanted)
            .map_err(|e| e.to_string())?;
        let busy = adapter.status(http.as_ref(), &host).ok().and_then(|s| s.busy);
        Ok((device, Ok((snapshot, plan, busy, wanted))))
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
                busy: None,
                can_send: false,
                reason: Some(reason),
            })
        }
        Ok((snapshot, plan, busy, target)) => {
            let change_ids: Vec<String> = plan.changes.iter().map(|c| c.id.clone()).collect();
            let can_send = !plan.writes.is_empty();
            sends.insert(
                address,
                SendSession {
                    kind: device.kind,
                    shown: snapshot,
                    target,
                    change_ids,
                    restore: None,
                },
            );
            Ok(SendPlan {
                device,
                controller_name: controller.name,
                reason: (!can_send).then(|| "The controller already matches your show.".to_string()),
                changes: plan.changes,
                notes: plan.notes,
                busy,
                can_send,
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
    let (kind, shown, target) = {
        let sends = lock(&state.devices.setup.sends);
        let session = sends
            .get(&address)
            .ok_or_else(|| "Review what will change before sending.".to_string())?;
        if session.change_ids != expected {
            return Err("What will change isn't what was shown. Review the changes again.".to_string());
        }
        (session.kind, session.shown.clone(), session.target.clone())
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
    if let Some(session) = lock(&state.devices.setup.sends).get_mut(&address) {
        // The plan is used up: sending again needs a fresh look.
        session.change_ids = vec!["(sent)".to_string()];
        if outcome.report.can_restore {
            session.restore = outcome.snapshot;
        }
    }
    Ok(outcome.report)
}

/// Puts back the setup the device had just before the last send. Changes the device, only from
/// the user's click.
#[tauri::command]
pub(crate) async fn restore_device_setup(
    state: State<'_, AppState>,
    address: String,
) -> Reply<RestoreReport> {
    let (kind, snapshot) = {
        let sends = lock(&state.devices.setup.sends);
        let session = sends.get(&address);
        let snapshot = session
            .and_then(|s| s.restore.clone())
            .ok_or_else(|| "There's no earlier setup to put back.".to_string())?;
        (session.map(|s| s.kind).unwrap_or(DeviceKind::Fpp), snapshot)
    };
    let http = Arc::clone(&state.devices.config_http);
    let host = address.clone();
    let report = off_thread(move || {
        let adapter = adapter_for(kind);
        Ok(restore_setup(adapter.as_ref(), http.as_ref(), &host, &snapshot))
    })
    .await?;
    if report.restored
        && let Some(session) = lock(&state.devices.setup.sends).get_mut(&address)
    {
        session.restore = None;
    }
    Ok(report)
}
