//! The real model, only when asked: it's downloaded, never in the repository or CI.
//!
//! `PF_ALIGN_MODEL=/path/to/model_fp16.onnx cargo test -p pf-align --release -- --ignored`
//! checks it loads and hears silence as blanks. With `PF_ALIGN_AUDIO` (a song), `PF_ALIGN_TEXT`
//! (a line of it), and `PF_ALIGN_AT_MS` (about when it's sung), it also lines the line up and
//! prints each word's time.

use pf_align::{Acoustic, Line, Wav2Vec2, vocab};
use std::path::Path;

fn model() -> Option<Wav2Vec2> {
    let path = std::env::var("PF_ALIGN_MODEL").ok()?;
    Some(Wav2Vec2::load(Path::new(&path)).expect("the model loads"))
}

#[test]
#[ignore = "needs the downloaded model (PF_ALIGN_MODEL)"]
fn the_real_model_hears_silence_as_blanks() {
    let Some(model) = model() else {
        eprintln!("PF_ALIGN_MODEL not set; skipped");
        return;
    };
    let e = model.emit(&vec![0.0; 32_000]).unwrap();
    assert_eq!(e.vocab, vocab::SIZE);
    assert_eq!(e.frames(), pf_align::model::frames_for(32_000));
    let blanks = (0..e.frames())
        .filter(|&t| {
            e.at(t, vocab::BLANK)
                >= (0..vocab::SIZE as u32)
                    .map(|v| e.at(t, v))
                    .fold(f32::MIN, f32::max)
        })
        .count();
    assert!(blanks * 10 >= e.frames() * 9, "{blanks} of {}", e.frames());
}

#[test]
#[ignore = "needs the downloaded model and a song (PF_ALIGN_MODEL, PF_ALIGN_AUDIO, PF_ALIGN_TEXT, PF_ALIGN_AT_MS)"]
fn the_real_model_lines_up_a_sung_line() {
    let (Some(model), Ok(audio), Ok(text), Ok(at)) = (
        model(),
        std::env::var("PF_ALIGN_AUDIO"),
        std::env::var("PF_ALIGN_TEXT"),
        std::env::var("PF_ALIGN_AT_MS"),
    ) else {
        eprintln!("PF_ALIGN_MODEL, PF_ALIGN_AUDIO, PF_ALIGN_TEXT, or PF_ALIGN_AT_MS not set; skipped");
        return;
    };
    let at: u64 = at.parse().expect("PF_ALIGN_AT_MS is a number of ms");
    let emission = pf_align::song_emission(&model, Path::new(&audio), &|| false, &|_, _| {}).unwrap();
    let line = Line {
        words: text.split_whitespace().map(str::to_string).collect(),
        start_ms: at,
        end_ms: at + 3_000,
    };
    let timed = pf_align::align_lines(&emission, std::slice::from_ref(&line));
    assert_eq!(timed[0].words.len(), line.words.len());
    for (word, time) in line.words.iter().zip(&timed[0].words) {
        let time = time.as_ref().expect("every word placed");
        eprintln!(
            "{word}: {}–{} ms ({:.2})",
            time.start_ms, time.end_ms, time.confidence
        );
        assert!(time.start_ms + 1_000 >= at && time.start_ms <= at + 4_000);
    }
}
