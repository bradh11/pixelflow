//! What the shell's tests run on: the real commands on Tauri's mock runtime, with nothing
//! reaching the network, a sound device, or a folder outside the test's own.

use crate::devices::DeviceAccess;
use crate::{AppState, context, pickers, preview_window, recent, sync, vendor, vocals, with_commands};
use pf_engine::Engine;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

pub(crate) fn app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
    app_in(tempfile::tempdir().unwrap())
}

/// The app with its data (autosaves, kept sequences) in `dir`.
pub(crate) fn app_in(
    dir: tempfile::TempDir,
) -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
    let app = app_without_window(dir.path());
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, webview, dir)
}

/// The app, with its data in `dir`, before its window is made (running it makes the
/// window from the app's configuration).
pub(crate) fn app_without_window(dir: &std::path::Path) -> App<MockRuntime> {
    // Nothing reaches the network or a sound device: packets are recorded and music is timed
    // by a silent stopwatch.
    let (transport, _recorded) = pf_output::RecordingTransport::new();
    let silent: pf_engine::ClockFactory =
        std::sync::Arc::new(|_| Ok(Box::new(pf_audio::SilentClock::new()) as Box<dyn pf_audio::AudioClock>));
    let engine = Engine::new(dir)
        .with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn pf_output::Transport>))
        .with_clocks(silent);
    with_commands(mock_builder())
        .manage(AppState {
            engine: Mutex::new(engine),
            devices: DeviceAccess::fake(pf_devices::testing::network())
                .with_setup_dir(dir.join("device-setups")),
            waveforms: Mutex::default(),
            photos: Default::default(),
            models: Default::default(),
            export_cancels: Default::default(),
            send_cancels: Default::default(),
            download_cancels: Default::default(),
            download_folders: Default::default(),
            checking_files: Default::default(),
            recent: Arc::new(recent::RecentShows::new(Some(dir.join("config")))),
            last_folders: pickers::LastFolders::new(Some(dir.join("config"))),
            dialog: Default::default(),
            imported_from: Mutex::default(),
            vendor_mappings: Arc::new(vendor::SavedMappings::new(Some(dir.join("config")))),
            preview: preview_window::PreviewState::new(Some(dir.join("config"))),
        })
        .manage(sync::SyncState::with_opener(Arc::new(|_| {
            Ok((
                Box::new(pf_audio::SilentClock::new()) as Box<dyn pf_audio::AudioClock + Send>,
                None,
            ))
        })))
        .manage(vocals::VocalsState::default())
        .build(context())
        .unwrap()
}

/// Calls a command the way the UI does and returns its JSON reply (or error message).
pub(crate) fn call(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: webview.url().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}

/// Calls a command that answers with raw bytes.
pub(crate) fn call_raw(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<Vec<u8>, Value> {
    let reply = get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: webview.url().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )?;
    match reply {
        InvokeResponseBody::Raw(bytes) => Ok(bytes.to_vec()),
        other => panic!("expected raw bytes, got {other:?}"),
    }
}
