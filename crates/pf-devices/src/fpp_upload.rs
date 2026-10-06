//! Putting a sequence and its music on an FPP, and on one of its playlists.
//!
//! Uploads use FPP's own file-manager upload (FPP 9.x, `www/api/controllers/files.php`), in two
//! steps so nothing on the FPP changes until every file has arrived whole:
//!
//! 1. [`stage`]: `PATCH /api/file/uploads` carries the file in chunks (`Upload-Name`,
//!    `Upload-Offset`, `Upload-Length` headers). FPP puts it together in its upload folder once
//!    every byte is in. `GET /api/files/uploads` then confirms the put-together file has every
//!    byte (FPP 9.x can answer "OK" for a file its full disk cut short).
//! 2. [`commit`]: `GET /api/file/move/<name>` moves it into the sequences or music folder by its
//!    extension, replacing a file of the same name.
//!
//! A cancelled or failed stage removes what it sent (`DELETE /api/file/uploads/<piece>`: FPP 9.x
//! appends chunks to `<name>.patch.0`, FPP 10 keeps `<name>.patch.<offset>` for each), so the
//! FPP is left as it was; anything that couldn't be removed is reported.
//!
//! What's on the FPP is read from `/api/sequence`, `/api/media`, `/api/playlists`, and the free
//! space from `/api/system/info`. Playlists are changed with `POST /api/playlist/<name>` (only for
//! a name the FPP has no playlist by) and `POST /api/playlist/<name>/mainPlaylist/item` (one more
//! entry; FPP keeps the rest of the playlist as it is).
//!
//! Everything here except reading changes the FPP: call it only when the user asks.

use crate::config::Destination;
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

/// Between an error and the FPP's own words for it, in [`UploadError`] messages. The window shows
/// what follows under "Details".
pub const DETAIL: &str = "\n\nFPP said: ";

/// Music file types FPP moves into its music folder (its `MoveFile()`).
const MUSIC_EXTENSIONS: [&str; 9] = ["mp3", "ogg", "m4a", "wav", "au", "m4p", "wma", "flac", "aac"];

fn detail_text(detail: &Option<String>) -> String {
    detail
        .as_deref()
        .map(|d| format!("{DETAIL}{d}"))
        .unwrap_or_default()
}

fn full_text(name: &str, sure: bool, detail: &Option<String>) -> String {
    let start = if sure {
        format!("The FPP's storage is full, so {name} couldn't be stored.")
    } else {
        format!("The FPP's storage may be full: {name} didn't save completely there.")
    };
    format!(
        "{start} Nothing on the FPP was replaced. Delete old sequences or music on the FPP (its File Manager), then try again.{}",
        detail_text(detail)
    )
}

/// Why sending a file to an FPP didn't work, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    #[error("The upload was cancelled. Nothing on the FPP was changed.")]
    Cancelled,
    #[error(
        "Couldn't reach the FPP at {address}: {reason}. Check that it's on and on the same network as this computer."
    )]
    Unreachable { address: String, reason: String },
    #[error(
        "The FPP at {address} stopped answering during the upload. Check its network connection, then try again."
    )]
    TimedOut { address: String },
    /// Out of space: `sure` when the FPP said so; otherwise the file came out short.
    #[error("{}", full_text(.name, *.sure, .detail))]
    Full {
        name: String,
        sure: bool,
        detail: Option<String>,
    },
    #[error(
        "There isn't room on the FPP: this needs {needed} and it has {free} free. Delete old sequences or music on the FPP (its File Manager), then try again."
    )]
    NoRoom { needed: String, free: String },
    #[error("The FPP couldn't store {name}.{}", detail_text(.detail))]
    Rejected { name: String, detail: Option<String> },
    #[error("Couldn't read {name}: {reason}")]
    Local { name: String, reason: String },
    /// `error`, after which these files from the upload couldn't be removed from the FPP's upload
    /// folder (by their names there).
    #[error(
        "{error} These may be left in the FPP's File Manager, under Uploads: {}. You can delete them there.",
        .names.join(", ")
    )]
    LeftBehind {
        error: Box<UploadError>,
        names: Vec<String>,
    },
}

/// Whether FPP's words say its storage is full (PHP's `file_put_contents` warning, FPP 10's
/// "disk full?", or the system's "No space left on device").
fn says_full(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    ["disk space", "no space left", "disk full"]
        .iter()
        .any(|s| text.contains(s))
}

/// JSON from an FPP answer, read past anything PHP printed ahead of it (warnings).
pub(crate) fn lenient_json(text: &str) -> Value {
    serde_json::from_str(text)
        .ok()
        .or_else(|| {
            let start = text.find('{')?;
            serde_json::from_str(&text[start..]).ok()
        })
        .or_else(|| {
            let start = text.rfind("{\"status\"")?;
            serde_json::from_str(&text[start..]).ok()
        })
        .unwrap_or(Value::Null)
}

/// Text with HTML tags and extra space taken out, cut to a readable length.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().count() > 300 {
        format!("{}…", out.chars().take(300).collect::<String>())
    } else {
        out
    }
}

/// What FPP printed ahead of its JSON (a PHP warning), if anything.
fn preamble(text: &str) -> Option<String> {
    let before = &text[..text.find('{').unwrap_or(text.len())];
    Some(plain(before)).filter(|s| !s.is_empty())
}

/// FPP's own explanation in an answer: its `error`, `Message`, or a `status` that isn't OK, else
/// any text it sent.
fn fpp_message(text: &str) -> Option<String> {
    let doc = lenient_json(text);
    for key in ["error", "Message"] {
        let value = str_field(&doc, key).trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    let status = str_field(&doc, "status").trim();
    if !status.is_empty() && !status.eq_ignore_ascii_case("ok") {
        return Some(status.trim_start_matches("ERROR: ").to_string());
    }
    if doc.is_null() {
        return Some(plain(text)).filter(|s| !s.is_empty());
    }
    preamble(text)
}

fn rejected_or_full(name: &str, message: Option<String>) -> UploadError {
    match message {
        Some(m) if says_full(&m) => UploadError::Full {
            name: name.to_string(),
            sure: true,
            detail: Some(m),
        },
        detail => UploadError::Rejected {
            name: name.to_string(),
            detail,
        },
    }
}

impl UploadError {
    fn from_device(error: DeviceError, name: &str) -> Self {
        match error {
            DeviceError::Unreachable { address, reason } if reason.contains("in time") => {
                UploadError::TimedOut { address }
            }
            DeviceError::Unreachable { address, reason } => UploadError::Unreachable { address, reason },
            DeviceError::Http { status, body, .. } => rejected_or_full(
                name,
                fpp_message(&body).or_else(|| Some(format!("HTTP {status}"))),
            ),
            other => UploadError::Rejected {
                name: name.to_string(),
                detail: Some(other.to_string()),
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

/// Characters FPP keeps in file names (its `sanitizeFilename()` drops the rest), less `[` and
/// `]`, which FPP 9.x's unescaped `glob()` of old upload pieces would misread.
fn fpp_keeps(c: char) -> bool {
    c.is_ascii_alphanumeric() || " -_~,;().".contains(c)
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

/// Whether a name the FPP already uses can be sent and moved as it is: one plain file name
/// (never a path), in characters that survive FPP's headers and its double URL decoding, that
/// aren't glob patterns (FPP 9.x's unescaped `glob()` of old pieces would match other files),
/// and that Windows allows in the temporary export's name. Anything else: Keep both sends a tidy
/// name instead.
pub fn is_safe_fpp_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_graphic() && !"/\\%+?#\"*[]<>:|".contains(c) || c == ' ')
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

/// What's on an FPP: its sequences and media (file names, as the FPP spells them), playlists,
/// and free space.
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

/// Whether a file name clashes with one on the FPP, and the name that would keep both.
///
/// A clash is found whatever the capitals (the safe direction: it can only warn too often), but
/// the file it clashes with is named as the FPP spells it: replacing it, or using it, means that
/// exact file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameCheck {
    /// The name PixelFlow would give the file.
    pub name: String,
    pub exists: bool,
    /// The clashing file's name on the FPP, exactly as the FPP spells it.
    pub fpp_name: Option<String>,
    pub keep_both_name: String,
}

impl FppFiles {
    fn check(list: &[String], name: &str) -> NameCheck {
        let taken = |n: &str| list.iter().any(|f| f.eq_ignore_ascii_case(n));
        let fpp_name = list
            .iter()
            .find(|f| *f == name)
            .or_else(|| list.iter().find(|f| f.eq_ignore_ascii_case(name)))
            .cloned();
        NameCheck {
            name: name.to_string(),
            exists: fpp_name.is_some(),
            fpp_name,
            keep_both_name: keep_both_name(name, taken),
        }
    }

    /// Whether the sequence file `name` (with `.fseq`) clashes with one on the FPP.
    pub fn check_sequence(&self, name: &str) -> NameCheck {
        Self::check(&self.sequences, name)
    }

    /// Whether the music file `name` clashes with one on the FPP.
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

/// A file sent whole to the FPP's upload folder, not yet moved into place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    /// Its name on the FPP.
    pub name: String,
    pub size: u64,
}

/// The names in FPP's upload folder (`GET /api/files/uploads`).
fn upload_folder(http: &dyn Http, host: &str) -> Result<Vec<String>, DeviceError> {
    let doc = lenient_json(&http.get(host, "/api/files/uploads")?);
    Ok(doc["files"]
        .as_array()
        .map(|files| files.iter().map(|f| str_field(f, "name").to_string()).collect())
        .unwrap_or_default())
}

/// Whether `file` in the upload folder belongs to the upload of `name`: one of its pieces
/// (`<name>.patch.<offset>`; FPP 9.x appends to `.patch.0`, FPP 10 keeps one per chunk) or, once
/// every byte was sent (`whole`), the file FPP put together.
fn is_upload_of(file: &str, name: &str, whole: bool) -> bool {
    let piece = file
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(".patch."))
        .is_some_and(|offset| !offset.is_empty() && offset.bytes().all(|b| b.is_ascii_digit()));
    piece || (whole && file == name)
}

/// Removes what the upload of `name` left in FPP's upload folder. The folder is listed once and
/// only this upload's files there are deleted (FPP 9.3 answers "Invalid path…" for a file that
/// isn't there, so nothing is asked for blindly); other files are left alone. Stops at the first
/// request that can't be made (an FPP that can't be reached would make each one wait). Returns
/// the files still left, by their names in the folder.
fn tidy(http: &dyn Http, host: &str, name: &str, whole: bool) -> Vec<String> {
    let Ok(listed) = upload_folder(http, host) else {
        return vec![name.to_string()];
    };
    let ours: Vec<String> = listed
        .into_iter()
        .filter(|f| is_upload_of(f, name, whole))
        .collect();
    let mut left = Vec::new();
    for (i, file) in ours.iter().enumerate() {
        match http.delete(host, &format!("/api/file/uploads/{}", encode_segment(file))) {
            Ok(reply) if str_field(&lenient_json(&reply), "status").eq_ignore_ascii_case("ok") => {}
            Ok(_) => left.push(file.clone()),
            Err(_) => {
                left.extend(ours[i..].iter().cloned());
                break;
            }
        }
    }
    left
}

/// `error`, after trying to remove what the upload of `name` left behind.
fn tidied(http: &dyn Http, host: &str, name: &str, whole: bool, error: UploadError) -> UploadError {
    let names = tidy(http, host, name, whole);
    if names.is_empty() {
        error
    } else {
        UploadError::LeftBehind {
            error: Box::new(error),
            names,
        }
    }
}

/// The size of `name` in FPP's upload folder, from `GET /api/files/uploads` (`files[].name`,
/// `sizeBytes`: a number on a 32-bit FPP, a string on a 64-bit one).
fn uploaded_size(http: &dyn Http, host: &str, name: &str) -> Result<Option<u64>, DeviceError> {
    let doc = lenient_json(&http.get(host, "/api/files/uploads")?);
    Ok(doc["files"].as_array().and_then(|files| {
        files
            .iter()
            .find(|f| str_field(f, "name") == name)
            .and_then(|f| u64::try_from(int_field(f, "sizeBytes")).ok())
    }))
}

/// Sends the file at `local` to the FPP's upload folder as `name`, without replacing anything:
/// [`commit`] moves it into place. `progress` hears (bytes sent, bytes in all) as it goes and
/// returns `false` to cancel. The file is read a piece at a time, never whole. Once every byte is
/// in, the FPP's copy is checked to be the right size.
///
/// On cancel or any failure, what was sent is removed again, so the FPP is as it was
/// ([`UploadError::LeftBehind`] says when that didn't work). Changes the FPP: only when the user
/// asks.
pub fn stage(
    http: &dyn Http,
    host: &str,
    local: &Path,
    name: &str,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Staged, UploadError> {
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
        // FPP may have put the file together if this was the last chunk.
        let last = offset + length == total;
        let reply = match reply {
            Ok(reply) => reply,
            Err(_) if cancelled => return Err(tidied(http, host, name, false, UploadError::Cancelled)),
            // Nothing reached an FPP that couldn't be connected to.
            Err(e @ DeviceError::Unreachable { .. }) if offset == 0 && !e.to_string().contains("in time") => {
                return Err(UploadError::from_device(e, name));
            }
            Err(e) => {
                let error = UploadError::from_device(e, name);
                return Err(tidied(http, host, name, last, error));
            }
        };
        let doc = lenient_json(&reply);
        let status = str_field(&doc, "status");
        if !status.eq_ignore_ascii_case("ok") {
            let error = rejected_or_full(name, fpp_message(&reply));
            return Err(tidied(http, host, name, last, error));
        }
        // FPP answers with how much of the file it now holds: exactly what was sent so far, or
        // less when its storage ran out part way (more means it counted a stale piece too).
        let held = u64::try_from(int_field(&doc, "size")).unwrap_or(0);
        if held < offset + length {
            let error = UploadError::Full {
                name: name.to_string(),
                sure: true,
                detail: preamble(&reply),
            };
            return Err(tidied(http, host, name, false, error));
        }
        if held > offset + length {
            let error = UploadError::Rejected {
                name: name.to_string(),
                detail: Some(format!(
                    "The FPP has more of {name} than was sent ({held} bytes, not {}), so PixelFlow won't use it.",
                    offset + length
                )),
            };
            return Err(tidied(http, host, name, last, error));
        }
        offset += length;
    }
    if !(progress)(total, total) {
        return Err(tidied(http, host, name, true, UploadError::Cancelled));
    }
    // FPP 9.x answers "OK" with the full size even when its disk filled while it put the file
    // together: the upload folder's listing has the real size.
    match uploaded_size(http, host, name) {
        Ok(Some(size)) if size == total => Ok(Staged {
            name: name.to_string(),
            size: total,
        }),
        Ok(_) => {
            let error = UploadError::Full {
                name: name.to_string(),
                sure: false,
                detail: None,
            };
            Err(tidied(http, host, name, true, error))
        }
        Err(e) => {
            let error = UploadError::from_device(e, name);
            Err(tidied(http, host, name, true, error))
        }
    }
}

/// Moves a staged file into the FPP's sequences or music folder, replacing a file of that name.
/// Changes the FPP: only when the user asks.
pub fn commit(http: &dyn Http, host: &str, staged: &Staged) -> Result<(), UploadError> {
    let name = &staged.name;
    let moved = get_json(http, host, &format!("/api/file/move/{}", encode_segment(name)))
        .map_err(|e| UploadError::from_device(e, name))?;
    if str_field(&moved, "status") != "OK" {
        let error = UploadError::Rejected {
            name: name.clone(),
            detail: fpp_message(&moved.to_string()),
        };
        return Err(tidied(http, host, name, true, error));
    }
    Ok(())
}

/// Removes a staged file that won't be moved into place (the send was cancelled). Returns the
/// files that couldn't be removed from the upload folder, by name (empty when it's clean).
pub fn discard(http: &dyn Http, host: &str, staged: &Staged) -> Vec<String> {
    tidy(http, host, &staged.name, true)
}

/// [`stage`] then [`commit`]: sends one file and moves it into place.
pub fn upload(
    http: &dyn Http,
    host: &str,
    local: &Path,
    name: &str,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), UploadError> {
    let staged = stage(http, host, local, name, progress)?;
    commit(http, host, &staged)
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

fn playlist_error(text: String) -> DeviceError {
    DeviceError::Message(text)
}

/// Puts `entry` on the chosen playlist and returns its name (`None` when no playlist was chosen).
///
/// - **Existing:** appends the one entry (unless the same sequence is on it already); FPP keeps
///   the rest of the playlist as it is. A playlist FPP can't read is never touched (FPP would
///   rebuild it with only the new entry).
/// - **New:** made only when the FPP has no playlist by that name, whatever its capitals, and
///   answers its "no such playlist" (`""`). Anything else is refused: FPP would replace it whole.
///
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
        return Err(playlist_error(format!("\"{name}\" isn't a playlist name.")));
    }
    let names = string_list(&get_json(http, host, "/api/playlists")?);
    let path = format!("/api/playlist/{}", encode_segment(name));
    if create {
        if let Some(taken) = names.iter().find(|n| n.eq_ignore_ascii_case(name)) {
            return Err(playlist_error(format!(
                "The FPP already has a playlist called \"{taken}\". Choose it under \"Add it to\", or pick another name."
            )));
        }
        let missing = match http.get(host, &path) {
            Ok(body) => lenient_json(&body) == json!(""),
            Err(DeviceError::Http { status: 404, .. }) => true,
            Err(e) => return Err(e),
        };
        if !missing {
            return Err(playlist_error(format!(
                "The FPP already has a playlist called \"{name}\". Choose it under \"Add it to\", or pick another name."
            )));
        }
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
        let reply = lenient_json(&http.post_json(host, &path, &playlist.to_string())?);
        if str_field(&reply, "Status") == "Error" {
            return Err(playlist_error(format!(
                "Couldn't make the FPP playlist \"{name}\": {}",
                str_field(&reply, "Message")
            )));
        }
        return Ok(Some(name.to_string()));
    }
    if !names.iter().any(|n| n == name) {
        return Err(playlist_error(format!(
            "The FPP has no playlist called \"{name}\"."
        )));
    }
    let playlist = lenient_json(&http.get(host, &path)?);
    let readable = playlist.is_object()
        && playlist
            .get("mainPlaylist")
            .is_none_or(|items| items.is_array() || items.is_null());
    if !readable {
        return Err(playlist_error(format!(
            "PixelFlow can't read the FPP playlist \"{name}\", so it left it alone. Check it on FPP's Playlists page."
        )));
    }
    let sequence = &entry["sequenceName"];
    let already = playlist["mainPlaylist"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| &item["sequenceName"] == sequence));
    if !already {
        let reply =
            lenient_json(&http.post_json(host, &format!("{path}/mainPlaylist/item"), &entry.to_string())?);
        if !str_field(&reply, "Status").eq_ignore_ascii_case("ok") {
            return Err(playlist_error(format!(
                "Couldn't add it to the FPP playlist \"{name}\": {}",
                str_field(&reply, "Message")
            )));
        }
    }
    Ok(Some(name.to_string()))
}

/// Where a sequence puts one controller's channels (1-based), for [`layout_warnings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutBlock {
    pub name: String,
    pub address: String,
    pub start: u32,
    pub count: u32,
}

fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Plain warnings where a sequence of `channels` channels, laid out as `blocks`, doesn't match
/// what the FPP sends to its controllers (`destinations`, from its channel outputs). Nothing to
/// compare with gives no warnings.
pub fn layout_warnings(channels: u32, blocks: &[LayoutBlock], destinations: &[Destination]) -> Vec<String> {
    let end = |start: u32, count: u32| start.saturating_add(count).saturating_sub(1);
    let Some(top) = destinations
        .iter()
        .filter(|d| d.channels > 0)
        .map(|d| end(d.start_channel, d.channels))
        .max()
    else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if channels < top {
        warnings.push(format!(
            "This sequence has {} channels but the FPP sends {}. Lights past channel {} will stay dark.",
            thousands(channels),
            thousands(top),
            thousands(channels)
        ));
    } else if channels > top {
        warnings.push(format!(
            "This sequence has {} channels but the FPP only sends {}. Channels past {} won't reach any lights.",
            thousands(channels),
            thousands(top),
            thousands(top)
        ));
    }
    for block in blocks {
        let sent: Vec<&Destination> = destinations
            .iter()
            .filter(|d| d.address == block.address)
            .collect();
        match sent.first() {
            None => warnings.push(format!(
                "{} ({}): the FPP doesn't send to it, so it won't light up.",
                block.name, block.address
            )),
            Some(first)
                if !sent
                    .iter()
                    .any(|d| d.start_channel == block.start && d.channels == block.count) =>
            {
                warnings.push(format!(
                    "{}: this sequence puts it at channels {}–{}, but the FPP sends it channels {}–{}.",
                    block.name,
                    thousands(block.start),
                    thousands(end(block.start, block.count)),
                    thousands(first.start_channel),
                    thousands(end(first.start_channel, first.channels))
                ));
            }
            Some(_) => {}
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_text(1), "1 KB");
        assert_eq!(size_text(3 * 1024 * 1024 + 1), "3.0 MB");
        assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(thousands(6147), "6,147");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1,000,000");
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
            body: String::new(),
        };
        assert_eq!(
            UploadError::from_device(http, "Show.fseq").to_string(),
            format!("The FPP couldn't store Show.fseq.{DETAIL}HTTP 507")
        );
        let full = DeviceError::Http {
            address: "a".into(),
            path: "/p".into(),
            status: 500,
            body: "write failed: No space left on device".into(),
        };
        assert!(matches!(
            UploadError::from_device(full, "x"),
            UploadError::Full { sure: true, .. }
        ));
    }

    #[test]
    fn json_is_found_behind_php_warnings() {
        assert_eq!(lenient_json(r#"{"a":1}"#), json!({"a": 1}));
        assert_eq!(
            lenient_json("<b>Warning</b>: x<br />\n{\"status\":\"OK\"}"),
            json!({"status": "OK"})
        );
        assert_eq!(lenient_json("\"\""), json!(""));
        assert_eq!(lenient_json("no json"), Value::Null);
        assert_eq!(
            preamble("<br />\n<b>Warning</b>:  disk  space<br />\n{}").as_deref(),
            Some("Warning: disk space")
        );
    }

    #[test]
    fn names_the_fpp_already_uses_are_safe_only_as_plain_file_names() {
        for ok in ["Show.fseq", "Rock'n Roll.mp3", "Song (Remix), v2.mp3"] {
            assert!(is_safe_fpp_name(ok), "{ok}");
        }
        for bad in [
            "../x",
            "a/b",
            "a\\b",
            "100%.mp3",
            "a+b.mp3",
            ".hidden",
            "Café.mp3",
            "",
            // Glob patterns (FPP 9.x's unescaped glob of old pieces) and characters Windows
            // can't put in the temporary export's name: Keep both sends a tidy name instead.
            "Song*.mp3",
            "Song [Remix].mp3",
            "a?.mp3",
            "a<b.mp3",
            "a>b.mp3",
            "a:b.mp3",
            "a|b.mp3",
        ] {
            assert!(!is_safe_fpp_name(bad), "{bad}");
        }
    }

    #[test]
    fn playlist_choices_read_from_json() {
        let none: PlaylistChoice = serde_json::from_str(r#"{"kind":"none"}"#).unwrap();
        assert_eq!(none, PlaylistChoice::None);
        let new: PlaylistChoice = serde_json::from_str(r#"{"kind":"new","name":"Show"}"#).unwrap();
        assert_eq!(new, PlaylistChoice::New("Show".into()));
    }
}
