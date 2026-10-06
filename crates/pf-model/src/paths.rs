//! File paths in show and sequence files: written as text without losing anything, and kept
//! relative to the file when they're inside its folder, so a show folder can move.
//!
//! **Text form.** A path that is valid UTF-8 (nearly every path) is written as it is. A Linux
//! path can hold bytes that aren't UTF-8; each such byte is written as a NUL character followed
//! by two hex digits (`"Caf\u0000e9.mp3"`). No file name can hold a NUL, so the marker never
//! clashes with a real path, and the path reads back byte for byte.
//!
//! **Relative paths.** A file inside the folder of the file that refers to it (or in a folder
//! below it) is written relative to that folder with `/` between folders (`"Music/Song.mp3"`),
//! so it still works when the whole folder moves or is synced to another computer. Anything
//! else stays a full path.

use std::path::{Component, Path, PathBuf};

/// Marks a byte that isn't UTF-8 in a path's text (see the module notes).
const MARK: char = '\0';

/// A path as text for a show or sequence file, without losing any characters (see the module
/// notes). Read it back with [`path_from_text`].
pub fn path_to_text(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::fmt::Write;
        use std::os::unix::ffi::OsStrExt;
        let bytes = path.as_os_str().as_bytes();
        let mut text = String::with_capacity(bytes.len());
        for chunk in bytes.utf8_chunks() {
            text.push_str(chunk.valid());
            for byte in chunk.invalid() {
                text.push(MARK);
                let _ = write!(text, "{byte:02x}");
            }
        }
        text
    }
    #[cfg(not(unix))]
    {
        // Windows paths come from the system as UTF-16 that is almost always valid; the rare
        // one that isn't keeps its readable part.
        path.to_string_lossy().into_owned()
    }
}

/// The path written as `text` by [`path_to_text`] (any plain path text works too).
pub fn path_from_text(text: &str) -> PathBuf {
    if !text.contains(MARK) {
        return PathBuf::from(text);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = Vec::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(MARK) {
            bytes.extend_from_slice(&rest.as_bytes()[..at]);
            let after = &rest[at + MARK.len_utf8()..];
            match marked_byte(after) {
                Some(byte) => {
                    bytes.push(byte);
                    rest = &after[2..];
                }
                // A mark without its two digits can't be part of a path: it's left out.
                None => rest = after,
            }
        }
        bytes.extend_from_slice(rest.as_bytes());
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(display_text(text))
    }
}

/// The byte two hex digits at the start of `text` stand for.
fn marked_byte(text: &str) -> Option<u8> {
    let digits = text.as_bytes().get(..2)?;
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u8::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
}

/// A path's text for people to read: bytes that aren't UTF-8 show as `�`.
pub fn display_text(text: &str) -> String {
    if !text.contains(MARK) {
        return text.to_string();
    }
    let mut shown = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(MARK) {
        shown.push_str(&rest[..at]);
        let after = &rest[at + MARK.len_utf8()..];
        if marked_byte(after).is_some() {
            shown.push('\u{FFFD}');
            rest = &after[2..];
        } else {
            rest = after;
        }
    }
    shown.push_str(rest);
    shown
}

/// The file name at the end of a path's text, for messages ("Christmas Medley 2017.mp3"). Both
/// `/` and `\` count as folder separators, so a path written on Windows reads well anywhere.
pub fn file_name_of(text: &str) -> String {
    let trimmed = text.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    display_text(if name.is_empty() { text } else { name })
}

/// Whether `text` is a full path here or on another kind of computer: `/…` (or `\…`), a
/// Windows drive (`C:\…`, `C:/…`), or a network share (`\\server\…`). Only other paths are
/// taken to be relative to a folder.
pub fn is_full_path_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    text.starts_with(['/', '\\']) || drive || Path::new(text).is_absolute()
}

/// `text` as a show or sequence file in `folder` stores it: relative to `folder` (with `/`
/// between folders) when it's a full path to a file inside `folder` or a folder below it, as it
/// is otherwise.
pub fn relative_text(text: &str, folder: &Path) -> String {
    if text.is_empty() || !is_full_path_text(text) || folder.as_os_str().is_empty() {
        return text.to_string();
    }
    let path = path_from_text(text);
    let Ok(inside) = path.strip_prefix(folder) else {
        return text.to_string();
    };
    let mut parts = Vec::new();
    for component in inside.components() {
        match component {
            Component::Normal(part) => parts.push(path_to_text(Path::new(part))),
            // "..", "." and the like: keep the full path rather than a confusing relative one.
            _ => return text.to_string(),
        }
    }
    if parts.is_empty() {
        return text.to_string();
    }
    parts.join("/")
}

/// `text`, read from a show or sequence file in `folder`, as a full path: a relative path is
/// taken to start in `folder`; a full path (or nothing) is left as it is.
pub fn resolve_text(text: &str, folder: &Path) -> String {
    if text.is_empty() || is_full_path_text(text) || folder.as_os_str().is_empty() {
        return text.to_string();
    }
    let mut path = folder.to_path_buf();
    // Written with "/" between folders on every computer.
    for part in text.split('/').filter(|p| !p.is_empty() && *p != ".") {
        path.push(path_from_text(part));
    }
    path_to_text(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_paths_are_written_as_they_are() {
        let path = Path::new("/Shows/Haas 2024/Música/Christmas Medley 2017.mp3");
        let text = path_to_text(path);
        assert_eq!(text, "/Shows/Haas 2024/Música/Christmas Medley 2017.mp3");
        assert_eq!(path_from_text(&text), path);
        assert_eq!(display_text(&text), text);
    }

    #[cfg(unix)]
    #[test]
    fn paths_that_are_not_utf8_keep_every_byte() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        // "Café" written in Latin-1 (a lone 0xE9), as older Linux systems name files.
        let name = std::ffi::OsString::from_vec(b"/music/Caf\xe9 \xff%.mp3".to_vec());
        let path = PathBuf::from(name);
        let text = path_to_text(&path);
        assert_eq!(text, "/music/Caf\u{0}e9 \u{0}ff%.mp3");
        let back = path_from_text(&text);
        assert_eq!(back.as_os_str().as_bytes(), b"/music/Caf\xe9 \xff%.mp3");
        assert_eq!(display_text(&text), "/music/Caf\u{FFFD} \u{FFFD}%.mp3");
        assert_eq!(file_name_of(&text), "Caf\u{FFFD} \u{FFFD}%.mp3");
        // Through JSON and back, as a show file stores it.
        let json = serde_json::to_string(&text).unwrap();
        let read: String = serde_json::from_str(&json).unwrap();
        assert_eq!(path_from_text(&read), path);
        // Relative to its folder and back, too.
        let relative = relative_text(&text, Path::new("/music"));
        assert_eq!(relative, "Caf\u{0}e9 \u{0}ff%.mp3");
        assert_eq!(
            path_from_text(&resolve_text(&relative, Path::new("/music"))),
            path
        );
    }

    #[test]
    fn a_stray_mark_is_left_out() {
        assert_eq!(display_text("a\u{0}zz.mp3"), "azz.mp3");
        assert_eq!(display_text("a\u{0}+f.mp3"), "a+f.mp3");
        assert_eq!(path_from_text("end\u{0}"), PathBuf::from("end"));
    }

    #[test]
    fn file_names_read_plainly_whoever_wrote_the_path() {
        assert_eq!(
            file_name_of("/Shows/Christmas Medley 2017.mp3"),
            "Christmas Medley 2017.mp3"
        );
        assert_eq!(file_name_of("C:\\xLights\\Music\\Wizards.mp3"), "Wizards.mp3");
        assert_eq!(file_name_of("Music/Song.mp3"), "Song.mp3");
        assert_eq!(file_name_of("Song.mp3"), "Song.mp3");
        assert_eq!(file_name_of("/photos/"), "photos");
    }

    #[test]
    fn full_paths_from_any_computer_are_recognized() {
        for full in [
            "/Users/me/a.mp3",
            "\\\\nas\\show\\a.mp3",
            "C:\\Music\\a.mp3",
            "d:/a.mp3",
        ] {
            assert!(is_full_path_text(full), "{full}");
        }
        for relative in ["a.mp3", "Music/a.mp3", "Music\\a.mp3", "C:a.mp3", ""] {
            assert!(!is_full_path_text(relative), "{relative}");
        }
    }

    #[test]
    fn files_inside_the_folder_become_relative() {
        let folder = Path::new("/Shows/Haas 2024");
        assert_eq!(relative_text("/Shows/Haas 2024/Song.fseq", folder), "Song.fseq");
        assert_eq!(
            relative_text("/Shows/Haas 2024/MP3 Music/Christmas Medley 2017.mp3", folder),
            "MP3 Music/Christmas Medley 2017.mp3"
        );
        // Outside the folder (even one named like it), or the folder itself: kept in full.
        for kept in [
            "/Shows/Haas 2024 old/Song.mp3",
            "/Shows/Other/Song.mp3",
            "/Shows/Haas 2024",
            "/Shows/Haas 2024/../Other/Song.mp3",
            "C:\\Music\\Song.mp3",
            "already/relative.mp3",
            "",
        ] {
            assert_eq!(relative_text(kept, folder), kept, "{kept}");
        }
        assert_eq!(relative_text("/a/b.mp3", Path::new("")), "/a/b.mp3");
    }

    #[test]
    fn relative_paths_resolve_against_the_folder() {
        let folder = Path::new("/Users/me/Shows/Haas 2024");
        assert_eq!(
            resolve_text("MP3 Music/Christmas Medley 2017.mp3", folder),
            path_to_text(&folder.join("MP3 Music").join("Christmas Medley 2017.mp3"))
        );
        assert_eq!(
            resolve_text("./Song.fseq", folder),
            path_to_text(&folder.join("Song.fseq"))
        );
        for kept in ["/elsewhere/Song.mp3", "C:\\Music\\Song.mp3", ""] {
            assert_eq!(resolve_text(kept, folder), kept, "{kept}");
        }
        assert_eq!(resolve_text("Song.mp3", Path::new("")), "Song.mp3");
    }

    #[test]
    fn relative_and_back_is_the_same_file() {
        let folder = Path::new("/Shows/Haas 2024");
        for full in ["/Shows/Haas 2024/a/b/c.mp3", "/Shows/Other/c.mp3"] {
            assert_eq!(resolve_text(&relative_text(full, folder), folder), full);
        }
    }
}
