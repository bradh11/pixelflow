//! How locking word times onto the voice changes them on a real song: published lyrics and
//! what the recognizer heard (both as Find lyrics keeps them in its cache) are put together,
//! locked onto the song's lead vocal (see `pf_ai::lyrics::refine`), and compared: the shift
//! found, how far words moved, how near onsets they start, the rests left between them, and
//! some words before and after, to check by ear. Nothing is looked up or sent.
//!
//! `cargo run --release -p pf-ai --example lyric_timing_eval -- song.mp3 lrclib.json openai.json`

use pf_ai::lyrics::lrclib::Published;
use pf_ai::lyrics::transcribe::Heard;
use pf_ai::lyrics::{combine, lrc, refine};
use std::path::Path;

fn mmss(ms: u64) -> String {
    format!("{:02}:{:06.3}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [song, published, heard] = args.as_slice() else {
        eprintln!("usage: lyric_timing_eval song.mp3 lrclib.json openai.json");
        std::process::exit(2);
    };
    let voice =
        pf_analysis::vocal_track_file(Path::new(song), &|| false, &|_| {}).expect("the song can't be read");
    let text = std::fs::read_to_string(published).expect("the published lyrics can't be read");
    // As kept now (a list), or by an earlier version (one entry).
    let published: Vec<Published> = serde_json::from_str(&text)
        .or_else(|_| serde_json::from_str::<Published>(&text).map(|p| vec![p]))
        .expect("not published lyrics");
    let heard: Heard =
        serde_json::from_str(&std::fs::read_to_string(heard).expect("what was heard can't be read"))
            .expect("not heard words");
    let synced = published
        .iter()
        .find_map(|p| p.synced.clone())
        .expect("no synced lyrics");
    let end = (voice.len() as f64 * voice.hop_ms) as u64;
    let lines = combine::timed_lines(&lrc::parse_lrc(&synced), end);
    let before = combine::from_lines_and_heard(&lines, &heard, end);
    let locked = refine::lock_to_voice(&before, &voice, end);
    let r = &locked.report;
    let published_words: usize = lines.iter().map(|l| l.words.len()).sum();
    let words: Vec<_> = before.iter().flat_map(|p| &p.words).collect();
    let short = words.iter().filter(|w| w.end_ms < w.start_ms + 30).count();
    println!(
        "published words {published_words}, placed {}, under 30 ms {short}",
        words.len()
    );
    println!(
        "best shift {:+} ms (score {:.1}, made: {}), lines shifted {}",
        r.best_shift_ms, r.shift_score, r.shifted, r.lines_shifted
    );
    println!(
        "words {}, locked onto an onset {} ({:.0}%), earlier {}, later {}",
        r.words,
        r.locked,
        100.0 * r.locked as f64 / r.words.max(1) as f64,
        r.earlier,
        r.later
    );
    println!(
        "moved: mean {:.0} ms, median {:.0} ms, 90th percentile {:.0} ms",
        r.mean_move_ms, r.median_move_ms, r.p90_move_ms
    );
    println!(
        "to the nearest onset: {:.0} ms before, {:.0} ms after",
        r.onset_distance_before_ms, r.onset_distance_after_ms
    );
    println!(
        "pairs with a rest over 50 ms: {:.0}% before, {:.0}% after; median word {:.0} ms before, {:.0} ms after",
        100.0 * r.gaps_before,
        100.0 * r.gaps_after,
        r.median_length_before_ms,
        r.median_length_after_ms
    );
    let after: Vec<_> = locked.phrases.iter().flat_map(|p| &p.words).collect();
    let every = (words.len() / 10).max(1);
    for (a, b) in words.iter().zip(&after).step_by(every).take(10) {
        println!(
            "  {:<14} {} -> {}  (end {} -> {})",
            a.text,
            mmss(a.start_ms),
            mmss(b.start_ms),
            mmss(a.end_ms),
            mmss(b.end_ms)
        );
    }
}
