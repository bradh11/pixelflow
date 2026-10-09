//! Where to look for each line: its rough time a second either side ([`PAD_MS`]). Lines whose
//! stretches overlap are aligned together, so two lines never claim the same sound, up to
//! [`MAX_WINDOW_MS`]; past that, two neighbours split the stretch between them.

use std::ops::Range;

/// How far either side of a line's rough time it's looked for (ms).
pub const PAD_MS: u64 = 1_000;
/// The longest stretch aligned as one (ms).
pub const MAX_WINDOW_MS: u64 = 45_000;

/// A sung line: its words and roughly when it's sung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub words: Vec<String>,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Lines aligned together within `start_ms..end_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub lines: Range<usize>,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The stretches to align `lines` (in order) within, each padded by `pad_ms` and kept inside
/// `0..song_ms`; overlapping ones merged while no longer than `max_ms`, else split halfway
/// between the two lines.
pub fn windows(lines: &[Line], pad_ms: u64, max_ms: u64, song_ms: u64) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let start = line.start_ms.saturating_sub(pad_ms).min(song_ms);
        let end = line.end_ms.max(line.start_ms).saturating_add(pad_ms).min(song_ms);
        match out.last_mut() {
            Some(last) if start < last.end_ms => {
                if end.max(last.end_ms) - last.start_ms <= max_ms {
                    last.lines.end = i + 1;
                    last.end_ms = last.end_ms.max(end);
                } else {
                    // Halfway between the line before's end and this one's start.
                    let before = &lines[i - 1];
                    let middle = (before.end_ms.min(line.start_ms) + line.start_ms.max(before.end_ms)) / 2;
                    let middle = middle.clamp(last.start_ms, end);
                    last.end_ms = middle.max(last.start_ms);
                    out.push(Window {
                        lines: i..i + 1,
                        start_ms: middle,
                        end_ms: end.max(middle),
                    });
                }
            }
            _ => out.push(Window {
                lines: i..i + 1,
                start_ms: start,
                end_ms: end.max(start),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start_ms: u64, end_ms: u64) -> Line {
        Line {
            words: vec!["la".into()],
            start_ms,
            end_ms,
        }
    }

    fn spans(w: &[Window]) -> Vec<(Range<usize>, u64, u64)> {
        w.iter()
            .map(|w| (w.lines.clone(), w.start_ms, w.end_ms))
            .collect()
    }

    #[test]
    fn lines_far_apart_get_their_own_padded_stretch() {
        let w = windows(&[line(500, 2_000), line(10_000, 12_000)], 1_000, 45_000, 60_000);
        assert_eq!(spans(&w), [(0..1, 0, 3_000), (1..2, 9_000, 13_000)]);
        // The song's end bounds the last.
        let w = windows(&[line(58_000, 59_500)], 1_000, 45_000, 60_000);
        assert_eq!(spans(&w), [(0..1, 57_000, 60_000)]);
    }

    #[test]
    fn overlapping_lines_are_aligned_together_up_to_the_longest_stretch() {
        let lines = [line(1_000, 3_000), line(3_500, 6_000), line(6_200, 9_000)];
        let w = windows(&lines, 1_000, 45_000, 60_000);
        assert_eq!(spans(&w), [(0..3, 0, 10_000)]);
        // Too long together: split halfway between a line's end and the next one's start.
        let w = windows(&lines, 1_000, 6_000, 60_000);
        assert_eq!(
            spans(&w),
            [(0..1, 0, 3_250), (1..2, 3_250, 6_100), (2..3, 6_100, 10_000)]
        );
        for pair in w.windows(2) {
            assert!(pair[0].end_ms <= pair[1].start_ms);
        }
    }
}
