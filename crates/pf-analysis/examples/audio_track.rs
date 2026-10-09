//! Prints a music file's audio track (what effects that follow the music read): how long it took,
//! and a line per second with the level, bands, and the beats and onsets in it.
//!
//! `cargo run --release -p pf-analysis --example audio_track -- song.mp3 [frame ms]`

use pf_analysis::audio_track_file;
use std::path::Path;
use std::time::Instant;

fn bar(v: f32) -> String {
    let n = (v.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("{:<10}", "#".repeat(n))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: audio_track <music file> [frame ms]");
        std::process::exit(2);
    };
    let frame_ms = args.next().and_then(|a| a.parse().ok()).unwrap_or(25);
    let started = Instant::now();
    let track = match audio_track_file(Path::new(&path), frame_ms, &|| false, &|_| {}) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let took = started.elapsed();
    let frames = track.len() as u64;
    println!(
        "{path}: {frames} frames of {frame_ms} ms ({:.1} s) in {:.2} s, {} KB",
        frames as f64 * f64::from(frame_ms) / 1000.0,
        took.as_secs_f64(),
        track.to_bytes().len() / 1024
    );
    let per_second = u64::from(1000 / frame_ms.max(1)).max(1);
    println!("  s  level      bass       treble     beats onsets");
    for start in (0..frames).step_by(per_second as usize) {
        let span = start..(start + per_second).min(frames);
        let mean = |f: &dyn Fn(u64) -> f32| span.clone().map(f).sum::<f32>() / span.clone().count() as f32;
        println!(
            "{:3}  {} {} {} {:5} {:6}",
            start / per_second,
            bar(mean(&|i| track.level(i))),
            bar(mean(&|i| track.bass(i))),
            bar(mean(&|i| track.treble(i))),
            span.clone().filter(|&i| track.is_beat(i)).count(),
            span.clone().filter(|&i| track.is_note_on(i)).count(),
        );
    }
}
