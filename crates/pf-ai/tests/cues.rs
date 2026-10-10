//! Director-level cues: `stage_cue` expands hits, blackouts, ramps, sweeps, chases, and the rest
//! into effects on the draft, by the props' roles and places in the layout and the song's tempo,
//! on layers above the section looks, never overlapping and never past the sequence.

use pf_ai::provider::Message;
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{Cancel, ChatSession, Draft, UiContext, Workspace, apply_proposal};
use pf_analysis::{Analysis, BarEnergy, Confidence, Moment, MomentKind};
use pf_engine::{Edit, Engine};
use pf_model::{
    FaceDefinition, Generator, Group, GroupMember, NodeRange, Phoneme, Prop, Region, ShapeSource, TreeStyle,
    Vec3,
};
use pf_sequence::{Effect, EffectKind, EffectParams, Mark, Row, Sequence, Target, TimingKind, TimingTrack};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

const SONG_MS: u64 = 32_000;
/// 120 BPM.
const BEAT: u64 = 500;

fn at(mut prop: Prop, x: f32, y: f32) -> Prop {
    prop.transform.position = Vec3::new(x, y, 0.0);
    prop
}

fn line(name: &str, x: f32, y: f32) -> Prop {
    at(
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line {
                nodes: 20,
                length: 4.0,
            }),
        ),
        x,
        y,
    )
}

/// A show laid out left to right: a roof line and an arch on each side, a tree in the middle with
/// a matrix above it, and a singing bulb (a face) right of the tree. Groups: All, and Arches.
fn props() -> Vec<Prop> {
    let arch = |name: &str, x: f32| {
        at(
            Prop::new(name, ShapeSource::Generator(Generator::arch(20, 2.0, 1.0))),
            x,
            0.0,
        )
    };
    let mut bulb = at(
        Prop::new("Singing Bulb", ShapeSource::Generator(Generator::circle(12, 0.5))),
        3.0,
        2.0,
    );
    let mut face = FaceDefinition::default();
    face.mouths.insert(Phoneme::Ai, vec![NodeRange::new(1, 4)]);
    bulb.regions.push(Region::face("Face", face));
    vec![
        line("Left Roof", -10.0, 5.0),
        arch("Left Arch", -6.0),
        at(
            Prop::new(
                "Mega Tree",
                ShapeSource::Generator(Generator::tree(8, 20, 4.0, 1.0, 0.0, TreeStyle::Round)),
            ),
            0.0,
            0.0,
        ),
        at(
            Prop::new(
                "Matrix",
                ShapeSource::Generator(Generator::Matrix {
                    columns: 16,
                    rows: 8,
                    width: 4.0,
                    height: 2.0,
                    wiring: Default::default(),
                }),
            ),
            0.0,
            8.0,
        ),
        bulb,
        arch("Right Arch", 6.0),
        line("Right Roof", 10.0, 5.0),
    ]
}

fn moment(
    time_ms: u64,
    end_ms: Option<u64>,
    kind: MomentKind,
    importance: f32,
    label: Option<&str>,
) -> Moment {
    Moment {
        time_ms,
        end_ms,
        kind,
        strength: importance,
        importance,
        label: label.map(str::to_string),
        suggest: kind.suggest(),
    }
}

/// A 32 s song at 120 BPM with a build into a drop, a stop, a fill, a shout, a hold, and a
/// breakdown.
fn analysis() -> Analysis {
    let beats: Vec<u64> = (0..64).map(|i| i * BEAT).collect();
    Analysis {
        duration_ms: SONG_MS,
        tempo_bpm: Some(120.0),
        bars: beats.iter().copied().step_by(4).collect(),
        onsets: beats.clone(),
        beats,
        bar_energy: (0..16)
            .map(|_| BarEnergy {
                overall: 0.5,
                low: 0.5,
                mid: 0.5,
                high: 0.5,
            })
            .collect(),
        moments: vec![
            moment(4_000, Some(8_000), MomentKind::Build, 0.6, None),
            moment(8_000, None, MomentKind::Drop, 0.95, None),
            moment(10_000, Some(10_500), MomentKind::Fill, 0.55, None),
            moment(12_000, Some(13_000), MomentKind::Stop, 0.8, Some("full")),
            moment(13_000, None, MomentKind::Restart, 0.7, None),
            moment(16_250, None, MomentKind::Impact, 0.7, None),
            moment(18_000, None, MomentKind::Shout, 0.9, Some("Lantern")),
            moment(20_000, Some(22_000), MomentKind::Hold, 0.4, Some("end")),
            moment(24_000, Some(28_000), MomentKind::Breakdown, 0.6, None),
        ],
        confidence: Confidence {
            tempo: 0.9,
            downbeat: 0.7,
            sections: 0.0,
        },
        ..Analysis::default()
    }
}

/// Made-up sung words: "Lantern" at 18 s and 26 s.
fn lyrics() -> Vec<TimingTrack> {
    let words = vec![
        Mark::new(17_000, 17_500, "Glow"),
        Mark::new(18_000, 18_600, "Lantern"),
        Mark::new(18_600, 19_000, "bright"),
        Mark::new(26_000, 26_700, "Lantern"),
    ];
    vec![
        TimingTrack::new(
            "Lyrics",
            TimingKind::Lyrics,
            vec![Mark::new(17_000, 19_000, "Glow Lantern bright")],
        ),
        TimingTrack::new("Lyrics (words)", TimingKind::Words, words),
        TimingTrack::new(
            "Vocals",
            TimingKind::Custom,
            vec![
                Mark::new(17_000, 19_000, "Vocals"),
                Mark::new(26_000, 26_700, "Vocals"),
            ],
        ),
    ]
}

struct Setup {
    engine: Engine,
    _dir: tempfile::TempDir,
}

/// The show with a sequence on it: a row for each group, then for each prop (as a new sequence
/// starts), and a wash on All's bottom layer for the whole song (the section look).
fn setup(with_lyrics: bool) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    let props = props();
    let mut all = Group::new("All");
    all.members = props.iter().map(|p| GroupMember::Prop(p.id)).collect();
    let mut arches = Group::new("Arches");
    arches.members = props
        .iter()
        .filter(|p| p.name.ends_with("Arch"))
        .map(|p| GroupMember::Prop(p.id))
        .collect();
    let mut edits: Vec<Edit> = props.iter().map(|p| Edit::AddProp { prop: p.clone() }).collect();
    edits.push(Edit::AddGroup { group: all.clone() });
    edits.push(Edit::AddGroup {
        group: arches.clone(),
    });
    engine.apply(edits).unwrap();
    let mut rows = vec![
        Row::new(Target::Group(all.id)),
        Row::new(Target::Group(arches.id)),
    ];
    rows[0].layers[0]
        .effects
        .push(Effect::new(EffectKind::ColorWash, 0, SONG_MS));
    rows.extend(props.iter().map(|p| Row::new(Target::Prop(p.id))));
    engine
        .new_sequence_doc_with_rows("Lanterns", SONG_MS, Some("/music/lanterns.mp3"), rows)
        .unwrap();
    if with_lyrics {
        let mut doc = engine.sequence_document().unwrap().clone();
        doc.timing_tracks.extend(lyrics());
        engine.adopt_sequence_doc(doc).unwrap();
    }
    Setup { engine, _dir: dir }
}

fn draft(s: &Setup) -> Draft {
    Draft::new(Workspace::from_engine(&s.engine, UiContext::default()))
}

fn stage(draft: &mut Draft, input: Value) -> Result<String, String> {
    pf_ai::cues::stage(draft, Some(&analysis()), &input)
}

/// One effect the draft added: the row's name, its layer, and the effect.
#[derive(Debug, Clone)]
struct Added {
    row: String,
    layer: usize,
    effect: Effect,
}

fn row_name(draft: &Draft, row: &Row) -> String {
    let show = draft.show();
    match row.target {
        Target::Prop(id) => show.props.iter().find(|p| p.id == id).unwrap().name.clone(),
        Target::Group(id) => show.groups.iter().find(|g| g.id == id).unwrap().name.clone(),
        Target::Region { .. } => "region".into(),
    }
}

fn staged(draft: &Draft) -> Vec<Added> {
    let before = draft.base().sequence.as_ref().unwrap();
    let old: Vec<_> = before
        .doc
        .rows
        .iter()
        .flat_map(|r| r.layers.iter().flat_map(|l| l.effects.iter().map(|e| e.id)))
        .collect();
    let doc = draft.sequence().unwrap();
    let mut out = Vec::new();
    for row in &doc.rows {
        for (layer, l) in row.layers.iter().enumerate() {
            for effect in l.effects.iter().filter(|e| !old.contains(&e.id)) {
                out.push(Added {
                    row: row_name(draft, row),
                    layer,
                    effect: effect.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.effect
            .start_ms
            .cmp(&b.effect.start_ms)
            .then(a.row.cmp(&b.row))
            .then(a.layer.cmp(&b.layer))
    });
    out
}

fn on<'a>(added: &'a [Added], row: &str) -> Vec<&'a Added> {
    added.iter().filter(|a| a.row == row).collect()
}

fn of_kind(added: &[Added], kind: EffectKind) -> Vec<&Added> {
    added.iter().filter(|a| a.effect.kind() == kind).collect()
}

/// Nothing overlaps on any layer, and everything is inside the sequence.
fn assert_well_formed(doc: &Sequence) {
    for row in &doc.rows {
        for layer in &row.layers {
            for (i, a) in layer.effects.iter().enumerate() {
                assert!(a.start_ms < a.end_ms && a.end_ms <= doc.duration_ms, "{a:?}");
                for b in &layer.effects[i + 1..] {
                    assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }
}

fn params(effect: &Effect) -> Value {
    serde_json::to_value(&effect.params).unwrap()
}

const PROPS: [&str; 7] = [
    "Left Roof",
    "Left Arch",
    "Mega Tree",
    "Matrix",
    "Singing Bulb",
    "Right Arch",
    "Right Roof",
];

#[test]
fn a_hit_dips_then_punches_every_prop_above_the_section_look() {
    let s = setup(false);
    let mut d = draft(&s);
    let said = stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 16_000, "intensity": 0.5 }] }),
    )
    .unwrap();
    assert!(said.starts_with("Staged 1 cue: 1 hit"), "{said}");
    let added = staged(&d);
    // The prop rows (after the groups) cover the groups' look, so the cue goes on them.
    assert!(on(&added, "All").is_empty() && on(&added, "Arches").is_empty());
    for prop in PROPS {
        let here = on(&added, prop);
        assert_eq!(here.len(), 2, "{prop}: {here:?}");
        let (dip, hit) = (here[0], here[1]);
        assert_eq!(dip.effect.kind(), EffectKind::Off);
        assert_eq!(
            (dip.effect.start_ms, dip.effect.end_ms),
            (16_000 - BEAT / 2, 16_000)
        );
        assert_eq!(hit.effect.kind(), EffectKind::Impact);
        // 1–2 beats of decay by intensity: 1.5 beats at 0.5.
        assert_eq!((hit.effect.start_ms, hit.effect.end_ms), (16_000, 16_750));
        assert_eq!(params(&hit.effect)["decay"], "punch");
        assert_eq!(params(&hit.effect)["color"], "white");
        // Never the bottom layer, where the looks are.
        assert_eq!((dip.layer, hit.layer), (1, 1));
    }
    // Not big enough for accents.
    assert!(of_kind(&added, EffectKind::Strobe).is_empty());
    // The look below is untouched.
    let doc = d.sequence().unwrap();
    assert_eq!(doc.rows[0].layers[0].effects.len(), 1);
    assert_well_formed(doc);
}

#[test]
fn the_biggest_hits_add_strobes_on_outlines_and_lightning_on_matrices() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 8_000, "intensity": 1.0, "colors": ["#ff0000"] }] }),
    )
    .unwrap();
    let added = staged(&d);
    let strobes: Vec<&str> = of_kind(&added, EffectKind::Strobe)
        .iter()
        .map(|a| a.row.as_str())
        .collect();
    assert_eq!(strobes, ["Left Arch", "Left Roof", "Right Arch", "Right Roof"]);
    let lightning = of_kind(&added, EffectKind::Lightning);
    assert_eq!(lightning.len(), 1);
    assert_eq!(lightning[0].row, "Matrix");
    // Accents go above the hit, for half a beat (lightning a beat).
    let strobe = of_kind(&added, EffectKind::Strobe)[0];
    assert_eq!(strobe.layer, 2);
    assert_eq!(strobe.effect.end_ms - strobe.effect.start_ms, BEAT / 2);
    let hit = of_kind(&added, EffectKind::Impact)[0];
    assert_eq!(
        hit.effect.end_ms - hit.effect.start_ms,
        2 * BEAT,
        "2 beats at full intensity"
    );
    assert_eq!(params(&hit.effect)["color"], "palette");
    assert_eq!(hit.effect.palette.colors.len(), 1);
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn the_dip_is_skipped_when_its_time_is_taken() {
    let s = setup(false);
    let mut d = draft(&s);
    // The second hit's dip would cut into the first one's decay; one at 100 ms has no room.
    stage(
        &mut d,
        json!({ "cues": [
            { "cue": "hit", "at": 100, "targets": ["Mega Tree"] },
            { "cue": "hit", "at": 16_000, "targets": ["Mega Tree"], "intensity": 1.0 },
            { "cue": "hit", "at": 16_800, "targets": ["Mega Tree"] },
        ] }),
    )
    .unwrap();
    let added = staged(&d);
    let offs: Vec<u64> = of_kind(&added, EffectKind::Off)
        .iter()
        .map(|a| a.effect.start_ms)
        .collect();
    assert_eq!(offs, [16_000 - BEAT / 2]);
    let hits: Vec<(u64, usize)> = of_kind(&added, EffectKind::Impact)
        .iter()
        .map(|a| (a.effect.start_ms, a.layer))
        .collect();
    // The third overlaps the second's decay, so it goes a layer up.
    assert_eq!(hits, [(100, 1), (16_000, 1), (16_800, 2)]);
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn hits_on_the_same_instant_are_staged_once() {
    let s = setup(false);
    let mut d = draft(&s);
    let said = stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 8_000 }, { "cue": "hit", "at": 8_100 }] }),
    )
    .unwrap();
    assert!(
        said.contains("Staged 1 cue: 1 hit") && said.contains("Skipped 1"),
        "{said}"
    );
}

#[test]
fn a_blackout_goes_dark_through_the_stop_and_comes_back_with_a_hit() {
    let s = setup(false);
    let mut d = draft(&s);
    // Moment 2 (by importance) is the stop, 12–13 s.
    let said = stage(&mut d, json!({ "cues": [{ "cue": "blackout", "at": "m2" }] })).unwrap();
    assert!(said.starts_with("Staged 1 cue: 1 blackout"), "{said}");
    let added = staged(&d);
    for prop in PROPS {
        let here = on(&added, prop);
        let kinds: Vec<(EffectKind, u64, u64)> = here
            .iter()
            .map(|a| (a.effect.kind(), a.effect.start_ms, a.effect.end_ms))
            .collect();
        // No dip before the re-entry: the blackout is the contrast.
        assert_eq!(kinds[0], (EffectKind::Off, 12_000, 13_000), "{prop}");
        assert_eq!(kinds[1].0, EffectKind::Impact);
        assert_eq!(kinds[1].1, 13_000);
        assert_eq!(here.len(), 2, "{prop}");
    }
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn a_ramp_climbs_bar_by_bar_and_lands_on_the_drop() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "ramp", "at": 4_000, "until": 8_000 }] }),
    )
    .unwrap();
    let added = staged(&d);
    // Two bars, two steps: outlines and arches chase faster, the rest pulse on the beats higher.
    let roof: Vec<f64> = on(&added, "Left Roof")
        .iter()
        .filter(|a| a.effect.kind() == EffectKind::Chase)
        .map(|a| params(&a.effect)["speed"].as_f64().unwrap())
        .collect();
    assert_eq!(roof.len(), 2);
    assert!(roof[1] > roof[0], "{roof:?}");
    let tree: Vec<&Added> = on(&added, "Mega Tree")
        .into_iter()
        .filter(|a| a.effect.kind() == EffectKind::Pulse)
        .collect();
    assert_eq!(tree.len(), 2);
    assert_eq!((tree[0].effect.start_ms, tree[0].effect.end_ms), (4_000, 6_000));
    assert_eq!((tree[1].effect.start_ms, tree[1].effect.end_ms), (6_000, 8_000));
    let max = |a: &Added| params(&a.effect)["max"].as_f64().unwrap();
    assert!(max(tree[1]) > max(tree[0]));
    // The pulses step on the Beats track, which staging added.
    let beats = d
        .sequence()
        .unwrap()
        .timing_tracks
        .iter()
        .find(|t| t.name == "Beats")
        .unwrap()
        .id;
    assert_eq!(params(&tree[0].effect)["timingTrack"], json!(beats));
    // Heating up: a different, hotter color each step.
    assert_ne!(tree[0].effect.palette, tree[1].effect.palette);
    // The drop at 8 s is an impact: the ramp ends on a hit there.
    let hits = of_kind(&added, EffectKind::Impact);
    assert!(!hits.is_empty() && hits.iter().all(|a| a.effect.start_ms == 8_000));
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn a_sweep_reaches_each_prop_in_turn_across_the_layout() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "sweep", "at": 0, "until": 2_000, "targets": ["Left Roof", "Mega Tree", "Right Roof"] }] }),
    )
    .unwrap();
    let added = staged(&d);
    let starts: Vec<(&str, u64)> = added
        .iter()
        .map(|a| (a.row.as_str(), a.effect.start_ms))
        .collect();
    assert_eq!(starts[0], ("Left Roof", 0));
    assert_eq!(starts[1].0, "Mega Tree");
    assert_eq!(starts[2].0, "Right Roof");
    assert!(starts[1].1 > 0 && starts[2].1 > starts[1].1, "{starts:?}");
    assert!(added.iter().all(|a| a.effect.kind() == EffectKind::Wipe
        && params(&a.effect)["direction"] == "leftToRight"
        && a.effect.end_ms <= 2_000));
    assert_eq!(
        added[2].effect.end_ms, 2_000,
        "the last prop's wipe ends with the sweep"
    );
    // The other way round.
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "sweep", "at": 0, "until": 2_000, "direction": "rightToLeft" }] }),
    )
    .unwrap();
    let first = &added_first(&d);
    assert_eq!(first, "Right Roof");
}

fn added_first(d: &Draft) -> String {
    staged(d)[0].row.clone()
}

#[test]
fn a_chase_runs_prop_by_prop_left_to_right_on_eighth_notes() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "chase", "at": 10_000, "until": 12_000 }] }),
    )
    .unwrap();
    let added = staged(&d);
    // Eight eighth notes over a bar, around seven props, left to right.
    assert_eq!(added.len(), 8);
    let order: Vec<&str> = added.iter().map(|a| a.row.as_str()).collect();
    assert_eq!(&order[..7], &PROPS);
    assert_eq!(order[7], "Left Roof");
    let starts: Vec<u64> = added.iter().map(|a| a.effect.start_ms).collect();
    assert_eq!(starts, (0..8).map(|k| 10_000 + k * BEAT / 2).collect::<Vec<_>>());
    assert!(added.iter().all(|a| a.effect.kind() == EffectKind::Impact));
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn call_and_response_alternates_the_left_and_right_halves_by_bar() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "call_response", "at": 0, "until": 8_000 }] }),
    )
    .unwrap();
    let added = staged(&d);
    let at = |ms: u64| -> Vec<&str> {
        added
            .iter()
            .filter(|a| a.effect.start_ms == ms)
            .map(|a| a.row.as_str())
            .collect()
    };
    // Four props on the left (by position), three on the right; four bars.
    assert_eq!(at(0), ["Left Arch", "Left Roof", "Matrix", "Mega Tree"]);
    assert_eq!(at(2_000), ["Right Arch", "Right Roof", "Singing Bulb"]);
    assert_eq!(at(4_000), at(0));
    assert_eq!(at(6_000), at(2_000));
    // Two named sides.
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "call_response", "at": 0, "until": 4_000, "targets": ["Arches"], "with": ["Mega Tree"] }] }),
    )
    .unwrap();
    let rows: Vec<String> = added_rows(&d);
    assert_eq!(rows, ["Left Arch", "Right Arch", "Mega Tree"]);
    assert_well_formed(d.sequence().unwrap());
}

fn added_rows(d: &Draft) -> Vec<String> {
    staged(d).into_iter().map(|a| a.row).collect()
}

#[test]
fn the_left_and_right_words_split_the_show_by_where_props_sit() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 4_000, "targets": ["left"] }] }),
    )
    .unwrap();
    let mut rows: Vec<String> = added_rows(&d);
    rows.sort();
    rows.dedup();
    assert_eq!(rows, ["Left Arch", "Left Roof"]);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 4_000, "targets": ["right", "trees"] }] }),
    )
    .unwrap();
    let mut rows: Vec<String> = added_rows(&d);
    rows.sort();
    rows.dedup();
    assert_eq!(
        rows,
        ["Matrix", "Mega Tree", "Right Arch", "Right Roof", "Singing Bulb"]
    );
}

#[test]
fn a_word_pop_lands_on_the_matching_sung_words_and_the_biggest_sing() {
    let s = setup(true);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "word_pop", "at": 0, "until": 32_000, "match": "lantern", "targets": ["Mega Tree"], "intensity": 0.6 }] }),
    )
    .unwrap();
    let added = staged(&d);
    let pops: Vec<(&str, u64, u64)> = added
        .iter()
        .map(|a| (a.row.as_str(), a.effect.start_ms, a.effect.end_ms))
        .collect();
    // The word or a beat, whichever is longer.
    assert_eq!(
        pops,
        [("Mega Tree", 18_000, 18_600), ("Mega Tree", 26_000, 26_700)]
    );
    // The shout (moment 1, importance 0.9) pops everything and sets the face singing.
    let mut d = draft(&s);
    let said = stage(&mut d, json!({ "cues": [{ "cue": "word_pop", "at": "m1" }] })).unwrap();
    assert!(said.starts_with("Staged 1 cue: 1 word pop"), "{said}");
    let added = staged(&d);
    assert_eq!(of_kind(&added, EffectKind::Impact).len(), PROPS.len());
    let faces = of_kind(&added, EffectKind::Faces);
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].row, "Singing Bulb");
    assert_eq!(faces[0].effect.start_ms, 18_000);
    assert_eq!(params(&faces[0].effect)["face"], "Face");
    assert!(faces[0].layer > of_kind(&added, EffectKind::Impact)[0].layer);
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn sing_puts_faces_and_mouths_on_talking_props_through_the_vocals() {
    let s = setup(true);
    let mut d = draft(&s);
    stage(&mut d, json!({ "cues": [{ "cue": "sing", "at": 0 }] })).unwrap();
    let added = staged(&d);
    // The face sings on the phonemes staging made from the words, one effect per sung stretch.
    let spans: Vec<(&str, EffectKind, u64, u64)> = added
        .iter()
        .map(|a| {
            (
                a.row.as_str(),
                a.effect.kind(),
                a.effect.start_ms,
                a.effect.end_ms,
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            ("Singing Bulb", EffectKind::Faces, 17_000, 19_000),
            ("Singing Bulb", EffectKind::Faces, 26_000, 26_700)
        ]
    );
    let doc = d.sequence().unwrap();
    let phonemes = doc
        .timing_tracks
        .iter()
        .find(|t| t.kind == TimingKind::Phonemes)
        .unwrap();
    assert_eq!(params(&added[0].effect)["timingTrack"], json!(phonemes.id));
    // A prop without a face sings with its brightness, on the syllables.
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "sing", "at": 0, "targets": ["Mega Tree"] }] }),
    )
    .unwrap();
    let added = staged(&d);
    assert!(
        added
            .iter()
            .all(|a| a.row == "Mega Tree" && a.effect.kind() == EffectKind::Sing)
    );
    // Without sung words, sing is refused and nothing changes.
    let s = setup(false);
    let mut d = draft(&s);
    let refused = stage(&mut d, json!({ "cues": [{ "cue": "sing", "at": 0 }] })).unwrap_err();
    assert!(refused.contains("no sung words"), "{refused}");
    assert!(!d.has_edits());
}

#[test]
fn minimal_keeps_the_tree_breathing_and_darkens_the_rest() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "minimal", "at": 24_000, "until": 28_000 }] }),
    )
    .unwrap();
    let added = staged(&d);
    let tree = on(&added, "Mega Tree");
    let kinds: Vec<(EffectKind, usize)> = tree.iter().map(|a| (a.effect.kind(), a.layer)).collect();
    assert_eq!(kinds, [(EffectKind::Off, 1), (EffectKind::Pulse, 2)]);
    assert_eq!(params(&tree[1].effect)["source"], "bass");
    for prop in PROPS.iter().filter(|p| **p != "Mega Tree") {
        let here = on(&added, prop);
        assert_eq!(here.len(), 1, "{prop}");
        assert_eq!(here[0].effect.kind(), EffectKind::Off);
        assert!(here[0].effect.fade_in_ms > 0, "the rest dims into the breakdown");
    }
}

#[test]
fn full_gives_each_role_its_peak_effect_opened_by_a_hit() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "full", "at": 8_000, "until": 16_000, "intensity": 0.9 }] }),
    )
    .unwrap();
    let added = staged(&d);
    let look = |prop: &str| on(&added, prop)[0].effect.kind();
    assert_eq!(look("Matrix"), EffectKind::VuMeter);
    assert_eq!(look("Mega Tree"), EffectKind::Spiral);
    assert_eq!(look("Left Roof"), EffectKind::Chase);
    assert_eq!(look("Left Arch"), EffectKind::Chase);
    assert_eq!(params(&on(&added, "Left Arch")[0].effect)["bounce"], true);
    assert_eq!(look("Singing Bulb"), EffectKind::Pulse);
    // The opening hit goes above the look.
    let hit = on(&added, "Mega Tree")
        .into_iter()
        .find(|a| a.effect.kind() == EffectKind::Impact)
        .unwrap();
    assert_eq!((hit.effect.start_ms, hit.layer), (8_000, 2));
    assert_well_formed(d.sequence().unwrap());
}

#[test]
fn sustain_holds_then_fades_out() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "sustain", "at": 20_000, "until": 22_000, "targets": ["Mega Tree"] }] }),
    )
    .unwrap();
    let added = staged(&d);
    assert_eq!(added.len(), 1);
    let off = &added[0].effect;
    assert_eq!(
        (off.kind(), off.start_ms, off.end_ms),
        (EffectKind::Off, 20_800, 22_000)
    );
    assert_eq!(off.fade_in_ms, 1_200);
}

#[test]
fn a_color_shift_travels_across_the_props() {
    let s = setup(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "color_shift", "at": 4_000, "until": 8_000, "colors": ["#00ff00", "#0000ff"] }] }),
    )
    .unwrap();
    let added = staged(&d);
    assert!(
        added
            .iter()
            .all(|a| a.effect.kind() == EffectKind::ColorShift && a.effect.end_ms == 8_000)
    );
    let starts: Vec<(&str, u64)> = added
        .iter()
        .map(|a| (a.row.as_str(), a.effect.start_ms))
        .collect();
    assert_eq!(starts[0], ("Left Roof", 4_000));
    assert_eq!(starts.last().unwrap().0, "Right Roof");
    assert!(starts.windows(2).all(|w| w[0].1 <= w[1].1));
    assert!(starts.last().unwrap().1 < 4_000 + BEAT);
}

#[test]
fn breathe_pulses_with_the_bass_or_on_a_track() {
    let s = setup(false);
    let mut d = draft(&s);
    // Staging adds the Beats track a breath can follow.
    stage(
        &mut d,
        json!({ "cues": [
            { "cue": "breathe", "at": 0, "until": 4_000, "targets": ["Arches"] },
            { "cue": "breathe", "at": 4_000, "until": 8_000, "targets": ["Mega Tree"], "track": "Beats" },
        ] }),
    )
    .unwrap();
    let added = staged(&d);
    let sources: Vec<(&str, Value)> = added
        .iter()
        .map(|a| (a.row.as_str(), params(&a.effect)["source"].clone()))
        .collect();
    assert_eq!(
        sources,
        [
            ("Left Arch", json!("bass")),
            ("Right Arch", json!("bass")),
            ("Mega Tree", json!("marks"))
        ]
    );
    assert!(added.iter().all(|a| a.effect.kind() == EffectKind::Pulse));
}

/// Two pillars (12 × 50 matrices) either side of a roof line, with a wash on each for the whole
/// song. `own_rows`: a row for each prop; without, only a row for the group of the pillars.
fn pillars(own_rows: bool) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    let pillar = |name: &str, x: f32| {
        let matrix = Generator::Matrix {
            columns: 12,
            rows: 50,
            width: 0.6,
            height: 2.5,
            wiring: Default::default(),
        };
        at(Prop::new(name, ShapeSource::Generator(matrix)), x, 0.0)
    };
    let props = [
        pillar("Pillar Right", 3.0),
        line("Roof", -2.0, 5.0),
        pillar("Pillar Left", -3.0),
    ];
    let mut both = Group::new("Pillars");
    both.members = vec![GroupMember::Prop(props[0].id), GroupMember::Prop(props[2].id)];
    let mut edits: Vec<Edit> = props.iter().map(|p| Edit::AddProp { prop: p.clone() }).collect();
    edits.push(Edit::AddGroup { group: both.clone() });
    engine.apply(edits).unwrap();
    let mut rows: Vec<Row> = match own_rows {
        true => props.iter().map(|p| Row::new(Target::Prop(p.id))).collect(),
        false => vec![Row::new(Target::Group(both.id))],
    };
    for row in &mut rows {
        row.layers[0]
            .effects
            .push(Effect::new(EffectKind::ColorWash, 0, SONG_MS));
    }
    engine
        .new_sequence_doc_with_rows("Bones", SONG_MS, Some("/music/bones.mp3"), rows)
        .unwrap();
    Setup { engine, _dir: dir }
}

#[test]
fn a_dance_puts_dancers_on_the_matrices_with_a_pair_facing_each_other() {
    let s = pillars(true);
    let mut d = draft(&s);
    let said = stage(
        &mut d,
        json!({ "cues": [{ "cue": "dance", "at": 8_000, "until": 16_000, "intensity": 0.5 }] }),
    )
    .unwrap();
    assert!(said.starts_with("Staged 1 cue: 1 dance"), "{said}");
    let added = staged(&d);
    // Staging added the song's beats for them to follow.
    let beats = d
        .sequence()
        .unwrap()
        .timing_tracks
        .iter()
        .find(|t| t.name == "Beats")
        .unwrap()
        .id;
    assert!(on(&added, "Roof").is_empty(), "only the matrices dance");
    for (pillar, mirror) in [("Pillar Left", false), ("Pillar Right", true)] {
        let here = on(&added, pillar);
        // Dark over the look, the dancer over that.
        let kinds: Vec<(EffectKind, usize)> = here.iter().map(|a| (a.effect.kind(), a.layer)).collect();
        assert_eq!(kinds, [(EffectKind::Off, 1), (EffectKind::Dancer, 2)], "{pillar}");
        assert!(
            here.iter()
                .all(|a| (a.effect.start_ms, a.effect.end_ms) == (8_000, 16_000))
        );
        let dancer = params(&here[1].effect);
        assert_eq!(dancer["character"], "skeleton", "{pillar}");
        assert_eq!(dancer["mirror"], mirror, "{pillar}");
        assert_eq!(dancer["timingTrack"], json!(beats));
        assert_eq!(dancer["usePalette"], false);
        assert_eq!(
            dancer["bassBounce"].as_f64().map(|v| (v * 100.0).round()),
            Some(30.0)
        );
    }
    // Another character, in the cue's colors, for four bars; one prop alone isn't mirrored.
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "dance", "at": 20_000, "targets": ["Pillar Right"], "match": "Santa", "colors": ["#ff0000"] }] }),
    )
    .unwrap();
    let santa = of_kind(&staged(&d), EffectKind::Dancer)
        .into_iter()
        .find(|a| a.effect.start_ms == 20_000)
        .unwrap()
        .clone();
    assert_eq!(santa.row, "Pillar Right");
    assert_eq!(santa.effect.end_ms, 20_000 + 16 * BEAT);
    let settings = params(&santa.effect);
    assert_eq!(
        (
            &settings["character"],
            &settings["mirror"],
            &settings["usePalette"]
        ),
        (&json!("santa"), &json!(false), &json!(true))
    );
    assert_eq!(santa.effect.palette.colors, [pf_sequence::Rgb::RED]);
    assert_well_formed(d.sequence().unwrap());
    // Who dances is one of the characters.
    let before = staged(&d).len();
    let refused = stage(
        &mut d,
        json!({ "cues": [{ "cue": "dance", "at": 0, "match": "zombie" }] }),
    )
    .unwrap_err();
    assert!(
        refused.contains("skeleton, ghost, witch, santa, snowman, elf"),
        "{refused}"
    );
    assert_eq!(staged(&d).len(), before);
    // Props named that aren't matrices dance as named.
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "dance", "at": 0, "until": 2_000, "targets": ["Roof"] }] }),
    )
    .unwrap();
    assert_eq!(of_kind(&on_row(&staged(&d), "Roof"), EffectKind::Dancer).len(), 1);
}

fn on_row(added: &[Added], row: &str) -> Vec<Added> {
    added.iter().filter(|a| a.row == row).cloned().collect()
}

#[test]
fn a_dance_on_a_group_row_draws_a_dancer_on_each_of_its_props() {
    let s = pillars(false);
    let mut d = draft(&s);
    stage(
        &mut d,
        json!({ "cues": [{ "cue": "dance", "at": 4_000, "until": 8_000, "match": "ghost" }] }),
    )
    .unwrap();
    let added = staged(&d);
    let here = on(&added, "Pillars");
    assert_eq!(here.len(), 2, "{added:?}");
    let dancer = &here[1].effect;
    assert_eq!(dancer.kind(), EffectKind::Dancer);
    assert_eq!(dancer.render_style, pf_model::RenderStyle::PerModelDefault);
    assert_eq!(params(dancer)["character"], "ghost");
    assert_eq!(params(dancer)["mirror"], false);
    // Among every kind of prop, only the matrix dances.
    let s = setup(false);
    let mut d = draft(&s);
    stage(&mut d, json!({ "cues": [{ "cue": "dance", "at": 0 }] })).unwrap();
    let dancers = staged(&d);
    assert_eq!(of_kind(&dancers, EffectKind::Dancer).len(), 1);
    assert_eq!(of_kind(&dancers, EffectKind::Dancer)[0].row, "Matrix");
    // A show without matrices has nothing to dance on until props are named.
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    let roof = line("Roof", 0.0, 5.0);
    engine.apply(vec![Edit::AddProp { prop: roof.clone() }]).unwrap();
    let rows = vec![Row::new(Target::Prop(roof.id))];
    engine
        .new_sequence_doc_with_rows("Bones", SONG_MS, None, rows)
        .unwrap();
    let mut d = Draft::new(Workspace::from_engine(&engine, UiContext::default()));
    let refused = stage(&mut d, json!({ "cues": [{ "cue": "dance", "at": 0 }] })).unwrap_err();
    assert!(refused.contains("name the props to dance on"), "{refused}");
    assert!(!d.has_edits());
}

#[test]
fn many_cues_go_in_one_call_as_one_draft_step() {
    let s = setup(true);
    let mut d = draft(&s);
    let said = stage(
        &mut d,
        json!({ "cues": [
            { "cue": "hit", "at": 16_250 },
            { "cue": "full", "at": 8_000, "until": 12_000 },
            { "cue": "blackout", "at": 12_000, "until": 13_000 },
            { "cue": "word_pop", "at": 18_000, "match": "lantern", "intensity": 0.6 },
            { "cue": "ramp", "at": 4_000, "until": 8_000 },
            { "cue": "minimal", "at": 24_000, "until": 28_000 },
        ] }),
    )
    .unwrap();
    // 8 s gets one hit (from the ramp or the full look, not both).
    assert!(
        said.starts_with("Staged 6 cues: 1 hit, 1 blackout, 1 ramp, 1 word pop, 1 minimal look, 1 full look"),
        "{said}"
    );
    let doc = d.sequence().unwrap();
    assert_well_formed(doc);
    // The hit inside the full look goes above it.
    let added = staged(&d);
    let hit = on(&added, "Mega Tree")
        .into_iter()
        .find(|a| a.effect.start_ms == 16_250)
        .unwrap();
    assert_eq!(hit.layer, 1, "16.25 s is after the full look");
    // One cue that fails refuses the call: nothing is staged.
    let mut d = draft(&s);
    let refused = stage(
        &mut d,
        json!({ "cues": [{ "cue": "hit", "at": 1_000 }, { "cue": "hit", "at": 40_000 }] }),
    )
    .unwrap_err();
    assert!(
        refused.contains("cue 2 (hit)") && refused.contains("past the end"),
        "{refused}"
    );
    assert!(!d.has_edits());
    for (input, says) in [
        (
            json!({ "cues": [{ "cue": "explode", "at": 0 }] }),
            "cue is one of",
        ),
        (
            json!({ "cues": [{ "cue": "hit", "at": 0, "targets": ["Garage"] }] }),
            "no group or prop \"Garage\"",
        ),
        (
            json!({ "cues": [{ "cue": "hit", "at": 0, "speed": 2 }] }),
            "a cue has no speed",
        ),
        (json!({ "cues": [{ "cue": "hit", "at": "m99" }] }), "no moment 99"),
        (
            json!({ "cues": [{ "cue": "hit", "at": 4_000, "until": 3_000 }] }),
            "until must come after at",
        ),
        (json!({}), "Give cues, or stageMoments"),
    ] {
        let error = stage(&mut d, input).unwrap_err();
        assert!(error.contains(says), "{error}");
    }
}

#[test]
fn stage_moments_gives_each_moment_its_suggested_treatment() {
    let s = setup(true);
    let mut d = draft(&s);
    let said = stage(&mut d, json!({ "stageMoments": { "minImportance": 0.5 } })).unwrap();
    // Above 0.5: the drop (burst: a hit), the shout (word pop), the stop (blackout, back with a
    // hit that is the restart's too), the restart (a hit, already there), the impact (hit), the
    // build (ramp), the breakdown (minimal), the fill (chase). The hold (0.4) is left out.
    assert!(
        said.starts_with("Staged 7 cues: 2 hits, 1 blackout, 1 ramp, 1 chase, 1 word pop, 1 minimal look"),
        "{said}"
    );
    assert!(said.contains("Skipped 1"), "{said}");
    let added = staged(&d);
    let tree: Vec<(EffectKind, u64)> = on(&added, "Mega Tree")
        .iter()
        .map(|a| (a.effect.kind(), a.effect.start_ms))
        .collect();
    // The ramp's own end hit is left to the drop.
    assert_eq!(
        tree.iter()
            .filter(|(k, t)| *k == EffectKind::Impact && *t == 8_000)
            .count(),
        1
    );
    assert!(tree.contains(&(EffectKind::Pulse, 4_000)));
    assert!(tree.contains(&(EffectKind::Off, 12_000)));
    assert!(tree.contains(&(EffectKind::Impact, 13_000)));
    assert!(tree.contains(&(EffectKind::Impact, 16_250)));
    assert!(tree.contains(&(EffectKind::Impact, 18_000)), "the shout pops");
    // The shout is big: the face sings it.
    assert!(
        of_kind(&added, EffectKind::Faces)
            .iter()
            .any(|a| a.effect.start_ms == 18_000)
    );
    assert_well_formed(d.sequence().unwrap());
    // Only some kinds.
    let mut d = draft(&s);
    let said = stage(
        &mut d,
        json!({ "stageMoments": { "minImportance": 0.0, "kinds": ["stop", "hold"] } }),
    )
    .unwrap();
    assert!(said.starts_with("Staged 2 cues: 1 blackout, 1 sustain"), "{said}");
    let error = stage(&mut d, json!({ "stageMoments": { "kinds": ["solo"] } })).unwrap_err();
    assert!(error.contains("no moment kind \"solo\""), "{error}");
}

#[test]
fn staging_is_deterministic() {
    let s = setup(true);
    let strip = |d: &Draft| -> Vec<(String, usize, Value)> {
        staged(d)
            .into_iter()
            .map(|a| {
                let mut effect = serde_json::to_value(&a.effect).unwrap();
                effect["id"] = Value::Null;
                // Timing track ids are new each time staging adds a track.
                if let Some(params) = effect["params"].as_object_mut() {
                    params.remove("timingTrack");
                }
                (a.row, a.layer, effect)
            })
            .collect()
    };
    let input = json!({
        "stageMoments": { "minImportance": 0.3 },
        "cues": [{ "cue": "sweep", "at": 2_000, "direction": "centerOut" }, { "cue": "call_response", "at": 28_000 }],
    });
    let mut a = draft(&s);
    let mut b = draft(&s);
    assert_eq!(stage(&mut a, input.clone()), stage(&mut b, input));
    assert_eq!(strip(&a), strip(&b));
    assert!(!strip(&a).is_empty());
}

#[test]
fn every_cue_stays_inside_the_sequence() {
    let s = setup(true);
    let mut d = draft(&s);
    let cues: Vec<Value> = pf_ai::cues::CUES
        .iter()
        .map(|cue| json!({ "cue": cue, "at": SONG_MS - 300, "targets": ["all"] }))
        .collect();
    stage(&mut d, json!({ "cues": cues })).unwrap();
    assert_well_formed(d.sequence().unwrap());
    assert!(staged(&d).iter().all(|a| a.effect.end_ms <= SONG_MS));
}

#[test]
fn a_session_analyzes_stages_and_proposes_one_undo_step() {
    let s = setup(true);
    let mut engine = s.engine;
    let before = engine.sequence_document().unwrap().clone();
    let provider = ScriptedProvider::new(vec![
        calls("", &[("analyze_song", json!({}))]),
        calls(
            "",
            &[("stage_cue", json!({ "stageMoments": { "minImportance": 0.5 } }))],
        ),
        calls(
            "",
            &[(
                "stage_cue",
                json!({ "cues": [
                    { "cue": "sweep", "at": 2_000, "until": 3_000 },
                    { "cue": "hit", "at": "m4", "targets": ["Arches"] },
                    { "cue": "sustain", "at": 20_000, "until": 22_000 },
                ] }),
            )],
        ),
        calls(
            "",
            &[(
                "propose_changes",
                json!({ "summary": "A first pass with the big moments staged." }),
            )],
        ),
        says("Here it is."),
    ]);
    let mut session = ChatSession::new().with_analyzer(Arc::new(|_: &Path, _: &Cancel| Ok(analysis())));
    let workspace = Workspace::from_engine(&engine, UiContext::default());
    let mut labels = Vec::new();
    let reply = session
        .run_turn(
            &provider,
            &fake_key(),
            "scripted",
            "make it dramatic",
            workspace,
            &Cancel::new(),
            &mut |e| {
                if let pf_ai::ChatEvent::Activity { label } = e {
                    labels.push(label);
                }
            },
        )
        .unwrap();
    assert!(labels.iter().any(|l| l == "Drafting: staging cues"), "{labels:?}");
    let results: Vec<(String, bool)> = match provider.requests()[2].last() {
        Some(Message::ToolResults(results)) => {
            results.iter().map(|r| (r.content.clone(), r.is_error)).collect()
        }
        other => panic!("{other:?}"),
    };
    assert!(
        !results[0].1 && results[0].0.starts_with("Staged 7 cues"),
        "{results:?}"
    );
    let proposal = reply.proposal.unwrap();
    // Counted over both calls (m4 is the impact at 16.25 s: the arches hit again, above).
    assert_eq!(
        proposal.cues.as_deref(),
        Some(
            "Staged 10 cues: 3 hits, 1 blackout, 1 ramp, 1 sweep, 1 chase, 1 word pop, 1 minimal look, 1 sustain"
        )
    );
    assert!(proposal.changes_sequence && !proposal.changes_show);
    apply_proposal(&mut engine, session.proposal().unwrap()).unwrap();
    let after = engine.sequence_document().unwrap().clone();
    assert_ne!(after, before);
    assert_well_formed(&after);
    // One undo takes all of it back.
    engine.undo_sequence().unwrap();
    assert_eq!(engine.sequence_document().unwrap(), &before);
}

#[test]
fn cue_summaries_count_by_kind_most_first() {
    let counts: BTreeMap<String, usize> =
        [("hit", 6), ("word_pop", 3), ("blackout", 2), ("call_response", 1)]
            .into_iter()
            .map(|(k, n)| (k.to_string(), n))
            .collect();
    assert_eq!(
        pf_ai::cues::summary(&counts).unwrap(),
        "Staged 12 cues: 6 hits, 3 word pops, 2 blackouts, 1 call and response"
    );
    assert_eq!(pf_ai::cues::summary(&BTreeMap::new()), None);
    // The effects are ordinary ones: their settings are the kind's own.
    let s = setup(false);
    let mut d = draft(&s);
    stage(&mut d, json!({ "cues": [{ "cue": "hit", "at": 4_000 }] })).unwrap();
    for a in staged(&d) {
        let back: EffectParams = serde_json::from_value(params(&a.effect)).unwrap();
        assert_eq!(back, a.effect.params);
    }
}
