//! Sing: any prop sings along with a timing track, no face needed. PixelFlow's own.
//!
//! The mouth opens by the sound under the playhead, from the same mouth shapes the Faces effect
//! uses: a phonemes track's marks, or the shapes of a word or syllable (see
//! [`crate::faces::word_phonemes`]) spread evenly over its mark. Wide for AI and O, half open for
//! E, U, L, and WQ, nearly shut for FV, shut for MBP and at rest. Marks with no letters (vocals,
//! beats) open it wide. On a word or syllable track it closes at the end of each mark, so syllables
//! sung back to back still pulse.
//!
//! - **Mouth:** the whole prop as bright as the mouth is open.
//! - **Word pop:** a flash on each mark, fading over it, a palette color per mark.
//! - **Mouth bar:** a band across the middle of the prop (along it, on a line) as wide as the
//!   mouth is open.
//! - **Karaoke fill:** the first palette color filling the prop left to right over each mark, the
//!   rest in the second color at the closed brightness.

use crate::audio::RenderContext;
use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::faces::word_phonemes;
use crate::geometry::Pixel;
use crate::wipe::sweep_position;
use pf_model::Phoneme;
use pf_sequence::{Mark, SingMode, SingParams, Sweep, TimingKind, TimingTrack};

/// How far the mouth opens (0–1) for a mouth shape.
pub fn openness(phoneme: Phoneme) -> f32 {
    match phoneme {
        Phoneme::Ai => 1.0,
        Phoneme::O => 0.9,
        Phoneme::E => 0.75,
        Phoneme::U => 0.6,
        Phoneme::Wq | Phoneme::L => 0.5,
        Phoneme::Etc => 0.45,
        Phoneme::Fv => 0.3,
        Phoneme::Mbp | Phoneme::Rest => 0.0,
    }
}

/// How open a mark with no letters (a vocals or beat mark) sings.
const NO_LETTERS: f32 = 0.8;
/// The longest the mouth takes to close at the end of a word or syllable, and its most as a
/// share of the mark.
const CLOSE_MS: f32 = 60.0;
const CLOSE_SHARE: f32 = 0.15;

/// The mark under `t_ms` on `track`: its number and the mark.
fn mark_at(track: &TimingTrack, t_ms: u64) -> Option<(usize, &Mark)> {
    let i = track
        .marks
        .partition_point(|m| m.start_ms <= t_ms)
        .checked_sub(1)?;
    let mark = &track.marks[i];
    (t_ms < mark.end_ms).then_some((i, mark))
}

/// How open the mouth is at `t_ms` on `track` (0 between marks).
pub fn mouth_open(track: &TimingTrack, t_ms: u64) -> f32 {
    let Some((_, mark)) = mark_at(track, t_ms) else {
        return 0.0;
    };
    if track.kind == TimingKind::Phonemes {
        return Phoneme::from_name(&mark.label).map_or(NO_LETTERS, openness);
    }
    let length = (mark.end_ms - mark.start_ms).max(1);
    let shapes = word_phonemes(&mark.label);
    let open = if shapes.is_empty() {
        NO_LETTERS
    } else {
        let k = ((t_ms - mark.start_ms) as u128 * shapes.len() as u128 / u128::from(length)) as usize;
        openness(shapes[k.min(shapes.len() - 1)])
    };
    let close = (length as f32 * CLOSE_SHARE).clamp(1.0, CLOSE_MS);
    open * unit((mark.end_ms - t_ms) as f32 / close)
}

#[derive(Debug, Clone, Copy)]
enum Look {
    /// Everywhere at one brightness, in one color.
    Even([f32; 3], f32),
    /// A band around the middle, `half` wide on each side.
    Bar([f32; 3], f32),
    /// Filled `progress` of the way across.
    Fill([f32; 3], [f32; 3], f32),
    Dark,
}

pub struct Sing {
    look: Look,
    min: f32,
    canvas: Canvas,
}

impl Sing {
    pub fn new(
        p: &SingParams,
        time: &EffectTime,
        colors: Colors,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        let min = unit(p.min);
        let t_ms = time.start_ms + time.elapsed_ms;
        let track = cx.track(p.timing_track);
        let look = match (p.mode, track) {
            (_, None) => Look::Dark,
            (SingMode::Mouth, Some(track)) => {
                Look::Even(colors.get(0), min + (1.0 - min) * mouth_open(track, t_ms))
            }
            (SingMode::BarMouth, Some(track)) => Look::Bar(colors.get(0), mouth_open(track, t_ms) / 2.0),
            (SingMode::WordPop, Some(track)) => match mark_at(track, t_ms) {
                Some((i, mark)) => {
                    let x = (t_ms - mark.start_ms) as f32 / (mark.end_ms - mark.start_ms).max(1) as f32;
                    Look::Even(colors.get(i as u64), min + (1.0 - min) * (-3.0 * x).exp())
                }
                None => Look::Even(colors.get(0), min),
            },
            (SingMode::Karaoke, Some(track)) => match mark_at(track, t_ms) {
                Some((_, mark)) => {
                    let x = (t_ms - mark.start_ms) as f32 / (mark.end_ms - mark.start_ms).max(1) as f32;
                    Look::Fill(colors.get(0), colors.get(1), x)
                }
                None => Look::Dark,
            },
        };
        Self { look, min, canvas }
    }
}

impl Shade for Sing {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (color, level) = match self.look {
            Look::Dark => return Rgba::CLEAR,
            Look::Even(color, level) => (color, level),
            Look::Bar(color, half) => {
                // Up the middle of a matrix; along a line or outline.
                let at = if self.canvas.rows <= 1 { px.u } else { px.v };
                let inside = (at - 0.5).abs() <= half && half > 0.0;
                (color, if inside { 1.0 } else { self.min })
            }
            Look::Fill(sung, ahead, progress) => {
                if sweep_position(px, Sweep::LeftToRight, self.canvas) <= progress {
                    (sung, 1.0)
                } else {
                    (ahead, self.min)
                }
            }
        };
        if level <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba::with_alpha(color, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_vowels_open_widest_and_lips_shut() {
        assert!(openness(Phoneme::Ai) > openness(Phoneme::E));
        assert!(openness(Phoneme::O) > openness(Phoneme::Fv));
        assert_eq!(openness(Phoneme::Mbp), 0.0);
        assert_eq!(openness(Phoneme::Rest), 0.0);
    }

    #[test]
    fn the_mouth_follows_phonemes_and_closes_between_syllables() {
        let phonemes = TimingTrack::new(
            "Lyrics (phonemes)",
            TimingKind::Phonemes,
            vec![
                Mark::new(0, 100, "MBP"),
                Mark::new(100, 200, "AI"),
                Mark::new(200, 300, "O"),
            ],
        );
        assert_eq!(mouth_open(&phonemes, 50), 0.0);
        assert_eq!(mouth_open(&phonemes, 150), 1.0, "phonemes run into each other");
        assert_eq!(mouth_open(&phonemes, 199), 1.0);
        assert_eq!(mouth_open(&phonemes, 250), 0.9);
        assert_eq!(mouth_open(&phonemes, 400), 0.0, "rest between marks");
        // Made-up syllables: open on the vowel, closing at each one's end.
        let syllables = TimingTrack::new(
            "Lyrics (syllables)",
            TimingKind::Custom,
            vec![Mark::new(0, 400, "ma"), Mark::new(400, 800, "ma")],
        );
        assert!(
            mouth_open(&syllables, 300) > 0.9,
            "{}",
            mouth_open(&syllables, 300)
        );
        assert!(mouth_open(&syllables, 399) < 0.1, "closes before the next");
        assert!(mouth_open(&syllables, 700) > 0.9);
        let vocals = TimingTrack::new("Vocals", TimingKind::Custom, vec![Mark::new(0, 1000, "")]);
        assert_eq!(mouth_open(&vocals, 500), NO_LETTERS);
    }
}
