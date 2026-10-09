//! Words the English rules can't sound out: accented Latin ("corazón", "Mädchen"), Cyrillic
//! ("молоко"), Greek, and so on. Each vowel group is a syllable's nucleus (each Cyrillic vowel
//! letter its own, as Russian and Ukrainian say them); one consonant between two goes to the
//! second syllable, more are split after the first unless they end in a consonant then an r or
//! l ("ma·dre"). Each syllable's phones are a rough guess from its letters spelled in Latin.

use crate::phones::Phone;
use crate::rules::letter_rules;

/// Whether `c` is a vowel letter in Latin (with or without accents), Cyrillic, or Greek. A `y`
/// counts here; [`syllables`] treats one before a vowel as a consonant.
pub fn is_vowel_letter(c: char) -> bool {
    let c = c.to_lowercase().next().unwrap_or(c);
    matches!(
        c,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | 'y'
            | 'à'
            | 'á'
            | 'â'
            | 'ã'
            | 'ä'
            | 'å'
            | 'ā'
            | 'ă'
            | 'ą'
            | 'æ'
            | 'è'
            | 'é'
            | 'ê'
            | 'ë'
            | 'ē'
            | 'ė'
            | 'ę'
            | 'ě'
            | 'ì'
            | 'í'
            | 'î'
            | 'ï'
            | 'ī'
            | 'į'
            | 'ı'
            | 'ò'
            | 'ó'
            | 'ô'
            | 'õ'
            | 'ö'
            | 'ø'
            | 'ō'
            | 'ő'
            | 'œ'
            | 'ù'
            | 'ú'
            | 'û'
            | 'ü'
            | 'ū'
            | 'ů'
            | 'ű'
            | 'ų'
            | 'ý'
            | 'ÿ'
            | 'а'
            | 'е'
            | 'ё'
            | 'и'
            | 'о'
            | 'у'
            | 'ы'
            | 'э'
            | 'ю'
            | 'я'
            | 'і'
            | 'ї'
            | 'є'
            | 'α'
            | 'ε'
            | 'η'
            | 'ι'
            | 'ο'
            | 'υ'
            | 'ω'
            | 'ά'
            | 'έ'
            | 'ή'
            | 'ί'
            | 'ό'
            | 'ύ'
            | 'ώ'
            | 'ϊ'
            | 'ϋ'
            | 'ΐ'
            | 'ΰ'
    )
}

fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

/// An r or l, which a consonant before it starts a syllable with.
fn liquid(c: char) -> bool {
    matches!(
        c.to_lowercase().next().unwrap_or(c),
        'r' | 'l' | 'р' | 'л' | 'ρ' | 'λ'
    )
}

/// Whether `chars` has a letter beyond plain a–z.
pub fn has_foreign_letters(chars: &[char]) -> bool {
    chars
        .iter()
        .any(|c| c.is_alphabetic() && !c.is_ascii_alphabetic())
}

/// A letter spelled in plain a–z, for guessing its sound ("" for a sign with none, like ь).
fn latin(c: char) -> &'static str {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'a' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'а' | 'α' | 'ά' => "a",
        'b' | 'б' => "b",
        'c' | 'ç' | 'ć' => "c",
        'd' | 'đ' | 'д' | 'δ' => "d",
        'e' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' | 'е' | 'э' | 'ε' | 'έ' => "e",
        'f' | 'ф' | 'φ' => "f",
        'g' | 'ğ' | 'г' | 'ґ' | 'γ' => "g",
        'h' | 'х' | 'χ' => "h",
        'i' | 'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' | 'и' | 'ы' | 'і' | 'η' | 'ι' | 'υ' | 'ή' | 'ί'
        | 'ύ' | 'ϊ' | 'ϋ' | 'ΐ' | 'ΰ' => "i",
        'j' => "j",
        'k' | 'к' | 'κ' => "k",
        'l' | 'ł' | 'л' | 'λ' => "l",
        'm' | 'м' | 'μ' => "m",
        'n' | 'ñ' | 'ń' | 'ň' | 'н' | 'ν' => "n",
        'o' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' | 'о' | 'ο' | 'ω' | 'ό' | 'ώ' => "o",
        'p' | 'п' | 'π' => "p",
        'q' => "q",
        'r' | 'ř' | 'р' | 'ρ' => "r",
        's' | 'ś' | 'с' | 'σ' | 'ς' => "s",
        'š' | 'ş' | 'ш' | 'щ' => "sh",
        't' | 'ť' | 'ț' | 'т' | 'τ' => "t",
        'u' | 'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' | 'у' | 'ў' => "u",
        'v' | 'в' | 'β' => "v",
        'w' => "w",
        'x' | 'ξ' => "x",
        'y' | 'ý' | 'ÿ' | 'й' => "y",
        'z' | 'ź' | 'ż' | 'з' => "z",
        'ž' | 'ж' => "zh",
        'č' | 'ч' => "ch",
        'ц' => "ts",
        'ё' => "yo",
        'ю' => "yu",
        'я' => "ya",
        'ї' => "yi",
        'є' => "ye",
        'θ' => "th",
        'ψ' => "ps",
        'æ' => "ae",
        'œ' => "oe",
        'ß' => "ss",
        _ => "",
    }
}

/// Rough phones for some letters, through the letter rules.
fn guess_phones(letters: &[char]) -> Vec<Phone> {
    let spelled: Vec<u8> = letters.iter().flat_map(|&c| latin(c).bytes()).collect();
    letter_rules(&spelled).into_iter().map(|(p, _)| p).collect()
}

/// `chars` split into syllables: where each starts (a character index) and its phones. `None`
/// without letters.
pub fn syllables(chars: &[char]) -> Option<Vec<(usize, Vec<Phone>)>> {
    let letters: Vec<usize> = (0..chars.len()).filter(|&i| chars[i].is_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let letter = |k: usize| chars[letters[k]];
    let vowel = |k: usize| {
        let c = letter(k);
        let y = matches!(c, 'y' | 'Y');
        is_vowel_letter(c) && !(y && k + 1 < letters.len() && is_vowel_letter(letter(k + 1)))
    };
    // Vowel groups, as ranges of `letters`.
    let mut nuclei: Vec<(usize, usize)> = Vec::new();
    let mut k = 0;
    while k < letters.len() {
        if !vowel(k) {
            k += 1;
            continue;
        }
        let start = k;
        k += 1;
        while k < letters.len() && vowel(k) && !is_cyrillic(letter(k)) && letters[k] == letters[k - 1] + 1 {
            k += 1;
        }
        nuclei.push((start, k));
    }
    let mut starts = vec![0];
    for pair in nuclei.windows(2) {
        let ((_, a_end), (b_start, _)) = (pair[0], pair[1]);
        let start = match b_start - a_end {
            0 | 1 => a_end,
            2 if liquid(letter(b_start - 1)) && !liquid(letter(b_start - 2)) => b_start - 2,
            _ => a_end + 1,
        };
        starts.push(start);
    }
    let mut out = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(letters.len());
        let at = if i == 0 { 0 } else { letters[start] };
        let these: Vec<char> = (start..end).map(letter).collect();
        out.push((at, guess_phones(&these)));
    }
    Some(out)
}
