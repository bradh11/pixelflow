//! What the shell's tests run on: the real commands on Tauri's mock runtime, with nothing
//! reaching the network, a sound device, or a folder outside the test's own.

use crate::devices::DeviceAccess;
use crate::{AppState, context, pickers, preview_window, recent, sync, vendor, vocals, with_commands};
use pf_engine::Engine;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

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
    builder(dir).build(context()).unwrap()
}

/// The app with its data in `dir`, before it's built: tests that need more state than every
/// command's (the assistant's, Find lyrics') manage it first.
pub(crate) fn builder(dir: &std::path::Path) -> tauri::Builder<MockRuntime> {
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
}

/// Calls a command the way the UI does and returns its reply as it came (or error message).
pub(crate) fn request(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<InvokeResponseBody, Value> {
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
}

/// Calls a command the way the UI does and returns its JSON reply (or error message).
pub(crate) fn call(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    request(webview, cmd, args).map(|body| body.deserialize::<Value>().unwrap())
}

/// Calls a command that answers with raw bytes.
pub(crate) fn call_raw(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<Vec<u8>, Value> {
    match request(webview, cmd, args)? {
        InvokeResponseBody::Raw(bytes) => Ok(bytes.to_vec()),
        other => panic!("expected raw bytes, got {other:?}"),
    }
}

/// What the app sends as `event` from now on: every payload, in order.
pub(crate) fn heard(app: &App<MockRuntime>, event: &str) -> Arc<Mutex<Vec<Value>>> {
    let events: Arc<Mutex<Vec<Value>>> = Default::default();
    let seen = Arc::clone(&events);
    app.listen_any(event, move |event| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    events
}

/// Calls `command` (one that takes nothing) the next time the app sends `event` with a payload
/// `at` accepts, once. The call is made from the thread sending the event, so the work that
/// reported it only goes on once the command has answered: a cancel lands at a known point.
pub(crate) fn call_at(
    app: &App<MockRuntime>,
    webview: &WebviewWindow<MockRuntime>,
    event: &str,
    at: impl Fn(&Value) -> bool + Send + 'static,
    command: &'static str,
) {
    let webview = webview.clone();
    let waiting = AtomicBool::new(true);
    app.listen_any(event, move |event| {
        let payload: Value = serde_json::from_str(event.payload()).unwrap();
        if at(&payload) && waiting.swap(false, Ordering::AcqRel) {
            call(&webview, command, json!({})).unwrap();
        }
    });
}

/// Every file under `dir`, relative to it, hidden ones included (folders themselves aren't
/// listed).
pub(crate) fn files_under(dir: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() {
            found.extend(
                files_under(&entry.path())
                    .into_iter()
                    .map(|below| format!("{name}/{below}")),
            );
        } else {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// A strip wired to a controller and a new 2 s sequence with one red effect on it.
pub(crate) fn authored(webview: &WebviewWindow<MockRuntime>) {
    let prop = json!({
        "id": "11111111-0000-4000-8000-0000000000bb", "name": "Strip",
        "shape": { "source": "generator", "type": "line", "nodes": 4, "length": 1.0 }
    });
    let controller = json!({
        "id": "33333333-0000-4000-8000-0000000000bb", "name": "Bench", "address": "127.0.0.1:9",
        "protocol": { "type": "ddp" },
        "ports": [{ "number": 1, "slots": [{ "prop": "11111111-0000-4000-8000-0000000000bb" }] }]
    });
    call(
        webview,
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": prop }, { "type": "addController", "controller": controller }] }),
    )
    .unwrap();
    call(
        webview,
        "new_sequence_doc",
        json!({ "name": "Song", "durationMs": 2000 }),
    )
    .unwrap();
    let row = json!({ "id": "44444444-0000-4000-8000-0000000000bb",
        "target": { "prop": "11111111-0000-4000-8000-0000000000bb" }, "layers": [{ "effects": [] }] });
    let effect = json!({ "id": "55555555-0000-4000-8000-0000000000bb", "startMs": 0, "endMs": 1000,
        "params": { "kind": "on" }, "palette": { "colors": ["#ff0000"] }, "blend": "normal",
        "fadeInMs": 0, "fadeOutMs": 0 });
    call(
        webview,
        "edit_sequence",
        json!({ "edits": [
            { "type": "addRow", "row": row },
            { "type": "addEffect", "row": "44444444-0000-4000-8000-0000000000bb", "layer": 0, "effect": effect },
        ] }),
    )
    .unwrap();
}

/// A 12 s, 22.05 kHz mono WAV with a click every 500 ms.
pub(crate) fn write_clicks(path: &std::path::Path) {
    let rate = 22_050u32;
    let mut samples = vec![0i16; rate as usize * 12];
    for beat in 0..23 {
        let start = (250 + beat * 500) * rate as usize / 1000;
        for k in 0..200 {
            let v = (-(k as f32) / 40.0).exp() * (k as f32 * 0.6).sin() * 0.9;
            samples[start + k] = (v * i16::MAX as f32) as i16;
        }
    }
    let data_len = (samples.len() * 2) as u32;
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}
