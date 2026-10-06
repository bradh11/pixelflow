//! The macOS menu bar: the standard menus, with a File menu for shows (New, Open…, Open Recent,
//! Close Show, Save, Save As…). The window does the work: each item sends it a [`MENU_EVENT`],
//! and it runs the same action as its show menu and shortcuts (asking about unsaved changes
//! first). A recent show chosen here is opened through `open_show` like any other.
//!
//! The window sees ⌘-keys before the menu does, so its shortcuts keep working; the menu's
//! key equivalents only act when the window leaves a key alone.
//!
//! Quit (⌘Q) is the app's own item, not the system's `terminate:`, which would end the app
//! without asking: it closes the window like its close button, so unsaved work is asked about
//! first, and the app quits once the window has closed.

use crate::recent::RecentShows;
use serde::Serialize;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The event the window gets when a File menu item is chosen.
pub(crate) const MENU_EVENT: &str = "menu";

const FILE: &str = "file";
const OPEN_RECENT: &str = "open-recent";
const RECENT_PREFIX: &str = "recent:";
const CLEAR_RECENT: &str = "clear-recent";
const CLOSE_WINDOW: &str = "close-window";
const QUIT: &str = "quit";

/// What the window is asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub(crate) enum MenuAction {
    NewShow,
    OpenShow,
    OpenRecent { path: String },
    ClearRecent,
    CloseShow,
    Save,
    SaveAs,
}

/// The action for a menu item's id (a recent show only while it's still on the list).
pub(crate) fn action_for(id: &str, recent: &RecentShows) -> Option<MenuAction> {
    Some(match id {
        "new-show" => MenuAction::NewShow,
        "open-show" => MenuAction::OpenShow,
        "close-show" => MenuAction::CloseShow,
        "save" => MenuAction::Save,
        "save-as" => MenuAction::SaveAs,
        CLEAR_RECENT => MenuAction::ClearRecent,
        _ => {
            let path = id.strip_prefix(RECENT_PREFIX)?;
            recent.names().iter().find(|(p, _)| p == path)?;
            MenuAction::OpenRecent {
                path: path.to_string(),
            }
        }
    })
}

/// The menu bar (macOS), with the recent shows as they are now.
pub(crate) fn build<R: Runtime>(app: &AppHandle<R>, recent: &RecentShows) -> tauri::Result<Menu<R>> {
    let item =
        |id: &str, text: &str, accelerator: Option<&str>| MenuItem::with_id(app, id, text, true, accelerator);
    let open_recent = Submenu::with_id(app, OPEN_RECENT, "Open Recent", true)?;
    fill_recent(app, &open_recent, recent)?;
    let file = Submenu::with_id_and_items(
        app,
        FILE,
        "File",
        true,
        &[
            &item("new-show", "New Show", Some("CmdOrCtrl+N"))?,
            &item("open-show", "Open…", Some("CmdOrCtrl+O"))?,
            &open_recent,
            &PredefinedMenuItem::separator(app)?,
            &item("close-show", "Close Show", Some("CmdOrCtrl+W"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("save", "Save", Some("CmdOrCtrl+S"))?,
            &item("save-as", "Save As…", Some("CmdOrCtrl+Shift+S"))?,
        ],
    )?;
    let name = app.package_info().name.clone();
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                name,
                true,
                &[
                    &PredefinedMenuItem::about(app, None, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::show_all(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &item(QUIT, "Quit PixelFlow", Some("CmdOrCtrl+Q"))?,
                ],
            )?,
            &file,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(app, "View", true, &[&PredefinedMenuItem::fullscreen(app, None)?])?,
            &Submenu::with_items(
                app,
                "Window",
                true,
                &[
                    &PredefinedMenuItem::minimize(app, None)?,
                    &PredefinedMenuItem::maximize(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    // ⌘W closes the show; the window closes with ⇧⌘W (asking about unsaved
                    // work first, like its close button).
                    &item(CLOSE_WINDOW, "Close Window", Some("CmdOrCtrl+Shift+W"))?,
                ],
            )?,
        ],
    )
}

/// Puts the recent shows (and Clear Recent Shows) in the Open Recent submenu.
fn fill_recent<R: Runtime>(
    app: &AppHandle<R>,
    submenu: &Submenu<R>,
    recent: &RecentShows,
) -> tauri::Result<()> {
    for item in submenu.items()? {
        submenu.remove(&item)?;
    }
    let shows = recent.names();
    if shows.is_empty() {
        submenu.append(&MenuItem::with_id(
            app,
            "no-recent",
            "No Recent Shows",
            false,
            None::<&str>,
        )?)?;
    }
    for (path, name) in &shows {
        let label = format!("{name} — {}", pf_model::file_name_of(path));
        submenu.append(&MenuItem::with_id(
            app,
            format!("{RECENT_PREFIX}{path}"),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    submenu.append(&PredefinedMenuItem::separator(app)?)?;
    submenu.append(&MenuItem::with_id(
        app,
        CLEAR_RECENT,
        "Clear Recent Shows",
        !shows.is_empty(),
        None::<&str>,
    )?)?;
    Ok(())
}

/// Brings File → Open Recent up to date (when the app has a menu bar).
pub(crate) fn refresh_recent<R: Runtime>(app: &AppHandle<R>, recent: &RecentShows) {
    let Some(menu) = app.menu() else { return };
    let submenu = menu
        .get(FILE)
        .and_then(|file| file.as_submenu().and_then(|f| f.get(OPEN_RECENT)))
        .and_then(|item| item.as_submenu().cloned());
    if let Some(submenu) = submenu
        && let Err(error) = fill_recent(app, &submenu, recent)
    {
        log::warn!("couldn't update Open Recent: {error}");
    }
}

/// Hands a chosen menu item to the window.
pub(crate) fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    if id == CLOSE_WINDOW || id == QUIT {
        // Asks about unsaved work first, like the window's close button; the app quits when
        // its last window has closed.
        match app.get_webview_window("main") {
            Some(window) => {
                let _ = window.close();
            }
            None if id == QUIT => app.exit(0),
            None => {}
        }
        return;
    }
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    if let Some(action) = action_for(id, &state.recent) {
        let _ = app.emit_to("main", MENU_EVENT, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recent::Visit;

    #[test]
    fn items_become_actions_and_recent_shows_must_be_on_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let recent = RecentShows::new(Some(dir.path().to_path_buf()));
        let show = dir.path().join("house.pixelflow.json");
        recent.record(
            Visit {
                path: show.clone(),
                name: "House".into(),
                props: 0,
                pixels: 0,
                controllers: 0,
                points: vec![],
            },
            1,
        );
        let text = pf_model::path_to_text(&show);
        assert_eq!(action_for("new-show", &recent), Some(MenuAction::NewShow));
        assert_eq!(action_for("close-show", &recent), Some(MenuAction::CloseShow));
        assert_eq!(
            action_for(&format!("recent:{text}"), &recent),
            Some(MenuAction::OpenRecent { path: text })
        );
        assert_eq!(action_for("recent:/etc/passwd", &recent), None);
        assert_eq!(action_for("something-else", &recent), None);
        // Quit and Close Window go through the window's close (and its question), not here.
        assert_eq!(action_for(QUIT, &recent), None);
        assert_eq!(action_for(CLOSE_WINDOW, &recent), None);
        assert_eq!(
            serde_json::to_value(MenuAction::OpenRecent { path: "/a".into() }).unwrap(),
            serde_json::json!({ "action": "openRecent", "path": "/a" })
        );
    }
}
