//! The PixelFlow desktop shell: a thin bridge between the React UI, `pf-engine`, and
//! `pf-devices`.
//! Every command locks the engine, calls it, and returns its result as JSON. Commands are
//! `async` so they run off the UI thread (opening files and resolving controller addresses
//! can take a moment).

mod assistant;
mod device_setup;
mod devices;
mod files;
mod fpp_send;
mod house;
mod layout;
mod logging;
mod menu;
mod pickers;
mod playback;
mod probes;
mod recent;
mod sequencer;
mod xlights;

use devices::DeviceAccess;
use pf_engine::{
    Edit, Engine, EngineError, HistoryEntry, OutputStatus, PatternSpec, ShowSnapshot, TargetSpec,
};
use pf_model::Show;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime, State};

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
    /// Bumped by `cancel_fpp_send`: a send to an FPP started before the bump stops.
    send_cancels: std::sync::atomic::AtomicU64,
    /// Set while a check of the show's files runs (see `files::check_files`).
    checking_files: std::sync::atomic::AtomicBool,
    /// Shows opened lately (written only here, when a show is opened, saved, or restored).
    recent: Arc<recent::RecentShows>,
    /// The folder each kind of file dialog was last used in.
    last_folders: pickers::LastFolders,
    /// Set while a file dialog is showing (one at a time).
    dialog: pickers::DialogSlot,
    /// The xLights folder the open show was imported from, and that show's generation: its
    /// first save starts there.
    imported_from: Mutex<Option<(u64, PathBuf)>>,
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
            self.photos.add(pf_model::path_from_text(&background.path));
        }
        if let Some(model) = &show.house_model {
            self.models.add(pf_model::path_from_text(&model.path));
        }
    }

    /// Like [`Self::trust_files_of`] for a snapshot just read from disk, passing it on.
    fn trusting(&self, snapshot: ShowSnapshot) -> ShowSnapshot {
        self.trust_files_of(&snapshot.show);
        snapshot
    }
}

type Reply<T> = Result<T, String>;

/// A path from the window, as path text (see `pf_model::path_to_text`), read back without
/// losing anything: the dialogs hand the window paths this way.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PathArg(PathBuf);

impl<'de> serde::Deserialize<'de> for PathArg {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self(pf_model::path_from_text(&text)))
    }
}

impl std::ops::Deref for PathArg {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

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

/// The sample show ("Try the demo show"), built into the app: the same house as the browser's
/// `?demo`.
const SAMPLE_SHOW: &str = include_str!("../../src/api/sampleShow.json");

/// Opens the sample show as a new, unsaved show: saving it asks where, so the copy built into
/// the app is never written. Like a new show, it has nothing to save until it's changed.
#[tauri::command]
async fn open_sample_show(state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    let show: Show =
        serde_json::from_str(SAMPLE_SHOW).map_err(|e| format!("The sample show couldn't be read ({e})."))?;
    let show = pf_engine::CheckedShow::new(show).map_err(message)?;
    Ok(state.engine().start_from(show))
}

/// Opens the show file at `path`. The path comes from the window (normally one the Open dialog
/// just gave it, but any path is accepted) and the show goes on the recent list.
#[tauri::command]
async fn open_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathArg,
) -> Reply<ShowSnapshot> {
    open_show_at(&app, &state, path.0).await
}

/// Opens the show file at `path`, and puts it at the top of the recent shows. Every open (the
/// Open dialog, a recent show, Locate…) comes through here.
async fn open_show_at<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    path: PathBuf,
) -> Reply<ShowSnapshot> {
    let started = Instant::now();
    // Read (and its files looked for) without holding the engine.
    let file = path.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || pf_engine::read_show(&file))
        .await
        .map_err(|_| "Something went wrong opening the show.".to_string())?
        .map_err(message)?;
    let read = started.elapsed();
    let snapshot = state.engine().open_read(&path, loaded);
    let snapshot = state.trusting(snapshot);
    log::debug!(
        "open show: read in {} ms, ready in {} ms",
        read.as_millis(),
        started.elapsed().as_millis()
    );
    remember(app, state, &snapshot).await;
    Ok(snapshot)
}

/// Puts the saved show of `snapshot` at the top of the recent shows (and File → Open Recent).
/// Runs off the engine and the window; a list that can't be written is only logged.
async fn remember<R: Runtime>(app: &AppHandle<R>, state: &AppState, snapshot: &ShowSnapshot) {
    let Some(visit) = recent::Visit::of(snapshot) else {
        return;
    };
    let list = Arc::clone(&state.recent);
    let _ = tauri::async_runtime::spawn_blocking(move || list.record(visit, recent::now_ms())).await;
    menu::refresh_recent(app, &state.recent);
}

#[tauri::command]
async fn save_show<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> Reply<ShowSnapshot> {
    let snapshot = state.engine().save().map_err(message)?;
    remember(&app, &state, &snapshot).await;
    Ok(snapshot)
}

/// Saves the show at `path` and puts it on the recent list. The path comes from the window
/// (normally one the Save dialog just gave it, but any path is accepted).
#[tauri::command]
async fn save_show_as<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: PathArg,
) -> Reply<ShowSnapshot> {
    let snapshot = state.engine().save_as(&path).map_err(message)?;
    remember(&app, &state, &snapshot).await;
    Ok(snapshot)
}

#[tauri::command]
async fn list_history(state: State<'_, AppState>) -> Reply<Vec<HistoryEntry>> {
    Ok(state.engine().history())
}

#[tauri::command]
async fn restore_history<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Reply<ShowSnapshot> {
    let file = state.engine().history_file(&id).map_err(message)?;
    let restored = tauri::async_runtime::spawn_blocking(move || file.read())
        .await
        .map_err(|_| "Something went wrong reading that version.".to_string())?
        .map_err(message)?;
    let snapshot = state.engine().restore_read(restored);
    let snapshot = state.trusting(snapshot);
    remember(&app, &state, &snapshot).await;
    Ok(snapshot)
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
        open_sample_show,
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
        devices::check_controllers,
        devices::fpp_sequences,
        devices::fpp_start,
        devices::fpp_stop,
        devices::fpp_files,
        devices::fpp_schedule,
        devices::fpp_setup_plan,
        devices::fpp_set_up_show,
        devices::open_device_page,
        device_setup::compare_device,
        device_setup::take_from_device_setup,
        device_setup::plan_device_setup,
        device_setup::send_device_setup,
        device_setup::restore_device_setup,
        device_setup::forget_device_setup_copy,
        fpp_send::fpp_send_plan,
        fpp_send::fpp_send,
        fpp_send::cancel_fpp_send,
        fpp_send::fpp_sequence_names,
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
        sequencer::set_sequence_doc_loop,
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
        assistant::ai_key_storage,
        assistant::set_api_key,
        assistant::use_api_key_for_session,
        assistant::has_api_key,
        assistant::api_key_location,
        assistant::delete_api_key,
        assistant::list_ai_models,
        assistant::ai_send,
        assistant::ai_stop,
        assistant::ai_new_chat,
        assistant::ai_apply,
        assistant::ai_discard,
        assistant::ai_preview,
        assistant::ai_preview_frame,
        assistant::ai_sync,
        files::check_files,
        files::find_missing_files,
        files::locate_file,
        files::sequence_music_missing,
        files::find_sequence_music,
        files::locate_sequence_music,
        pickers::pick_path,
        recent::list_recent_shows,
        recent::forget_recent_show,
        recent::clear_recent_shows,
        recent::locate_recent_show,
    ])
}

/// The app's compiled configuration and permissions (expanded once, shared with tests).
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

/// Starts the desktop app.
pub fn run() {
    logging::init();
    with_commands(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(menu::on_event)
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let setups_dir = data_dir.join("device-setups");
            let config_dir = app.path().app_config_dir().ok();
            app.manage(AppState {
                engine: Mutex::new(Engine::new(data_dir)),
                devices: DeviceAccess::network().with_setup_dir(setups_dir),
                waveforms: Mutex::default(),
                photos: Default::default(),
                models: Default::default(),
                export_cancels: Default::default(),
                send_cancels: Default::default(),
                checking_files: Default::default(),
                recent: Arc::new(recent::RecentShows::new(config_dir.clone())),
                last_folders: pickers::LastFolders::new(config_dir),
                dialog: Default::default(),
                imported_from: Mutex::default(),
            });
            // macOS has a menu bar either way: this one has the show's File menu.
            #[cfg(target_os = "macos")]
            app.set_menu(menu::build(app.handle(), &app.state::<AppState>().recent)?)?;
            app.manage(assistant::AiState::live());
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
        .run(on_run_event);
}

/// The app's own events: an exit asked for while there's unsaved work waits for the window to
/// ask about it, and quitting keeps unsaved work and blacks out the lights.
fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::ExitRequested { code, api, .. } if exit_must_wait(app, code) => {
            api.prevent_exit();
            // The window asks (Save / Don't save / Cancel), like its close button; the app
            // exits when it has closed. Asked again meanwhile, it shows the same question.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.close();
            }
        }
        tauri::RunEvent::Exit => {
            if let Some(state) = app.try_state::<AppState>() {
                shut_down(&state);
            }
        }
        _ => {}
    }
}

/// Whether an exit (`code`: `None` once the last window has closed, else asked for by the app)
/// should wait for the window to ask about unsaved work. Once the window has closed it already
/// asked, so the app exits.
fn exit_must_wait<R: Runtime>(app: &AppHandle<R>, code: Option<i32>) -> bool {
    code.is_some()
        && app.get_webview_window("main").is_some()
        && app
            .try_state::<AppState>()
            .is_some_and(|state| state.engine().has_unsaved_changes())
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
        let app = app_without_window(dir.path());
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        (app, webview, dir)
    }

    /// The app, with its data in `dir`, before its window is made (running it makes the
    /// window from the app's configuration).
    fn app_without_window(dir: &std::path::Path) -> App<MockRuntime> {
        // Nothing reaches the network or a sound device: packets are recorded and music is timed
        // by a silent stopwatch.
        let (transport, _recorded) = pf_output::RecordingTransport::new();
        let silent: pf_engine::ClockFactory = std::sync::Arc::new(|_| {
            Ok(Box::new(pf_audio::SilentClock::new()) as Box<dyn pf_audio::AudioClock>)
        });
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
                checking_files: Default::default(),
                recent: Arc::new(recent::RecentShows::new(Some(dir.join("config")))),
                last_folders: pickers::LastFolders::new(Some(dir.join("config"))),
                dialog: Default::default(),
                imported_from: Mutex::default(),
            })
            .build(context())
            .unwrap()
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
    fn check_controllers_says_which_answer_and_which_are_on_this_network() {
        let (_app, webview, _dir) = app();
        let checks = call(
            &webview,
            "check_controllers",
            json!({ "addresses": ["192.0.2.10", "192.0.2.11", "192.168.1.50"] }),
        )
        .unwrap();
        assert_eq!(
            checks,
            json!([
                { "address": "192.0.2.10", "answering": true, "onLocalNetwork": true },
                { "address": "192.0.2.11", "answering": false, "onLocalNetwork": true },
                { "address": "192.168.1.50", "answering": false, "onLocalNetwork": false },
            ])
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
        assert!(error.as_str().unwrap().contains("Controllers screen"), "{error}");

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

        // A new sequence can start with its rows (a row for every prop and group), still clean.
        let with_rows = call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Rows", "durationMs": 2000, "rows": [
                { "id": "44444444-0000-4000-8000-0000000000f1", "target": { "group": "22222222-0000-4000-8000-0000000000aa" }, "layers": [{ "effects": [] }] },
                { "id": "44444444-0000-4000-8000-0000000000f2", "target": { "prop": "11111111-0000-4000-8000-0000000000aa" }, "layers": [{ "effects": [] }] }
            ] }),
        )
        .unwrap();
        assert_eq!(with_rows["sequence"]["rows"].as_array().unwrap().len(), 2);
        assert_eq!(
            with_rows["sequence"]["rows"][0]["target"]["group"],
            "22222222-0000-4000-8000-0000000000aa"
        );
        assert_eq!(
            (with_rows["dirty"].clone(), with_rows["canUndo"].clone()),
            (json!(false), json!(false))
        );

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
        assert_eq!(status["looping"], false);
        // Looping switches at once while playing, and is kept for the next play.
        let status = call(&webview, "set_sequence_doc_loop", json!({ "looping": true })).unwrap();
        assert_eq!(
            (status["state"].clone(), status["looping"].clone()),
            (json!("playing"), json!(true))
        );
        call(&webview, "stop_playback", json!({})).unwrap();
        let status = call(&webview, "play_sequence_doc", json!({ "positionMs": 0 })).unwrap();
        assert_eq!(status["looping"], true);
        call(&webview, "stop_playback", json!({})).unwrap();
        assert_eq!(
            call(&webview, "set_sequence_doc_loop", json!({ "looping": false })).unwrap(),
            json!(null)
        );
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
    fn the_open_sequence_is_sent_to_an_fpp_with_its_music_and_a_playlist() {
        use tauri::Listener;
        let fpp = pf_devices::testing::FakeFpp::start().with_playlist("Main");
        let (app, webview, dir) = app();
        authored(&webview);
        let music = dir.path().join("Song.mp3");
        std::fs::write(&music, b"not really music").unwrap();
        let source = json!({ "kind": "openSequence", "name": "Song" });

        // Planning only reads.
        let plan = call(
            &webview,
            "fpp_send_plan",
            json!({ "address": fpp.address(), "source": source, "music": music }),
        )
        .unwrap();
        assert_eq!(
            plan["sequence"],
            json!({ "name": "Song.fseq", "exists": false, "fppName": null, "keepBothName": "Song (2).fseq" })
        );
        // The show's bench controller isn't one the (fixture) FPP sends to: said plainly.
        assert_eq!(
            plan["layoutWarnings"],
            json!([
                "This sequence has 12 channels but the FPP sends 6,147. Lights past channel 12 will stay dark.",
                "Bench (127.0.0.1:9): the FPP doesn't send to it, so it won't light up."
            ])
        );
        assert_eq!(plan["music"]["name"], "Song.mp3");
        assert_eq!(plan["playlists"], json!(["Main"]));
        assert_eq!(plan["newPlaylistName"], "Song");
        assert!(fpp.state().writes().is_empty(), "{:?}", fpp.state().writes());

        let events: Arc<Mutex<Vec<Value>>> = Default::default();
        let seen = events.clone();
        app.listen_any(fpp_send::FPP_SEND_PROGRESS_EVENT, move |event| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_str(event.payload()).unwrap());
        });
        let result = call(
            &webview,
            "fpp_send",
            json!({ "address": fpp.address(), "request": {
                "source": source, "music": music, "sequenceName": "Song.fseq", "musicName": "Song.mp3",
                "uploadMusic": true, "playlist": { "kind": "existing", "name": "Main" } } }),
        )
        .unwrap();
        assert_eq!(
            result,
            json!({ "sequenceName": "Song.fseq", "musicName": "Song.mp3", "playlist": "Main",
                    "playName": "Song.fseq", "notes": [] })
        );
        {
            let state = fpp.state();
            assert!(state.sequences["Song.fseq"].size > 0);
            assert_eq!(state.music["Song.mp3"].size, 16);
            let entry = &state.playlists["Main"]["mainPlaylist"][0];
            assert_eq!(
                (
                    entry["sequenceName"].clone(),
                    entry["mediaName"].clone(),
                    entry["duration"].clone()
                ),
                (json!("Song.fseq"), json!("Song.mp3"), json!(2.0))
            );
        }
        let steps: Vec<String> = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e["step"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(steps.first().map(String::as_str), Some("export"));
        assert!(steps.iter().any(|s| s == "sequence") && steps.iter().any(|s| s == "music"));
        // The exported copy was only for sending.
        let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("pixelflow-send-{}-", std::process::id()))
            })
            .filter(|e| e.path().join("Song.fseq").exists())
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // Sending never starts anything by itself: "Play it now" is a separate click (fpp_start).
        assert!(fpp.state().commands.is_empty());

        // Cancel bumps the counter that sends watch.
        call(&webview, "cancel_fpp_send", json!({})).unwrap();
        assert_eq!(
            app.state::<AppState>()
                .send_cancels
                .load(std::sync::atomic::Ordering::Acquire),
            1
        );
    }

    #[test]
    fn the_open_sequences_own_music_is_found_next_to_it() {
        let fpp = pf_devices::testing::FakeFpp::start();
        let (_app, webview, dir) = app();
        authored(&webview);
        std::fs::write(dir.path().join("Song.mp3"), b"music").unwrap();
        call(
            &webview,
            "edit_sequence",
            json!({ "edits": [{ "type": "updateInfo", "name": "Song", "audio": "Song.mp3",
                                "durationMs": 2000, "frameMs": 25 }] }),
        )
        .unwrap();
        call(
            &webview,
            "save_sequence_doc_as",
            json!({ "path": dir.path().join("Song.pfseq.json") }),
        )
        .unwrap();
        let source = json!({ "kind": "openSequence", "name": "Song" });
        let plan = call(
            &webview,
            "fpp_send_plan",
            json!({ "address": fpp.address(), "source": source, "music": "Song.mp3" }),
        )
        .unwrap();
        assert_eq!(plan["music"]["name"], "Song.mp3");
        call(
            &webview,
            "fpp_send",
            json!({ "address": fpp.address(), "request": {
                "source": source, "music": "Song.mp3", "sequenceName": "Relative music.fseq", "musicName": "Song.mp3",
                "uploadMusic": true, "playlist": { "kind": "none" } } }),
        )
        .unwrap();
        assert_eq!(fpp.state().music["Song.mp3"].size, 5);
    }

    #[test]
    fn an_exported_sequence_names_the_fpps_own_copy_of_the_music_exactly() {
        let fpp = pf_devices::testing::FakeFpp::start().with_music("song.mp3", 5);
        let (_app, webview, dir) = app();
        authored(&webview);
        let music = dir.path().join("Song.mp3");
        std::fs::write(&music, b"music").unwrap();
        let source = json!({ "kind": "openSequence", "name": "Exact" });
        let plan = call(
            &webview,
            "fpp_send_plan",
            json!({ "address": fpp.address(), "source": source, "music": music }),
        )
        .unwrap();
        assert_eq!(plan["music"]["fppName"], "song.mp3");
        call(
            &webview,
            "fpp_send",
            json!({ "address": fpp.address(), "request": {
                "source": source, "music": music, "sequenceName": "Exact.fseq", "musicName": "song.mp3",
                "uploadMusic": false, "playlist": { "kind": "none" } } }),
        )
        .unwrap();
        let state = fpp.state();
        let head = &state.heads["Exact.fseq"];
        assert!(
            head.windows(11).any(|w| w == b"mfsong.mp3\0"),
            "the .fseq names the FPP's song.mp3 exactly"
        );
        assert_eq!(state.music["song.mp3"].size, 5, "the FPP's copy is untouched");
    }

    #[test]
    fn the_sequences_on_an_fpp_are_listed_in_one_request() {
        let (_app, webview, _dir) = app();
        let names = call(
            &webview,
            "fpp_sequence_names",
            json!({ "address": pf_devices::testing::FPP }),
        )
        .unwrap();
        assert_eq!(names, json!(["Christmas Medley 2017.fseq"]));
    }

    #[test]
    fn an_fpps_files_and_schedule_are_read_without_changing_anything() {
        let fpp = pf_devices::testing::FakeFpp::start()
            .with_sequence("Show.fseq", 1000)
            .with_duration("Show.fseq", 60_000)
            .with_music("Show.mp3", 500)
            .with_playlist("Main")
            .with_schedule(json!([{ "enabled": 1, "day": 7, "playlist": "Main",
                "startTime": "17:00:00", "endTime": "22:00:00", "repeat": 1, "stopType": 0 }]));
        let (_app, webview, _dir) = app();
        let address = fpp.address();
        let sequences = call(
            &webview,
            "fpp_files",
            json!({ "address": address, "folder": "sequences" }),
        )
        .unwrap();
        assert_eq!(sequences[0]["name"], "Show.fseq");
        assert_eq!(sequences[0]["durationMs"], 60_000);
        assert_eq!(sequences[0]["sizeBytes"], 1000);
        let music = call(
            &webview,
            "fpp_files",
            json!({ "address": address, "folder": "music" }),
        )
        .unwrap();
        assert_eq!(music[0]["name"], "Show.mp3");
        let playlists = call(
            &webview,
            "fpp_files",
            json!({ "address": address, "folder": "playlists" }),
        )
        .unwrap();
        assert_eq!(playlists[0]["name"], "Main");
        let schedule = call(&webview, "fpp_schedule", json!({ "address": address })).unwrap();
        assert_eq!(schedule[0]["name"], "Main");
        assert_eq!(schedule[0]["kind"], "playlist");
        assert_eq!(schedule[0]["startTime"], "17:00:00");
        assert!(fpp.state().writes().is_empty());
    }

    #[test]
    fn setting_up_the_show_from_an_fpp_adds_its_targets_as_one_undo_step() {
        let (_app, webview, _dir) = app();
        let fpp = pf_devices::testing::FPP;
        let plan = call(&webview, "fpp_setup_plan", json!({ "address": fpp })).unwrap();
        assert_eq!(plan["own"], json!(null));
        let controllers = plan["controllers"].as_array().unwrap();
        assert_eq!(controllers.len(), 1);
        assert_eq!(controllers[0]["name"], "Falcon_F16V5_B9F5");
        assert_eq!(controllers[0]["address"], pf_devices::testing::FALCON);
        assert_eq!(
            controllers[0]["sequenceChannels"],
            json!({ "start": 1, "count": 6147, "rawDdpOffsets": true })
        );
        // Planning changed nothing.
        let snapshot = call(&webview, "get_snapshot", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["controllers"], 0);

        // A stale plan adds nothing.
        let error = call(
            &webview,
            "fpp_set_up_show",
            json!({ "address": fpp, "expected": ["192.0.2.77"] }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("changed since you looked"),
            "{error}"
        );

        let expected = json!([pf_devices::testing::FALCON]);
        let snapshot = call(
            &webview,
            "fpp_set_up_show",
            json!({ "address": fpp, "expected": expected }),
        )
        .unwrap();
        assert_eq!(snapshot["summary"]["controllers"], 1);
        assert_eq!(snapshot["show"]["controllers"][0]["name"], "Falcon_F16V5_B9F5");
        let again = call(&webview, "fpp_setup_plan", json!({ "address": fpp })).unwrap();
        assert_eq!(again["controllers"], json!([]));
        assert!(
            again["skipped"][0]["reason"]
                .as_str()
                .unwrap()
                .starts_with("Already in your show")
        );

        let snapshot = call(&webview, "undo", json!({})).unwrap();
        assert_eq!(snapshot["summary"]["controllers"], 0, "one undo step");
    }

    #[test]
    fn a_device_page_link_must_be_a_plain_address() {
        let (_app, webview, _dir) = app();
        let error = call(
            &webview,
            "open_device_page",
            json!({ "address": "192.0.2.10/x?y" }),
        )
        .unwrap_err();
        assert_eq!(error, json!("192.0.2.10/x?y isn't a controller address."));
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

    const PNG: [u8; 4] = [0x89, b'P', b'N', b'G'];

    /// A show saved in `dir/Show` with its photo in `dir/Show/photos` and a model outside it.
    fn show_with_files(dir: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
        let root = dir.join("Show");
        let photo = root.join("photos/house.png");
        std::fs::create_dir_all(photo.parent().unwrap()).unwrap();
        std::fs::write(&photo, PNG).unwrap();
        let model = dir.join("Models/house.obj");
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, b"v 0 0 0\n").unwrap();
        let show = root.join("show.pixelflow.json");
        let mut engine = Engine::new(dir.join("data"));
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
        engine.save_as(&show).unwrap();
        (show, photo, model)
    }

    #[test]
    fn a_moved_show_opens_with_its_files_readable_and_says_what_is_missing() {
        let (_app, webview, dir) = app();
        let (show, _, _) = show_with_files(dir.path());
        let moved = dir.path().join("Moved");
        std::fs::rename(show.parent().unwrap(), &moved).unwrap();
        let snapshot = call(
            &webview,
            "open_show",
            json!({ "path": moved.join("show.pixelflow.json") }),
        )
        .unwrap();
        let photo = moved.join("photos/house.png");
        assert_eq!(
            snapshot["show"]["background"]["path"],
            json!(photo.to_str().unwrap())
        );
        assert_eq!(snapshot["missingFiles"], json!([]));
        assert_eq!(
            call_raw(&webview, "read_image", json!({ "path": photo })).unwrap(),
            PNG
        );
    }

    #[test]
    fn found_files_become_readable_only_inside_the_show_folder() {
        let (app, webview, dir) = app();
        let (show, photo, model) = show_with_files(dir.path());
        call(&webview, "open_show", json!({ "path": show })).unwrap();
        // The photo moved within the show's folder; the model is gone (a copy sits outside).
        let moved = show.parent().unwrap().join("pictures/house.png");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&photo, &moved).unwrap();
        std::fs::remove_file(&model).unwrap();
        std::fs::write(dir.path().join("house.obj"), b"v 0 0 0\n").unwrap();
        // Snapshots never look at the disk: only a check notices.
        let snapshot = call(&webview, "get_snapshot", json!({})).unwrap();
        assert_eq!(snapshot["missingFiles"], json!([]));
        assert_eq!(snapshot["filesChecked"], true);
        let snapshot = call(&webview, "check_files", json!({ "all": true })).unwrap();
        let missing = snapshot["missingFiles"].as_array().unwrap();
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0]["file"], json!({ "kind": "photo" }));
        assert_eq!(missing[0]["message"], "house.png isn't where it was.");
        assert_eq!(missing[1]["file"], json!({ "kind": "houseModel" }));

        // Find again on the model alone: nothing found, nothing changed.
        let report = call(
            &webview,
            "find_missing_files",
            json!({ "file": { "kind": "houseModel" } }),
        )
        .unwrap();
        assert_eq!(report["found"], json!([]));
        assert_eq!(report["stillMissing"].as_array().unwrap().len(), 2);
        let report = call(&webview, "find_missing_files", json!({})).unwrap();
        assert_eq!(report["found"].as_array().unwrap().len(), 1);
        assert_eq!(report["found"][0]["to"], json!(moved.to_str().unwrap()));
        assert_eq!(report["stillMissing"][0]["name"], "house.obj");
        assert_eq!(report["snapshot"]["canUndo"], true);
        assert_eq!(
            call_raw(&webview, "read_image", json!({ "path": moved })).unwrap(),
            PNG
        );
        // A model the user locates becomes readable; other files named by the window don't.
        let elsewhere = dir.path().join("house.obj");
        assert!(call(&webview, "read_house_model", json!({ "path": elsewhere })).is_err());
        let state = app.state::<AppState>();
        let snapshot = tauri::async_runtime::block_on(files::located(
            &state,
            pf_engine::FileRole::HouseModel,
            &elsewhere,
        ))
        .unwrap();
        assert!(snapshot.missing_files.is_empty());
        assert!(call_raw(&webview, "read_house_model", json!({ "path": elsewhere })).is_ok());
        let error = tauri::async_runtime::block_on(files::located(
            &state,
            pf_engine::FileRole::Photo,
            &dir.path().join("gone.png"),
        ))
        .unwrap_err();
        assert_eq!(error, "gone.png isn't there anymore. Choose another file.");
    }

    #[cfg(unix)]
    #[test]
    fn photo_paths_that_are_not_utf8_reach_the_shell_intact() {
        use std::os::unix::ffi::OsStringExt;
        let (app, webview, dir) = app();
        // "Café.png" with a Latin-1 é: the window gets it as path text and sends it back.
        let photo = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"Caf\xe9.png".to_vec()));
        app.state::<AppState>().photos.add(photo.clone());
        let text = pf_model::path_to_text(&photo);
        assert!(text.ends_with("Caf\u{0}e9.png"), "{text}");
        let error = call(&webview, "read_image", json!({ "path": text })).unwrap_err();
        // Allowed (the same path), just not on this disk.
        assert_eq!(
            error,
            json!("This photo was moved or deleted. Choose it again with Replace…")
        );
    }

    #[test]
    fn an_unsaved_show_is_asked_to_be_saved_before_a_search() {
        let (_app, webview, _dir) = app();
        let error = call(&webview, "find_missing_files", json!({})).unwrap_err();
        assert_eq!(
            error,
            json!(
                "Save the show first, so PixelFlow knows which folder to look in. Or use Locate… to choose the file."
            )
        );
    }

    #[test]
    fn a_sequences_missing_music_is_found_again() {
        let (_app, webview, dir) = app();
        let music = dir.path().join("Seq/Music/Carol.wav");
        std::fs::create_dir_all(music.parent().unwrap()).unwrap();
        write_wav(&music);
        let file = dir.path().join("Seq/Carol.pfseq.json");
        call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Carol", "durationMs": 1000, "audio": music }),
        )
        .unwrap();
        call(&webview, "save_sequence_doc_as", json!({ "path": file })).unwrap();
        assert_eq!(
            call(&webview, "sequence_music_missing", json!({})).unwrap(),
            Value::Null
        );
        let moved = dir.path().join("Seq/Audio/Carol.wav");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&music, &moved).unwrap();
        let missing = call(&webview, "sequence_music_missing", json!({})).unwrap();
        assert_eq!(missing["message"], "Carol.wav isn't where it was.");
        let found = call(&webview, "find_sequence_music", json!({})).unwrap();
        assert_eq!(found["found"]["to"], json!(moved.to_str().unwrap()));
        assert_eq!(found["result"]["changed"], true);
        assert_eq!(
            call(&webview, "sequence_music_missing", json!({})).unwrap(),
            Value::Null
        );
        let again = call(&webview, "find_sequence_music", json!({})).unwrap();
        assert_eq!(again, json!({ "found": null, "result": null, "gaveUp": false }));
    }

    #[test]
    fn new_paths_are_checked_apart_from_edits() {
        let (_app, webview, dir) = app();
        let photo = dir.path().join("gone.png");
        let background = json!({ "path": photo, "x": 0, "y": 0, "width": 10, "opacity": 1 });
        let snapshot = call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "setBackground", "background": background }] }),
        )
        .unwrap();
        assert_eq!(snapshot["filesChecked"], false);
        assert_eq!(snapshot["missingFiles"], json!([]));
        let snapshot = call(&webview, "check_files", json!({ "all": false })).unwrap();
        assert_eq!(snapshot["filesChecked"], true);
        assert_eq!(snapshot["missingFiles"][0]["name"], "gone.png");
    }

    /// The recent shows, as the window gets them.
    fn recent_shows(webview: &WebviewWindow<MockRuntime>) -> Vec<Value> {
        call(webview, "list_recent_shows", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn shows_saved_opened_and_restored_go_to_the_top_of_the_recent_list() {
        let (_app, webview, dir) = app();
        assert!(recent_shows(&webview).is_empty());
        let house = dir.path().join("house.pixelflow.json");
        let prop = json!({
            "id": "11111111-0000-4000-8000-000000000001", "name": "Line",
            "shape": { "source": "generator", "type": "line", "nodes": 50, "length": 2 },
            "transform": { "position": { "x": 0, "y": 0, "z": 0 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 },
                           "scale": { "x": 1, "y": 1, "z": 1 } },
            "colorOrder": "RGB", "regions": [], "tags": []
        });
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "renameShow", "name": "House" }, { "type": "addProp", "prop": prop }] }),
        )
        .unwrap();
        call(&webview, "save_show_as", json!({ "path": house })).unwrap();
        let list = recent_shows(&webview);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "House");
        assert_eq!(list[0]["path"], json!(house.to_str().unwrap()));
        assert_eq!(list[0]["props"], 1);
        assert_eq!(list[0]["pixels"], 50);
        assert_eq!(list[0]["status"], "here");
        assert!(list[0]["thumbnail"].as_str().unwrap().starts_with("<svg"));

        // A new show isn't on the list until it's saved.
        call(&webview, "new_show", json!({ "name": "Shed" })).unwrap();
        assert_eq!(recent_shows(&webview).len(), 1);
        let shed = dir.path().join("shed.pixelflow.json");
        call(&webview, "save_show_as", json!({ "path": shed })).unwrap();
        let names: Vec<Value> = recent_shows(&webview).iter().map(|s| s["name"].clone()).collect();
        assert_eq!(names, vec![json!("Shed"), json!("House")]);

        // Opening brings a show back to the top; a failed open changes nothing.
        call(&webview, "open_show", json!({ "path": house })).unwrap();
        assert!(
            call(
                &webview,
                "open_show",
                json!({ "path": dir.path().join("nope.json") })
            )
            .is_err()
        );
        let names: Vec<Value> = recent_shows(&webview).iter().map(|s| s["name"].clone()).collect();
        assert_eq!(names, vec![json!("House"), json!("Shed")]);

        // Restoring an autosaved version keeps the show at the top, under its file.
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "renameShow", "name": "House 2" }] }),
        )
        .unwrap();
        autosave(&_app.state::<AppState>(), "test");
        let id = call(&webview, "list_history", json!({})).unwrap()[0]["id"].clone();
        call(&webview, "open_show", json!({ "path": shed })).unwrap();
        call(&webview, "open_show", json!({ "path": house })).unwrap();
        call(&webview, "restore_history", json!({ "id": id })).unwrap();
        let list = recent_shows(&webview);
        assert_eq!(list[0]["path"], json!(house.to_str().unwrap()));
        assert_eq!(list[0]["name"], "House 2");
    }

    #[test]
    fn recent_shows_that_are_gone_stay_listed_until_taken_off() {
        let (_app, webview, dir) = app();
        let a = dir.path().join("a.pixelflow.json");
        let b = dir.path().join("b.pixelflow.json");
        call(&webview, "save_show_as", json!({ "path": a })).unwrap();
        call(&webview, "save_show_as", json!({ "path": b })).unwrap();
        std::fs::remove_file(&a).unwrap();
        let list = recent_shows(&webview);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1]["status"], "missing");
        // Opening it fails, and it stays on the list.
        assert!(call(&webview, "open_show", json!({ "path": a })).is_err());
        assert_eq!(recent_shows(&webview).len(), 2);
        call(&webview, "forget_recent_show", json!({ "path": a })).unwrap();
        assert_eq!(recent_shows(&webview).len(), 1);
        call(&webview, "clear_recent_shows", json!({})).unwrap();
        assert!(recent_shows(&webview).is_empty());
        // The window can't name a show for the list: locating one needs it on the list.
        let error = call(&webview, "locate_recent_show", json!({ "path": a })).unwrap_err();
        assert_eq!(error, json!("That show isn't on your recent list any more."));
    }

    #[test]
    fn an_exit_asked_for_with_unsaved_work_waits_for_the_window_to_ask() {
        let (app, webview, dir) = app();
        let handle = app.handle();
        // Nothing unsaved: exit at once.
        assert!(!exit_must_wait(handle, Some(0)));
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [{ "type": "renameShow", "name": "Changed" }] }),
        )
        .unwrap();
        // Unsaved work, and the window is there to ask about it.
        assert!(exit_must_wait(handle, Some(0)));
        // The last window closed: it already asked (or there's nothing left to ask with).
        assert!(!exit_must_wait(handle, None));
        let path = pf_model::path_to_text(&dir.path().join("h.pixelflow.json"));
        call(&webview, "save_show_as", json!({ "path": path })).unwrap();
        assert!(!exit_must_wait(handle, Some(0)));
    }

    #[test]
    fn quit_from_the_menu_goes_through_the_window_and_quits_when_nothing_is_unsaved() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_without_window(dir.path());
        let handle = app.handle().clone();
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&events);
        std::thread::spawn(move || {
            // Once the running app has made its window.
            while handle.get_webview_window("main").is_none() {
                std::thread::sleep(Duration::from_millis(10));
            }
            menu::on_event(&handle, tauri::menu::MenuEvent { id: "quit".into() });
        });
        // Returns once the app has exited.
        app.run(move |app, event| {
            match &event {
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::CloseRequested { .. },
                    ..
                } => seen.lock().unwrap().push("close requested".into()),
                tauri::RunEvent::ExitRequested { .. } => seen.lock().unwrap().push("exit requested".into()),
                tauri::RunEvent::Exit => seen.lock().unwrap().push("exit".into()),
                _ => {}
            }
            on_run_event(app, event);
        });
        assert_eq!(
            *events.lock().unwrap(),
            vec!["close requested", "exit requested", "exit"]
        );
    }

    #[test]
    fn close_show_from_the_menu_is_ignored_while_another_window_is_in_front() {
        use tauri::Listener;
        let (app, _webview, _dir) = app();
        let sent = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&sent);
        app.listen_any(menu::MENU_EVENT, move |event| {
            seen.lock().unwrap().push(event.payload().to_string());
        });
        // The mock window never has focus: as if the About panel were in front.
        let choose = |id: &str| menu::on_event(app.handle(), tauri::menu::MenuEvent { id: id.into() });
        choose("close-show");
        choose("new-show");
        assert_eq!(*sent.lock().unwrap(), vec![r#"{"action":"newShow"}"#]);
    }

    #[test]
    fn a_dialog_asked_for_while_one_is_showing_is_refused_not_queued() {
        let (app, webview, _dir) = app();
        let state = app.state::<AppState>();
        let showing = state.dialog.take().unwrap();
        // Answered at once, as if cancelled: no second sheet is queued behind the first.
        assert_eq!(
            call(&webview, "pick_path", json!({ "kind": "show" })),
            Ok(Value::Null)
        );
        assert_eq!(call(&webview, "pick_image", json!({})), Ok(Value::Null));
        drop(showing);
    }

    #[test]
    fn the_sample_show_opens_as_an_unsaved_copy() {
        let (_app, webview, _dir) = app();
        let snapshot = call(&webview, "open_sample_show", json!({})).unwrap();
        assert_eq!(snapshot["show"]["name"], "Demo House");
        assert_eq!(snapshot["path"], Value::Null);
        // Nothing to ask about until the user changes it.
        assert_eq!(snapshot["dirty"], false);
        assert_eq!(snapshot["summary"]["props"], 4);
        assert_eq!(snapshot["summary"]["controllers"], 2);
        // Not a file of the user's: it isn't a recent show.
        assert!(recent_shows(&webview).is_empty());
    }

    #[test]
    fn dialogs_start_in_the_shows_folder_and_an_import_saves_first_into_its_xlights_folder() {
        use pickers::PickKind;
        let (app, webview, dir) = app();
        let state = app.state::<AppState>();
        let folders = |kind| pickers::starting_folders(app.handle(), &state, kind, None);
        let xlights = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/pf-xlights/fixtures/sample-show");
        call(&webview, "import_xlights", json!({ "folder": xlights })).unwrap();
        assert_eq!(folders(PickKind::ShowSave).first(), Some(&xlights));
        // Only for saving the show.
        assert_ne!(folders(PickKind::Music).first(), Some(&xlights));
        let saved = dir.path().join("Shows/house.pixelflow.json");
        std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
        call(&webview, "save_show_as", json!({ "path": saved })).unwrap();
        assert!(!folders(PickKind::ShowSave).contains(&xlights));
        assert!(folders(PickKind::Photo).contains(&saved.parent().unwrap().to_path_buf()));
        // A new show isn't the imported one any more.
        call(&webview, "import_xlights", json!({ "folder": xlights })).unwrap();
        call(&webview, "new_show", json!({ "name": "New" })).unwrap();
        assert!(!folders(PickKind::ShowSave).contains(&xlights));
    }

    #[test]
    fn path_text_from_the_window_is_read_back_exactly() {
        let arg: PathArg = serde_json::from_value(json!("/shows/Caf\u{0}e9.json")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(arg.as_os_str().as_bytes(), b"/shows/Caf\xe9.json");
        }
        let plain: PathArg = serde_json::from_value(json!("/shows/House.json")).unwrap();
        assert_eq!(&*plain, Path::new("/shows/House.json"));
    }

    #[test]
    fn a_show_file_moved_alone_finds_its_photo_where_it_was_saved_and_may_show_it() {
        let (_app, webview, dir) = app();
        let (show, photo, _) = show_with_files(dir.path());
        let alone = dir.path().join("Elsewhere/show.pixelflow.json");
        std::fs::create_dir_all(alone.parent().unwrap()).unwrap();
        std::fs::rename(&show, &alone).unwrap();
        // The photo also moved, within the folder the show was saved in.
        let moved = show.parent().unwrap().join("pictures/house.png");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&photo, &moved).unwrap();
        let snapshot = call(&webview, "open_show", json!({ "path": alone })).unwrap();
        assert_eq!(snapshot["filesChecked"], true);
        let missing = &snapshot["missingFiles"][0];
        assert_eq!(missing["wasAt"], json!(photo.to_str().unwrap()));
        assert!(call(&webview, "read_image", json!({ "path": moved })).is_err());
        let report = call(&webview, "find_missing_files", json!({})).unwrap();
        assert_eq!(report["found"][0]["to"], json!(moved.to_str().unwrap()));
        assert_eq!(
            call_raw(&webview, "read_image", json!({ "path": moved })).unwrap(),
            PNG
        );
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

    fn ids(changes: &Value) -> Vec<String> {
        changes
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn an_import_can_wire_props_already_in_the_show() {
        let (_app, webview, _dir) = app();
        let falcon = pf_devices::testing::FALCON;
        let details = call(&webview, "inspect_device", json!({ "address": falcon })).unwrap();
        let first = details["config"]["ports"][0]["number"].as_u64().unwrap();
        let prop = pf_model::Prop::new(
            "Garage Arch",
            pf_model::ShapeSource::Generator(pf_model::Generator::Line {
                nodes: 50,
                length: 2.0,
            }),
        );
        let id = prop.id;
        call(
            &webview,
            "apply_edits",
            json!({ "edits": [Edit::AddProp { prop }] }),
        )
        .unwrap();
        let key = format!("port{first}/string1");
        let snapshot = call(
            &webview,
            "import_device",
            json!({ "address": falcon, "useProps": { key: id } }),
        )
        .unwrap();
        // Three strings: one wires Garage Arch, two get starter props.
        assert_eq!(snapshot["summary"]["props"], 3);
        assert_eq!(
            snapshot["show"]["controllers"][0]["ports"][0]["slots"][0]["prop"],
            json!(id)
        );
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
        let restored = call(&webview, "restore_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(restored["restored"], true, "{restored}");
        assert_eq!(fpp.state().pixel_strings.clone().unwrap(), saved);
        // The copy stays until it's dismissed.
        call(&webview, "forget_device_setup_copy", json!({ "address": host })).unwrap();
        let error = call(&webview, "restore_device_setup", json!({ "address": host })).unwrap_err();
        assert_eq!(error, json!("There's no earlier setup to put back."));
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
        let restored = call(&webview, "restore_device_setup", json!({ "address": host })).unwrap();
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
        let restored = call(&webview, "restore_device_setup", json!({ "address": host })).unwrap();
        assert_eq!(restored["restored"], false);
        fpp.state().fail_config_writes = None;
        let restored = call(&webview, "restore_device_setup", json!({ "address": host })).unwrap();
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
