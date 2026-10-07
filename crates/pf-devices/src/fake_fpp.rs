//! A fake FPP that answers real HTTP on `127.0.0.1` (feature `test-fixtures`), for testing
//! uploads end to end without a device. It implements only the endpoints PixelFlow uses to send
//! a sequence, modelled on FPP 9.3's PHP (`www/api/controllers/files.php`, `playlist.php`):
//! the file-manager upload into the upload folder, the listing of that folder, the move into
//! place, deleting from it, playlists, status, outputs, and commands.
//!
//! It can be made slow, failing, or out of space, can behave like FPP 10 (each chunk its own
//! `.patch.<offset>` file), can print PHP warnings ahead of its JSON, and can put together a
//! short file while still answering "OK" (a disk filling during assembly on 9.x).
//!
//! Stored files keep only their size and a checksum, so a 100 MB upload costs no memory here.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// A position-aware checksum: pieces of a file checksummed at their own offsets add up to the
/// checksum of the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Checksum {
    sum: u64,
    weighted: u64,
}

impl Checksum {
    /// Adds `data`, which sits at `offset` in its file.
    pub fn add_at(&mut self, offset: u64, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            let position = offset.wrapping_add(i as u64).wrapping_add(1);
            self.sum = self.sum.wrapping_add(u64::from(byte));
            self.weighted = self.weighted.wrapping_add(u64::from(byte).wrapping_mul(position));
        }
    }

    pub fn combine(self, other: Checksum) -> Checksum {
        Checksum {
            sum: self.sum.wrapping_add(other.sum),
            weighted: self.weighted.wrapping_add(other.weighted),
        }
    }

    pub fn value(&self) -> u64 {
        self.weighted.rotate_left(17) ^ self.sum
    }
}

/// A file the fake FPP holds: its length and the [`Checksum`] of its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoredFile {
    pub size: u64,
    pub checksum: u64,
}

impl StoredFile {
    /// The file `data` would be once stored.
    pub fn of(data: &[u8]) -> Self {
        let mut sum = Checksum::default();
        sum.add_at(0, data);
        Self {
            size: data.len() as u64,
            checksum: sum.value(),
        }
    }
}

/// A file in FPP's upload folder: a received piece (`<name>.patch.<offset>`) or a put-together
/// upload (`<name>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UploadFile {
    /// Where the piece starts in its file (0 for a put-together upload).
    pub offset: u64,
    pub size: u64,
    pub sum: Checksum,
    /// The file's first bytes (up to 64 KiB), for checking headers.
    pub head: Vec<u8>,
}

/// How much of the start of each upload is kept (an `.fseq`'s header and variable headers).
const HEAD_BYTES: usize = 64 * 1024;

/// Everything the fake FPP knows; tests read and change it through [`FakeFpp::state`].
#[derive(Debug)]
pub struct FakeFppState {
    /// Sequences by file name (with `.fseq`).
    pub sequences: BTreeMap<String, StoredFile>,
    /// Music by file name.
    pub music: BTreeMap<String, StoredFile>,
    /// The upload folder, by file name.
    pub uploads: BTreeMap<String, UploadFile>,
    /// The first bytes of each file moved into place, by name.
    pub heads: BTreeMap<String, Vec<u8>>,
    /// Playlists by name, as FPP stores them.
    pub playlists: BTreeMap<String, Value>,
    /// Playlist files FPP can't parse (its GET answers `null`), by name.
    pub broken_playlists: Vec<String>,
    /// Bytes of free space on the media drive; uploads past it are cut short, as on a full disk.
    pub free_bytes: u64,
    /// Every request, as `METHOD /path` (uploads add ` <name>@<offset>+<length>`).
    pub requests: Vec<String>,
    /// Bodies of `/api/command` requests.
    pub commands: Vec<String>,
    /// The largest upload request body seen.
    pub largest_body: u64,
    /// A pause after each 64 KiB of an upload body is read (a slow network or SD card).
    pub read_delay: Duration,
    /// Answer every upload request with this HTTP status and body (after reading the body).
    pub fail_uploads: Option<(u16, String)>,
    /// FPP 10: every chunk is kept as its own `.patch.<offset>` file until the upload is whole.
    pub chunk_files: bool,
    /// FPP 9.x with the disk filling during assembly: the put-together file is this many bytes
    /// short, yet FPP still answers "OK" with the full size.
    pub short_assembly: Option<u64>,
    /// PHP warning text printed ahead of the JSON of every upload answer.
    pub php_warning: Option<String>,
    /// Deleting from the upload folder fails (permissions).
    pub refuse_deletes: bool,
    /// Moving this file into place fails, as when its folder is not writable.
    pub fail_move: Option<String>,
    /// Bytes added to every upload answer's `size`, as when a stale piece is counted too.
    pub extra_held: u64,
    /// What `/api/fppd/status` answers.
    pub status: Value,
    /// What `/api/channel/output/universeOutputs` answers.
    pub outputs: Value,
}

impl Default for FakeFppState {
    fn default() -> Self {
        Self {
            sequences: BTreeMap::new(),
            music: BTreeMap::new(),
            uploads: BTreeMap::new(),
            heads: BTreeMap::new(),
            playlists: BTreeMap::new(),
            broken_playlists: Vec::new(),
            free_bytes: 8 * 1024 * 1024 * 1024,
            requests: Vec::new(),
            commands: Vec::new(),
            largest_body: 0,
            read_delay: Duration::ZERO,
            fail_uploads: None,
            chunk_files: false,
            short_assembly: None,
            php_warning: None,
            refuse_deletes: false,
            fail_move: None,
            extra_held: 0,
            status: json!({
                "status_name": "idle", "current_playlist": {"playlist": ""}, "current_sequence": "",
                "seconds_elapsed": "0", "seconds_remaining": "0",
                "next_playlist": {"playlist": "No playlist scheduled.", "start_time": ""}
            }),
            outputs: serde_json::from_str(include_str!(
                "../fixtures/fpp/api_channel_output_universeOutputs.json"
            ))
            .expect("fixture parses"),
        }
    }
}

impl FakeFppState {
    /// Bytes of `name` still sitting in FPP's upload folder (pieces and put-together file).
    pub fn upload_bytes(&self, name: &str) -> u64 {
        self.uploads
            .iter()
            .filter(|(file, _)| *file == name || file.starts_with(&format!("{name}.patch.")))
            .map(|(_, f)| f.size)
            .sum()
    }

    /// The requests that changed something (anything but GET).
    pub fn writes(&self) -> Vec<String> {
        self.requests
            .iter()
            .filter(|r| !r.starts_with("GET "))
            .cloned()
            .collect()
    }

    /// Plays `sequence` as the scheduler would.
    pub fn play(&mut self, sequence: &str) {
        self.status = json!({
            "status_name": "playing", "current_playlist": {"playlist": "Christmas Show"},
            "current_sequence": sequence, "seconds_elapsed": "10", "seconds_remaining": "100",
            "next_playlist": {"playlist": "No playlist scheduled.", "start_time": ""}
        });
    }
}

/// A fake FPP listening on `127.0.0.1` until dropped.
pub struct FakeFpp {
    address: String,
    state: Arc<Mutex<FakeFppState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeFpp {
    /// Starts a fake FPP with nothing on it.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = listener.local_addr().expect("local address").to_string();
        let state = Arc::new(Mutex::new(FakeFppState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || {
                        let _ = serve(stream, &state);
                    });
                }
            })
        };
        Self {
            address,
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// `127.0.0.1:<port>`: use it where an FPP's address goes.
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn state(&self) -> MutexGuard<'_, FakeFppState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts a sequence of `size` bytes on the FPP.
    pub fn with_sequence(self, name: &str, size: u64) -> Self {
        self.state()
            .sequences
            .insert(name.to_string(), StoredFile { size, checksum: 0 });
        self
    }

    /// Puts a music file of `size` bytes on the FPP.
    pub fn with_music(self, name: &str, size: u64) -> Self {
        self.state()
            .music
            .insert(name.to_string(), StoredFile { size, checksum: 0 });
        self
    }

    /// Adds an empty playlist.
    pub fn with_playlist(self, name: &str) -> Self {
        self.state().playlists.insert(
            name.to_string(),
            json!({"name": name, "version": 3, "repeat": 0, "loopCount": 0, "empty": true,
                   "desc": "", "random": 0, "leadIn": [], "mainPlaylist": [], "leadOut": [],
                   "playlistInfo": {}}),
        );
        self
    }

    pub fn with_free_bytes(self, free: u64) -> Self {
        self.state().free_bytes = free;
        self
    }
}

impl Drop for FakeFpp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the accept loop so it sees the stop.
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((high * 16 + low) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_head(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<Request>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let path = target.split('?').next().unwrap_or("").to_string();
    let mut headers = BTreeMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(Some(Request {
        method,
        path,
        headers,
    }))
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        411 => "Length Required",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn content_length(request: &Request) -> Option<u64> {
    request.headers.get("content-length")?.parse().ok()
}

fn read_body(reader: &mut BufReader<TcpStream>, request: &Request) -> std::io::Result<Vec<u8>> {
    let mut body = vec![0; content_length(request).unwrap_or(0) as usize];
    reader.read_exact(&mut body)?;
    Ok(body)
}

fn serve(stream: TcpStream, state: &Mutex<FakeFppState>) -> std::io::Result<()> {
    let lock = || state.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut stream = stream;
    let Some(request) = read_head(&mut reader)? else {
        return Ok(());
    };
    let path = request.path.clone();
    let method = request.method.clone();
    if method == "PATCH" && (path == "/api/file/uploads" || path == "/api/file/upload") {
        return upload(&mut reader, &mut stream, &request, state);
    }
    lock().requests.push(format!("{method} {path}"));
    let body = if method == "POST" {
        read_body(&mut reader, &request)?
    } else {
        Vec::new()
    };
    let segments: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .map(percent_decode)
        .collect();
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    let (status, reply) = {
        let mut s = lock();
        route(&mut s, &method, &segments, &body)
    };
    respond(&mut stream, status, &reply)
}

/// FPP 9.3's `DeleteFile()` answer for a file that isn't there: `realpath()` of a missing file is
/// false, so it reports a bad path rather than "File Not Found".
const MISSING_FILE: &str = "Invalid path: directory traversal detected or file outside allowed directory";

const MUSIC: [&str; 9] = [
    ".mp3", ".ogg", ".m4a", ".wav", ".flac", ".aac", ".wma", ".m4p", ".au",
];

fn route(s: &mut FakeFppState, method: &str, segments: &[&str], body: &[u8]) -> (u16, String) {
    let ok = |v: Value| (200, v.to_string());
    match (method, segments) {
        ("GET", ["api", "system", "info"]) => ok(json!({
            "HostName": "FakeFPP", "Platform": "Raspberry Pi", "Variant": "Pi 4", "Mode": "player",
            "Version": "9.3", "majorVersion": 9, "minorVersion": 3,
            "Utilization": {"Disk": {"Media": {"Free": s.free_bytes, "Total": 31_000_000_000u64}}}
        })),
        ("GET", ["api", "fppd", "status"]) => ok(s.status.clone()),
        ("GET", ["api", "channel", "output", "universeOutputs"]) => ok(s.outputs.clone()),
        ("GET", ["api", "sequence"]) => ok(json!(
            s.sequences
                .keys()
                .filter_map(|n| n.strip_suffix(".fseq"))
                .collect::<Vec<_>>()
        )),
        ("GET", ["api", "media"]) => ok(json!(s.music.keys().collect::<Vec<_>>())),
        ("GET", ["api", "playlists"]) => {
            let mut names: Vec<&String> = s.playlists.keys().chain(s.broken_playlists.iter()).collect();
            names.sort();
            ok(json!(names))
        }
        // FPP answers a missing playlist with the JSON string "", and one it can't parse with null.
        ("GET", ["api", "playlist", name]) => match s.playlists.get(*name) {
            Some(p) => ok(p.clone()),
            None if s.broken_playlists.iter().any(|b| b == name) => (200, "null".to_string()),
            None => (200, "\"\"".to_string()),
        },
        // FPP's playlist_update(): writes the whole file, whatever was there.
        ("POST", ["api", "playlist", name]) => {
            let Ok(playlist) = serde_json::from_slice::<Value>(body) else {
                return (400, "{}".to_string());
            };
            s.broken_playlists.retain(|b| b != name);
            s.playlists.insert((*name).to_string(), playlist.clone());
            ok(playlist)
        }
        // FPP's PlaylistSectionInsertItem(): reads the file, pushes the entry, writes it back. A
        // file it can't parse comes back as just the new entry.
        ("POST", ["api", "playlist", name, section, "item"]) => {
            let Ok(entry) = serde_json::from_slice::<Value>(body) else {
                return (400, "{}".to_string());
            };
            if s.broken_playlists.iter().any(|b| b == name) {
                s.broken_playlists.retain(|b| b != name);
                s.playlists
                    .insert((*name).to_string(), json!({ *section: [entry] }));
                return ok(json!({"Status": "OK", "Message": ""}));
            }
            match s.playlists.get_mut(*name) {
                Some(playlist) => {
                    let list = &mut playlist[*section];
                    if !list.is_array() {
                        *list = json!([]);
                    }
                    list.as_array_mut().expect("an array").push(entry);
                    ok(json!({"Status": "OK", "Message": "", "playlistName": name, "sectionName": section}))
                }
                None => ok(json!({"Status": "Error", "Message": "Playlist does not exist."})),
            }
        }
        // GetFiles(): sizes are strings on a 64-bit FPP.
        ("GET", ["api", "files", "uploads"]) => ok(json!({
            "status": "ok",
            "files": s.uploads.iter().map(|(name, f)| json!({
                "name": name, "mtime": "10/06/26  12:00 PM", "sizeBytes": f.size.to_string(),
                "sizeHuman": format!("{} B", f.size)
            })).collect::<Vec<_>>()
        })),
        ("GET", ["api", "file", "move", name]) => {
            if s.fail_move.as_deref() == Some(*name) {
                return ok(json!({"status": "ERROR: Couldn't move sequence file"}));
            }
            let Some(file) = s.uploads.remove(*name) else {
                return ok(
                    json!({"status": format!("ERROR: Couldn't find file '{name}' in upload directory")}),
                );
            };
            let stored = StoredFile {
                size: file.size,
                checksum: file.sum.value(),
            };
            s.heads.insert((*name).to_string(), file.head.clone());
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".fseq") {
                s.sequences.insert((*name).to_string(), stored);
            } else if MUSIC.iter().any(|e| lower.ends_with(e)) {
                s.music.insert((*name).to_string(), stored);
            } else {
                s.uploads.insert((*name).to_string(), file);
                return ok(json!({"status": "ERROR: Couldn't move file"}));
            }
            ok(json!({"status": "OK"}))
        }
        ("DELETE", ["api", "file", "uploads", name]) => {
            if s.refuse_deletes {
                return ok(json!({"status": "Unable to delete file: Permission denied", "file": name}));
            }
            let removed = s.uploads.remove(*name).is_some();
            ok(json!({"status": if removed { "OK" } else { MISSING_FILE }, "file": name, "dir": "uploads"}))
        }
        ("POST", ["api", "command"]) => {
            s.commands.push(String::from_utf8_lossy(body).into_owned());
            (200, "Playlist Starting".to_string())
        }
        _ => (404, json!({"status": "not found"}).to_string()),
    }
}

/// `PATCH /api/file/uploads`, as FPP 9.3's `PatchFile()` handles it (or FPP 10's, with
/// `chunk_files`): the chunk at `Upload-Offset` of the file `Upload-Name`, `Upload-Length` bytes
/// in all. Offset 0 clears old pieces; a chunk that follows `.patch.0` is appended to it (9.x),
/// otherwise it becomes `.patch.<offset>`. Once the pieces add up to the length, the file is put
/// together in the upload folder. A full disk stores what fits and still answers "OK" with the
/// size it has, as PHP's `file_put_contents` does; `size` is a string, as `bcadd` makes it.
fn upload(
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
    request: &Request,
    state: &Mutex<FakeFppState>,
) -> std::io::Result<()> {
    let lock = || state.lock().unwrap_or_else(PoisonError::into_inner);
    let header = |name: &str| request.headers.get(name).cloned().unwrap_or_default();
    let name = header("upload-name");
    let offset: u64 = header("upload-offset").parse().unwrap_or(0);
    let total: u64 = header("upload-length").parse().unwrap_or(0);
    let Some(length) = content_length(request) else {
        lock()
            .requests
            .push(format!("PATCH /api/file/uploads {name}@{offset}+?"));
        return respond(stream, 411, "{}");
    };
    let prefix = format!("{name}.patch.");
    let (delay, fail, piece) = {
        let mut s = lock();
        s.requests
            .push(format!("PATCH /api/file/uploads {name}@{offset}+{length}"));
        s.largest_body = s.largest_body.max(length);
        if offset == 0 {
            s.uploads.retain(|file, _| !file.starts_with(&prefix));
        }
        let first = format!("{prefix}0");
        let appends =
            !s.chunk_files && offset != 0 && s.uploads.get(&first).is_some_and(|p| p.size == offset);
        let piece = if appends {
            first
        } else {
            format!("{prefix}{offset}")
        };
        (s.read_delay, s.fail_uploads.clone(), piece)
    };
    if let Some((status, body)) = fail {
        // PHP reads the whole request before its answer goes out.
        std::io::copy(&mut reader.take(length), &mut std::io::sink())?;
        return respond(stream, status, &body);
    }
    lock().uploads.entry(piece.clone()).or_insert(UploadFile {
        offset,
        ..UploadFile::default()
    });
    let mut left = length;
    let mut at = offset;
    let mut buffer = vec![0u8; 64 * 1024];
    while left > 0 {
        let want = buffer.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            // The sender went away (a cancelled upload): what arrived stays as a piece.
            return Ok(());
        }
        left -= read as u64;
        {
            let mut s = lock();
            let fits = (read as u64).min(s.free_bytes);
            let Some(file) = s.uploads.get_mut(&piece) else {
                // Deleted while it was arriving.
                return Ok(());
            };
            file.sum.add_at(at, &buffer[..fits as usize]);
            let room = HEAD_BYTES.saturating_sub(file.head.len());
            if file.offset == 0 && room > 0 {
                file.head.extend_from_slice(&buffer[..(fits as usize).min(room)]);
            }
            file.size += fits;
            s.free_bytes -= fits;
        }
        at += read as u64;
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    let (size, warning) = {
        let mut s = lock();
        let pieces: Vec<(String, UploadFile)> = s
            .uploads
            .iter()
            .filter(|(file, _)| file.starts_with(&prefix))
            .map(|(file, f)| (file.clone(), f.clone()))
            .collect();
        let size: u64 = pieces.iter().map(|(_, f)| f.size).sum();
        if size == total {
            let sum = pieces
                .iter()
                .fold(Checksum::default(), |sum, (_, f)| sum.combine(f.sum));
            let head = pieces
                .iter()
                .find(|(_, f)| f.offset == 0)
                .map(|(_, f)| f.head.clone())
                .unwrap_or_default();
            for (file, _) in &pieces {
                s.uploads.remove(file);
            }
            let short = s.short_assembly.unwrap_or(0);
            s.uploads.insert(
                name.clone(),
                UploadFile {
                    offset: 0,
                    size: total.saturating_sub(short),
                    sum,
                    head,
                },
            );
        }
        (size, s.php_warning.clone())
    };
    let held = size + lock().extra_held;
    let json = json!({"status": "OK", "file": name, "dir": "uploads", "size": held.to_string()});
    respond(stream, 200, &format!("{}{json}", warning.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("A%20B%2Ec"), "A B.c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%E2%9C%93"), "✓");
    }

    #[test]
    fn checksums_see_order_and_add_up_in_pieces() {
        assert_ne!(StoredFile::of(b"ab"), StoredFile::of(b"ba"));
        let mut first = Checksum::default();
        first.add_at(0, b"ab");
        let mut second = Checksum::default();
        second.add_at(2, b"cd");
        assert_eq!(first.combine(second).value(), StoredFile::of(b"abcd").checksum);
    }
}
