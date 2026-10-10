//! Dancer: a character drawn by code that dances to the beat. PixelFlow's own.
//!
//! Pictures and GIFs don't survive a 12 × 50 pillar, so the characters are drawn fresh at the
//! size of whatever they're on: a skeleton, a ghost, a witch, Santa, a snowman, and an elf, each
//! built on a small rig (see `figures.rs`) and posed by keyed moves (see `moves.rs`).
//!
//! - **The beat:** the marks of the effect's timing track; without one (or with one the sequence
//!   lost), the song's Beats track; without that, two beats a second from the effect's start.
//!   Before the first mark and after the last, the beat carries on at the pace of the nearest
//!   marks. Bars are four beats, counted from the first mark labeled "1" when the marks are
//!   numbered within their bars (as the song's own are). Half time and double time stretch that
//!   clock.
//! - **The dance:** a pose on each beat, a move per bar, the same for every dancer with the same
//!   routine number wherever its effect starts, so dancers on different props move together. A
//!   small dip on every beat, the head a moment behind, and a deeper one with the bass when
//!   asked.
//! - **The picture:** each dancer stands in its own share of the target's columns, as tall as
//!   `size` says or as its share's width allows, drawn fresh for the frame from the time alone.
//!   Mirrored, the whole picture is flipped left to right.

mod figures;
mod moves;
mod paint;

use crate::audio::{Audio, RenderContext};
use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{Raster, cell_of};
use figures::{Figure, Tones};
use paint::Paint;
use pf_sequence::{DancerBackground, DancerParams, DancerSpeed, Mark, TimingKind, TimingTrack};
use std::f32::consts::TAU;

/// A beat's length without marks to go by (two a second).
const FREE_BEAT_MS: f64 = 500.0;
/// How far the hips dip on every beat, and the head after them, in figure heights.
const DIP: f32 = 0.02;
const HEAD_DIP: f32 = 0.012;
/// How far behind the hips the head dips, in beats.
const HEAD_LAG: f32 = 0.15;
/// The deepest the bass pushes the hips down, in figure heights.
const BASS_DIP: f32 = 0.09;
/// The bass is the loudest of this many frames back, each counting for less.
const BASS_FRAMES: u64 = 6;
const BASS_FALL: f32 = 0.72;
/// How bright the glow behind a dancer is at its middle.
const GLOW: f32 = 0.22;

/// Where `t_ms` is in the beats `marks` start: the mark's number, plus how far it is to the
/// next. `None` with fewer than two marks.
fn beat_on(marks: &[Mark], t_ms: u64) -> Option<f64> {
    if marks.len() < 2 {
        return None;
    }
    let last = marks.len() - 1;
    // The beat `t_ms` is in, or the nearest one with a next beat to pace by.
    let i = marks
        .partition_point(|m| m.start_ms <= t_ms)
        .saturating_sub(1)
        .min(last - 1);
    let (from, to) = (marks[i].start_ms as f64, marks[i + 1].start_ms as f64);
    Some(i as f64 + (t_ms as f64 - from) / (to - from).max(1.0))
}

/// Which mark starts a bar: the first labeled "1", when marks are numbered within their bars.
fn downbeat(marks: &[Mark]) -> f64 {
    marks
        .iter()
        .take(8)
        .position(|m| m.label.trim() == "1")
        .map_or(0.0, |i| (i % 4) as f64)
}

/// The track a dancer follows: its own when the sequence has it, else the song's beats.
fn followed<'a>(p: &DancerParams, cx: &RenderContext<'a>) -> Option<&'a TimingTrack> {
    let usable = |track: &&TimingTrack| track.marks.len() >= 2;
    cx.track(p.timing_track).filter(usable).or_else(|| {
        cx.tracks
            .iter()
            .filter(|track| track.kind == TimingKind::Beats)
            .find(usable)
    })
}

/// How hard the bass is hitting now, 0–1: the loudest of the last few frames, the older ones
/// counting for less, so the dancer drops with a kick and rises after it.
fn bass(audio: Audio, t_ms: u64) -> f32 {
    let now = audio.frame_at(t_ms);
    (0..BASS_FRAMES)
        .filter_map(|back| {
            let frame = now.checked_sub(back)?;
            Some(unit(audio.bass(frame)) * BASS_FALL.powi(back as i32))
        })
        .fold(0.0, f32::max)
}

pub struct Dancer {
    raster: Raster,
    mirror: bool,
}

impl Dancer {
    pub fn new(
        p: &DancerParams,
        time: &EffectTime,
        colors: Colors,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        let mut raster = Raster::new(canvas);
        let (width, height) = (raster.width, raster.height);
        let t_ms = time.start_ms + time.elapsed_ms;
        let beat = match followed(p, cx) {
            Some(track) => beat_on(&track.marks, t_ms).unwrap_or(0.0) - downbeat(&track.marks),
            None => time.elapsed_ms as f64 / FREE_BEAT_MS,
        };
        let pace = match p.speed {
            DancerSpeed::Half => 0.5,
            DancerSpeed::Normal => 1.0,
            DancerSpeed::Double => 2.0,
        };
        let thump = match cx.audio {
            Some(audio) if p.bass_bounce > 0.0 => p.bass_bounce * bass(audio, t_ms),
            _ => 0.0,
        };
        // Each dancer's share of the columns, and how tall that lets it stand.
        let count = (p.count.max(1) as i32).min(width).max(1);
        let share = width as f32 / count as f32;
        let size =
            figures::fitted((p.size / 100.0 * height as f32).min(share * figures::tallest(p.character)));
        let ground = (p.y / 100.0 * height as f32).round();
        let tones = Tones(p.use_palette.then_some(colors));
        let glow = match p.use_palette {
            true => colors.get(colors.len() as u64 - 1),
            false => figures::glow(p.character),
        };
        for i in 0..count {
            let from = (i as f32 * share).round() as i32;
            let to = ((i + 1) as f32 * share).round() as i32;
            let across = from as f32 + (to - from) as f32 * p.x / 100.0;
            let center = (across.floor() + 0.5).clamp(from as f32 + 0.5, (to as f32 - 0.5).max(0.5));
            let own = (beat - f64::from(p.stagger) * f64::from(i)) * pace;
            let mut pose = moves::pose_at(p.moves, p.routine, own);
            let phase = own.rem_euclid(1.0) as f32;
            let dip = |phase: f32| 0.5 + 0.5 * (TAU * phase).cos();
            pose.root[1] -= DIP * dip(phase) + BASS_DIP * thump;
            pose.head[1] -= HEAD_DIP * dip(phase - HEAD_LAG);
            let mut fig = Figure {
                paint: Paint {
                    raster: &mut raster,
                    center,
                    ground,
                    columns: (from, to),
                    sharp: figures::edges(size),
                },
                size,
                reach: ((to - from - 1) / 2) as f32,
                headroom: height as f32 - ground - size,
                pose,
                beat: own,
                tones,
            };
            figures::draw(p.character, &mut fig);
            if p.background == DancerBackground::Glow {
                fig.paint.glow_behind(
                    [0.0, 0.5 * size],
                    [0.6 * share.max(0.4 * size), 0.65 * size],
                    glow,
                    GLOW,
                );
            }
        }
        Self {
            raster,
            mirror: p.mirror,
        }
    }
}

impl Shade for Dancer {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.raster.width, self.raster.height);
        let x = if self.mirror { self.raster.width - 1 - x } else { x };
        self.raster.get(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(starts: &[u64]) -> Vec<Mark> {
        starts.iter().map(|&s| Mark::new(s, s + 10, "")).collect()
    }

    #[test]
    fn the_beat_counts_marks_and_carries_on_past_them() {
        let m = marks(&[1000, 1500, 2500, 3000]);
        let at = |t| beat_on(&m, t).unwrap();
        assert_eq!(at(1000), 0.0);
        assert_eq!(at(1250), 0.5);
        assert_eq!(at(1500), 1.0);
        assert_eq!(at(2000), 1.5, "a longer beat is still one beat");
        assert_eq!(at(3000), 3.0);
        // Before the first mark and after the last, at their neighbors' pace.
        assert_eq!(at(500), -1.0);
        assert_eq!(at(3250), 3.5);
        assert_eq!(at(4000), 5.0);
        assert_eq!(beat_on(&m[..1], 1000), None);
        assert_eq!(beat_on(&[], 0), None);
        // Marks at the same moment don't divide by nothing.
        assert!(beat_on(&marks(&[100, 100, 100]), 100).unwrap().is_finite());
    }

    #[test]
    fn bars_start_on_the_mark_labeled_one() {
        let numbered: Vec<Mark> = ["3", "4", "1", "2", "3", "4", "1"]
            .iter()
            .enumerate()
            .map(|(i, label)| Mark::new(i as u64 * 500, i as u64 * 500 + 10, *label))
            .collect();
        assert_eq!(downbeat(&numbered), 2.0);
        assert_eq!(downbeat(&marks(&[0, 500, 1000])), 0.0);
    }
}
