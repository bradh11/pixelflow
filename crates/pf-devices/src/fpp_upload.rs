//! Putting a sequence and its music on an FPP, and on one of its playlists.
//!
//! Uploads use FPP's own file-manager upload (FPP 9.x, `www/api/controllers/files.php`):
//! `PATCH /api/file/uploads` carries the file in chunks (`Upload-Name`, `Upload-Offset`,
//! `Upload-Length` headers), FPP puts it together in its upload folder once every byte is in,
//! and `GET /api/file/move/<name>` then moves it into the sequences or music folder by its
//! extension (replacing a file of the same name). A cancelled upload never touches the copy
//! already on the FPP, and its partial file is removed with
//! `DELETE /api/file/uploads/<name>.patch.0`.
//!
//! What's on the FPP is read from `/api/sequence`, `/api/media`, `/api/playlists`, and the free
//! space from `/api/system/info`. Playlists are changed with `POST /api/playlist/<name>` (a new
//! one) and `POST /api/playlist/<name>/mainPlaylist/item` (one more entry).
//!
//! Everything here except reading changes the FPP: call it only when the user asks.

use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, str_field};
use crate::fpp_player::encode_segment;
use crate::http::Http;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// How much of a file goes in one request. Small enough that a slow link still finishes each
/// request well inside its time limit, and that progress and Cancel respond quickly.
pub const CHUNK_BYTES: u64 = 4 * 1024 * 1024;

/// Music file types FPP moves into its music folder (its `MoveFile()`).
const MUSIC_EXTENSIONS: [&str; 9] = ["mp3", "ogg", "m4a", "wav", "au", "m4p", "wma", "flac", "aac"];

/// Why sending a file to an FPP didn't work, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    #[error("The upload was cancelled.")]
    Cancelled,
    #[error(
        "Couldn't reach the FPP at {address}: {reason}. Check that it's on and on the same network as this computer."
    )]
    Unreachable { address: String, reason: String },
    #[error(
        "The FPP at {address} stopped answering during the upload. Check its network connection, then try again."
    )]
    TimedOut { address: String },
    #[error(
        "The FPP's storage is full, so {name} couldn't be stored. Delete old sequences or music on the FPP (its File Manager), then try again."
    )]
    Full { name: String },
    #[error(
        "There isn't room on the FPP: this needs {needed} and it has {free} free. Delete old sequences or music on the FPP (its File Manager), then try again."
    )]
    NoRoom { needed: String, free: String },
    #[error("The FPP couldn't store {name} ({reason}).")]
    Rejected { name: String, reason: String },
    #[error("Couldn't read {name}: {reason}")]
    Local { name: String, reason: String },
}

impl UploadError {
    fn from_device(error: DeviceError, name: &str) -> Self {
        match error {
            DeviceError::Unreachable { address, reason } if reason.contains("in time") => {
                UploadError::TimedOut { address }
            }
            DeviceError::Unreachable { address, reason } => UploadError::Unreachable { address, reason },
            DeviceError::Http { status, .. } => UploadError::Rejected {
                name: name.to_string(),
                reason: format!("it answered HTTP {status}"),
            },
            other => UploadError::Rejected {
                name: name.to_string(),
                reason: other.to_string(),
            },
        }
    }
}

/// A byte count for people: "512 KB", "3.4 MB", "1.2 GB".
pub fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 1024.0 * MB {
        format!("{:.1} GB", b / (1024.0 * MB))
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// Fails, before anything is sent, when `needed` bytes won't fit in `free` (unknown free space
/// doesn't stop anything).
pub fn ensure_room(free: Option<u64>, needed: u64) -> Result<(), UploadError> {
    match free {
        Some(free) if needed > free => Err(UploadError::NoRoom {
            needed: size_text(needed),
            free: size_text(free),
        }),
        _ => Ok(()),
    }
}

/// Characters FPP keeps in file names (its `sanitizeFilename()` drops the rest).
fn fpp_keeps(c: char) -> bool {
    c.is_ascii_alphanumeric() || " -_~,;[]().".contains(c)
}

/// `name` as an FPP file name ending in `.extension`: only characters FPP keeps (so it stores
/// the file under exactly this name), no `..`, never empty.
pub fn fpp_file_name(name: &str, extension: &str) -> String {
    let extension = extension.to_ascii_lowercase();
    let stem = name.trim();
    let stem = match stem.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case(&extension) => stem,
        _ => stem,
    };
    let mut kept = String::new();
    for c in stem.chars() {
        let c = if c == '/' || c == '\\' { ' ' } else { c };
        if !fpp_keeps(c) || (c == ' ' && kept.ends_with(' ')) || (c == '.' && kept.ends_with('.')) {
            continue;
        }
        kept.push(c);
    }
    let kept = kept.trim_matches(|c| c == ' ' || c == '.');
    let stem = if kept.is_empty() { "Sequence" } else { kept };
    format!("{stem}.{extension}")
}

/// `name` as an FPP playlist name: letters, numbers, spaces, hyphens, and underscores only (the
/// FPP playlist editor's rule).
pub fn playlist_name(name: &str) -> String {
    let mut kept = String::new();
    for c in name.chars() {
        if (c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
            && !(c == ' ' && kept.ends_with(' '))
        {
            kept.push(c);
        }
    }
    let kept = kept.trim();
    if kept.is_empty() {
        "PixelFlow".to_string()
    } else {
        kept.to_string()
    }
}

/// The name to keep both files by: `Show (2).fseq`, or the next number not `taken`.
pub fn keep_both_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){extension}"))
        .find(|candidate| !taken(candidate))
        .expect("some number is free")
}

/// Whether FPP treats `name` as music it can play with a sequence.
pub fn is_music(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| MUSIC_EXTENSIONS.iter().any(|m| ext.eq_ignore_ascii_case(m)))
}

/// What's on an FPP: its sequences and media (file names), playlists, and free space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FppFiles {
    /// Sequence file names, with `.fseq`.
    pub sequences: Vec<String>,
    /// Music and video file names.
    pub media: Vec<String>,
    pub playlists: Vec<String>,
    /// Free bytes on the FPP's media storage, when it says.
    pub free_bytes: Option<u64>,
}

/// Whether a file name is taken on the FPP, and the name that would keep both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameCheck {
    pub name: String,
    pub exists: bool,
    pub keep_both_name: String,
}

impl FppFiles {
    fn check(list: &[String], name: &str) -> NameCheck {
        let taken = |n: &str| list.iter().any(|f| f.eq_ignore_ascii_case(n));
        NameCheck {
            name: name.to_string(),
            exists: taken(name),
            keep_both_name: keep_both_name(name, taken),
        }
    }

    /// Whether the sequence file `name` (with `.fseq`) is on the FPP already.
    pub fn check_sequence(&self, name: &str) -> NameCheck {
        Self::check(&self.sequences, name)
    }

    /// Whether the music file `name` is on the FPP already.
    pub fn check_media(&self, name: &str) -> NameCheck {
        Self::check(&self.media, name)
    }
}

fn string_list(doc: &Value) -> Vec<String> {
    doc.as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The sequence files on the FPP (with `.fseq`), in one request (changes nothing).
pub fn sequence_names(http: &dyn Http, host: &str) -> Result<Vec<String>, DeviceError> {
    let names = get_json(http, host, "/api/sequence")?;
    Ok(string_list(&names)
        .into_iter()
        .map(|n| format!("{n}.fseq"))
        .collect())
}

/// Reads what's on the FPP (changes nothing).
pub fn read_files(http: &dyn Http, host: &str) -> Result<FppFiles, DeviceError> {
    let sequences = sequence_names(http, host)?;
    let media = string_list(&get_json(http, host, "/api/media")?);
    let playlists = string_list(&get_json(http, host, "/api/playlists")?);
    let free_bytes = get_json(http, host, "/api/system/info").ok().and_then(|info| {
        info.pointer("/Utilization/Disk/Media/Free")
            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
    });
    Ok(FppFiles {
        sequences,
        media,
        playlists,
        free_bytes,
    })
}

/// Reads a file chunk, reporting progress and stopping when told to.
struct Counting<'a, R> {
    inner: R,
    done: u64,
    total: u64,
    progress: &'a mut dyn FnMut(u64, u64) -> bool,
    cancelled: bool,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancelled || !(self.progress)(self.done, self.total) {
            self.cancelled = true;
            return Err(io::Error::other("cancelled"));
        }
        let read = self.inner.read(buf)?;
        self.done += read as u64;
        Ok(read)
    }
}

/// Sends the file at `local` to the FPP as `name` (a `.fseq` goes to its sequences, music to its
/// music), replacing a file of that name. `progress` hears (bytes sent, bytes in all) as it goes
/// and returns `false` to cancel. The file is read a piece at a time, never whole.
/// Changes the FPP: only when the user asks.
pub fn upload(
    http: &dyn Http,
    host: &str,
    local: &Path,
    name: &str,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), UploadError> {
    let local_error = |e: io::Error| UploadError::Local {
        name: local.file_name().map_or_else(
            || local.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        reason: e.to_string(),
    };
    let mut file = File::open(local).map_err(local_error)?;
    let total = file.metadata().map_err(local_error)?.len();
    if total == 0 {
        return Err(local_error(io::Error::other("the file is empty")));
    }
    let tidy = || {
        // The partial file in FPP's upload folder: FPP appends chunks sent in order to `.patch.0`.
        let _ = http.delete(
            host,
            &format!("/api/file/uploads/{}.patch.0", encode_segment(name)),
        );
    };
    let total_text = total.to_string();
    let mut offset = 0;
    while offset < total {
        let length = CHUNK_BYTES.min(total - offset);
        let offset_text = offset.to_string();
        let headers = [
            ("Upload-Name", name),
            ("Upload-Offset", offset_text.as_str()),
            ("Upload-Length", total_text.as_str()),
            ("Content-Type", "application/offset+octet-stream"),
        ];
        let mut body = Counting {
            inner: (&mut file).take(length),
            done: offset,
            total,
            progress: &mut *progress,
            cancelled: false,
        };
        let reply = http.send_body("PATCH", host, "/api/file/uploads", &headers, &mut body, length);
        let cancelled = body.cancelled;
        let reply = match reply {
            Ok(reply) => reply,
            Err(_) if cancelled => {
                tidy();
                return Err(UploadError::Cancelled);
            }
            Err(e) => return Err(UploadError::from_device(e, name)),
        };
        let reply: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
        let status = str_field(&reply, "status");
        if !status.eq_ignore_ascii_case("ok") {
            tidy();
            return Err(UploadError::Rejected {
                name: name.to_string(),
                reason: if status.is_empty() {
                    "it gave no answer PixelFlow understands".to_string()
                } else {
                    status.to_string()
                },
            });
        }
        // FPP answers with how much of the file it now holds: less than was sent means its
        // storage ran out part way.
        let stored = u64::try_from(int_field(&reply, "size")).unwrap_or(0);
        if stored < offset + length {
            tidy();
            return Err(UploadError::Full {
                name: name.to_string(),
            });
        }
        offset += length;
    }
    (progress)(total, total);
    let moved = get_json(http, host, &format!("/api/file/move/{}", encode_segment(name)))
        .map_err(|e| UploadError::from_device(e, name))?;
    let status = str_field(&moved, "status");
    if status != "OK" {
        return Err(UploadError::Rejected {
            name: name.to_string(),
            reason: status.trim_start_matches("ERROR: ").to_string(),
        });
    }
    Ok(())
}

/// Which FPP playlist a sent sequence goes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "camelCase")]
pub enum PlaylistChoice {
    None,
    Existing(String),
    New(String),
}

/// A playlist entry that plays `sequence` (an FPP file name, with `.fseq`) with `media`, as
/// FPP's playlist editor writes it.
pub fn playlist_entry(sequence: &str, media: Option<&str>, duration_secs: f64) -> Value {
    let mut entry = json!({
        "type": "sequence",
        "enabled": 1,
        "playOnce": 0,
        "sequenceName": sequence,
        "duration": duration_secs,
    });
    if let Some(media) = media {
        entry["type"] = json!("both");
        entry["mediaName"] = json!(media);
        entry["videoOut"] = json!("--Default--");
    }
    entry
}

fn playlist_error(name: &str, why: impl std::fmt::Display) -> DeviceError {
    DeviceError::Message(format!("Couldn't add it to the FPP playlist \"{name}\": {why}"))
}

/// Puts `entry` on the chosen playlist: appended to an existing one (unless the same sequence is
/// on it already), or on a new one (an existing playlist of that name is added to instead).
/// Returns the playlist's name, or `None` when no playlist was chosen.
/// Changes the FPP: only when the user asks.
pub fn put_on_playlist(
    http: &dyn Http,
    host: &str,
    choice: &PlaylistChoice,
    entry: &Value,
) -> Result<Option<String>, DeviceError> {
    let (name, create) = match choice {
        PlaylistChoice::None => return Ok(None),
        PlaylistChoice::Existing(name) => (name.as_str(), false),
        PlaylistChoice::New(name) => (name.as_str(), true),
    };
    // FPP stores a playlist as `<name>.json` in its playlist folder: a name is never a path.
    if name.trim().is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(playlist_error(name, "that isn't a playlist name"));
    }
    let path = format!("/api/playlist/{}", encode_segment(name));
    let existing = match get_json(http, host, &path) {
        Ok(doc) if doc.is_object() && doc.get("mainPlaylist").is_some() => Some(doc),
        Ok(_) | Err(DeviceError::Http { status: 404, .. }) => None,
        Err(e) => return Err(e),
    };
    match existing {
        Some(playlist) => {
            let sequence = &entry["sequenceName"];
            let already = playlist["mainPlaylist"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| &item["sequenceName"] == sequence));
            if !already {
                let reply = http.post_json(host, &format!("{path}/mainPlaylist/item"), &entry.to_string())?;
                let reply: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
                if !str_field(&reply, "Status").eq_ignore_ascii_case("ok") {
                    return Err(playlist_error(name, str_field(&reply, "Message")));
                }
            }
        }
        None if create => {
            let duration = entry["duration"].as_f64().unwrap_or(0.0);
            let playlist = json!({
                "name": name,
                "version": 4,
                "repeat": 0,
                "loopCount": 0,
                "desc": "Sent from PixelFlow",
                "random": 0,
                "empty": false,
                "leadIn": [],
                "mainPlaylist": [entry],
                "leadOut": [],
                "playlistInfo": {
                    "total_duration": duration,
                    "total_items": 1,
                    "leadIn_duration": 0.0,
                    "leadIn_items": 0,
                    "mainPlaylist_duration": duration,
                    "mainPlaylist_items": 1,
                    "leadOut_duration": 0.0,
                    "leadOut_items": 0,
                },
            });
            let reply = http.post_json(host, &path, &playlist.to_string())?;
            let reply: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
            if str_field(&reply, "Status") == "Error" {
                return Err(playlist_error(name, str_field(&reply, "Message")));
            }
        }
        None => return Err(playlist_error(name, "the FPP has no playlist by that name")),
    }
    Ok(Some(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_text(1), "1 KB");
        assert_eq!(size_text(3 * 1024 * 1024 + 1), "3.0 MB");
        assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn device_errors_become_upload_errors() {
        let timeout = DeviceError::Unreachable {
            address: "a".into(),
            reason: "it didn't answer in time".into(),
        };
        assert!(matches!(
            UploadError::from_device(timeout, "x"),
            UploadError::TimedOut { .. }
        ));
        let http = DeviceError::Http {
            address: "a".into(),
            path: "/p".into(),
            status: 507,
        };
        assert_eq!(
            UploadError::from_device(http, "Show.fseq").to_string(),
            "The FPP couldn't store Show.fseq (it answered HTTP 507)."
        );
    }

    #[test]
    fn playlist_choices_read_from_json() {
        let none: PlaylistChoice = serde_json::from_str(r#"{"kind":"none"}"#).unwrap();
        assert_eq!(none, PlaylistChoice::None);
        let new: PlaylistChoice = serde_json::from_str(r#"{"kind":"new","name":"Show"}"#).unwrap();
        assert_eq!(new, PlaylistChoice::New("Show".into()));
    }
}
