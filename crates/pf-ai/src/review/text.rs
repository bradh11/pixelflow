//! Whether a Text effect's words can be read on the grid they're drawn on, from the way the
//! renderer lays text out (`pf_render`'s Text): a 5×7 font (8 with descenders) scaled to the
//! letter height, six dots from one letter to the next, and moving text crossing the grid at
//! two and a half cells a second for each point of speed.

use pf_sequence::{Effect, TextCountdown, TextMovement, TextOrientation, TextParams};
use serde_json::{Map, Value};

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
    if size < SMALLEST_LETTERS {
        return Some(format!(
            "at a letter height of {size} the font loses dots; it needs {SMALLEST_LETTERS} or more (8 is its own size)."
        ));
    }
    let stacked = text.orientation != TextOrientation::Across;
    let tall = if stacked {
        letters as u32 * size
    } else {
        lines.len() as u32 * size
    };
    let moves_up = matches!(text.movement, TextMovement::Up | TextMovement::Down);
    if tall > rows && !moves_up {
        return Some(if stacked {
            format!("{letters} letters stacked at a height of {size} need {tall} rows, and it has {rows}.")
        } else if lines.len() > 1 {
            format!(
                "{} lines at a letter height of {size} need {tall} rows, and it has {rows}.",
                lines.len()
            )
        } else {
            format!("letters {size} tall don't fit its {rows} rows.")
        });
    }
    let across = if stacked {
        width(1, size)
    } else {
        width(longest, size)
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
        let fit = ((columns as f32 * FONT_HEIGHT as f32 / size as f32 + 1.0) / ADVANCE).floor() as u32;
        return Some(format!(
            "{longest} letters at a height of {size} are {:.0} columns wide, and it has {columns}: {fit} letters fit. Scroll it (movement left), make the letters smaller, or split the words over several effects.",
            across
        ));
    }
    if text.movement == TextMovement::None {
        let needs = STILL_MS + STILL_MS_A_LETTER * letters as u64;
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
        let across = width(fixed.text.chars().count(), fixed.size);
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
