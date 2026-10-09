//! How far a long job on a music file has got, for a progress bar: a fraction from 0 to 1,
//! taken from how much of the file has been read.

use std::cell::Cell;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The smallest step passed on: at most about a hundred reports per job.
const STEP: f32 = 0.01;

/// Items between looks at how far the file has been read.
const LOOK_EVERY: usize = 1 << 14;

/// Passes a job's progress on to `report`: never going backwards, only in whole steps (about one
/// per percent), and always ending at 1.0 once [`Progress::finish`] is called.
pub struct Progress<'a> {
    report: &'a dyn Fn(f32),
    last: Cell<Option<f32>>,
}

impl<'a> Progress<'a> {
    pub fn new(report: &'a dyn Fn(f32)) -> Self {
        Self {
            report,
            last: Cell::new(None),
        }
    }

    /// The job is `fraction` done (0–1). The first report always goes through, so a bar can
    /// show at once.
    pub fn set(&self, fraction: f32) {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let due = match self.last.get() {
            None => true,
            Some(last) => fraction >= last + STEP || (fraction >= 1.0 && last < 1.0),
        };
        if due {
            self.last.set(Some(fraction));
            (self.report)(fraction);
        }
    }

    /// The job is done.
    pub fn finish(&self) {
        self.set(1.0);
    }
}

impl std::fmt::Debug for Progress<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("last", &self.last.get())
            .finish_non_exhaustive()
    }
}

/// A progress callback that ignores what it's told.
pub fn no_progress(_: f32) {}

/// How far into a music file its decoder has read, shared with whoever wants to know.
#[derive(Debug, Clone)]
pub struct ReadPosition {
    at: Arc<AtomicU64>,
    len: u64,
}

impl ReadPosition {
    /// The part of the file read so far (0–1).
    pub fn fraction(&self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        (self.at.load(Ordering::Relaxed) as f64 / self.len as f64).min(1.0) as f32
    }
}

/// A file that keeps a [`ReadPosition`] up to date as it's read and moved about in.
#[derive(Debug)]
pub(crate) struct CountedFile {
    file: File,
    pos: u64,
    at: Arc<AtomicU64>,
    len: u64,
}

impl CountedFile {
    pub(crate) fn new(file: File) -> std::io::Result<(Self, ReadPosition)> {
        let len = file.metadata()?.len();
        let at = Arc::new(AtomicU64::new(0));
        let position = ReadPosition {
            at: Arc::clone(&at),
            len,
        };
        Ok((
            Self {
                file,
                pos: 0,
                at,
                len,
            },
            position,
        ))
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }
}

impl Read for CountedFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.file.read(buf)?;
        self.pos += n as u64;
        self.at.store(self.pos, Ordering::Relaxed);
        Ok(n)
    }
}

impl Seek for CountedFile {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.pos = self.file.seek(to)?;
        self.at.store(self.pos, Ordering::Relaxed);
        Ok(self.pos)
    }
}

impl symphonia::core::io::MediaSource for CountedFile {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// `items`, telling `progress` every so often how far through the file `read` they've got,
/// scaled to end at `upto` (the rest is left for work after the reading).
pub fn reported<'p, I>(
    items: I,
    read: ReadPosition,
    progress: &'p Progress<'p>,
    upto: f32,
) -> impl Iterator<Item = I::Item> + 'p
where
    I: Iterator + 'p,
{
    items.enumerate().map(move |(i, item)| {
        if i % LOOK_EVERY == 0 {
            progress.set(read.fraction() * upto);
        }
        item
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn progress_only_goes_forwards_in_steps_and_ends_at_one() {
        let seen = RefCell::new(Vec::new());
        let report = |f: f32| seen.borrow_mut().push(f);
        let progress = Progress::new(&report);
        for i in 0..=10_000 {
            progress.set(i as f32 / 10_000.0);
        }
        // Backwards, a small step, and nonsense change nothing.
        progress.set(0.2);
        progress.set(f32::NAN);
        progress.finish();
        progress.finish();
        let seen = seen.into_inner();
        assert!(seen.len() <= 102, "{} reports", seen.len());
        assert!(seen.len() > 90, "{} reports", seen.len());
        assert_eq!(seen.first(), Some(&0.0));
        assert_eq!(seen.last(), Some(&1.0));
        assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
        assert_eq!(seen.iter().filter(|&&f| f == 1.0).count(), 1);
    }

    #[test]
    fn finishing_early_still_ends_at_one() {
        let seen = RefCell::new(Vec::new());
        let report = |f: f32| seen.borrow_mut().push(f);
        let progress = Progress::new(&report);
        progress.set(0.995);
        progress.finish();
        assert_eq!(seen.into_inner(), [0.995, 1.0]);
    }
}
