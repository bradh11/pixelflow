//! Layout editor support: the background photo.

use crate::Reply;
use std::path::{Path, PathBuf};
use tauri::ipc::Response;

/// Image files the layout can show behind the props.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
/// Larger photos are refused rather than loaded into the window.
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// The bytes of a background photo, sent raw so the window can show it without any file access
/// of its own. Only image files are read.
#[tauri::command]
pub(crate) async fn read_image(path: PathBuf) -> Reply<Response> {
    tauri::async_runtime::spawn_blocking(move || read_image_file(&path))
        .await
        .map_err(|_| "Something went wrong reading the photo.".to_string())?
        .map(Response::new)
}

fn read_image_file(path: &Path) -> Result<Vec<u8>, String> {
    let name = path.display();
    let is_image = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !is_image {
        return Err(format!(
            "{name} isn't a photo PixelFlow can show. Choose a PNG, JPEG, WebP, GIF, or BMP image."
        ));
    }
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Could not read {name}: {e}"))?
        .len();
    if size > MAX_IMAGE_BYTES {
        return Err(format!(
            "{name} is too large to show ({} MB). Choose a photo under {} MB.",
            size / (1024 * 1024),
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(|e| format!("Could not read {name}: {e}"))
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
        assert!(missing.starts_with("Could not read"), "{missing}");
    }
}
