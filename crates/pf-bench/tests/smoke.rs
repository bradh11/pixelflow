//! The benchmark runs: a small show, a second of sequence, every mode and both sinks. Nothing
//! here asserts how long anything took (the machine running the tests may be slow or busy), only
//! that every part of the harness does its job and accounts for what it did.

use pf_bench::effects::breakdown;
use pf_bench::measure::{Mode, Options, RunResult};
use pf_bench::pipeline::Workload;
use pf_bench::report::{CAVEAT, Report};
use pf_bench::run::run;
use pf_bench::sequence::{KINDS, Variant};
use pf_bench::sink::SinkKind;
use pf_bench::sync::{SyncOptions, run_sync};
use std::process::Command;

/// Large enough for a matrix on a DDP controller of its own beside the sACN strings.
const PIXELS: u32 = 2_000;
const SECOND: Options = Options {
    seconds: 1,
    warmup_seconds: 0,
    send_hz: None,
};

fn workload(variant: Variant) -> Workload {
    Workload::new(PIXELS, variant, 2).expect("a workload")
}

/// What every run must get right, however fast or slow the machine.
fn check(run: &RunResult) {
    assert_eq!((run.pixels, run.frame_ms), (PIXELS, 25));
    assert_eq!(run.wiring.channels, PIXELS as usize * 3);
    assert!(run.wiring.universes > 0 && run.wiring.packets_per_pass > run.wiring.universes);
    assert_eq!(run.frames.sent + run.frames.dropped, run.frames.due);
    assert!(run.frames.sent > 0 && run.fps > 0.0);
    assert!(run.network.packets > 0 && run.network.bytes > 0);
    assert_eq!((run.network.refused, run.network.send_failures), (0, 0));
    assert!(run.resources.wall_seconds > 0.0);
    assert!(!run.verdict.summary.is_empty());
}

#[test]
fn flat_out_every_stage_is_timed_for_every_frame() {
    for variant in Variant::ALL {
        let result = run(&workload(variant), Mode::Asap, SinkKind::Null, SECOND).unwrap();
        check(&result);
        assert_eq!((result.frames.due, result.frames.dropped), (40, 0));
        let s = &result.stages;
        for stage in [
            s.render,
            s.preview,
            s.map,
            s.packetize,
            s.send,
            s.output_pass,
            s.frame,
        ] {
            assert_eq!(stage.count, 40);
            assert!(stage.p50 <= stage.p95 && stage.p95 <= stage.p99 && stage.p99 <= stage.max);
        }
        assert!(s.frame.mean >= s.render.mean);
        // A pass for every frame, nothing lost between the pipeline and the sink.
        assert_eq!(result.network.packets, 40 * result.wiring.packets_per_pass as u64);
        assert_eq!(result.network.bytes, 40 * result.wiring.bytes_per_pass as u64);
        assert!(
            result.lit_percent > 1.0,
            "something was drawn: {}",
            result.lit_percent
        );
        assert!(result.preview_poll.count > 0 && result.pacing.is_none());
    }
}

#[test]
fn packets_reach_listeners_on_the_loopback_interface() {
    let result = run(
        &workload(Variant::Typical),
        Mode::Asap,
        SinkKind::Loopback,
        SECOND,
    )
    .unwrap();
    check(&result);
    let received = result
        .network
        .received_packets
        .expect("listeners count what arrives");
    assert!(received > 0, "nothing arrived over loopback");
    // Every packet was either sent or dropped at a full send buffer.
    let passes = 40 * result.wiring.packets_per_pass as u64;
    assert_eq!(result.network.packets + result.network.send_buffer_full, passes);
    assert_eq!(result.resources.pipeline_threads, 2);
}

#[test]
fn in_real_time_every_frame_is_accounted_for() {
    for sink in SinkKind::ALL {
        let result = run(&workload(Variant::Typical), Mode::Paced, sink, SECOND).unwrap();
        check(&result);
        assert_eq!(result.frames.due, 40);
        assert_eq!(result.send_hz, Some(80));
        let pacing = result.pacing.expect("a paced run keeps time");
        assert_eq!(pacing.latency.count as u64, result.frames.sent);
        assert!(pacing.shown.count as u64 >= result.frames.sent);
        assert!(result.frames.late <= result.frames.due);
        assert!(result.stages.output_pass.count > 0 && result.preview_poll.count > 0);
        assert!((0.0..=100.0).contains(&result.verdict.on_time_percent));
    }
}

#[test]
fn the_engine_itself_plays_the_same_show_and_sequence() {
    let workload = workload(Variant::WorstCase).without_media();
    let result = run(&workload, Mode::Engine, SinkKind::Null, SECOND).unwrap();
    check(&result);
    assert_eq!((result.mode, result.frames.due), (Mode::Engine, 40));
    assert!(result.pacing.expect("frames were watched").shown.count > 0);
    assert!(result.notes.iter().any(|n| n.contains("engine itself")));
}

#[test]
fn the_lights_are_timed_against_the_music_in_the_engine() {
    // Three seconds of sequence on a loop, played for four: once round and into the next.
    let workload = Workload::new(PIXELS, Variant::Typical, 3)
        .unwrap()
        .without_media();
    let options = SyncOptions {
        seconds: 4,
        warmup_seconds: 0,
        load_threads: 1,
        seek_seconds: 0,
    };
    let result = run_sync(&workload, SinkKind::Null, options).unwrap();
    assert_eq!((result.pixels, result.frame_ms, result.send_hz), (PIXELS, 25, 80));
    assert_eq!((result.sequence_seconds, result.load_threads), (3, 1));
    // Every frame that went out was read off its marker and is accounted for.
    let frames = result.frames;
    assert!(frames.sent > 0 && frames.sent + frames.missed == frames.due);
    assert_eq!(result.sent.count as u64, frames.sent);
    assert!(result.began.count > 0 && result.began.p50 >= 0.0);
    assert!(result.sent.p50 >= 0.0);
    // The music was taken back to its top with the lights.
    assert!(result.loop_late.count >= 1 && result.loop_late.p50 >= 0.0);
    assert_eq!(result.seek_recovery.count, 0);
    assert!(result.notes.is_empty(), "{:?}", result.notes);
    assert!(!result.verdict.summary.is_empty());
    // A sequence too short to jump about in is refused when jumps are asked for.
    let jumping = SyncOptions {
        seek_seconds: 1,
        ..options
    };
    assert!(run_sync(&workload, SinkKind::Null, jumping).is_err());
}

#[test]
fn a_sync_run_is_reported_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("sync.json");
    let out = Command::new(env!("CARGO_BIN_EXE_pf-bench"))
        .args(["--tier", "2000", "--mode", "sync", "--sink", "loopback"])
        .args(["--seconds", "1", "--warmup-seconds", "0"])
        .args(["--sequence-seconds", "3", "--seek-seconds", "0"])
        .arg("--json")
        .arg(&json)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Report = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!((report.syncs.len(), report.runs.len()), (1, 0));
    assert!(report.syncs[0].frames.sent > 0);
    let printed = String::from_utf8_lossy(&out.stdout);
    for heading in [
        "## Lights against the music",
        "## Frames, jumps, and rounds of the loop",
    ] {
        assert!(printed.contains(heading), "no {heading:?} in:\n{printed}");
    }
    // A report saved before there were sync runs still reads.
    let mut old: serde_json::Value = serde_json::to_value(&report).unwrap();
    old.as_object_mut().unwrap().remove("syncs");
    let old: Report = serde_json::from_value(old).unwrap();
    assert!(old.syncs.is_empty());
}

#[test]
fn every_kind_of_effect_is_costed_alone_and_as_part_of_both_sequences() {
    // The smallest show there is: this draws a few thousand frames.
    let workload = Workload::new(600, Variant::Typical, 2).unwrap();
    let costs = breakdown(&workload, 1, 1, &|_| {});
    assert_eq!(costs.kinds.len(), KINDS.len());
    assert_eq!(costs.pixels, 600);
    // Five, and Text in a computer font when this computer has one to cost it in.
    assert_eq!(
        costs.extras.len(),
        5 + usize::from(pf_bench::effects::text_font().is_some())
    );
    assert_eq!(costs.sequences.len(), 2);
    for sequence in &costs.sequences {
        assert!(!sequence.kinds.is_empty());
        // The shares add up whenever the clock saw the effects take any time. On a show this
        // small and a busy machine it can time an empty frame longer than a full one, and then
        // there are no shares to speak of: what the clock said is never what's checked.
        let (shares, time): (f64, f64) = sequence
            .kinds
            .iter()
            .fold((0.0, 0.0), |(shares, time), k| (shares + k.percent, time + k.ms));
        assert!(time <= f64::EPSILON || (shares - 100.0).abs() < 0.01, "{shares}");
        assert!(
            sequence
                .kinds
                .iter()
                .all(|k| k.effects > 0 && k.layers_per_pixel > 0.0)
        );
    }
}

#[test]
fn the_program_runs_and_writes_its_report_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let (json, markdown) = (dir.path().join("run.json"), dir.path().join("run.md"));
    let out = Command::new(env!("CARGO_BIN_EXE_pf-bench"))
        .args(["--tier", "2000", "--mode", "asap", "--sink", "null"])
        .args([
            "--seconds",
            "1",
            "--warmup-seconds",
            "0",
            "--sequence-seconds",
            "2",
        ])
        .arg("--json")
        .arg(&json)
        .arg("--markdown")
        .arg(&markdown)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Report = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(
        (report.runs.len(), report.shows.len(), report.sequences.len()),
        (1, 1, 1)
    );
    check(&report.runs[0]);
    assert_eq!(report.caveat, CAVEAT);
    let printed = String::from_utf8_lossy(&out.stdout);
    let written = std::fs::read_to_string(&markdown).unwrap();
    assert_eq!(printed.trim_end(), written.trim_end());
    for heading in [
        CAVEAT,
        "## Machine",
        "## Shows (pixels in props)",
        "## Sequences",
        "## Flat out: each stage, in milliseconds",
        "## Network, memory, and warm-up",
    ] {
        assert!(written.contains(heading), "no {heading:?} in:\n{written}");
    }

    // A saved report prints as it did the first time.
    let out = Command::new(env!("CARGO_BIN_EXE_pf-bench"))
        .arg("--render")
        .arg(&json)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        written.trim_end()
    );

    // Describing a tier runs nothing.
    let out = Command::new(env!("CARGO_BIN_EXE_pf-bench"))
        .args(["--tier", "2000", "--describe", "--sequence-seconds", "2"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.contains("| 2k | 2,000 |") && printed.contains("| 2k | worst |"));
    assert!(!printed.contains("## Flat out"));
}
