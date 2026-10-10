//! The picture files Picture effects draw (see `pf_render::Pictures`): where an effect's `file`
//! setting points, which files may be read, and which aren't there.
//!
//! - **Where they live:** a picture chosen for an effect is copied into an `images` folder next
//!   to the show file, and the effect stores `images/<name>`, so the show folder can move as a
//!   whole. While the show isn't saved there's no such folder: the effect stores the full path
//!   of the file as it was chosen.
//! - **What may be read:** only files inside the show's `images` folder, and files the user
//!   chose this session ([`PictureFiles::allow`]). A path an edit put in an effect is not enough
//!   on its own, and only real picture files of a sensible size are read.
//! - **When:** never inside the engine. The renderer's library reads a picture on a thread of
//!   its own the first time it's drawn; exports read it where they run. A [`PictureCheck`] is
//!   copied out of the engine to see which pictures are missing.

use crate::files::{FileRole, MissingFile};
use pf_model::{file_name_of, is_full_path_text, path_from_text, path_to_text, relative_text, resolve_text};
use pf_render::Pictures;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

/// The folder next to the show file that pictures are copied into.
pub const IMAGES_FOLDER: &str = "images";
/// The kinds of picture file read, by extension.
pub const PICTURE_EXTENSIONS: [&str; 6] = ["gif", "png", "jpg", "jpeg", "webp", "bmp"];
/// Larger files are refused rather than read.
pub const MAX_PICTURE_BYTES: u64 = 64 * 1024 * 1024;
/// The most names listed from the images folder.
const MAX_LISTED: usize = 500;

/// A file as it was when last looked at: its size and when it changed.
type Stamp = (u64, Option<SystemTime>);

/// The open show's picture files: where they are and which may be read.
#[derive(Debug, Default)]
pub struct PictureFiles {
    /// The show's folder, once it has been saved.
    folder: Mutex<Option<PathBuf>>,
    /// Files outside the images folder that may be read: ones the user chose this session.
    chosen: Mutex<HashSet<PathBuf>>,
    /// Each picture as it was at the last check, by what its effect stores (none: not there).
    seen: Mutex<HashMap<String, Option<Stamp>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// True when `bytes` start like a GIF, PNG, JPEG, WebP, or BMP picture.
fn looks_like_picture(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

fn has_picture_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PICTURE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Reads a picture file: only a real file with a picture's extension and contents, and at most
/// [`MAX_PICTURE_BYTES`] of it. The error is why not, in a few words ("it isn't there").
pub fn read_picture_file(path: &Path) -> Result<Vec<u8>, String> {
    const NOT_A_PICTURE: &str = "it isn't a GIF, PNG, JPEG, WebP, or BMP picture";
    if !has_picture_extension(path) {
        return Err(NOT_A_PICTURE.into());
    }
    let unreadable = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound => "it isn't there".to_string(),
        std::io::ErrorKind::PermissionDenied => "PixelFlow isn't allowed to read it".to_string(),
        kind => format!("it couldn't be opened ({kind})"),
    };
    // A pipe or device named like a picture is never opened (opening one can wait for ever).
    if !fs::metadata(path).map_err(unreadable)?.is_file() {
        return Err(NOT_A_PICTURE.into());
    }
    let file = fs::File::open(path).map_err(unreadable)?;
    let too_large = || format!("it's larger than {} MB", MAX_PICTURE_BYTES / (1024 * 1024));
    let metadata = file.metadata().map_err(unreadable)?;
    if !metadata.is_file() {
        return Err(NOT_A_PICTURE.into());
    }
    if metadata.len() > MAX_PICTURE_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PICTURE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() as u64 > MAX_PICTURE_BYTES {
        return Err(too_large());
    }
    if !looks_like_picture(&bytes) {
        return Err(NOT_A_PICTURE.into());
    }
    Ok(bytes)
}

impl PictureFiles {
    /// The show's folder from now on (`None`: the show isn't saved). True when it changed.
    pub(crate) fn set_folder(&self, folder: Option<&Path>) -> bool {
        let mut current = lock(&self.folder);
        if current.as_deref() == folder {
            return false;
        }
        *current = folder.map(Path::to_path_buf);
        lock(&self.seen).clear();
        true
    }

    /// The folder pictures are copied into: `images` next to the show file, once it's saved.
    pub fn images_folder(&self) -> Option<PathBuf> {
        lock(&self.folder).as_ref().map(|f| f.join(IMAGES_FOLDER))
    }

    /// Lets `path` be read: a file the user chose in a dialog.
    pub fn allow(&self, path: &Path) {
        lock(&self.chosen).insert(path.to_path_buf());
    }

    /// The file an effect's `file` setting names: a full path as it is, anything else in the
    /// show's folder. `None` when nothing is named, or the show isn't saved (so has no folder).
    pub fn resolve(&self, file: &str) -> Option<PathBuf> {
        let file = file.trim();
        if file.is_empty() {
            return None;
        }
        if is_full_path_text(file) {
            return Some(path_from_text(file));
        }
        let folder = lock(&self.folder).clone()?;
        Some(path_from_text(&resolve_text(file, &folder)))
    }

    /// Whether `path` may be read: it's in the show's images folder (or a folder below it), or
    /// the user chose it.
    pub fn allowed(&self, path: &Path) -> bool {
        if lock(&self.chosen).contains(path) {
            return true;
        }
        let Some(images) = self.images_folder() else {
            return false;
        };
        path.strip_prefix(&images).is_ok_and(|inside| {
            let mut parts = inside.components().peekable();
            parts.peek().is_some() && parts.all(|c| matches!(c, Component::Normal(_)))
        })
    }

    /// What an effect stores for the picture at `path`: `images/<name>` when it's in the show's
    /// images folder, its full path otherwise.
    pub fn stored(&self, path: &Path) -> String {
        let text = path_to_text(path);
        let folder = lock(&self.folder).clone();
        match folder {
            Some(folder) if path.starts_with(folder.join(IMAGES_FOLDER)) => relative_text(&text, &folder),
            _ => text,
        }
    }

    /// The bytes of the picture an effect's `file` setting names, when it may be read (reads the
    /// disk). The error is why not, in a few words.
    pub fn read(&self, file: &str) -> Result<Vec<u8>, String> {
        if file.trim().is_empty() {
            return Err("no picture is chosen".into());
        }
        let Some(path) = self.resolve(file) else {
            return Err("the show isn't saved yet, so it has no images folder to look in".into());
        };
        if !self.allowed(&path) {
            return Err(
                "it isn't in the show's images folder; choose it again in the effect's settings".into(),
            );
        }
        read_picture_file(&path)
    }

    /// Takes the picture at `path` (a file the user chose) for the show (reads and writes the
    /// disk): copied into the show's images folder under its own name, unless it's there
    /// already. The same picture already there is used as it is; a different one of that name
    /// is kept, and this one saved as "name (2).gif". While the show isn't saved there's
    /// nowhere to copy it: the file itself is used. Returns what an effect stores for it.
    pub fn adopt(&self, path: &Path) -> Result<String, String> {
        let name = file_name_of(&path_to_text(path));
        let bytes = read_picture_file(path).map_err(|why| format!("{name} can't be used: {why}."))?;
        let Some(images) = self.images_folder() else {
            self.allow(path);
            return Ok(path_to_text(path));
        };
        if self.allowed(path) && path.starts_with(&images) {
            return Ok(self.stored(path));
        }
        let own = match path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            Some(own) if !own.trim().is_empty() && !own.starts_with('.') => own,
            _ => {
                return Err(format!(
                    "{name} can't be used: its name can't be used as a file name."
                ));
            }
        };
        let write_err = |e: std::io::Error| format!("Couldn't copy {name} into {} ({e}).", images.display());
        fs::create_dir_all(&images).map_err(write_err)?;
        let named = Path::new(&own);
        let (stem, ext) = (
            named
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            named
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default(),
        );
        let mut n = 1;
        let target = loop {
            let candidate = images.join(if n == 1 {
                own.clone()
            } else {
                format!("{stem} ({n}){ext}")
            });
            match fs::metadata(&candidate) {
                // The same picture, copied before.
                Ok(meta)
                    if meta.is_file()
                        && meta.len() == bytes.len() as u64
                        && fs::read(&candidate).is_ok_and(|there| there == bytes) =>
                {
                    return Ok(self.stored(&candidate));
                }
                Ok(_) => n += 1,
                Err(_) => break candidate,
            }
            if n > 1000 {
                return Err(format!(
                    "There are too many pictures named {name} in the show's images folder already."
                ));
            }
        };
        let temp = images.join(format!(".{own}.pixelflow-part"));
        let written = fs::File::create(&temp)
            .and_then(|mut f| f.write_all(&bytes).and_then(|_| f.sync_all()))
            .and_then(|_| fs::rename(&temp, &target));
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(write_err(e));
        }
        Ok(self.stored(&target))
    }

    /// The pictures in the show's images folder, as effects store them (`images/<name>`), by
    /// name (reads the disk). None while the show isn't saved.
    pub fn listed(&self) -> Vec<String> {
        let Some(images) = self.images_folder() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&images) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| has_picture_extension(p) && p.is_file())
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|n| !n.starts_with('.'))
            .take(MAX_LISTED)
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
            .into_iter()
            .map(|n| format!("{IMAGES_FOLDER}/{n}"))
            .collect()
    }
}

/// Which of the open sequence's pictures aren't there (or can't be read): copied out of the
/// engine to [`run`](Self::run) without holding it, then handed back with
/// [`crate::Engine::publish_picture_status`].
#[derive(Debug, Clone)]
pub struct PictureCheck {
    pub(crate) files: Arc<PictureFiles>,
    pub(crate) pictures: Pictures,
    /// The sequence document it was made for.
    pub(crate) doc: u64,
    /// What each Picture effect stores, and what it belongs to ("Picture effect at 0:05.000"),
    /// one of each.
    pub(crate) wanted: Vec<(String, String)>,
}

/// What a [`PictureCheck`] found.
#[derive(Debug, Clone)]
pub struct PictureStatus {
    pub(crate) doc: u64,
    /// The pictures that can't be drawn, by what their effects store.
    pub(crate) missing: Vec<(String, MissingFile)>,
}

impl PictureStatus {
    /// The pictures that can't be drawn.
    pub fn missing(&self) -> Vec<MissingFile> {
        self.missing.iter().map(|(_, m)| m.clone()).collect()
    }
}

impl PictureCheck {
    /// How many pictures it will look at.
    pub fn len(&self) -> usize {
        self.wanted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wanted.is_empty()
    }

    /// Looks at each picture (reads the disk): the ones that aren't there, may not be read, or
    /// aren't pictures. A picture that changed, came back, or went away since the last look is
    /// read again the next time it's drawn.
    pub fn run(&self) -> PictureStatus {
        let mut missing = Vec::new();
        for (file, owner) in &self.wanted {
            let path = self.files.resolve(file);
            let there = path
                .as_deref()
                .and_then(|p| fs::metadata(p).ok())
                .filter(fs::Metadata::is_file)
                .map(|m| (m.len(), m.modified().ok()));
            let allowed = path.as_deref().is_some_and(|p| self.files.allowed(p));
            let stamp = there.filter(|_| allowed);
            let before = lock(&self.files.seen).insert(file.clone(), stamp);
            let changed = before.is_some_and(|before| before != stamp);
            let failed = self.pictures.problem(file);
            // Also one that couldn't be read before it was first looked at: it may be there now.
            if changed || (before.is_none() && stamp.is_some() && failed.is_some()) {
                self.pictures.forget(file);
            }
            let unreadable = failed.filter(|_| stamp.is_some() && !changed && before.is_some());
            if stamp.is_some() && unreadable.is_none() {
                continue;
            }
            let shown = path.as_deref().map_or_else(|| file.clone(), path_to_text);
            let mut entry = MissingFile::new(FileRole::Picture, &shown, owner.clone(), None);
            if let Some(why) = unreadable {
                entry.message = format!("{} couldn't be read: {why}.", entry.name);
            } else if there.is_some() {
                entry.message = format!(
                    "{} isn't in the show's images folder, so PixelFlow doesn't read it.",
                    entry.name
                );
            }
            missing.push((file.clone(), entry));
        }
        PictureStatus {
            doc: self.doc,
            missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest GIF: one clear pixel.
    pub(crate) const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

    fn files_in(folder: &Path) -> PictureFiles {
        let files = PictureFiles::default();
        files.set_folder(Some(folder));
        files
    }

    #[test]
    fn only_the_images_folder_and_chosen_files_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("show");
        fs::create_dir_all(show.join("images/sub")).unwrap();
        fs::write(show.join("images/santa.gif"), GIF).unwrap();
        fs::write(show.join("images/sub/elf.gif"), GIF).unwrap();
        fs::write(show.join("secret.gif"), GIF).unwrap();
        fs::write(dir.path().join("outside.gif"), GIF).unwrap();
        let files = files_in(&show);
        // In the images folder, by what an effect stores.
        assert_eq!(files.read("images/santa.gif").unwrap(), GIF);
        assert_eq!(files.read("images/sub/elf.gif").unwrap(), GIF);
        assert_eq!(
            files.read(&path_to_text(&show.join("images/santa.gif"))).unwrap(),
            GIF
        );
        // Anywhere else, by a relative or a full path, is refused, and so is climbing out.
        let outside = path_to_text(&dir.path().join("outside.gif"));
        for file in [
            "secret.gif",
            "images/../secret.gif",
            "../outside.gif",
            "images/../../outside.gif",
            outside.as_str(),
        ] {
            let why = files.read(file).unwrap_err();
            assert!(
                why.contains("images folder") || why.contains("isn't there"),
                "{file}: {why}"
            );
        }
        assert!(
            !files.allowed(&show.join("images")),
            "the folder itself isn't a picture"
        );
        assert!(!files.allowed(&show.join("images/../secret.gif")));
        // Until the user chooses it.
        files.allow(&dir.path().join("outside.gif"));
        assert_eq!(files.read(&outside).unwrap(), GIF);
        // What isn't there, or isn't chosen, says so plainly.
        assert_eq!(files.read("images/gone.gif").unwrap_err(), "it isn't there");
        assert_eq!(files.read("  ").unwrap_err(), "no picture is chosen");
        // Without a saved show there's no folder: only chosen files are read.
        let unsaved = PictureFiles::default();
        assert!(
            unsaved
                .read("images/santa.gif")
                .unwrap_err()
                .contains("isn't saved")
        );
        assert!(unsaved.read(&outside).unwrap_err().contains("images folder"));
        unsaved.allow(&dir.path().join("outside.gif"));
        assert_eq!(unsaved.read(&outside).unwrap(), GIF);
    }

    #[test]
    fn only_real_picture_files_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let images = dir.path().join("images");
        fs::create_dir_all(&images).unwrap();
        fs::write(images.join("notes.txt"), GIF).unwrap();
        fs::write(images.join("notes.gif"), b"Dear Santa").unwrap();
        fs::create_dir_all(images.join("folder.gif")).unwrap();
        fs::write(images.join("SANTA.GIF"), GIF).unwrap();
        let files = files_in(dir.path());
        for file in ["images/notes.txt", "images/notes.gif", "images/folder.gif"] {
            let why = files.read(file).unwrap_err();
            assert!(why.contains("isn't a GIF, PNG"), "{file}: {why}");
        }
        assert_eq!(files.read("images/SANTA.GIF").unwrap(), GIF);
        assert!(looks_like_picture(b"\x89PNG\r\n\x1a\n") && looks_like_picture(b"BM....."));
        assert!(
            looks_like_picture(b"RIFF\x00\x00\x00\x00WEBPVP8 ")
                && !looks_like_picture(b"RIFF\x00\x00\x00\x00WAVE")
        );
        assert!(looks_like_picture(&[0xff, 0xd8, 0xff, 0xe0]) && !looks_like_picture(b""));
    }

    #[test]
    fn a_chosen_picture_is_copied_into_the_shows_images_folder() {
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("show");
        fs::create_dir_all(&show).unwrap();
        let picked = dir.path().join("Downloads");
        fs::create_dir_all(&picked).unwrap();
        fs::write(picked.join("santa dancing.gif"), GIF).unwrap();
        let files = files_in(&show);
        // Copied under its own name (the folder is made), and stored relative to the show.
        let stored = files.adopt(&picked.join("santa dancing.gif")).unwrap();
        assert_eq!(stored, "images/santa dancing.gif");
        assert_eq!(fs::read(show.join("images/santa dancing.gif")).unwrap(), GIF);
        assert_eq!(files.read(&stored).unwrap(), GIF);
        assert!(picked.join("santa dancing.gif").is_file(), "the original stays");
        // The same picture again is the same file; one already in the folder is used where it is.
        assert_eq!(files.adopt(&picked.join("santa dancing.gif")).unwrap(), stored);
        assert_eq!(
            files.adopt(&show.join("images/santa dancing.gif")).unwrap(),
            stored
        );
        assert_eq!(fs::read_dir(show.join("images")).unwrap().count(), 1);
        // A different picture of the same name is kept beside it.
        let other: Vec<u8> = [GIF, b"more"].concat();
        fs::write(picked.join("santa dancing.gif"), &other).unwrap();
        let second = files.adopt(&picked.join("santa dancing.gif")).unwrap();
        assert_eq!(second, "images/santa dancing (2).gif");
        assert_eq!(
            fs::read(show.join("images/santa dancing (2).gif")).unwrap(),
            other
        );
        assert_eq!(fs::read(show.join("images/santa dancing.gif")).unwrap(), GIF);
        assert_eq!(files.adopt(&picked.join("santa dancing.gif")).unwrap(), second);
        // Nothing half-written is left behind, and the folder lists its pictures by name.
        fs::write(show.join("images/readme.txt"), "x").unwrap();
        fs::write(show.join("images/Angel.PNG"), b"\x89PNG....").unwrap();
        assert_eq!(
            files.listed(),
            [
                "images/Angel.PNG",
                "images/santa dancing (2).gif",
                "images/santa dancing.gif"
            ]
        );
        // What isn't a picture is refused, by name.
        fs::write(picked.join("list.gif"), "not a picture").unwrap();
        let err = files.adopt(&picked.join("list.gif")).unwrap_err();
        assert!(err.starts_with("list.gif can't be used: it isn't a GIF"), "{err}");
        let err = files.adopt(&picked.join("gone.gif")).unwrap_err();
        assert_eq!(err, "gone.gif can't be used: it isn't there.");
        // While the show isn't saved, the file is used where it is.
        let unsaved = PictureFiles::default();
        fs::write(picked.join("elf.gif"), GIF).unwrap();
        let stored = unsaved.adopt(&picked.join("elf.gif")).unwrap();
        assert_eq!(stored, path_to_text(&picked.join("elf.gif")));
        assert_eq!(unsaved.read(&stored).unwrap(), GIF);
        assert!(unsaved.listed().is_empty());
    }
}
