//! Downloading a sequence and its music from an FPP to this computer — read-only.
//!
//! Only these endpoints are read, checked against FPP 9.5.3's PHP:
//! - `GET /api/file/sequences/<name>` and `GET /api/file/music/<name>` (`www/api/index.php`
//!   routes `/file/:DirName/**` to `files.php` `GetFile()`, whose `GetFileImpl()` `readfile()`s
//!   the file from that folder as `application/binary`, or answers 404). Only these two folders
//!   are ever named, and only with a plain file name the FPP itself listed: `GetFileImpl()`
//!   doesn't refuse `..`, so a name with a path in it could read files outside the folder.
//! - `/api/files/{sequences,music}` (`GetFiles()`): the names and sizes there (see [`fpp_info`]).
//! - `/api/sequence/<name>/meta` (`GetSequenceMetaData()`, FPP's `fsequtils -j`): the sequence's
//!   channel count and its `mf` (music file) header.
//!
//! A file arrives in a hidden temporary file next to where it goes, and is renamed into place
//! only once every byte is in: a cancelled or failed download leaves nothing behind.
//!
//! [`fpp_info`]: crate::fpp_info

use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, str_field};
use crate::fpp_info::{FppFile, FppFolder, base, listing};
use crate::fpp_player::encode_segment;
use crate::fpp_upload::keep_both_name;
use crate::http::Http;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Why a download didn't work, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    #[error("The download was cancelled. Nothing was saved.")]
    Cancelled,
    #[error(
        "Couldn't reach the FPP at {address}: {reason}. Check that it's on and on the same network as this computer. Nothing was saved."
    )]
    Unreachable { address: String, reason: String },
    #[error("The FPP no longer has {name}. Refresh the list and try again.")]
    Gone { name: String },
    #[error("Couldn't download {name} from the FPP: {reason}. Nothing was saved.")]
    Failed { name: String, reason: String },
    #[error("Couldn't save {name} on this computer: {reason}. Nothing was saved.")]
    Local { name: String, reason: String },
    #[error(
        "There's now a file called {name} in that folder, so nothing was replaced. Check again and choose what to do."
    )]
    Exists { name: String },
    #[error("PixelFlow can't download {name} because of the characters in its name.")]
    BadName { name: String },
}

impl DownloadError {
    fn from_device(name: &str, error: DeviceError) -> Self {
        match error {
            DeviceError::Unreachable { address, reason } => Self::Unreachable { address, reason },
            DeviceError::Http { status: 404, .. } => Self::Gone {
                name: name.to_string(),
            },
            other => Self::Failed {
                name: name.to_string(),
                reason: other.to_string().trim_end_matches('.').to_string(),
            },
        }
    }
}

/// Whether `name` is one plain file name (never a path), safe to put in a download URL and to
/// save under on this computer.
pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

/// The music file's name from a sequence's `mf` header. xLights writes the path on the computer
/// that made the sequence (`C:\Shows\Audio\Song.mp3`, `/Users/me/Song.mp3`), so only the last
/// part is kept.
pub fn media_file_name(mf: &str) -> Option<&str> {
    let name = mf.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty()).then_some(name)
}

/// The FPP's music file the `mf` header names: the same name, else the same name whatever its
/// capitals.
pub fn match_music<'a>(mf: &str, music: &'a [String]) -> Option<&'a String> {
    let name = media_file_name(mf)?;
    music
        .iter()
        .find(|m| *m == name)
        .or_else(|| music.iter().find(|m| m.eq_ignore_ascii_case(name)))
}

/// A sequence on the FPP, as read before downloading it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSequence {
    pub file: FppFile,
    /// The `mf` header as the sequence has it (often a path on another computer).
    pub media: Option<String>,
    /// The FPP's music file it names, when the FPP has it.
    pub music: Option<FppFile>,
}

/// Reads what downloading `name` (a sequence the FPP lists, with `.fseq`) would fetch: its size
/// and channels, and the music its `mf` header names, if the FPP has it (changes nothing).
pub fn read_sequence(http: &dyn Http, host: &str, name: &str) -> Result<RemoteSequence, DownloadError> {
    if !is_plain_name(name) {
        return Err(DownloadError::BadName {
            name: name.to_string(),
        });
    }
    let device = |e| DownloadError::from_device(name, e);
    let mut file = listing(http, host, "sequences")
        .map_err(device)?
        .iter()
        .find(|f| str_field(f, "name").trim() == name)
        .map(base)
        .ok_or_else(|| DownloadError::Gone {
            name: name.to_string(),
        })?;
    let stem = name.strip_suffix(".fseq").unwrap_or(name);
    let meta = get_json(
        http,
        host,
        &format!("/api/sequence/{}/meta", encode_segment(stem)),
    )
    .ok();
    file.channels = meta
        .as_ref()
        .and_then(|m| u32::try_from(int_field(m, "ChannelCount")).ok())
        .filter(|&c| c > 0);
    let media = meta
        .as_ref()
        .and_then(|m| m.pointer("/variableHeaders/mf"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(String::from);
    let music = match &media {
        Some(mf) => {
            let files: Vec<FppFile> = listing(http, host, "music")
                .map_err(device)?
                .iter()
                .map(base)
                .filter(|f| is_plain_name(&f.name))
                .collect();
            let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
            match_music(mf, &names).and_then(|n| files.iter().find(|f| &f.name == n).cloned())
        }
        None => None,
    };
    Ok(RemoteSequence { file, media, music })
}

/// What to do when the folder already has a file by that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Clash {
    Replace,
    KeepBoth,
}

/// Where `name` is saved in `folder`: under its own name when that's free, or when the user
/// chose to replace the file there; under the next free "Name (2)" name for Keep both. With no
/// choice made, a file that has appeared there since is never replaced.
pub fn target_path(folder: &Path, name: &str, clash: Option<Clash>) -> Result<PathBuf, DownloadError> {
    if !is_plain_name(name) {
        return Err(DownloadError::BadName {
            name: name.to_string(),
        });
    }
    let taken = |n: &str| folder.join(n).exists();
    match clash {
        _ if !taken(name) => Ok(folder.join(name)),
        Some(Clash::Replace) => Ok(folder.join(name)),
        Some(Clash::KeepBoth) => Ok(folder.join(keep_both_name(name, taken))),
        None => Err(DownloadError::Exists {
            name: name.to_string(),
        }),
    }
}

/// A downloaded file waiting, whole, in a hidden temporary file; dropped without
/// [`Downloaded::place`], it's deleted.
#[derive(Debug)]
pub struct Downloaded {
    temp: PathBuf,
    pub bytes: u64,
}

impl Downloaded {
    /// Moves the file to `to` (in the folder it was downloaded into), replacing what's there.
    pub fn place(self, to: &Path) -> Result<(), DownloadError> {
        std::fs::rename(&self.temp, to).map_err(|e| DownloadError::Local {
            name: to
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            reason: e.to_string(),
        })
        // On failure, dropping `self` removes the temporary file.
    }
}

impl Drop for Downloaded {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temp);
    }
}

/// Writes to the temporary file, reporting progress and stopping when told to.
struct Counting<'a, W> {
    inner: W,
    done: u64,
    progress: &'a mut dyn FnMut(u64) -> bool,
    cancelled: bool,
    failed: Option<io::Error>,
}

impl<W: Write> Write for Counting<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancelled || !(self.progress)(self.done) {
            self.cancelled = true;
            return Err(io::Error::other("cancelled"));
        }
        match self.inner.write(buf) {
            Ok(n) => {
                self.done += n as u64;
                Ok(n)
            }
            Err(e) => {
                self.failed = Some(io::Error::new(e.kind(), e.to_string()));
                Err(e)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Downloads `name` from the FPP's `folder` (sequences or music) into a temporary file in
/// `into`, calling `progress` with the bytes so far; it returns `false` to cancel. Once every
/// byte is in, [`Downloaded::place`] puts it where it goes. On cancel or any failure nothing is
/// left in `into`.
pub fn download(
    http: &dyn Http,
    host: &str,
    folder: FppFolder,
    name: &str,
    into: &Path,
    expected: Option<u64>,
    progress: &mut dyn FnMut(u64) -> bool,
) -> Result<Downloaded, DownloadError> {
    let folder = match folder {
        FppFolder::Sequences => "sequences",
        FppFolder::Music => "music",
        // Playlists aren't files to download; nothing else is ever asked for.
        FppFolder::Playlists => {
            return Err(DownloadError::BadName {
                name: name.to_string(),
            });
        }
    };
    if !is_plain_name(name) {
        return Err(DownloadError::BadName {
            name: name.to_string(),
        });
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = into.join(format!(
        ".{name}.{}-{}.pfdownload",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let local = |e: io::Error| DownloadError::Local {
        name: name.to_string(),
        reason: e.to_string(),
    };
    let file = File::create(&temp).map_err(local)?;
    // From here on, the temporary file goes again however this ends.
    let mut downloaded = Downloaded { temp, bytes: 0 };
    let mut sink = Counting {
        inner: BufWriter::with_capacity(256 * 1024, file),
        done: 0,
        progress,
        cancelled: false,
        failed: None,
    };
    let path = format!("/api/file/{folder}/{}", encode_segment(name));
    let result = http.get_to(host, &path, &mut sink, expected);
    if sink.cancelled {
        return Err(DownloadError::Cancelled);
    }
    if let Some(e) = sink.failed.take() {
        return Err(local(e));
    }
    let bytes = result.map_err(|e| DownloadError::from_device(name, e))?;
    let file = sink
        .inner
        .into_inner()
        .map_err(|e| local(io::Error::new(e.error().kind(), e.error().to_string())))?;
    file.sync_all().map_err(local)?;
    (sink.progress)(bytes);
    downloaded.bytes = bytes;
    Ok(downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_names_come_from_the_end_of_the_mf_path() {
        assert_eq!(media_file_name("/Shows/Audio/Song.mp3"), Some("Song.mp3"));
        assert_eq!(
            media_file_name(r"C:\Users\Me\xLights\Wizards.mp3"),
            Some("Wizards.mp3")
        );
        assert_eq!(media_file_name("Plain.ogg"), Some("Plain.ogg"));
        assert_eq!(media_file_name("/Shows/Audio/"), None);
        assert_eq!(media_file_name(""), None);
    }

    #[test]
    fn music_matches_by_name_then_whatever_the_capitals() {
        let music = vec![
            "song.MP3".to_string(),
            "Song.mp3".to_string(),
            "Other.mp3".to_string(),
        ];
        assert_eq!(
            match_music(r"D:\a\Song.mp3", &music).map(String::as_str),
            Some("Song.mp3")
        );
        assert_eq!(
            match_music("/x/other.MP3", &music).map(String::as_str),
            Some("Other.mp3")
        );
        assert_eq!(match_music("/x/Missing.mp3", &music), None);
    }

    #[test]
    fn only_plain_names_are_downloaded() {
        assert!(is_plain_name("Christmas Medley 2017.fseq"));
        assert!(is_plain_name("Song (2).mp3"));
        for bad in [
            "",
            ".hidden",
            "../settings",
            "a/b.fseq",
            r"a\b.fseq",
            "x..fseq",
            " lead.fseq",
            "c:x",
        ] {
            assert!(!is_plain_name(bad), "{bad}");
        }
    }

    #[test]
    fn clashes_replace_keep_both_or_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        assert_eq!(
            target_path(folder, "A.fseq", None).unwrap(),
            folder.join("A.fseq")
        );
        std::fs::write(folder.join("A.fseq"), b"old").unwrap();
        std::fs::write(folder.join("A (2).fseq"), b"old").unwrap();
        assert_eq!(
            target_path(folder, "A.fseq", None),
            Err(DownloadError::Exists {
                name: "A.fseq".into()
            })
        );
        assert_eq!(
            target_path(folder, "A.fseq", Some(Clash::Replace)).unwrap(),
            folder.join("A.fseq")
        );
        assert_eq!(
            target_path(folder, "A.fseq", Some(Clash::KeepBoth)).unwrap(),
            folder.join("A (3).fseq")
        );
        // A choice made for a file that has since gone: saved under its own name.
        assert_eq!(
            target_path(folder, "B.fseq", Some(Clash::KeepBoth)).unwrap(),
            folder.join("B.fseq")
        );
        assert!(matches!(
            target_path(folder, "../B.fseq", Some(Clash::Replace)),
            Err(DownloadError::BadName { .. })
        ));
    }
}
