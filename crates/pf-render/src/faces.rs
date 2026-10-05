//! The Faces effect: a prop's singing face (see `pf_model::FaceDefinition`) lit for the sound
//! under the playhead, following xLights' `FacesEffect` for node-range faces.
//!
//! - **Mouth:** on a Phonemes track, the mark's phoneme (`AI`, `E`, `etc`, `FV`, `L`, `MBP`, `O`,
//!   `rest`, `U`, `WQ`). On a Words or Lyrics track PixelFlow guesses the mouth shapes from the
//!   word's letters ([`word_phonemes`]): an approximation, not real speech analysis. Between
//!   marks, with no track, and on a track without lyrics (beats, bars, sections, custom), the
//!   mouth is at rest, as in xLights.
//! - **Eyes:** open, closed, or open and blinking: a 150 ms blink every 3–5 seconds, at times
//!   picked from the effect's id, so every render of a frame is the same.
//! - **Colors:** the face's own colors (white where it has none), or the palette as xLights uses
//!   it: first color the mouth, second the eyes, third the outline (a short palette repeats its
//!   last color).
//!
//! Drawn in xLights' order, later parts winning where they share pixels: outline, mouth, eyes.
//!
//! **On a group row** every member prop with the face sings, each in its own pixels. That's a
//! deliberate difference: xLights draws nothing for a node-range face on a group
//! (`FacesEffect::RenderFaces` returns early for groups unless the face is a picture).

use crate::color::Rgba;
use crate::effects::hash;
use crate::geometry::{PixelBuffer, SceneGeometry};
use pf_model::{FaceDefinition, NodeRange, Phoneme, RegionKind, Rgb};
use pf_sequence::{Effect, FaceColorSource, FaceEyes, FacesParams, Sequence, Target, TimingKind};

/// How long a blink lasts.
pub const BLINK_MS: u64 = 150;
/// Shortest and longest time from one blink to the next.
const BLINK_EVERY_MS: (u64, u64) = (3_000, 5_000);

/// The mouth shapes a word makes, guessed from its letters (an approximation): vowels open the
/// mouth (`a`/`i` AI, `e` E, `o` O, `u` and `oo` U, a final `y` E), `m`/`b`/`p` close it (MBP),
/// `f`/`v` show FV, `l` L, `w`/`q` WQ, and other consonants etc. Repeats in a row merge.
pub fn word_phonemes(word: &str) -> Vec<Phoneme> {
    let letters: Vec<char> = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    let mut out: Vec<Phoneme> = Vec::new();
    let mut i = 0;
    while i < letters.len() {
        let c = letters[i];
        let next = letters.get(i + 1).copied();
        let phoneme = match c {
            'o' if next == Some('o') => {
                i += 1;
                Phoneme::U
            }
            'a' | 'i' => Phoneme::Ai,
            'e' => Phoneme::E,
            'o' => Phoneme::O,
            'u' => Phoneme::U,
            'y' if i + 1 == letters.len() && i > 0 => Phoneme::E,
            'm' | 'b' | 'p' => Phoneme::Mbp,
            'f' | 'v' => Phoneme::Fv,
            'l' => Phoneme::L,
            'w' | 'q' => Phoneme::Wq,
            _ => Phoneme::Etc,
        };
        if out.last() != Some(&phoneme) {
            out.push(phoneme);
        }
        i += 1;
    }
    out
}

/// The mouth shape at `t_ms` on the timing track `track` (rest without one).
pub fn phoneme_at(seq: &Sequence, track: Option<pf_sequence::TimingTrackId>, t_ms: u64) -> Phoneme {
    let Some(track) = track.and_then(|id| seq.timing_track(id)) else {
        return Phoneme::Rest;
    };
    let at = track.marks.partition_point(|m| m.start_ms <= t_ms);
    let Some(mark) = at.checked_sub(1).map(|i| &track.marks[i]) else {
        return Phoneme::Rest;
    };
    if t_ms >= mark.end_ms {
        return Phoneme::Rest;
    }
    match track.kind {
        TimingKind::Phonemes => return Phoneme::from_name(&mark.label).unwrap_or(Phoneme::Rest),
        TimingKind::Words | TimingKind::Lyrics => {}
        // xLights rests on any track that isn't a lyric track.
        TimingKind::Beats | TimingKind::Bars | TimingKind::Sections | TimingKind::Custom => {
            return Phoneme::Rest;
        }
    }
    // Words and lyrics: the letters' shapes spread evenly over the mark.
    let shapes = word_phonemes(&mark.label);
    if shapes.is_empty() {
        return Phoneme::Rest;
    }
    let length = (mark.end_ms - mark.start_ms).max(1);
    let k = ((t_ms - mark.start_ms) as u128 * shapes.len() as u128 / length as u128) as usize;
    shapes[k.min(shapes.len() - 1)]
}

/// True when blinking eyes are shut at `t_ms` in an effect starting at `start_ms`: the first
/// blink comes 1–3 seconds in, then one every 3–5 seconds, each [`BLINK_MS`] long, at times
/// picked from `seed`.
pub fn blinking(seed: u64, start_ms: u64, t_ms: u64) -> bool {
    let (shortest, longest) = BLINK_EVERY_MS;
    let mut at = start_ms + 1_000 + hash(seed, 0, 0) % 2_001;
    let mut k = 1;
    while at + BLINK_MS <= t_ms {
        at += shortest + hash(seed, k, 1) % (longest - shortest + 1);
        k += 1;
    }
    t_ms >= at
}

fn color(rgb: Rgb) -> [f32; 3] {
    [
        f32::from(rgb.r) / 255.0,
        f32::from(rgb.g) / 255.0,
        f32::from(rgb.b) / 255.0,
    ]
}

/// The face named `name` on a prop's regions (blank: the first face; names match ignoring case
/// and surrounding spaces).
pub(crate) fn find_face<'a>(regions: &'a [pf_model::Region], name: &str) -> Option<&'a FaceDefinition> {
    let name = name.trim();
    regions.iter().find_map(|r| match &r.kind {
        RegionKind::Face(face) if name.is_empty() || r.name.trim().eq_ignore_ascii_case(name) => Some(face),
        _ => None,
    })
}

/// One prop a Faces effect can draw on, for one target: the prop (its place in
/// [`SceneGeometry`]'s props) and where each of its nodes is in the target's buffer
/// (`u32::MAX`: not in it). Worked out once per target, not every frame.
#[derive(Debug, Clone)]
pub(crate) struct FaceProp {
    prop: usize,
    at: Vec<u32>,
}

/// The props of `target` that have a face, each with where its nodes sit in `buffer`.
pub(crate) fn face_props(geometry: &SceneGeometry, target: Target, buffer: &PixelBuffer) -> Vec<FaceProp> {
    let mut out: Vec<FaceProp> = geometry
        .target_props(target)
        .into_iter()
        .filter(|&i| {
            geometry.props[i]
                .regions
                .iter()
                .any(|r| matches!(r.kind, RegionKind::Face(_)))
        })
        .map(|i| FaceProp {
            prop: i,
            at: vec![u32::MAX; geometry.props[i].points.len()],
        })
        .collect();
    if out.is_empty() {
        return out;
    }
    // One pass over the buffer: each pixel's prop (by show-wide pixel) and node.
    let mut starts: Vec<(usize, usize)> = out
        .iter()
        .enumerate()
        .map(|(k, f)| (geometry.props[f.prop].first_pixel, k))
        .collect();
    starts.sort_unstable();
    for (i, &g) in buffer.show_pixels().iter().enumerate() {
        let g = g as usize;
        let k = starts.partition_point(|&(first, _)| first <= g);
        let Some(&(first, slot)) = k.checked_sub(1).map(|k| &starts[k]) else {
            continue;
        };
        if let Some(entry) = out[slot].at.get_mut(g - first) {
            *entry = i as u32;
        }
    }
    out
}

/// What the face shows at `t_ms`: fills `lit` with a color for each of the buffer's `len`
/// pixels that's lit (`None` for the rest). `lit` is reused from frame to frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lit_pixels(
    params: &FacesParams,
    effect: &Effect,
    t_ms: u64,
    seq: &Sequence,
    geometry: &SceneGeometry,
    faces: &[FaceProp],
    len: usize,
    lit: &mut Vec<Option<Rgba>>,
) {
    lit.clear();
    lit.resize(len, None);
    let phoneme = phoneme_at(seq, params.timing_track, t_ms);
    let closed = match params.eyes {
        FaceEyes::Open => false,
        FaceEyes::Closed => true,
        FaceEyes::Auto => blinking(effect.id.seed(), effect.start_ms, t_ms),
    };
    let palette = |i: usize| -> [f32; 3] {
        let colors = &effect.palette.colors;
        colors
            .get(i.min(colors.len().saturating_sub(1)))
            .map_or([1.0; 3], |c| color(*c))
    };
    for entry in faces {
        let Some(face) = find_face(&geometry.props[entry.prop].regions, &params.face) else {
            continue;
        };
        let own = match (&face.colors, params.colors) {
            (Some(colors), FaceColorSource::Face) => Some(colors),
            _ => None,
        };
        let pick = |own_color: Option<Rgb>, palette_index: usize| match own {
            Some(_) => own_color.map_or([1.0; 3], color),
            None => palette(palette_index),
        };
        let outline = params
            .outline
            .then(|| (face.outline.as_slice(), pick(own.and_then(|c| c.outline), 2)));
        let mouth = face.mouths.get(&phoneme).map(|mouth| {
            (
                mouth.as_slice(),
                pick(own.and_then(|c| c.mouths.get(&phoneme).copied()), 0),
            )
        });
        let eyes = if closed {
            (
                face.eyes_closed.as_slice(),
                pick(own.and_then(|c| c.eyes_closed), 1),
            )
        } else {
            (face.eyes_open.as_slice(), pick(own.and_then(|c| c.eyes_open), 1))
        };
        let parts: [Option<(&[NodeRange], [f32; 3])>; 3] = [outline, mouth, Some(eyes)];
        for (ranges, rgb) in parts.into_iter().flatten() {
            for range in ranges {
                for node in range.start..range.end {
                    if let Some(&i) = entry.at.get(node as usize)
                        && i != u32::MAX
                    {
                        lit[i as usize] = Some(Rgba::opaque(rgb));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{Mark, TimingTrack};

    #[test]
    fn words_become_rough_mouth_shapes() {
        use Phoneme::*;
        assert_eq!(word_phonemes("Moon"), vec![Mbp, U, Etc]);
        assert_eq!(word_phonemes("silent"), vec![Etc, Ai, L, E, Etc]);
        assert_eq!(word_phonemes("Happy!"), vec![Etc, Ai, Mbp, E]);
        assert_eq!(word_phonemes("we've"), vec![Wq, E, Fv, E]);
        assert_eq!(word_phonemes("y"), vec![Etc]);
        assert_eq!(word_phonemes("123 ..."), Vec::<Phoneme>::new());
    }

    #[test]
    fn phonemes_come_from_the_mark_under_the_playhead() {
        let mut seq = Sequence::new("s", 10_000);
        let phonemes = TimingTrack::new(
            "Lyrics (phonemes)",
            TimingKind::Phonemes,
            vec![
                Mark::new(100, 200, "AI"),
                Mark::new(200, 300, "etc"),
                Mark::new(400, 500, "huh"),
            ],
        );
        let words = TimingTrack::new(
            "Lyrics (words)",
            TimingKind::Words,
            vec![Mark::new(1000, 1400, "moon")],
        );
        let (p, w) = (Some(phonemes.id), Some(words.id));
        seq.timing_tracks = vec![phonemes, words];
        assert_eq!(phoneme_at(&seq, p, 50), Phoneme::Rest);
        assert_eq!(phoneme_at(&seq, p, 100), Phoneme::Ai);
        assert_eq!(phoneme_at(&seq, p, 299), Phoneme::Etc);
        assert_eq!(phoneme_at(&seq, p, 300), Phoneme::Rest, "between marks");
        assert_eq!(phoneme_at(&seq, p, 450), Phoneme::Rest, "an unknown label");
        assert_eq!(phoneme_at(&seq, None, 150), Phoneme::Rest);
        assert_eq!(
            phoneme_at(&seq, Some(pf_sequence::TimingTrackId::new()), 150),
            Phoneme::Rest
        );
        // A track without lyrics (beats, bars, sections, custom) keeps the mouth at rest, as
        // xLights does for any track that isn't a lyric track.
        for kind in [
            TimingKind::Beats,
            TimingKind::Bars,
            TimingKind::Sections,
            TimingKind::Custom,
        ] {
            let mut seq = seq.clone();
            let beats = TimingTrack::new("Beats", kind, vec![Mark::new(0, 1000, "Chorus")]);
            let b = Some(beats.id);
            seq.timing_tracks.push(beats);
            assert_eq!(phoneme_at(&seq, b, 500), Phoneme::Rest, "{kind:?}");
        }
        // "moon" is M, OO, N over 400 ms.
        assert_eq!(phoneme_at(&seq, w, 1000), Phoneme::Mbp);
        assert_eq!(phoneme_at(&seq, w, 1200), Phoneme::U);
        assert_eq!(phoneme_at(&seq, w, 1399), Phoneme::Etc);
    }

    #[test]
    fn blinks_are_short_spaced_and_the_same_every_time() {
        let seed = 0x00DE_C0DE;
        let closed: Vec<u64> = (0..60_000)
            .step_by(10)
            .filter(|&t| blinking(seed, 0, t))
            .collect();
        // Runs of closed frames (10 ms apart) are blinks.
        let mut blinks: Vec<(u64, u64)> = Vec::new();
        for t in closed {
            match blinks.last_mut() {
                Some((_, end)) if *end + 10 == t => *end = t,
                _ => blinks.push((t, t)),
            }
        }
        assert!(
            (12..=20).contains(&blinks.len()),
            "{} blinks in a minute",
            blinks.len()
        );
        assert!(
            blinks[0].0 >= 1_000 && blinks[0].0 <= 3_000,
            "first blink {:?}",
            blinks[0]
        );
        for pair in blinks.windows(2) {
            let gap = pair[1].0 - pair[0].0;
            assert!((2_990..=5_010).contains(&gap), "{gap}");
        }
        for (start, end) in &blinks {
            assert!(end - start < BLINK_MS, "{start}..{end}");
        }
        assert!(blinking(seed, 0, blinks[3].0));
        assert_ne!(
            (0..60_000)
                .step_by(10)
                .filter(|&t| blinking(seed + 1, 0, t))
                .collect::<Vec<_>>(),
            (0..60_000)
                .step_by(10)
                .filter(|&t| blinking(seed, 0, t))
                .collect::<Vec<_>>(),
            "another effect blinks at other times"
        );
        assert!(!blinking(seed, 5_000, 4_000), "before the effect starts");
    }
}
