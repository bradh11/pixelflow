//! Device discovery and import commands. Network work runs on a blocking thread and never
//! holds the engine lock, so the UI stays responsive while devices are slow to answer.

use crate::{AppState, Reply};
use pf_devices::{Device, DeviceConfig, DiscoverOptions, Discovery, Http, HttpClient, ImportPlan};
use pf_engine::{Edit, ShowSnapshot};
use pf_model::Show;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

/// How the app reaches devices (recorded responses in tests).
pub(crate) struct DeviceAccess {
    http: Arc<dyn Http>,
    sweep_http: Arc<dyn Http>,
    /// FPP ping, mDNS, and the subnet sweep. Off in tests so nothing touches the network.
    network_discovery: bool,
}

impl DeviceAccess {
    pub(crate) fn network() -> Self {
        Self {
            http: Arc::new(HttpClient::new(Duration::from_millis(1500))),
            sweep_http: Arc::new(HttpClient::with_connect_timeout(
                Duration::from_millis(400),
                Duration::from_millis(1500),
            )),
            network_discovery: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn fake(http: pf_devices::FakeHttp) -> Self {
        let http: Arc<dyn Http> = Arc::new(http);
        Self {
            sweep_http: Arc::clone(&http),
            http,
            network_discovery: false,
        }
    }
}

/// A device, its configuration, and what importing it would add.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceDetails {
    device: Device,
    config: DeviceConfig,
    plan: ImportPlan,
}

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> Reply<T> + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong talking to the device.".to_string())?
}

fn inspect(http: &dyn Http, address: &str, show: &Show) -> Reply<DeviceDetails> {
    let device = pf_devices::identify(http, address, None).map_err(|e| e.to_string())?;
    let config = pf_devices::read_config(http, &device).map_err(|e| e.to_string())?;
    let plan = pf_devices::plan_import(&device, &config, show);
    Ok(DeviceDetails { device, config, plan })
}

/// Finds controllers (plus any addresses the user typed). Takes a few seconds. With `network` off,
/// only the typed addresses and the controllers an FPP lists are checked (no ping, mDNS, or sweep).
#[tauri::command]
pub(crate) async fn discover_devices(
    state: State<'_, AppState>,
    hosts: Vec<String>,
    network: bool,
) -> Reply<Discovery> {
    let http = Arc::clone(&state.devices.http);
    let sweep_http = Arc::clone(&state.devices.sweep_http);
    let network = network && state.devices.network_discovery;
    off_thread(move || {
        let options = DiscoverOptions {
            ping: network,
            mdns: network,
            sweep: network,
            extra_hosts: hosts,
            ..DiscoverOptions::default()
        };
        Ok(pf_devices::discover(http.as_ref(), sweep_http.as_ref(), &options))
    })
    .await
}

/// Reads a device's configuration and previews the import (changes nothing).
#[tauri::command]
pub(crate) async fn inspect_device(state: State<'_, AppState>, address: String) -> Reply<DeviceDetails> {
    let show = state.engine().show().clone();
    let http = Arc::clone(&state.devices.http);
    off_thread(move || inspect(http.as_ref(), &address, &show)).await
}

/// Adds the device as a controller with a starter prop per string, as one undo step.
#[tauri::command]
pub(crate) async fn import_device(state: State<'_, AppState>, address: String) -> Reply<ShowSnapshot> {
    let show = state.engine().show().clone();
    let http = Arc::clone(&state.devices.http);
    let details = off_thread(move || inspect(http.as_ref(), &address, &show)).await?;
    if !details.plan.can_import {
        return Err(format!("{} has no pixel outputs to import.", details.device.name));
    }
    let ImportPlan {
        controller, props, ..
    } = details.plan;
    let mut edits: Vec<Edit> = props.into_iter().map(|prop| Edit::AddProp { prop }).collect();
    edits.push(Edit::AddController { controller });
    state.engine().apply(edits).map_err(|e| e.to_string())
}
