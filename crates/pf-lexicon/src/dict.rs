//! The CMU Pronouncing Dictionary, embedded packed (see `examples/build_cmudict.rs`) and opened
//! the first time a word is looked up.

use crate::phones::Phone;
use std::sync::OnceLock;

/// The packed dictionary: zstd over sorted lines of a word, its phone bytes, and a newline.
static PACKED: &[u8] = include_bytes!("../data/cmudict.zst");

/// Bytes the packed dictionary opens up to, at most (it's about 2 MB).
const MAX_OPEN_BYTES: usize = 8 << 20;

/// The opened dictionary: its lines, and where each starts.
struct Dictionary {
    text: Vec<u8>,
    starts: Vec<u32>,
}

impl Dictionary {
    fn open() -> Self {
        // The data is part of the program; it can't fail but on a broken build, which the tests
        // catch, and then every word is simply unknown.
        let text = zstd::bulk::decompress(PACKED, MAX_OPEN_BYTES).unwrap_or_default();
        let mut starts = vec![0u32];
        starts.extend(
            text.iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i as u32 + 1),
        );
        starts.pop();
        Self { text, starts }
    }

    /// Entry `i`: its word and its phone bytes.
    fn entry(&self, i: usize) -> (&[u8], &[u8]) {
        let start = self.starts[i] as usize;
        let end = self.starts.get(i + 1).map_or(self.text.len(), |&s| s as usize) - 1;
        let line = &self.text[start..end];
        let split = line.iter().position(|&b| b >= 0x80).unwrap_or(line.len());
        line.split_at(split)
    }

    fn find(&self, word: &[u8]) -> Option<&[u8]> {
        let at = self
            .starts
            .binary_search_by(|&s| {
                let start = s as usize;
                let rest = &self.text[start..];
                let len = rest
                    .iter()
                    .position(|&b| b >= 0x80 || b == b'\n')
                    .unwrap_or(rest.len());
                rest[..len].cmp(word)
            })
            .ok()?;
        Some(self.entry(at).1)
    }
}

fn dictionary() -> &'static Dictionary {
    static DICT: OnceLock<Dictionary> = OnceLock::new();
    DICT.get_or_init(Dictionary::open)
}

/// Words in the dictionary.
pub fn word_count() -> usize {
    dictionary().starts.len()
}

/// The size of the embedded (compressed) dictionary, in bytes.
pub fn packed_bytes() -> usize {
    PACKED.len()
}

/// How the dictionary says `word` (lowercase letters and apostrophes, as it lists words), if it
/// has it.
pub fn lookup(word: &str) -> Option<Vec<Phone>> {
    let codes = dictionary().find(word.as_bytes())?;
    codes.iter().map(|&c| Phone::from_code(c)).collect()
}

/// Whether the dictionary has `word`.
pub fn contains(word: &str) -> bool {
    dictionary().find(word.as_bytes()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phones::spelled;

    #[test]
    fn words_are_looked_up() {
        let said = |w: &str| lookup(w).map(|p| spelled(&p));
        assert_eq!(said("ghost").as_deref(), Some("G OW1 S T"));
        assert_eq!(said("beautiful").as_deref(), Some("B Y UW1 T AH0 F AH0 L"));
        assert_eq!(said("don't").as_deref(), Some("D OW1 N T"));
        // The first and last words, and some that aren't there.
        let first = dictionary().entry(0).0.to_vec();
        let last = dictionary().entry(word_count() - 1).0.to_vec();
        assert!(lookup(std::str::from_utf8(&first).unwrap()).is_some());
        assert!(lookup(std::str::from_utf8(&last).unwrap()).is_some());
        assert_eq!(lookup("zzzzqx"), None);
        assert_eq!(lookup(""), None);
        assert_eq!(lookup("Ghost"), None, "words are looked up lowercase");
        assert!(word_count() > 100_000, "{}", word_count());
    }

    #[test]
    fn the_embedded_dictionary_stays_small() {
        // The whole CMU dictionary is 3.6 MB as text.
        assert!(packed_bytes() <= 1_500_000, "{} bytes", packed_bytes());
    }
}
