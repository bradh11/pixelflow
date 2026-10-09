//! How well locking to the music repairs a sloppy draft of a real song: a look per detected
//! section and a flash per hit, each edge pushed 50–400 ms (flashes 50–200 ms) early or late, as a
//! model's arithmetic might leave them; then the draft is locked (see `pf_ai::align`) and each
//! edge's distance from the moment it was meant for is measured before and after.
//!
//! `cargo run --release -p pf-ai --example lock_eval -- song.mp3 [more.mp3 …]`

use pf_ai::align::{Anchors, lock};
use pf_analysis::{EventKind, analyze_file};
use pf_sequence::{Effect, EffectId, EffectKind, Row, Sequence, Target};
use std::path::Path;

/// A fixed pseudo-random sequence, so every run drafts the same mistakes.
struct Jitter(u64);

impl Jitter {
    fn next(&mut self, lo: u64, hi: u64) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let size = (lo + (self.0 >> 33) % (hi - lo + 1)) as i64;
        if (self.0 >> 20) & 1 == 0 { size } else { -size }
    }
}

/// Edge errors (ms) before and after.
#[derive(Default)]
struct Errors {
    before: Vec<u64>,
    after: Vec<u64>,
}

impl Errors {
    fn line(&self, what: &str) {
        let stats = |v: &[u64]| {
            let mean = v.iter().sum::<u64>() as f64 / v.len().max(1) as f64;
            let exact = v.iter().filter(|&&e| e == 0).count();
            (mean, v.iter().copied().max().unwrap_or(0), exact)
        };
        let (b_mean, b_max, b_exact) = stats(&self.before);
        let (a_mean, a_max, a_exact) = stats(&self.after);
        println!(
            "  {what:<15} {:>4} edges: before mean {b_mean:>5.0} ms, max {b_max:>3} ms, {b_exact:>3} exact | after mean {a_mean:>5.1} ms, max {a_max:>3} ms, {a_exact:>3} exact",
            self.before.len()
        );
    }
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: lock_eval <music file> [more …]");
        std::process::exit(2);
    }
    for (n, path) in paths.iter().enumerate() {
        let analysis = match analyze_file(Path::new(path)) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        let end = analysis.duration_ms;
        let mut jitter = Jitter(17 + n as u64);
        let off = |t: u64, by: i64| t.saturating_add_signed(by).min(end);
        let mut doc = Sequence::new("Eval", end);
        let mut looks = Row::new(Target::Group(pf_model::GroupId::new()));
        let mut flashes = Row::new(Target::Group(pf_model::GroupId::new()));
        // What each drafted edge was meant to be: (effect, meant start, meant end).
        let mut meant: Vec<(EffectId, u64, Option<u64>, bool)> = Vec::new();
        let sections = analysis.sections();
        for s in &sections {
            let start = if s.start_ms == 0 {
                0
            } else {
                off(s.start_ms, jitter.next(50, 400))
            };
            let stop = if s.end_ms >= end {
                end
            } else {
                off(s.end_ms, jitter.next(50, 400))
            };
            if stop <= start {
                continue;
            }
            let effect = Effect::new(EffectKind::ColorWash, start, stop);
            meant.push((effect.id, s.start_ms, Some(s.end_ms), true));
            looks.layers[0].effects.push(effect);
        }
        let mut last_end = 0;
        for hit in analysis.events.iter().filter(|e| e.kind == EventKind::Hit) {
            let start = off(hit.time_ms, jitter.next(50, 200)).max(last_end);
            let stop = (start + 250).min(end);
            if stop <= start {
                continue;
            }
            last_end = stop;
            let effect = Effect::new(EffectKind::Strobe, start, stop);
            meant.push((effect.id, hit.time_ms, None, false));
            flashes.layers[0].effects.push(effect);
        }
        doc.rows = vec![looks, flashes];
        let Some(anchors) = Anchors::for_song(Some(&analysis), None) else {
            eprintln!("{path}: nothing to lock to");
            continue;
        };
        let locked = lock(Some(&Sequence::new("Eval", end)), &doc, &anchors, &[]);
        let after = pf_engine::edited_sequence(&doc, &locked.edits).expect("locking makes valid edits");
        let (mut section_edges, mut hit_edges) = (Errors::default(), Errors::default());
        for (id, start, stop, is_section) in &meant {
            let was = doc.effect(*id).expect("drafted");
            let now = after.effect(*id).expect("kept");
            let errors = if *is_section {
                &mut section_edges
            } else {
                &mut hit_edges
            };
            // The song's first and last edges were drafted exactly; they're not counted.
            if *start > 0 {
                errors.before.push(was.start_ms.abs_diff(*start));
                errors.after.push(now.start_ms.abs_diff(*start));
            }
            if let Some(stop) = stop.filter(|&s| s < end) {
                errors.before.push(was.end_ms.abs_diff(stop));
                errors.after.push(now.end_ms.abs_diff(stop));
            }
        }
        println!(
            "{path}: {:.1} BPM, {} sections, {} hits; locked {} effect edges",
            analysis.tempo_bpm.unwrap_or(0.0),
            sections.len(),
            hit_edges.before.len(),
            locked.effect_edges
        );
        section_edges.line("section edges");
        hit_edges.line("hit starts");
    }
}
