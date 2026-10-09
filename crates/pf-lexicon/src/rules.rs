//! Letter rules: how an English word the dictionary doesn't have is probably said, from common
//! spelling patterns (`tch`, `igh`, a silent final e that makes the vowel before it long, `-le`
//! as its own syllable, `-ed` and `-es` that are or aren't). A guess, right for most everyday
//! words and close for names; every phone keeps the letters it came from.

use crate::phones::{Arpa, Phone};
use std::ops::Range;

/// A phone from the rules and the letters (indices into the word) it came from.
pub type Ruled = (Phone, Range<usize>);

fn is_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u')
}

struct Word<'a> {
    w: &'a [u8],
}

impl Word<'_> {
    fn at(&self, i: usize) -> Option<u8> {
        self.w.get(i).copied()
    }

    fn len(&self) -> usize {
        self.w.len()
    }

    /// A `y` sounds as a vowel unless it starts the word or a vowel follows it.
    fn y_vowel(&self, i: usize) -> bool {
        self.at(i) == Some(b'y') && i > 0 && !self.at(i + 1).is_some_and(is_vowel)
    }

    fn vowel(&self, i: usize) -> bool {
        self.at(i).is_some_and(is_vowel) || self.y_vowel(i)
    }

    fn vowel_before(&self, end: usize) -> bool {
        (0..end).any(|i| self.vowel(i))
    }

    fn consonant(&self, i: usize) -> bool {
        self.at(i).is_some() && !self.vowel(i)
    }

    /// Whether the `e` at `i` is silent: last in the word after a consonant ("tone"), or before
    /// a final s or d that doesn't make a syllable of it ("tones", "toned"; not "roses",
    /// "rested"), with a vowel earlier in the word.
    fn silent_e(&self, i: usize) -> bool {
        let n = self.len();
        if self.at(i) != Some(b'e') || i < 2 || !self.consonant(i - 1) || !self.vowel_before(i - 1) {
            return false;
        }
        if i + 1 == n {
            return true;
        }
        if i + 2 != n {
            return false;
        }
        let before = self.w[i - 1];
        let pair = &self.w[i - 2..i];
        match self.w[n - 1] {
            b'd' => !matches!(before, b't' | b'd'),
            b's' => !matches!(before, b's' | b'x' | b'z' | b'c' | b'g') && pair != b"ch" && pair != b"sh",
            _ => false,
        }
    }

    /// Whether the single vowel at `i` is long: a consonant, then a silent e ("tone", "tones"),
    /// a consonant and a final `le` ("table"), or before `-tion` ("nation"; not "position").
    fn long(&self, i: usize) -> bool {
        let n = self.len();
        if self.at(i) != Some(b'i') && (self.starts(i + 1, "tion") || self.starts(i + 1, "sion")) {
            return true;
        }
        if !self.consonant(i + 1) || matches!(self.at(i + 1), Some(b'x' | b'w' | b'y')) {
            return false;
        }
        self.silent_e(i + 2)
            || (self.at(i + 2) == Some(b'l') && self.at(i + 3) == Some(b'e') && i + 4 == n && i > 0)
    }

    fn starts(&self, i: usize, pattern: &str) -> bool {
        self.w[i..].starts_with(pattern.as_bytes())
    }

    /// Whether `pattern` at `i` ends the word.
    fn ends(&self, i: usize, pattern: &str) -> bool {
        self.starts(i, pattern) && i + pattern.len() == self.len()
    }
}

use Arpa::*;

/// The phones for a word's letters (lowercase a–z), each with the letters it came from;
/// silent letters belong to the phone before them. The first vowel is stressed, and the
/// unstressed short vowels after it are reduced (AH).
pub fn letter_rules(letters: &[u8]) -> Vec<Ruled> {
    let word = Word { w: letters };
    let n = word.len();
    let mut out: Vec<Ruled> = Vec::new();
    let mut i = 0;
    // Silent letters before any phone.
    let mut lead = 0;
    while i < n {
        let (phones, used): (&[Arpa], usize) = next(&word, i, &out);
        if phones.is_empty() {
            match out.last_mut() {
                Some((_, range)) => range.end = i + used,
                None => lead = i + used,
            }
        } else {
            for (k, &arpa) in phones.iter().enumerate() {
                let start = if k == 0 && out.is_empty() { lead.min(i) } else { i };
                out.push((Phone::new(arpa, 0), start..i + used));
            }
        }
        i += used;
    }
    stress(&mut out);
    out
}

/// The phones at letter `i`, and how many letters they use.
fn next(word: &Word<'_>, i: usize, out: &[Ruled]) -> (&'static [Arpa], usize) {
    let n = word.len();
    let c = word.w[i];
    let next = word.at(i + 1);
    let s = |p: &str| word.starts(i, p);
    if word.vowel(i) {
        return vowel(word, i);
    }
    // Groups of consonants.
    for (pattern, phones) in [
        ("tch", &[Ch][..]),
        ("sch", &[S, K]),
        ("tion", &[Sh, Ah, N]),
        ("cial", &[Sh, Ah, L]),
        ("tial", &[Sh, Ah, L]),
        ("cious", &[Sh, Ah, S]),
        ("tious", &[Sh, Ah, S]),
        ("ture", &[Ch, Er]),
    ] {
        if s(pattern) {
            return (phones, pattern.len());
        }
    }
    if s("sion") {
        let after_vowel = i > 0 && word.vowel(i - 1);
        return (if after_vowel { &[Zh, Ah, N] } else { &[Sh, Ah, N] }, 4);
    }
    if s("dg") && matches!(word.at(i + 2), Some(b'e' | b'i' | b'y')) {
        return (&[Jh], 2);
    }
    if i == 0 {
        for (pattern, phones) in [
            ("kn", &[N][..]),
            ("wr", &[R]),
            ("gn", &[N]),
            ("ps", &[S]),
            ("pn", &[N]),
            ("gh", &[G]),
        ] {
            if s(pattern) {
                return (phones, 2);
            }
        }
    }
    if word.ends(i, "gn") {
        return (&[N], 2);
    }
    if word.ends(i, "mb") {
        return (&[M], 2);
    }
    for (pattern, phones) in [
        ("ch", &[Ch][..]),
        ("sh", &[Sh]),
        ("th", &[Th]),
        ("ph", &[F]),
        ("wh", &[W]),
        ("ck", &[K]),
        ("qu", &[K, W]),
        // "-ngle" keeps its g ("jingle"); elsewhere it's hard to tell ("finger", "singer").
        ("ngl", &[Ng, G]),
        ("ng", &[Ng]),
        ("nk", &[Ng, K]),
        // After a vowel ("night" is caught with its vowel): silent.
        ("gh", &[]),
    ] {
        if s(pattern) {
            return (phones, 2);
        }
    }
    // A final "-le" after a consonant is a syllable of its own ("jingle"), as is "-les".
    if c == b'l'
        && next == Some(b'e')
        && i > 0
        && word.consonant(i - 1)
        && (i + 2 == n || (i + 3 == n && matches!(word.at(i + 2), Some(b's' | b'd'))))
    {
        return (&[Ah, L], 2);
    }
    let soft = matches!(
        word.at(i + 1 + usize::from(next == Some(c))),
        Some(b'e' | b'i' | b'y')
    );
    // Doubled letters sound once ("ll", "tt"), but "cc" before e or i is "ks".
    let used = if next == Some(c) { 2 } else { 1 };
    let phones: &[Arpa] = match c {
        b'c' if used == 2 && soft => &[K, S],
        b'c' if soft => &[S],
        b'c' => &[K],
        b'g' if soft && used == 1 => &[Jh],
        b'g' => &[G],
        b'x' if i == 0 => &[Z],
        b'x' => &[K, S],
        b'j' => &[Jh],
        b'q' => &[K],
        b'z' => &[Z],
        b's' => {
            let between = i > 0 && word.vowel(i - 1) && word.vowel(i + used);
            let voiced = out.last().is_some_and(|(p, _)| {
                p.is_vowel() || matches!(p.arpa, B | D | G | L | M | N | Ng | R | V | W)
            });
            if between || (i + used == n && i > 0 && voiced) {
                &[Z]
            } else {
                &[S]
            }
        }
        // Sounded before a vowel, silent after one ("oh", "ooh").
        b'h' if word.vowel(i + 1) => &[Hh],
        b'h' => &[],
        b'w' if word.vowel(i + 1) || i == 0 => &[W],
        b'w' => &[],
        b'y' => &[Y],
        b'b' => &[B],
        b'd' => &[D],
        b'f' => &[F],
        b'k' => &[K],
        b'l' => &[L],
        b'm' => &[M],
        b'n' => &[N],
        b'p' => &[P],
        b'r' => &[R],
        b't' => &[T],
        b'v' => &[V],
        _ => &[],
    };
    (phones, used)
}

/// The vowel sound starting at letter `i`, and how many letters it uses.
fn vowel(word: &Word<'_>, i: usize) -> (&'static [Arpa], usize) {
    let n = word.len();
    let s = |p: &str| word.starts(i, p);
    let ends = |p: &str| word.ends(i, p);
    let c = word.w[i];
    for (pattern, phones) in [
        ("eau", &[Ow][..]),
        ("augh", &[Ao]),
        ("eigh", &[Ey]),
        ("igh", &[Ay]),
    ] {
        if s(pattern) {
            return (phones, pattern.len());
        }
    }
    if ends("eah") {
        return (&[Ae], 3);
    }
    if s("ough") {
        return (if ends("ough") { &[Ow] } else { &[Ao] }, 4);
    }
    let r_after =
        |k: usize| word.at(i + k) == Some(b'r') && !word.vowel(i + k + 1) && word.at(i + k + 1) != Some(b'r');
    if s("ai") && r_after(2) {
        return (&[Eh, R], 3);
    }
    if s("ea") && r_after(2) {
        return (&[Ih, R], 3);
    }
    let pair: &[Arpa] = match word.w.get(i..i + 2) {
        Some(b"ee" | b"ea") => &[Iy],
        Some(b"ai" | b"ay" | b"ei") => &[Ey],
        Some(b"ey") if i + 2 == n => &[Iy],
        Some(b"ey") => &[Ey],
        Some(b"oa") => &[Ow],
        Some(b"oe") if i + 2 == n => &[Ow],
        Some(b"oi" | b"oy") => &[Oy],
        Some(b"oo") if word.at(i + 2) == Some(b'k') => &[Uh],
        Some(b"oo") => &[Uw],
        Some(b"ou") => &[Aw],
        Some(b"ow") if i + 2 == n => &[Ow],
        Some(b"ow") => &[Aw],
        Some(b"au" | b"aw") => &[Ao],
        Some(b"ew" | b"ue" | b"ui") => &[Uw],
        Some(b"ie") if i + 2 == n && n <= 3 => &[Ay],
        Some(b"ie") => &[Iy],
        _ => &[],
    };
    if !pair.is_empty() {
        return (pair, 2);
    }
    // A vowel and an h that ends the word or comes before a consonant: "oh", "yeah".
    if word.at(i + 1) == Some(b'h') && !word.vowel(i + 2) {
        let phones: &[Arpa] = match c {
            b'o' => &[Ow],
            b'a' => &[Aa],
            b'e' => &[Eh],
            _ => &[Ah],
        };
        return (phones, 2);
    }
    if r_after(1) {
        let phones: &[Arpa] = match c {
            b'a' => &[Aa, R],
            b'o' => &[Ao, R],
            _ => &[Er],
        };
        return (phones, 2);
    }
    if c == b'e' && word.silent_e(i) {
        return (&[], 1);
    }
    let last = i + 1 == n;
    let only = !word.vowel_before(i);
    let phones: &[Arpa] = match c {
        b'y' if last && only => &[Ay],
        b'y' if last => &[Iy],
        b'y' => &[Ih],
        _ if word.long(i) => match c {
            b'a' => &[Ey],
            b'e' => &[Iy],
            b'i' => &[Ay],
            b'o' => &[Ow],
            _ => &[Uw],
        },
        b'e' if last => &[Iy],
        b'o' if last => &[Ow],
        b'a' if last && only => &[Aa],
        b'i' if last => &[Iy],
        b'u' if last => &[Uw],
        b'a' => &[Ae],
        b'e' => &[Eh],
        b'i' => &[Ih],
        b'o' => &[Aa],
        _ => &[Ah],
    };
    (phones, 1)
}

/// Stresses the first vowel; the short vowels after it are unstressed and reduced to AH.
fn stress(phones: &mut [Ruled]) {
    let mut first = true;
    for (phone, _) in phones.iter_mut().filter(|(p, _)| p.is_vowel()) {
        if first {
            phone.stress = 1;
            first = false;
        } else if matches!(phone.arpa, Ae | Eh | Aa | Uh) {
            phone.arpa = Ah;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phones::spelled;

    fn said(word: &str) -> String {
        let ruled = letter_rules(word.as_bytes());
        spelled(&ruled.iter().map(|(p, _)| *p).collect::<Vec<_>>())
    }

    #[test]
    fn common_spellings_sound_as_expected() {
        for (word, phones) in [
            ("gonna", "G AA1 N AH0"),
            ("thriller", "TH R IH1 L ER0"),
            ("ooh", "UW1"),
            ("oh", "OW1"),
            ("tone", "T OW1 N"),
            ("tones", "T OW1 N Z"),
            ("rested", "R EH1 S T AH0 D"),
            ("jingle", "JH IH1 NG G AH0 L"),
            ("night", "N AY1 T"),
            ("flake", "F L EY1 K"),
            ("catch", "K AE1 CH"),
            ("knight", "N AY1 T"),
            ("happy", "HH AE1 P IY0"),
            ("my", "M AY1"),
            ("yeah", "Y AE1"),
            ("station", "S T EY1 SH AH0 N"),
        ] {
            assert_eq!(said(word), phones, "{word}");
        }
    }

    #[test]
    fn every_letter_belongs_to_a_phone() {
        for word in ["knight", "tone", "thriller", "ooh", "laughed"] {
            let ruled = letter_rules(word.as_bytes());
            assert_eq!(ruled[0].1.start, 0, "{word}");
            assert_eq!(ruled.last().unwrap().1.end, word.len(), "{word}");
            for pair in ruled.windows(2) {
                assert!(pair[0].1.start <= pair[1].1.start, "{word}");
            }
        }
    }
}
