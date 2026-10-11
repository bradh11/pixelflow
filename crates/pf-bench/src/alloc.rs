//! Counts heap allocations, when the benchmark binary installs [`Counting`] as its allocator.
//!
//! This is the one place the crate needs `unsafe`: a global allocator can't be written without
//! it. Every call goes straight to the system allocator; the counting is a few atomic adds.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

/// The system allocator, counted.
pub struct Counting;

fn count(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    let live = LIVE.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every method hands its arguments unchanged to `System`, which upholds the
// `GlobalAlloc` contract; the counters never touch the memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's layout, passed on as it came.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            count(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's layout, passed on as it came.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            count(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's pointer and layout, passed on as they came.
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's pointer, layout, and size, passed on as they came.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
            count(new_size);
        }
        new
    }
}

/// The counters at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counted {
    /// Allocations made so far (a reallocation counts as one).
    pub allocations: u64,
    /// Bytes asked for so far.
    pub bytes: u64,
    /// The most bytes held at once so far.
    pub peak_live: u64,
}

/// The counters now, or `None` when nothing is counting (the allocator isn't installed, as in
/// the library's own tests).
pub fn counted() -> Option<Counted> {
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    (allocations > 0).then(|| Counted {
        allocations,
        bytes: BYTES.load(Ordering::Relaxed),
        peak_live: PEAK.load(Ordering::Relaxed),
    })
}
