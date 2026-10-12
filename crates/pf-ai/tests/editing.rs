//! What a person does to effects already in a sequence, the Assistant can do in a call: change
//! many at once (settings, colors, sparkles, curves), retime them, copy them to other rows, move
//! them, remove them; and place effects with everything an effect can have. Each refusal says
//! what would work.

use pf_ai::provider::ToolCall;
use pf_ai::run::{Outcome, run_tool};
use pf_ai::stage::{Stage, Suit};
use pf_ai::{Cancel, Draft, OpenDoc, Toolbox, UiContext, Workspace};
use pf_render::{AudioSource, Pictures};
use pf_sequence::{Blend, CurveShape, Effect, EffectKind, EffectParams, Sequence, Target};
use serde_json::{Value, json};

struct Setup {
    stage: Stage,
    draft: Draft,
    arches: Vec<String>,
    roofline: String,
    matrix: String,
}

fn setup() -> Setup {
    let stage = Stage::new();
    let doc = stage.sequence();
    let row = |target: Target| stage.row(&doc, target).unwrap().to_string();
    let arches = stage.arches.iter().map(|a| row(Target::Prop(*a))).collect();
    let (roofline, matrix) = (row(stage.target(Suit::String)), row(stage.target(Suit::Matrix)));
    let draft = Draft::new(Workspace {
        show: stage.show.clone(),
        revision: 0,
        show_generation: 0,
        sequence: Some(OpenDoc { id: 1, doc }),
        music: None,
        audio: AudioSource::none(),
        pictures: Pictures::none(),
        images: vec![pf_ai::stage::PICTURE.to_string()],
        shaders: Vec::new(),
        context: UiContext::default(),
    });
    Setup {
        stage,
        draft,
        arches,
        roofline,
        matrix,
    }
}

/// Calls a tool as the model would; the answer, and whether it was an error.
fn call(draft: &mut Draft, name: &str, input: Value) -> (String, bool) {
    let toolbox = Toolbox::new();
    let cancel = Cancel::new();
    let mut cache = None;
    let analyzer = |_: &std::path::Path, _: &Cancel| Err("no song".to_string());
    let mut song = pf_ai::song::Song {
        music: None,
        cache: &mut cache,
        analyzer: &analyzer,
        cancel: &cancel,
    };
    let call = ToolCall {
        id: "1".into(),
        name: name.into(),
        input,
        input_error: None,
    };
    match run_tool(&toolbox, &call, draft, &mut song) {
        Outcome::Answer { content, is_error } => (content, is_error),
        other => panic!("{name} answered {other:?}"),
    }
}

fn done(draft: &mut Draft, name: &str, input: Value) -> String {
    let (answer, is_error) = call(draft, name, input.clone());
    assert!(!is_error, "{name} {input}: {answer}");
    answer
}

fn refused(draft: &mut Draft, name: &str, input: Value) -> String {
    let before = draft.sequence().cloned();
    let (answer, is_error) = call(draft, name, input.clone());
    assert!(is_error, "{name} {input} should be refused: {answer}");
    assert_eq!(draft.sequence().cloned(), before, "a refusal changes nothing");
    answer
}

/// The effects on the row with id `row`, by layer.
fn on_row(doc: &Sequence, row: &str) -> Vec<Vec<Effect>> {
    doc.rows
        .iter()
        .find(|r| r.id.to_string() == row)
        .map(|r| r.layers.iter().map(|l| l.effects.clone()).collect())
        .unwrap_or_default()
}

/// A bar of Bars on each arch for four bars from 24 s: sixteen effects.
fn bars_on_the_arches(s: &mut Setup) {
    let answer = done(
        &mut s.draft,
        "place_effects",
        json!({
            "rowIds": s.arches, "fromMs": 24_000, "toMs": 32_000, "track": "Bars",
            "effect": { "kind": "bars", "settings": { "axis": "horizontal", "count": 3 }, "colors": ["#ff0000", "#ffffff"] },
        }),
    );
    assert!(answer.starts_with("Placed 16 Bars effects on 4 rows"), "{answer}");
}

#[test]
fn place_effects_takes_everything_an_effect_has_and_tracks_by_name() {
    let mut s = setup();
    done(
        &mut s.draft,
        "place_effects",
        json!({
            "rowIds": [s.roofline], "fromMs": 24_000, "toMs": 28_000,
            "effect": {
                "kind": "chase",
                "settings": { "bands": 2, "timingTrack": "Beats" },
                "colors": ["#00ff00"], "blend": "max", "fadeInMs": 100,
                "sparkles": 40, "sparkleColor": "#ffd700", "musicSparkles": true, "blur": 1,
                "brightness": 0.8, "hueAdjust": 0.1, "renderStyle": "singleLine", "bufferTransform": "flipHorizontal",
                "curves": { "brightness": { "shape": "timingFade", "from": 0.3, "to": 1, "fade": 8, "timingTrack": "Bars" } },
            },
        }),
    );
    let doc = s.draft.sequence().unwrap();
    let effect = &on_row(doc, &s.roofline)[0][0];
    let track = |name: &str| doc.timing_tracks.iter().find(|t| t.name == name).unwrap().id;
    let EffectParams::Chase(chase) = &effect.params else {
        panic!("{:?}", effect.params)
    };
    assert_eq!(chase.timing_track, Some(track("Beats")));
    assert_eq!(
        (effect.sparkles, effect.music_sparkles, effect.blur, effect.blend),
        (40, true, 1, Blend::Max)
    );
    assert_eq!(effect.render_style, pf_model::RenderStyle::SingleLine);
    let curve = &effect.curves["brightness"];
    assert_eq!(
        (curve.shape, curve.timing_track),
        (CurveShape::TimingFade, Some(track("Bars")))
    );

    // What doesn't fit is answered with what would, and nothing is drafted.
    let place = |effect: Value| json!({ "rowIds": [s.matrix], "fromMs": 0, "toMs": 1000, "effect": effect });
    let range = refused(
        &mut s.draft,
        "place_effects",
        place(json!({ "kind": "bars", "settings": { "count": 400 } })),
    );
    assert_eq!(range, "count is 400; use 1 to 100 (default 4).");
    let track = refused(
        &mut s.draft,
        "place_effects",
        place(json!({ "kind": "chase", "settings": { "timingTrack": "Claps" } })),
    );
    assert!(
        track.contains("no such timing track") && track.contains("Beats, Bars, Lyrics"),
        "{track}"
    );
    let picture = refused(
        &mut s.draft,
        "place_effects",
        place(json!({ "kind": "picture", "settings": { "file": "images/reindeer.png" } })),
    );
    assert!(
        picture.contains("isn't one of the show's pictures: images/snowman.png"),
        "{picture}"
    );
}

#[test]
fn many_effects_are_changed_retimed_copied_moved_and_removed_a_call_each() {
    let mut s = setup();
    bars_on_the_arches(&mut s);
    let chorus = json!({ "rowIds": s.arches, "fromMs": 24_000, "toMs": 32_000 });
    let with = |more: Value| {
        let mut input = chorus.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        input
    };

    // New colors, a faster speed, and sparkles on all sixteen; what isn't named stays.
    let answer = done(
        &mut s.draft,
        "change_effects",
        with(
            json!({ "change": { "colors": ["#00c040", "#ffd700"], "settings": { "speed": 2 }, "sparkles": 30 } }),
        ),
    );
    assert_eq!(answer, "Changed 16 effects on 4 rows (in your draft).");
    for row in &s.arches {
        for effect in &on_row(s.draft.sequence().unwrap(), row)[0] {
            let EffectParams::Bars(bars) = &effect.params else {
                panic!()
            };
            assert_eq!((bars.count, bars.speed, effect.sparkles), (3, 2.0, 30));
            assert_eq!(effect.palette.colors.len(), 2);
            assert_eq!(effect.palette.colors[0], pf_sequence::Rgb::new(0, 0xc0, 0x40));
        }
    }

    // Each a second long, and all half a beat later.
    done(
        &mut s.draft,
        "change_effects",
        with(json!({ "lengthMs": 1_000, "shiftMs": 250 })),
    );
    let first = on_row(s.draft.sequence().unwrap(), &s.arches[0])[0].clone();
    let times: Vec<(u64, u64)> = first.iter().map(|e| (e.start_ms, e.end_ms)).collect();
    assert_eq!(
        times,
        [
            (24_250, 25_250),
            (26_250, 27_250),
            (28_250, 29_250),
            (30_250, 31_250)
        ]
    );

    // One arch's four copied to the roofline, ids new; then moved up a layer there.
    let ids: Vec<String> = first.iter().map(|e| e.id.to_string()).collect();
    let answer = done(
        &mut s.draft,
        "change_effects",
        json!({ "ids": ids, "copyToRowIds": [s.roofline] }),
    );
    assert_eq!(answer, "Copied 4 effects to 1 row (in your draft).");
    let copies = on_row(s.draft.sequence().unwrap(), &s.roofline)[0].clone();
    assert_eq!(copies.len(), 4);
    assert!(copies.iter().all(|c| !ids.contains(&c.id.to_string())));
    assert_eq!(copies[0].params, first[0].params);
    done(
        &mut s.draft,
        "change_effects",
        json!({ "rowIds": [s.roofline], "moveTo": { "layer": 1 } }),
    );
    let layers = on_row(s.draft.sequence().unwrap(), &s.roofline);
    assert_eq!((layers[0].len(), layers[1].len()), (0, 4));

    // Only the Bars of the first half of the stretch removed.
    let answer = done(
        &mut s.draft,
        "change_effects",
        json!({ "kinds": ["bars"], "rowIds": s.arches, "toMs": 28_000, "remove": true }),
    );
    assert_eq!(answer, "Removed 8 effects from 4 rows (in your draft).");
    assert_eq!(on_row(s.draft.sequence().unwrap(), &s.arches[3])[0].len(), 2);
}

#[test]
fn a_change_that_cant_be_made_says_what_can() {
    let mut s = setup();
    bars_on_the_arches(&mut s);
    let which = refused(&mut s.draft, "change_effects", json!({ "remove": true }));
    assert!(
        which.contains("Say which effects") && which.contains("ids, rowIds, fromMs"),
        "{which}"
    );
    let none = refused(
        &mut s.draft,
        "change_effects",
        json!({ "kinds": ["spiral"], "remove": true }),
    );
    assert!(none.contains("No effects match"), "{none}");
    let kind = refused(
        &mut s.draft,
        "change_effects",
        json!({ "kinds": ["stripes"], "remove": true }),
    );
    assert!(
        kind.contains("no effect kind \"stripes\"") && kind.contains("bars"),
        "{kind}"
    );
    let what = refused(&mut s.draft, "change_effects", json!({ "fromMs": 0 }));
    assert!(
        what.contains("Give one thing to do") && what.contains("16 effects match"),
        "{what}"
    );
    let both = refused(
        &mut s.draft,
        "change_effects",
        json!({ "fromMs": 0, "remove": true, "shiftMs": 100 }),
    );
    assert!(both.contains("Give one thing to do"), "{both}");
    // A setting the effects' kind doesn't have, named with the ones it does.
    let setting = refused(
        &mut s.draft,
        "change_effects",
        json!({ "fromMs": 0, "change": { "settings": { "bands": 3 } } }),
    );
    assert!(
        setting.contains("The Bars effect has no setting \"bands\"")
            && setting.contains("count, speed, axis"),
        "{setting}"
    );
    let early = refused(
        &mut s.draft,
        "change_effects",
        json!({ "fromMs": 0, "shiftMs": -30_000 }),
    );
    assert!(early.contains("before the sequence does"), "{early}");
    let late = refused(
        &mut s.draft,
        "change_effects",
        json!({ "fromMs": 0, "shiftMs": 90_000 }),
    );
    assert!(late.contains("past the end of the sequence"), "{late}");
    // Stretching each to three bars would overlap the next on its layer.
    let overlap = refused(
        &mut s.draft,
        "change_effects",
        json!({ "fromMs": 0, "lengthMs": 6_000 }),
    );
    assert!(overlap.contains("would overlap"), "{overlap}");
    // Copying onto a row with something in the way: refused, or (with replace) it takes its place.
    done(
        &mut s.draft,
        "place_effects",
        json!({ "rowIds": [s.roofline], "fromMs": 24_500, "toMs": 25_000, "effect": { "kind": "on" } }),
    );
    let copy = json!({ "rowIds": [s.arches[0]], "copyToRowIds": [s.roofline] });
    let taken = refused(&mut s.draft, "change_effects", copy.clone());
    assert!(
        taken.contains("already has an effect on layer 0") && taken.contains("replace: true"),
        "{taken}"
    );
    let mut replacing = copy;
    replacing["replace"] = json!(true);
    let answer = done(&mut s.draft, "change_effects", replacing);
    assert_eq!(
        answer,
        "Copied 4 effects to 1 row, replacing 1 effect (in your draft)."
    );
    let kinds: Vec<EffectKind> = on_row(s.draft.sequence().unwrap(), &s.roofline)[0]
        .iter()
        .map(Effect::kind)
        .collect();
    assert_eq!(kinds, [EffectKind::Bars; 4]);
}

#[test]
fn props_are_listed_with_their_role_grid_and_faces_and_a_long_sequence_reads_back_briefly() {
    let mut s = setup();
    let props: Value = serde_json::from_str(&done(&mut s.draft, "list_props", json!({}))).unwrap();
    let prop = |name: &str| {
        props
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))
            .clone()
    };
    let matrix = prop("Matrix");
    assert_eq!(
        (matrix["role"].as_str(), matrix["grid"].as_str()),
        (Some("matrix"), Some("40x20"))
    );
    assert_eq!(matrix["faces"], json!(["Face"]));
    let tree = prop("Mega Tree");
    assert_eq!(
        (tree["role"].as_str(), tree["grid"].as_str()),
        (Some("tree"), Some("16x30"))
    );
    assert!(tree.get("faces").is_none());
    let roofline = prop("Roofline");
    assert_eq!(roofline["role"], "outline");
    assert!(roofline.get("grid").is_none(), "a single run has no grid");
    assert_eq!(prop("Arch 2")["role"], "arch");
    let _ = &s.stage;

    // Sixteen effects come back in full; asked for briefly, as a line each.
    bars_on_the_arches(&mut s);
    let full: Value = serde_json::from_str(&done(&mut s.draft, "list_sequence_effects", json!({}))).unwrap();
    assert_eq!(full.as_array().unwrap().len(), 16);
    assert!(full[0]["effect"]["params"]["count"].is_number());
    let brief: Value = serde_json::from_str(&done(
        &mut s.draft,
        "list_sequence_effects",
        json!({ "brief": true }),
    ))
    .unwrap();
    assert_eq!(brief["rows"].as_array().unwrap().len(), 4);
    assert_eq!(brief["rows"][0]["lights"], "Arch 1");
    assert_eq!(brief["rows"][0]["effects"].as_array().unwrap().len(), 4);
    assert_eq!(brief["each"], "[id, layer, kind, startMs, endMs]");
    assert_eq!(brief["rows"][0]["effects"][0][2], "bars");
    // More than fits in full is given briefly, saying so, rather than cut off mid-effect.
    for row in [&s.roofline, &s.matrix] {
        done(
            &mut s.draft,
            "place_effects",
            json!({ "rowIds": [row], "fromMs": 0, "toMs": 96_000, "track": "Beats", "effect": { "kind": "twinkle" } }),
        );
    }
    let (many, is_error) = call(&mut s.draft, "list_sequence_effects", json!({}));
    assert!(
        !is_error && !many.contains("cut short"),
        "{}",
        &many[many.len() - 200..]
    );
    let many: Value = serde_json::from_str(&many).unwrap();
    let listed: usize = many["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["effects"].as_array().unwrap().len())
        .sum();
    assert_eq!(listed, 16 + 2 * 192);
    assert!(
        many["note"]
            .as_str()
            .unwrap()
            .contains("too many to give in full"),
        "{}",
        many["note"]
    );
    // And more than fits even briefly, as each row's layers: how many effects, of which kinds, when.
    for layer in 1..5 {
        done(
            &mut s.draft,
            "place_effects",
            json!({ "rowIds": [s.roofline, s.matrix], "fromMs": 0, "toMs": 96_000, "track": "Beats", "layer": layer, "effect": { "kind": "strobe" } }),
        );
    }
    let (most, is_error) = call(&mut s.draft, "list_sequence_effects", json!({ "brief": true }));
    assert!(
        !is_error && !most.contains("cut short"),
        "{}",
        &most[most.len() - 200..]
    );
    let most: Value = serde_json::from_str(&most).unwrap();
    let roofline = most["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["lights"] == "Roofline")
        .unwrap();
    assert_eq!(roofline["layers"].as_array().unwrap().len(), 5);
    assert_eq!(roofline["layers"][1]["effects"], 192);
    assert_eq!(roofline["layers"][1]["kinds"], json!(["strobe"]));
    assert!(
        most["note"].as_str().unwrap().contains("too many to list"),
        "{}",
        most["note"]
    );
}
