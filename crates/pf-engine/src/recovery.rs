//! Unsaved sequence work kept on disk, so a crash or a quit without saving doesn't lose it.
//!
//! Each run of the app writes its open, unsaved sequence as `<session>.pfseq.json` (a normal
//! sequence file) plus `<session>.recovery.json` (where it came from and when) in the history
//! folder. The files go away when the sequence is saved, closed, or replaced, so whatever is left
//! from an earlier run is work that was never saved: [`list`] finds it to offer it back.

use crate::error::EngineError;
use crate::persist::write_atomic;
use crate::sequence_doc::{load_sequence, write_sequence};
use pf_sequence::Sequence;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DOC_SUFFIX: &str = ".pfseq.json";
const META_SUFFIX: &str = ".recovery.json";

static SESSIONS: AtomicU64 = AtomicU64::new(0);

/// A sequence that wasn't saved when PixelFlow last closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceRecovery {
    /// Pass back to recover or discard it.
    pub id: String,
    /// The sequence's name.
    pub name: String,
    /// The file it was opened from or last saved to (null for a sequence never saved).
    pub path: Option<String>,
    /// When it was last kept (milliseconds since 1970).
    pub saved_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    name: String,
    path: Option<String>,
    saved_at_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A name for this run's files, different from every other run's (and every other engine's).
pub(crate) fn new_session() -> String {
    format!(
        "{}-{}-{}",
        now_ms(),
        std::process::id(),
        SESSIONS.fetch_add(1, Ordering::Relaxed)
    )
}

/// The folder kept sequences go in.
pub(crate) fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("history").join("sequences")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Keeps `doc` (opened from or saved to `path`, if anywhere) as `session`'s unsaved work.
pub(crate) fn write(
    dir: &Path,
    session: &str,
    doc: &Sequence,
    path: Option<&Path>,
) -> Result<(), EngineError> {
    write_sequence(&dir.join(format!("{session}{DOC_SUFFIX}")), doc)?;
    let meta = Meta {
        name: doc.name.clone(),
        path: path.map(pf_model::path_to_text),
        saved_at_ms: now_ms(),
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| EngineError::Write {
        path: dir.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    write_atomic(&dir.join(format!("{session}{META_SUFFIX}")), &json)
}

/// Forgets `id`'s kept work (nothing happens if there is none).
pub(crate) fn remove(dir: &Path, id: &str) {
    if !valid_id(id) {
        return;
    }
    let _ = fs::remove_file(dir.join(format!("{id}{META_SUFFIX}")));
    let _ = fs::remove_file(dir.join(format!("{id}{DOC_SUFFIX}")));
}

/// Kept work from every run but `except`, newest first.
pub(crate) fn list(dir: &Path, except: &str) -> Vec<SequenceRecovery> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<SequenceRecovery> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let id = file.strip_suffix(META_SUFFIX)?.to_string();
            if id == except || !valid_id(&id) || !dir.join(format!("{id}{DOC_SUFFIX}")).exists() {
                return None;
            }
            let meta: Meta = serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
            Some(SequenceRecovery {
                id,
                name: meta.name,
                path: meta.path,
                saved_at_ms: meta.saved_at_ms,
            })
        })
        .collect();
    found.sort_by_key(|r| std::cmp::Reverse(r.saved_at_ms));
    found
}

/// Reads `id`'s kept sequence and where it came from.
pub(crate) fn load(dir: &Path, id: &str, except: &str) -> Result<(Sequence, Option<PathBuf>), EngineError> {
    let recovery = list(dir, except)
        .into_iter()
        .find(|r| r.id == id)
        .ok_or(EngineError::UnknownRecovery)?;
    let doc = load_sequence(&dir.join(format!("{id}{DOC_SUFFIX}")))?;
    Ok((doc, recovery.path.as_deref().map(pf_model::path_from_text)))
}
