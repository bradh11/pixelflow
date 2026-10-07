//! Device discovery and import commands. Network work runs on a blocking thread and never
//! holds the engine lock, so the UI stays responsive while devices are slow to answer.

use crate::{AppState, Reply};
use pf_devices::fpp_player::{self, FppSequence, PlayerStatus};
use pf_devices::{
    Device, DeviceConfig, DiscoverOptions, Discovery, Http, HttpClient, ImportPlan, Reach, ReachCheck, TcpReach,
};
use pf_engine::{Edit, ShowSnapshot};
use pf_model::Show;
use serde::Serialize;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

/// How the app reaches devices (recorded responses in tests).
pub(crate) struct DeviceAccess {
    http: Arc<dyn Http>,
    sweep_http: Arc<dyn Http>,
    /// FPP ping, mDNS, and the subnet sweep. Off in tests so nothing touches the network.
    network_discovery: bool,
    /// Whether a controller answers (the Test screen's quick look).
    reach: Arc<dyn Reach>,
    /// This computer's networks (address and netmask).
    networks: fn() -> Vec<(Ipv4Addr, Ipv4Addr)>,
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
            reach: Arc::new(TcpReach::new(Duration::from_millis(800))),
            networks: pf_devices::local_networks,
        }
    }

    #[cfg(test)]
    pub(crate) fn fake(http: pf_devices::FakeHttp) -> Self {
        let http: Arc<dyn Http> = Arc::new(http);
        Self {
            sweep_http: Arc::clone(&http),
            http,
            network_discovery: false,
            // In tests, only 192.0.2.10 answers, and this computer is on 192.0.2.0/24.
            reach: Arc::new(pf_devices::FakeReach::new(["192.0.2.10"])),
            networks: || vec![(Ipv4Addr::new(192, 0, 2, 1), Ipv4Addr::new(255, 255, 255, 0))],
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
        mut controller,
        props,
        ..
    } = details.plan;
    let mut engine = state.engine();
    let mut edits: Vec<Edit> = props.into_iter().map(|prop| Edit::AddProp { prop }).collect();
    // A controller added from an FPP's output list (same address, no ports yet) is filled in
    // rather than duplicated.
    let placeholder = engine
        .show()
        .controllers
        .iter()
        .find(|c| c.address == controller.address && pf_devices::is_placeholder(c))
        .map(|c| (c.id, c.name.clone(), c.sequence_channels));
    if let Some((id, name, sequence_channels)) = placeholder {
        controller.id = id;
        controller.name = name;
        controller.sequence_channels = sequence_channels;
        edits.push(Edit::UpdateController { controller });
    } else {
        edits.push(Edit::AddController { controller });
    }
    engine.apply(edits).map_err(|e| e.to_string())
}

/// Adds a controller that an FPP sends to, from the FPP's output list (works even when the
/// controller isn't answering), as one undo step. `protocol` is the destination's protocol as the
/// FPP lists it ("DDP", "sACN unicast", ...), since one address can be listed more than once.
#[tauri::command]
pub(crate) async fn import_fpp_destination(
    state: State<'_, AppState>,
    address: String,
    destination: String,
    protocol: String,
) -> Reply<ShowSnapshot> {
    let http = Arc::clone(&state.devices.http);
    let target = off_thread(move || {
        let fpp = pf_devices::fpp::probe(http.as_ref(), &address).map_err(|e| e.to_string())?;
        let config = pf_devices::read_config(http.as_ref(), &fpp).map_err(|e| e.to_string())?;
        let target = config
            .destinations
            .into_iter()
            .find(|d| d.address == destination && d.protocol == protocol)
            .ok_or_else(|| format!("{} doesn't send to {destination}.", fpp.name))?;
        Ok(target)
    })
    .await?;
    // Plan against the show as it is now, under the same lock that applies the change, so names
    // and the duplicate check can't go stale while the FPP was being read.
    let mut engine = state.engine();
    if let Some(existing) = engine
        .show()
        .controllers
        .iter()
        .find(|c| c.address == target.address)
    {
        return Err(format!(
            "{} is already in your show as {}.",
            fpp_name_or_address(&target.address, &target.description),
            existing.name
        ));
    }
    let plan = pf_devices::plan_destination_import(&target, engine.show());
    if !plan.can_import {
        return Err(plan.notes.join(" "));
    }
    engine
        .apply(vec![Edit::AddController {
            controller: plan.controller,
        }])
        .map_err(|e| e.to_string())
}

fn fpp_name_or_address(address: &str, description: &str) -> String {
    if description.trim().is_empty() {
        address.to_string()
    } else {
        description.trim().to_string()
    }
}

/// Whether each controller answers, and whether it's on this computer's network (changes
/// nothing: a connection to its web port is opened and closed, and nothing is read).
#[tauri::command]
pub(crate) async fn check_controllers(state: State<'_, AppState>, addresses: Vec<String>) -> Reply<Vec<ReachCheck>> {
    let reach = Arc::clone(&state.devices.reach);
    let networks = state.devices.networks;
    off_thread(move || Ok(pf_devices::check_reach(reach.as_ref(), &addresses, &networks()))).await
}

/// What an FPP is playing (changes nothing).
#[tauri::command]
pub(crate) async fn fpp_status(state: State<'_, AppState>, address: String) -> Reply<PlayerStatus> {
    let http = Arc::clone(&state.devices.http);
    off_thread(move || fpp_player::status(http.as_ref(), &address).map_err(|e| e.to_string())).await
}

/// The sequences stored on an FPP (changes nothing).
#[tauri::command]
pub(crate) async fn fpp_sequences(state: State<'_, AppState>, address: String) -> Reply<Vec<FppSequence>> {
    let http = Arc::clone(&state.devices.http);
    off_thread(move || fpp_player::sequences(http.as_ref(), &address).map_err(|e| e.to_string())).await
}

/// Starts a playlist or sequence on an FPP. Only ever called when the user clicks Play.
#[tauri::command]
pub(crate) async fn fpp_start(state: State<'_, AppState>, address: String, name: String) -> Reply<()> {
    let http = Arc::clone(&state.devices.http);
    off_thread(move || fpp_player::start(http.as_ref(), &address, &name).map_err(|e| e.to_string())).await
}

/// Stops an FPP now or at the end of the current sequence. Only ever called when the user clicks Stop.
#[tauri::command]
pub(crate) async fn fpp_stop(state: State<'_, AppState>, address: String, gracefully: bool) -> Reply<()> {
    let http = Arc::clone(&state.devices.http);
    off_thread(move || fpp_player::stop(http.as_ref(), &address, gracefully).map_err(|e| e.to_string())).await
}
