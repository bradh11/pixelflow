//! What was found for a song, kept by the song file's SHA-256 in the app's cache folder, so
//! finding its lyrics again asks no one: `lyrics/<hash>-lrclib.json` (the published entry) and
//! `lyrics/<hash>-openai.json` (what the recognizer heard). A cache that can't be read or
//! written is skipped.

use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The SHA-256 of a file's bytes, in hex, giving up when `stop` says so.
pub fn file_hash(path: &Path, stop: &dyn Fn() -> bool) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        if stop() {
            return None;
        }
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The lyrics cache folder.
#[derive(Debug, Clone)]
pub struct LyricsCache {
    dir: PathBuf,
}

impl LyricsCache {
    /// Kept in `lyrics/` under `cache_dir`.
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            dir: cache_dir.join("lyrics"),
        }
    }

    fn path(&self, hash: &str, kind: &str) -> PathBuf {
        self.dir.join(format!("{hash}-{kind}.json"))
    }

    pub fn load<T: DeserializeOwned>(&self, hash: &str, kind: &str) -> Option<T> {
        let text = std::fs::read_to_string(self.path(hash, kind)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn store<T: Serialize>(&self, hash: &str, kind: &str, value: &T) {
        let Ok(text) = serde_json::to_vec(value) else {
            return;
        };
        if std::fs::create_dir_all(&self.dir).is_ok() {
            let _ = pf_engine::write_atomic(&self.path(hash, kind), &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_known_by_their_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        std::fs::write(&a, b"abc").unwrap();
        assert_eq!(
            file_hash(&a, &|| false).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(file_hash(&a, &|| true), None);
        assert_eq!(file_hash(&dir.path().join("missing.mp3"), &|| false), None);
    }

    #[test]
    fn values_come_back_by_hash_and_kind() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LyricsCache::new(dir.path());
        assert_eq!(cache.load::<Vec<u32>>("abc", "openai"), None);
        cache.store("abc", "openai", &vec![1u32, 2, 3]);
        assert_eq!(cache.load::<Vec<u32>>("abc", "openai"), Some(vec![1, 2, 3]));
        assert_eq!(cache.load::<Vec<u32>>("abc", "lrclib"), None);
        assert!(dir.path().join("lyrics/abc-openai.json").exists());
    }
}
