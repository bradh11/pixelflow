//! The PixelFlow desktop shell: a thin bridge between the React UI and `pf-engine`.
//! Every command locks the engine, calls it, and returns its result as JSON. Commands are
//! `async` so they run off the UI thread (opening files and resolving controller addresses
//! can take a moment).

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
        .run(context())
        .expect("error while running PixelFlow");
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
}
