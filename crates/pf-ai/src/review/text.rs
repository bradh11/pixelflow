//! Whether a Text effect's words can be read on the grid they're drawn on, from the way the
//! renderer lays text out (`pf_render`'s Text): a 5×7 font (8 with descenders) scaled to the
//! letter height, six dots from one letter to the next, or a font on the computer measured at
//! its size; and moving text crossing the grid at two and a half cells a second for each point
//! of speed.

use pf_fonts::SizedFont;
use pf_sequence::{Effect, TextCountdown, TextMovement, TextOrientation, TextParams};
use serde_json::{Map, Value};
use std::sync::Arc;

/// The fewest rows the font's letters need to keep their shapes.
pub(super) const SMALLEST_LETTERS: u32 = 7;
/// The font's own letter height: below it, dots are dropped as it's scaled down.
const FONT_HEIGHT: u32 = 8;
/// Font dots from one letter to the next.
const ADVANCE: f32 = 6.0;
/// Cells a second moving text travels for each point of speed.
const CELLS_A_SECOND: f32 = 2.5;
/// The shortest a letter of moving text may stay on the grid and still be read (seconds).
const LETTER_ON_SCREEN: f32 = 0.5;
/// The shortest still text may show: this long, and this much more a letter (ms).
const STILL_MS: u64 = 300;
const STILL_MS_A_LETTER: u64 = 60;

/// How wide `letters` letters are at `size`, in cells.
fn width(letters: usize, size: u32) -> f32 {
    (letters as f32 * ADVANCE - 1.0).max(0.0) * size as f32 / FONT_HEIGHT as f32
}

/// What the words are drawn in: the pixel font, or a font on this computer at the effect's size
/// (a font the computer hasn't got is drawn in the pixel font, so it's measured as that).
enum Lettering {
    Pixel,
    Font(String, Arc<SizedFont>),
}

impl Lettering {
    fn of(text: &TextParams, size: u32) -> Self {
        let name = text.font.trim();
        match pf_fonts::sized(name, size as f32, text.bold, text.italic) {
            Some(font) if !name.is_empty() => Lettering::Font(name.to_string(), font),
            _ => Lettering::Pixel,
        }
    }

    /// How wide `line` is at `size`, in cells.
    fn width(&self, line: &str, size: u32) -> f32 {
        match self {
            Lettering::Pixel => width(line.chars().count(), size),
            Lettering::Font(_, font) => {
                let mut last: Option<Arc<pf_fonts::Glyph>> = None;
                let mut across = 0.0;
                for c in line.chars() {
                    let glyph = font.glyph(c);
                    if let Some(last) = &last {
                        across += font.kerning(last, &glyph);
                    }
                    across += glyph.advance;
                    last = Some(glyph);
                }
                across
            }
        }
    }

    /// How many rows a line takes at `size`.
    fn height(&self, size: u32) -> u32 {
        match self {
            Lettering::Pixel => size,
            Lettering::Font(_, font) => (font.ascent + font.descent).round().max(1.0) as u32,
        }
    }
}

/// The letters still `text` has, as reading counts them: every character but the line breaks.
fn letters_in(text: &str) -> usize {
    text.chars().filter(|c| *c != '\n').count()
}

/// How long still `text` must show to be read (ms): [`STILL_MS`], and [`STILL_MS_A_LETTER`]
/// more for each letter. The one rule: the review checks it, and the tools that place words
/// hold them this long.
pub(crate) fn read_ms(text: &str) -> u64 {
    STILL_MS + STILL_MS_A_LETTER * letters_in(text) as u64
}

/// The most letters a line holds across `columns` at a letter height of `size`.
fn letters_across(size: u32, columns: u32) -> usize {
    let mut n = 1;
    while width(n + 1, size) <= columns as f32 {
        n += 1;
    }
    n
}

/// A word cut into lines that fit `across` letters each, as evenly as can be ("LANT", "ERNS").
fn cut(word: &str, across: usize) -> Vec<String> {
    let letters: Vec<char> = word.chars().collect();
    let lines = letters.len().div_ceil(across.max(1)).max(1);
    let each = letters.len().div_ceil(lines);
    letters.chunks(each.max(1)).map(|c| c.iter().collect()).collect()
}

/// `lines` with `word` after them: on the last line when it fits, else on lines of its own.
/// `None` when that would be more lines than `most`.
fn with_word(lines: &[String], word: &str, across: usize, most: usize) -> Option<Vec<String>> {
    let mut out = lines.to_vec();
    match out.last_mut() {
        Some(last) if last.chars().count() + 1 + word.chars().count() <= across => {
            last.push(' ');
            last.push_str(word);
        }
        _ => out.extend(cut(word, across)),
    }
    (out.len() <= most).then_some(out)
}

/// A gap between two words this short is bridged: the first stays up until the next comes,
/// rather than the grid blinking dark between them.
const BRIDGE_MS: u64 = 250;

/// The sung words `words` (each mark's start, end, and words, in order) as still text on a grid
/// of `columns` by `rows` at a letter height of `size`, ending by `until`: each shown from when it
/// is sung for as long as [`read_ms`] says it takes to read, two or three short words sharing the
/// grid when one alone would be gone too soon, a word too wide for the grid cut over two lines,
/// and a word that comes while the one before is still being read waiting for it. Each piece:
/// its start, its end, and its text (capitals, lines split by \n).
pub(crate) fn sung_words(
    words: &[(u64, u64, String)],
    until: u64,
    size: u32,
    columns: u32,
    rows: u32,
) -> Vec<(u64, u64, String)> {
    let across = letters_across(size.max(1), columns);
    let most = (rows / size.max(1)).max(1) as usize;
    let said: Vec<String> = words
        .iter()
        .map(|(_, _, label)| {
            label
                .split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\''))
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .to_uppercase()
        })
        .collect();
    let mut out: Vec<(u64, u64, String)> = Vec::new();
    let mut free_from = 0;
    let mut i = 0;
    while i < words.len() {
        if said[i].is_empty() {
            i += 1;
            continue;
        }
        let start = words[i].0.max(free_from);
        if start >= until {
            break;
        }
        let mut lines: Vec<String> = Vec::new();
        for word in said[i].split(' ') {
            lines = with_word(&lines, word, across, usize::MAX).unwrap_or_default();
        }
        let mut next = i + 1;
        // Short words share the grid until the words up can be read before the next is sung.
        while next < words.len() {
            let comes = words[next].0.min(until);
            if comes >= start + read_ms(&lines.join("\n")) {
                break;
            }
            let more = said[next]
                .split(' ')
                .try_fold(lines.clone(), |acc, word| with_word(&acc, word, across, most));
            match more {
                Some(more) => {
                    lines = more;
                    next += 1;
                }
                None => break,
            }
        }
        let text = lines.join("\n");
        let mut end = (start + read_ms(&text)).max(words[next - 1].1);
        if let Some(comes) = words.get(next).map(|w| w.0)
            && end < comes
            && comes - end < BRIDGE_MS
        {
            end = comes;
        }
        let end = end.min(until);
        if end > start {
            out.push((start, end, text));
        }
        free_from = end;
        i = next;
    }
    out
}

/// Why `text`'s words can't be read on a grid of `columns` by `rows`, if they can't.
pub(super) fn problem(text: &TextParams, effect: &Effect, columns: u32, rows: u32) -> Option<String> {
    // A countdown writes its own digits.
    if text.countdown != TextCountdown::None {
        return None;
    }
    let size = text.size.clamp(4, 100);
    let lines: Vec<&str> = text.text.split('\n').collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let letters: usize = lines.iter().map(|l| l.chars().count()).sum();
    let lettering = Lettering::of(text, size);
    if size < SMALLEST_LETTERS {
        return Some(match &lettering {
            Lettering::Pixel => format!(
                "at a letter height of {size} the font loses dots; it needs {SMALLEST_LETTERS} or more (8 is its own size)."
            ),
            Lettering::Font(name, _) => format!(
                "at a size of {size}, {name}'s letters are too few pixels to tell apart; it needs {SMALLEST_LETTERS} or more."
            ),
        });
    }
    let stacked = text.orientation != TextOrientation::Across;
    let line_rows = lettering.height(size);
    let tall = if stacked {
        letters as u32 * line_rows
    } else {
        lines.len() as u32 * line_rows
    };
    let moves_up = matches!(text.movement, TextMovement::Up | TextMovement::Down);
    if tall > rows && !moves_up {
        return Some(match (&lettering, stacked, lines.len() > 1) {
            (_, true, _) => format!(
                "{letters} letters stacked at a height of {size} need {tall} rows, and it has {rows}."
            ),
            (_, false, true) => format!(
                "{} lines at a letter height of {size} need {tall} rows, and it has {rows}.",
                lines.len()
            ),
            (Lettering::Pixel, false, false) => format!("letters {size} tall don't fit its {rows} rows."),
            (Lettering::Font(name, _), false, false) => {
                format!("{name} at a size of {size} takes {tall} rows, and it has {rows}.")
            }
        });
    }
    let across = if stacked {
        text.text
            .chars()
            .filter(|c| *c != '\n')
            .map(|c| lettering.width(&c.to_string(), size))
            .fold(0.0, f32::max)
    } else {
        lines.iter().map(|l| lettering.width(l, size)).fold(0.0, f32::max)
    };
    let seconds = effect.duration_ms() as f32 / 1000.0;
    let sideways = matches!(text.movement, TextMovement::Left | TextMovement::Right);
    if sideways {
        if text.speed == 0 {
            return None;
        }
        let cells = CELLS_A_SECOND * text.speed as f32;
        let on_screen = columns as f32 / cells;
        if on_screen < LETTER_ON_SCREEN {
            return Some(format!(
                "each letter crosses its {columns} columns in {on_screen:.1} s. A speed of {} or less keeps a letter in view for half a second.",
                ((columns as f32 / (CELLS_A_SECOND * LETTER_ON_SCREEN)).floor() as u32).max(1)
            ));
        }
        let crossing = (columns as f32 + across) / cells;
        if seconds + 0.05 < crossing && !text.to_center {
            return Some(format!(
                "the effect lasts {seconds:.1} s and the words need {crossing:.1} s to cross at this speed. Make it longer, or faster (up to the limit above), or the words shorter."
            ));
        }
        return None;
    }
    if across > columns as f32 && !moves_up && text.movement != TextMovement::Vector {
        let fit = match &lettering {
            Lettering::Pixel => {
                ((columns as f32 * FONT_HEIGHT as f32 / size as f32 + 1.0) / ADVANCE).floor() as u32
            }
            // As many letters as fit at the widest line's own spacing.
            Lettering::Font(..) => (columns as f32 * longest as f32 / across).floor() as u32,
        };
        return Some(format!(
            "{longest} letters at a height of {size} are {:.0} columns wide, and it has {columns}: {fit} letters fit. Scroll it (movement left), make the letters smaller, or split the words over several effects.",
            across
        ));
    }
    if text.movement == TextMovement::None {
        let needs = read_ms(&text.text);
        if effect.duration_ms() < needs {
            return Some(format!(
                "it shows for {} ms; {letters} letters want about {needs} ms to be read.",
                effect.duration_ms()
            ));
        }
    }
    None
}

/// The settings that make `text`'s words readable on a grid of `columns` by `rows` for as long
/// as `effect` lasts, when changing settings alone can do it: letters at the font's own height
/// (or as tall as fit), and, when the words are too wide to stand still or move at the wrong
/// speed, crossing once in the time there is. `None` when no setting does it (too many words for
/// the time, stacked letters, several lines): that takes a decision.
pub(super) fn readable(text: &TextParams, effect: &Effect, columns: u32, rows: u32) -> Option<Value> {
    if text.countdown != TextCountdown::None
        || text.orientation != TextOrientation::Across
        || text.text.contains('\n')
        || rows < SMALLEST_LETTERS
    {
        return None;
    }
    let mut fixed = text.clone();
    let mut settings = Map::new();
    if fixed.size < SMALLEST_LETTERS || fixed.size > rows {
        fixed.size = FONT_HEIGHT.min(rows).max(SMALLEST_LETTERS);
        settings.insert("size".into(), fixed.size.into());
    }
    if problem(&fixed, effect, columns, rows).is_some() {
        let across = Lettering::of(&fixed, fixed.size).width(&fixed.text, fixed.size);
        let seconds = (effect.duration_ms() as f32 / 1000.0).max(0.1);
        let speed = ((columns as f32 + across) / (seconds * CELLS_A_SECOND)).ceil();
        fixed.speed = (speed as u32).clamp(1, 100);
        settings.insert("speed".into(), fixed.speed.into());
        if !matches!(fixed.movement, TextMovement::Left | TextMovement::Right) {
            fixed.movement = TextMovement::Left;
            settings.insert("movement".into(), "left".into());
        }
        if fixed.to_center {
            fixed.to_center = false;
            settings.insert("toCenter".into(), false.into());
        }
    }
    (!settings.is_empty() && problem(&fixed, effect, columns, rows).is_none())
        .then_some(Value::Object(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{EffectKind, EffectParams};

    #[test]
    fn settings_that_make_words_readable_are_worked_out_when_settings_can() {
        // Too small: the font's own height. It then fits standing still.
        let (params, effect) = text("JOY", |p| p.size = 5, 4_000);
        assert_eq!(
            readable(&params, &effect, 40, 20),
            Some(serde_json::json!({ "size": 8 }))
        );
        // Too wide to stand still: once across in the four seconds there are (40 columns and
        // 95 of words at two and a half columns a second for each point of speed).
        let (params, effect) = text("MERRY AND BRIGHT", |_| {}, 4_000);
        assert_eq!(
            readable(&params, &effect, 40, 20),
            Some(serde_json::json!({ "movement": "left", "speed": 14 }))
        );
        // Scrolling too fast to read, the other way: slowed, still going right.
        let (params, effect) = text(
            "JOY",
            |p| {
                p.movement = TextMovement::Right;
                p.speed = 60;
            },
            4_000,
        );
        assert_eq!(
            readable(&params, &effect, 40, 20),
            Some(serde_json::json!({ "speed": 6 }))
        );
        // Every answer leaves nothing to fix.
        for (words, ms) in [
            ("JOY", 4_000),
            ("MERRY AND BRIGHT", 4_000),
            ("LANTERNS HIGH ABOVE", 8_000),
        ] {
            let (mut params, effect) = text(words, |p| p.size = 4, ms);
            let settings = readable(&params, &effect, 40, 20).unwrap();
            params.size = settings["size"].as_u64().unwrap() as u32;
            if let Some(speed) = settings["speed"].as_u64() {
                (params.speed, params.movement) = (speed as u32, TextMovement::Left);
            }
            assert_eq!(problem(&params, &effect, 40, 20), None, "{words}");
        }
        // Too many words for the time, or nothing wrong: no settings to give.
        let (params, effect) = text("EVERY WINDOW STARTS TO GLOW TONIGHT AND TOMORROW", |_| {}, 1_000);
        assert_eq!(readable(&params, &effect, 40, 20), None);
        let (params, effect) = text("JOY", |_| {}, 4_000);
        assert_eq!(readable(&params, &effect, 40, 20), None);
        let (params, effect) = text("JOY\nJOY", |p| p.size = 5, 4_000);
        assert_eq!(readable(&params, &effect, 40, 20), None);
    }

    /// The stage's chorus lines, as its words track times them: six words over 3.5 s, and five.
    fn chorus_words() -> Vec<(u64, u64, String)> {
        let mut out = Vec::new();
        for (start, line) in [
            (24_000, "Light it up, light it up"),
            (32_000, "Lanterns high above the town"),
        ] {
            let sung: Vec<&str> = line.split_whitespace().collect();
            let each = 3_500 / sung.len() as u64;
            for (i, word) in sung.iter().enumerate() {
                let from = start + i as u64 * each;
                out.push((from, from + each - 60, word.to_string()));
            }
        }
        out
    }

    #[test]
    fn sung_words_are_held_long_enough_to_read_by_the_reviews_own_rule() {
        let words = chorus_words();
        let shown = sung_words(&words, 40_000, 8, 40, 20);
        let texts: Vec<&str> = shown.iter().map(|s| s.2.as_str()).collect();
        // "LIGHT" alone would be gone in 583 ms (600 wanted): "IT" joins it, on a line of its
        // own (eight letters across don't fit 40 columns). "LANTERNS" is too wide for one line,
        // so it's cut over two, and "HIGH" waits the 80 ms it takes to read.
        assert_eq!(
            texts,
            [
                "LIGHT\nIT",
                "UP",
                "LIGHT\nIT",
                "UP",
                "LANT\nERNS",
                "HIGH",
                "ABOVE",
                "THE",
                "TOWN"
            ]
        );
        for (start, end, words) in &shown {
            let (params, effect) = text(words, |_| {}, end - start);
            assert_eq!(problem(&params, &effect, 40, 20), None, "{words} at {start}");
        }
        // Nothing overlaps, each starts as (or just after) it is sung, and a word sung a
        // moment after the last is up straight away: no blink between them.
        for pair in shown.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "{pair:?}");
        }
        assert_eq!((shown[0].0, shown[0].1), (24_000, 25_166));
        assert_eq!(shown[5].0, 32_780);
        // The last word of a line isn't held into the silence after it.
        assert_eq!(shown[3].1, 27_438);
        // And none reaches past the end given.
        let cut_short = sung_words(&words, 33_000, 8, 40, 20);
        assert!(cut_short.iter().all(|s| s.1 <= 33_000), "{cut_short:?}");
        // On a wider grid "LIGHT IT" fits one line.
        assert_eq!(sung_words(&words, 40_000, 8, 60, 20)[0].2, "LIGHT IT");
    }

    #[test]
    fn the_rule_counts_letters_as_the_review_does() {
        assert_eq!(read_ms("JOY"), 480);
        assert_eq!(read_ms("LANT\nERNS"), 780);
        assert_eq!(read_ms("LIGHT IT"), 780);
    }

    fn text(words: &str, change: impl Fn(&mut TextParams), ms: u64) -> (TextParams, Effect) {
        let mut params = TextParams {
            text: words.to_string(),
            ..TextParams::default()
        };
        change(&mut params);
        let effect = Effect::new(EffectKind::Text, 0, ms).with_params(EffectParams::Text(params.clone()));
        (params, effect)
    }

    #[test]
    fn words_in_a_computer_font_are_measured_in_that_font() {
        // The tiny test font: at 20 pixels each box letter moves on 12 columns and a line takes
        // 20 rows (the pixel font's letters at 20 would take 15 columns each).
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            pf_fonts::add_font(pf_fonts::testing::tiny_font(
                "PixelFlow Review Sans",
                false,
                false,
            ));
        });
        let font = |p: &mut TextParams| {
            p.font = "PixelFlow Review Sans".into();
            p.size = 20;
        };
        let (params, effect) = text("ABC", font, 4_000);
        // 36 columns: it fits 40 standing still, where the pixel font's 42.5 wouldn't.
        assert_eq!(problem(&params, &effect, 40, 24), None);
        let pixel = TextParams {
            font: String::new(),
            ..params.clone()
        };
        let pixel_said = problem(&pixel, &effect, 40, 24).unwrap();
        assert!(pixel_said.contains("42 columns wide"), "{pixel_said}");
        let wide = problem(&params, &effect, 30, 24).unwrap();
        assert!(
            wide.contains("36 columns wide, and it has 30: 2 letters fit"),
            "{wide}"
        );
        let tall = problem(&params, &effect, 40, 18).unwrap();
        assert_eq!(
            tall,
            "PixelFlow Review Sans at a size of 20 takes 20 rows, and it has 18."
        );
        // A font this computer hasn't got is drawn, and so measured, in the pixel font.
        let (missing, effect) = text(
            "ABC",
            |p| {
                p.font = "PixelFlow Made Up Narrow".into();
                p.size = 20;
            },
            4_000,
        );
        let missing_said = problem(&missing, &effect, 40, 24).unwrap();
        assert!(missing_said.contains("42 columns wide"), "{missing_said}");
    }

    #[test]
    fn words_that_fit_and_stay_long_enough_can_be_read() {
        // "JOY" at the font's own size is 17 columns wide: it fits a 40 by 20 matrix.
        let (params, effect) = text("JOY", |_| {}, 2_000);
        assert_eq!(problem(&params, &effect, 40, 20), None);
        // Scrolling at the usual speed, a letter is in view for 1.6 s, and the words cross in 2.3 s.
        let (params, effect) = text("JOY", |p| p.movement = TextMovement::Left, 4_000);
        assert_eq!(problem(&params, &effect, 40, 20), None);
    }

    #[test]
    fn words_too_wide_too_tall_too_small_too_fast_or_too_brief_are_explained() {
        let said = |words: &str, change: &dyn Fn(&mut TextParams), ms: u64, grid: (u32, u32)| {
            let (params, effect) = text(words, change, ms);
            problem(&params, &effect, grid.0, grid.1).unwrap_or_default()
        };
        let wide = said("MERRY AND BRIGHT", &|_| {}, 4_000, (40, 20));
        assert!(
            wide.contains("16 letters") && wide.contains("6 letters fit"),
            "{wide}"
        );
        let tall = said("JOY", &|p| p.size = 24, 4_000, (40, 20));
        assert!(tall.contains("letters 24 tall don't fit its 20 rows"), "{tall}");
        let small = said("JOY", &|p| p.size = 5, 4_000, (40, 20));
        assert!(small.contains("loses dots"), "{small}");
        let fast = said(
            "JOY",
            &|p| {
                p.movement = TextMovement::Left;
                p.speed = 60;
            },
            4_000,
            (40, 20),
        );
        assert!(fast.contains("A speed of 32 or less"), "{fast}");
        let cut = said(
            "MERRY AND BRIGHT",
            &|p| p.movement = TextMovement::Left,
            2_000,
            (40, 20),
        );
        assert!(cut.contains("lasts 2.0 s and the words need 5.4 s"), "{cut}");
        let brief = said("JOY", &|_| {}, 200, (40, 20));
        assert!(brief.contains("shows for 200 ms"), "{brief}");
        let stacked = said(
            "JOY",
            &|p| p.orientation = TextOrientation::StackedDown,
            4_000,
            (40, 20),
        );
        assert!(
            stacked.contains("3 letters stacked at a height of 8 need 24 rows"),
            "{stacked}"
        );
    }
}
