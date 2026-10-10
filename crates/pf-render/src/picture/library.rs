//! The pictures a renderer draws: read and decoded once, kept at about the size they're drawn.
//!
//! A [`Pictures`] is shared by every renderer of a show. The renderer never reads the disk: a
//! picture it hasn't got is read by whoever made the library (see [`Pictures::new`]), on a
//! thread of its own, and the effect draws nothing until it's there. Exports wait for it
//! instead ([`Pictures::waiting`]), so a file always has its pictures.
//!
//! Two things are kept, each within a budget, the least recently drawn dropped first:
//!
//! - **a picture's frames**, by the file's contents, how far they were shrunk on reading, and how
//!   (crisp, black made clear): at most about twice the size the effect draws them, so a long
//!   GIF on a small matrix takes a few megabytes rather than hundreds;
//! - **a frame at the exact size it's drawn**, by picture, frame, size, and turn, so drawing a
//!   frame again is a lookup.
//!
//! While a preview waits for frames it has asked for (after the fit, the scale, or the look
//! changed), it draws the frames it has of the same file, so the picture doesn't blink out.

use super::decode::{Decoded, Look, MAX_LEVEL, decode};
use super::resample::{Bitmap, resize, turned};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The most memory the pictures' frames take together, and the frames at the size they're drawn.
const FRAMES_BUDGET: usize = 96 * 1024 * 1024;
const DRAWN_BUDGET: usize = 24 * 1024 * 1024;
/// The most one picture's frames take: a quarter of what all of them may.
const ONE_PICTURE_SHARE: usize = 4;
/// The most pictures read at once (the rest are asked for again with a later frame).
const MAX_READING: usize = 4;
/// How often a renderer that waits looks again for a picture another is reading.
const WAIT: std::time::Duration = std::time::Duration::from_millis(2);

/// Reads a picture's file, given what its effect's `file` setting says: the file's bytes, or why
/// it can't be read (in a few plain words, to follow "couldn't be read: ").
pub type ReadPicture = dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync;

/// What a picture is wanted for: the grid it's drawn on, and how it's fitted to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Want {
    pub columns: u32,
    pub rows: u32,
    pub need: Need,
    /// How many times larger than its fit it's drawn, as doublings.
    pub doublings: u8,
    pub look: Look,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Need {
    /// All of it inside the grid.
    Fit,
    /// Covering the grid.
    Fill,
    /// Its own pixels.
    Full,
}

impl Want {
    /// How many times a picture `native` pixels in size can be halved and still be at least the
    /// size it's drawn.
    pub fn level(&self, native: (u32, u32)) -> u8 {
        let across = f64::from(self.columns.max(1)) / f64::from(native.0.max(1));
        let down = f64::from(self.rows.max(1)) / f64::from(native.1.max(1));
        let scale = match self.need {
            Need::Fit => across.min(down),
            Need::Fill => across.max(down),
            Need::Full => 1.0,
        } * f64::from(1u32 << self.doublings.min(8));
        if scale >= 1.0 {
            0
        } else {
            ((1.0 / scale).log2().floor() as u8).min(MAX_LEVEL)
        }
    }
}

/// A picture's frames, as kept.
#[derive(Debug)]
pub(crate) struct Frames {
    /// Tells one set of frames from another.
    id: u64,
    pub native: (u32, u32),
    pub frames: Vec<Bitmap>,
    /// How long each frame shows, in milliseconds.
    pub delays: Vec<u32>,
}

impl Frames {
    fn bytes(&self) -> usize {
        self.frames.iter().map(Bitmap::bytes).sum()
    }
}

/// A frame at the size it's drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Drawn {
    pub frame: u32,
    pub width: u32,
    pub height: u32,
    /// Quarter turns to the right.
    pub quarters: u8,
    pub crisp: bool,
}

/// Things kept within a budget of bytes: the least recently used go first.
struct Kept<K, V> {
    budget: usize,
    bytes: usize,
    clock: u64,
    items: HashMap<K, (V, usize, u64)>,
}

impl<K: Hash + Eq + Clone, V: Clone> Kept<K, V> {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            clock: 0,
            items: HashMap::new(),
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        self.clock += 1;
        let (value, _, used) = self.items.get_mut(key)?;
        *used = self.clock;
        Some(value.clone())
    }

    /// Keeps `value`, dropping the least recently used until everything fits. Something larger
    /// than the whole budget isn't kept.
    fn put(&mut self, key: K, value: V, bytes: usize) {
        if let Some((_, old, _)) = self.items.remove(&key) {
            self.bytes -= old;
        }
        if bytes > self.budget {
            return;
        }
        self.clock += 1;
        self.items.insert(key, (value, bytes, self.clock));
        self.bytes += bytes;
        while self.bytes > self.budget {
            let Some(oldest) = self
                .items
                .iter()
                .min_by_key(|(_, (_, _, used))| *used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((_, old, _)) = self.items.remove(&oldest) {
                self.bytes -= old;
            }
        }
    }

    fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
}

/// What's known about a picture file once it has been read.
#[derive(Debug, Clone)]
enum File {
    /// Its contents (hashed) and size.
    Read { contents: u64, native: (u32, u32) },
    /// Why it couldn't be read.
    Failed(String),
}

struct State {
    files: HashMap<String, File>,
    frames: Kept<(u64, u8, Look), Arc<Frames>>,
    drawn: Kept<(u64, Drawn), Arc<Bitmap>>,
    /// The pictures being read now.
    reading: HashSet<(String, Want)>,
}

struct Library {
    read: Box<ReadPicture>,
    state: Mutex<State>,
    /// Told whenever a picture read in the background is there (or couldn't be read).
    arrived: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// Whether a sequence's pictures are being read ahead (one sequence's worth at a time).
    ahead: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Library {
    /// Reads and decodes `file` for `want`, and keeps it. This is the slow part.
    fn fetch(&self, file: &str, want: Want) -> Option<Arc<Frames>> {
        let one = lock(&self.state).frames.budget / ONE_PICTURE_SHARE;
        let read = (self.read)(file).and_then(|bytes| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            // A file made to trip its decoder up is a picture that can't be read, nothing worse.
            let decoding = || decode(&bytes, |native| want.level(native), want.look, one);
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(decoding))
                .unwrap_or_else(|_| Err("it's damaged, or isn't a picture".to_string()))?;
            Ok((hasher.finish(), decoded))
        });
        let mut state = lock(&self.state);
        match read {
            Ok((contents, decoded)) => {
                let Decoded {
                    native,
                    frames,
                    delays,
                    ..
                } = decoded;
                let frames = Arc::new(Frames {
                    id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                    native,
                    frames,
                    delays,
                });
                state
                    .files
                    .insert(file.to_string(), File::Read { contents, native });
                let bytes = frames.bytes();
                let key = (contents, want.level(native), want.look);
                state.frames.put(key, Arc::clone(&frames), bytes);
                Some(frames)
            }
            Err(why) => {
                state.files.insert(file.to_string(), File::Failed(why));
                None
            }
        }
    }
}

/// A picture being read: no one else reads it meanwhile, however the reading ends.
struct Reading {
    library: Arc<Library>,
    key: (String, Want),
}

impl Drop for Reading {
    fn drop(&mut self) {
        lock(&self.library.state).reading.remove(&self.key);
    }
}

/// The pictures effects draw: none, or a library shared by every renderer of a show.
#[derive(Clone, Default)]
pub struct Pictures {
    library: Option<Arc<Library>>,
    /// Whether a frame waits for a picture it hasn't got (exports) rather than drawing without it
    /// while it's read (previews).
    wait: bool,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match (&self.library, self.wait) {
            (None, _) => "Pictures(none)",
            (Some(_), false) => "Pictures(shared)",
            (Some(_), true) => "Pictures(shared, waiting)",
        })
    }
}

/// Two are equal when they're the same library, read the same way ([`Pictures::same`]).
impl PartialEq for Pictures {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}

impl Pictures {
    /// No pictures: Picture effects draw nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// Pictures read with `read`, on a thread of their own the first time each is drawn.
    pub fn new(read: impl Fn(&str) -> Result<Vec<u8>, String> + Send + Sync + 'static) -> Self {
        Self::with_budget(read, FRAMES_BUDGET)
    }

    /// [`Pictures::new`] keeping at most `bytes` of frames (and a quarter as much again of frames
    /// at the size they're drawn).
    pub fn with_budget(
        read: impl Fn(&str) -> Result<Vec<u8>, String> + Send + Sync + 'static,
        bytes: usize,
    ) -> Self {
        let drawn = if bytes == FRAMES_BUDGET {
            DRAWN_BUDGET
        } else {
            bytes / 4
        };
        Self {
            library: Some(Arc::new(Library {
                read: Box::new(read),
                state: Mutex::new(State {
                    files: HashMap::new(),
                    frames: Kept::new(bytes),
                    drawn: Kept::new(drawn),
                    reading: HashSet::new(),
                }),
                arrived: Mutex::new(None),
                ahead: AtomicBool::new(false),
            })),
            wait: false,
        }
    }

    /// The same pictures for a renderer that waits for each one it draws (an export, which must
    /// not leave any out).
    pub fn waiting(&self) -> Self {
        Self {
            library: self.library.clone(),
            wait: true,
        }
    }

    /// Whether the two are the same library, read the same way.
    pub fn same(&self, other: &Pictures) -> bool {
        self.wait == other.wait
            && match (&self.library, &other.library) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }

    /// Tells `told` whenever a picture read in the background is there or couldn't be read, so
    /// whoever shows frames can draw again (`None`: no one).
    pub fn on_arrival(&self, told: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Some(library) = &self.library {
            *lock(&library.arrived) = told;
        }
    }

    /// Why `file` couldn't be read, if it was tried and couldn't.
    pub fn problem(&self, file: &str) -> Option<String> {
        let library = self.library.as_ref()?;
        match lock(&library.state).files.get(file)? {
            File::Failed(why) => Some(why.clone()),
            File::Read { .. } => None,
        }
    }

    /// The size of `file`'s picture in its own pixels, once it has been read.
    pub fn size(&self, file: &str) -> Option<(u32, u32)> {
        let library = self.library.as_ref()?;
        match lock(&library.state).files.get(file)? {
            File::Read { native, .. } => Some(*native),
            File::Failed(_) => None,
        }
    }

    /// Forgets what's known about `file`, so it's read again the next time it's drawn: for a file
    /// that changed, appeared, or went away.
    pub fn forget(&self, file: &str) {
        if let Some(library) = &self.library {
            lock(&library.state).files.remove(file);
        }
    }

    /// Forgets every picture (another show's files are other files).
    pub fn clear(&self) {
        if let Some(library) = &self.library {
            let mut state = lock(&library.state);
            state.files.clear();
            state.frames.clear();
            state.drawn.clear();
        }
    }

    /// The bytes of frames kept now.
    pub fn kept_bytes(&self) -> usize {
        self.library.as_ref().map_or(0, |library| {
            let state = lock(&library.state);
            state.frames.bytes + state.drawn.bytes
        })
    }

    /// `file`'s frames for `want`: `None` while they're read (unless this waits), and when the
    /// file can't be read.
    pub(crate) fn frames(&self, file: &str, want: Want) -> Option<Arc<Frames>> {
        let library = self.library.as_ref()?;
        if file.trim().is_empty() {
            return None;
        }
        loop {
            let mut state = lock(&library.state);
            let mut meanwhile = None;
            match state.files.get(file) {
                Some(File::Failed(_)) => return None,
                Some(&File::Read { contents, native }) => {
                    let key = (contents, want.level(native), want.look);
                    if let Some(frames) = state.frames.get(&key) {
                        return Some(frames);
                    }
                    // The frames it has of the same file, the sharpest first, until these come.
                    if !self.wait {
                        meanwhile = state
                            .frames
                            .items
                            .iter()
                            .filter(|(key, _)| key.0 == contents)
                            .min_by_key(|(key, _)| (key.2 != want.look, key.1))
                            .map(|(_, (frames, ..))| Arc::clone(frames));
                    }
                }
                None => {}
            }
            // Someone is reading it already: wait for them, or draw without it meanwhile.
            let key = (file.to_string(), want);
            if state.reading.contains(&key) {
                if !self.wait {
                    return meanwhile;
                }
                drop(state);
                std::thread::sleep(WAIT);
                continue;
            }
            if !self.wait && state.reading.len() >= MAX_READING {
                return meanwhile;
            }
            state.reading.insert(key.clone());
            drop(state);
            let reading = Reading {
                library: Arc::clone(library),
                key,
            };
            if self.wait {
                return library.fetch(file, want);
            }
            // No thread to read it on: it's asked for again with the next frame.
            let _ = std::thread::Builder::new().name("picture".into()).spawn(move || {
                reading.library.fetch(&reading.key.0, reading.key.1);
                let told = lock(&reading.library.arrived).clone();
                drop(reading);
                if let Some(told) = told {
                    told();
                }
            });
            return meanwhile;
        }
    }

    /// Reads the pictures in `wanted` that aren't here yet, one after another on a thread of
    /// their own, so effects later in a sequence find theirs ready. Whoever shows frames is told
    /// once they're all there. While one lot is being read, another isn't started (the answer is
    /// then `false`): ask again for what's still wanted.
    pub(crate) fn read_ahead(&self, wanted: Vec<(String, Want)>) -> bool {
        let Some(library) = &self.library else {
            return true;
        };
        let wanted: Vec<(String, Want)> = {
            let mut state = lock(&library.state);
            let mut missing: Vec<(String, Want)> = Vec::new();
            for (file, want) in wanted {
                let here = match state.files.get(&file) {
                    Some(File::Failed(_)) => true,
                    Some(&File::Read { contents, native }) => state
                        .frames
                        .get(&(contents, want.level(native), want.look))
                        .is_some(),
                    None => false,
                };
                if !here && !file.trim().is_empty() && !missing.iter().any(|m| m.0 == file && m.1 == want) {
                    missing.push((file, want));
                }
            }
            missing
        };
        if wanted.is_empty() {
            return true;
        }
        if library.ahead.swap(true, Ordering::AcqRel) {
            return false;
        }
        let waiting = self.waiting();
        let shared = Arc::clone(library);
        let work = move || {
            for (file, want) in wanted {
                waiting.frames(&file, want);
            }
            shared.ahead.store(false, Ordering::Release);
            let told = lock(&shared.arrived).clone();
            if let Some(told) = told {
                told();
            }
        };
        if std::thread::Builder::new()
            .name("pictures".into())
            .spawn(work)
            .is_err()
        {
            // No thread to read them on: each is read when it's first drawn instead.
            library.ahead.store(false, Ordering::Release);
        }
        true
    }

    /// One of `frames`' frames at the size and turn it's drawn.
    pub(crate) fn drawn(&self, frames: &Frames, at: Drawn) -> Option<Arc<Bitmap>> {
        let library = self.library.as_ref()?;
        let key = (frames.id, at);
        if let Some(bitmap) = lock(&library.state).drawn.get(&key) {
            return Some(bitmap);
        }
        let source = frames.frames.get(at.frame as usize)?;
        // Resized before it's turned, so a quarter turn swaps the sides.
        let (width, height) = if at.quarters % 2 == 1 {
            (at.height, at.width)
        } else {
            (at.width, at.height)
        };
        let bitmap = Arc::new(turned(resize(source, width, height, at.crisp), at.quarters));
        let bytes = bitmap.bytes();
        lock(&library.state).drawn.put(key, Arc::clone(&bitmap), bytes);
        Some(bitmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_least_recently_used_go_first_and_the_budget_holds() {
        let mut kept: Kept<u32, &str> = Kept::new(100);
        kept.put(1, "a", 40);
        kept.put(2, "b", 40);
        assert_eq!(kept.get(&1), Some("a"));
        // A third doesn't fit: the one not used since goes.
        kept.put(3, "c", 40);
        assert_eq!(
            (kept.get(&2), kept.get(&1), kept.get(&3)),
            (None, Some("a"), Some("c"))
        );
        assert_eq!(kept.bytes, 80);
        // Putting the same thing again replaces it rather than counting twice.
        kept.put(3, "c2", 60);
        assert_eq!((kept.get(&3), kept.bytes), (Some("c2"), 100));
        // Something larger than the budget isn't kept, and pushes nothing out.
        kept.put(4, "huge", 101);
        assert_eq!((kept.get(&4), kept.bytes, kept.items.len()), (None, 100, 2));
        kept.clear();
        assert_eq!((kept.bytes, kept.items.len()), (0, 0));
    }

    #[test]
    fn a_picture_is_halved_until_it_is_about_the_size_it_is_drawn() {
        let want = |need, doublings| Want {
            columns: 64,
            rows: 32,
            need,
            doublings,
            look: Look::default(),
        };
        // 500 × 500 on 64 × 32: fitted it's 32 tall (1/15.6), so three halvings leave 63 × 63.
        assert_eq!(want(Need::Fit, 0).level((500, 500)), 3);
        // Filling, it's 64 across (1/7.8): two halvings leave 125 × 125.
        assert_eq!(want(Need::Fill, 0).level((500, 500)), 2);
        // Its own pixels are never halved, nor is a picture smaller than the grid.
        assert_eq!(want(Need::Full, 0).level((500, 500)), 0);
        assert_eq!(want(Need::Fit, 0).level((16, 16)), 0);
        // Drawn twice as large, it's halved once less.
        assert_eq!(want(Need::Fit, 1).level((500, 500)), 2);
        assert_eq!(want(Need::Fit, 8).level((500, 500)), 0);
        // Nothing divides by nothing.
        assert_eq!(want(Need::Fit, 0).level((0, 0)), 0);
    }
}
