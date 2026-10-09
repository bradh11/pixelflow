//! Syllables: phones split by maximal onset (each vowel a syllable's nucleus; the consonants
//! between two go to the second as far as English lets a syllable start, "ghost·bus·ters"),
//! and each syllable's share of the word's spelling, matched up through the letter rules.

use crate::phones::{Arpa, Phone};
use crate::rules::letter_rules;
use std::ops::Range;

use Arpa::*;

/// Consonant groups a syllable can start with, besides any one consonant but NG.
const ONSETS: &[&[Arpa]] = &[
    &[P, R],
    &[P, L],
    &[B, R],
    &[B, L],
    &[T, R],
    &[D, R],
    &[K, R],
    &[K, L],
    &[G, R],
    &[G, L],
    &[F, R],
    &[F, L],
    &[Th, R],
    &[Sh, R],
    &[S, P],
    &[S, T],
    &[S, K],
    &[S, M],
    &[S, N],
    &[S, L],
    &[S, W],
    &[S, F],
    &[T, W],
    &[D, W],
    &[K, W],
    &[G, W],
    &[Th, W],
    &[P, Y],
    &[B, Y],
    &[F, Y],
    &[M, Y],
    &[K, Y],
    &[V, Y],
    &[Hh, Y],
    &[S, P, R],
    &[S, P, L],
    &[S, T, R],
    &[S, K, R],
    &[S, K, W],
    &[S, P, Y],
    &[S, K, Y],
];

fn onset_ok(consonants: &[Arpa]) -> bool {
    match consonants {
        [] => true,
        [one] => *one != Ng,
        _ => ONSETS.contains(&consonants),
    }
}

/// A short vowel that's stressed holds on to a consonant ("bus·ters", "thril·ler"): English
/// doesn't end a stressed syllable on one.
fn checked(phone: Phone) -> bool {
    phone.stress > 0 && matches!(phone.arpa, Ih | Eh | Ae | Ah | Uh)
}

/// The syllables of `phones`, as ranges of them: one per vowel, the consonants between two
/// vowels split by maximal onset. Phones without a vowel are one syllable.
pub fn syllabify(phones: &[Phone]) -> Vec<Range<usize>> {
    let nuclei: Vec<usize> = (0..phones.len()).filter(|&i| phones[i].is_vowel()).collect();
    if nuclei.is_empty() {
        let all = 0..phones.len();
        return if all.is_empty() {
            Vec::new()
        } else {
            std::iter::once(all).collect()
        };
    }
    let mut starts = vec![0];
    for pair in nuclei.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let between: Vec<Arpa> = phones[a + 1..b].iter().map(|p| p.arpa).collect();
        let mut start = (a + 1..=b)
            .find(|&k| onset_ok(&between[k - a - 1..]))
            .unwrap_or(b);
        if start == a + 1 && b > a + 1 && checked(phones[a]) {
            start += 1;
        }
        starts.push(start);
    }
    let mut out: Vec<Range<usize>> = starts.windows(2).map(|w| w[0]..w[1]).collect();
    out.push(*starts.last().unwrap_or(&0)..phones.len());
    out
}

/// Whether the vowel `arpa` is often spelled starting with `letter`.
fn spelled_with(arpa: Arpa, letter: u8) -> bool {
    let letters: &[u8] = match arpa {
        Aa | Ao => b"ao",
        Ae => b"a",
        Ah => b"aeiouy",
        Aw => b"oa",
        Ay => b"iyea",
        Eh => b"ea",
        Er => b"eiuoya",
        Ey => b"ae",
        Ih => b"iyeu",
        Iy => b"eyi",
        Ow => b"oe",
        Oy => b"o",
        Uh => b"ou",
        Uw => b"oue",
        _ => b"",
    };
    letters.contains(&letter)
}

fn is_vowel_letter(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

/// Letters that sound as one consonant: `th`, `sh`, `ch`, `ph`, `gh`, `ck`, `wh`, `qu`,
/// `tch`, and doubled letters. A vowel letter among them (a silent one) joins the one before.
fn consonant_units(letters: &[u8], range: Range<usize>) -> Vec<Range<usize>> {
    let mut units: Vec<Range<usize>> = Vec::new();
    let mut i = range.start;
    while i < range.end {
        let c = letters[i];
        if is_vowel_letter(c) && !units.is_empty() {
            if let Some(last) = units.last_mut() {
                last.end = i + 1;
            }
            i += 1;
            continue;
        }
        let rest = &letters[i..range.end];
        let used = if rest.starts_with(b"tch") {
            3
        } else if rest.len() >= 2
            && (rest[0] == rest[1]
                || [b"th", b"sh", b"ch", b"ph", b"gh", b"ck", b"wh", b"qu"].contains(&&[rest[0], rest[1]]))
        {
            2
        } else {
            1
        };
        units.push(i..i + used);
        i += used;
    }
    units
}

/// Where each syllable of `phones` (split as `syllables`) starts in `letters` (lowercase a–z):
/// the vowels matched to the letter rules' vowels, the consonants between shared out among the
/// letters between, and a doubled consonant split down the middle ("thril·ler"). Each syllable
/// gets at least one letter when there are enough.
pub fn spell(letters: &[u8], phones: &[Phone], syllables: &[Range<usize>]) -> Vec<usize> {
    let n = letters.len();
    let k = syllables.len();
    if k == 0 {
        return Vec::new();
    }
    let start_of = phone_letters(letters, phones);
    let mut starts: Vec<usize> = syllables.iter().map(|s| start_of[s.start]).collect();
    starts[0] = 0;
    let doubled = |i: usize| i + 1 < n && letters[i] == letters[i + 1] && !is_vowel_letter(letters[i]);
    for start in starts.iter_mut().skip(1) {
        let b = *start;
        if b >= 2 && doubled(b - 2) {
            *start = b - 1;
        } else if doubled(b) {
            *start = b + 1;
        }
    }
    // In order, each with a letter where there are enough.
    for s in 1..k {
        starts[s] = starts[s].max(starts[s - 1] + 1).min(n);
    }
    for s in (1..k).rev() {
        let room = n.saturating_sub(k - s);
        if starts[s] > room {
            starts[s] = room.max(starts[s - 1]);
        }
    }
    starts
}

/// Where each of `phones` starts in `letters` (lowercase a–z), as an index: the vowels matched
/// to the letter rules' vowels, the consonants between shared out among the letters between.
/// Letters left over (silent ones) go with the phone before them; a phone with no letter of its
/// own ("-er" of "fire") starts where the one before it does.
pub fn phone_letters(letters: &[u8], phones: &[Phone]) -> Vec<usize> {
    let n = letters.len();
    let ruled: Vec<Range<usize>> = letter_rules(letters)
        .into_iter()
        .filter(|(p, _)| p.is_vowel())
        .map(|(_, r)| r)
        .collect();
    let vowels: Vec<usize> = (0..phones.len()).filter(|&i| phones[i].is_vowel()).collect();
    let matched = align(letters, phones, &vowels, &ruled);
    // Where each phone starts in the letters: matched vowels where they were found, the rest
    // spread over the letters between.
    let mut anchors: Vec<(isize, Range<usize>)> = vec![(-1, 0..0)];
    anchors.extend(
        matched
            .iter()
            .map(|&(v, r)| (vowels[v] as isize, ruled[r].clone())),
    );
    anchors.push((phones.len() as isize, n..n));
    let mut start_of = vec![0; phones.len()];
    for pair in anchors.windows(2) {
        let ((pa, a), (pb, b)) = (&pair[0], &pair[1]);
        if *pa >= 0 {
            start_of[*pa as usize] = a.start;
        }
        let first = (pa + 1) as usize;
        let count = (pb - pa - 1) as usize;
        if count == 0 {
            continue;
        }
        let gap = a.end.min(b.start)..b.start;
        let units = consonant_units(letters, gap.clone());
        // Letters left over are silent, and most often end a syllable ("Christ·mas", "is·land"),
        // so the phones take the last ones; with too few, the last phones take a letter each
        // (within a unit, from its start) and the first share what's left.
        let extra = units.len().checked_sub(count);
        for t in 0..count {
            start_of[first + t] = match extra {
                Some(extra) => units[t + extra].start,
                None => {
                    let at = gap.end.saturating_sub(count - t).max(gap.start);
                    units.iter().find(|u| u.contains(&at)).map_or(at, |u| u.start)
                }
            };
        }
    }
    for i in 1..start_of.len() {
        start_of[i] = start_of[i].max(start_of[i - 1]).min(n);
    }
    start_of
}

/// Matches the dictionary's vowels (indices into `phones`, by `vowels`) with the letter rules'
/// vowels (their letters, `ruled`), keeping order, by the fewest mismatches: a rule vowel left
/// over (a silent e, most often) or a dictionary vowel without letters (the "-er" of "fire")
/// costs more than a vowel spelled unusually. Answers (dictionary vowel, rule vowel) pairs.
fn align(letters: &[u8], phones: &[Phone], vowels: &[usize], ruled: &[Range<usize>]) -> Vec<(usize, usize)> {
    let (v, r) = (vowels.len(), ruled.len());
    let first = |j: usize| letters[ruled[j].start];
    let pair = |i: usize, j: usize| {
        if spelled_with(phones[vowels[i]].arpa, first(j)) {
            0.0
        } else {
            0.6
        }
    };
    let skip_letters = |j: usize| if first(j) == b'e' { 0.8 } else { 1.0 };
    // A stressed vowel is always written.
    let no_letters = |i: usize| if phones[vowels[i]].stress > 0 { 1.2 } else { 1.0 };
    let mut cost = vec![vec![f64::INFINITY; r + 1]; v + 1];
    cost[0][0] = 0.0;
    for i in 0..=v {
        for j in 0..=r {
            let here = cost[i][j];
            if i < v && j < r {
                let c = here + pair(i, j);
                if c < cost[i + 1][j + 1] {
                    cost[i + 1][j + 1] = c;
                }
            }
            if j < r {
                let c = here + skip_letters(j);
                if c < cost[i][j + 1] {
                    cost[i][j + 1] = c;
                }
            }
            if i < v {
                let c = here + no_letters(i);
                if c < cost[i + 1][j] {
                    cost[i + 1][j] = c;
                }
            }
        }
    }
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let mut out = Vec::new();
    let (mut i, mut j) = (v, r);
    // From the end; on a tie the later letters are the ones left over, so a vowel takes the
    // first of two that could spell it ("ev·e·ry" is EH V R IY: the second e is silent).
    while i > 0 || j > 0 {
        if j > 0 && close(cost[i][j], cost[i][j - 1] + skip_letters(j - 1)) {
            j -= 1;
        } else if i > 0 && j > 0 && close(cost[i][j], cost[i - 1][j - 1] + pair(i - 1, j - 1)) {
            out.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else {
            i -= 1;
        }
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phones::parse_phones;

    fn split(word: &str, phones: &str) -> Vec<String> {
        let phones = parse_phones(phones).unwrap();
        let syllables = syllabify(&phones);
        let starts = spell(word.as_bytes(), &phones, &syllables);
        let mut ends = starts[1..].to_vec();
        ends.push(word.len());
        starts
            .iter()
            .zip(ends)
            .map(|(&s, e)| word[s..e].to_string())
            .collect()
    }

    #[test]
    fn maximal_onset_with_short_stressed_vowels_closed() {
        let phones = parse_phones("G OW1 S T B AH2 S T ER0 Z").unwrap();
        assert_eq!(syllabify(&phones), [0..4, 4..7, 7..10]);
        // "beautiful": the t starts the second syllable (UW is long).
        assert_eq!(
            syllabify(&parse_phones("B Y UW1 T AH0 F AH0 L").unwrap()),
            [0..3, 3..5, 5..8]
        );
        assert_eq!(syllabify(&parse_phones("HH M").unwrap()), vec![0..2usize; 1]);
        assert!(syllabify(&[]).is_empty());
    }

    #[test]
    fn syllables_get_their_letters() {
        assert_eq!(
            split("ghostbusters", "G OW1 S T B AH2 S T ER0 Z"),
            ["ghost", "bus", "ters"]
        );
        assert_eq!(split("thriller", "TH R IH1 L ER0"), ["thril", "ler"]);
        assert_eq!(split("beautiful", "B Y UW1 T AH0 F AH0 L"), ["beau", "ti", "ful"]);
        assert_eq!(
            split("everything", "EH1 V R IY0 TH IH2 NG"),
            ["eve", "ry", "thing"]
        );
        assert_eq!(split("fire", "F AY1 ER0"), ["fi", "re"]);
        assert_eq!(split("hello", "HH AH0 L OW1"), ["hel", "lo"]);
        // Fewer letters than syllables can't give each one.
        assert_eq!(split("mr", "M IH1 S T ER0"), ["m", "r"]);
    }
}
