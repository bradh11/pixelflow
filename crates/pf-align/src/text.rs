//! Sung words as the model's tokens ([`crate::vocab`]): each word's letters, a word gap ("|")
//! between words, and which word and character each token came from.

use crate::vocab;

/// Words as tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    pub tokens: Vec<u32>,
    /// For each token: the word and the character in it (an index into the word's characters);
    /// `None` for a word gap.
    pub owners: Vec<Option<(usize, usize)>>,
    pub words: usize,
}

impl Transcript {
    /// `words` as tokens. A word with nothing the model hears ("42", "—") has no tokens, and
    /// no gap of its own.
    pub fn new<S: AsRef<str>>(words: &[S]) -> Self {
        let mut out = Transcript {
            words: words.len(),
            ..Self::default()
        };
        for (w, word) in words.iter().enumerate() {
            let letters: Vec<(usize, u32)> = word
                .as_ref()
                .chars()
                .enumerate()
                .filter_map(|(c, ch)| vocab::token(ch).map(|t| (c, t)))
                .collect();
            // An apostrophe quoting the word isn't heard.
            let first = letters.iter().position(|&(_, t)| vocab::char_of(t) != Some('\''));
            let last = letters
                .iter()
                .rposition(|&(_, t)| vocab::char_of(t) != Some('\''));
            let (Some(first), Some(last)) = (first, last) else {
                continue;
            };
            if !out.tokens.is_empty() {
                out.tokens.push(vocab::WORD_GAP);
                out.owners.push(None);
            }
            for &(c, t) in &letters[first..=last] {
                out.tokens.push(t);
                out.owners.push(Some((w, c)));
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_become_letters_with_gaps_and_owners() {
        let t = Transcript::new(&["I", "ain't", "42", "'fraid!"]);
        let text: String = t.tokens.iter().filter_map(|&k| vocab::char_of(k)).collect();
        assert_eq!(text, "I|AIN'T|FRAID");
        assert_eq!(t.words, 4);
        assert_eq!(t.owners[0], Some((0, 0)));
        assert_eq!(t.owners[1], None);
        // "ain't": its apostrophe heard, at character 3.
        assert_eq!(t.owners[5], Some((1, 3)));
        // "'fraid!": the quoting apostrophe and the "!" aren't; "f" is character 1.
        assert_eq!(t.owners[8], Some((3, 1)));
        assert_eq!(t.tokens.len(), t.owners.len());
        assert!(Transcript::new(&["42", "…"]).is_empty());
    }
}
