//! Sending a frame allocates nothing: the output thread's pass gathers each controller's
//! channels, copies them into its packets, and sends them, all in buffers made once.
//!
//! Counted with the benchmark's allocator, which only a program can install, so this is a test
//! program of its own with a single test: nothing else is allocating beside it.

use pf_bench::alloc::{Counting, counted};
use pf_bench::pipeline::{Controllers, Workload};
use pf_bench::sequence::Variant;
use pf_bench::sink::{Guarded, Sent, SinkKind};
use pf_output::OutputSettings;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn an_output_pass_allocates_nothing() {
    // Large enough for sACN and DDP controllers both.
    let workload = Workload::new(6_000, Variant::Typical, 2).unwrap().without_media();
    let sent = Arc::new(Sent::default());
    let mut transport = Guarded::open(SinkKind::Null, Arc::clone(&sent)).unwrap();
    let mut controllers = Controllers::new(workload.plan(None), &OutputSettings::default()).unwrap();
    let wiring = controllers.wiring();
    assert!(wiring.universes > 0 && wiring.packets_per_pass > wiring.universes);
    let frame: Vec<u8> = (0..workload.map.frame_len).map(|i| i as u8).collect();
    controllers.send(&frame, &mut transport);

    let before = counted().expect("the counting allocator is installed");
    for _ in 0..100 {
        controllers.send(&frame, &mut transport);
    }
    let after = counted().expect("the counting allocator is installed");
    assert_eq!(after.allocations - before.allocations, 0);
    assert_eq!(after.bytes - before.bytes, 0);
    assert_eq!(sent.now().packets, 101 * wiring.packets_per_pass as u64);
}
