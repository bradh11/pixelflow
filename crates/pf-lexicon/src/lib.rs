//! How English words are said, for singing faces and lyric timing: a word's phones (ARPAbet,
//! `G OW1 S T`), its syllables with their share of the spelling ("Ghost", "bus", "ters"), and
//! the mouth shapes xLights' singing faces use.
//!
//! - **The CMU Pronouncing Dictionary** ([`dict`]) answers for about 125,000 words. It's
//!   Copyright (C) 1993-2015 Carnegie Mellon University, used under its BSD-style licence
//!   (`data/cmudict-LICENSE.txt`, which travels with the data), from
//!   <https://github.com/cmusphinx/cmudict>. It's embedded packed and compressed (about 670 KB;
//!   `examples/build_cmudict.rs` makes it) and opened the first time a word is looked up.
//! - **Other words** (names, slang, compounds) are worked out: a stretched sung word
//!   ("sooo") shortened, a dictionary word with an ending ("dancin'", "snowmen's"), two or three
//!   dictionary words run together ("ghostbusters"), and failing those the letter rules
//!   ([`rules`]).
//! - **Words in other languages** (accented Latin not in the dictionary, Cyrillic, Greek) are
//!   split by their vowels ([`foreign`]), their phones a rough guess.
//! - **Syllables** ([`syllables`]) split by maximal onset, the vowels their nuclei.
//! - **Mouth shapes** ([`Arpa::mouth`]) follow xLights' `phoneme_mapping` (the Preston Blair
//!   set: AI, E, O, U, WQ, L, MBP, FV, etc).

pub mod dict;
pub mod foreign;
pub mod phones;
pub mod rules;
pub mod syllables;

pub use foreign::is_vowel_letter;
pub use phones::{Arpa, Phone, mouth_shapes, parse_phones, spelled};

use std::ops::Range;

/// Where a pronunciation came from, surest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// The CMU dictionary, as it is or with an ending added.
    Dictionary,
    /// Dictionary words run together.
    Compound,
    /// The letter rules.
    Rules,
}

/// How a word is said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pronunciation {
    pub phones: Vec<Phone>,
    pub source: Source,
}

/// One syllable of a word: its share of the word as written (punctuation included) and its
/// phones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syllable {
    pub text: String,
    pub phones: Vec<Phone>,
}

impl Syllable {
    /// Its vowel (none in a word without one, "hmm").
    pub fn nucleus(&self) -> Option<Phone> {
        self.phones.iter().copied().find(|p| p.is_vowel())
    }

    /// How long it's held compared with its neighbours: diphthongs (AY, OW …) longest, then
    /// long vowels (IY, UW, AA …), short ones, and unstressed reduced ones (AH0) shortest, each
    /// consonant adding a little.
    pub fn weight(&self) -> f64 {
        let vowel = match self.nucleus() {
            None => 0.5,
            Some(p) => {
                let base = match p.arpa {
                    Arpa::Ay | Arpa::Aw | Arpa::Oy | Arpa::Ey | Arpa::Ow => 1.5,
                    Arpa::Iy | Arpa::Uw | Arpa::Aa | Arpa::Ao | Arpa::Er => 1.25,
                    _ => 1.0,
                };
                let reduced = matches!(p.arpa, Arpa::Ah | Arpa::Ih | Arpa::Er | Arpa::Uh | Arpa::Eh);
                match (p.stress, reduced) {
                    (0, true) => base * 0.7,
                    (0, false) => base * 0.85,
                    _ => base,
                }
            }
        };
        let consonants = self.phones.iter().filter(|p| !p.is_vowel()).count();
        vowel + 0.12 * consonants as f64
    }
}

/// A dictionary-shaped key for a letter: lowercase a–z, accents dropped; `None` for anything
/// else.
fn fold(c: char) -> Option<u8> {
    let c = c.to_lowercase().next().unwrap_or(c);
    let folded = match c {
        'a'..='z' => c,
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        _ => return None,
    };
    Some(folded as u8)
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | '‘')
}

/// A part of a word said one way: its letters (indices into the word's letters) and phones.
type Part = (Range<usize>, Vec<Phone>);

/// The dictionary's phones for `key`, or for it with a common ending on a dictionary word:
/// `-s`, `-es`, `'s`, `-ed`, `-ing`, `-in'`, `-er`, `-ers`, `-est`, `-ly`.
fn listed(key: &str) -> Option<Vec<Phone>> {
    if let Some(phones) = dict::lookup(key) {
        return Some(phones);
    }
    let plain: String = key.chars().filter(|&c| c != '\'').collect();
    if plain != key
        && let Some(phones) = dict::lookup(&plain)
    {
        return Some(phones);
    }
    let p = |text: &str| parse_phones(text).unwrap_or_default();
    // "dancin'" and "dancin" are "dancing" with an N.
    for stem in [key.strip_suffix("in'"), plain.strip_suffix("in")]
        .into_iter()
        .flatten()
    {
        if let Some(mut phones) = dict::lookup(&format!("{stem}ing")) {
            if phones.last().is_some_and(|p| p.arpa == Arpa::Ng) {
                phones.pop();
                phones.push(Phone::new(Arpa::N, 0));
            }
            return Some(phones);
        }
    }
    let with = |stem: &str, ending: Vec<Phone>| -> Option<Vec<Phone>> {
        if stem.len() < 3 {
            return None;
        }
        let mut phones = dict::lookup(stem).or_else(|| dict::lookup(&format!("{stem}e")))?;
        phones.extend(ending);
        Some(phones)
    };
    let plural = |stem: &str| -> Option<Vec<Phone>> {
        let last = dict::lookup(stem)?.last().copied()?;
        let ending = match last.arpa {
            Arpa::S | Arpa::Z | Arpa::Sh | Arpa::Zh | Arpa::Ch | Arpa::Jh => p("IH0 Z"),
            Arpa::P | Arpa::T | Arpa::K | Arpa::F | Arpa::Th => p("S"),
            _ => p("Z"),
        };
        with(stem, ending)
    };
    let stem = |suffix: &str| plain.strip_suffix(suffix).filter(|s| !s.is_empty());
    let tries: [Option<Vec<Phone>>; 9] = [
        key.strip_suffix("'s").and_then(plural),
        stem("es").and_then(plural),
        stem("s").and_then(plural),
        stem("ing").and_then(|s| with(s, p("IH0 NG"))),
        stem("ed").and_then(|s| with(s, p("D"))),
        stem("ers").and_then(|s| with(s, p("ER0 Z"))),
        stem("er").and_then(|s| with(s, p("ER0"))),
        stem("est").and_then(|s| with(s, p("AH0 S T"))),
        stem("ly").and_then(|s| with(s, p("L IY0"))),
    ];
    tries.into_iter().flatten().next()
}

/// Shortens a stretched sung word ("sooo", "nooooo", "yeahhh"): runs of three or more of a
/// letter cut to `keep`.
fn unstretched(key: &str, keep: usize) -> String {
    let chars: Vec<char> = key.chars().collect();
    let mut out = String::new();
    for run in chars.chunk_by(|a, b| a == b) {
        let count = if run.len() >= 3 { keep } else { run.len() };
        out.extend(&run[..count]);
    }
    out
}

/// `key` (dictionary-shaped, apostrophes kept) as dictionary words run together, two or three,
/// each at least three letters: the split with the longest shortest part. Parts are by letter
/// (apostrophes not counted).
fn compound(key: &str) -> Option<Vec<Part>> {
    let plain: String = key.chars().filter(|&c| c != '\'').collect();
    let n = plain.len();
    let mut best: Option<(usize, Vec<Part>)> = None;
    let mut consider = |parts: Vec<Part>| {
        let shortest = parts.iter().map(|(r, _)| r.len()).min().unwrap_or(0);
        if best.as_ref().is_none_or(|(s, _)| shortest > *s) {
            best = Some((shortest, parts));
        }
    };
    for a in 3..n.saturating_sub(2) {
        let Some(first) = dict::lookup(&plain[..a]) else {
            continue;
        };
        if let Some(second) = listed(&plain[a..]) {
            consider(vec![(0..a, first.clone()), (a..n, second)]);
            continue;
        }
        for b in a + 3..n.saturating_sub(2) {
            if let (Some(second), Some(third)) = (dict::lookup(&plain[a..b]), listed(&plain[b..])) {
                consider(vec![(0..a, first.clone()), (a..b, second), (b..n, third)]);
            }
        }
    }
    best.map(|(_, parts)| parts)
}

/// How a word not in the dictionary is said (`key`: lowercase a–z and apostrophes), as parts.
fn guess(key: &str) -> (Vec<Part>, Source) {
    let n = key.chars().filter(|&c| c != '\'').count();
    for keep in [1, 2] {
        let shorter = unstretched(key, keep);
        if shorter != key
            && let Some(phones) = listed(&shorter)
        {
            return (vec![(0..n, phones)], Source::Dictionary);
        }
    }
    if let Some(parts) = compound(key) {
        return (parts, Source::Compound);
    }
    let letters: Vec<u8> = key.bytes().filter(|&b| b != b'\'').collect();
    let phones = rules::letter_rules(&letters)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    (vec![(0..n, phones)], Source::Rules)
}

/// How `key` is said, as parts: the dictionary's (perhaps with an ending), else a guess.
fn parts(key: &str, dictionary: bool) -> (Vec<Part>, Source) {
    let n = key.chars().filter(|&c| c != '\'').count();
    if dictionary && let Some(phones) = listed(key) {
        return (vec![(0..n, phones)], Source::Dictionary);
    }
    guess(key)
}

/// A piece of a word between hyphens: its characters, and its letters as (folded letter,
/// character index), apostrophes kept for the dictionary.
type Piece = (Range<usize>, Vec<(u8, usize)>);

/// A word's pieces between hyphens (and slashes).
fn pieces(chars: &[char]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut start = 0;
    for i in 0..=chars.len() {
        let breaks = i == chars.len() || matches!(chars[i], '-' | '–' | '—' | '/');
        if !breaks {
            continue;
        }
        let letters: Vec<(u8, usize)> = (start..i)
            .filter_map(|j| {
                let c = chars[j];
                if is_apostrophe(c) {
                    Some((b'\'', j))
                } else {
                    fold(c).map(|b| (b, j))
                }
            })
            .collect();
        // Apostrophes only inside or at the end ("dancin'"), not quoting the word.
        let first = letters.iter().position(|(b, _)| *b != b'\'');
        let letters = match first {
            Some(f) => letters[f..].to_vec(),
            None => Vec::new(),
        };
        if !letters.is_empty() {
            out.push((start..i + usize::from(i < chars.len()), letters));
        }
        start = i + 1;
    }
    out
}

/// Whether `chars` should be split by its vowels ([`foreign`]): it has a letter beyond a–z and
/// isn't a dictionary word with accents ("Café").
fn sounded_by_vowels(chars: &[char], dictionary: bool) -> bool {
    if !foreign::has_foreign_letters(chars) {
        return false;
    }
    let all_fold = chars
        .iter()
        .filter(|c| c.is_alphabetic())
        .all(|&c| fold(c).is_some());
    let listed_word = dictionary
        && all_fold
        && pieces(chars).iter().all(|(_, letters)| {
            let key: String = letters.iter().map(|&(b, _)| b as char).collect();
            listed(&key).is_some()
        });
    !listed_word
}

/// How `word` is said (any case, punctuation ignored); `None` without letters.
pub fn pronounce(word: &str) -> Option<Pronunciation> {
    let chars: Vec<char> = word.chars().collect();
    if sounded_by_vowels(&chars, true) {
        let phones = foreign::syllables(&chars)?
            .into_iter()
            .flat_map(|(_, p)| p)
            .collect();
        return Some(Pronunciation {
            phones,
            source: Source::Rules,
        });
    }
    let mut phones = Vec::new();
    let mut source = Source::Dictionary;
    let pieces = pieces(&chars);
    if pieces.is_empty() {
        return None;
    }
    for (_, letters) in pieces {
        let key: String = letters.iter().map(|&(b, _)| b as char).collect();
        let (parts, from) = parts(&key, true);
        source = source.max(from);
        phones.extend(parts.into_iter().flat_map(|(_, p)| p));
    }
    Some(Pronunciation { phones, source })
}

/// The mouth shapes a word makes, as xLights breaks it down (see [`mouth_shapes`]); none
/// without letters.
pub fn word_mouths(word: &str) -> Vec<&'static str> {
    pronounce(word).map_or_else(Vec::new, |p| mouth_shapes(&p.phones))
}

/// A word's syllables, in order, their texts together making the word as written ("Ghost",
/// "bus", "ters!"). A word without letters is one syllable with no phones.
pub fn syllables(word: &str) -> Vec<Syllable> {
    syllables_from(word, true)
}

fn syllables_from(word: &str, dictionary: bool) -> Vec<Syllable> {
    let chars: Vec<char> = word.chars().collect();
    let by_vowels = sounded_by_vowels(&chars, dictionary)
        .then(|| foreign::syllables(&chars))
        .flatten();
    let pieces = if by_vowels.is_some() {
        Vec::new()
    } else {
        pieces(&chars)
    };
    if pieces.is_empty() && by_vowels.is_none() {
        return vec![Syllable {
            text: word.to_string(),
            phones: Vec::new(),
        }];
    }
    // Each syllable's phones and the character it starts at.
    let mut found: Vec<(usize, Vec<Phone>)> = by_vowels.unwrap_or_default();
    for (k, (span, letters)) in pieces.iter().enumerate() {
        let key: String = letters.iter().map(|&(b, _)| b as char).collect();
        let plain: Vec<(u8, usize)> = letters.iter().copied().filter(|&(b, _)| b != b'\'').collect();
        let (parts, _) = parts(&key, dictionary);
        for (p, (range, phones)) in parts.iter().enumerate() {
            let spelling: Vec<u8> = plain[range.clone()].iter().map(|&(b, _)| b).collect();
            let split = syllables::syllabify(phones);
            let starts = syllables::spell(&spelling, phones, &split);
            for (s, (phones_at, letter)) in split.iter().zip(starts).enumerate() {
                let at = if s == 0 && p == 0 {
                    // The piece's first syllable takes anything before its letters.
                    if k == 0 { 0 } else { span.start }
                } else {
                    plain.get(range.start + letter).map_or(span.end, |&(_, c)| c)
                };
                found.push((at, phones[phones_at.clone()].to_vec()));
            }
        }
    }
    let mut out: Vec<Syllable> = Vec::new();
    for (i, (at, phones)) in found.iter().enumerate() {
        let end = found.get(i + 1).map_or(chars.len(), |(next, _)| *next);
        let text: String = chars[(*at).min(end)..end].iter().collect();
        match out.last_mut() {
            // Too few letters to go round: shares the one before.
            Some(last) if text.is_empty() => last.phones.extend(phones),
            _ => out.push(Syllable {
                text,
                phones: phones.clone(),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(syllables: &[Syllable]) -> Vec<&str> {
        syllables.iter().map(|s| s.text.as_str()).collect()
    }

    fn said(word: &str) -> String {
        spelled(&pronounce(word).unwrap().phones)
    }

    #[test]
    fn dictionary_words_and_their_endings() {
        assert_eq!(said("Ghost!"), "G OW1 S T");
        assert_eq!(pronounce("Ghost").unwrap().source, Source::Dictionary);
        assert_eq!(said("don’t"), "D OW1 N T");
        assert_eq!(said("sooo"), "S OW1");
        assert_eq!(said("Café"), spelled(&dict::lookup("cafe").unwrap()));
        assert_eq!(pronounce("123"), None);
        assert_eq!(pronounce("…"), None);
    }

    #[test]
    fn unlisted_words_are_split_or_sounded_out() {
        // As if the dictionary didn't have them.
        let guessed = |w: &str| {
            let (parts, source) = guess(w);
            let phones: Vec<Phone> = parts.into_iter().flat_map(|(_, p)| p).collect();
            (spelled(&phones), source)
        };
        assert_eq!(
            guessed("ghostbusters"),
            ("G OW1 S T B AH1 S T ER0 Z".into(), Source::Compound)
        );
        assert_eq!(guessed("gonna"), ("G AA1 N AH0".into(), Source::Rules));
        assert_eq!(guessed("thriller"), ("TH R IH1 L ER0".into(), Source::Rules));
        assert_eq!(guessed("ooh"), ("UW1".into(), Source::Rules));
        // Made-up words.
        assert_eq!(
            said("snorflake").split(' ').filter(|p| p.ends_with('1')).count(),
            1
        );
        assert_eq!(pronounce("glimmerwick").unwrap().source, Source::Compound);
        assert_eq!(pronounce("zibbleflop").unwrap().source, Source::Rules);
        assert_eq!(said("dancin'"), "D AE1 N S IH0 N");
        assert_eq!(said("snowglobes"), "S N OW1 G L OW1 B Z");
    }

    #[test]
    fn unlisted_words_get_syllables_too() {
        let guessed = |w: &str| syllables_from(w, false);
        assert_eq!(texts(&guessed("ghostbusters")), ["ghost", "bus", "ters"]);
        assert_eq!(texts(&guessed("thriller")), ["thril", "ler"]);
        assert_eq!(texts(&guessed("gonna")), ["gon", "na"]);
        assert_eq!(texts(&guessed("ooh")), ["ooh"]);
        assert_eq!(texts(&guessed("jingle")), ["jin", "gle"]);
        assert_eq!(texts(&guessed("tones")), ["tones"]);
    }

    #[test]
    fn syllables_keep_the_word_as_written() {
        let s = syllables("Ghostbusters!");
        assert_eq!(texts(&s), ["Ghost", "bus", "ters!"]);
        assert_eq!(spelled(&s[1].phones), "B AH2 S");
        assert_eq!(syllables("beautiful").len(), 3);
        assert_eq!(syllables("fire").len(), 2, "CMU says F AY1 ER0");
        assert_eq!(texts(&syllables("everything")), ["eve", "ry", "thing"]);
        assert_eq!(texts(&syllables("\"Snow-white,")), ["\"Snow-", "white,"]);
        assert_eq!(texts(&syllables("don't")), ["don't"]);
        assert_eq!(texts(&syllables("hmm")), ["hmm"]);
        assert_eq!(texts(&syllables("42")), ["42"]);
        assert!(syllables("42")[0].phones.is_empty());
        for word in [
            "Ghostbusters!",
            "everything",
            "rock'n'roll",
            "a",
            "x-mas",
            "Noël",
            "«молоко»,",
            "corazón!",
        ] {
            assert_eq!(
                syllables(word)
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<String>(),
                word
            );
        }
    }

    #[test]
    fn other_languages_split_by_their_vowels() {
        assert_eq!(texts(&syllables("привет")), ["при", "вет"]);
        assert_eq!(texts(&syllables("Молоко,")), ["Мо", "ло", "ко,"]);
        assert_eq!(texts(&syllables("моя")), ["мо", "я"]);
        assert_eq!(syllables("здравствуйте").len(), 3);
        assert_eq!(texts(&syllables("corazón")), ["co", "ra", "zón"]);
        assert_eq!(texts(&syllables("canción")), ["can", "ción"]);
        assert_eq!(texts(&syllables("mañana")), ["ma", "ña", "na"]);
        assert_eq!(texts(&syllables("Mädchen")), ["Mäd", "chen"]);
        assert_eq!(texts(&syllables("καλημέρα")), ["κα", "λη", "μέ", "ρα"]);
        // Each has a vowel to sing, and mouth shapes.
        assert!(syllables("молоко").iter().all(|s| s.nucleus().is_some()));
        assert!(!word_mouths("привет").is_empty());
        // English stays as it was, accented dictionary words too.
        assert_eq!(texts(&syllables("Ghostbusters!")), ["Ghost", "bus", "ters!"]);
        assert_eq!(said("Café"), spelled(&dict::lookup("cafe").unwrap()));
    }

    #[test]
    fn long_vowels_weigh_more_than_reduced_ones() {
        let s = syllables("beautiful");
        // beau (B Y UW1) > ti (T AH0) < ful (F AH0 L).
        assert!(s[0].weight() > s[1].weight());
        assert!(s[2].weight() > s[1].weight());
        let fire = syllables("fire");
        assert!(fire[0].weight() > fire[1].weight());
    }

    #[test]
    fn mouths_come_from_the_dictionary() {
        // "moon": M UW N.
        assert_eq!(word_mouths("Moon"), ["MBP", "U", "etc"]);
        assert_eq!(word_mouths("ghost"), ["etc", "O", "etc"]);
        assert!(word_mouths("...").is_empty());
    }
}
