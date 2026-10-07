//! Quick looks at the disk (is this folder there? is that show file still there?) that never
//! hold the app up. Each look runs on a thread of its own and the caller waits only so long;
//! a look at a network share that has gone away can be stuck for minutes. So that stuck looks
//! can't pile up, a path still being looked at isn't looked at again until that look ends, and
//! only so many looks may be waiting at once, across the app. A path that can't be looked at
//! now is answered at once as "don't know".

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

/// How many looks may be waiting at once, across the app.
const MAX_LOOKS: usize = 16;

/// Looks at paths off the calling thread, never more than `max` at once.
pub(crate) struct Probes {
    max: usize,
    /// The paths being looked at now.
    busy: Arc<Mutex<HashSet<PathBuf>>>,
}

static SHARED: LazyLock<Probes> = LazyLock::new(|| Probes::new(MAX_LOOKS));

impl Probes {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            max,
            busy: Arc::default(),
        }
    }

    /// The app's looks (the dialogs' starting folders, the recent shows).
    pub(crate) fn shared() -> &'static Self {
        &SHARED
    }

    /// Starts `look` at each of `paths`. Each answer arrives as `(index, Some(answer))` when
    /// its look ends, or as `(index, None)` at once when the path can't be looked at now (a
    /// look at it hasn't ended yet, or too many looks are waiting).
    pub(crate) fn start<T, F>(&self, paths: &[PathBuf], look: F) -> mpsc::Receiver<(usize, Option<T>)>
    where
        T: Send + 'static,
        F: Fn(&Path) -> T + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let look = Arc::new(look);
        for (i, path) in paths.iter().enumerate() {
            if !self.take(path) {
                let _ = tx.send((i, None));
                continue;
            }
            let (sender, busy, look, owned) = (
                tx.clone(),
                Arc::clone(&self.busy),
                Arc::clone(&look),
                path.clone(),
            );
            let spawned = std::thread::Builder::new()
                .name("pixelflow-disk-look".into())
                .spawn(move || {
                    let answer = look(&owned);
                    busy.lock().unwrap_or_else(PoisonError::into_inner).remove(&owned);
                    let _ = sender.send((i, Some(answer)));
                });
            if spawned.is_err() {
                self.busy
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(path);
                let _ = tx.send((i, None));
            }
        }
        rx
    }

    /// Marks `path` as being looked at; false when it already is, or too many are.
    fn take(&self, path: &Path) -> bool {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        busy.len() < self.max && !busy.contains(path) && busy.insert(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const SOON: Duration = Duration::from_millis(100);

    #[test]
    fn stuck_looks_dont_pile_up() {
        let probes = Probes::new(2);
        let gate = Arc::new(Mutex::new(()));
        let looks = Arc::new(AtomicUsize::new(0));
        // A look that's stuck until the gate opens, like one at a share that's gone away.
        let stuck = {
            let (gate, looks) = (Arc::clone(&gate), Arc::clone(&looks));
            move |_: &Path| {
                looks.fetch_add(1, Ordering::SeqCst);
                drop(gate.lock().unwrap());
                true
            }
        };
        let (a, b, c) = (PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c"));
        let closed = gate.lock().unwrap();
        let first = probes.start(std::slice::from_ref(&a), stuck.clone());
        assert!(first.recv_timeout(SOON).is_err(), "stuck");
        // Asked again (the next dialog, the next list): /a isn't looked at again, /b is, and
        // /c would make three waiting.
        let second = probes.start(&[a, b, c.clone()], stuck.clone());
        let mut at_once: Vec<_> = (0..2).map(|_| second.recv_timeout(SOON).unwrap()).collect();
        at_once.sort_by_key(|(i, _)| *i);
        assert_eq!(at_once, vec![(0, None), (2, None)]);
        std::thread::sleep(SOON);
        assert_eq!(looks.load(Ordering::SeqCst), 2);
        // Once the share answers, its looks end and the paths can be looked at again.
        drop(closed);
        assert_eq!(
            first.recv_timeout(Duration::from_secs(5)).unwrap(),
            (0, Some(true))
        );
        assert_eq!(
            second.recv_timeout(Duration::from_secs(5)).unwrap(),
            (1, Some(true))
        );
        let third = probes.start(&[c], stuck);
        assert_eq!(
            third.recv_timeout(Duration::from_secs(5)).unwrap(),
            (0, Some(true))
        );
    }

    #[test]
    fn answers_arrive_for_every_path() {
        let dir = tempfile::tempdir().unwrap();
        let paths = vec![dir.path().to_path_buf(), dir.path().join("none")];
        let answers = Probes::new(4).start(&paths, |p| p.is_dir());
        let mut got: Vec<_> = answers.iter().collect();
        got.sort_by_key(|(i, _)| *i);
        assert_eq!(got, vec![(0, Some(true)), (1, Some(false))]);
    }
}
