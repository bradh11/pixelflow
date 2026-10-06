//! A fake FPP that answers real HTTP on `127.0.0.1` (feature `test-fixtures`), for testing
//! uploads end to end without a device. It implements only the endpoints PixelFlow uses to send
//! a sequence (FPP 9.x's file-manager upload, the move into place, playlists) plus status and
//! commands, and it can be made slow, failing, or out of space.
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
        sum.add(data);
        Self {
            size: data.len() as u64,
            checksum: sum.value(),
        }
    }
}

/// An order-sensitive running checksum (Adler-style, 64-bit, wrapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checksum {
    a: u64,
    b: u64,
}

impl Default for Checksum {
    fn default() -> Self {
        Self { a: 1, b: 0 }
    }
}

impl Checksum {
    pub fn add(&mut self, data: &[u8]) {
        for &byte in data {
            self.a = self.a.wrapping_add(u64::from(byte));
            self.b = self.b.wrapping_add(self.a);
        }
    }

    pub fn value(&self) -> u64 {
        self.b.rotate_left(32) ^ self.a
    }
}

/// An upload in progress: the bytes received so far (FPP keeps them as `<name>.patch.<offset>`).
#[derive(Debug, Clone, Copy, Default)]
struct Partial {
    received: u64,
    sum: Checksum,
}

/// Everything the fake FPP knows; tests read and change it through [`FakeFpp::state`].
#[derive(Debug)]
pub struct FakeFppState {
    /// Sequences by file name (with `.fseq`).
    pub sequences: BTreeMap<String, StoredFile>,
    /// Music by file name.
    pub music: BTreeMap<String, StoredFile>,
    /// Finished uploads waiting to be moved into place, by file name.
    pub uploaded: BTreeMap<String, StoredFile>,
    /// Playlists by name, as FPP stores them.
    pub playlists: BTreeMap<String, Value>,
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
    /// Answer every upload request with this HTTP status instead of storing it.
    pub fail_uploads: Option<u16>,
    partials: BTreeMap<String, Partial>,
}

impl Default for FakeFppState {
    fn default() -> Self {
        Self {
            sequences: BTreeMap::new(),
            music: BTreeMap::new(),
            uploaded: BTreeMap::new(),
            playlists: BTreeMap::new(),
            free_bytes: 8 * 1024 * 1024 * 1024,
            requests: Vec::new(),
            commands: Vec::new(),
            largest_body: 0,
            read_delay: Duration::ZERO,
            fail_uploads: None,
            partials: BTreeMap::new(),
        }
    }
}

impl FakeFppState {
    /// Bytes of an upload still sitting unfinished in FPP's upload folder.
    pub fn partial_bytes(&self, name: &str) -> Option<u64> {
        self.partials.get(name).map(|p| p.received)
    }

    /// The requests that changed something (anything but GET).
    pub fn writes(&self) -> Vec<String> {
        self.requests
            .iter()
            .filter(|r| !r.starts_with("GET "))
            .cloned()
            .collect()
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
            json!({"name": name, "version": 4, "repeat": 0, "loopCount": 0, "empty": true,
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
    if method == "PATCH" && path == "/api/file/uploads" {
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

fn route(s: &mut FakeFppState, method: &str, segments: &[&str], body: &[u8]) -> (u16, String) {
    let ok = |v: Value| (200, v.to_string());
    match (method, segments) {
        ("GET", ["api", "system", "info"]) => ok(json!({
            "HostName": "FakeFPP", "Platform": "Raspberry Pi", "Variant": "Pi 4", "Mode": "player",
            "Version": "9.5.3", "majorVersion": 9, "minorVersion": 5,
            "Utilization": {"Disk": {"Media": {"Free": s.free_bytes, "Total": 31_000_000_000u64}}}
        })),
        ("GET", ["api", "fppd", "status"]) => ok(json!({
            "status_name": "idle", "current_playlist": {"playlist": ""}, "current_sequence": "",
            "seconds_elapsed": "0", "seconds_remaining": "0"
        })),
        ("GET", ["api", "sequence"]) => ok(json!(
            s.sequences
                .keys()
                .filter_map(|n| n.strip_suffix(".fseq"))
                .collect::<Vec<_>>()
        )),
        ("GET", ["api", "media"]) => ok(json!(s.music.keys().collect::<Vec<_>>())),
        ("GET", ["api", "playlists"]) => ok(json!(s.playlists.keys().collect::<Vec<_>>())),
        ("GET", ["api", "playlist", name]) => match s.playlists.get(*name) {
            Some(p) => ok(p.clone()),
            None => (404, "{}".to_string()),
        },
        ("POST", ["api", "playlist", name]) => {
            let Ok(playlist) = serde_json::from_slice::<Value>(body) else {
                return (400, "{}".to_string());
            };
            s.playlists.insert((*name).to_string(), playlist.clone());
            ok(playlist)
        }
        ("POST", ["api", "playlist", name, section, "item"]) => {
            let Ok(entry) = serde_json::from_slice::<Value>(body) else {
                return (400, "{}".to_string());
            };
            match s.playlists.get_mut(*name) {
                Some(playlist) => {
                    let list = &mut playlist[*section];
                    if !list.is_array() {
                        *list = json!([]);
                    }
                    list.as_array_mut().expect("an array").push(entry);
                    playlist["empty"] = json!(false);
                    ok(json!({"Status": "OK", "Message": "", "playlistName": name, "sectionName": section}))
                }
                None => ok(json!({"Status": "Error", "Message": "Playlist does not exist."})),
            }
        }
        ("GET", ["api", "file", "move", name]) => {
            let Some(file) = s.uploaded.remove(*name) else {
                return ok(
                    json!({"status": format!("ERROR: Couldn't find file '{name}' in upload directory")}),
                );
            };
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".fseq") {
                s.sequences.insert((*name).to_string(), file);
            } else if [
                ".mp3", ".ogg", ".m4a", ".wav", ".flac", ".aac", ".wma", ".m4p", ".au",
            ]
            .iter()
            .any(|e| lower.ends_with(e))
            {
                s.music.insert((*name).to_string(), file);
            } else {
                return ok(json!({"status": "ERROR: Couldn't move file"}));
            }
            ok(json!({"status": "OK"}))
        }
        ("DELETE", ["api", "file", "uploads", name]) => {
            let removed = name
                .strip_suffix(".patch.0")
                .and_then(|n| s.partials.remove(n))
                .is_some()
                || s.uploaded.remove(*name).is_some();
            ok(
                json!({"status": if removed { "OK" } else { "File Not Found" }, "file": name, "dir": "uploads"}),
            )
        }
        ("POST", ["api", "command"]) => {
            s.commands.push(String::from_utf8_lossy(body).into_owned());
            (200, "Playlist Starting".to_string())
        }
        _ => (404, json!({"status": "not found"}).to_string()),
    }
}

/// `PATCH /api/file/uploads`, as FPP's `PatchFile()` handles it: the chunk at `Upload-Offset` of
/// the file `Upload-Name`, `Upload-Length` bytes long in all; once every byte is in, the file is
/// put together in the upload folder. A full disk stores what fits and still answers "OK" with
/// the size it has, as PHP's `file_put_contents` does.
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
    let (delay, fail) = {
        let mut s = lock();
        s.requests
            .push(format!("PATCH /api/file/uploads {name}@{offset}+{length}"));
        s.largest_body = s.largest_body.max(length);
        (s.read_delay, s.fail_uploads)
    };
    if let Some(status) = fail {
        // PHP reads the whole request before its answer goes out.
        std::io::copy(&mut reader.take(length), &mut std::io::sink())?;
        return respond(stream, status, "{\"status\":\"failed\"}");
    }
    {
        let mut s = lock();
        let partial = s.partials.entry(name.clone()).or_default();
        if offset == 0 {
            *partial = Partial::default();
        }
        if partial.received != offset {
            return respond(stream, 400, "{\"status\":\"out of order\"}");
        }
    }
    let mut left = length;
    let mut buffer = vec![0u8; 64 * 1024];
    while left > 0 {
        let want = buffer.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let read = reader.read(&mut buffer[..want])?;
        if read == 0 {
            // The sender went away (a cancelled upload): what arrived stays as a partial file.
            return Ok(());
        }
        left -= read as u64;
        {
            let mut s = lock();
            let fits = (read as u64).min(s.free_bytes);
            let Some(partial) = s.partials.get_mut(&name) else {
                // Deleted while it was arriving.
                return Ok(());
            };
            partial.sum.add(&buffer[..fits as usize]);
            partial.received += fits;
            s.free_bytes -= fits;
        }
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    let size = {
        let mut s = lock();
        let partial = s.partials.get(&name).copied().unwrap_or_default();
        if partial.received == total {
            s.partials.remove(&name);
            s.uploaded.insert(
                name.clone(),
                StoredFile {
                    size: total,
                    checksum: partial.sum.value(),
                },
            );
        }
        partial.received
    };
    respond(
        stream,
        200,
        &json!({"status": "OK", "file": name, "dir": "uploads", "size": size}).to_string(),
    )
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
    fn checksums_see_order() {
        assert_ne!(StoredFile::of(b"ab"), StoredFile::of(b"ba"));
        let mut sum = Checksum::default();
        sum.add(b"a");
        sum.add(b"b");
        assert_eq!(sum.value(), StoredFile::of(b"ab").checksum);
    }
}
