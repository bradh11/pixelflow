//! The threads frames are drawn on.
//!
//! A frame's work is shared out among a few threads that are started once and kept (see
//! `render.rs` for how it's cut up). Every renderer in the process draws on the same ones unless
//! it's told otherwise: several renderers are often alive at once (playback, the preview, an
//! export), and a set of threads each would have them fighting over the same cores.
//!
//! How many threads there are never changes what's drawn, only how soon.

use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::{Arc, OnceLock};

/// Cores left alone by the shared threads: the player thread waits while a frame is drawn, but
/// the output thread, the music, and the app's own drawing go on.
const SPARE_CORES: usize = 2;

/// Most threads the shared pool has. Past this a frame's last few tiles are all that's left to
/// share, and waking more threads costs more than they save (see `docs/benchmarks.md`).
const MOST_THREADS: usize = 12;

/// Sets the shared pool's size, for measuring and for ruling the threads out when something
/// looks wrong (`1` draws everything on the caller's thread).
const THREADS_VARIABLE: &str = "PIXELFLOW_RENDER_THREADS";

/// Threads that draw frames.
pub struct RenderPool {
    pool: ThreadPool,
    threads: usize,
}

/// The pool renderers share, once the first frame worth sharing out has started it.
static SHARED: OnceLock<Option<Arc<RenderPool>>> = OnceLock::new();

impl std::fmt::Debug for RenderPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RenderPool({} threads)", self.threads)
    }
}

impl RenderPool {
    /// A pool of `threads` threads of its own (at least one), for a renderer that should draw on
    /// exactly that many: tests and measurements. `None` when the threads can't be started.
    pub fn new(threads: usize) -> Option<Arc<Self>> {
        let threads = threads.max(1);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("pf-render-{i}"))
            .build()
            .ok()?;
        Some(Arc::new(Self { pool, threads }))
    }

    /// The pool renderers share: [`default_threads`] threads, started the first time a frame is
    /// worth sharing out. `None` when that's one thread (frames are then drawn on the caller's),
    /// or the threads can't be started.
    pub fn shared() -> Option<Arc<Self>> {
        SHARED.get_or_init(|| Self::sized(default_threads())).clone()
    }

    /// Starts the shared pool with `threads` threads rather than [`default_threads`] (one: no
    /// pool, every frame is drawn on the caller's thread). False when it had started already: it
    /// then stays the size it is.
    pub fn share(threads: usize) -> bool {
        let mut started = false;
        SHARED.get_or_init(|| {
            started = true;
            Self::sized(threads)
        });
        started
    }

    /// A pool of `threads` threads, or none for one thread.
    fn sized(threads: usize) -> Option<Arc<Self>> {
        match threads {
            0 | 1 => None,
            threads => Self::new(threads),
        }
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Runs `work` on the pool's threads and waits for it. Whatever `work` shares out with
    /// `rayon::join` or a parallel iterator is shared among them.
    pub(crate) fn run<R: Send>(&self, work: impl FnOnce() -> R + Send) -> R {
        self.pool.install(work)
    }
}

/// How many threads the shared pool has on this machine: its cores less two, at least one and at
/// most twelve, or what `PIXELFLOW_RENDER_THREADS` says.
pub fn default_threads() -> usize {
    let asked = std::env::var(THREADS_VARIABLE)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&threads| threads > 0);
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    threads_for(cores, asked)
}

fn threads_for(cores: usize, asked: Option<usize>) -> usize {
    match asked {
        // Never more threads than cores: they would only take turns.
        Some(threads) => threads.min(cores.max(1)),
        None => cores.saturating_sub(SPARE_CORES).clamp(1, MOST_THREADS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_pool_leaves_two_cores_and_stops_at_twelve_threads() {
        assert_eq!(threads_for(1, None), 1);
        assert_eq!(threads_for(2, None), 1);
        assert_eq!(threads_for(3, None), 1);
        assert_eq!(threads_for(4, None), 2);
        assert_eq!(threads_for(8, None), 6);
        assert_eq!(threads_for(14, None), 12);
        assert_eq!(threads_for(64, None), 12);
    }

    #[test]
    fn a_size_asked_for_is_kept_up_to_the_machines_cores() {
        assert_eq!(threads_for(8, Some(1)), 1);
        assert_eq!(threads_for(8, Some(8)), 8);
        assert_eq!(threads_for(8, Some(40)), 8);
    }

    #[test]
    fn a_pool_runs_work_on_its_own_threads() {
        let pool = RenderPool::new(3).expect("three threads");
        assert_eq!(pool.threads(), 3);
        let name = pool.run(|| std::thread::current().name().map(str::to_string));
        assert!(name.is_some_and(|name| name.starts_with("pf-render-")));
        let (a, b) = pool.run(|| rayon::join(|| 1 + 1, || 2 + 2));
        assert_eq!((a, b), (2, 4));
    }
}
