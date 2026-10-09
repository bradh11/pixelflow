//! Prints what analysis finds in a music file: tempo, the first downbeat, sections, the
//! strongest hits, and how long it took.
//!
//! `cargo run --release -p pf-analysis --example analyze -- song.mp3`

use pf_analysis::{EventKind, analyze_file};
use std::path::Path;
use std::time::Instant;

fn clock(ms: u64) -> String {
    format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: analyze <music file>");
        std::process::exit(2);
    };
    let started = Instant::now();
    let a = match analyze_file(Path::new(&path)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let took = started.elapsed();
    println!("{path}");
    println!(
        "{} long, {:.1} BPM, {} beats, {} bars, bar 1 at {}; analyzed in {:.2} s",
        clock(a.duration_ms),
        a.tempo_bpm.unwrap_or(0.0),
        a.beats.len(),
        a.bars.len(),
        a.bars.first().map_or("-".into(), |&b| clock(b)),
        took.as_secs_f64()
    );
    println!("confidence: {:?}", a.confidence);
    println!("sections:");
    for s in a.sections() {
        println!(
            "  {:>7} – {:>7}  {:<11} {:<2} energy {:.2} ({:?}), sure {:.2}",
            clock(s.start_ms),
            clock(s.end_ms),
            s.label,
            s.group,
            s.energy,
            s.level,
            s.confidence
        );
    }
    let count = |kind: EventKind| a.events.iter().filter(|e| e.kind == kind).count();
    println!(
        "events: {} ({} hits, {} drops, {} breaks, {} builds)",
        a.events.len(),
        count(EventKind::Hit),
        count(EventKind::Drop),
        count(EventKind::Break),
        count(EventKind::Build)
    );
    let mut hits: Vec<_> = a.events.iter().filter(|e| e.kind == EventKind::Hit).collect();
    hits.sort_by(|x, y| y.strength.total_cmp(&x.strength));
    let top: Vec<String> = hits
        .iter()
        .take(10)
        .map(|e| format!("{} ({:.2})", clock(e.time_ms), e.strength))
        .collect();
    println!("top hits: {}", top.join(", "));
    for e in a.events.iter().filter(|e| e.kind != EventKind::Hit) {
        println!(
            "  {:?} at {} strength {:.2}{}",
            e.kind,
            clock(e.time_ms),
            e.strength,
            e.duration_ms
                .map_or(String::new(), |d| format!(" for {:.1} s", d as f64 / 1000.0))
        );
    }
}
