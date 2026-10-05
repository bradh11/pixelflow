//! The PixelFlow desktop shell: a thin bridge between the React UI, `pf-engine`, and
//! `pf-devices`.
//! Every command locks the engine, calls it, and returns its result as JSON. Commands are
//! `async` so they run off the UI thread (opening files and resolving controller addresses
//! can take a moment).

mod devices;
mod house;
mod layout;
mod playback;
mod sequencer;
mod xlights;

use devices::DeviceAccess;
use pf_engine::{
    Edit, Engine, EngineError, HistoryEntry, OutputStatus, PatternSpec, ShowSnapshot, TargetSpec,
};
use pf_model::Show;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::{Manager, State};

/// How often unsaved work (the show and the open sequence) is kept on disk.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(30);

struct AppState {
    engine: Mutex<Engine>,
    devices: DeviceAccess,
    /// Decoded music waveforms by file version and slices.
    waveforms: Mutex<std::collections::HashMap<playback::WaveformKey, playback::WaveformCell>>,
    /// Background photos the window may read: ones the user picked this session, and those of
    /// shows read from disk.
    photos: layout::PickedPhotos,
    /// House models the window may read, likewise.
    models: house::PickedModels,
    /// Bumped by `cancel_sequence_export`: an export started before the bump stops.
    export_cancels: std::sync::atomic::AtomicU64,
}

impl AppState {
    fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets the window read the photo and house model of a show read from disk (opened,
    /// restored, or imported). Files named only by an edit from the window never become
    /// readable this way: the allowlist lives here, not in the show the window can change.
    fn trust_files_of(&self, show: &Show) {
        if let Some(background) = &show.background {
            self.photos.add(PathBuf::from(&background.path));
        }
        if let Some(model) = &show.house_model {
            self.models.add(PathBuf::from(&model.path));
        }
    }

    /// Like [`Self::trust_files_of`] for a snapshot just read from disk, passing it on.
    fn trusting(&self, snapshot: ShowSnapshot) -> ShowSnapshot {
        self.trust_files_of(&snapshot.show);
        snapshot
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
    let snapshot = state.engine().open(&path).map_err(message)?;
    Ok(state.trusting(snapshot))
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
    let snapshot = state.engine().restore(&id).map_err(message)?;
    Ok(state.trusting(snapshot))
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
        playback::playback_stop_reason,
        playback::live_frame,
        playback::sequence_frame,
        playback::add_sequence,
        playback::play_sequence,
        playback::set_playback_volume,
        playback::audio_waveform,
        sequencer::new_sequence_doc,
        sequencer::sequence_recoveries,
        sequencer::recover_sequence,
        sequencer::discard_sequence_recovery,
        sequencer::open_sequence_doc,
        sequencer::save_sequence_doc,
        sequencer::save_sequence_doc_as,
        sequencer::close_sequence_doc,
        sequencer::get_sequence_doc,
        sequencer::edit_sequence,
        sequencer::effect_catalog,
        sequencer::cancel_sequence_export,
        sequencer::undo_sequence,
        sequencer::redo_sequence,
        sequencer::sequence_doc_frame,
        sequencer::play_sequence_doc,
        sequencer::set_sequence_doc_output,
        sequencer::add_sequence_doc_to_show,
        sequencer::sequence_export_layout,
        sequencer::export_sequence_doc,
        sequencer::analyze_audio,
        sequencer::detect_beats,
        sequencer::import_timing_file,
        sequencer::export_timing_track,
        xlights::import_xlights,
        xlights::import_xlights_sequence,
        layout::preview_props,
        layout::preview_props_3d,
        layout::pick_image,
        layout::read_image,
        house::pick_house_model,
        house::read_house_model,
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
                waveforms: Mutex::default(),
                photos: Default::default(),
                models: Default::default(),
                export_cancels: Default::default(),
            });
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("pixelflow-autosave".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(AUTOSAVE_EVERY);
                        autosave(&handle.state::<AppState>(), "autosave");
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

/// Keeps unsaved work: the show in its autosave history, and an open sequence with unsaved
/// changes where the next run offers it back.
fn autosave(state: &AppState, when: &str) {
    let mut engine = state.engine();
    if let Err(error) = engine.autosave() {
        eprintln!("{when} of the show failed: {error}");
    }
    if let Err(error) = engine.autosave_sequence() {
        eprintln!("{when} of the sequence failed: {error}");
    }
}

/// Runs when the app quits: keeps unsaved work (see [`autosave`]) and blacks out the lights.
fn shut_down(state: &AppState) {
    autosave(state, "autosave on exit");
    let mut engine = state.engine();
    engine.stop_output();
    engine.stop_playback();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{App, WebviewWindow, WebviewWindowBuilder};

    fn app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
        app_in(tempfile::tempdir().unwrap())
    }

    /// The app with its data (autosaves, kept sequences) in `dir`.
    fn app_in(dir: tempfile::TempDir) -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
        // Nothing reaches the network or a sound device: packets are recorded and music is timed
        // by a silent stopwatch.
        let (transport, _recorded) = pf_output::RecordingTransport::new();
        let silent: pf_engine::ClockFactory = std::sync::Arc::new(|_| {
            Ok(Box::new(pf_audio::SilentClock::new()) as Box<dyn pf_audio::AudioClock>)
        });
        let engine = Engine::new(dir.path())
            .with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn pf_output::Transport>))
            .with_clocks(silent);
        let app = with_commands(mock_builder())
            .manage(AppState {
                engine: Mutex::new(engine),
                devices: DeviceAccess::fake(pf_devices::testing::network()),
                waveforms: Mutex::default(),
                photos: Default::default(),
                models: Default::default(),
                export_cancels: Default::default(),
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

    /// Calls a command that answers with raw bytes.
    fn call_raw(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Vec<u8>, Value> {
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
    fn a_photo_set_from_the_ui_is_read_only_once_picked() {
        let (app, webview, dir) = app();
        let photo = dir.path().join("house.png");
        std::fs::write(&photo, [0x89, b'P', b'N', b'G']).unwrap();
        let background = json!({ "path": photo, "x": -10, "y": 8, "width": 20, "opacity": 0.6 });
        let snapshot = call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "setBackground", "background": background }] }),
        )
        .unwrap();
        assert_eq!(snapshot["show"]["background"]["width"], 20.0);
        assert_eq!(snapshot["canUndo"], true);

        // Naming a file in an edit doesn't make it readable: only picking it does.
        let error = call(&webview, "read_image", json!({ "path": photo })).unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .contains("can only show a photo you picked"),
            "{error}"
        );
        app.state::<AppState>().photos.add(photo.clone());
        let bytes = call_raw(&webview, "read_image", json!({ "path": photo })).unwrap();
        assert_eq!(bytes, vec![0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn a_house_model_set_from_the_ui_is_read_only_once_picked() {
        let (app, webview, dir) = app();
        let model = dir.path().join("house.glb");
        std::fs::write(&model, b"glTF\x02\0\0\0").unwrap();
        let house_model = json!({ "path": model, "position": { "x": 0, "y": 0, "z": -3 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 }, "scale": 1, "opacity": 0.8 });
        let snapshot = call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "setHouseModel", "houseModel": house_model }] }),
        )
        .unwrap();
        assert_eq!(snapshot["show"]["houseModel"]["opacity"], 0.8);

        // Naming any file in an edit doesn't make it readable.
        let error = call(&webview, "read_house_model", json!({ "path": model })).unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .contains("can only show a model you picked"),
            "{error}"
        );
        app.state::<AppState>().models.add(model.clone());
        let bytes = call_raw(&webview, "read_house_model", json!({ "path": model })).unwrap();
        assert_eq!(&bytes[..4], b"glTF");
    }

    #[test]
    fn a_shows_own_photo_and_model_are_readable_once_it_is_opened_from_disk() {
        let (app, webview, dir) = app();
        let photo = dir.path().join("house.png");
        std::fs::write(&photo, [0x89, b'P', b'N', b'G']).unwrap();
        let model = dir.path().join("house.obj");
        std::fs::write(&model, b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let path = dir.path().join("house.pixelflow.json");
        // A show saved earlier (here by another session: nothing is picked in this one).
        {
            let mut engine = Engine::new(dir.path());
            let background =
                serde_json::from_value(json!({ "path": photo, "x": 0, "y": 0, "width": 10, "opacity": 1 }))
                    .unwrap();
            let house_model = serde_json::from_value(json!({ "path": model, "position": { "x": 0, "y": 0, "z": 0 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 }, "scale": 1, "opacity": 1 })).unwrap();
            engine
                .apply(vec![
                    Edit::SetBackground {
                        background: Some(background),
                    },
                    Edit::SetHouseModel {
                        house_model: Some(house_model),
                    },
                ])
                .unwrap();
            engine.save_as(&path).unwrap();
        }
        assert!(call(&webview, "read_house_model", json!({ "path": model })).is_err());
        call(&webview, "open_show", json!({ "path": path })).unwrap();
        assert!(call_raw(&webview, "read_image", json!({ "path": photo })).is_ok());
        assert!(call_raw(&webview, "read_house_model", json!({ "path": model })).is_ok());

        // Pointing the open show at another file from the UI still doesn't make that one readable.
        let other = dir.path().join("secret.obj");
        std::fs::write(&other, b"password\n").unwrap();
        let house_model = json!({ "path": other, "position": { "x": 0, "y": 0, "z": 0 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 }, "scale": 1, "opacity": 1 });
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "setHouseModel", "houseModel": house_model }] }),
        )
        .unwrap();
        let error = call(&webview, "read_house_model", json!({ "path": other })).unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .contains("can only show a model you picked"),
            "{error}"
        );
        let error = call(&webview, "read_image", json!({ "path": other })).unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .contains("can only show a photo you picked"),
            "{error}"
        );
        drop(app);
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
    fn an_unsaved_sequence_is_kept_on_quit_and_offered_back_on_the_next_start() {
        let (app, webview, dir) = app();
        // A new sequence with its music starts clean, with nothing to undo.
        let snap = call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Song", "durationMs": 2000, "audio": "/music/song.mp3" }),
        )
        .unwrap();
        assert_eq!(snap["sequence"]["audio"], "/music/song.mp3");
        assert_eq!(
            (snap["dirty"].clone(), snap["canUndo"].clone()),
            (json!(false), json!(false))
        );
        // Nothing unsaved: quitting keeps nothing.
        shut_down(&app.state::<AppState>());
        assert!(Engine::new(dir.path()).sequence_recoveries().is_empty());

        let row = json!({ "id": "44444444-0000-4000-8000-0000000000cc",
            "target": { "prop": "11111111-0000-4000-8000-0000000000cc" }, "layers": [{ "effects": [] }] });
        call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "addRow", "row": row }] }),
        )
        .unwrap();
        shut_down(&app.state::<AppState>());
        drop(app);

        // The next start offers it back; recovering opens it with unsaved changes.
        let (_next, webview, _next_dir) = app_in(dir);
        let offered = call(&webview, "sequence_recoveries", json!({})).unwrap();
        assert_eq!(offered.as_array().unwrap().len(), 1);
        assert_eq!(offered[0]["name"], "Song");
        assert_eq!(offered[0]["path"], Value::Null);
        assert!(offered[0]["savedAtMs"].as_u64().unwrap() > 0);
        let id = offered[0]["id"].clone();
        let snap = call(&webview, "recover_sequence", json!({ "id": id })).unwrap();
        assert_eq!(snap["dirty"], true);
        assert_eq!(snap["sequence"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(snap["sequence"]["audio"], "/music/song.mp3");
        assert_eq!(
            call(&webview, "sequence_recoveries", json!({})).unwrap(),
            json!([])
        );
        let err = call(&webview, "recover_sequence", json!({ "id": id })).unwrap_err();
        assert_eq!(err, "That unsaved sequence isn't there anymore.");
        call(
            &webview,
            "discard_sequence_recovery",
            json!({ "id": "nothing-here" }),
        )
        .unwrap();
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
            json!({ "address": fpp, "destination": falcon, "protocol": "DDP" }),
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
            json!({ "start": 1, "count": 6147, "rawDdpOffsets": true })
        );

        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["props"], 0);
        assert_eq!(snapshot["show"]["controllers"][0]["ports"], json!([]));

        let error = call(
            &webview,
            "import_fpp_destination",
            json!({ "address": fpp, "destination": "192.0.2.77", "protocol": "DDP" }),
        )
        .unwrap_err();
        assert_eq!(error, json!("FPP doesn't send to 192.0.2.77."));

        // The destination is picked by address and protocol; an existing controller is never doubled.
        let error = call(
            &webview,
            "import_fpp_destination",
            json!({ "address": fpp, "destination": falcon, "protocol": "sACN unicast" }),
        )
        .unwrap_err();
        assert_eq!(error, json!(format!("FPP doesn't send to {falcon}.")));
        let error = call(
            &webview,
            "import_fpp_destination",
            json!({ "address": fpp, "destination": falcon, "protocol": "DDP" }),
        )
        .unwrap_err();
        assert_eq!(
            error,
            json!("Falcon_F16V5_B9F5 is already in your show as Falcon_F16V5_B9F5.")
        );

        // Inspecting the real device says it will fill the placeholder in.
        let details = call(&webview, "inspect_device", json!({ "address": falcon })).unwrap();
        assert_eq!(details["plan"]["alreadyInShow"], false);
        assert!(
            details["plan"]["notes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n == "Fills in Falcon_F16V5_B9F5, added from your FPP's output list."),
            "{details}"
        );
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
        let preview = call_raw(&webview, "preview_props", json!({})).unwrap();
        assert_eq!(&preview[4..8], &0u32.to_le_bytes(), "no props");
        let deep = call_raw(&webview, "preview_props_3d", json!({})).unwrap();
        assert_eq!(&deep[0..4], &2u32.to_le_bytes(), "x, y, z triples");
        call(&webview, "stop_playback", json!({})).unwrap();
        assert_eq!(call(&webview, "playback_status", json!({})).unwrap(), json!(null));
        assert_eq!(
            call(&webview, "playback_stop_reason", json!({})).unwrap(),
            json!(null)
        );
    }

    #[test]
    fn imports_an_xlights_show_folder_as_a_new_unsaved_show() {
        let (_app, webview, dir) = app();
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/pf-xlights/fixtures/sample-show");
        let imported = call(&webview, "import_xlights", json!({ "folder": folder })).unwrap();
        assert_eq!(imported["snapshot"]["show"]["name"], "sample-show");
        assert_eq!(imported["snapshot"]["dirty"], true);
        assert_eq!(imported["summary"]["props"], 6);
        assert_eq!(imported["summary"]["wired"], 6);
        assert_eq!(imported["snapshot"]["summary"]["controllers"], 2);
        let error = call(&webview, "import_xlights", json!({ "folder": dir.path() })).unwrap_err();
        assert!(
            error
                .as_str()
                .unwrap()
                .contains("doesn't look like an xLights show folder"),
            "{error}"
        );
    }

    #[test]
    fn imports_an_xlights_sequence_onto_the_open_show_as_an_unsaved_sequence() {
        let (app, webview, dir) = app();
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/pf-xlights/fixtures");
        call(
            &webview,
            "import_xlights",
            json!({ "folder": fixtures.join("sample-show") }),
        )
        .unwrap();
        let imported = call(
            &webview,
            "import_xlights_sequence",
            json!({ "path": fixtures.join("sequences/effects.xsq") }),
        )
        .unwrap();
        assert_eq!(imported["snapshot"]["sequence"]["name"], "Effects");
        assert_eq!(imported["snapshot"]["dirty"], true);
        assert_eq!(imported["snapshot"]["path"], json!(null));
        assert_eq!(imported["summary"]["rows"], 8);
        assert_eq!(imported["summary"]["placeholders"], 1);
        assert!(
            imported["notes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n.as_str().unwrap().contains("first color: Text (1)")),
            "{imported}"
        );
        let open = call(&webview, "get_sequence_doc", json!({})).unwrap();
        assert_eq!(open["sequence"]["name"], "Effects");

        let error = call(
            &webview,
            "import_xlights_sequence",
            json!({ "path": dir.path().join("missing.xsq") }),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("Could not read"), "{error}");
        let open = call(&webview, "get_sequence_doc", json!({})).unwrap();
        assert_eq!(
            open["sequence"]["name"], "Effects",
            "a failed import changes nothing"
        );

        // Never saved, so it's kept like any unsaved sequence and offered back next time.
        shut_down(&app.state::<AppState>());
        drop(app);
        let (_next, webview, _dir) = app_in(dir);
        let offered = call(&webview, "sequence_recoveries", json!({})).unwrap();
        assert_eq!(offered.as_array().unwrap().len(), 1, "{offered}");
        assert_eq!(offered[0]["name"], "Effects");
        assert_eq!(offered[0]["path"], Value::Null);
    }

    /// A tiny silent 16-bit mono WAV, half a second long.
    fn write_wav(path: &std::path::Path) {
        write_wav_ms(path, 500);
    }

    /// A silent 16-bit mono WAV, `ms` long.
    fn write_wav_ms(path: &std::path::Path, ms: usize) {
        let rate = 8000u32;
        let data = vec![0u8; rate as usize * ms / 1000 * 2];
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend(data);
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn sequences_are_added_with_their_music_and_played() {
        let (_app, webview, dir) = app();
        let path = write_sequence(dir.path());
        let song = dir.path().join("show.wav");
        write_wav(&song);
        let snapshot = call(&webview, "add_sequence", json!({ "path": path })).unwrap();
        let entry = snapshot["show"]["sequences"][0].clone();
        assert_eq!(entry["name"], "show");
        assert_eq!(entry["audio"], json!(song.display().to_string()));
        let waveform = call(&webview, "audio_waveform", json!({ "path": song, "slices": 10 })).unwrap();
        assert_eq!(waveform["durationMs"], 500);
        assert_eq!(waveform["peaks"].as_array().unwrap().len(), 10);
        // A changed file is read again.
        write_wav_ms(&song, 750);
        let waveform = call(&webview, "audio_waveform", json!({ "path": song, "slices": 10 })).unwrap();
        assert_eq!(waveform["durationMs"], 750);
        let missing = call(
            &webview,
            "audio_waveform",
            json!({ "path": dir.path().join("gone.mp3"), "slices": 10 }),
        )
        .unwrap_err();
        assert!(
            missing
                .as_str()
                .unwrap()
                .starts_with("PixelFlow can't find the music file"),
            "{missing}"
        );
        // Adding the same sequence again names it apart.
        let snapshot = call(&webview, "add_sequence", json!({ "path": path })).unwrap();
        assert_eq!(snapshot["show"]["sequences"][1]["name"], "show (2)");

        // Play without music here (tests must not open the sound output): drop the audio first.
        let mut silent = entry.clone();
        silent["audio"] = json!(null);
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "updateSequence", "sequence": silent }] }),
        )
        .unwrap();
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
            "play_sequence",
            json!({ "id": entry["id"], "positionMs": 0 }),
        )
        .unwrap();
        assert_eq!(status["sequence"], entry["id"]);
        assert_eq!(status["state"], "playing");
        let status = call(&webview, "set_playback_volume", json!({ "volume": 0.5 })).unwrap();
        assert_eq!(status["volume"], 0.5);
        call(&webview, "stop_playback", json!({})).unwrap();
    }

    /// A 12 s, 22.05 kHz mono WAV with a click every 500 ms.
    fn write_clicks(path: &std::path::Path) {
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

    #[test]
    fn sequences_are_authored_previewed_played_exported_and_beat_detected() {
        let (_app, webview, dir) = app();
        let prop = json!({
            "id": "11111111-0000-4000-8000-0000000000aa", "name": "Strip",
            "shape": { "source": "generator", "type": "line", "nodes": 4, "length": 1.0 }
        });
        let controller = json!({
            "id": "33333333-0000-4000-8000-0000000000aa", "name": "Bench", "address": "127.0.0.1:9",
            "protocol": { "type": "ddp" },
            "ports": [{ "number": 1, "slots": [{ "prop": "11111111-0000-4000-8000-0000000000aa" }] }]
        });
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "addProp", "prop": prop }, { "type": "addController", "controller": controller }] }),
        )
        .unwrap();
        assert_eq!(
            call(&webview, "get_sequence_doc", json!({})).unwrap(),
            Value::Null
        );
        let error = call(&webview, "edit_sequence", json!({ "edits": [] })).unwrap_err();
        assert_eq!(error, "No sequence is open. Create or open one first.");

        let snap = call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Song", "durationMs": 2000 }),
        )
        .unwrap();
        assert_eq!(snap["sequence"]["frameMs"], 25);
        // The JSON shapes the UI's sequence helpers build.
        let row = json!({ "id": "44444444-0000-4000-8000-000000000001",
            "target": { "prop": "11111111-0000-4000-8000-0000000000aa" }, "layers": [{ "effects": [] }] });
        let effect = json!({ "id": "55555555-0000-4000-8000-000000000001", "startMs": 0, "endMs": 1000,
            "params": { "kind": "on" }, "palette": { "colors": ["#ff0000"] }, "blend": "normal",
            "fadeInMs": 0, "fadeOutMs": 0 });
        let snap = call(
            &webview,
            "edit_sequence",
            json!({ "edits": [
                { "type": "addRow", "row": row },
                { "type": "addEffect", "row": "44444444-0000-4000-8000-000000000001", "layer": 0, "effect": effect },
            ] }),
        )
        .unwrap();
        assert_eq!(snap["canUndo"], true);
        assert_eq!(snap["issues"], json!([]));
        // The reply lists what changed, not the whole document.
        assert!(snap.get("sequence").is_none());
        assert_eq!(
            snap["changes"]["rows"][0]["id"],
            "44444444-0000-4000-8000-000000000001"
        );
        let frame = call_raw(&webview, "sequence_doc_frame", json!({ "positionMs": 500 })).unwrap();
        assert_eq!(frame, [255, 0, 0].repeat(4));
        let error = call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "setEffectTiming", "id": "55555555-0000-4000-8000-000000000001", "startMs": 9, "endMs": 9 }] }),
        )
        .unwrap_err();
        assert_eq!(error, "An effect must end after it starts.");

        let status = call(&webview, "play_sequence_doc", json!({ "positionMs": 0 })).unwrap();
        assert_eq!(
            (status["authored"].clone(), status["state"].clone()),
            (json!(true), json!("playing"))
        );
        // Preview only, while editing: switching keeps it playing.
        let status = call(&webview, "set_sequence_doc_output", json!({ "send": false })).unwrap();
        assert_eq!(status["state"], "playing");
        call(&webview, "stop_playback", json!({})).unwrap();
        assert_eq!(
            call(&webview, "set_sequence_doc_output", json!({ "send": true })).unwrap(),
            json!(null)
        );

        let layout = call(&webview, "sequence_export_layout", json!({})).unwrap();
        assert_eq!(layout["channels"], 12);
        let fseq = dir.path().join("song.fseq");
        let summary = call(&webview, "export_sequence_doc", json!({ "path": fseq })).unwrap();
        assert_eq!(
            (summary["frames"].clone(), summary["channels"].clone()),
            (json!(80), json!(12))
        );
        assert!(fseq.exists());
        let show = call(&webview, "add_sequence_doc_to_show", json!({ "path": fseq })).unwrap();
        assert_eq!(show["show"]["sequences"][0]["name"], "Song");
        assert_eq!(
            show["show"]["sequences"][0]["path"],
            json!(fseq.display().to_string())
        );
        call(&webview, "undo", json!({})).unwrap();

        let saved = dir.path().join("song.pfseq.json");
        let snap = call(&webview, "save_sequence_doc_as", json!({ "path": saved })).unwrap();
        assert_eq!(snap["dirty"], false);

        // Beat detection needs music; then it adds timing tracks as one undo step.
        let error = call(&webview, "detect_beats", json!({})).unwrap_err();
        assert!(error.as_str().unwrap().contains("no music yet"), "{error}");
        write_clicks(&dir.path().join("clicks.wav"));
        let analysis = call(
            &webview,
            "analyze_audio",
            json!({ "path": dir.path().join("clicks.wav") }),
        )
        .unwrap();
        let bpm = analysis["tempoBpm"].as_f64().unwrap();
        assert!((bpm - 120.0).abs() < 2.0, "{bpm}");
        call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "updateInfo", "name": "Song", "audio": "clicks.wav", "durationMs": 12000, "frameMs": 25 }] }),
        )
        .unwrap();
        call(&webview, "detect_beats", json!({})).unwrap();
        let doc = call(&webview, "get_sequence_doc", json!({})).unwrap();
        let tracks = doc["sequence"]["timingTracks"].as_array().unwrap();
        let names: Vec<&str> = tracks.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["Beats", "Bars", "Onsets"]);
        assert!(tracks[0]["marks"].as_array().unwrap().len() >= 20);
        let snap = call(&webview, "undo_sequence", json!({})).unwrap();
        assert_eq!(
            snap["changes"]["removedTimingTracks"].as_array().unwrap().len(),
            3
        );
        let doc = call(&webview, "get_sequence_doc", json!({})).unwrap();
        assert_eq!(doc["sequence"]["timingTracks"], json!([]));

        call(&webview, "close_sequence_doc", json!({})).unwrap();
        let snap = call(&webview, "open_sequence_doc", json!({ "path": saved })).unwrap();
        assert_eq!(snap["sequence"]["name"], "Song");
        assert!(
            call_raw(&webview, "sequence_doc_frame", json!({ "positionMs": 0 }))
                .unwrap()
                .len()
                == 12
        );
    }

    /// A strip wired to a controller and a new 2 s sequence with one red effect on it.
    fn authored(webview: &WebviewWindow<MockRuntime>) {
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

    #[test]
    fn the_effect_catalog_comes_from_the_engine_and_matches_the_ui_copy() {
        let (_app, webview, _dir) = app();
        let catalog = call(&webview, "effect_catalog", json!({})).unwrap();
        // (Through text, like the IPC: f32 settings read back as their short decimal forms.)
        let direct: Value =
            serde_json::from_str(&serde_json::to_string(&pf_sequence::effect_catalog()).unwrap()).unwrap();
        assert_eq!(catalog, direct);
        assert_eq!(catalog[0]["settings"][1]["step"], 0.01);
        assert_eq!(catalog[4]["settings"][0]["key"], "speed");
        assert_eq!(catalog[4]["settings"][0]["max"], 50.0);
        // The browser stand-in (app/src/api/effectCatalog.json) is a copy of this table; it must
        // not drift. Run with PIXELFLOW_UPDATE_CATALOG=1 to rewrite it.
        let copy = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api/effectCatalog.json");
        let text = serde_json::to_string_pretty(&catalog).unwrap() + "\n";
        if std::env::var_os("PIXELFLOW_UPDATE_CATALOG").is_some() {
            std::fs::write(&copy, &text).unwrap();
        }
        let saved = std::fs::read_to_string(&copy).unwrap_or_default();
        assert_eq!(
            saved, text,
            "app/src/api/effectCatalog.json is out of date: run `PIXELFLOW_UPDATE_CATALOG=1 cargo test`"
        );
    }

    #[test]
    fn edits_with_one_gesture_id_undo_together() {
        let (_app, webview, _dir) = app();
        authored(&webview);
        for start in [100, 200, 300] {
            let reply = call(
                &webview,
                "edit_sequence",
                json!({ "edits": [{ "type": "setEffectTiming", "id": "55555555-0000-4000-8000-0000000000bb",
                    "startMs": start, "endMs": start + 1000 }], "gesture": "drag-7" }),
            )
            .unwrap();
            assert_eq!(reply["changes"]["effects"][0]["effect"]["startMs"], start);
            assert_eq!(reply["changes"]["rows"], json!([]));
        }
        let reply = call(&webview, "undo_sequence", json!({})).unwrap();
        let effect = &reply["changes"]["rows"][0]["layers"][0]["effects"][0];
        assert_eq!(effect["startMs"], 0, "the whole drag undoes at once");
        let error = call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "setEffectParams", "id": "55555555-0000-4000-8000-0000000000bb",
                "params": { "kind": "chase", "speed": 1e39 } }] }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("Speed isn't a usable number"),
            "{error}"
        );
    }

    #[test]
    fn exports_report_progress_and_can_be_cancelled() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use tauri::Listener;
        let (app, webview, dir) = app();
        authored(&webview);
        let events: std::sync::Arc<Mutex<Vec<Value>>> = Default::default();
        let seen = events.clone();
        app.listen_any(sequencer::EXPORT_PROGRESS_EVENT, move |event| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_str(event.payload()).unwrap());
        });
        let path = dir.path().join("song.fseq");
        let summary = call(&webview, "export_sequence_doc", json!({ "path": path })).unwrap();
        assert_eq!(summary["frames"], 80);
        let events = events.lock().unwrap().clone();
        assert!(events.len() >= 2 && events.len() <= 101, "{}", events.len());
        assert_eq!(
            events.last().unwrap(),
            &json!({ "path": path.display().to_string(), "framesDone": 80, "frames": 80, "percent": 100 })
        );
        let percents: Vec<u64> = events.iter().map(|e| e["percent"].as_u64().unwrap()).collect();
        assert!(percents.windows(2).all(|w| w[0] < w[1]), "{percents:?}");

        // Cancelling: the plumbing with a progress collector, cancelled part way.
        let job = app.state::<AppState>().engine().sequence_export().unwrap();
        let cancels = AtomicU64::new(0);
        let mut reports = Vec::new();
        let cancelled = dir.path().join("cancelled.fseq");
        let err = sequencer::run_export(&job, &cancelled, &cancels, 0, |p| {
            if p.percent >= 25 {
                cancels.fetch_add(1, Ordering::AcqRel);
            }
            reports.push(p.percent);
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "The export was cancelled.");
        assert!(*reports.last().unwrap() < 30, "{reports:?}");
        assert!(!cancelled.exists());

        // The command bumps the counter; an export started before it stops.
        call(&webview, "cancel_sequence_export", json!({})).unwrap();
        assert_eq!(app.state::<AppState>().export_cancels.load(Ordering::Acquire), 1);
        let err = sequencer::run_export(
            &job,
            &cancelled,
            &app.state::<AppState>().export_cancels,
            0,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "The export was cancelled.");
    }

    #[test]
    fn timing_edits_match_the_browser_stand_in() {
        // app/src/api/timingEditCases.json is also run against the in-memory sequencer, so the
        // browser demo and the UI tests edit timing tracks exactly as the engine does.
        let (_app, webview, _dir) = app();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api/timingEditCases.json");
        let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(cases.len() > 10);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            call(
                &webview,
                "new_sequence_doc",
                json!({ "name": "Song", "durationMs": 60_000 }),
            )
            .unwrap();
            let adds: Vec<Value> = case["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|track| json!({ "type": "addTimingTrack", "track": track }))
                .collect();
            if !adds.is_empty() {
                call(&webview, "edit_sequence", json!({ "edits": adds })).unwrap();
            }
            let reply = call(&webview, "edit_sequence", json!({ "edits": case["edits"] }));
            let doc = call(&webview, "get_sequence_doc", json!({})).unwrap();
            match case["expect"].get("error") {
                Some(error) => assert_eq!(reply.unwrap_err(), *error, "{name}"),
                None => {
                    reply.unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert_eq!(
                        doc["sequence"]["timingTracks"], case["expect"]["tracks"],
                        "{name}"
                    );
                }
            }
        }
    }

    #[test]
    fn timing_files_import_and_export_through_commands() {
        let (app, webview, dir) = app();
        authored(&webview);
        let xtiming = dir.path().join("Vocals.xtiming");
        std::fs::write(
            &xtiming,
            r#"<timing name="Vocals"><EffectLayer><Effect label="Hi there" starttime="0" endtime="1000"/><Effect label="late" starttime="5000" endtime="6000"/></EffectLayer>
<EffectLayer><Effect label="Hi" starttime="0" endtime="400"/><Effect label="there" starttime="400" endtime="1000"/></EffectLayer></timing>"#,
        )
        .unwrap();
        let reply = call(&webview, "import_timing_file", json!({ "path": xtiming })).unwrap();
        assert_eq!(reply["tracks"], json!(["Vocals", "Vocals (words)"]));
        assert_eq!(
            reply["result"]["changes"]["timingTracks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            reply["notes"],
            json!(["1 mark in Vocals.xtiming started after the end of the sequence and was left out."])
        );
        // Imported again, the copies get their own names; one undo takes them back out.
        let again = call(&webview, "import_timing_file", json!({ "path": xtiming })).unwrap();
        assert_eq!(again["tracks"], json!(["Vocals 2", "Vocals 2 (words)"]));
        call(&webview, "undo_sequence", json!({})).unwrap();

        // A lyrics track goes out with its words, and comes back the same.
        let id = {
            let state = app.state::<AppState>();
            let engine = state.engine();
            engine.sequence_document().unwrap().timing_tracks[0].id
        };
        let out = dir.path().join("out.xtiming");
        let marks = call(&webview, "export_timing_track", json!({ "id": id, "path": out })).unwrap();
        assert_eq!(marks, json!(1));
        let written = std::fs::read_to_string(&out).unwrap();
        assert_eq!(written.matches("<EffectLayer>").count(), 2, "{written}");
        let back = pf_xlights::read_timing_file(&out, 2000).unwrap();
        assert_eq!(back.tracks[1].marks.len(), 2);
        // Audacity labels for anything else.
        let labels = dir.path().join("out.txt");
        call(
            &webview,
            "export_timing_track",
            json!({ "id": id, "path": labels }),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&labels).unwrap(),
            "0.000000\t1.000000\tHi there\n"
        );
        // .xml is read as xLights XML, so it's written that way too.
        let xml = dir.path().join("out.xml");
        call(&webview, "export_timing_track", json!({ "id": id, "path": xml })).unwrap();
        assert_eq!(pf_xlights::read_timing_file(&xml, 2000).unwrap().tracks.len(), 2);
        // Nothing but timing files is written.
        let script = dir.path().join("evil.sh");
        assert_eq!(
            call(
                &webview,
                "export_timing_track",
                json!({ "id": id, "path": script })
            )
            .unwrap_err(),
            "Timing tracks are saved as xLights timing files (.xtiming) or Audacity labels (.txt)."
        );
        assert!(!script.exists());

        let err = call(
            &webview,
            "import_timing_file",
            json!({ "path": dir.path().join("nope.xtiming") }),
        )
        .unwrap_err();
        assert!(err.as_str().unwrap().starts_with("Could not read"), "{err}");
        let err = call(
            &webview,
            "export_timing_track",
            json!({ "id": "66666666-0000-4000-8000-000000000000", "path": labels }),
        )
        .unwrap_err();
        assert_eq!(err, json!("That timing track isn't in the sequence anymore."));
        // A file read for one sequence isn't added to another opened meanwhile.
        let doc = app.state::<AppState>().engine().sequence_doc_id().unwrap();
        call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Other", "durationMs": 1000 }),
        )
        .unwrap();
        let import = pf_xlights::read_timing_file(&xtiming, 2000).unwrap();
        let err =
            sequencer::add_imported_tracks(&mut app.state::<AppState>().engine(), doc, import).unwrap_err();
        assert!(err.contains("Another sequence was opened"), "{err}");
    }

    #[test]
    fn detected_beats_only_go_to_the_sequence_they_were_found_for() {
        let (app, webview, dir) = app();
        authored(&webview);
        let state = app.state::<AppState>();
        call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "updateInfo", "name": "Song", "audio": "song.wav", "durationMs": 2000, "frameMs": 25 }] }),
        )
        .unwrap();
        let saved = dir.path().join("song.pfseq.json");
        call(&webview, "save_sequence_doc_as", json!({ "path": saved })).unwrap();
        let (doc, music) = {
            let engine = state.engine();
            (
                engine.sequence_doc_id().unwrap(),
                engine.sequence_music().unwrap(),
            )
        };
        let tracks = || {
            vec![pf_sequence::TimingTrack::new(
                "Beats",
                pf_sequence::TimingKind::Beats,
                vec![],
            )]
        };
        // Another sequence opened meanwhile: nothing is added to it.
        call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Other", "durationMs": 1000 }),
        )
        .unwrap();
        let err = sequencer::add_detected_tracks(&mut state.engine(), doc, &music, tracks()).unwrap_err();
        assert!(err.contains("changed while the beats were being found"), "{err}");
        // The same sequence, reopened, is a different document too.
        call(&webview, "open_sequence_doc", json!({ "path": saved })).unwrap();
        assert!(sequencer::add_detected_tracks(&mut state.engine(), doc, &music, tracks()).is_err());
        let doc = state.engine().sequence_doc_id().unwrap();
        let music = dir.path().join("song.wav");
        call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "updateInfo", "name": "Song", "audio": "other.wav", "durationMs": 2000, "frameMs": 25 }] }),
        )
        .unwrap();
        assert!(
            sequencer::add_detected_tracks(&mut state.engine(), doc, &music, tracks()).is_err(),
            "new music"
        );
        call(&webview, "undo_sequence", json!({})).unwrap();
        let reply = sequencer::add_detected_tracks(&mut state.engine(), doc, &music, tracks()).unwrap();
        assert_eq!(reply.changes.timing_tracks.len(), 1);
    }
}
