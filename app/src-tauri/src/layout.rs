//! Layout editor support: the background photo, and every prop's pixel positions.

use crate::{AppState, Reply};
use pf_engine::PreviewProp;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use tauri::State;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

/// Image files the layout can show behind the props.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
/// Larger photos are refused rather than loaded into the window.
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// Photos the window may read: ones the user picked in the photo dialog this session. The
/// show's own background photo is always allowed too (see [`read_image`]).
#[derive(Default)]
pub(crate) struct PickedPhotos(Mutex<HashSet<PathBuf>>);

impl PickedPhotos {
    pub(crate) fn add(&self, path: PathBuf) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).insert(path);
    }

    fn contains(&self, path: &Path) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(path)
    }
}

/// Asks the user for a photo of their house; the one they pick may then be read.
#[tauri::command]
pub(crate) async fn pick_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<Option<PathBuf>> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .file()
            .add_filter("Photo", IMAGE_EXTENSIONS)
            .set_title("Choose a photo of your house")
            .blocking_pick_file()
    })
    .await
    .map_err(|_| "Something went wrong opening the photo dialog.".to_string())?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    state.photos.add(path.clone());
    Ok(Some(path))
}

/// The bytes of a background photo, sent raw so the window can show it without any file access
/// of its own. Only photos the user picked, or the show's own background photo, are read, and
/// only if they really are image files.
#[tauri::command]
pub(crate) async fn read_image(state: State<'_, AppState>, path: PathBuf) -> Reply<Response> {
    let is_background = state
        .engine()
        .show()
        .background
        .as_ref()
        .is_some_and(|b| Path::new(&b.path) == path);
    if !is_background && !state.photos.contains(&path) {
        return Err(
            "PixelFlow can only show a photo you picked. Choose it with Choose photo… or Replace…".into(),
        );
    }
    tauri::async_runtime::spawn_blocking(move || read_image_file(&path))
        .await
        .map_err(|_| "Something went wrong reading the photo.".to_string())?
        .map(Response::new)
}

/// The file's name for messages (its whole path if it has no name).
fn label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Opens a file for reading without ever waiting: a pipe or device named like a photo would
/// otherwise block until something writes to it.
fn open_without_waiting(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        File::open(path)
    }
}

fn read_error(name: &str, error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "This photo was moved or deleted. Choose it again with Replace…".into(),
        io::ErrorKind::PermissionDenied => format!(
            "PixelFlow isn't allowed to read {name}. Check the file's permissions, or choose another photo."
        ),
        kind => format!("Could not read {name} ({kind})."),
    }
}

fn too_large(name: &str, bytes: u64) -> String {
    format!(
        "{name} is too large to show ({} MB). Choose a photo under {} MB.",
        bytes / (1024 * 1024),
        MAX_IMAGE_BYTES / (1024 * 1024)
    )
}

/// True when `bytes` start like a PNG, JPEG, GIF, WebP, or BMP image.
fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

/// Reads an image file. The checks are made on the opened file itself, so nothing can be
/// swapped in between checking and reading, and at most `MAX_IMAGE_BYTES` are ever read.
fn read_image_file(path: &Path) -> Result<Vec<u8>, String> {
    let name = label(path);
    let is_image = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    let not_a_photo =
        || format!("{name} isn't a photo PixelFlow can show. Choose a PNG, JPEG, WebP, GIF, or BMP image.");
    if !is_image {
        return Err(not_a_photo());
    }
    let file = open_without_waiting(path).map_err(|e| read_error(&name, &e))?;
    let metadata = file.metadata().map_err(|e| read_error(&name, &e))?;
    if !metadata.is_file() {
        return Err(not_a_photo());
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(too_large(&name, metadata.len()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| read_error(&name, &e))?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(too_large(&name, bytes.len() as u64));
    }
    if !looks_like_image(&bytes) {
        return Err(not_a_photo());
    }
    Ok(bytes)
}

/// Every prop's pixel positions for the 2D views, raw (see [`encode_preview`]).
#[tauri::command]
pub(crate) async fn preview_props(state: State<'_, AppState>) -> Reply<Response> {
    let engine = state.engine();
    Ok(Response::new(encode_preview(
        engine.revision(),
        &engine.preview_props(),
    )))
}

/// Size of the preview header, and of each prop's entry after it.
const PREVIEW_HEADER: usize = 16;
const PREVIEW_ENTRY: usize = 48;

/// Packs the props' pixel positions into bytes the window reads without parsing (a large
/// show's positions as JSON would be megabytes of text after every edit). Little-endian:
///
/// - header: format `u32` (1), prop count `u32`, the show revision the positions are for `f64`
/// - one 48-byte entry per prop: its id as 36 ASCII characters, frame offset `u32`, channels
///   per pixel `u32`, pixel count `u32`
/// - then every prop's x, y pairs as `f32`, props in the same order
pub(crate) fn encode_preview(revision: u64, props: &[PreviewProp]) -> Vec<u8> {
    let floats: usize = props.iter().map(|p| p.points.len()).sum();
    let mut out = Vec::with_capacity(PREVIEW_HEADER + PREVIEW_ENTRY * props.len() + 4 * floats);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(props.len() as u32).to_le_bytes());
    out.extend_from_slice(&(revision as f64).to_le_bytes());
    for p in props {
        out.extend_from_slice(p.prop.to_string().as_bytes());
        out.extend_from_slice(&(p.frame_offset as u32).to_le_bytes());
        out.extend_from_slice(&u32::from(p.channels_per_pixel).to_le_bytes());
        out.extend_from_slice(&((p.points.len() / 2) as u32).to_le_bytes());
    }
    for p in props {
        for v in &p.points {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_image_files_and_refuses_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("House.JPG");
        std::fs::write(&photo, [0xff, 0xd8, 0xff]).unwrap();
        assert_eq!(read_image_file(&photo).unwrap(), vec![0xff, 0xd8, 0xff]);

        let show = dir.path().join("house.pixelflow.json");
        std::fs::write(&show, "{}").unwrap();
        let error = read_image_file(&show).unwrap_err();
        assert!(error.contains("isn't a photo PixelFlow can show"), "{error}");

        let missing = read_image_file(&dir.path().join("gone.png")).unwrap_err();
        assert_eq!(
            missing,
            "This photo was moved or deleted. Choose it again with Replace…"
        );
    }

    #[test]
    fn a_file_named_like_a_photo_must_really_be_one() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.png");
        std::fs::write(&text, "not a picture").unwrap();
        let error = read_image_file(&text).unwrap_err();
        assert!(error.contains("isn't a photo"), "{error}");

        for (name, bytes) in [
            ("a.png", &b"\x89PNG\r\n\x1a\n"[..]),
            ("a.gif", b"GIF89a.."),
            ("a.webp", b"RIFF\0\0\0\0WEBPVP8 "),
            ("a.bmp", b"BM\0\0"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(read_image_file(&path).unwrap(), bytes, "{name}");
        }
    }

    #[test]
    fn a_folder_named_like_a_photo_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("holiday.png");
        std::fs::create_dir(&folder).unwrap();
        assert!(read_image_file(&folder).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_named_like_a_photo_is_refused_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let pipe = dir.path().join("pipe.png");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .expect("mkfifo runs");
        assert!(made.success());
        // Nothing ever writes to the pipe: reading it must not wait for that.
        let error = read_image_file(&pipe).unwrap_err();
        assert!(error.contains("isn't a photo"), "{error}");
    }

    #[test]
    fn photos_the_user_picked_are_remembered() {
        let photos = PickedPhotos::default();
        assert!(!photos.contains(Path::new("/photos/house.jpg")));
        photos.add(PathBuf::from("/photos/house.jpg"));
        assert!(photos.contains(Path::new("/photos/house.jpg")));
    }

    #[test]
    fn preview_positions_are_packed_for_the_window() {
        let id: pf_model::PropId = serde_json::from_str("\"11111111-0000-4000-8000-000000000001\"").unwrap();
        let props = [PreviewProp {
            prop: id,
            frame_offset: 6,
            channels_per_pixel: 3,
            points: vec![1.5, -2.0, 3.0, 4.25],
        }];
        let bytes = encode_preview(7, &props);
        assert_eq!(bytes.len(), 16 + 48 + 4 * 4);
        assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &1u32.to_le_bytes());
        assert_eq!(&bytes[8..16], &7f64.to_le_bytes());
        assert_eq!(&bytes[16..52], b"11111111-0000-4000-8000-000000000001");
        assert_eq!(&bytes[52..56], &6u32.to_le_bytes());
        assert_eq!(&bytes[56..60], &3u32.to_le_bytes());
        assert_eq!(&bytes[60..64], &2u32.to_le_bytes(), "two pixels");
        assert_eq!(&bytes[64..68], &1.5f32.to_le_bytes());
        assert_eq!(&bytes[76..80], &4.25f32.to_le_bytes());
        assert_eq!(encode_preview(0, &[]).len(), 16);
    }
}
