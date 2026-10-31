//! Cisco's OpenH264 encoder, as Cisco builds and hands it out.
//!
//! Cisco pays the H.264 patent pool's royalties for its own OpenH264 binary, and only when each
//! user's copy of an app fetches that binary from Cisco ([`LICENCE`]; the FAQ at
//! <https://www.openh264.org/faq.html>). An encoder compiled from the source and shipped inside
//! PixelFlow isn't covered. So the app ships no H.264 encoder: when the user agrees,
//! [`Store::download`] fetches the binary for this computer straight from Cisco's server, checks
//! it against the SHA-256 pinned here, and keeps it in the app's data folder. [`load`] checks it
//! again every time, before loading it. The user can turn it off ([`Store::set_enabled`]) and
//! remove it ([`Store::remove`]), and the app shows [`ATTRIBUTION`] where they do.
//!
//! The version is the one the `openh264` crate's bindings are made for. Cisco publishes an MD5
//! for each binary (`<file>.signed.md5.txt` beside it), not a SHA-256. The SHA-256s here were
//! computed from Cisco's files, whose MD5s match Cisco's published ones; they are also the ones
//! `openh264-sys2` lists (`src/blobs/hashes.txt`), which it checks again as it loads the file.

use openh264::OpenH264API;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The release of Cisco's binary used: the one the `openh264` crate's bindings are made for.
pub const VERSION: &str = "2.6.0";

/// What Cisco's licence says must be shown where the user can turn the encoder on and off.
pub const ATTRIBUTION: &str = "OpenH264 Video Codec provided by Cisco Systems, Inc.";

/// Cisco's licence for its binary, word for word (from <https://www.openh264.org/BINARY_LICENSE.txt>).
/// Its terms ask for the whole text where the app shows licensing information.
pub const LICENCE: &str = include_str!("../data/openh264-BINARY_LICENSE.txt");

/// Where Cisco publishes its licence.
pub const LICENCE_URL: &str = "https://www.openh264.org/BINARY_LICENSE.txt";

/// Cisco's server for the binaries.
pub const HOST: &str = "ciscobinary.openh264.org";

/// One of Cisco's binaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binary {
    /// The system it's for, as Rust names it (`std::env::consts::OS` and `ARCH`).
    pub os: &'static str,
    pub arch: &'static str,
    /// The library's file name, as Cisco names it.
    pub file: &'static str,
    /// Where Cisco publishes it, compressed with bzip2.
    pub url: &'static str,
    /// The library's SHA-256, lowercase hex.
    pub sha256: &'static str,
    /// The library's size.
    pub bytes: u64,
    /// The size of the download (the compressed file), about.
    pub download_bytes: u64,
}

/// Cisco's OpenH264 2.6.0 for each system PixelFlow runs on.
pub const BINARIES: [Binary; 5] = [
    Binary {
        os: "macos",
        arch: "aarch64",
        file: "libopenh264-2.6.0-mac-arm64.dylib",
        url: "https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-arm64.dylib.bz2",
        sha256: "052e98bfcf7a9167d22f3bbb3f5988ef79065591f36af8b52924b22b13624551",
        bytes: 1_207_136,
        download_bytes: 482_124,
    },
    Binary {
        os: "macos",
        arch: "x86_64",
        file: "libopenh264-2.6.0-mac-x64.dylib",
        url: "https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-x64.dylib.bz2",
        sha256: "e3dc8bc01fe69363f61fd3c02fd27798537a585eadd38cd808f303d1ee505a19",
        bytes: 1_381_104,
        download_bytes: 529_614,
    },
    Binary {
        os: "windows",
        arch: "x86_64",
        file: "openh264-2.6.0-win64.dll",
        url: "https://ciscobinary.openh264.org/openh264-2.6.0-win64.dll.bz2",
        sha256: "2076cb5675ec6c1a4c70e7a2a322552f547b6eeed649d6dfcd9e02a543b24691",
        bytes: 978_520,
        download_bytes: 452_053,
    },
    Binary {
        os: "linux",
        arch: "x86_64",
        file: "libopenh264-2.6.0-linux64.8.so",
        url: "https://ciscobinary.openh264.org/libopenh264-2.6.0-linux64.8.so.bz2",
        sha256: "2f0cde7c6a6abcf5cae76942894ea42897fa677bce4ed6c91a24dd1b041d5f04",
        bytes: 1_731_128,
        download_bytes: 634_264,
    },
    Binary {
        os: "linux",
        arch: "aarch64",
        file: "libopenh264-2.6.0-linux-arm64.8.so",
        url: "https://ciscobinary.openh264.org/libopenh264-2.6.0-linux-arm64.8.so.bz2",
        sha256: "12e7b33623667cdab0e575170c147b1b36eadb77d0d2aa7ceb5afd3e58902140",
        bytes: 1_492_120,
        download_bytes: 619_371,
    },
];

/// Cisco's binary for the system this program was built for, if Cisco makes one.
pub fn for_this_computer() -> Option<&'static Binary> {
    BINARIES
        .iter()
        .find(|b| b.os == std::env::consts::OS && b.arch == std::env::consts::ARCH)
}

/// The most a download may be: Cisco's files are well under a megabyte.
const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024;

/// Bytes read between checks for a stop and reports of progress.
const BLOCK: usize = 1 << 14;

/// The file in the encoder's folder that says the user turned it off.
const OFF: &str = "off";

/// How PixelFlow names itself to Cisco's server.
const USER_AGENT: &str = concat!("PixelFlow/", env!("CARGO_PKG_VERSION"));

/// Whether the encoder can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Cisco makes no binary for this system.
    Unsupported,
    /// Not downloaded.
    Missing,
    /// Downloaded, and turned off by the user.
    Off,
    /// Downloaded and on (checked again when it's loaded).
    Ready,
}

/// Why the encoder couldn't be downloaded, kept, or loaded: each a sentence for the user.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error(
        "Cisco doesn't make its OpenH264 encoder for this system ({os}, {arch}), so videos can only be exported with ffmpeg here.",
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH
    )]
    Unsupported,
    #[error("Cisco's OpenH264 video encoder isn't downloaded yet.")]
    Missing,
    #[error("Cisco's OpenH264 video encoder is turned off in Settings, under Video export.")]
    Off,
    #[error(
        "PixelFlow couldn't reach Cisco's server ({HOST}). Check that this computer is online, then try again."
    )]
    Offline,
    #[error("Cisco's server took too long to answer. Try again in a moment.")]
    Timeout,
    #[error(
        "The proxy this computer uses to reach the internet stopped the download ({0}). Check the proxy named in HTTPS_PROXY or ALL_PROXY, or try another network."
    )]
    Proxy(String),
    #[error(
        "The connection to Cisco's server couldn't be checked as secure, so nothing was downloaded. A network filter or proxy may be in the way ({0})."
    )]
    Insecure(String),
    #[error("Cisco's server answered with an error (HTTP {0}). Try again later.")]
    Server(u16),
    #[error("The download from Cisco stopped part way ({0}). Try again.")]
    Interrupted(String),
    #[error(
        "Cisco's server sent more than expected, so the download was stopped and nothing was kept. Try again."
    )]
    TooLarge,
    #[error("The file from Cisco couldn't be unpacked ({0}), so nothing was kept. Try again.")]
    Damaged(String),
    #[error(
        "The file from Cisco isn't the one PixelFlow expects (its SHA-256 doesn't match), so it was thrown away. Try again; if it happens again, something on this network may be changing downloads."
    )]
    Mismatch { expected: String, found: String },
    #[error(
        "PixelFlow couldn't keep the encoder in {}: {source}. Check that the disk has room and that the folder can be written to.",
        path.display()
    )]
    Folder {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "Cisco's OpenH264 encoder in PixelFlow's data folder isn't the file that was checked when it was downloaded, so it wasn't used. Download it again."
    )]
    Changed,
    #[error("Cisco's OpenH264 encoder couldn't be loaded: {0}")]
    Load(String),
    #[error("Stopped.")]
    Cancelled,
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A download's error, as the user can act on it.
fn network_error(error: ureq::Error) -> CodecError {
    use ureq::Error;
    match error {
        Error::HostNotFound | Error::ConnectionFailed => CodecError::Offline,
        Error::Io(e)
            if matches!(
                e.kind(),
                ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset
                    | ErrorKind::NotConnected
                    | ErrorKind::AddrNotAvailable
                    | ErrorKind::NetworkUnreachable
                    | ErrorKind::HostUnreachable
                    | ErrorKind::NetworkDown
            ) =>
        {
            CodecError::Offline
        }
        Error::Timeout(_) => CodecError::Timeout,
        Error::Io(e) if e.kind() == ErrorKind::TimedOut => CodecError::Timeout,
        Error::ConnectProxyFailed(why) => CodecError::Proxy(why),
        Error::InvalidProxyUrl => CodecError::Proxy("its address isn't a valid URL".into()),
        Error::Tls(why) => CodecError::Insecure(why.to_string()),
        Error::Rustls(e) => CodecError::Insecure(e.to_string()),
        Error::RequireHttpsOnly(_) => CodecError::Insecure("it was sent to a plain HTTP address".into()),
        other => CodecError::Interrupted(other.to_string()),
    }
}

/// Where the encoder is kept: `openh264/<version>/<Cisco's file name>` in the app's data folder,
/// with `openh264/off` there while the user has it turned off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
    binary: Option<&'static Binary>,
}

impl Store {
    /// The encoder for this computer, kept under `data_dir`.
    pub fn new(data_dir: &Path) -> Self {
        Self::for_binary(data_dir, for_this_computer())
    }

    /// `binary` (none: a system Cisco makes none for), kept under `data_dir`.
    pub fn for_binary(data_dir: &Path, binary: Option<&'static Binary>) -> Self {
        Self {
            root: data_dir.join("openh264"),
            binary,
        }
    }

    pub fn binary(&self) -> Option<&'static Binary> {
        self.binary
    }

    fn dir(&self) -> PathBuf {
        self.root.join(VERSION)
    }

    /// Where the library is kept (whether or not it's there).
    pub fn library(&self) -> Option<PathBuf> {
        self.binary.map(|b| self.dir().join(b.file))
    }

    fn folder_error(&self, source: std::io::Error) -> CodecError {
        CodecError::Folder {
            path: self.root.clone(),
            source,
        }
    }

    /// Whether the encoder is here, at its size, and on. The file's SHA-256 is checked when
    /// it's loaded ([`load`]).
    pub fn state(&self) -> State {
        let (Some(binary), Some(library)) = (self.binary, self.library()) else {
            return State::Unsupported;
        };
        if !fs::metadata(library).is_ok_and(|m| m.is_file() && m.len() == binary.bytes) {
            State::Missing
        } else if self.root.join(OFF).exists() {
            State::Off
        } else {
            State::Ready
        }
    }

    /// The library, for an export: when it's here and on.
    pub fn ready(&self) -> Result<PathBuf, CodecError> {
        match self.state() {
            State::Unsupported => Err(CodecError::Unsupported),
            State::Missing => Err(CodecError::Missing),
            State::Off => Err(CodecError::Off),
            State::Ready => self.library().ok_or(CodecError::Unsupported),
        }
    }

    /// Turns the encoder on or off (kept across runs). Off, video export doesn't load it.
    pub fn set_enabled(&self, on: bool) -> Result<(), CodecError> {
        let off = self.root.join(OFF);
        if on {
            match fs::remove_file(&off) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(self.folder_error(e)),
                _ => Ok(()),
            }
        } else {
            fs::create_dir_all(&self.root).map_err(|e| self.folder_error(e))?;
            fs::write(off, b"").map_err(|e| self.folder_error(e))
        }
    }

    /// Keeps the library from `compressed` (Cisco's `.bz2`, `total` bytes about): read whole,
    /// unpacked, checked against the pinned SHA-256, written beside its place, then put there,
    /// and turned on. Tells `progress` the bytes read so far and `total`; gives up when `stop`
    /// says so, keeping nothing.
    pub fn install_from(
        &self,
        compressed: &mut dyn Read,
        total: u64,
        progress: &dyn Fn(u64, u64),
        stop: &dyn Fn() -> bool,
    ) -> Result<PathBuf, CodecError> {
        let binary = self.binary.ok_or(CodecError::Unsupported)?;
        let library = self.library().ok_or(CodecError::Unsupported)?;
        let packed = read_all(compressed, total, progress, stop)?;
        if stop() {
            return Err(CodecError::Cancelled);
        }
        let unpacked = unpack(&packed, binary.bytes)?;
        let found = hex(&Sha256::digest(&unpacked));
        if found != binary.sha256 || unpacked.len() as u64 != binary.bytes {
            return Err(CodecError::Mismatch {
                expected: binary.sha256.to_string(),
                found,
            });
        }
        let dir = self.dir();
        fs::create_dir_all(&dir).map_err(|e| self.folder_error(e))?;
        let part = dir.join(format!("{}.part", binary.file));
        let written = (|| {
            let mut out = fs::File::create(&part)?;
            out.write_all(&unpacked)?;
            out.sync_all()?;
            drop(out);
            fs::rename(&part, &library)
        })();
        if let Err(error) = written {
            let _ = fs::remove_file(&part);
            return Err(self.folder_error(error));
        }
        // Downloading it is the user turning it on.
        self.set_enabled(true)?;
        Ok(library)
    }

    /// Downloads the library from Cisco's server over HTTPS (see [`Self::install_from`]).
    pub fn download(
        &self,
        progress: &dyn Fn(u64, u64),
        stop: &dyn Fn() -> bool,
    ) -> Result<PathBuf, CodecError> {
        let binary = self.binary.ok_or(CodecError::Unsupported)?;
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(5 * 60)))
            .http_status_as_error(false)
            .https_only(true)
            .max_redirects(3)
            .user_agent(USER_AGENT)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let response = agent.get(binary.url).call().map_err(network_error)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(CodecError::Server(status));
        }
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(binary.download_bytes);
        let mut body = response.into_body().into_reader();
        self.install_from(&mut body, total, progress, stop)
    }

    /// Deletes the encoder, every version of it, and the setting that turns it off.
    pub fn remove(&self) -> Result<(), CodecError> {
        match fs::remove_dir_all(&self.root) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(self.folder_error(e)),
            _ => Ok(()),
        }
    }
}

/// All of `source`, at most [`MAX_DOWNLOAD`] bytes.
fn read_all(
    source: &mut dyn Read,
    total: u64,
    progress: &dyn Fn(u64, u64),
    stop: &dyn Fn() -> bool,
) -> Result<Vec<u8>, CodecError> {
    let mut packed = Vec::with_capacity(total.min(MAX_DOWNLOAD) as usize);
    let mut buf = vec![0u8; BLOCK];
    loop {
        if stop() {
            return Err(CodecError::Cancelled);
        }
        let n = match source.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut => return Err(CodecError::Timeout),
            Err(e) => return Err(CodecError::Interrupted(e.to_string())),
        };
        packed.extend_from_slice(&buf[..n]);
        if packed.len() as u64 > MAX_DOWNLOAD {
            return Err(CodecError::TooLarge);
        }
        progress(packed.len() as u64, total.max(packed.len() as u64));
    }
    Ok(packed)
}

/// `packed` unpacked, refusing more than `bytes` (the size the library must be).
fn unpack(packed: &[u8], bytes: u64) -> Result<Vec<u8>, CodecError> {
    let mut unpacked = Vec::with_capacity(bytes as usize);
    bzip2_rs::DecoderReader::new(packed)
        .take(bytes + 1)
        .read_to_end(&mut unpacked)
        .map_err(|e| CodecError::Damaged(e.to_string()))?;
    Ok(unpacked)
}

/// Whether the file at `path` is `binary`: its size, then its SHA-256.
fn check(path: &Path, binary: &Binary) -> Result<(), CodecError> {
    let size = fs::metadata(path).map_err(|_| CodecError::Missing)?.len();
    if size != binary.bytes {
        return Err(CodecError::Changed);
    }
    let bytes = fs::read(path).map_err(|_| CodecError::Missing)?;
    if hex(&Sha256::digest(&bytes)) != binary.sha256 {
        return Err(CodecError::Changed);
    }
    Ok(())
}

/// Cisco's library at `path`, checked against this computer's pinned SHA-256 and then loaded.
/// Done for every export, so a file changed since it was downloaded is never loaded.
pub fn load(path: &Path) -> Result<OpenH264API, CodecError> {
    let binary = for_this_computer().ok_or(CodecError::Unsupported)?;
    check(path, binary)?;
    // `openh264` checks the file once more against its own list of Cisco's binaries.
    OpenH264API::from_blob_path(path).map_err(|e| CodecError::Load(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// "not really a library", compressed with bzip2.
    const PACKED: [u8; 55] = [
        0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0xd9, 0x37, 0xe5, 0x47, 0x00, 0x00, 0x06,
        0x11, 0x80, 0x40, 0x00, 0x32, 0x25, 0x94, 0x20, 0x20, 0x00, 0x22, 0x01, 0xa6, 0x8d, 0x08, 0x06, 0x9a,
        0x68, 0x52, 0xa8, 0x23, 0x1b, 0x79, 0x27, 0xb8, 0x05, 0x87, 0xc5, 0xdc, 0x91, 0x4e, 0x14, 0x24, 0x36,
        0x4d, 0xf9, 0x51, 0xc0,
    ];
    const FAKE: Binary = Binary {
        os: "test",
        arch: "test",
        file: "libfake.dylib",
        url: "https://ciscobinary.openh264.org/libfake.dylib.bz2",
        sha256: "8c94411e7c8b87bd238af6ff286f5e356e89b0b393364163b58e2ca433a40fe5",
        bytes: 20,
        download_bytes: 55,
    };
    const WRONG: Binary = Binary {
        sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ..FAKE
    };

    fn store(dir: &Path, binary: &'static Binary) -> Store {
        Store::for_binary(dir, Some(binary))
    }

    #[test]
    fn a_download_is_unpacked_checked_kept_and_turned_on() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), &FAKE);
        assert_eq!(store.state(), State::Missing);
        assert!(matches!(store.ready(), Err(CodecError::Missing)));
        store.set_enabled(false).unwrap();
        let seen = Cell::new((0, 0));
        let kept = store
            .install_from(&mut &PACKED[..], 55, &|n, of| seen.set((n, of)), &|| false)
            .unwrap();
        assert_eq!(seen.get(), (55, 55));
        assert_eq!(kept, dir.path().join("openh264/2.6.0/libfake.dylib"));
        assert_eq!(fs::read(&kept).unwrap(), b"not really a library");
        assert_eq!(store.state(), State::Ready);
        assert_eq!(store.ready().unwrap(), kept);
        let leftovers: Vec<_> = fs::read_dir(kept.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "nothing half-written is left");
    }

    #[test]
    fn a_file_that_isnt_ciscos_is_thrown_away() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), &WRONG);
        let error = store
            .install_from(&mut &PACKED[..], 55, &|_, _| {}, &|| false)
            .unwrap_err();
        assert!(matches!(error, CodecError::Mismatch { .. }), "{error}");
        assert!(error.to_string().contains("isn't the one PixelFlow expects"));
        assert_eq!(store.state(), State::Missing);
        assert!(!dir.path().join("openh264/2.6.0/libfake.dylib.part").exists());
    }

    #[test]
    fn damaged_or_oversized_downloads_keep_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), &FAKE);
        let mut cut = PACKED;
        cut[30] ^= 0xff;
        let error = store
            .install_from(&mut &cut[..], 55, &|_, _| {}, &|| false)
            .unwrap_err();
        assert!(
            matches!(error, CodecError::Damaged(_) | CodecError::Mismatch { .. }),
            "{error}"
        );
        let error = store
            .install_from(&mut &b"plain text, not bzip2"[..], 21, &|_, _| {}, &|| false)
            .unwrap_err();
        assert!(matches!(error, CodecError::Damaged(_)), "{error}");
        let mut endless = std::io::repeat(0x42);
        let error = store
            .install_from(&mut endless, 55, &|_, _| {}, &|| false)
            .unwrap_err();
        assert!(matches!(error, CodecError::TooLarge), "{error}");
        assert_eq!(store.state(), State::Missing);
    }

    #[test]
    fn a_stopped_download_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), &FAKE);
        let error = store
            .install_from(&mut &PACKED[..], 55, &|_, _| {}, &|| true)
            .unwrap_err();
        assert!(matches!(error, CodecError::Cancelled));
        assert_eq!(error.to_string(), "Stopped.");
        assert!(!dir.path().join("openh264").exists());
    }

    #[test]
    fn an_unwritable_folder_says_where() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the encoder's folder would go.
        fs::write(dir.path().join("openh264"), b"in the way").unwrap();
        let store = store(dir.path(), &FAKE);
        let error = store
            .install_from(&mut &PACKED[..], 55, &|_, _| {}, &|| false)
            .unwrap_err();
        assert!(matches!(error, CodecError::Folder { .. }), "{error}");
        assert!(
            error.to_string().contains("couldn't keep the encoder in"),
            "{error}"
        );
    }

    #[test]
    fn it_can_be_turned_off_and_on_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), &FAKE);
        store
            .install_from(&mut &PACKED[..], 55, &|_, _| {}, &|| false)
            .unwrap();
        store.set_enabled(false).unwrap();
        assert_eq!(store.state(), State::Off);
        assert!(matches!(store.ready(), Err(CodecError::Off)));
        // Kept across runs: a new store sees it off.
        assert_eq!(Store::for_binary(dir.path(), Some(&FAKE)).state(), State::Off);
        store.set_enabled(true).unwrap();
        store.set_enabled(true).unwrap();
        assert_eq!(store.state(), State::Ready);
        store.remove().unwrap();
        assert_eq!(store.state(), State::Missing);
        assert!(!dir.path().join("openh264").exists());
        store.remove().unwrap();
    }

    #[test]
    fn a_system_cisco_makes_nothing_for() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::for_binary(dir.path(), None);
        assert_eq!(store.state(), State::Unsupported);
        assert!(matches!(store.ready(), Err(CodecError::Unsupported)));
        let error = store.download(&|_, _| {}, &|| false).unwrap_err();
        assert!(matches!(error, CodecError::Unsupported));
        assert!(
            error
                .to_string()
                .starts_with("Cisco doesn't make its OpenH264 encoder for this system (")
        );
        assert!(!dir.path().join("openh264").exists());
    }

    #[test]
    fn a_changed_file_is_never_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("libopenh264.dylib");
        match for_this_computer() {
            Some(binary) => {
                // The right size, the wrong bytes.
                fs::write(&path, vec![0u8; binary.bytes as usize]).unwrap();
                assert!(matches!(load(&path), Err(CodecError::Changed)));
                fs::write(&path, b"short").unwrap();
                assert!(matches!(load(&path), Err(CodecError::Changed)));
                fs::remove_file(&path).unwrap();
                assert!(matches!(load(&path), Err(CodecError::Missing)));
            }
            None => assert!(matches!(load(&path), Err(CodecError::Unsupported))),
        }
    }

    #[test]
    fn network_failures_read_as_what_to_do() {
        let offline = network_error(ureq::Error::HostNotFound);
        assert!(matches!(offline, CodecError::Offline));
        assert!(offline.to_string().contains("ciscobinary.openh264.org"));
        let refused = network_error(ureq::Error::Io(std::io::Error::from(
            ErrorKind::ConnectionRefused,
        )));
        assert!(matches!(refused, CodecError::Offline));
        let proxy = network_error(ureq::Error::ConnectProxyFailed(
            "407 Proxy Authentication Required".into(),
        ));
        assert!(matches!(proxy, CodecError::Proxy(_)));
        assert!(proxy.to_string().contains("HTTPS_PROXY"), "{proxy}");
        assert!(matches!(
            network_error(ureq::Error::Timeout(ureq::Timeout::Connect)),
            CodecError::Timeout
        ));
        assert!(matches!(
            network_error(ureq::Error::Tls("bad certificate")),
            CodecError::Insecure(_)
        ));
        assert_eq!(
            CodecError::Server(503).to_string(),
            "Cisco's server answered with an error (HTTP 503). Try again later."
        );
    }

    #[test]
    fn every_binary_is_pinned_to_ciscos_server_over_https() {
        for b in BINARIES {
            assert_eq!(b.sha256.len(), 64, "{}", b.file);
            assert!(
                b.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            );
            assert_eq!(b.url, format!("https://{HOST}/{}.bz2", b.file));
            assert!(b.file.contains(&format!("-{VERSION}-")), "{}", b.file);
            assert!(b.bytes > b.download_bytes && b.download_bytes > 100_000);
        }
        // Every system the app is built for has one.
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
        ] {
            assert_eq!(
                BINARIES.iter().filter(|b| b.os == os && b.arch == arch).count(),
                1
            );
        }
        // The licence shown is Cisco's, with the line the app must show.
        assert!(LICENCE.contains(ATTRIBUTION));
        assert!(LICENCE.contains("AVC/H.264 Patent Portfolio License Conditions"));
    }

    #[test]
    fn the_crate_refuses_a_file_it_does_not_know_too() {
        // `openh264` checks a library's SHA-256 against its own list of Cisco's binaries before
        // loading it: a second lock behind the pinned hashes here.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-cisco.dylib");
        fs::write(&path, b"not a library").unwrap();
        let refused = OpenH264API::from_blob_path(&path).err().map(|e| e.to_string());
        assert!(
            refused.is_some_and(|e| e.contains("Invalid hash")),
            "unknown files are refused"
        );
    }
}
