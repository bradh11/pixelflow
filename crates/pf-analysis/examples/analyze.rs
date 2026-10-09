//! Prints what analysis finds in a music file: tempo, the first downbeat, sections, the
//! strongest hits, drum counts, the top moments, and how long it took.
//!
//! `cargo run --release -p pf-analysis --example analyze -- song.mp3`

use pf_analysis::{BarDrums, EventKind, Moment, MomentKind, analyze_file};
use std::path::Path;
use std::time::Instant;

fn clock(ms: u64) -> String {
    format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

fn line(m: &Moment) -> String {
    let span = m.end_ms.map_or(String::new(), |e| format!("–{}", clock(e)));
    let label = m.label.as_ref().map_or(String::new(), |l| format!(" ({l})"));
    format!(
        "{}{span} {}{label} {:.2} → {}",
        clock(m.time_ms),
        m.kind.word(),
        m.importance,
        m.suggest.word()
    )
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
    let total = |f: fn(&BarDrums) -> u16| a.bar_drums.iter().map(|b| u32::from(f(b))).sum::<u32>();
    println!(
        "drums: {} kicks, {} snares, {} hats, {} crashes; {} notable",
        total(|b| b.kick),
        total(|b| b.snare),
        total(|b| b.hat),
        total(|b| b.crash),
        a.drums.len()
    );
    println!("top moments ({} in all):", a.moments.len());
    for m in a.top_moments(25) {
        println!("  {}", line(m));
    }
    for kind in [MomentKind::Breakdown, MomentKind::Stop, MomentKind::Fill] {
        let of: Vec<String> = a.moments.iter().filter(|m| m.kind == kind).map(line).collect();
        println!("{}s: {}", kind.word(), of.len());
        for l in of {
            println!("  {l}");
        }
    }
}
