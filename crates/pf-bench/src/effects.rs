//! Which effects the render time goes on.
//!
//! Two views of one show:
//!
//! - **each kind alone**: one effect of the kind on every prop, and one on the whole-house group,
//!   with nothing added: what a pixel of that effect costs to draw, and what it allocates;
//! - **each kind's share of a synthetic sequence**: the sequence drawn with only that kind's
//!   effects left in, and drawn again without its blur, its sparkles, or its curves, and with
//!   only its prop rows or only its group rows.
//!
//! Every time is the mean time to draw a frame, less the time an empty frame takes. It is the
//! work an effect takes that's wanted, so the benchmark draws these on one thread unless it's
//! told how many: on several, a time says how well the work was shared out as much as how much
//! of it there was.

use crate::alloc::counted;
use crate::pipeline::Workload;
use crate::sequence::{
    FRAME_MS, KINDS, Maker, On, Variant, empty_sequence, synthetic_sequence, target_pixels,
};
use crate::show::{Category, GroupKind, tier_label};
use crate::stats::ms;
use pf_sequence::{Effect, EffectKind, EffectParams, Layer, Row, Sequence, Target};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// How long each kind is drawn on its own, in seconds, after a few frames to settle.
pub const ALONE_SECONDS: u32 = 3;
const SETTLE_FRAMES: u64 = 4;

/// Something done to an effect before it is costed.
type Change<'a> = &'a dyn Fn(&mut Effect);

/// What the effects cost on one show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown {
    pub tier: String,
    pub pixels: u32,
    /// Drawing a frame with nothing on it: clearing and writing every pixel.
    pub empty_frame_ms: f64,
    pub kinds: Vec<KindCost>,
    pub extras: Vec<ExtraCost>,
    pub sequences: Vec<SequenceShare>,
}

/// One kind of effect drawn alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCost {
    pub kind: String,
    /// One on every prop: milliseconds a frame, and nanoseconds a pixel.
    pub on_props_ms: f64,
    pub on_props_ns: f64,
    /// What drawing those allocates each frame (none when allocations aren't counted).
    pub on_props_allocations: Option<f64>,
    pub on_props_kb: Option<f64>,
    /// One on the whole-house group.
    pub on_house_ms: f64,
    pub on_house_ns: f64,
}

/// What blur, sparkles, or a curve add to an effect on every prop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraCost {
    pub what: String,
    pub ms: f64,
    pub ns: f64,
}

/// Where a synthetic sequence's render time goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceShare {
    pub variant: Variant,
    pub seconds: u32,
    /// The whole sequence, per frame.
    pub ms: f64,
    pub allocations: Option<f64>,
    pub kb: Option<f64>,
    pub kinds: Vec<KindShare>,
    /// The sequence with only its prop rows, and with only its group rows.
    pub prop_rows_ms: f64,
    pub group_rows_ms: f64,
    /// What the sequence saves without any blur; without any sparkles (set, or by a curve); and
    /// without the curves on any other setting.
    pub blur_ms: f64,
    pub sparkles_ms: f64,
    pub curves_ms: f64,
}

/// One kind's part of a sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindShare {
    pub kind: String,
    pub effects: usize,
    /// How many times each of the show's pixels is drawn by this kind in a frame, on average.
    pub layers_per_pixel: f64,
    /// The sequence with only this kind's effects, per frame.
    pub ms: f64,
    /// Of all the kinds' times added up.
    pub percent: f64,
    /// Per pixel drawn.
    pub ns: f64,
}

/// What drawing a frame took, on average.
struct Drawn {
    ms: f64,
    allocations: Option<f64>,
    kb: Option<f64>,
}

/// Draws `seq`'s frames in order from the start (effects that carry on from frame to frame need
/// every frame before) and says what each from `from` up to `to` took, on average.
fn draw(workload: &Workload, seq: &Sequence, from: u64, to: u64) -> Drawn {
    let mut renderer = workload.renderer();
    let mut frame = vec![0u8; workload.map.frame_len];
    for index in 0..from {
        renderer.render_frame(seq, index, &mut frame);
    }
    let before = counted();
    let started = Instant::now();
    for index in from..to {
        renderer.render_frame(seq, index, &mut frame);
    }
    let took = started.elapsed();
    let allocated = before.zip(counted());
    std::hint::black_box(&frame);
    let frames = (to - from).max(1) as f64;
    Drawn {
        ms: ms(took) / frames,
        allocations: allocated.map(|(a, b)| (b.allocations - a.allocations) as f64 / frames),
        kb: allocated.map(|(a, b)| (b.bytes - a.bytes) as f64 / 1024.0 / frames),
    }
}

/// `seq` with only the effects `keep` picks (every row and layer stays).
fn only(seq: &Sequence, keep: impl Fn(&Row, &Effect) -> bool) -> Sequence {
    let mut out = seq.clone();
    for row in &mut out.rows {
        let kept: Vec<Layer> = row
            .layers
            .iter()
            .map(|layer| Layer {
                effects: layer.effects.iter().filter(|e| keep(row, e)).cloned().collect(),
            })
            .collect();
        row.layers = kept;
    }
    out
}

/// `seq` with every effect changed by `change`.
fn with_each(seq: &Sequence, change: impl Fn(&mut Effect)) -> Sequence {
    let mut out = seq.clone();
    for effect in out
        .rows
        .iter_mut()
        .flat_map(|r| &mut r.layers)
        .flat_map(|l| &mut l.effects)
    {
        change(effect);
    }
    out
}

/// What the effects cost on the workload's show: each kind alone for `alone_seconds`, and each
/// kind's share of both synthetic sequences over their first `seconds` seconds. `progress` hears
/// what is being drawn.
pub fn breakdown(
    workload: &Workload,
    seconds: u32,
    alone_seconds: u32,
    progress: &dyn Fn(&str),
) -> Breakdown {
    let show = &workload.show;
    let pixels = show.pixels();
    let per_second = u64::from(1000 / FRAME_MS);
    let alone = (u64::from(alone_seconds) * per_second).max(SETTLE_FRAMES + 1);
    let ns = |ms: f64, pixels: f64| ms * 1e6 / pixels.max(1.0);

    let (blank, mut maker) = empty_sequence("Alone".to_string(), Variant::Typical, alone_seconds.max(1));
    let span = (0, blank.duration_ms);
    let empty_frame_ms = draw(workload, &blank, SETTLE_FRAMES, alone).ms;
    let matrix_rows = |i: usize| match show.show.props[i].shape {
        pf_model::ShapeSource::Generator(pf_model::Generator::Matrix { rows, .. }) => rows,
        _ => 0,
    };
    // One effect of `kind` on every prop, changed by `change`.
    let on_props = |maker: &mut Maker, kind: EffectKind, change: Change| {
        let mut seq = blank.clone();
        for (i, prop) in show.props.iter().enumerate() {
            let on = On {
                rows: matrix_rows(i),
                surface: matches!(prop.category, Category::Tree | Category::Matrix),
            };
            let mut effect = maker.plain(kind, span, i as u64, on);
            change(&mut effect);
            let mut row = Row::new(Target::Prop(prop.id));
            row.layers[0].effects.push(effect);
            seq.rows.push(row);
        }
        seq
    };
    let house = show
        .groups
        .iter()
        .find(|g| g.kind == GroupKind::WholeHouse)
        .map(|g| g.id);
    let mut kinds = Vec::new();
    for kind in KINDS {
        progress(&format!("{} alone", kind.label()));
        let props = draw(
            workload,
            &on_props(&mut maker, kind, &|_| {}),
            SETTLE_FRAMES,
            alone,
        );
        let mut seq = blank.clone();
        if let Some(house) = house {
            let mut row = Row::new(Target::Group(house));
            let on = On {
                rows: 0,
                surface: true,
            };
            row.layers[0].effects.push(maker.plain(kind, span, 0, on));
            seq.rows.push(row);
        }
        let house_ms = draw(workload, &seq, SETTLE_FRAMES, alone).ms - empty_frame_ms;
        let props_ms = props.ms - empty_frame_ms;
        kinds.push(KindCost {
            kind: kind.label().to_string(),
            on_props_ms: props_ms,
            on_props_ns: ns(props_ms, f64::from(pixels)),
            on_props_allocations: props.allocations,
            on_props_kb: props.kb,
            on_house_ms: house_ms,
            on_house_ns: ns(house_ms, f64::from(pixels)),
        });
    }

    progress("blur, sparkles, curves, and fonts");
    let changes: [(&str, EffectKind, Change); 5] = [
        ("Plasma", EffectKind::Plasma, &|_| {}),
        ("Plasma with sparkles", EffectKind::Plasma, &|e| e.sparkles = 100),
        ("Plasma with blur 2", EffectKind::Plasma, &|e| e.blur = 2),
        ("Plasma with blur 8", EffectKind::Plasma, &|e| e.blur = 8),
        ("Plasma with a curve on its speed", EffectKind::Plasma, &|e| {
            Maker::speed_curve(e)
        }),
    ];
    // Text in a font on this computer, where it has one of these (the pixel font's own cost is
    // the Text kind's, above).
    let family = ["Arial", "Helvetica", "DejaVu Sans", "Liberation Sans"]
        .into_iter()
        .find_map(pf_fonts::installed);
    let in_font = |e: &mut Effect| {
        if let (Some(family), EffectParams::Text(p)) = (&family, &mut e.params) {
            p.font = family.clone();
        }
    };
    let font_label = family.as_ref().map(|f| format!("Text in a computer font ({f})"));
    let extras = changes
        .into_iter()
        .map(|(what, kind, change)| (what.to_string(), kind, change))
        .chain(font_label.map(|label| (label, EffectKind::Text, &in_font as Change)))
        .map(|(what, kind, change)| {
            let seq = on_props(&mut maker, kind, change);
            let ms = draw(workload, &seq, SETTLE_FRAMES, alone).ms - empty_frame_ms;
            ExtraCost {
                what,
                ms,
                ns: ns(ms, f64::from(pixels)),
            }
        })
        .collect();

    // The same sequences the runs play (as long as the workload's), drawn as far as `seconds`.
    let length = workload.sequence.summary.seconds;
    let seconds = seconds.min(length);
    let frames = u64::from(seconds) * per_second;
    let window_ms = u64::from(seconds) * 1000;
    let sequences = Variant::ALL
        .into_iter()
        .map(|variant| {
            let seq = synthetic_sequence(show, variant, length).sequence;
            let cost = |what: &str, seq: &Sequence| {
                progress(&format!("the {} sequence: {what}", variant.label()));
                draw(workload, seq, 0, frames).ms - empty_frame_ms
            };
            progress(&format!("the {} sequence: all of it", variant.label()));
            let all = draw(workload, &seq, 0, frames);
            let whole = all.ms - empty_frame_ms;
            let mut shares: Vec<KindShare> = KINDS
                .into_iter()
                .filter_map(|kind| {
                    let playing = |e: &&Effect| e.kind() == kind && e.start_ms < window_ms;
                    let effects = seq.effects().filter(playing).count();
                    if effects == 0 {
                        return None;
                    }
                    // Pixels drawn by the kind over the time drawn, per pixel of the show per frame.
                    let drawn: f64 = seq
                        .rows
                        .iter()
                        .map(|row| {
                            let time: u64 = row
                                .layers
                                .iter()
                                .flat_map(|l| &l.effects)
                                .filter(playing)
                                .map(|e| e.end_ms.min(window_ms) - e.start_ms)
                                .sum();
                            f64::from(target_pixels(show, row.target)) * time as f64
                        })
                        .sum();
                    let layers_per_pixel = drawn / (f64::from(pixels) * window_ms.max(1) as f64);
                    let ms = cost(kind.label(), &only(&seq, |_, e| e.kind() == kind));
                    Some(KindShare {
                        kind: kind.label().to_string(),
                        effects,
                        layers_per_pixel,
                        ms,
                        percent: 0.0,
                        ns: ns(ms, layers_per_pixel * f64::from(pixels)),
                    })
                })
                .collect();
            let sum: f64 = shares.iter().map(|s| s.ms).sum();
            for share in &mut shares {
                share.percent = 100.0 * share.ms / sum.max(f64::EPSILON);
            }
            shares.sort_by(|a, b| b.ms.total_cmp(&a.ms));
            let is_group = |row: &Row| matches!(row.target, Target::Group(_));
            let no_sparkles = with_each(&seq, |e| {
                e.sparkles = 0;
                e.curves.remove("sparkles");
            });
            let no_curves = with_each(&seq, |e| e.curves.retain(|key, _| key == "sparkles"));
            SequenceShare {
                variant,
                seconds,
                ms: whole,
                allocations: all.allocations,
                kb: all.kb,
                prop_rows_ms: cost("prop rows", &only(&seq, |row, _| !is_group(row))),
                group_rows_ms: cost("group rows", &only(&seq, |row, _| is_group(row))),
                blur_ms: whole - cost("without blur", &with_each(&seq, |e| e.blur = 0)),
                sparkles_ms: whole - cost("without sparkles", &no_sparkles),
                curves_ms: whole - cost("without curves", &no_curves),
                kinds: shares,
            }
        })
        .collect();

    Breakdown {
        tier: tier_label(pixels),
        pixels,
        empty_frame_ms,
        kinds,
        extras,
        sequences,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::EffectKind;

    #[test]
    fn a_sequence_can_be_cut_down_to_one_kind_or_one_sort_of_row() {
        let workload = Workload::new(1_500, Variant::WorstCase, 8)
            .unwrap()
            .without_media();
        let seq = &workload.sequence.sequence;
        let twinkles = only(seq, |_, e| e.kind() == EffectKind::Twinkle);
        assert!(twinkles.effect_count() > 0 && twinkles.effect_count() < seq.effect_count());
        assert!(twinkles.effects().all(|e| e.kind() == EffectKind::Twinkle));
        assert_eq!(twinkles.rows.len(), seq.rows.len());
        let groups = only(seq, |row, _| matches!(row.target, Target::Group(_)));
        let props = only(seq, |row, _| matches!(row.target, Target::Prop(_)));
        assert_eq!(groups.effect_count() + props.effect_count(), seq.effect_count());
        let plain = with_each(seq, |e| e.blur = 0);
        assert!(seq.effects().any(|e| e.blur > 0) && plain.effects().all(|e| e.blur == 0));
    }
}
