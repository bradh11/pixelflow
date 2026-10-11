//! The preview in a window of its own ("pop out"): the main window's room goes to the controls,
//! and the lights can go to another display, full screen there.
//!
//! The shell makes the window. The main window asks for it by command and has no permission to
//! make windows itself, so there is only ever this one: labelled [`LABEL`], showing the app's
//! own page with `?window=preview`, which draws nothing but the preview.
//!
//! It is given only what it needs:
//! - no core or plugin permissions at all (`capabilities/preview.json`): no events, no window
//!   or webview APIs, no dialogs;
//! - of the app's own commands, only [`COMMANDS`]: what it draws (the show, where its pixels
//!   are, frames, where playback is), the link below, and filling its own display. Anything
//!   else is refused before it runs (see [`may_call`]), so it can't edit, save, read other
//!   files, reach devices, or start, stop, or move playback.
//!
//! The two windows stay in step through the engine (both ask it for frames and where playback
//! is) and through [`preview_link`]: each leaves a small note for the other and reads the
//! other's. The main window's says what its preview would show (the open sequence at its
//! playhead, or the lights live) and how (2D or 3D, glow); the preview's says what was chosen
//! there, and counts presses of play/pause there, which the main window then acts on.

use crate::{AppState, Reply};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// The preview window's label.
pub(crate) const LABEL: &str = "preview";
/// The main window's label (see `tauri.conf.json`).
pub(crate) const MAIN: &str = "main";
/// The app's page, told to draw only the preview.
const PAGE: &str = "index.html?window=preview";
const TITLE: &str = "PixelFlow Preview";
const PLACE_FILE: &str = "preview-window.json";
/// The window's size until it has been somewhere, and the least it can be made.
const DEFAULT_SIZE: (f64, f64) = (960.0, 600.0);
const MIN_SIZE: (f64, f64) = (320.0, 220.0);
/// A note is a handful of numbers and a name: anything much longer isn't one.
const MAX_NOTE_BYTES: usize = 16 * 1024;
/// This much of the window must be on a display for it to open where it was.
const MIN_ON_DISPLAY: i64 = 80;

/// Every command the preview's window may call. All but the last three only read.
pub(crate) const COMMANDS: &[&str] = &[
    "get_snapshot",
    "preview_props",
    "preview_props_3d",
    "read_image",
    "read_house_model",
    "live_frame",
    "sequence_doc_frame",
    "playback_status",
    "preview_link",
    "set_fullscreen",
    "is_fullscreen",
];

/// What a window is told when it asks for something it may not.
pub(crate) const REFUSED: &str = "This window can't do that.";

/// Whether the window labelled `window` may call `command`: the main window any of them, the
/// preview's window its few, and any other window none.
pub(crate) fn may_call(window: &str, command: &str) -> bool {
    window == MAIN || (window == LABEL && COMMANDS.contains(&command))
}

/// Where the preview's window was, in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Place {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// A display's place among the others, in screen pixels: left, top, width, height.
type Display = (i32, i32, u32, u32);

/// Whether enough of a window at `place` would be on one of `displays` to find and drag it (a
/// display it was on may have been unplugged since).
fn on_a_display(place: Place, displays: &[Display]) -> bool {
    let (left, top) = (i64::from(place.x), i64::from(place.y));
    let (right, bottom) = (left + i64::from(place.width), top + i64::from(place.height));
    displays.iter().any(|&(x, y, width, height)| {
        let (x, y) = (i64::from(x), i64::from(y));
        let across = right.min(x + i64::from(width)) - left.max(x);
        let down = bottom.min(y + i64::from(height)) - top.max(y);
        across >= MIN_ON_DISPLAY && down >= MIN_ON_DISPLAY
    })
}

/// What the two windows last told each other.
#[derive(Default)]
struct Notes {
    main: Option<Value>,
    preview: Option<Value>,
}

/// What a window gets back from [`preview_link`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Linked {
    /// The other window's latest note, if it has left one.
    theirs: Option<Value>,
    /// Whether the preview's window is open.
    open: bool,
}

/// The preview window's state: whether it's open, the notes, and where it was last (kept in
/// `preview-window.json` in the config folder).
pub(crate) struct PreviewState {
    open: AtomicBool,
    /// Held while the window is made, so asking twice at once makes one.
    opening: Mutex<()>,
    notes: Mutex<Notes>,
    file: Option<PathBuf>,
}

impl PreviewState {
    pub(crate) fn new(config_dir: Option<PathBuf>) -> Self {
        Self {
            open: AtomicBool::new(false),
            opening: Mutex::default(),
            notes: Mutex::default(),
            file: config_dir.map(|d| d.join(PLACE_FILE)),
        }
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Where the window was when it last closed, if that was kept.
    fn place(&self) -> Option<Place> {
        let text = crate::recent::read_small(self.file.as_ref()?, crate::recent::MAX_LIST_BYTES)?;
        serde_json::from_str(&text).ok()
    }

    fn remember(&self, place: Place) {
        let Some(file) = &self.file else { return };
        let json = serde_json::to_vec(&place).unwrap_or_default();
        if let Err(error) = crate::recent::write_atomic(file, &json) {
            log::warn!("couldn't remember where the preview window was: {error}");
        }
    }

    /// Leaves `note` (when given) from the main window, or from the preview's, and hands back
    /// the other's. Once the preview's window has closed, what it said is forgotten.
    fn exchange(&self, from_preview: bool, note: Option<Value>) -> Reply<Linked> {
        let open = self.is_open();
        let mut notes = self.notes.lock().unwrap_or_else(PoisonError::into_inner);
        if !open {
            notes.preview = None;
        }
        if let Some(note) = note {
            let size = serde_json::to_vec(&note).map_or(usize::MAX, |bytes| bytes.len());
            if size > MAX_NOTE_BYTES {
                return Err("That is too much to tell the other window.".to_string());
            }
            if !from_preview {
                notes.main = Some(note);
            } else if open {
                notes.preview = Some(note);
            }
        }
        let theirs = if from_preview { &notes.main } else { &notes.preview };
        Ok(Linked {
            theirs: theirs.clone(),
            open,
        })
    }
}

/// Opens the preview's window where it was last (when that is still on a display), or brings
/// it to the front when it's already open.
pub(crate) fn open<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let _opening = state
        .preview
        .opening
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if state.preview.is_open()
        && let Some(window) = app.get_webview_window(LABEL)
    {
        let _ = window.unminimize();
        return window.set_focus();
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(PAGE.into()))
        .title(TITLE)
        .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        // Shown once it's where it was, so it doesn't jump there.
        .visible(false)
        .build()?;
    if let Some(place) = state.preview.place()
        && on_a_display(place, &displays(&window))
    {
        let _ = window.set_position(PhysicalPosition::new(place.x, place.y));
        let _ = window.set_size(PhysicalSize::new(place.width, place.height));
    }
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        // Its own close button: where it is now is where it opens next time.
        WindowEvent::CloseRequested { .. } => keep_place(&handle),
        WindowEvent::Destroyed => {
            if let Some(state) = handle.try_state::<AppState>() {
                state.preview.open.store(false, Ordering::Release);
            }
        }
        _ => {}
    });
    state.preview.open.store(true, Ordering::Release);
    window.show()?;
    window.set_focus()
}

/// Closes the preview's window, if it's open, remembering where it was.
pub(crate) fn close<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if !state.preview.open.swap(false, Ordering::AcqRel) {
        return;
    }
    keep_place(app);
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// A window has gone. When it's the main one the preview's goes with it: nothing would feed it,
/// and the app is over once its last window has closed.
pub(crate) fn window_gone<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if label == MAIN {
        close(app);
    }
}

/// Close Window from the menu bar: closes the preview's window when it's the one in front.
/// True when it was.
pub(crate) fn close_if_in_front<R: Runtime>(app: &AppHandle<R>) -> bool {
    let in_front = app
        .get_webview_window(LABEL)
        .is_some_and(|window| window.is_focused().unwrap_or(false));
    if in_front {
        close(app);
    }
    in_front
}

/// Remembers where the preview's window is, unless it fills its display or is put away in the
/// dock or taskbar (it has no place of its own then).
fn keep_place<R: Runtime>(app: &AppHandle<R>) {
    let (Some(window), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<AppState>()) else {
        return;
    };
    if window.is_fullscreen().unwrap_or(true) || window.is_minimized().unwrap_or(true) {
        return;
    }
    if let (Ok(at), Ok(size)) = (window.outer_position(), window.inner_size())
        && size.width > 0
        && size.height > 0
    {
        state.preview.remember(Place {
            x: at.x,
            y: at.y,
            width: size.width,
            height: size.height,
        });
    }
}

fn displays<R: Runtime>(window: &WebviewWindow<R>) -> Vec<Display> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| (m.position().x, m.position().y, m.size().width, m.size().height))
        .collect()
}

/// Opens the preview in a window of its own (or brings that window to the front).
#[tauri::command]
pub(crate) async fn open_preview_window<R: Runtime>(app: AppHandle<R>) -> Reply<()> {
    open(&app).map_err(|e| format!("PixelFlow couldn't open the preview window: {e}"))
}

/// Closes the preview's window ("bring it back").
#[tauri::command]
pub(crate) async fn close_preview_window<R: Runtime>(app: AppHandle<R>) -> Reply<()> {
    close(&app);
    Ok(())
}

/// Leaves `note` for the other window (when given) and answers with the other's latest, and
/// whether the preview's window is open. Which note is whose goes by the window asking, not by
/// anything it says.
#[tauri::command]
pub(crate) async fn preview_link<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    note: Option<Value>,
) -> Reply<Linked> {
    state.preview.exchange(window.label() == LABEL, note)
}

/// Makes the window that asks fill its display, or puts it back: only ever that window. The
/// preview's window fills whichever display it was dragged to.
#[tauri::command]
pub(crate) async fn set_fullscreen<R: Runtime>(window: WebviewWindow<R>, on: bool) -> Reply<()> {
    window
        .set_fullscreen(on)
        .map_err(|e| format!("PixelFlow couldn't change to full screen: {e}"))
}

/// Whether the window that asks fills its display now (the system's own button changes it too).
#[tauri::command]
pub(crate) async fn is_fullscreen<R: Runtime>(window: WebviewWindow<R>) -> Reply<bool> {
    Ok(window.is_fullscreen().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LAPTOP: Display = (0, 0, 2880, 1800);
    const BESIDE: Display = (2880, 0, 3840, 2160);

    fn place(x: i32, y: i32) -> Place {
        Place {
            x,
            y,
            width: 1200,
            height: 800,
        }
    }

    #[test]
    fn the_preview_window_is_allowed_only_what_it_draws_with() {
        for command in COMMANDS {
            assert!(may_call(LABEL, command), "{command}");
            assert!(may_call(MAIN, command), "{command}");
        }
        // Nothing that changes the show, a sequence, a file, a device, or playback.
        for command in [
            "apply_edits",
            "save_show",
            "open_show",
            "edit_sequence",
            "start_playback",
            "play_sequence",
            "play_sequence_doc",
            "pause_playback",
            "seek_playback",
            "stop_playback",
            "start_output",
            "fpp_stop",
            "pick_path",
            "open_preview_window",
            "close_preview_window",
            "ai_send",
        ] {
            assert!(!may_call(LABEL, command), "{command}");
            assert!(may_call(MAIN, command), "{command}");
        }
        // A window the shell didn't make gets nothing.
        assert!(!may_call("another", "get_snapshot"));
    }

    #[test]
    fn a_window_opens_where_it_was_only_while_that_is_still_on_a_display() {
        assert!(on_a_display(place(100, 100), &[LAPTOP]));
        // On the display beside the laptop's, and still there.
        assert!(on_a_display(place(3200, 200), &[LAPTOP, BESIDE]));
        // That display was unplugged.
        assert!(!on_a_display(place(3200, 200), &[LAPTOP]));
        // A corner still showing is enough to drag it back by, a sliver isn't.
        assert!(on_a_display(place(2700, 1600), &[LAPTOP]));
        assert!(!on_a_display(place(2860, 100), &[LAPTOP]));
        // Left of and above the first display (another one is arranged there).
        assert!(on_a_display(
            place(-1500, -700),
            &[LAPTOP, (-1920, -1080, 1920, 1080)]
        ));
        assert!(!on_a_display(place(100, 100), &[]));
    }

    #[test]
    fn where_the_window_was_is_kept_between_runs() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        assert_eq!(PreviewState::new(Some(config.clone())).place(), None);
        PreviewState::new(Some(config.clone())).remember(place(3200, 200));
        assert_eq!(
            PreviewState::new(Some(config.clone())).place(),
            Some(place(3200, 200))
        );
        // A file that isn't a place is passed over.
        std::fs::write(config.join(PLACE_FILE), "{").unwrap();
        assert_eq!(PreviewState::new(Some(config)).place(), None);
        // Nowhere to keep it: nothing is remembered, and nothing fails.
        let nowhere = PreviewState::new(None);
        nowhere.remember(place(1, 1));
        assert_eq!(nowhere.place(), None);
    }

    #[test]
    fn each_window_reads_the_others_note() {
        let state = PreviewState::new(None);
        // Before the preview's window is open, the main window's note waits for it.
        let linked = state.exchange(false, Some(json!({ "source": "live" }))).unwrap();
        assert_eq!(
            linked,
            Linked {
                theirs: None,
                open: false
            }
        );
        state.open.store(true, Ordering::Release);
        assert_eq!(
            state.exchange(true, None).unwrap().theirs,
            Some(json!({ "source": "live" }))
        );
        state.exchange(true, Some(json!({ "toggles": 1 }))).unwrap();
        let linked = state.exchange(false, None).unwrap();
        assert_eq!(linked.theirs, Some(json!({ "toggles": 1 })));
        assert!(linked.open);
        // Reading doesn't use a note up, and a new one replaces the old.
        state
            .exchange(false, Some(json!({ "source": "sequence" })))
            .unwrap();
        assert_eq!(
            state.exchange(true, None).unwrap().theirs,
            Some(json!({ "source": "sequence" }))
        );
        // Closed: what it said is forgotten, so the next window starts afresh.
        state.open.store(false, Ordering::Release);
        assert_eq!(
            state.exchange(false, None).unwrap(),
            Linked {
                theirs: None,
                open: false
            }
        );
        state.exchange(true, Some(json!({ "toggles": 2 }))).unwrap();
        state.open.store(true, Ordering::Release);
        assert_eq!(state.exchange(false, None).unwrap().theirs, None);
    }

    #[test]
    fn a_note_too_long_to_be_one_is_refused() {
        let state = PreviewState::new(None);
        let long = json!({ "title": "x".repeat(MAX_NOTE_BYTES) });
        assert!(state.exchange(false, Some(long)).is_err());
        assert_eq!(state.exchange(true, None).unwrap().theirs, None);
    }
}
