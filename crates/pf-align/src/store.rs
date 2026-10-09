//! Where the alignment model lives: downloaded once, only after the user agrees, from where its
//! makers publish it (a fixed revision on Hugging Face), checked against its SHA-256 before it's
//! kept, in the app's data folder (`models/<id>/`). Nothing is bundled with the app or kept in
//! the repository. "Remove models" deletes the folder.

use crate::AlignError;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One file of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Its name in the model's folder.
    pub name: &'static str,
    pub url: &'static str,
    /// Its SHA-256, lowercase hex.
    pub sha256: &'static str,
    pub bytes: u64,
}

/// A model to download: what it is, under what licence, and its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest {
    /// Its folder's name, and part of what alignments made with it are kept under: a new
    /// revision gets a new id.
    pub id: &'static str,
    pub name: &'static str,
    pub licence: &'static str,
    /// The page that describes it.
    pub source: &'static str,
    pub files: &'static [ModelFile],
}

impl Manifest {
    /// Its files' size together.
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

/// wav2vec2-base-960h (Facebook AI, Apache-2.0), as converted to ONNX by the ONNX Community
/// (16-bit weights), at a fixed revision.
pub const WAV2VEC2: Manifest = Manifest {
    id: "wav2vec2-base-960h-fp16-729c1a6",
    name: "wav2vec2-base-960h (English letters)",
    licence: "Apache-2.0",
    source: "https://huggingface.co/onnx-community/wav2vec2-base-960h-ONNX",
    files: &[ModelFile {
        name: "model_fp16.onnx",
        url: "https://huggingface.co/onnx-community/wav2vec2-base-960h-ONNX/resolve/729c1a6730fb549c20a1c73a3d3f96f11020225e/onnx/model_fp16.onnx",
        sha256: "a659290401309a0a1dff3ea59d8ff8d534c92bf098ce9d08cd1f28f5650525ca",
        bytes: 189_118_943,
    }],
};

/// Bytes read between checks for a stop and reports of progress.
const BLOCK: usize = 1 << 16;

/// A model's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStore {
    dir: PathBuf,
    manifest: Manifest,
}

/// Lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl ModelStore {
    /// `manifest`'s model, in `models/<id>/` under `data_dir`.
    pub fn new(data_dir: &Path, manifest: Manifest) -> Self {
        Self {
            dir: data_dir.join("models").join(manifest.id),
            manifest,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where `file` is kept.
    pub fn path(&self, file: &ModelFile) -> PathBuf {
        self.dir.join(file.name)
    }

    /// The model's main file (its first).
    pub fn model_path(&self) -> Option<PathBuf> {
        self.manifest.files.first().map(|f| self.path(f))
    }

    /// Whether every file is here, at its size (each was checked when it was kept).
    pub fn is_installed(&self) -> bool {
        self.manifest
            .files
            .iter()
            .all(|f| fs::metadata(self.path(f)).is_ok_and(|m| m.is_file() && m.len() == f.bytes))
    }

    /// Keeps `file` from `source`: written beside it first, its SHA-256 checked, then put in
    /// place. Tells `progress` the bytes so far; gives up when `stop` says so, keeping nothing.
    pub fn install_from(
        &self,
        file: &ModelFile,
        source: &mut dyn Read,
        progress: &dyn Fn(u64),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), AlignError> {
        fs::create_dir_all(&self.dir)?;
        let part = self.dir.join(format!("{}.part", file.name));
        let result = (|| {
            let mut out = fs::File::create(&part)?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; BLOCK];
            let mut total = 0u64;
            loop {
                if stop() {
                    return Err(AlignError::Cancelled);
                }
                let n = match source.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(AlignError::Download(e.to_string())),
                };
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
                total += n as u64;
                progress(total);
            }
            out.sync_all()?;
            let found = hex(&hasher.finalize());
            if found != file.sha256 || total != file.bytes {
                return Err(AlignError::Checksum {
                    expected: file.sha256.to_string(),
                    found,
                });
            }
            fs::rename(&part, self.path(file))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&part);
        }
        result
    }

    /// Downloads every file not already here, telling `progress` the bytes so far of the
    /// model's [`Manifest::bytes`].
    pub fn download(&self, progress: &dyn Fn(u64, u64), stop: &dyn Fn() -> bool) -> Result<(), AlignError> {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
            .http_status_as_error(false)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let total = self.manifest.bytes();
        let mut before = 0;
        for file in self.manifest.files {
            let kept = fs::metadata(self.path(file)).is_ok_and(|m| m.len() == file.bytes);
            if !kept {
                let response = agent
                    .get(file.url)
                    .call()
                    .map_err(|e| AlignError::Download(e.to_string()))?;
                let status = response.status().as_u16();
                if status != 200 {
                    return Err(AlignError::Download(format!("HTTP {status}")));
                }
                let mut body = response
                    .into_body()
                    .into_with_config()
                    .limit(file.bytes + 1)
                    .reader();
                self.install_from(file, &mut body, &|n| progress(before + n, total), stop)?;
            }
            before += file.bytes;
            progress(before, total);
        }
        Ok(())
    }

    /// Deletes the model's folder (and anything half-downloaded in it).
    pub fn remove(&self) -> Result<(), AlignError> {
        match fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: ModelFile = ModelFile {
        name: "hello.onnx",
        url: "https://example.invalid/hello.onnx",
        // SHA-256 of "hello".
        sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        bytes: 5,
    };
    const MANIFEST: Manifest = Manifest {
        id: "test-model-1",
        name: "Test",
        licence: "MIT",
        source: "https://example.invalid",
        files: &[HELLO],
    };

    #[test]
    fn a_file_is_kept_only_when_its_checksum_matches() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path(), MANIFEST);
        assert!(!store.is_installed());
        // The wrong bytes: refused, nothing left behind.
        let error = store
            .install_from(&HELLO, &mut "jello".as_bytes(), &|_| {}, &|| false)
            .unwrap_err();
        assert!(matches!(error, AlignError::Checksum { .. }), "{error}");
        assert!(!store.is_installed());
        assert_eq!(fs::read_dir(store.dir()).unwrap().count(), 0);
        // The right ones.
        let seen = std::cell::Cell::new(0);
        store
            .install_from(&HELLO, &mut "hello".as_bytes(), &|n| seen.set(n), &|| false)
            .unwrap();
        assert_eq!(seen.get(), 5);
        assert!(store.is_installed());
        assert_eq!(
            store.model_path().unwrap(),
            dir.path().join("models/test-model-1/hello.onnx")
        );
        // Removed: gone, and removing again is fine.
        store.remove().unwrap();
        assert!(!store.is_installed());
        store.remove().unwrap();
    }

    #[test]
    fn a_stopped_download_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path(), MANIFEST);
        let error = store
            .install_from(&HELLO, &mut "hello".as_bytes(), &|_| {}, &|| true)
            .unwrap_err();
        assert!(matches!(error, AlignError::Cancelled));
        assert!(!store.is_installed());
        assert!(!store.dir().join("hello.onnx.part").exists());
    }

    #[test]
    fn the_real_manifest_is_pinned_and_sized() {
        assert_eq!(WAV2VEC2.bytes(), 189_118_943);
        for f in WAV2VEC2.files {
            assert_eq!(f.sha256.len(), 64);
            assert!(f.url.starts_with("https://huggingface.co/") && f.url.contains("/resolve/"));
            // A fixed revision, not "main".
            assert!(!f.url.contains("/resolve/main/"));
        }
        assert!(WAV2VEC2.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
