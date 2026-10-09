//! Choosing the published lyrics that are the song, among LRCLIB's candidates. With only a title
//! to go by, a search also finds covers, other songs of the same name, and other languages.
//!
//! Each candidate that could be the song ([`lrclib::score`]: name and length) is scored again:
//!
//! - **Words heard in common** (when the recognizer ran): the share of the heard words in the
//!   candidate's lyrics and of its words heard, weighted highest, since a cover in another
//!   language or another song of the same name shares few.
//! - **Language**: the candidate's lyrics in the language heard (or, without the recognizer,
//!   the one expected) score more, in another one less.
//! - **The same title exactly** scores a little more.
//!
//! Synced lyrics are already preferred by [`lrclib::score`]. The best [`KEPT`] are kept, so the
//! user can pick another.

use super::combine::normalize;
use super::language;
use super::lrc;
use super::lrclib::{self, Published, SongQuery};
use super::transcribe::Heard;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How many candidates are kept.
pub const KEPT: usize = 5;
/// What words heard in common are worth, all of them in common.
const OVERLAP_WEIGHT: f64 = 10.0;
/// What lyrics in the right language are worth (and in another, cost).
const LANGUAGE_WEIGHT: f64 = 2.0;
/// What the same title exactly is worth.
const EXACT_TITLE: f64 = 0.5;

/// Published lyrics that could be the song.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub entry: Published,
    /// The language its lyrics are in, when it can be told.
    pub language: Option<String>,
    pub score: f64,
}

/// An entry's lyrics as plain text, one line each (the synced ones without their stamps).
pub fn lyrics_text(entry: &Published) -> String {
    if let Some(plain) = &entry.plain {
        return plain.clone();
    }
    entry
        .synced
        .as_deref()
        .map(|synced| {
            lrc::parse_lrc(synced)
                .into_iter()
                .map(|l| l.text)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn word_set(words: impl Iterator<Item = String>) -> HashSet<String> {
    words.map(|w| normalize(&w)).filter(|w| !w.is_empty()).collect()
}

/// How much of what was heard is in `lyrics`, 0–1: the share of the heard words found in them,
/// and of their different words heard, averaged.
pub fn overlap(heard: &Heard, lyrics: &str) -> f64 {
    let heard_words: Vec<String> = heard
        .words
        .iter()
        .map(|w| normalize(&w.text))
        .filter(|w| !w.is_empty())
        .collect();
    let heard_set: HashSet<String> = heard_words.iter().cloned().collect();
    let published = word_set(lyrics.split_whitespace().map(str::to_string));
    if heard_words.is_empty() || published.is_empty() {
        return 0.0;
    }
    let found =
        heard_words.iter().filter(|w| published.contains(*w)).count() as f64 / heard_words.len() as f64;
    let sung = published.iter().filter(|w| heard_set.contains(*w)).count() as f64 / published.len() as f64;
    (found + sung) / 2.0
}

/// The candidates among `entries` best first, at most [`KEPT`]. `expected`: the language the
/// song is thought to be in; what the recognizer heard, when it ran, says more.
pub fn rank(
    query: &SongQuery,
    entries: Vec<Published>,
    expected: &str,
    heard: Option<&Heard>,
) -> Vec<Candidate> {
    let heard = heard.filter(|h| !h.words.is_empty());
    let target = heard
        .and_then(|h| language::detect(&h.text()))
        .unwrap_or(expected);
    let mut ranked: Vec<Candidate> = entries
        .into_iter()
        .filter_map(|entry| {
            let base = lrclib::score(query, &entry)?;
            let text = lyrics_text(&entry);
            let language = language::detect(&text).map(str::to_string);
            let mut score = base;
            if lrclib::similarity(&query.title, &entry.title) == 1.0 {
                score += EXACT_TITLE;
            }
            score += match language.as_deref() {
                Some(l) if l == target => LANGUAGE_WEIGHT,
                Some(_) => -LANGUAGE_WEIGHT,
                None => 0.0,
            };
            if let Some(heard) = heard {
                score += OVERLAP_WEIGHT * overlap(heard, &text);
            }
            Some(Candidate {
                entry,
                language,
                score,
            })
        })
        .collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    ranked.truncate(KEPT);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::transcribe::HeardWord;

    /// Made-up lyrics: an English song and a Russian cover of it the same length.
    const ENGLISH: &str = "[00:01.00]Paper lanterns glowing in the night\n[00:05.00]You know the snow is falling and it's bright";
    const RUSSIAN: &str = "[00:01.00]Привет молоко привет\n[00:05.00]Молоко и снег и свет";

    fn entry(id: i64, artist: &str, synced: &str) -> Published {
        Published {
            id,
            artist: artist.into(),
            title: "Lantern Song".into(),
            duration_s: 200.0,
            instrumental: false,
            synced: Some(synced.into()),
            plain: None,
        }
    }

    fn query() -> SongQuery {
        SongQuery {
            artist: None,
            title: "lantern song".into(),
            album: None,
            duration_s: Some(200.5),
        }
    }

    fn heard(text: &str) -> Heard {
        Heard {
            words: text
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| HeardWord {
                    text: w.into(),
                    start_ms: i as u64 * 300,
                    end_ms: i as u64 * 300 + 250,
                })
                .collect(),
            ..Heard::default()
        }
    }

    #[test]
    fn the_original_beats_a_cover_in_another_language() {
        // The cover first, as a search might list it.
        let entries = vec![entry(2, "Cover Band", RUSSIAN), entry(1, "Lantern Band", ENGLISH)];
        // Without the recognizer: English expected.
        let ranked = rank(&query(), entries.clone(), "en", None);
        assert_eq!(ranked[0].entry.id, 1);
        assert_eq!(ranked[0].language.as_deref(), Some("en"));
        assert_eq!(ranked[1].language.as_deref(), Some("ru"));
        // Russian expected (the setting), nothing heard: the cover.
        assert_eq!(rank(&query(), entries.clone(), "ru", None)[0].entry.id, 2);
        // The recognizer heard the English words: they decide, whatever was expected.
        let english = heard("paper lanterns glowing in the night you know the snow is falling");
        assert_eq!(
            rank(&query(), entries.clone(), "ru", Some(&english))[0].entry.id,
            1
        );
        let russian = heard("привет молоко привет молоко и снег");
        assert_eq!(rank(&query(), entries, "en", Some(&russian))[0].entry.id, 2);
    }

    #[test]
    fn words_in_common_beat_a_closer_length() {
        let mut other = entry(
            3,
            "Someone Else",
            "[00:01.00]Rooftop waltz is turning\n[00:04.00]Round and round",
        );
        other.duration_s = 200.5;
        let mut original = entry(1, "Lantern Band", ENGLISH);
        original.duration_s = 202.5;
        let english = heard("paper lanterns glowing in the night");
        let ranked = rank(&query(), vec![other, original], "en", Some(&english));
        assert_eq!(ranked[0].entry.id, 1);
        assert!(overlap(&english, &lyrics_text(&ranked[0].entry)) > 0.5);
        assert_eq!(overlap(&english, &lyrics_text(&ranked[1].entry)), 0.0);
    }

    #[test]
    fn at_most_five_are_kept() {
        let entries: Vec<Published> = (0..9).map(|i| entry(i, "Lantern Band", ENGLISH)).collect();
        assert_eq!(rank(&query(), entries, "en", None).len(), KEPT);
    }
}
