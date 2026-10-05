//! The PixelFlow desktop shell: a thin bridge between the React UI, `pf-engine`, and
//! `pf-devices`.
//! Every command locks the engine, calls it, and returns its result as JSON. Commands are
//! `async` so they run off the UI thread (opening files and resolving controller addresses
//! can take a moment).

mod devices;
mod playback;

use devices::DeviceAccess;
use pf_engine::{
    Edit, Engine, EngineError, HistoryEntry, OutputStatus, PatternSpec, ShowSnapshot, TargetSpec,
};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::{Manager, State};

/// How often unsaved work is copied into the autosave history.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(30);

struct AppState {
    engine: Mutex<Engine>,
    devices: DeviceAccess,
}

impl AppState {
    fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

type Reply<T> = Result<T, String>;

fn message(error: EngineError) -> String {
    error.to_string()
}

#[tauri::command]
async fn get_snapshot(state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    Ok(state.engine().snapshot())
}

#[tauri::command]
async fn apply_edits(state: State<'_, AppState>, edits: Vec<Edit>) -> Reply<ShowSnapshot> {
    state.engine().apply(edits).map_err(message)
}

#[tauri::command]
async fn undo(state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    Ok(state.engine().undo())
}

#[tauri::command]
async fn redo(state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    Ok(state.engine().redo())
}

#[tauri::command]
async fn new_show(state: State<'_, AppState>, name: String) -> Reply<ShowSnapshot> {
    Ok(state.engine().new_show(&name))
}

#[tauri::command]
async fn open_show(state: State<'_, AppState>, path: PathBuf) -> Reply<ShowSnapshot> {
    state.engine().open(&path).map_err(message)
}

#[tauri::command]
async fn save_show(state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    state.engine().save().map_err(message)
}

#[tauri::command]
async fn save_show_as(state: State<'_, AppState>, path: PathBuf) -> Reply<ShowSnapshot> {
    state.engine().save_as(&path).map_err(message)
}

#[tauri::command]
async fn list_history(state: State<'_, AppState>) -> Reply<Vec<HistoryEntry>> {
    Ok(state.engine().history())
}

#[tauri::command]
async fn restore_history(state: State<'_, AppState>, id: String) -> Reply<ShowSnapshot> {
    state.engine().restore(&id).map_err(message)
}

#[tauri::command]
async fn start_output(
    state: State<'_, AppState>,
    pattern: PatternSpec,
    target: TargetSpec,
) -> Reply<OutputStatus> {
    state.engine().start_output(pattern, target).map_err(message)
}

#[tauri::command]
async fn stop_output(state: State<'_, AppState>) -> Reply<OutputStatus> {
    Ok(state.engine().stop_output())
}

#[tauri::command]
async fn output_status(state: State<'_, AppState>) -> Reply<OutputStatus> {
    Ok(state.engine().output_status())
}

/// Registers every command the UI can call.
fn with_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        get_snapshot,
        apply_edits,
        undo,
        redo,
        new_show,
        open_show,
        save_show,
        save_show_as,
        list_history,
        restore_history,
        start_output,
        stop_output,
        output_status,
        devices::discover_devices,
        devices::inspect_device,
        devices::import_device,
        devices::import_fpp_destination,
        devices::fpp_status,
        devices::fpp_sequences,
        devices::fpp_start,
        devices::fpp_stop,
        playback::start_playback,
        playback::pause_playback,
        playback::seek_playback,
        playback::stop_playback,
        playback::playback_status,
        playback::live_frame,
        playback::sequence_frame,
        playback::preview_props,
    ])
}

/// The app's compiled configuration and permissions (expanded once, shared with tests).
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// Starts the desktop app.
pub fn run() {
    with_commands(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                engine: Mutex::new(Engine::new(data_dir)),
                devices: DeviceAccess::network(),
            });
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("pixelflow-autosave".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(AUTOSAVE_EVERY);
                        let state = handle.state::<AppState>();
                        if let Err(error) = state.engine().autosave() {
                            eprintln!("autosave failed: {error}");
                        }
                    }
                })?;
            Ok(())
        })
        .build(context())
        .expect("error while building PixelFlow")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event
                && let Some(state) = app.try_state::<AppState>()
            {
                shut_down(&state);
            }
        });
}

/// Runs when the app quits: keeps unsaved work in the autosave history and blacks out the lights.
fn shut_down(state: &AppState) {
    let mut engine = state.engine();
    if let Err(error) = engine.autosave() {
        eprintln!("autosave on exit failed: {error}");
    }
    engine.stop_output();
    engine.stop_playback();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{App, WebviewWindow, WebviewWindowBuilder};

    fn app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = with_commands(mock_builder())
            .manage(AppState {
                engine: Mutex::new(Engine::new(dir.path())),
                devices: DeviceAccess::fake(pf_devices::testing::network()),
            })
            .build(context())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        (app, webview, dir)
    }

    /// Calls a command the way the UI does and returns its JSON reply (or error message).
    fn call(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
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

    #[test]
    fn edits_from_the_ui_round_trip_through_the_engine() {
        let (_app, webview, _dir) = app();
        let snapshot = call(&webview, "get_snapshot", json!({})).unwrap();
        assert_eq!(snapshot["show"]["name"], "Untitled Show");
        assert_eq!(snapshot["canUndo"], false);

        // The same JSON shape the UI's newProp() builds.
        let prop = json!({
            "id": "11111111-0000-4000-8000-000000000001",
            "name": "Mega Tree 1",
            "shape": { "source": "generator", "type": "tree", "strings": 16, "nodesPerString": 50,
                       "height": 5, "baseRadius": 1.5, "topRadius": 0.1, "serpentine": true },
            "transform": { "position": { "x": 0, "y": 0, "z": 0 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 },
                           "scale": { "x": 1, "y": 1, "z": 1 } },
            "colorOrder": "RGB",
            "regions": [],
            "tags": []
        });
        let snapshot = call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "addProp", "prop": prop }] }),
        )
        .unwrap();
        assert_eq!(snapshot["summary"]["pixels"], 800);
        assert_eq!(snapshot["dirty"], true);
        assert_eq!(snapshot["channelMap"]["props"][0]["nodes"], 800);

        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["props"], 0);
        let snapshot = call(&webview, "redo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["props"], 1);
    }

    #[test]
    fn errors_come_back_as_plain_messages() {
        let (_app, webview, _dir) = app();
        let error = call(&webview, "save_show", json!({})).unwrap_err();
        assert_eq!(
            error,
            json!("This show has not been saved yet. Choose where to save it.")
        );
        let error = call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "removeProp", "id": "11111111-0000-4000-8000-000000000009" }] }),
        )
        .unwrap_err();
        assert_eq!(error, json!("There is no prop with that id."));
    }

    #[test]
    fn save_as_then_open_by_path() {
        let (_app, webview, dir) = app();
        let path = dir.path().join("house.pixelflow.json");
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "renameShow", "name": "House" }] }),
        )
        .unwrap();
        let snapshot = call(&webview, "save_show_as", json!({ "path": path })).unwrap();
        assert_eq!(snapshot["dirty"], false);
        call(&webview, "new_show", json!({ "name": "Other" })).unwrap();
        let snapshot = call(&webview, "open_show", json!({ "path": path })).unwrap();
        assert_eq!(snapshot["show"]["name"], "House");
    }

    #[test]
    fn output_commands_accept_the_ui_specs() {
        let (_app, webview, _dir) = app();
        let status = call(&webview, "output_status", json!({})).unwrap();
        assert_eq!(status["running"], false);
        assert_eq!(status["stopReason"], Value::Null);
        // An empty show has nothing to light, so starting is refused with a plain message.
        let error = call(
            &webview,
            "start_output",
            json!({ "pattern": { "kind": "solid", "color": "ff0000" }, "target": { "type": "show" } }),
        )
        .unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .starts_with("The chosen target has no pixels to light."),
            "{error}"
        );
        let status = call(&webview, "stop_output", json!({})).unwrap();
        assert_eq!(status["running"], false);
    }

    #[test]
    fn quitting_autosaves_and_stops_output() {
        let (app, webview, _dir) = app();
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "renameShow", "name": "Unsaved" }] }),
        )
        .unwrap();
        assert!(
            call(&webview, "list_history", json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        shut_down(&app.state::<AppState>());
        let history = call(&webview, "list_history", json!({})).unwrap();
        assert!(!history.as_array().unwrap().is_empty());
        let status = call(&webview, "output_status", json!({})).unwrap();
        assert_eq!(status["running"], false);
    }

    #[test]
    fn discovery_finds_typed_hosts_and_the_controllers_an_fpp_lists() {
        let (_app, webview, _dir) = app();
        let found = call(
            &webview,
            "discover_devices",
            json!({ "hosts": [pf_devices::testing::FPP], "network": false }),
        )
        .unwrap();
        let kinds: Vec<_> = found["devices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["kind"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds, vec!["fpp", "falcon"]);
        assert_eq!(found["devices"][1]["foundBy"], json!(["fppPeer"]));
        assert_eq!(found["silent"], json!([]));
    }

    #[test]
    fn inspecting_and_importing_a_falcon_adds_one_undo_step() {
        let (_app, webview, _dir) = app();
        let falcon = pf_devices::testing::FALCON;
        let details = call(&webview, "inspect_device", json!({ "address": falcon })).unwrap();
        assert_eq!(details["device"]["model"], "F16v5");
        assert_eq!(details["plan"]["canImport"], true);
        assert_eq!(details["plan"]["props"].as_array().unwrap().len(), 3);

        let snapshot = call(&webview, "import_device", json!({ "address": falcon })).unwrap();
        assert_eq!(snapshot["summary"]["props"], 3);
        assert_eq!(snapshot["summary"]["controllers"], 1);
        assert_eq!(snapshot["show"]["controllers"][0]["adapter"], "falcon");
        assert_eq!(snapshot["show"]["controllers"][0]["address"], falcon);
        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["props"], 0);
        assert_eq!(snapshot["summary"]["controllers"], 0);
    }

    #[test]
    fn devices_with_nothing_to_import_and_unknown_hosts_get_plain_errors() {
        let (_app, webview, _dir) = app();
        let error = call(
            &webview,
            "import_device",
            json!({ "address": pf_devices::testing::FPP }),
        )
        .unwrap_err();
        assert_eq!(error, json!("FPP has no pixel outputs to import."));
        let error = call(&webview, "inspect_device", json!({ "address": "192.0.2.99" })).unwrap_err();
        assert!(
            error.as_str().unwrap().starts_with("Could not reach 192.0.2.99"),
            "{error}"
        );
    }

    #[test]
    fn fpp_status_sequences_and_playback_control() {
        let (_app, webview, _dir) = app();
        let fpp = pf_devices::testing::FPP;
        let status = call(&webview, "fpp_status", json!({ "address": fpp })).unwrap();
        assert_eq!(status["state"], "playing");
        assert_eq!(status["sequence"], "Christmas Medley 2017.fseq");
        assert_eq!(status["secondsRemaining"], 456);
        let sequences = call(&webview, "fpp_sequences", json!({ "address": fpp })).unwrap();
        assert_eq!(sequences[0]["name"], "Christmas Medley 2017");
        assert_eq!(sequences[0]["stepMs"], 50);
        let started = call(
            &webview,
            "fpp_start",
            json!({ "address": fpp, "name": "Christmas Medley 2017.fseq" }),
        );
        assert_eq!(started.unwrap(), json!(null));
        assert!(
            call(
                &webview,
                "fpp_stop",
                json!({ "address": fpp, "gracefully": true })
            )
            .is_ok()
        );
        let error = call(&webview, "fpp_status", json!({ "address": "192.0.2.99" })).unwrap_err();
        assert!(
            error.as_str().unwrap().starts_with("Could not reach 192.0.2.99"),
            "{error}"
        );
    }

    #[test]
    fn a_controller_added_from_an_fpp_is_filled_in_when_imported_later() {
        let (_app, webview, _dir) = app();
        let (fpp, falcon) = (pf_devices::testing::FPP, pf_devices::testing::FALCON);
        let snapshot = call(
            &webview,
            "import_fpp_destination",
            json!({ "address": fpp, "destination": falcon }),
        )
        .unwrap();
        assert_eq!(snapshot["summary"]["controllers"], 1);
        assert_eq!(snapshot["show"]["controllers"][0]["name"], "Falcon_F16V5_B9F5");
        assert_eq!(snapshot["show"]["controllers"][0]["ports"], json!([]));
        let id = snapshot["show"]["controllers"][0]["id"].clone();

        let snapshot = call(&webview, "import_device", json!({ "address": falcon })).unwrap();
        assert_eq!(snapshot["summary"]["controllers"], 1, "filled in, not duplicated");
        assert_eq!(snapshot["summary"]["props"], 3);
        assert_eq!(snapshot["show"]["controllers"][0]["id"], id);
        assert_eq!(snapshot["show"]["controllers"][0]["adapter"], "falcon");
        assert_eq!(
            snapshot["show"]["controllers"][0]["sequenceChannels"],
            json!({ "start": 1, "count": 6147 })
        );

        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["props"], 0);
        assert_eq!(snapshot["show"]["controllers"][0]["ports"], json!([]));

        let error = call(
            &webview,
            "import_fpp_destination",
            json!({ "address": fpp, "destination": "192.0.2.77" }),
        )
        .unwrap_err();
        assert_eq!(error, json!("FPP doesn't send to 192.0.2.77."));
    }

    /// A tiny uncompressed sequence: 6 channels, 40 frames, 25 ms apart; every channel of frame
    /// `n` holds `n + 1`.
    fn write_sequence(dir: &std::path::Path) -> PathBuf {
        let mut out = b"PSEQ".to_vec();
        out.extend_from_slice(&28u16.to_le_bytes());
        out.extend_from_slice(&[0, 1]);
        out.extend_from_slice(&28u16.to_le_bytes());
        out.extend_from_slice(&6u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.push(25);
        out.extend_from_slice(&[0; 9]);
        for frame in 0..40u8 {
            out.extend_from_slice(&[frame + 1; 6]);
        }
        let path = dir.join("show.fseq");
        std::fs::write(&path, out).unwrap();
        path
    }

    #[test]
    fn plays_a_sequence_with_pause_seek_and_stop() {
        let (_app, webview, dir) = app();
        let path = write_sequence(dir.path());
        let error = call(
            &webview,
            "start_playback",
            json!({ "path": path, "positionMs": 0 }),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("Devices screen"), "{error}");

        // Loopback only: nothing leaves this machine.
        let controller = json!({
            "id": "33333333-0000-4000-8000-000000000009", "name": "Bench", "address": "127.0.0.1:9",
            "protocol": { "type": "ddp" }, "ports": [], "sequenceChannels": { "start": 1, "count": 6 }
        });
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "addController", "controller": controller }] }),
        )
        .unwrap();
        let status = call(
            &webview,
            "start_playback",
            json!({ "path": path, "positionMs": 100 }),
        )
        .unwrap();
        assert_eq!(status["state"], "playing");
        assert_eq!(
            (status["positionMs"].clone(), status["durationMs"].clone()),
            (json!(100), json!(1000))
        );
        let status = call(&webview, "pause_playback", json!({ "paused": true })).unwrap();
        assert_eq!(status["state"], "paused");
        let status = call(&webview, "seek_playback", json!({ "positionMs": 500 })).unwrap();
        assert_eq!(status["positionMs"], 500);
        assert_eq!(call(&webview, "preview_props", json!({})).unwrap(), json!([]));
        call(&webview, "stop_playback", json!({})).unwrap();
        assert_eq!(call(&webview, "playback_status", json!({})).unwrap(), json!(null));
    }
}
