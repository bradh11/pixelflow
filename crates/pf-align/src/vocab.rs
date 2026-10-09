//! The letters wav2vec2-base-960h hears (its `vocab.json`): a blank, a word gap ("|"), the
//! capital letters A–Z, and the apostrophe.

/// The blank: nothing new heard this step.
pub const BLANK: u32 = 0;
/// Between two words ("|").
pub const WORD_GAP: u32 = 4;
/// How many tokens the model scores each step.
pub const SIZE: usize = 32;

/// Each letter's token, A to Z.
const LETTERS: [u32; 26] = [
    7, 24, 19, 14, 5, 20, 21, 11, 10, 29, 26, 15, 17, 9, 8, 23, 30, 13, 12, 6, 16, 25, 18, 28, 22, 31,
];
const APOSTROPHE: u32 = 27;

/// A letter with its accent dropped ("é" is "e"), lowercase a–z; `None` for anything else.
fn plain_letter(c: char) -> Option<char> {
    let c = c.to_lowercase().next().unwrap_or(c);
    let plain = match c {
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
    Some(plain)
}

/// The token for a character of a sung word: its letter, or the apostrophe (’ too); `None` for
/// what the model doesn't hear (digits, punctuation, other alphabets).
pub fn token(c: char) -> Option<u32> {
    if matches!(c, '\'' | '’' | '‘') {
        return Some(APOSTROPHE);
    }
    plain_letter(c).map(|l| LETTERS[usize::from(l as u8 - b'a')])
}

/// The character a token stands for ("|" between words); `None` for the blank and the
/// sentence markers.
pub fn char_of(token: u32) -> Option<char> {
    match token {
        WORD_GAP => Some('|'),
        APOSTROPHE => Some('\''),
        _ => LETTERS
            .iter()
            .position(|&t| t == token)
            .map(|i| char::from(b'A' + i as u8)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_tokens_match_the_models_vocabulary() {
        // From the model's vocab.json.
        assert_eq!(token('E'), Some(5));
        assert_eq!(token('t'), Some(6));
        assert_eq!(token('Z'), Some(31));
        assert_eq!(token('’'), Some(27));
        assert_eq!(token('é'), token('e'));
        assert_eq!(token('3'), None);
        assert_eq!(token('!'), None);
        assert_eq!(token('п'), None);
        // Every letter a token of its own, read back as itself.
        let mut seen = std::collections::HashSet::new();
        for c in 'a'..='z' {
            let t = token(c).unwrap();
            assert!(seen.insert(t) && (t as usize) < SIZE && t != BLANK && t != WORD_GAP);
            assert_eq!(char_of(t), Some(c.to_ascii_uppercase()));
        }
        assert_eq!(char_of(WORD_GAP), Some('|'));
        assert_eq!(char_of(BLANK), None);
    }
}
