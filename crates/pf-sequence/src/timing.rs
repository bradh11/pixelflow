//! Working with timing marks: keeping a track's marks in order without overlaps, generating
//! marks (a fixed interval, every Nth mark of another track), spreading lyrics over time and
//! breaking phrases into words, and Audacity label files. Problems are explained in plain
//! language (the `Err` strings are shown to people as they are).

use crate::{MAX_MARKS, Mark, TimingTrack, format_ms};

/// Closest two generated marks may be.
pub const MIN_MARK_INTERVAL_MS: u64 = 10;
/// Largest timing file (`.xtiming`, Audacity labels) PixelFlow reads.
pub const MAX_TIMING_FILE_BYTES: usize = 16 * 1024 * 1024;

/// True when two marks share some time (marks that only touch don't overlap).
pub fn marks_overlap(a: &Mark, b: &Mark) -> bool {
    a.start_ms < b.end_ms && b.start_ms < a.end_ms
}

/// A mark must have some length.
pub fn check_mark(mark: &Mark) -> Result<(), String> {
    if mark.end_ms <= mark.start_ms {
        return Err("A mark must end after it starts.".to_string());
    }
    Ok(())
}

impl TimingTrack {
    /// Where a mark starting at `start_ms` goes to keep the marks in order (after any that start
    /// at the same time).
    pub fn insert_index(&self, start_ms: u64) -> usize {
        self.marks.partition_point(|m| m.start_ms <= start_ms)
    }

    /// The first mark (by index) that overlaps `mark`, skipping the marks at `skip`.
    pub fn overlap_with(&self, mark: &Mark, skip: &[usize]) -> Option<usize> {
        self.marks
            .iter()
            .enumerate()
            .find(|(i, m)| !skip.contains(i) && marks_overlap(m, mark))
            .map(|(i, _)| i)
    }

    /// Adds `marks`, each where it belongs in time. Refused (nothing changes) when one has no
    /// length or overlaps a mark already there or another new one.
    pub fn add_marks(&mut self, marks: &[Mark]) -> Result<(), String> {
        let mut next = self.clone();
        for mark in marks {
            check_mark(mark)?;
            if let Some(i) = next.overlap_with(mark, &[]) {
                return Err(self.overlap_message(&next.marks[i]));
            }
            let at = next.insert_index(mark.start_ms);
            next.marks.insert(at, mark.clone());
        }
        *self = next;
        Ok(())
    }

    /// Takes out every mark that shares time with `from..to`.
    pub fn clear_range(&mut self, from_ms: u64, to_ms: u64) {
        let span = Mark::new(from_ms, to_ms, "");
        self.marks.retain(|m| !marks_overlap(m, &span));
    }

    /// Says a mark would overlap `other` on this track.
    pub fn overlap_message(&self, other: &Mark) -> String {
        format!(
            "That would overlap the mark at {} on '{}'; marks on a timing track can't overlap.",
            format_ms(other.start_ms),
            self.name
        )
    }
}

fn check_range(from_ms: u64, to_ms: u64) -> Result<(), String> {
    if to_ms <= from_ms {
        return Err("Choose a time range that ends after it starts.".to_string());
    }
    Ok(())
}

fn check_count(count: u64) -> Result<(), String> {
    if count > MAX_MARKS as u64 {
        return Err(format!(
            "That would make {count} marks; at most {MAX_MARKS} are allowed."
        ));
    }
    Ok(())
}

/// A mark every `every_ms` from `from_ms` to `to_ms` (the last one ends at `to_ms`).
pub fn fixed_marks(every_ms: u64, from_ms: u64, to_ms: u64) -> Result<Vec<Mark>, String> {
    if every_ms < MIN_MARK_INTERVAL_MS {
        return Err(format!("Marks must be at least {MIN_MARK_INTERVAL_MS} ms apart."));
    }
    check_range(from_ms, to_ms)?;
    check_count((to_ms - from_ms).div_ceil(every_ms))?;
    Ok((from_ms..to_ms)
        .step_by(every_ms as usize)
        .map(|start| Mark::new(start, (start + every_ms).min(to_ms), ""))
        .collect())
}

/// Every `every`th mark of `source` (1 = all of them), each lasting until the next one taken (the
/// last one keeps its own end, or runs to the end of the group it stands for). Labels come along.
pub fn every_nth_mark(source: &[Mark], every: usize) -> Result<Vec<Mark>, String> {
    if every == 0 {
        return Err("Take every 1st, 2nd, 3rd… mark: the step must be at least 1.".to_string());
    }
    let taken: Vec<usize> = (0..source.len()).step_by(every).collect();
    let mut marks = Vec::with_capacity(taken.len());
    for (k, &i) in taken.iter().enumerate() {
        let start_ms = source[i].start_ms;
        let end_ms = match taken.get(k + 1) {
            Some(&next) => source[next].start_ms,
            None => source[(i + every - 1).min(source.len() - 1)].end_ms,
        };
        if end_ms > start_ms {
            marks.push(Mark::new(start_ms, end_ms, source[i].label.clone()));
        }
    }
    Ok(marks)
}

/// How much time a piece of text gets: its letters and digits (at least one).
fn weight(text: &str) -> u64 {
    (text.chars().filter(|c| c.is_alphanumeric()).count() as u64).max(1)
}

/// Splits `from..to` into one span per item, each as long as its share of `weights`, every span at
/// least 1 ms. `None` when the time is shorter than the number of items.
fn divide(from_ms: u64, to_ms: u64, weights: &[u64]) -> Option<Vec<(u64, u64)>> {
    let n = weights.len() as u64;
    let length = to_ms.checked_sub(from_ms)?;
    if n == 0 || length < n {
        return None;
    }
    let total: u64 = weights.iter().sum();
    let mut spans = Vec::with_capacity(weights.len());
    let mut start = from_ms;
    let mut sum = 0;
    for (i, w) in weights.iter().enumerate() {
        sum += w;
        let left = n - 1 - i as u64;
        let ideal = from_ms + ((length as u128 * sum as u128 + total as u128 / 2) / total as u128) as u64;
        let end = if left == 0 {
            to_ms
        } else {
            ideal.max(start + 1).min(to_ms - left)
        };
        spans.push((start, end));
        start = end;
    }
    Some(spans)
}

/// The lines of pasted lyrics: one phrase per line, trimmed, blank lines left out.
pub fn lyric_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// One mark per phrase over `from..to`, back to back, each as long as its share of the letters
/// (so a long line gets more time than a short one).
pub fn spread_phrases(lines: &[String], from_ms: u64, to_ms: u64) -> Result<Vec<Mark>, String> {
    let lines: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return Err("Paste at least one line of lyrics.".to_string());
    }
    check_range(from_ms, to_ms)?;
    check_count(lines.len() as u64)?;
    let weights: Vec<u64> = lines.iter().map(|l| weight(l)).collect();
    let spans = divide(from_ms, to_ms, &weights).ok_or_else(|| {
        format!(
            "{} is too short for {} lines of lyrics.",
            format_ms(to_ms - from_ms),
            lines.len()
        )
    })?;
    Ok(spans
        .into_iter()
        .zip(lines)
        .map(|((s, e), l)| Mark::new(s, e, l))
        .collect())
}

/// One mark per word of a phrase mark (its label split on spaces), sharing the phrase's time by
/// letter count. Empty when the phrase has no words.
pub fn split_words(phrase: &Mark) -> Result<Vec<Mark>, String> {
    let words: Vec<&str> = phrase.label.split_whitespace().collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let weights: Vec<u64> = words.iter().map(|w| weight(w)).collect();
    let spans = divide(phrase.start_ms, phrase.end_ms, &weights).ok_or_else(|| {
        format!(
            "The phrase at {} is too short to split into {} words.",
            format_ms(phrase.start_ms),
            words.len()
        )
    })?;
    Ok(spans
        .into_iter()
        .zip(words)
        .map(|((s, e), w)| Mark::new(s, e, w))
        .collect())
}

/// What [`tidy_marks`] left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TidyReport {
    /// Marks with no length (or that end before they start).
    pub no_length: usize,
    /// Marks that overlapped an earlier one.
    pub overlapping: usize,
    /// Marks that start after `end_ms`.
    pub outside: usize,
    /// Marks cut short at `end_ms`.
    pub cut: usize,
}

impl TidyReport {
    /// What was left out or cut, in plain language, for a file named `what`.
    pub fn notes(&self, what: &str) -> Vec<String> {
        let marks = |n: usize| {
            if n == 1 {
                "1 mark".to_string()
            } else {
                format!("{n} marks")
            }
        };
        let mut notes = Vec::new();
        if self.no_length > 0 {
            notes.push(format!(
                "{} in {what} had no length and {} left out.",
                marks(self.no_length),
                if self.no_length == 1 { "was" } else { "were" }
            ));
        }
        if self.overlapping > 0 {
            notes.push(format!(
                "{} in {what} overlapped the mark before and {} left out.",
                marks(self.overlapping),
                if self.overlapping == 1 { "was" } else { "were" }
            ));
        }
        if self.outside > 0 {
            notes.push(format!(
                "{} in {what} started after the end of the sequence and {} left out.",
                marks(self.outside),
                if self.outside == 1 { "was" } else { "were" }
            ));
        }
        if self.cut > 0 {
            notes.push(format!(
                "{} in {what} ran past the end of the sequence and {} cut short.",
                marks(self.cut),
                if self.cut == 1 { "was" } else { "were" }
            ));
        }
        notes
    }
}

/// Marks from a file, made fit for a timing track: in order, each with some length, none
/// overlapping an earlier one, none past `end_ms` (cut there). Says what was left out.
pub fn tidy_marks(mut marks: Vec<Mark>, end_ms: u64) -> (Vec<Mark>, TidyReport) {
    let mut report = TidyReport::default();
    marks.sort_by_key(|m| (m.start_ms, m.end_ms));
    let mut kept: Vec<Mark> = Vec::with_capacity(marks.len());
    for mut mark in marks {
        if mark.end_ms <= mark.start_ms {
            report.no_length += 1;
            continue;
        }
        if mark.start_ms >= end_ms {
            report.outside += 1;
            continue;
        }
        if mark.end_ms > end_ms {
            mark.end_ms = end_ms;
            report.cut += 1;
        }
        if kept.last().is_some_and(|last| last.end_ms > mark.start_ms) {
            report.overlapping += 1;
            continue;
        }
        kept.push(mark);
    }
    (kept, report)
}

/// Seconds as Audacity writes them ("1.500000") → milliseconds; `None` when it's not a time.
fn seconds_to_ms(text: &str) -> Option<u64> {
    let seconds: f64 = text.trim().parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 || seconds > crate::MAX_DURATION_MS as f64 / 1000.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as u64)
}

/// An Audacity label file: one label per line, `start<TAB>end<TAB>label` in seconds. Lines that
/// start with `\` (Audacity's frequency ranges) are skipped. A point label (start = end) lasts
/// until the next label starts (the last one, half a second). The marks still need
/// [`tidy_marks`] before they go on a track.
pub fn parse_audacity_labels(text: &str) -> Result<Vec<Mark>, String> {
    if text.len() > MAX_TIMING_FILE_BYTES {
        return Err(format!(
            "it is {} MB; PixelFlow reads timing files up to {} MB",
            text.len() / (1024 * 1024),
            MAX_TIMING_FILE_BYTES / (1024 * 1024)
        ));
    }
    let mut marks = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('\\') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let start = parts.next().and_then(seconds_to_ms);
        let end = parts.next().and_then(seconds_to_ms);
        let (Some(start_ms), Some(end_ms)) = (start, end) else {
            return Err(format!(
                "line {} isn't an Audacity label (start and end in seconds, then the label, separated by tabs)",
                number + 1
            ));
        };
        let label = parts.next().unwrap_or("").trim().to_string();
        marks.push(Mark::new(start_ms, end_ms, label));
        if marks.len() > MAX_MARKS {
            return Err(format!("it has more than {MAX_MARKS} labels"));
        }
    }
    if marks.is_empty() {
        return Err("it has no labels in it".to_string());
    }
    // Point labels last until the next label starts.
    let mut starts: Vec<u64> = marks.iter().map(|m| m.start_ms).collect();
    starts.sort_unstable();
    for mark in &mut marks {
        if mark.end_ms == mark.start_ms {
            let next = starts.iter().copied().find(|&s| s > mark.start_ms);
            mark.end_ms = next.unwrap_or(mark.start_ms + 500);
        }
    }
    Ok(marks)
}

/// Marks as an Audacity label file (seconds with six decimals, tab-separated).
pub fn audacity_labels(marks: &[Mark]) -> String {
    let seconds = |ms: u64| format!("{}.{:03}000", ms / 1000, ms % 1000);
    let mut out = String::new();
    for m in marks {
        let label = m.label.replace(['\t', '\n', '\r'], " ");
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            seconds(m.start_ms),
            seconds(m.end_ms),
            label
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TimingKind;

    fn spans(marks: &[Mark]) -> Vec<(u64, u64, &str)> {
        marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect()
    }

    #[test]
    fn marks_go_in_order_and_never_overlap() {
        let mut track = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![Mark::new(1000, 2000, "b")]);
        track
            .add_marks(&[Mark::new(0, 1000, "a"), Mark::new(2000, 2500, "c")])
            .unwrap();
        assert_eq!(
            spans(&track.marks),
            vec![(0, 1000, "a"), (1000, 2000, "b"), (2000, 2500, "c")]
        );
        let before = track.clone();
        let err = track
            .add_marks(&[Mark::new(3000, 3100, ""), Mark::new(1500, 1600, "x")])
            .unwrap_err();
        assert_eq!(
            err,
            "That would overlap the mark at 0:01.000 on 'Lyrics'; marks on a timing track can't overlap."
        );
        assert_eq!(track, before, "nothing changes");
        assert_eq!(
            track.add_marks(&[Mark::new(5, 5, "")]).unwrap_err(),
            "A mark must end after it starts."
        );
        // New marks can't overlap each other either.
        assert!(
            track
                .add_marks(&[Mark::new(3000, 3200, ""), Mark::new(3100, 3300, "")])
                .is_err()
        );
        assert_eq!(track.overlap_with(&Mark::new(900, 1100, ""), &[0]), Some(1));
        assert_eq!(track.insert_index(1000), 2);
        track.clear_range(900, 2000);
        assert_eq!(spans(&track.marks), vec![(2000, 2500, "c")]);
    }

    #[test]
    fn fixed_marks_fill_the_range() {
        assert_eq!(
            spans(&fixed_marks(400, 1000, 2000).unwrap()),
            vec![(1000, 1400, ""), (1400, 1800, ""), (1800, 2000, "")]
        );
        assert_eq!(
            fixed_marks(5, 0, 1000).unwrap_err(),
            "Marks must be at least 10 ms apart."
        );
        assert_eq!(
            fixed_marks(500, 1000, 1000).unwrap_err(),
            "Choose a time range that ends after it starts."
        );
        assert!(
            fixed_marks(10, 0, 4 * 3_600_000)
                .unwrap_err()
                .contains("at most 500000 are allowed")
        );
    }

    #[test]
    fn every_nth_mark_spans_the_group_it_stands_for() {
        let beats: Vec<Mark> = (0..6)
            .map(|i| Mark::new(i * 500, i * 500 + 500, (i % 4 + 1).to_string()))
            .collect();
        assert_eq!(every_nth_mark(&beats, 1).unwrap(), beats);
        assert_eq!(
            spans(&every_nth_mark(&beats, 4).unwrap()),
            vec![(0, 2000, "1"), (2000, 3000, "1")]
        );
        assert!(every_nth_mark(&beats, 0).is_err());
        assert!(every_nth_mark(&[], 2).unwrap().is_empty());
    }

    #[test]
    fn lyrics_spread_by_letters() {
        let lines = lyric_lines("  Silent night\n\nholy night  \r\nAll is calm, all is bright\n");
        assert_eq!(
            lines,
            vec!["Silent night", "holy night", "All is calm, all is bright"]
        );
        // 11 + 9 + 20 letters over 4 seconds.
        let marks = spread_phrases(&lines, 1000, 5000).unwrap();
        assert_eq!(
            spans(&marks),
            vec![
                (1000, 2100, "Silent night"),
                (2100, 3000, "holy night"),
                (3000, 5000, "All is calm, all is bright")
            ]
        );
        assert_eq!(
            spread_phrases(&[" ".into()], 0, 10).unwrap_err(),
            "Paste at least one line of lyrics."
        );
        assert_eq!(
            spread_phrases(&lines, 0, 2).unwrap_err(),
            "0:00.002 is too short for 3 lines of lyrics."
        );
    }

    #[test]
    fn phrases_break_into_words_by_letters() {
        let phrase = Mark::new(1000, 2000, "Deck  the halls");
        // 4 + 3 + 5 letters.
        assert_eq!(
            spans(&split_words(&phrase).unwrap()),
            vec![(1000, 1333, "Deck"), (1333, 1583, "the"), (1583, 2000, "halls")]
        );
        assert!(split_words(&Mark::new(0, 100, "  ")).unwrap().is_empty());
        assert_eq!(
            split_words(&Mark::new(0, 2, "a b c")).unwrap_err(),
            "The phrase at 0:00.000 is too short to split into 3 words."
        );
        // Every word gets at least a millisecond, however lopsided the letters.
        let tight = split_words(&Mark::new(0, 3, "a b supercalifragilistic")).unwrap();
        assert_eq!(
            spans(&tight),
            vec![(0, 1, "a"), (1, 2, "b"), (2, 3, "supercalifragilistic")]
        );
    }

    #[test]
    fn tidying_file_marks_explains_what_was_left_out() {
        let (kept, report) = tidy_marks(
            vec![
                Mark::new(2000, 3000, "b"),
                Mark::new(0, 1000, "a"),
                Mark::new(500, 1500, "overlap"),
                Mark::new(4000, 4000, "point"),
                Mark::new(9000, 12_000, "cut"),
                Mark::new(10_000, 11_000, "late"),
            ],
            10_000,
        );
        assert_eq!(
            spans(&kept),
            vec![(0, 1000, "a"), (2000, 3000, "b"), (9000, 10_000, "cut")]
        );
        assert_eq!(
            report,
            TidyReport {
                no_length: 1,
                overlapping: 1,
                outside: 1,
                cut: 1
            }
        );
        assert_eq!(
            report.notes("song.txt"),
            vec![
                "1 mark in song.txt had no length and was left out.",
                "1 mark in song.txt overlapped the mark before and was left out.",
                "1 mark in song.txt started after the end of the sequence and was left out.",
                "1 mark in song.txt ran past the end of the sequence and was cut short."
            ]
        );
    }

    #[test]
    fn audacity_labels_round_trip() {
        let marks = vec![
            Mark::new(0, 1500, "Silent night"),
            Mark::new(1500, 3250, "holy\tnight"),
        ];
        let text = audacity_labels(&marks);
        assert_eq!(
            text,
            "0.000000\t1.500000\tSilent night\n1.500000\t3.250000\tholy night\n"
        );
        let back = parse_audacity_labels(&text).unwrap();
        assert_eq!(
            spans(&back),
            vec![(0, 1500, "Silent night"), (1500, 3250, "holy night")]
        );
    }

    #[test]
    fn audacity_point_labels_and_bad_files() {
        let text = "1.0\t1.0\tone\n\\\t200.0\t3000.0\n0.5\t0.5\r\n\n2\t2\tlast\n";
        assert_eq!(
            spans(&parse_audacity_labels(text).unwrap()),
            vec![(1000, 2000, "one"), (500, 1000, ""), (2000, 2500, "last")]
        );
        assert_eq!(
            parse_audacity_labels("1.0 2.0 words\n").unwrap_err(),
            "line 1 isn't an Audacity label (start and end in seconds, then the label, separated by tabs)"
        );
        assert!(parse_audacity_labels("nan\t1\tx").is_err());
        assert!(parse_audacity_labels("-1\t1\tx").is_err());
        assert_eq!(
            parse_audacity_labels("\n \n").unwrap_err(),
            "it has no labels in it"
        );
        let huge = "x".repeat(MAX_TIMING_FILE_BYTES + 1);
        assert!(parse_audacity_labels(&huge).unwrap_err().contains("up to 16 MB"));
    }
}
