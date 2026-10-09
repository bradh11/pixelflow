//! The open sequence's music as an audio track for the renderer (see `pf_render::audio`).
//!
//! A track is worked out once per music file (by its contents) and frame time, in the
//! background, and kept in memory and, when the engine has a cache folder, on disk, so playing,
//! scrubbing, and exporting never work it out again. Renderers get an [`AudioSource`] at once:
//! effects draw as in silence until the track is there, then follow the music. Exports wait for
//! it ([`AudioTracks::track`]). How far a track has got is passed to whoever asked to be told
//! ([`AudioTracks::set_progress`]).

use pf_analysis::{AudioTrack, TRACK_FORMAT, audio_track_file};
use pf_render::{AudioFill, AudioSource};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

/// Cache files kept on disk (the oldest go first).
const MAX_CACHED_FILES: usize = 48;
/// Music files whose sources are remembered.
const MAX_REQUESTS: usize = 16;

/// A music file as it was when its track was asked for (another file at the same path, or the
/// same one changed, is asked for again).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Request {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    frame_ms: u32,
}

impl Request {
    fn of(path: &Path, frame_ms: u32) -> Option<Self> {
        let meta = fs::metadata(path).ok().filter(|m| m.is_file())?;
        Some(Self {
            path: path.to_path_buf(),
            len: meta.len(),
            modified: meta.modified().ok(),
            frame_ms: frame_ms.max(1),
        })
    }
}

/// Told how far working out a track has got: the music file and a fraction (0–1). Each track
/// worked out ends with 1, even one that can't be.
pub type TrackProgress = dyn Fn(&Path, f32) + Send + Sync;

/// Who's told how far tracks have got.
#[derive(Default)]
struct Reporter(Option<Arc<TrackProgress>>);

impl std::fmt::Debug for Reporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Reporter(set)"
        } else {
            "Reporter(none)"
        })
    }
}

/// Audio tracks by music file, worked out once and shared.
#[derive(Debug, Default)]
pub struct AudioTracks {
    /// Where tracks are kept on disk (none: in memory only).
    dir: Mutex<Option<PathBuf>>,
    /// Tracks worked out, by the music's contents and the frame time.
    memory: Mutex<HashMap<(String, u32), Arc<AudioTrack>>>,
    /// The source handed out for each music file asked for, oldest first.
    requests: Mutex<Vec<(Request, AudioSource)>>,
    /// Told how far tracks being worked out have got.
    progress: Mutex<Reporter>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The music file's contents as a hash (hex).
fn content_hash(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

impl AudioTracks {
    /// Keeps tracks on disk in `dir` from now on (`None`: memory only).
    pub fn set_dir(&self, dir: Option<PathBuf>) {
        *lock(&self.dir) = dir;
    }

    /// Tells `report` how far each track worked out from now on has got (`None`: no one).
    pub fn set_progress(&self, report: Option<Arc<TrackProgress>>) {
        *lock(&self.progress) = Reporter(report);
    }

    fn report(&self, music: &Path, fraction: f32) {
        let report = lock(&self.progress).0.clone();
        if let Some(report) = report {
            report(music, fraction);
        }
    }

    fn file_for(&self, hash: &str, frame_ms: u32) -> Option<PathBuf> {
        let dir = lock(&self.dir).clone()?;
        Some(dir.join(format!("{hash}-{frame_ms}ms-v{TRACK_FORMAT}.pfaudio")))
    }

    /// The source for `music` at `frame_ms` frames: ready when its track has been worked out,
    /// else filled in the background (none when the file isn't there).
    pub fn source(self: &Arc<Self>, music: &Path, frame_ms: u32) -> AudioSource {
        let Some(request) = Request::of(music, frame_ms) else {
            return AudioSource::none();
        };
        let mut requests = lock(&self.requests);
        if let Some((_, source)) = requests.iter().find(|(r, _)| *r == request) {
            return source.clone();
        }
        let (source, fill) = AudioSource::pending();
        if requests.len() >= MAX_REQUESTS {
            requests.remove(0);
        }
        requests.push((request.clone(), source.clone()));
        drop(requests);
        let tracks = Arc::clone(self);
        let worked_out = std::thread::Builder::new()
            .name("audio track".into())
            .spawn(move || tracks.fill(&request, &fill));
        if worked_out.is_err() {
            // No thread to work it out on: the effects draw as in silence.
            return AudioSource::none();
        }
        source
    }

    fn fill(&self, request: &Request, fill: &AudioFill) {
        if let Some(track) = self.track(&request.path, request.frame_ms) {
            fill.fill(track);
        }
    }

    /// The track for `music` at `frame_ms` frames, worked out now unless it's in memory or on
    /// disk; `None` when the music can't be read.
    pub fn track(&self, music: &Path, frame_ms: u32) -> Option<Arc<AudioTrack>> {
        let frame_ms = frame_ms.max(1);
        let hash = content_hash(music)?;
        let key = (hash.clone(), frame_ms);
        if let Some(track) = lock(&self.memory).get(&key) {
            return Some(Arc::clone(track));
        }
        let file = self.file_for(&hash, frame_ms);
        let cached = file
            .as_ref()
            .and_then(|f| fs::read(f).ok())
            .and_then(|bytes| AudioTrack::from_bytes(&bytes))
            .filter(|t| t.frame_ms() == frame_ms);
        let track = match cached {
            Some(track) => Arc::new(track),
            None => {
                let worked_out = audio_track_file(music, frame_ms, &|| false, &|f| self.report(music, f));
                let Ok(track) = worked_out else {
                    self.report(music, 1.0);
                    return None;
                };
                let track = Arc::new(track);
                if let Some(file) = &file {
                    keep_on_disk(file, &track.to_bytes());
                }
                track
            }
        };
        lock(&self.memory).insert(key, Arc::clone(&track));
        Some(track)
    }
}

/// Writes a cache file (atomically; a failure only means it's worked out again next time), and
/// drops the oldest when there are too many.
fn keep_on_disk(file: &Path, bytes: &[u8]) {
    let Some(dir) = file.parent() else { return };
    if fs::create_dir_all(dir).is_err() || crate::persist::write_atomic(file, bytes).is_err() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut kept: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pfaudio"))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    if kept.len() > MAX_CACHED_FILES {
        kept.sort();
        for (_, old) in &kept[..kept.len() - MAX_CACHED_FILES] {
            let _ = fs::remove_file(old);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short WAV of a 440 Hz tone at `amplitude`.
    fn wav(path: &Path, amplitude: f32) {
        let samples: Vec<f32> = (0..22_050)
            .map(|i| amplitude * (std::f32::consts::TAU * 440.0 * i as f32 / 22_050.0).sin())
            .collect();
        fs::write(path, pf_audio::wav_bytes(&samples, 22_050)).unwrap();
    }

    fn wait(source: &AudioSource) -> Arc<AudioTrack> {
        for _ in 0..500 {
            if let Some(track) = source.track() {
                return Arc::clone(track);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the track never came");
    }

    #[test]
    fn tracks_are_kept_by_contents_and_frame_time_in_memory_and_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (song, copy, other) = (
            dir.path().join("a.wav"),
            dir.path().join("b.wav"),
            dir.path().join("c.wav"),
        );
        wav(&song, 0.5);
        fs::copy(&song, &copy).unwrap();
        wav(&other, 0.25);
        let cache = dir.path().join("cache");
        let tracks = Arc::new(AudioTracks::default());
        tracks.set_dir(Some(cache.clone()));

        // Asked for twice: one source, filled once.
        let source = tracks.source(&song, 25);
        assert!(source.same(&tracks.source(&song, 25)));
        let track = wait(&source);
        assert_eq!((track.frame_ms(), track.len()), (25, 40));
        // The same music under another name is the same track; other music or another frame
        // time isn't.
        assert!(Arc::ptr_eq(&tracks.track(&copy, 25).unwrap(), &track));
        assert!(!Arc::ptr_eq(&tracks.track(&other, 25).unwrap(), &track));
        assert_eq!(tracks.track(&song, 50).unwrap().frame_ms(), 50);
        let files: Vec<_> = fs::read_dir(&cache).unwrap().collect();
        assert_eq!(files.len(), 3, "{files:?}");

        // A new engine run reads it back from disk.
        let again = AudioTracks::default();
        again.set_dir(Some(cache));
        let read = again.track(&song, 25).unwrap();
        assert_eq!(*read, *track);

        // Changed music at the same path is worked out again.
        wav(&song, 0.1);
        let changed = wait(&tracks.source(&song, 25));
        assert_ne!(*changed, *track);

        // No file: no music.
        assert!(!tracks.source(&dir.path().join("gone.wav"), 25).has_music());
    }

    #[test]
    fn working_out_a_track_reports_how_far_it_has_got() {
        let dir = tempfile::tempdir().unwrap();
        let (song, junk) = (dir.path().join("a.wav"), dir.path().join("junk.mp3"));
        wav(&song, 0.5);
        fs::write(&junk, b"not audio").unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let tracks = AudioTracks::default();
        let heard = Arc::clone(&seen);
        tracks.set_progress(Some(Arc::new(move |path: &Path, f: f32| {
            lock(&heard).push((path.to_path_buf(), f));
        })));
        tracks.track(&song, 25).unwrap();
        let reports: Vec<f32> = lock(&seen)
            .iter()
            .map(|(p, f)| {
                assert_eq!(p, &song);
                *f
            })
            .collect();
        assert!(reports.windows(2).all(|w| w[1] > w[0]), "{reports:?}");
        assert_eq!(reports.last(), Some(&1.0));
        // Kept: nothing more to report.
        lock(&seen).clear();
        tracks.track(&song, 25).unwrap();
        assert!(lock(&seen).is_empty());
        // Music that can't be read still ends.
        assert!(tracks.track(&junk, 25).is_none());
        assert_eq!(lock(&seen).last().map(|(_, f)| *f), Some(1.0));
    }
}
