//! Network and device paths that a file names, and which of them may be opened.
//!
//! On Windows, looking at a path on a network share (`\\server\share\…`) makes the system
//! connect to that server and try to sign in, which hands the server a hash of the user's
//! password. A show, a sequence, or an xLights file from someone else can name any server, and
//! PixelFlow looks for a file's music and pictures as soon as the file is opened. Device paths
//! (`\\?\…`, `\\.\…`) reach pipes, raw disks, and the same servers by another spelling.
//!
//! So a path of either kind that was read out of a file is never looked at, on any system,
//! unless the user chose from its folder (or a folder above it) this session: picked something
//! there in a file dialog, or opened a show or sequence of their own from there
//! ([`trust_network_folder`]). Everything that turns a path from a file into a look at the disk
//! asks [`network_path_refusal`] first.
//!
//! The rule works on the path's text, so a file behaves the same on every computer: a show
//! written on Windows says the same thing about its network music when it's opened on a Mac.

use crate::paths::{display_text, file_name_of};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

/// The folders the user chose from this session, as [`folded`] text.
static TRUSTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn is_separator(c: char) -> bool {
    c == '/' || c == '\\'
}

/// What a path's text names, when it isn't a file on one of this computer's own drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `\\server\share\…`, or the same with `/`.
    Share,
    /// `\\?\…`, `\\.\…`, or `\??\…`.
    Device,
}

fn kind_of(text: &str) -> Option<Kind> {
    // Some readers trim a path before using it, so the spaces don't hide what follows.
    let text = text.trim_start();
    if text.starts_with("\\??\\") {
        return Some(Kind::Device);
    }
    let mut chars = text.chars();
    if !(chars.next().is_some_and(is_separator) && chars.next().is_some_and(is_separator)) {
        return None;
    }
    let device = matches!(chars.next(), Some('?' | '.')) && chars.next().is_none_or(is_separator);
    Some(if device { Kind::Device } else { Kind::Share })
}

/// Whether `text` names a network share (`\\server\share\…`, also written `//server/share/…`)
/// or a Windows device path (`\\?\…`, `\\.\…`, `\??\…`). Judged from the text alone, the same
/// on every system. A drive letter mapped to a share (`Z:\…`) is an ordinary path.
pub fn is_network_path_text(text: &str) -> bool {
    kind_of(text).is_some()
}

/// A path's text as it's compared: one spelling for each folder, whichever slashes and letter
/// case it was written with. `None` for a path with a `..` in it, which is never taken to be
/// inside anything.
fn folded(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for part in text.trim_start().split(is_separator).filter(|p| !p.is_empty()) {
        match part {
            "." => continue,
            ".." => return None,
            part => {
                out.push('\\');
                out.extend(part.chars().flat_map(char::to_lowercase));
            }
        }
    }
    Some(out)
}

/// Lets files in `folder`, and in the folders below it, be opened when a file names them, for
/// the rest of the session. Call it for the folder of anything the user chooses in a file
/// dialog; a folder on one of the computer's own drives needs no trust and isn't recorded.
pub fn trust_network_folder(folder: &Path) {
    trust(&crate::paths::path_to_text(folder));
}

/// [`trust_network_folder`] for the folder a chosen file is in.
pub fn trust_network_folder_of(file: &Path) {
    // Split on the text, so a Windows path is understood on every system.
    let text = crate::paths::path_to_text(file);
    let file = text.trim_end_matches(is_separator);
    if let Some(at) = file.rfind(is_separator) {
        trust(&file[..at]);
    }
}

fn trust(folder: &str) {
    if !is_network_path_text(folder) {
        return;
    }
    if let Some(folder) = folded(folder) {
        TRUSTED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(folder);
    }
}

fn trusted(text: &str) -> bool {
    let Some(path) = folded(text) else {
        return false;
    };
    TRUSTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|folder| {
            path.strip_prefix(folder.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
        })
}

/// The server and share a network path starts with (`\\nas\music`), or the start of a device
/// path, for messages.
fn place_of(text: &str) -> String {
    let text = display_text(text.trim_start());
    let mut parts = text.split(is_separator).filter(|p| !p.is_empty());
    let mut place = String::from("\\\\");
    place.push_str(parts.next().unwrap_or_default());
    if let Some(share) = parts.next() {
        place.push('\\');
        place.push_str(share);
    }
    place
}

/// Why the file at path text `text` must not be looked at: it's a network or device path that
/// came from a file, and the user hasn't chosen anything in its folder this session. `None` for
/// every other path. The message names the file ("Song.mp3 is on a network share (\\nas\music).
/// …").
pub fn network_path_refusal(text: &str) -> Option<String> {
    let kind = kind_of(text)?;
    if trusted(text) {
        return None;
    }
    let (name, place) = (file_name_of(text.trim_start()), place_of(text));
    Some(match kind {
        Kind::Share => format!(
            "{name} is on a network share ({place}). PixelFlow opens files from a network folder only after \
             you choose a file in that folder yourself."
        ),
        Kind::Device => format!(
            "{name} is named by a device path ({place}). PixelFlow opens a file like that only after you \
             choose it yourself."
        ),
    })
}

/// [`network_path_refusal`] for a path rather than its text.
pub fn network_path_refusal_of(path: &Path) -> Option<String> {
    network_path_refusal(&crate::paths::path_to_text(path))
}

/// Whether [`network_path_refusal`] refuses `text`.
pub fn is_refused_network_path(text: &str) -> bool {
    kind_of(text).is_some() && !trusted(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_and_device_paths_are_told_from_ordinary_paths_by_their_text() {
        for network in [
            "\\\\nas\\show\\a.mp3",
            "//nas/show/a.mp3",
            "\\/nas/show/a.mp3",
            "/\\nas\\show\\a.mp3",
            "\\\\10.0.0.9\\c$\\a.fseq",
            "\\\\nas",
            "\\\\",
            "  \\\\nas\\show\\a.mp3",
            "\\\\?\\C:\\Shows\\a.mp3",
            "\\\\?\\UNC\\nas\\show\\a.mp3",
            "\\\\.\\pipe\\a",
            "\\\\.\\PhysicalDrive0",
            "//?/C:/Shows/a.mp3",
            "//./COM1",
            "\\??\\UNC\\nas\\show\\a.mp3",
            "\\\\?",
        ] {
            assert!(is_network_path_text(network), "{network}");
            assert!(is_refused_network_path(network), "{network}");
        }
        for ordinary in [
            "",
            "a.mp3",
            "Music/a.mp3",
            "Music\\a.mp3",
            "/Users/me/a.mp3",
            "\\Shows\\a.mp3",
            "C:\\Music\\a.mp3",
            "Z:\\Music\\a.mp3",
            "d:/a.mp3",
            "Music//a.mp3",
            "a\\\\b.mp3",
            "?\\a.mp3",
        ] {
            assert!(!is_network_path_text(ordinary), "{ordinary}");
            assert_eq!(network_path_refusal(ordinary), None, "{ordinary}");
        }
    }

    #[test]
    fn the_refusal_names_the_file_and_where_it_is() {
        assert_eq!(
            network_path_refusal("\\\\attacker\\drop\\Lights\\Wizards.mp3").unwrap(),
            "Wizards.mp3 is on a network share (\\\\attacker\\drop). PixelFlow opens files from a network \
             folder only after you choose a file in that folder yourself."
        );
        assert_eq!(
            network_path_refusal("//attacker/drop/Wizards.mp3").unwrap(),
            "Wizards.mp3 is on a network share (\\\\attacker\\drop). PixelFlow opens files from a network \
             folder only after you choose a file in that folder yourself."
        );
        assert_eq!(
            network_path_refusal("\\\\.\\pipe\\Wizards.mp3").unwrap(),
            "Wizards.mp3 is named by a device path (\\\\.\\pipe). PixelFlow opens a file like that only \
             after you choose it yourself."
        );
    }

    #[test]
    fn a_folder_the_user_chose_from_is_trusted_with_the_folders_below_it() {
        trust_network_folder_of(Path::new("\\\\chosen-nas\\Shows\\2024\\Carol.mp3"));
        for inside in [
            "\\\\chosen-nas\\Shows\\2024\\Carol.mp3",
            "\\\\chosen-nas\\Shows\\2024\\Other.mp3",
            "\\\\CHOSEN-NAS\\shows\\2024\\Music\\Deep\\Other.mp3",
            "//chosen-nas/Shows/2024/Other.mp3",
            "\\\\chosen-nas\\Shows\\\\2024\\.\\Other.mp3",
        ] {
            assert_eq!(network_path_refusal(inside), None, "{inside}");
        }
        for outside in [
            "\\\\chosen-nas\\Shows\\Other.mp3",
            "\\\\chosen-nas\\Shows\\2024 old\\Other.mp3",
            "\\\\chosen-nas\\Private\\Other.mp3",
            "\\\\other-nas\\Shows\\2024\\Carol.mp3",
            "\\\\chosen-nas\\Shows\\2024\\..\\..\\Private\\Other.mp3",
            "\\\\?\\UNC\\chosen-nas\\Shows\\2024\\Carol.mp3",
        ] {
            assert!(is_refused_network_path(outside), "{outside}");
        }
    }

    #[test]
    fn folders_on_the_computers_own_drives_are_not_recorded() {
        trust_network_folder(Path::new("/Users/me/Shows"));
        trust_network_folder(Path::new("C:\\Shows"));
        let recorded = TRUSTED.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert!(!recorded.contains("\\users\\me\\shows"), "{recorded:?}");
        assert!(!recorded.contains("\\c:\\shows"), "{recorded:?}");
    }

    #[test]
    fn a_path_and_its_text_are_refused_alike() {
        let text = "\\\\by-path\\share\\a.mp3";
        assert_eq!(
            network_path_refusal_of(&crate::paths::path_from_text(text)),
            network_path_refusal(text)
        );
        assert_eq!(network_path_refusal_of(Path::new("/Users/me/a.mp3")), None);
    }
}
