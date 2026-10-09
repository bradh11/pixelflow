//! Which language lyrics are in, found cheaply: by the alphabet most of their letters are in,
//! and, for the Latin alphabet, by the common short words they use ("the", "and"; "que",
//! "los"). Languages are ISO 639-1 codes ("en"), as OpenAI's speech recognition takes them.

/// The language assumed when nothing else says.
pub const DEFAULT: &str = "en";

/// An alphabet (or writing system).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Hangul,
    Kana,
    Han,
}

fn script_of(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    Some(match c as u32 {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x0900..=0x097F => Script::Devanagari,
        0x0E00..=0x0E7F => Script::Thai,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x3040..=0x30FF => Script::Kana,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => Script::Han,
        _ => return None,
    })
}

/// The alphabet most of `text`'s letters are in (`None` without letters).
pub fn dominant_script(text: &str) -> Option<Script> {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for script in text.chars().filter_map(script_of) {
        match counts.iter_mut().find(|(s, _)| *s == script) {
            Some((_, n)) => *n += 1,
            None => counts.push((script, 1)),
        }
    }
    counts.into_iter().max_by_key(|&(_, n)| n).map(|(s, _)| s)
}

/// The alphabet a language is written in.
pub fn script_for(language: &str) -> Script {
    match language {
        "ru" | "uk" | "be" | "bg" | "mk" | "sr" | "kk" | "mn" => Script::Cyrillic,
        "el" => Script::Greek,
        "ar" | "fa" | "ur" => Script::Arabic,
        "he" | "yi" => Script::Hebrew,
        "hi" | "mr" | "ne" => Script::Devanagari,
        "th" => Script::Thai,
        "ko" => Script::Hangul,
        "ja" => Script::Kana,
        "zh" => Script::Han,
        _ => Script::Latin,
    }
}

/// Common short words of Latin-alphabet languages, few shared between them.
const COMMON_WORDS: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "the", "and", "you", "your", "i'm", "it's", "don't", "that", "this", "with", "what", "when",
            "there", "they", "was", "are", "just", "know", "can't", "never", "my", "is", "of", "we", "be",
        ],
    ),
    (
        "es",
        &[
            "el", "los", "las", "que", "por", "con", "una", "pero", "para", "como", "yo", "mi", "tu", "del",
            "muy", "más", "cuando", "quiero", "eres", "estoy", "nada", "y", "es", "la",
        ],
    ),
    (
        "fr",
        &[
            "le", "les", "et", "je", "est", "pas", "une", "des", "pour", "dans", "qui", "mon", "ton", "avec",
            "moi", "toi", "c'est", "j'ai", "nous", "vous", "sur", "mais", "ça",
        ],
    ),
    (
        "de",
        &[
            "der", "die", "das", "und", "ich", "nicht", "ist", "du", "ein", "eine", "mit", "mich", "dich",
            "auf", "sie", "wir", "mein", "dein", "für", "auch", "noch", "wenn",
        ],
    ),
    (
        "it",
        &[
            "il", "che", "non", "sono", "della", "ti", "ma", "io", "sei", "tutto", "anche", "nel", "gli",
            "questo", "perché", "cosa", "di",
        ],
    ),
    (
        "pt",
        &[
            "os", "não", "um", "você", "eu", "meu", "minha", "com", "do", "da", "em", "mais", "é", "tão",
            "sem", "seu", "nós",
        ],
    ),
    (
        "nl",
        &[
            "het", "een", "ik", "je", "niet", "van", "dat", "op", "mijn", "jij", "wij", "maar", "zijn",
            "voor", "naar",
        ],
    ),
];

/// The words of `text`, lowercase, apostrophes kept.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’'))
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('’', "'").to_lowercase())
        .collect()
}

/// The language `text` is in, when it can be told: by its alphabet, and for the Latin alphabet
/// by its common words (at least two, clearly more of one language's than another's).
pub fn detect(text: &str) -> Option<&'static str> {
    let script = dominant_script(text)?;
    Some(match script {
        Script::Cyrillic if text.chars().any(|c| matches!(c, 'і' | 'ї' | 'є' | 'ґ')) => "uk",
        Script::Cyrillic => "ru",
        Script::Greek => "el",
        Script::Arabic => "ar",
        Script::Hebrew => "he",
        Script::Devanagari => "hi",
        Script::Thai => "th",
        Script::Hangul => "ko",
        Script::Kana => "ja",
        Script::Han if text.chars().any(|c| script_of(c) == Some(Script::Kana)) => "ja",
        Script::Han => "zh",
        Script::Latin => {
            let words = words(text);
            let mut hits: Vec<(&'static str, usize)> = COMMON_WORDS
                .iter()
                .map(|(code, common)| {
                    (
                        *code,
                        words.iter().filter(|w| common.contains(&w.as_str())).count(),
                    )
                })
                .collect();
            hits.sort_by_key(|h| std::cmp::Reverse(h.1));
            let (best, n) = hits[0];
            let runner_up = hits.get(1).map_or(0, |h| h.1);
            if n < 2 || (n as f64) < 1.5 * runner_up as f64 {
                return None;
            }
            best
        }
    })
}

/// Whether `text` (what the recognizer heard) could be in `expected`: its alphabet is the
/// language's, and Latin-alphabet text long enough to tell isn't clearly another language.
pub fn fits(expected: &str, text: &str) -> bool {
    let Some(script) = dominant_script(text) else {
        return true;
    };
    let wanted = script_for(expected);
    let same_script = script == wanted || (wanted == Script::Kana && script == Script::Han);
    if !same_script {
        return false;
    }
    if script == Script::Latin && words(text).len() >= 20 {
        return detect(text).is_none_or(|found| found == expected);
    }
    true
}

/// A language's name in English ("English"), or its code when it isn't one PixelFlow names.
pub fn name(code: &str) -> String {
    let known = match code {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "pl" => "Polish",
        "ru" => "Russian",
        "uk" => "Ukrainian",
        "el" => "Greek",
        "ar" => "Arabic",
        "he" => "Hebrew",
        "hi" => "Hindi",
        "th" => "Thai",
        "ko" => "Korean",
        "ja" => "Japanese",
        "zh" => "Chinese",
        _ => return code.to_string(),
    };
    known.to_string()
}

/// A language setting or tag as an ISO 639-1 code: two letters as they are, the common
/// three-letter (ISO 639-2) codes a song's tags use ("eng") brought down to two; `None` for
/// anything else ("und", "xxx").
pub fn code(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    if !text.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let two = match text.as_str() {
        t if t.len() == 2 => return Some(t.to_string()),
        "eng" => "en",
        "spa" => "es",
        "fre" | "fra" => "fr",
        "ger" | "deu" => "de",
        "ita" => "it",
        "por" => "pt",
        "dut" | "nld" => "nl",
        "swe" => "sv",
        "pol" => "pl",
        "rus" => "ru",
        "ukr" => "uk",
        "gre" | "ell" => "el",
        "ara" => "ar",
        "heb" => "he",
        "hin" => "hi",
        "tha" => "th",
        "kor" => "ko",
        "jpn" => "ja",
        "chi" | "zho" => "zh",
        _ => return None,
    };
    Some(two.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabets_and_common_words_tell_the_language() {
        assert_eq!(detect("привет молоко"), Some("ru"));
        assert_eq!(detect("Привіт, їжак"), Some("uk"));
        assert_eq!(detect("καλημέρα"), Some("el"));
        // Made-up lines.
        assert_eq!(detect("The lanterns glow and you know it's snowing"), Some("en"));
        assert_eq!(detect("Las linternas que brillan con la nieve"), Some("es"));
        assert_eq!(detect("Lanterns glowing"), None);
        assert_eq!(detect("1234 !!"), None);
    }

    #[test]
    fn heard_text_fits_its_language_or_not() {
        assert!(fits("en", "Paper lanterns glowing"));
        assert!(!fits("en", "Привет молоко, привет"));
        assert!(fits("ru", "Привет молоко"));
        assert!(fits("en", "…"));
        let spanish = "Las linternas que brillan con la nieve y los tejados que brillan con la luz y \
                       el viento que canta por la noche";
        assert!(!fits("en", spanish));
        assert!(fits("es", spanish));
    }

    #[test]
    fn codes_and_names() {
        assert_eq!(code("eng").as_deref(), Some("en"));
        assert_eq!(code(" EN ").as_deref(), Some("en"));
        assert_eq!(code("und"), None);
        assert_eq!(code("en-US"), None);
        assert_eq!(name("ru"), "Russian");
        assert_eq!(name("xx"), "xx");
    }
}
