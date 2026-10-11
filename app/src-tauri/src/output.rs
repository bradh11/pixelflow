//! Live output commands: lighting a test pattern on the controllers, stopping it, and what
//! it's doing now.

use crate::{AppState, Reply, message};
use pf_engine::{OutputStatus, PatternSpec, TargetSpec};
use tauri::State;

#[tauri::command]
pub(crate) async fn start_output(
    state: State<'_, AppState>,
    pattern: PatternSpec,
    target: TargetSpec,
) -> Reply<OutputStatus> {
    state.engine().start_output(pattern, target).map_err(message)
}

#[tauri::command]
pub(crate) async fn stop_output(state: State<'_, AppState>) -> Reply<OutputStatus> {
    Ok(state.engine().stop_output())
}

#[tauri::command]
pub(crate) async fn output_status(state: State<'_, AppState>) -> Reply<OutputStatus> {
    Ok(state.engine().output_status())
}

#[cfg(test)]
mod tests {
    use crate::testing::{app, call};
    use serde_json::{Value, json};

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
