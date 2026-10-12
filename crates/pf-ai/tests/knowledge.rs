//! The Assistant's knowledge stays complete as PixelFlow grows: every effect kind, setting,
//! choice, blend, curve shape, render style, cue, and timing track is in a guide the Assistant
//! can fetch, every name a guide uses is a real one, and the guides fit their budgets. When one
//! of these fails, its message says what to write and where.

use pf_ai::effect_spec::{self, Context};
use pf_ai::knowledge::{self, MAX_GUIDE_BYTES, check, library, reference};
use pf_ai::provider::ToolCall;
use pf_ai::run::{Outcome, run_tool};
use pf_ai::stage::Stage;
use pf_ai::{Cancel, Draft, OpenDoc, Toolbox, UiContext, Workspace};
use pf_model::{GroupLayout, RenderStyle};
use pf_render::{AudioSource, Pictures};
use pf_sequence::{Blend, CurveShape, EffectKind, RowBlending, SettingValue};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn guides_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("guides")
}

/// A draft on the stage, with its sequence open.
fn draft() -> Draft {
    let stage = Stage::new();
    let doc = stage.sequence();
    Draft::new(Workspace {
        show: stage.show,
        revision: 0,
        show_generation: 0,
        sequence: Some(OpenDoc { id: 1, doc }),
        music: None,
        audio: AudioSource::none(),
        pictures: Pictures::none(),
        images: Vec::new(),
        context: UiContext::default(),
    })
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

fn guide(input: Value) -> String {
    let (answer, is_error) = call(&mut draft(), "get_guide", input);
    assert!(!is_error, "{answer}");
    answer
}

/// A topic as written (its generated lists not filled in): what a person typed.
fn written_topic(key: &str) -> String {
    library::topic_source(key)
        .unwrap_or_else(|| panic!("there is no \"{key}\" topic: write crates/pf-ai/guides/topics/{key}.md and list it in knowledge/library.rs"))
        .to_string()
}

/// Fails unless the topic `key`, as written, names every one of `values` in backticks.
fn topic_names_all(key: &str, what: &str, values: impl IntoIterator<Item = String>) {
    let written = written_topic(key);
    let missing: Vec<String> = values
        .into_iter()
        .filter(|v| !written.contains(&format!("`{v}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "The \"{key}\" topic (crates/pf-ai/guides/topics/{key}.md) doesn't say what these {what} are for: {}. Add a line for each, naming it in backticks, saying when to use it.",
        missing.join(", ")
    );
}

#[test]
fn every_effect_kinds_reference_has_every_setting_and_choice() {
    for info in pf_sequence::effect_catalog() {
        let key = reference::key_of(&info.kind);
        let text = knowledge::effect_text(info.kind);
        assert!(text.contains(&format!("(`{key}`)")), "{key}: {text}");
        assert!(text.contains(info.description), "{key}");
        for setting in &info.settings {
            assert!(
                text.contains(&format!("- `{}` ({}", setting.key, setting.label)),
                "{key}.{} is missing from its reference",
                setting.key
            );
            if let SettingValue::Choice { options, default } = &setting.value {
                for option in options {
                    assert!(
                        text.contains(&format!("`{}` ({}", option.value, option.label)),
                        "{key}.{} = {} is missing from its reference",
                        setting.key,
                        option.value
                    );
                }
                assert!(
                    text.contains(&format!("default `{default}`")),
                    "{key}.{}",
                    setting.key
                );
            }
        }
        assert!(
            text.len() <= MAX_GUIDE_BYTES,
            "The {key} guide is {} bytes as the Assistant reads it; keep it under {MAX_GUIDE_BYTES} so several fit in one answer.",
            text.len()
        );
    }
}

#[test]
fn every_blend_curve_shape_layout_cue_and_timing_track_is_in_its_topic() {
    topic_names_all("blends", "blends", Blend::ALL.iter().map(|b| b.key().to_string()));
    topic_names_all(
        "blends",
        "ways rows meet",
        RowBlending::ALL.iter().map(reference::key_of),
    );
    topic_names_all(
        "curves",
        "curve shapes",
        CurveShape::ALL.iter().map(reference::key_of),
    );
    topic_names_all(
        "curves",
        "settings any effect's curves can drive",
        reference::effect_wide_curve_keys().into_iter().map(String::from),
    );
    topic_names_all(
        "layouts",
        "render styles",
        RenderStyle::ALL.iter().map(reference::key_of),
    );
    topic_names_all(
        "layouts",
        "group layouts",
        GroupLayout::ALL.iter().map(reference::key_of),
    );
    topic_names_all("layouts", "turns and flips", reference::buffer_transform_keys());
    topic_names_all("timing", "timing track kinds", reference::timing_kind_keys());
    topic_names_all(
        "timing",
        "song tracks",
        pf_ai::song::TRACK_CHOICES.iter().map(|t| t.to_string()),
    );
    topic_names_all(
        "timing",
        "ways place_effects shares marks out",
        pf_ai::arrange::SPREADS.iter().map(|s| s.to_string()),
    );
    topic_names_all("cues", "cues", pf_ai::cues::CUES.iter().map(|c| c.to_string()));
    topic_names_all(
        "cues",
        "target words",
        pf_ai::cues::TARGET_WORDS.iter().map(|c| c.to_string()),
    );
    topic_names_all(
        "cues",
        "directions",
        pf_ai::cues::DIRECTIONS.iter().map(|c| c.to_string()),
    );
    topic_names_all(
        "effect-wide",
        "settings every effect has",
        reference::effect_wide_keys(),
    );

    // The generated lists carry every one too, with what the code's own comments say.
    let blends = knowledge::topic_text("blends").unwrap();
    for blend in Blend::ALL {
        assert!(
            blends.contains(&format!("- `{}`: ", blend.key())),
            "{}",
            blend.key()
        );
    }
    let curves = knowledge::topic_text("curves").unwrap();
    for shape in CurveShape::ALL {
        assert!(curves.contains(&format!("- `{}`: ", reference::key_of(&shape))));
    }
    let cues = knowledge::topic_text("cues").unwrap();
    for (cue, note) in pf_ai::cues::CUE_NOTES {
        assert!(cues.contains(&format!("- `{cue}`: {note}")), "{cue}");
    }
    // The notes are the cues, in order; the tool says the same of each.
    assert_eq!(pf_ai::cues::CUE_NOTES.map(|(cue, _)| cue), pf_ai::cues::CUES);
    let stage_cue = Toolbox::new().find("stage_cue").unwrap().spec.description.clone();
    for (cue, note) in pf_ai::cues::CUE_NOTES {
        assert!(stage_cue.contains(&format!("{cue}: {note}")), "{cue}");
    }
}

#[test]
fn no_topic_asks_for_a_list_that_isnt_generated() {
    for key in library::topic_keys() {
        let text = knowledge::topic_text(key).unwrap();
        assert!(
            !text.contains("{{"),
            "The \"{key}\" topic asks for a list PixelFlow doesn't generate; the lists are: {}.",
            knowledge::GENERATED.join(", ")
        );
        assert!(
            text.len() <= MAX_GUIDE_BYTES,
            "The \"{key}\" topic is {} bytes as the Assistant reads it; keep it under {MAX_GUIDE_BYTES}.",
            text.len()
        );
    }
}

#[test]
fn every_guide_on_disk_is_in_the_app_and_every_guide_in_the_app_is_on_disk() {
    let on_disk = |folder: &str| -> BTreeSet<String> {
        std::fs::read_dir(guides_dir().join(folder))
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.strip_suffix(".md").map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let topics: BTreeSet<String> = library::topic_keys().into_iter().map(String::from).collect();
    assert_eq!(
        on_disk("topics"),
        topics,
        "guides/topics and the TOPICS list in knowledge/library.rs name the same guides"
    );
    let effects: BTreeSet<String> = EffectKind::ALL
        .iter()
        .filter(|k| library::effect_source(**k).is_some())
        .map(reference::key_of)
        .collect();
    assert_eq!(
        on_disk("effects"),
        effects,
        "guides/effects and knowledge/library.rs name the same guides (a guide is named for its effect's kind, like colorWash.md)"
    );
}

#[test]
fn the_guide_tool_reaches_every_guide_and_teaches_the_names_when_one_is_wrong() {
    let tool = Toolbox::new()
        .find("get_guide")
        .expect("a guide tool")
        .spec
        .clone();
    let listed = |field: &str| -> Vec<String> {
        tool.input_schema["properties"][field]["items"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(listed("effects"), reference::kind_keys());
    assert_eq!(listed("topics"), library::topic_keys());

    // With nothing asked for: every guide in a line.
    let index = guide(json!({}));
    for key in reference::kind_keys() {
        assert!(index.contains(&format!("- `{key}`: ")), "{key} is in the index");
    }
    for key in library::topic_keys() {
        assert!(index.contains(&format!("- `{key}`: ")), "{key} is in the index");
    }
    assert!(index.len() < 12_000, "the index is {} bytes", index.len());

    // Every effect and topic answers, alone and several at once.
    for key in reference::kind_keys() {
        let text = guide(json!({ "effects": [key] }));
        assert!(text.contains("## Reference"), "{key}");
    }
    for key in library::topic_keys() {
        assert!(guide(json!({ "topics": [key] })).starts_with("# "), "{key}");
    }
    let several = guide(json!({ "effects": ["bars", "spiral"], "topics": ["blends"] }));
    assert_eq!(several.matches("\n\n---\n\n").count(), 2, "{several}");
    assert!(several.contains("Bars (`bars`)") && several.contains("Spiral (`spiral`)"));

    // A wrong name is answered with the right ones.
    let mut draft = draft();
    let (wrong, is_error) = call(&mut draft, "get_guide", json!({ "effects": ["sparkle"] }));
    assert!(
        is_error && wrong.contains("no effect \"sparkle\"") && wrong.contains("twinkle"),
        "{wrong}"
    );
    let (wrong, is_error) = call(&mut draft, "get_guide", json!({ "topics": ["colour"] }));
    assert!(
        is_error && wrong.contains("no topic \"colour\"") && wrong.contains("blends"),
        "{wrong}"
    );
    let (wrong, is_error) = call(&mut draft, "get_guide", json!({ "effect": "bars" }));
    assert!(is_error && wrong.contains("effects and topics"), "{wrong}");
    let many: Vec<String> = reference::kind_keys().into_iter().take(9).collect();
    let (wrong, is_error) = call(&mut draft, "get_guide", json!({ "effects": many }));
    assert!(is_error && wrong.contains("at most 8"), "{wrong}");
    assert!(!draft.has_edits(), "reading a guide changes nothing");
}

#[test]
fn a_long_answer_keeps_whole_guides_and_names_the_ones_left_for_another_call() {
    // Eight of the longest guides can't all fit one answer.
    let mut kinds: Vec<(usize, String)> = EffectKind::ALL
        .iter()
        .map(|k| (knowledge::effect_text(*k).len(), reference::key_of(k)))
        .collect();
    kinds.sort_by(|a, b| b.cmp(a));
    let longest: Vec<String> = kinds.iter().take(8).map(|(_, k)| k.clone()).collect();
    let total: usize = kinds.iter().take(8).map(|(n, _)| n).sum();
    let answer = guide(json!({ "effects": longest }));
    assert!(answer.len() <= pf_ai::run::MAX_RESULT_CHARS, "{}", answer.len());
    assert!(!answer.contains("cut short"));
    if total > pf_ai::run::MAX_RESULT_CHARS {
        assert!(
            answer.contains("Not included"),
            "{}",
            &answer[answer.len() - 200..]
        );
    }
}

#[test]
fn every_topic_is_sound() {
    // Names in backticks are real, generated lists are filled in, looks are real effects, and
    // each topic fits its budget.
    for key in library::topic_keys() {
        let problems = check::topic_problems(key, library::topic_source(key).unwrap());
        assert!(problems.is_empty(), "{}", problems.join("\n\n"));
    }
    // The check does catch a name that isn't one.
    let problems = check::topic_problems("blends", "# Blends\n\nA sentence.\n\nUse `screen` or `add`.\n");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("`screen`") && !problems[0].contains("`add`"),
        "{}",
        problems[0]
    );
}

#[test]
fn everything_the_guides_name_can_be_said_to_a_tool() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let pictures = [pf_ai::stage::PICTURE.to_string()];
    let cx = Context {
        tracks: &doc.timing_tracks,
        pictures: Some(&pictures),
    };
    let placed = |spec: Value| effect_spec::parse(&spec, &cx).unwrap_or_else(|e| panic!("{spec}: {e}"));
    // Every kind, every setting at both ends of its range, and every choice.
    for info in pf_sequence::effect_catalog() {
        let kind = reference::key_of(&info.kind);
        placed(json!({ "kind": kind }));
        for setting in &info.settings {
            let values: Vec<Value> = match &setting.value {
                SettingValue::Number { min, max, .. } => vec![json!(min), json!(max)],
                SettingValue::Int { min, max, .. } => vec![json!(min), json!(max)],
                SettingValue::Bool { .. } => vec![json!(true), json!(false)],
                SettingValue::Choice { options, .. } => options.iter().map(|o| json!(o.value)).collect(),
                SettingValue::Face { .. } | SettingValue::Text { .. } => vec![json!("Face")],
                SettingValue::Image { .. } => vec![json!(pf_ai::stage::PICTURE)],
                SettingValue::TimingTrack { .. } => vec![json!("Beats"), Value::Null],
            };
            for value in values {
                placed(json!({ "kind": kind, "settings": { setting.key: value } }));
            }
        }
        // And a curve on each setting its curves can drive.
        for key in reference::curve_keys(info.kind) {
            let range = pf_sequence::Effect::curve_range(info.kind, key).unwrap();
            placed(
                json!({ "kind": kind, "curves": { key: { "shape": "ramp", "from": range.min, "to": range.max } } }),
            );
        }
    }
    // Every blend, render style, turn or flip, and curve shape; every effect-wide setting.
    for blend in Blend::ALL {
        assert_eq!(placed(json!({ "kind": "on", "blend": blend.key() })).blend, blend);
    }
    for style in RenderStyle::ALL {
        let effect = placed(json!({ "kind": "on", "renderStyle": reference::key_of(&style) }));
        assert_eq!(effect.render_style, style);
    }
    for transform in reference::buffer_transform_keys() {
        placed(json!({ "kind": "on", "bufferTransform": transform }));
    }
    for shape in CurveShape::ALL {
        let mut curve = json!({ "shape": reference::key_of(&shape), "from": 0.2, "to": 1 });
        if shape.follows_marks() {
            curve["timingTrack"] = json!("Bars");
        }
        let effect = placed(json!({ "kind": "on", "curves": { "brightness": curve } }));
        assert_eq!(effect.curves["brightness"].shape, shape);
    }
    for key in reference::effect_wide_curve_keys() {
        placed(json!({ "kind": "on", "curves": { key: { "shape": "sine", "from": 0, "to": 1 } } }));
    }
    let listed: BTreeSet<&str> = effect_spec::EFFECT_WIDE.into_iter().collect();
    for key in reference::effect_wide_keys() {
        let name = if key == "palette" { "colors" } else { key.as_str() };
        assert!(listed.contains(name), "place_effects can't set an effect's {key}");
    }

    // The one-effect tool takes the same things by the document's own names.
    let toolbox = Toolbox::new();
    let add = &toolbox.find("sequence_add_effect").unwrap().spec.input_schema;
    let names = |def: &str| -> Vec<String> {
        add["$defs"][def]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{def} is a list of names in sequence_add_effect"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names("Blend"), Blend::ALL.map(|b| b.key().to_string()));
    assert_eq!(
        names("CurveShape"),
        CurveShape::ALL.map(|s| reference::key_of(&s))
    );
    assert_eq!(names("RenderStyle").len(), RenderStyle::ALL.len());
    assert_eq!(names("BufferTransform"), reference::buffer_transform_keys());
    assert_eq!(
        add["$defs"]["EffectParams"]["properties"]["kind"]["enum"],
        json!(reference::kind_keys())
    );
    // Cues, the song's tracks, and timing track kinds.
    let stage_cue = &toolbox.find("stage_cue").unwrap().spec.input_schema;
    assert_eq!(
        stage_cue["properties"]["cues"]["items"]["properties"]["cue"]["enum"],
        json!(pf_ai::cues::CUES)
    );
    let timing = &toolbox.find("add_song_timing").unwrap().spec.input_schema;
    assert_eq!(
        timing["properties"]["tracks"]["items"]["enum"],
        json!(pf_ai::song::TRACK_CHOICES)
    );
    let track = &toolbox
        .find("sequence_add_timing_track")
        .unwrap()
        .spec
        .input_schema;
    assert_eq!(
        track["$defs"]["TimingKind"]["enum"],
        json!(reference::timing_kind_keys())
    );
    let blending = &toolbox
        .find("sequence_set_row_blending")
        .unwrap()
        .spec
        .input_schema;
    let ways = blending["$defs"]["RowBlending"].to_string();
    for way in RowBlending::ALL {
        assert!(
            ways.contains(&format!("\"{}\"", reference::key_of(&way))),
            "{ways}"
        );
    }
}
