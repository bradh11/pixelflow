//! A show and a sequence with a little of everything the renderer does, for checking that it
//! draws the same frames however it goes about it (on one thread or many, see
//! `tests/render_threads.rs` and [`crate::digest`]).
//!
//! The two measured sequences ([`crate::sequence`]) use sixteen kinds of effect on whole props and
//! groups, as a sequence mostly does. This one is the opposite: nothing about it is likely, and
//! everything is in it. Every kind of effect, every blend, every render style and buffer
//! transform, on props, groups, a submodel, and a prop with a singing face; fades, sparkles, blur
//! (set, and changing over the effect), color adjustments, and curves; the effects that carry on
//! from frame to frame, one of them twice under the same id; and whichever way rows meet the
//! rows before them.

use crate::sequence::{FRAME_MS, Maker, Media, On, Variant, empty_sequence, synthetic_media};
use crate::show::{Category, SyntheticShow, fixed_id, synthetic_show};
use pf_mapping::ChannelMap;
use pf_model::{
    BufferTransform, FaceDefinition, NodeRange, NodeRun, Phoneme, PropId, Region, RegionId, RenderStyle, Show,
};
use pf_sequence::{
    Blend, Curve, CurveShape, Effect, EffectKind, EffectParams, FaceColorSource, FaceEyes, FacesParams,
    Layer, Mark, Palette, Rgb, Row, RowBlending, RowId, Sequence, Target, TimingKind, TimingTrack,
    TimingTrackId,
};

/// How long each effect of the varied sequence lasts, in milliseconds: long enough for the
/// effects that carry on from frame to frame to get somewhere, short enough that a few seconds
/// go through every kind.
const SLOT_MS: u64 = 600;
/// Layers on every row.
const LAYERS: usize = 3;
/// Props of each category that get a row.
const PROPS_PER_CATEGORY: usize = 2;
const FACE: &str = "Singer";

const TRANSFORMS: [BufferTransform; 8] = [
    BufferTransform::None,
    BufferTransform::RotateCw90,
    BufferTransform::RotateCcw90,
    BufferTransform::Rotate180,
    BufferTransform::FlipVertical,
    BufferTransform::FlipHorizontal,
    BufferTransform::RotateCw90FlipHorizontal,
    BufferTransform::RotateCcw90FlipHorizontal,
];

/// The varied show and sequence, with the music and the picture its effects draw from.
pub struct Varied {
    pub show: Show,
    pub map: ChannelMap,
    pub sequence: Sequence,
    pub media: Media,
}

impl Varied {
    /// A renderer for the show with the sequence's music and picture.
    pub fn renderer(&self) -> pf_render::Renderer {
        let mut renderer = pf_render::Renderer::new(&self.show, &self.map);
        renderer.set_audio(self.media.audio.clone());
        renderer.set_pictures(self.media.pictures.clone());
        renderer
    }
}

/// The synthetic show of `pixels` pixels with a submodel on its largest matrix and a face on its
/// first outline, and the two regions' targets.
fn show_with_regions(pixels: u32) -> (SyntheticShow, Vec<Target>) {
    let mut show = synthetic_show(pixels);
    let mut targets = Vec::new();
    let mut add = |show: &mut SyntheticShow, prop: PropId, mut region: Region, n: u64| {
        region.id = RegionId(fixed_id(8, n));
        targets.push(Target::Region {
            prop,
            region: region.id,
        });
        if let Some(prop) = show.show.props.iter_mut().find(|p| p.id == prop) {
            prop.regions.push(region);
        }
    };
    let first = |category: Category| show.props.iter().find(|p| p.category == category).cloned();
    let (matrix, outline) = (first(Category::Matrix), first(Category::Outline));
    if let Some(matrix) = matrix {
        // Three lines from the middle of the matrix, the last shorter than the others.
        let (quarter, line) = (matrix.pixels / 4, (matrix.pixels / 16).max(2));
        let lines = (0..3)
            .map(|n| {
                let from = quarter + n * line;
                vec![Some(NodeRun::new(from, from + line - 1 - n))]
            })
            .collect();
        add(&mut show, matrix.id, Region::nodes("Middle", lines), 0);
    }
    if let Some(outline) = outline {
        let eighth = (outline.pixels / 8).max(1);
        let part = |n: u32| vec![NodeRange::new(n * eighth, (n + 1) * eighth)];
        let mut face = FaceDefinition::default();
        for (n, phoneme) in [Phoneme::Ai, Phoneme::E, Phoneme::O, Phoneme::Rest]
            .into_iter()
            .enumerate()
        {
            face.mouths.insert(phoneme, part(n as u32));
        }
        face.eyes_open = part(4);
        face.eyes_closed = part(5);
        face.outline = part(6);
        add(&mut show, outline.id, Region::face(FACE, face), 1);
    }
    (show, targets)
}

/// The varied show of `pixels` pixels and its sequence, `seconds` long, its rows meeting the rows
/// before them as `blending` says. The same every time.
pub fn varied(pixels: u32, seconds: u32, blending: RowBlending) -> Varied {
    let (show, regions) = show_with_regions(pixels);
    let (map, _) = pf_mapping::map_show(&show.show);
    let (mut sequence, mut maker) =
        empty_sequence(format!("{} (varied)", show.show.name), Variant::Typical, seconds);
    sequence.row_blending = blending;
    let duration_ms = sequence.duration_ms;
    // Something to sing: a sound every quarter of a second.
    let sounds = ["AI", "E", "O", "rest", "MBP", "etc"];
    let marks = (0..duration_ms.div_ceil(250))
        .map(|n| {
            Mark::new(
                n * 250,
                ((n + 1) * 250).min(duration_ms),
                sounds[n as usize % sounds.len()],
            )
        })
        .collect();
    let mut phonemes = TimingTrack::new("Phonemes", TimingKind::Phonemes, marks);
    phonemes.id = TimingTrackId(fixed_id(6, 2));
    let singing = phonemes.id;
    sequence.timing_tracks.push(phonemes);

    // A few props of each kind, then the regions (after their props' rows, so a row that
    // replaces has something of its prop's to mix with), then every group.
    let mut targets: Vec<(Target, On)> = Vec::new();
    for category in Category::ALL {
        let props = show
            .props
            .iter()
            .enumerate()
            .filter(|(_, p)| p.category == category);
        for (i, prop) in props.take(PROPS_PER_CATEGORY) {
            let rows = match show.show.props[i].shape {
                pf_model::ShapeSource::Generator(pf_model::Generator::Matrix { rows, .. }) => rows,
                _ => 0,
            };
            let surface = matches!(category, Category::Tree | Category::Matrix);
            targets.push((Target::Prop(prop.id), On { rows, surface }));
        }
    }
    let surface = On {
        rows: 0,
        surface: true,
    };
    targets.extend(regions.into_iter().map(|target| (target, surface)));
    targets.extend(show.groups.iter().map(|g| (Target::Group(g.id), surface)));

    let slots = duration_ms.div_ceil(SLOT_MS);
    for (t, &(target, on)) in targets.iter().enumerate() {
        let mut row = Row::new(target);
        row.id = RowId(fixed_id(7, t as u64));
        row.layers = (0..LAYERS)
            .map(|layer| Layer {
                effects: (0..slots)
                    .map(|slot| {
                        // A different kind in every layer of every row, and a new one each slot.
                        let n = t * LAYERS + layer + slot as usize * 5;
                        let kind = EffectKind::ALL[(n * 7) % EffectKind::ALL.len()];
                        let span = (slot * SLOT_MS, ((slot + 1) * SLOT_MS).min(duration_ms));
                        let mut effect = maker.plain(kind, span, (n as u64) << 4 | slot, on);
                        dress(&mut effect, n, singing);
                        effect
                    })
                    .collect(),
            })
            .collect();
        sequence.rows.push(row);
    }
    // One effect that carries on from frame to frame twice under one id, on one row: the two
    // share what's kept of them from the frame before.
    if let Some(row) = sequence.rows.first_mut() {
        let mut twin = maker.plain(EffectKind::Life, (0, duration_ms), 0, surface);
        twin.blend = Blend::Add;
        row.layers.push(Layer {
            effects: vec![twin.clone()],
        });
        row.layers.push(Layer { effects: vec![twin] });
    }
    Varied {
        show: show.show,
        map,
        sequence,
        media: synthetic_media(seconds),
    }
}

/// Gives effect number `n` its blend, render style, and transform, and now and then a blur,
/// sparkles, fades, color adjustments, or a curve.
fn dress(effect: &mut Effect, n: usize, singing: TimingTrackId) {
    effect.blend = Blend::ALL[n % Blend::ALL.len()];
    effect.render_style = RenderStyle::ALL[(n / 2) % RenderStyle::ALL.len()];
    effect.buffer_transform = TRANSFORMS[(n / 3) % TRANSFORMS.len()];
    if let EffectParams::Faces(_) = effect.params {
        effect.params = EffectParams::Faces(FacesParams {
            face: FACE.to_string(),
            timing_track: Some(singing),
            eyes: FaceEyes::Auto,
            colors: FaceColorSource::Palette,
            outline: true,
        });
        effect.palette = Palette::new([Rgb::RED, Rgb::GREEN, Rgb::BLUE]);
    }
    match n % 9 {
        0 | 3 => effect.blur = 1 + (n % 6) as u32,
        6 => {
            let blur = Curve::shaped(CurveShape::Sine, 0.0, 6.0, 1.0);
            effect.curves.insert("blur".to_string(), blur);
        }
        _ => {}
    }
    match n % 8 {
        1 => effect.sparkles = 60 + (n % 100) as u32,
        5 => {
            effect.sparkles = 150;
            effect.music_sparkles = true;
        }
        _ => {}
    }
    if n % 5 == 2 {
        effect.fade_in_ms = 8 * FRAME_MS;
        effect.fade_out_ms = 12 * FRAME_MS;
    }
    if n % 6 == 4 {
        effect.hue_adjust = 0.2;
        effect.saturation_adjust = -0.1;
        effect.value_adjust = 0.1;
        effect.brightness = 0.8;
        effect.contrast = 0.2;
    }
    if n % 7 == 6 {
        Maker::speed_curve(effect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_varied_sequence_has_one_of_everything() {
        let varied = varied(3_000, 8, RowBlending::Replace);
        let seq = &varied.sequence;
        assert_eq!(pf_sequence::limit_problems(seq), Vec::<String>::new());
        assert!(
            pf_model::validate_show(&varied.show)
                .issues
                .iter()
                .all(|i| i.severity != pf_model::Severity::Error)
        );
        let effects = || seq.effects();
        let kinds: HashSet<EffectKind> = effects().map(Effect::kind).collect();
        assert_eq!(kinds.len(), EffectKind::ALL.len());
        assert!(
            Blend::ALL
                .iter()
                .all(|blend| effects().any(|e| e.blend == *blend))
        );
        let styles: HashSet<RenderStyle> = effects().map(|e| e.render_style).collect();
        assert_eq!(styles.len(), RenderStyle::ALL.len());
        let transforms: HashSet<BufferTransform> = effects().map(|e| e.buffer_transform).collect();
        assert_eq!(transforms.len(), TRANSFORMS.len());
        assert!(effects().any(|e| e.blur > 0) && effects().any(|e| e.curves.contains_key("blur")));
        assert!(effects().any(|e| e.sparkles > 0) && effects().any(|e| e.music_sparkles));
        assert!(effects().any(|e| e.fade_in_ms > 0) && effects().any(|e| e.brightness != 1.0));
        let regions = seq
            .rows
            .iter()
            .filter(|r| matches!(r.target, Target::Region { .. }));
        assert_eq!(regions.count(), 2);
        assert!(seq.rows.iter().any(|r| matches!(r.target, Target::Group(_))));
        // The twins.
        let ids: Vec<_> = seq.rows[0]
            .layers
            .iter()
            .flat_map(|l| &l.effects)
            .map(|e| e.id)
            .collect();
        assert!(ids.iter().collect::<HashSet<_>>().len() < ids.len());
        // Something is lit, and it's the same sequence every time.
        let mut frame = vec![0u8; varied.map.frame_len];
        varied.renderer().render_frame(seq, 60, &mut frame);
        assert!(frame.iter().any(|&b| b > 0));
        assert_eq!(super::varied(3_000, 8, RowBlending::Replace).sequence, *seq);
    }
}
