//! Lyrics as timing tracks, laid out as xLights lyric imports are: "Lyrics" (a mark per sung
//! line, kind lyrics) and "Lyrics (words)" (a mark per word, kind words), so the Faces effect
//! sings them and they export to `.xtiming` together; plus "Vocals" (a mark per sung stretch).
//! There's no phonemes track: the Faces effect works the mouth shapes out from the words.

use super::combine::Phrase;
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
}
