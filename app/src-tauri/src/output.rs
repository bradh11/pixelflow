//! Live output commands: lighting a test pattern on the controllers (or setting a DMX fixture's
//! channels by hand), stopping it, and what it's doing now; and the serial ports a USB DMX
//! interface could be on.

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

/// Sets the channels of the DMX fixture being tested by hand, without stopping output.
#[tauri::command]
pub(crate) async fn set_test_channels(state: State<'_, AppState>, channels: Vec<u8>) -> Reply<OutputStatus> {
    Ok(state.engine().set_test_channels(channels))
}

/// The serial ports a USB DMX interface could be on, with what each device is (none where they
/// can't be listed: the user types one). Lists them only; opens none.
#[tauri::command]
pub(crate) async fn list_serial_ports() -> Reply<Vec<pf_engine::SerialPortChoice>> {
    pf_engine::serial_ports().map_err(|e| e.to_string())
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
        // Setting channels by hand with nothing running changes nothing.
        let status = call(&webview, "set_test_channels", json!({ "channels": [255, 0, 0] })).unwrap();
        assert_eq!(status["running"], false);
        assert!(call(&webview, "list_serial_ports", json!({})).unwrap().is_array());
    }
}
