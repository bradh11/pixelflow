//! LRC lyrics: lines stamped with the time they're sung (`[01:02.34] Words of the line`), and the
//! enhanced form whose words carry their own stamps (`[01:02.34] <01:02.34> Words <01:02.80> of`).
//! Metadata tags (`[ar: …]`) are skipped, `[offset: …]` is applied, a line with several stamps
//! is sung at each, and an empty stamped line marks where the line before it ends.

/// One sung line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrcLine {
    pub start_ms: u64,
    /// The line's words (empty for a line that only marks an end).
    pub text: String,
    /// Words with their own start times, from enhanced LRC (empty without them).
    pub words: Vec<(u64, String)>,
    /// Where the last word ends, when an enhanced line closes with a stamp.
    pub words_end_ms: Option<u64>,
}

/// A stamp's time: `mm:ss`, `mm:ss.x…`, or `mm:ss:xx`, in ms.
fn stamp_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (minutes, rest) = text.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let seconds: u64 = seconds.trim().parse().ok()?;
    if seconds >= 60 || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Hundredths, thousandths, or tenths: as many places as are written.
    let fraction_ms = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()? * 100,
        2 => fraction.parse::<u64>().ok()? * 10,
        _ => fraction[..3].parse::<u64>().ok()?,
    };
    Some(minutes * 60_000 + seconds * 1000 + fraction_ms)
}

/// Words with their stamps, and where the last one ends.
type Stamped = (Vec<(u64, String)>, Option<u64>);

/// The words of an enhanced line (`<mm:ss.xx>word <mm:ss.xx>word`) and where the last one ends
/// (a closing stamp), or `None` when it has no word stamps.
fn word_stamps(text: &str) -> Option<Stamped> {
    if !text.contains('<') {
        return None;
    }
    let mut words: Vec<(u64, String)> = Vec::new();
    let mut end = None;
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let close = rest[open..].find('>')? + open;
        let at = stamp_ms(&rest[open + 1..close])?;
        let after = &rest[close + 1..];
        let next = after.find('<').unwrap_or(after.len());
        let word = after[..next].trim();
        if word.is_empty() {
            end = Some(at);
        } else {
            words.push((at, word.to_string()));
            end = None;
        }
        rest = &after[next..];
    }
    (!words.is_empty()).then_some((words, end))
}

/// Parses LRC text into lines, in time order.
pub fn parse_lrc(text: &str) -> Vec<LrcLine> {
    let mut offset_ms: i64 = 0;
    let mut lines: Vec<LrcLine> = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(close) = inner.find(']') else { break };
            let tag = &inner[..close];
            match stamp_ms(tag) {
                Some(ms) => stamps.push(ms),
                None => {
                    if let Some(value) = tag.strip_prefix("offset:") {
                        offset_ms = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            rest = inner[close + 1..].trim_start();
        }
        if stamps.is_empty() {
            continue;
        }
        let (words, words_end_ms) = word_stamps(rest).unwrap_or_default();
        let text = if words.is_empty() {
            rest.trim().to_string()
        } else {
            words
                .iter()
                .map(|(_, w)| w.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        for start in stamps {
            lines.push(LrcLine {
                start_ms: start,
                text: text.clone(),
                words: words.clone(),
                words_end_ms,
            });
        }
    }
    // A positive offset means the lyrics come sooner.
    let shift = |ms: u64| ms.saturating_add_signed(-offset_ms);
    for line in &mut lines {
        line.start_ms = shift(line.start_ms);
        for (at, _) in &mut line.words {
            *at = shift(*at);
        }
        line.words_end_ms = line.words_end_ms.map(shift);
    }
    lines.sort_by_key(|l| l.start_ms);
    lines
}

/// The lines of plain (unsynced) lyrics: trimmed, blank lines and section labels
/// (`[Chorus]`, `(Verse 2)`) left out.
pub fn plain_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let bracketed =
                (l.starts_with('[') && l.ends_with(']')) || (l.starts_with('(') && l.ends_with(')'));
            !(bracketed && l.split_whitespace().count() <= 3)
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_read_in_any_precision() {
        assert_eq!(stamp_ms("01:02.34"), Some(62_340));
        assert_eq!(stamp_ms("01:02.345"), Some(62_345));
        assert_eq!(stamp_ms("01:02.3"), Some(62_300));
        assert_eq!(stamp_ms("1:02"), Some(62_000));
        assert_eq!(stamp_ms("00:02:50"), Some(2_500));
        assert_eq!(stamp_ms("ar:Someone"), None);
        assert_eq!(stamp_ms("00:61.00"), None);
    }

    #[test]
    fn lines_metadata_offsets_and_repeats() {
        let text = "[ar:Nobody Real]\n[ti:Made Up Song]\n[offset:+500]\n\
                    [00:10.00]Paper lanterns glowing\n\
                    [00:14.50][01:20.00]Snowy rooftops shine\n\
                    [00:18.00]\n\
                    not a lyric line\n\
                    [00:20.00] Bells across the valley ";
        let lines = parse_lrc(text);
        let shown: Vec<(u64, &str)> = lines.iter().map(|l| (l.start_ms, l.text.as_str())).collect();
        assert_eq!(
            shown,
            [
                (9_500, "Paper lanterns glowing"),
                (14_000, "Snowy rooftops shine"),
                (17_500, ""),
                (19_500, "Bells across the valley"),
                (79_500, "Snowy rooftops shine"),
            ]
        );
    }

    #[test]
    fn enhanced_lines_carry_word_stamps() {
        let lines =
            parse_lrc("[00:05.00] <00:05.00> Paper <00:05.40> lanterns <00:06.10> glowing <00:07.00>");
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line.text, "Paper lanterns glowing");
        assert_eq!(
            line.words,
            [
                (5_000, "Paper".to_string()),
                (5_400, "lanterns".to_string()),
                (6_100, "glowing".to_string())
            ]
        );
        assert_eq!(line.words_end_ms, Some(7_000));
        // Without word stamps, no words.
        assert!(parse_lrc("[00:05.00]Paper lanterns")[0].words.is_empty());
    }

    #[test]
    fn plain_lyrics_drop_blank_lines_and_section_labels() {
        let text = "[Verse 1]\nPaper lanterns glowing\n\n(Chorus)\nSnowy rooftops shine\n(oh, the snowy rooftops shining bright)";
        assert_eq!(
            plain_lines(text),
            [
                "Paper lanterns glowing",
                "Snowy rooftops shine",
                "(oh, the snowy rooftops shining bright)"
            ]
        );
    }
}
