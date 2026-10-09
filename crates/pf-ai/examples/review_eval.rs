//! Whether reviewing a draft makes it better, without a model: for each song, a new sequence on
//! the show (a row per group and per prop) has the song's moments staged (`stageMoments` from
//! importance 0.5), is reviewed, has the review's fixes made as the assistant would (each fix's
//! tool and input, as given), and is reviewed again. Prints both reviews and how long each took.
//!
//! With `--lyrics <cache folder>`, sung words come from what Find lyrics kept for the song (read
//! only; nothing is looked up).
//!
//! `cargo run --release -p pf-ai --example review_eval -- show.pixelflow.json song.mp3 [more.mp3 …]
//! [--lyrics ~/Library/Caches/com.bradh11.pixelflow]`

use pf_ai::lyrics::lrclib::Published;
use pf_ai::lyrics::transcribe::Heard;
use pf_ai::lyrics::{LyricsCache, combine, lrc, tracks, vocals};
use pf_ai::review::Review;
use pf_ai::{Cancel, Draft, UiContext, Workspace};
use pf_analysis::Analysis;
use pf_engine::Engine;
use pf_sequence::{Row, Target, TimingTrack};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let lyrics = args.iter().position(|a| a == "--lyrics").map(|i| {
        let dir = args.get(i + 1).cloned().unwrap_or_default();
        args.drain(i..(i + 2).min(args.len()));
        PathBuf::from(dir)
    });
    if args.len() < 2 {
        eprintln!("usage: review_eval <show.pixelflow.json> <music file> [more …] [--lyrics <cache folder>]");
        std::process::exit(2);
    }
    let show = PathBuf::from(&args[0]);
    for song in &args[1..] {
        if let Err(e) = evaluate(&show, Path::new(song), lyrics.as_deref()) {
            eprintln!("{song}: {e}");
        }
    }
}

fn evaluate(show: &Path, song: &Path, lyrics: Option<&Path>) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut engine = Engine::new(dir.path());
    engine.open(show).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let analysis = pf_analysis::analyze_file(song).map_err(|e| e.to_string())?;
    let analyzed = started.elapsed();
    let name = song
        .file_stem()
        .map(|s| {
            s.to_string_lossy()
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '-')
                .to_string()
        })
        .unwrap_or_default();
    let shown = engine.show().clone();
    let mut rows: Vec<Row> = shown
        .groups
        .iter()
        .map(|g| Row::new(Target::Group(g.id)))
        .collect();
    rows.extend(shown.props.iter().map(|p| Row::new(Target::Prop(p.id))));
    let music = song.to_string_lossy().to_string();
    engine
        .new_sequence_doc_with_rows(&name, analysis.duration_ms, Some(&music), rows)
        .map_err(|e| e.to_string())?;
    let words = lyrics.and_then(|dir| sung_words(dir, song, &analysis));
    if let Some(found) = &words {
        let mut doc = engine.sequence_document().ok_or("no sequence")?.clone();
        doc.timing_tracks.extend(found.iter().cloned());
        engine.adopt_sequence_doc(doc).map_err(|e| e.to_string())?;
    }
    let mut draft = Draft::new(Workspace::from_engine(&engine, UiContext::default()));
    let cancel = Cancel::new();
    println!(
        "{}: {:.1} BPM, {:.0} s, {} moments, {} props, {} pixels, lyrics: {} (analysis {:.1} s)",
        song.display(),
        analysis.tempo_bpm.unwrap_or(0.0),
        analysis.duration_ms as f64 / 1000.0,
        analysis.moments.len(),
        shown.props.len(),
        shown.props.iter().map(|p| u64::from(p.node_count())).sum::<u64>(),
        if words.is_some() { "yes" } else { "no" },
        analyzed.as_secs_f64(),
    );
    let staged = pf_ai::cues::stage(
        &mut draft,
        Some(&analysis),
        &json!({ "stageMoments": { "minImportance": 0.5 } }),
    )?;
    println!("  {staged}");
    // The music for effects that follow it is worked out in the background when a sequence
    // opens (as in the app); it's done before the first review here, so reviews time alone.
    let waited = Instant::now();
    while draft.base().audio.has_music() && draft.base().audio.track().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!(
        "  music for the renderer worked out in {:.2} s",
        waited.elapsed().as_secs_f64()
    );

    // As the assistant works: review, fix, review once more, fix; then the proposal (locked to
    // the music) carries the final review.
    let (first, took) = timed(|| draft.review(Some(&analysis), &cancel))?;
    print_review("review 1", &first, took);
    let (_, again) = timed(|| draft.review(Some(&analysis), &cancel))?;
    println!("  (the same sequence again: {again:.3} s)");
    for line in apply_fixes(&mut draft, &analysis, &first) {
        println!("  fix: {line}");
    }
    let (second, took) = timed(|| draft.review(Some(&analysis), &cancel))?;
    print_review("review 2", &second, took);
    for line in apply_fixes(&mut draft, &analysis, &second) {
        println!("  fix: {line}");
    }
    let (third, took) = timed(|| draft.review(Some(&analysis), &cancel))?;
    print_review("after the second fixes", &third, took);
    let locked = draft.lock_to_music(Some(&analysis));
    let (last, took) = timed(|| draft.review(Some(&analysis), &cancel))?;
    print_review(&format!("as proposed ({locked} edges locked)"), &last, took);
    println!();
    Ok(())
}

fn timed(run: impl FnOnce() -> Result<Option<Review>, String>) -> Result<(Review, f64), String> {
    let started = Instant::now();
    let review = run()?.ok_or("no sequence to review")?;
    Ok((review, started.elapsed().as_secs_f64()))
}

fn print_review(when: &str, review: &Review, seconds: f64) {
    println!("  {when}: {} ({seconds:.2} s)", review.summary);
    println!(
        "    criteria: {}",
        serde_json::to_string(&review.criteria).unwrap_or_default()
    );
    for fix in &review.fixes {
        let mut how: Vec<String> = fix
            .calls
            .iter()
            .map(|c| {
                let input = c.input.to_string();
                let short: String = input.chars().take(160).collect();
                let cut = if short.len() < input.len() { "…" } else { "" };
                format!("{} {short}{cut}", c.tool)
            })
            .collect();
        how.extend(fix.hint.clone());
        println!("    - {} {} → {}", fix.at, fix.what, how.join("; "));
    }
    if review.more_fixes > 0 {
        println!("    …and {} more", review.more_fixes);
    }
    for note in &review.notes {
        println!("    note: {note}");
    }
}

/// Makes the review's fixes as the assistant would: the copies (repeat_effects) first, then
/// every cue in one stage_cue call (one at a time if that's refused).
fn apply_fixes(draft: &mut Draft, analysis: &Analysis, review: &Review) -> Vec<String> {
    let mut said = Vec::new();
    let mut cues: Vec<Value> = Vec::new();
    for fix in &review.fixes {
        if fix.calls.is_empty() {
            said.push(format!("left as is: {}", fix.what));
        }
        for call in &fix.calls {
            match call.tool {
                "repeat_effects" => {
                    let result = pf_ai::arrange::repeat(draft, &call.input);
                    said.push(format!("repeat_effects: {}", result.unwrap_or_else(|e| e)));
                }
                _ => cues.extend(call.input["cues"].as_array().cloned().unwrap_or_default()),
            }
        }
    }
    if cues.is_empty() {
        return said;
    }
    match pf_ai::cues::stage(draft, Some(analysis), &json!({ "cues": cues })) {
        Ok(result) => said.push(format!("stage_cue: {result}")),
        Err(_) => {
            for cue in cues {
                let result = pf_ai::cues::stage(draft, Some(analysis), &json!({ "cues": [cue] }));
                said.push(format!("stage_cue {cue}: {}", result.unwrap_or_else(|e| e)));
            }
        }
    }
    said
}

/// The song's sung words from what Find lyrics kept: the published lines (lined up with what
/// was heard, if that was kept too).
fn sung_words(dir: &Path, song: &Path, analysis: &Analysis) -> Option<Vec<TimingTrack>> {
    let cache = LyricsCache::new(dir);
    let hash = pf_ai::lyrics::cache::file_hash(song, &|| false)?;
    // What an earlier version kept (one entry, or a list).
    let earlier = |kind: &str| -> Option<Value> {
        let file = dir.join("lyrics").join(format!("{hash}-{kind}.json"));
        serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
    };
    let published: Vec<Published> = cache.load(&hash, "lrclib").or_else(|| {
        let value = earlier("lrclib")?;
        serde_json::from_value::<Vec<Published>>(value.clone())
            .ok()
            .or_else(|| serde_json::from_value::<Published>(value).ok().map(|p| vec![p]))
    })?;
    let duration_s = analysis.duration_ms as f64 / 1000.0;
    let entry = published.iter().filter(|p| p.synced.is_some()).min_by(|a, b| {
        (a.duration_s - duration_s)
            .abs()
            .total_cmp(&(b.duration_s - duration_s).abs())
    })?;
    let lines = combine::timed_lines(&lrc::parse_lrc(entry.synced.as_deref()?), analysis.duration_ms);
    let heard: Option<Heard> = cache
        .load(&hash, "openai")
        .or_else(|| serde_json::from_value(earlier("openai")?).ok())
        .filter(|h: &Heard| !h.words.is_empty());
    let phrases = match &heard {
        Some(heard) => combine::from_lines_and_heard(&lines, heard, analysis.duration_ms),
        None => combine::from_lines(&lines, &analysis.onsets),
    };
    let words: Vec<(u64, u64)> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| (w.start_ms, w.end_ms))
        .collect();
    let sung = vocals::regions_from_words(&words);
    let made = tracks::lyric_tracks(&phrases, &sung, &analysis.onsets, analysis.duration_ms);
    (!made.is_empty()).then_some(made)
}
