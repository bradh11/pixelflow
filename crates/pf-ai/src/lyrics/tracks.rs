//! Lyrics as timing tracks, laid out as xLights lyric imports are: "Lyrics" (a mark per sung
//! line, kind lyrics), "Lyrics (words)" (a mark per word, kind words), and "Lyrics (phonemes)"
//! (a mark per mouth shape, kind phonemes), so the Faces effect sings them and they export to
//! `.xtiming` together; plus "Lyrics (syllables)" (a mark per sung syllable, for effects that
//! move with each one) and "Vocals" (a mark per sung stretch). Syllables and mouth shapes come
//! from the words ([`super::syllables`]), and can be made again from a words track already
//! there ([`from_words`]).

use super::combine::Phrase;
use super::syllables::sung_marks;
use pf_engine::SequenceEdit;
use pf_sequence::{Mark, TimingKind, TimingTrack, tidy_marks};

pub const LYRICS_TRACK: &str = "Lyrics";
pub const WORDS_TRACK: &str = "Lyrics (words)";
pub const SYLLABLES_TRACK: &str = "Lyrics (syllables)";
pub const PHONEMES_TRACK: &str = "Lyrics (phonemes)";
pub const VOCALS_TRACK: &str = "Vocals";
/// The label on each Vocals mark.
pub const VOCALS_LABEL: &str = "Vocals";

/// What a syllables track's name ends with.
const SYLLABLES: &str = " (syllables)";

/// Whether `track` is a syllables track ("Lyrics (syllables)", "Vocals (syllables)").
pub fn is_syllables(track: &TimingTrack) -> bool {
    track.kind == TimingKind::Custom && track.name.ends_with(SYLLABLES)
}

/// The syllables and phonemes tracks for `words` (named after `lyrics`), syllables nudged onto
/// `onsets` (where the voice starts a note).
fn sung_tracks(lyrics: &str, words: &[Mark], onsets: &[u64], end_ms: u64) -> [TimingTrack; 2] {
    let sung = sung_marks(words, onsets);
    [
        TimingTrack::new(
            format!("{lyrics}{SYLLABLES}"),
            TimingKind::Custom,
            tidy_marks(sung.syllables, end_ms).0,
        ),
        TimingTrack::new(
            format!("{lyrics} (phonemes)"),
            TimingKind::Phonemes,
            tidy_marks(sung.phonemes, end_ms).0,
        ),
    ]
}

/// The Lyrics, Lyrics (words), Lyrics (syllables), Lyrics (phonemes), and Vocals tracks, marks
/// tidied to fit `end_ms`; syllables nudged onto `onsets`.
pub fn lyric_tracks(
    phrases: &[Phrase],
    vocals: &[(u64, u64)],
    onsets: &[u64],
    end_ms: u64,
) -> Vec<TimingTrack> {
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
    let words = TimingTrack::new(WORDS_TRACK, TimingKind::Words, tidy_marks(words, end_ms).0);
    let [syllables, phonemes] = sung_tracks(LYRICS_TRACK, &words.marks, onsets, end_ms);
    vec![
        TimingTrack::new(LYRICS_TRACK, TimingKind::Lyrics, tidy_marks(lines, end_ms).0),
        words,
        syllables,
        phonemes,
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

/// Adds `track`, or updates the one of its name and kind in `existing` (keeping its id, so
/// effects that follow it still do).
fn add_or_update(existing: &[TimingTrack], mut track: TimingTrack) -> SequenceEdit {
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
}

/// The edits that put found tracks (from [`lyric_tracks`]) in a sequence with `existing` tracks:
/// a track already there by name and kind gets the new marks (keeping its id, so effects that
/// sing to it still do); one of another kind is left alone and the found one named apart. The
/// words, syllables, and phonemes tracks stay named after the lyrics track.
pub fn track_edits(existing: &[TimingTrack], found: Vec<TimingTrack>) -> Vec<SequenceEdit> {
    let lyrics_name = free_name(existing, LYRICS_TRACK, TimingKind::Lyrics);
    found
        .into_iter()
        .map(|mut track| {
            let wanted = match track.kind {
                TimingKind::Words => format!("{lyrics_name} (words)"),
                TimingKind::Phonemes => format!("{lyrics_name} (phonemes)"),
                TimingKind::Lyrics => lyrics_name.clone(),
                _ if is_syllables(&track) => format!("{lyrics_name}{SYLLABLES}"),
                _ => track.name.clone(),
            };
            track.name = free_name(existing, &wanted, track.kind);
            add_or_update(existing, track)
        })
        .collect()
}

/// The words track syllables and phonemes are made from: the words track of a lyrics track
/// ("Lyrics (words)" beside "Lyrics"), else any words track with marks.
pub fn words_track(existing: &[TimingTrack]) -> Option<&TimingTrack> {
    let paired = existing.iter().find(|w| {
        w.kind == TimingKind::Words
            && !w.marks.is_empty()
            && existing
                .iter()
                .any(|l| l.kind == TimingKind::Lyrics && w.name == format!("{} (words)", l.name))
    });
    paired.or_else(|| {
        existing
            .iter()
            .find(|t| t.kind == TimingKind::Words && !t.marks.is_empty())
    })
}

/// Syllables and phonemes tracks made again from a words track already in a sequence (see
/// [`words_track`]), without looking the lyrics up again: named after it ("Lyrics
/// (syllables)" for "Lyrics (words)"), each replacing its namesake. Syllables are nudged onto
/// `onsets`. `None` without words.
pub fn from_words(existing: &[TimingTrack], onsets: &[u64], end_ms: u64) -> Option<Vec<SequenceEdit>> {
    let words = words_track(existing)?;
    let base = words.name.strip_suffix(" (words)").unwrap_or(&words.name);
    let edits = sung_tracks(base, &words.marks, onsets, end_ms)
        .into_iter()
        .map(|mut track| {
            track.name = free_name(existing, &track.name, track.kind);
            add_or_update(existing, track)
        })
        .collect();
    Some(edits)
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

    fn names(edits: &[SequenceEdit]) -> Vec<(&str, &str)> {
        edits
            .iter()
            .map(|e| match e {
                SequenceEdit::UpdateTimingTrack { track } => ("update", track.name.as_str()),
                SequenceEdit::AddTimingTrack { track } => ("add", track.name.as_str()),
                _ => ("other", ""),
            })
            .collect()
    }

    #[test]
    fn phrases_words_and_vocals_become_five_tracks() {
        let phrases = vec![Phrase {
            text: "Paper lanterns".into(),
            start_ms: 1_000,
            end_ms: 2_000,
            words: vec![word("Paper", 1_000, 1_400), word("lanterns", 1_400, 2_000)],
        }];
        let tracks = lyric_tracks(&phrases, &[(1_000, 2_500)], &[], 2_200);
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
            shown[0],
            (
                "Lyrics",
                TimingKind::Lyrics,
                vec![(1_000, 2_000, "Paper lanterns")]
            )
        );
        assert_eq!(
            shown[1],
            (
                "Lyrics (words)",
                TimingKind::Words,
                vec![(1_000, 1_400, "Paper"), (1_400, 2_000, "lanterns")]
            )
        );
        // "Pa·per lan·terns", each word's time shared between its two syllables.
        let (name, kind, syllables) = &shown[2];
        assert_eq!((*name, *kind), ("Lyrics (syllables)", TimingKind::Custom));
        let labels: Vec<&str> = syllables.iter().map(|s| s.2).collect();
        assert_eq!(labels, ["Pa", "per", "lan", "terns"]);
        assert_eq!((syllables[0].0, syllables[1].1), (1_000, 1_400));
        assert_eq!((syllables[2].0, syllables[3].1), (1_400, 2_000));
        // P EY P ER: MBP E MBP E.
        let (name, kind, phonemes) = &shown[3];
        assert_eq!((*name, *kind), ("Lyrics (phonemes)", TimingKind::Phonemes));
        let shapes: Vec<&str> = phonemes.iter().take(4).map(|s| s.2).collect();
        assert_eq!(shapes, ["MBP", "E", "MBP", "E"]);
        assert_eq!(phonemes.last().unwrap().1, 2_000);
        // Cut at the end of the sequence.
        assert_eq!(
            shown[4],
            ("Vocals", TimingKind::Custom, vec![(1_000, 2_200, "Vocals")])
        );
    }

    #[test]
    fn found_tracks_update_their_namesakes_and_never_take_another_kinds_name() {
        let found = lyric_tracks(&[], &[(0, 10)], &[], 100);
        // Nothing there yet: all added.
        let edits = track_edits(&[], found.clone());
        assert!(
            edits
                .iter()
                .all(|e| matches!(e, SequenceEdit::AddTimingTrack { .. }))
        );
        // A Lyrics track of the same kind keeps its id, as does a phonemes track from xLights; an
        // xLights "Vocals" lyrics track is left alone.
        let lyrics = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![]);
        let phonemes = TimingTrack::new(
            "Lyrics (phonemes)",
            TimingKind::Phonemes,
            vec![Mark::new(0, 5, "O")],
        );
        let theirs = TimingTrack::new("Vocals", TimingKind::Lyrics, vec![Mark::new(0, 5, "la")]);
        let edits = track_edits(&[lyrics.clone(), phonemes.clone(), theirs], found.clone());
        assert_eq!(
            names(&edits),
            [
                ("update", "Lyrics"),
                ("add", "Lyrics (words)"),
                ("add", "Lyrics (syllables)"),
                ("update", "Lyrics (phonemes)"),
                ("add", "Vocals (found)")
            ]
        );
        assert!(matches!(&edits[0], SequenceEdit::UpdateTimingTrack { track } if track.id == lyrics.id));
        assert!(matches!(&edits[3], SequenceEdit::UpdateTimingTrack { track } if track.id == phonemes.id));
        // A custom track named Lyrics: the found ones go beside it, the rest named to match.
        let custom = TimingTrack::new("Lyrics", TimingKind::Custom, vec![]);
        let edits = track_edits(&[custom], found);
        assert_eq!(
            names(&edits),
            [
                ("add", "Lyrics (found)"),
                ("add", "Lyrics (found) (words)"),
                ("add", "Lyrics (found) (syllables)"),
                ("add", "Lyrics (found) (phonemes)"),
                ("add", "Vocals")
            ]
        );
    }

    #[test]
    fn syllables_and_phonemes_are_made_again_from_words_already_there() {
        let words = TimingTrack::new(
            "Song (words)",
            TimingKind::Words,
            vec![
                Mark::new(1_000, 1_600, "thriller"),
                Mark::new(1_600, 2_000, "night"),
            ],
        );
        let lines = TimingTrack::new(
            "Song",
            TimingKind::Lyrics,
            vec![Mark::new(1_000, 2_000, "thriller night")],
        );
        let old = TimingTrack::new("Song (syllables)", TimingKind::Custom, vec![Mark::new(0, 1, "x")]);
        let existing = [lines, words, old.clone()];
        let edits = from_words(&existing, &[], 10_000).unwrap();
        assert_eq!(
            names(&edits),
            [("update", "Song (syllables)"), ("add", "Song (phonemes)")]
        );
        let SequenceEdit::UpdateTimingTrack { track } = &edits[0] else {
            panic!("{edits:?}");
        };
        assert_eq!(track.id, old.id);
        let labels: Vec<&str> = track.marks.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["thril", "ler", "night"]);
        assert_eq!(track.marks[1].end_ms, 1_600);
        assert!(is_syllables(track));
        // Without words, nothing.
        assert_eq!(from_words(&[], &[], 10_000), None);
        let empty = TimingTrack::new("Lyrics (words)", TimingKind::Words, vec![]);
        assert_eq!(from_words(&[empty], &[], 10_000), None);
    }
}
