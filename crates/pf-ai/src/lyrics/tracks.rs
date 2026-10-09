//! Lyrics as timing tracks, laid out as xLights lyric imports are: "Lyrics" (a mark per sung
//! line, kind lyrics) and "Lyrics (words)" (a mark per word, kind words), so the Faces effect
//! sings them and they export to `.xtiming` together; plus "Vocals" (a mark per sung stretch).
//! No phonemes track is made (the Faces effect works the mouth shapes out from the words), but
//! one already there from xLights is kept in time with the new words.

use super::combine::Phrase;
use pf_engine::SequenceEdit;
use pf_sequence::{Mark, TimingKind, TimingTrack, tidy_marks};

pub const LYRICS_TRACK: &str = "Lyrics";
pub const WORDS_TRACK: &str = "Lyrics (words)";
pub const VOCALS_TRACK: &str = "Vocals";
/// The label on each Vocals mark.
pub const VOCALS_LABEL: &str = "Vocals";

/// The Lyrics, Lyrics (words), and Vocals tracks, marks tidied to fit `end_ms`.
pub fn lyric_tracks(phrases: &[Phrase], vocals: &[(u64, u64)], end_ms: u64) -> Vec<TimingTrack> {
    let lines = phrases
        .iter()
        .map(|p| Mark::new(p.start_ms, p.end_ms, p.text.clone()))
        .collect();
    let words = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| Mark::new(w.start_ms, w.end_ms, w.text.clone()))
        .collect();
    let sung = vocals
        .iter()
        .map(|&(s, e)| Mark::new(s, e, VOCALS_LABEL))
        .collect();
    vec![
        TimingTrack::new(LYRICS_TRACK, TimingKind::Lyrics, tidy_marks(lines, end_ms).0),
        TimingTrack::new(WORDS_TRACK, TimingKind::Words, tidy_marks(words, end_ms).0),
        TimingTrack::new(VOCALS_TRACK, TimingKind::Custom, tidy_marks(sung, end_ms).0),
    ]
}

/// The name a found track goes under among `existing`: its own, unless a track of another kind
/// has it (an xLights "Vocals" lyrics track, say), then "<name> (found)".
fn free_name(existing: &[TimingTrack], name: &str, kind: TimingKind) -> String {
    let mut name = name.to_string();
    while existing.iter().any(|t| t.name == name && t.kind != kind) {
        name = format!("{name} (found)");
    }
    name
}

/// Mouth shapes for words: each word's time shared evenly by the shapes its letters make (the
/// Faces effect's own guess for words, [`pf_render::faces::word_phonemes`]).
pub fn phoneme_marks(words: &[Mark]) -> Vec<Mark> {
    let mut out = Vec::new();
    for word in words {
        let shapes = pf_render::faces::word_phonemes(&word.label);
        let length = word.end_ms.saturating_sub(word.start_ms);
        let count = (shapes.len() as u64).min(length);
        for (k, shape) in shapes.iter().take(count as usize).enumerate() {
            let k = k as u64;
            let start = word.start_ms + length * k / count;
            let end = word.start_ms + length * (k + 1) / count;
            out.push(Mark::new(start, end, shape.xlights_name()));
        }
    }
    out
}

/// The edits that put found tracks (from [`lyric_tracks`]) in a sequence with `existing` tracks:
/// a track already there by name and kind gets the new marks (keeping its id, so effects that
/// sing to it still do); one of another kind is left alone and the found one named apart. The
/// words track stays named after the lyrics track. A phonemes track already beside the lyrics
/// (from xLights) follows the new words, so a face singing to it stays in time; none is made
/// otherwise, as faces sing straight from the words.
pub fn track_edits(existing: &[TimingTrack], found: Vec<TimingTrack>) -> Vec<SequenceEdit> {
    let lyrics_name = free_name(existing, LYRICS_TRACK, TimingKind::Lyrics);
    let phonemes = existing
        .iter()
        .find(|t| t.kind == TimingKind::Phonemes && t.name == format!("{lyrics_name} (phonemes)"));
    let follow = phonemes
        .zip(found.iter().find(|t| t.kind == TimingKind::Words))
        .map(|(had, words)| {
            let mut track = had.clone();
            track.marks = phoneme_marks(&words.marks);
            SequenceEdit::UpdateTimingTrack { track }
        });
    found
        .into_iter()
        .map(|mut track| {
            let wanted = match track.kind {
                TimingKind::Words => format!("{lyrics_name} (words)"),
                TimingKind::Lyrics => lyrics_name.clone(),
                _ => track.name.clone(),
            };
            track.name = free_name(existing, &wanted, track.kind);
            match existing
                .iter()
                .find(|t| t.name == track.name && t.kind == track.kind)
            {
                Some(had) => {
                    track.id = had.id;
                    SequenceEdit::UpdateTimingTrack { track }
                }
                None => SequenceEdit::AddTimingTrack { track },
            }
        })
        .chain(follow)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::combine::{Word, WordSource};

    fn word(text: &str, start_ms: u64, end_ms: u64) -> Word {
        Word {
            text: text.into(),
            start_ms,
            end_ms,
            source: WordSource::Heard,
            confidence: 0.7,
        }
    }

    #[test]
    fn phrases_words_and_vocals_become_three_tracks() {
        let phrases = vec![Phrase {
            text: "Paper lanterns".into(),
            start_ms: 1_000,
            end_ms: 2_000,
            words: vec![word("Paper", 1_000, 1_400), word("lanterns", 1_400, 2_000)],
        }];
        let tracks = lyric_tracks(&phrases, &[(1_000, 2_500)], 2_200);
        type Shown<'a> = (&'a str, TimingKind, Vec<(u64, u64, &'a str)>);
        let shown: Vec<Shown> = tracks
            .iter()
            .map(|t| {
                (
                    t.name.as_str(),
                    t.kind,
                    t.marks
                        .iter()
                        .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "Lyrics",
                    TimingKind::Lyrics,
                    vec![(1_000, 2_000, "Paper lanterns")]
                ),
                (
                    "Lyrics (words)",
                    TimingKind::Words,
                    vec![(1_000, 1_400, "Paper"), (1_400, 2_000, "lanterns")]
                ),
                // Cut at the end of the sequence.
                ("Vocals", TimingKind::Custom, vec![(1_000, 2_200, "Vocals")]),
            ]
        );
    }

    #[test]
    fn found_tracks_update_their_namesakes_and_never_take_another_kinds_name() {
        let found = lyric_tracks(&[], &[(0, 10)], 100);
        // Nothing there yet: all added.
        let edits = track_edits(&[], found.clone());
        assert!(
            edits
                .iter()
                .all(|e| matches!(e, SequenceEdit::AddTimingTrack { .. }))
        );
        // A Lyrics track of the same kind keeps its id; an xLights "Vocals" lyrics track is left
        // alone.
        let lyrics = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![]);
        let theirs = TimingTrack::new("Vocals", TimingKind::Lyrics, vec![Mark::new(0, 5, "la")]);
        let edits = track_edits(&[lyrics.clone(), theirs], found.clone());
        let shown: Vec<(&str, &str)> = edits
            .iter()
            .map(|e| match e {
                SequenceEdit::UpdateTimingTrack { track } => ("update", track.name.as_str()),
                SequenceEdit::AddTimingTrack { track } => ("add", track.name.as_str()),
                _ => ("other", ""),
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("update", "Lyrics"),
                ("add", "Lyrics (words)"),
                ("add", "Vocals (found)")
            ]
        );
        assert!(matches!(&edits[0], SequenceEdit::UpdateTimingTrack { track } if track.id == lyrics.id));
        // A custom track named Lyrics: the found ones go beside it, words named to match.
        let custom = TimingTrack::new("Lyrics", TimingKind::Custom, vec![]);
        let names: Vec<String> = track_edits(&[custom], found)
            .into_iter()
            .filter_map(|e| match e {
                SequenceEdit::AddTimingTrack { track } => Some(track.name),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["Lyrics (found)", "Lyrics (found) (words)", "Vocals"]);
    }

    #[test]
    fn a_phonemes_track_already_there_follows_the_new_words() {
        assert_eq!(
            phoneme_marks(&[Mark::new(1_000, 1_600, "Paper"), Mark::new(2_000, 2_001, "glow")]),
            [
                Mark::new(1_000, 1_120, "MBP"),
                Mark::new(1_120, 1_240, "AI"),
                Mark::new(1_240, 1_360, "MBP"),
                Mark::new(1_360, 1_480, "E"),
                Mark::new(1_480, 1_600, "etc"),
                // Too short to share: its first shape only.
                Mark::new(2_000, 2_001, "etc"),
            ]
        );
        let phrases = vec![Phrase {
            text: "Paper".into(),
            start_ms: 1_000,
            end_ms: 1_600,
            words: vec![word("Paper", 1_000, 1_600)],
        }];
        let found = lyric_tracks(&phrases, &[], 10_000);
        let lyrics = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![]);
        let phonemes = TimingTrack::new(
            "Lyrics (phonemes)",
            TimingKind::Phonemes,
            vec![Mark::new(0, 5, "O")],
        );
        let edits = track_edits(&[lyrics, phonemes.clone()], found.clone());
        let Some(SequenceEdit::UpdateTimingTrack { track }) = edits.last() else {
            panic!("{edits:?}");
        };
        assert_eq!(track.id, phonemes.id);
        assert_eq!(track.marks.len(), 5);
        // Without one, none is made.
        assert_eq!(track_edits(&[], found).len(), 3);
    }
}
