//! What was found for a song, kept by the song file's SHA-256 in the app's cache folder, so
//! finding its lyrics again asks no one: `lyrics/<hash>-lrclib.v2.json` (the published
//! candidates), `lyrics/<hash>-openai.v2.json` (what the recognizer heard, and in which language
//! it was told to hear it), and `lyrics/<hash>-choice.v2.json` (the lyrics the user chose).
//! A cache that can't be read or written is skipped.
//!
//! Files are named with [`VERSION`], so what an earlier version kept (`<hash>-openai.json`,
//! perhaps heard in the wrong language) is never read again; it's left where it is.

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

/// The version in the cache's file names, raised when what's kept changes.
pub const VERSION: u32 = 2;

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
        self.dir.join(format!("{hash}-{kind}.v{VERSION}.json"))
    }

    pub fn load<T: DeserializeOwned>(&self, hash: &str, kind: &str) -> Option<T> {
        let text = std::fs::read_to_string(self.path(hash, kind)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Forgets what's kept for `hash` of `kind`.
    pub fn remove(&self, hash: &str, kind: &str) {
        let _ = std::fs::remove_file(self.path(hash, kind));
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
        assert!(dir.path().join("lyrics/abc-openai.v2.json").exists());
        cache.remove("abc", "openai");
        assert_eq!(cache.load::<Vec<u32>>("abc", "openai"), None);
    }

    #[test]
    fn what_an_earlier_version_kept_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lyrics")).unwrap();
        std::fs::write(dir.path().join("lyrics/abc-openai.json"), "[1,2,3]").unwrap();
        let cache = LyricsCache::new(dir.path());
        assert_eq!(cache.load::<Vec<u32>>("abc", "openai"), None);
        // And it's left alone.
        cache.store("abc", "openai", &vec![4u32]);
        assert!(dir.path().join("lyrics/abc-openai.json").exists());
    }
}
