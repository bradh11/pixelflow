//! The house model for the 3D view: picking its file, and reading it for the window.

use crate::layout::{PickedPhotos, label, open_without_waiting};
use crate::{AppState, Reply};
use std::io::{self, Read};
use std::path::Path;
use tauri::State;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

/// Model files the 3D view can show: glTF (binary or self-contained text) and OBJ.
pub(crate) const MODEL_EXTENSIONS: &[&str] = &["glb", "gltf", "obj"];
/// Larger models are refused rather than loaded into the window.
const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;

/// Models the window may read: ones the user picked in the model dialog this session, and the
/// house models of shows read from disk (see `AppState::trust_files_of`).
pub(crate) type PickedModels = PickedPhotos;

/// Asks the user for a 3D model of their house; the one they pick may then be read.
#[tauri::command]
pub(crate) async fn pick_house_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<String>> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .add_filter("3D model", MODEL_EXTENSIONS)
            .set_title("Choose a 3D model of your house")
            .blocking_pick_file()
    })
    .await
    .map_err(|_| "Something went wrong opening the model dialog.".to_string())?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let text = pf_model::path_to_text(&path);
    state.models.add(path);
    Ok(Some(text))
}

/// The bytes of a house model, sent raw. Only models the user picked, or that came with a show
/// read from disk, are read, and only if they really are model files. A path the window put
/// in the show with an edit is not enough on its own.
#[tauri::command]
pub(crate) async fn read_house_model(state: State<'_, AppState>, path: String) -> Reply<Response> {
    let path = pf_model::path_from_text(&path);
    if !state.models.contains(&path) {
        return Err(
            "PixelFlow can only show a model you picked. Choose it with Add house model… or Replace…".into(),
        );
    }
    tauri::async_runtime::spawn_blocking(move || read_model_file(&path))
        .await
        .map_err(|_| "Something went wrong reading the model.".to_string())?
        .map(Response::new)
}

fn read_error(name: &str, error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "This model was moved or deleted. Choose it again with Replace…".into(),
        io::ErrorKind::PermissionDenied => format!(
            "PixelFlow isn't allowed to read {name}. Check the file's permissions, or choose another model."
        ),
        kind => format!("Could not read {name} ({kind})."),
    }
}

fn too_large(name: &str, bytes: u64) -> String {
    format!(
        "{name} is too large to show ({} MB). Choose a model under {} MB.",
        bytes / (1024 * 1024),
        MAX_MODEL_BYTES / (1024 * 1024)
    )
}

/// True when `bytes` look like a model of the kind its `extension` says: a binary glTF starts
/// with "glTF", a text glTF is JSON, and an OBJ is text.
fn looks_like_model(extension: &str, bytes: &[u8]) -> bool {
    match extension {
        "glb" => bytes.starts_with(b"glTF"),
        "gltf" => bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{'),
        _ => !bytes.is_empty() && !bytes.iter().take(4096).any(|&b| b == 0),
    }
}

/// Reads a model file. The checks are made on the opened file itself, and at most
/// `MAX_MODEL_BYTES` are ever read.
fn read_model_file(path: &Path) -> Result<Vec<u8>, String> {
    let name = label(path);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let not_a_model =
        || format!("{name} isn't a 3D model PixelFlow can show. Choose a GLB, glTF, or OBJ file.");
    if !MODEL_EXTENSIONS.contains(&extension.as_str()) {
        return Err(not_a_model());
    }
    let file = open_without_waiting(path).map_err(|e| read_error(&name, &e))?;
    let metadata = file.metadata().map_err(|e| read_error(&name, &e))?;
    if !metadata.is_file() {
        return Err(not_a_model());
    }
    if metadata.len() > MAX_MODEL_BYTES {
        return Err(too_large(&name, metadata.len()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_MODEL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| read_error(&name, &e))?;
    if bytes.len() as u64 > MAX_MODEL_BYTES {
        return Err(too_large(&name, bytes.len() as u64));
    }
    if !looks_like_model(&extension, &bytes) {
        return Err(not_a_model());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_model_files_and_refuses_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [
            ("house.glb", &b"glTF\x02\0\0\0"[..]),
            ("House.GLTF", b"  {\"asset\":{\"version\":\"2.0\"}}"),
            ("house.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(read_model_file(&path).unwrap(), bytes, "{name}");
        }
        for (name, bytes) in [
            ("photo.png", &b"\x89PNG"[..]),
            ("fake.glb", b"not a model"),
            ("fake.gltf", b"<html>"),
            ("binary.obj", b"v 0\0\0\0"),
            ("empty.obj", b""),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let error = read_model_file(&path).unwrap_err();
            assert!(error.contains("isn't a 3D model"), "{name}: {error}");
        }
        assert_eq!(
            read_model_file(&dir.path().join("gone.glb")).unwrap_err(),
            "This model was moved or deleted. Choose it again with Replace…"
        );
        let folder = dir.path().join("folder.glb");
        std::fs::create_dir(&folder).unwrap();
        assert!(read_model_file(&folder).is_err());
    }
}
