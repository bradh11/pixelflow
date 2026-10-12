//! The shader files Shader effects run: listing the ones an effect can use, showing what one
//! is and takes in the effect's settings, and choosing one from the disk.
//!
//! A shader file the user chooses (the system's file dialog, shown here) is copied into a
//! `shaders` folder next to the show file, and the effect stores `shaders/<name>`. Only that
//! folder and files chosen this session are ever read (see `pf_engine::ShaderFiles`): a path the
//! window puts in an effect is not enough on its own. PixelFlow's own shaders are part of the
//! program. As with the show's other files, the disk is never read while the engine is locked,
//! and neither is a shader compiled there.
//!
//! A shader file is a program from a file. What the window is sent of one is what its header
//! says (a description, a credit, its inputs) and why it can't be used, if it can't: never the
//! program, and nothing the window does runs it. It is run only by the renderer, under
//! `pf_shader`'s limits.

use crate::pickers::{Pick, PickKind};
use crate::{AppState, Reply};
use pf_engine::ShaderInfo;
use pf_sequence::EffectId;
use tauri::{Emitter, State};

/// Runs `work` (which reads the disk and compiles) away from the engine and the window.
async fn off_lock<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong looking at the shader files.".to_string())
}

/// Tells the window whenever a shader file read in the background has arrived (or couldn't be
/// read), so it draws the frame again: the same word a picture's arrival sends.
pub(crate) fn report_arrivals<R: tauri::Runtime>(app: &tauri::AppHandle<R>, engine: &pf_engine::Engine) {
    let app = app.clone();
    engine.shaders().on_arrival(Some(std::sync::Arc::new(move || {
        // A window that's gone has nothing to draw.
        let _ = app.emit(crate::pictures::PICTURES_EVENT, ());
    })));
}

/// Every shader a Shader effect can use: PixelFlow's own, then the files in the show's shaders
/// folder, each with what it says of itself and its inputs (or why it can't be used).
#[tauri::command]
pub(crate) async fn list_shaders(state: State<'_, AppState>) -> Reply<Vec<ShaderInfo>> {
    let (files, shaders) = {
        let engine = state.engine();
        (engine.shader_files(), engine.shaders())
    };
    off_lock(move || pf_engine::list_shaders(&files, &shaders)).await
}

/// What the shader a Shader effect's `file` setting names is and takes, read and compiled now
/// if it hasn't been. A file that isn't there, may not be read, or can't be run comes back with
/// why (and the line, when it's on one); that isn't an error.
#[tauri::command]
pub(crate) async fn shader_info(state: State<'_, AppState>, file: String) -> Reply<ShaderInfo> {
    let shaders = state.engine().shaders();
    off_lock(move || shaders.describe(&file)).await
}

/// Asks the user for a shader file for a Shader effect. The one they pick is copied into the
/// show's shaders folder (while the show isn't saved, it's used where it is), and may then be
/// read. Returns it as an effect names it, with what it is and takes; `None` when they cancel.
#[tauri::command]
pub(crate) async fn pick_shader<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<ShaderInfo>> {
    let Some(path) = crate::pickers::pick(&app, &state, Pick::of(PickKind::Shader)).await? else {
        return Ok(None);
    };
    let (files, shaders) = {
        let engine = state.engine();
        (engine.shader_files(), engine.shaders())
    };
    let info = off_lock(move || {
        let stored = files.adopt(&path)?;
        // Read afresh: a file of that name may have been tried before it was there.
        shaders.forget(&stored);
        Ok::<_, String>(shaders.describe(&stored))
    })
    .await??;
    Ok(Some(info))
}

/// Why the open sequence's Shader effect `effect` drew nothing the last time it was drawn, when
/// it was stopped for doing more work than a shader may; `None` when it wasn't.
#[tauri::command]
pub(crate) fn shader_stopped(state: State<'_, AppState>, effect: EffectId) -> Reply<Option<String>> {
    Ok(state.engine().shader_stopped(effect))
}
