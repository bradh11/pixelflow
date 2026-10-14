//! The Assistant's knowledge stays complete as PixelFlow grows: every effect kind, setting,
//! choice, blend, curve shape, render style, cue, and timing track is in a guide the Assistant
//! can fetch, every name a guide uses is a real one, and the guides fit their budgets. When one
//! of these fails, its message says what to write and where.

use pf_ai::effect_spec::{self, Context};
use pf_ai::knowledge::{
    self, MAX_GUIDE_ANSWER_BYTES, MAX_GUIDE_BYTES, MAX_GUIDES_PER_CALL, PARTS, check, library, reference,
};
use pf_ai::provider::{Message, ToolCall};
use pf_ai::run::{Outcome, run_tool};
use pf_ai::stage::Stage;
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{Cancel, ChatSession, Draft, OpenDoc, Toolbox, UiContext, Workspace};
use pf_engine::Engine;
use pf_model::{GroupLayout, RenderStyle};
use pf_render::{AudioSource, Pictures};
use pf_sequence::{Blend, CurveShape, EffectKind, MusicLevelSource, RowBlending, SettingValue};
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
        shaders: pf_render::built_in_shaders()
            .iter()
            .map(|file| pf_render::Shaders::none().describe(file))
            .collect(),
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
        "music-reactive",
        "channels the music's level can come from",
        MusicLevelSource::ALL.iter().map(reference::key_of),
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
    // Every effect kind has its guide (the build itself insists: see `effect_source`), and
    // nothing else is in the folder.
    let effects: BTreeSet<String> = reference::kind_keys().into_iter().collect();
    assert_eq!(
        on_disk("effects"),
        effects,
        "guides/effects holds one guide for each effect kind, named for its key (like colorWash.md)"
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
    assert!(
        is_error && wrong.contains("effects, topics, and parts"),
        "{wrong}"
    );
    let (wrong, is_error) = call(
        &mut draft,
        "get_guide",
        json!({ "effects": ["bars"], "parts": ["recipes"] }),
    );
    assert!(
        is_error && wrong.contains("no part \"recipes\"") && wrong.contains("looks"),
        "{wrong}"
    );
    // Too many at once: the first twelve come (as many as fit), and the rest are named to ask
    // for next, rather than a refusal that costs a round trip.
    let many: Vec<&str> = library::topic_keys()
        .into_iter()
        .take(MAX_GUIDES_PER_CALL + 1)
        .collect();
    let (answer, is_error) = call(&mut draft, "get_guide", json!({ "topics": many }));
    assert!(!is_error, "{answer}");
    assert!(
        answer.starts_with("# How to build a sequence"),
        "the first are sent"
    );
    assert!(answer.len() <= MAX_GUIDE_ANSWER_BYTES, "{}", answer.len());
    let last = many.last().unwrap();
    assert!(
        answer.contains(&format!("({MAX_GUIDES_PER_CALL} at a time): "))
            && answer.contains(last)
            && answer.ends_with(". Ask for them next."),
        "{}",
        &answer[answer.len() - 200..]
    );
    let sent = answer.matches("\n\n---\n\n").count();
    assert!((8..=MAX_GUIDES_PER_CALL).contains(&sent), "{sent} guides sent");
    assert!(!draft.has_edits(), "reading a guide changes nothing");
}

#[test]
fn a_guide_read_once_in_a_chat_is_not_sent_again() {
    let mut read = knowledge::GuidesRead::default();
    let first = knowledge::answer_once(&json!({ "topics": ["method", "intensity"] }), &mut read).unwrap();
    assert!(first.starts_with("# How to build a sequence"), "{first}");
    assert!(read.contains("topic:method") && read.contains("topic:intensity"));
    // Asked again with one new guide: the new one comes in full, the others are pointed at.
    let again =
        knowledge::answer_once(&json!({ "topics": ["method", "intensity", "cues"] }), &mut read).unwrap();
    assert!(again.starts_with("# Cues"), "{again}");
    assert!(!again.contains("# How to build a sequence"));
    assert!(
        again.ends_with(
            "Already read in this chat, and unchanged (look back at them above): method, intensity."
        ),
        "{again}"
    );
    // Only what was asked for twice, alone: a line, not thousands of bytes.
    let repeat = knowledge::answer_once(&json!({ "effects": [], "topics": ["method"] }), &mut read).unwrap();
    assert!(repeat.len() < 120, "{repeat}");
    // An effect and a topic of the same name are different guides.
    assert!(!read.contains("effect:method"));
    // A chat does this for the model: the second ask is answered from memory.
    let provider = ScriptedProvider::new(vec![
        calls("", &[("get_guide", json!({ "topics": ["method"] }))]),
        calls("", &[("get_guide", json!({ "topics": ["method", "song"] }))]),
        says("Read."),
    ]);
    let mut session = ChatSession::new();
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::from_engine(&Engine::new(dir.path()), UiContext::default());
    session
        .run_turn(
            &provider,
            &fake_key(),
            "scripted",
            "read up",
            workspace,
            &Cancel::new(),
            &mut |_| {},
        )
        .unwrap();
    let answers: Vec<String> = session
        .messages()
        .iter()
        .filter_map(|m| match m {
            Message::ToolResults(results) => Some(results[0].content.clone()),
            _ => None,
        })
        .collect();
    assert!(answers[0].starts_with("# How to build a sequence"));
    assert!(answers[1].starts_with("# Reading a song"), "{}", answers[1]);
    assert!(!answers[1].contains("# How to build a sequence"));
    assert!(
        answers[1].ends_with("(look back at them above): method."),
        "{}",
        answers[1]
    );
}

#[test]
fn an_effects_guide_comes_in_parts_and_a_part_read_once_isnt_sent_again() {
    let mut read = knowledge::GuidesRead::default();
    let looks =
        knowledge::answer_once(&json!({ "effects": ["chase"], "parts": ["looks"] }), &mut read).unwrap();
    let whole = knowledge::effect_text(EffectKind::Chase);
    // Its title and summary, then only the looks: a fraction of the whole guide.
    assert!(looks.starts_with("# Chase\n\nBands of light"), "{looks}");
    assert!(looks.contains("## Looks\n") && looks.contains("**A white runner along an outline**"));
    assert!(
        !looks.contains("## Settings") && !looks.contains("## Reference"),
        "{looks}"
    );
    assert!(
        looks.len() * 3 < whole.len(),
        "{} of {}",
        looks.len(),
        whole.len()
    );
    // Every part there is, in the guide's order, is the whole guide.
    let all: Vec<&str> = PARTS.iter().map(|(p, _)| *p).collect();
    let pieces = knowledge::effect_parts(EffectKind::Chase, &all);
    for (_, heading) in PARTS {
        assert!(pieces.contains(&format!("\n## {heading}\n")), "{heading}");
    }
    assert_eq!(pieces.trim_end(), whole.trim_end());
    // Asked again with another part: only the new one comes.
    let more = knowledge::answer_once(
        &json!({ "effects": ["chase"], "parts": ["looks", "reference"] }),
        &mut read,
    )
    .unwrap();
    assert!(
        more.contains("## Reference") && !more.contains("## Looks"),
        "{more}"
    );
    let again =
        knowledge::answer_once(&json!({ "effects": ["chase"], "parts": ["looks"] }), &mut read).unwrap();
    assert!(again.starts_with("Already read in this chat"), "{again}");
    // A guide read whole has every part read.
    knowledge::answer_once(&json!({ "effects": ["bars"] }), &mut read).unwrap();
    let parts =
        knowledge::answer_once(&json!({ "effects": ["bars"], "parts": ["settings"] }), &mut read).unwrap();
    assert!(parts.starts_with("Already read in this chat"), "{parts}");
}

#[test]
fn every_effect_is_on_one_page_for_choosing() {
    let page = knowledge::topic_text("choosing").unwrap();
    for key in reference::kind_keys() {
        assert!(page.contains(&format!("- `{key}` (")), "{key}");
    }
    assert!(
        page.contains("- `chase` (string, arch, matrix, tree, house): Bands of light"),
        "{page}"
    );
    assert!(page.len() <= MAX_GUIDE_BYTES, "{}", page.len());
}

#[test]
fn the_range_says_what_every_effect_can_do_from_its_guide() {
    // A line for every effect: the props in its brackets are ones its guide has looks for, and
    // the look it names is one of that guide's. A look renamed or retargeted in its guide, or an
    // effect added without a line, fails here.
    let written = written_topic("range");
    let line = regex::Regex::new(r#"^- `(\w+)` \(([^)]*)\): .*"([^"]+)"\.$"#).unwrap();
    let mut described = BTreeSet::new();
    for text in written.lines() {
        let Some(found) = line.captures(text) else {
            assert!(
                !text.starts_with("- `"),
                "a line of topic `range` not in the form of the others: {text}"
            );
            continue;
        };
        let (key, props, name) = (&found[1], &found[2], &found[3]);
        let kind = reference::kind_named(key).unwrap_or_else(|| panic!("there's no effect `{key}`"));
        let guide = knowledge::effect_guide(kind).unwrap();
        let written_for: BTreeSet<knowledge::Suit> =
            guide.looks.iter().flat_map(|l| l.suits.clone()).collect();
        for prop in props.split(", ") {
            let suit = knowledge::Suit::parse(prop)
                .unwrap_or_else(|| panic!("`{key}`: \"{prop}\" isn't a kind of prop"));
            assert!(
                written_for.contains(&suit),
                "topic `range` puts `{key}` on a {prop}, which none of its guide's looks are written for"
            );
        }
        assert!(
            guide.looks.iter().any(|l| l.name == name),
            "topic `range` names \"{name}\", which `{key}`'s guide has no look called"
        );
        assert!(described.insert(key.to_string()), "`{key}` twice");
    }
    let missing: Vec<String> = reference::kind_keys()
        .into_iter()
        .filter(|key| !described.contains(key))
        .collect();
    assert!(missing.is_empty(), "topic `range` has no line for: {missing:?}");
    // As read: the effects with no look for a run of pixels (the ones the review flags on an
    // outline or an arch), listed from their guides.
    let page = knowledge::topic_text("range").unwrap();
    let flagged: Vec<String> = EffectKind::ALL
        .iter()
        .filter(|&&kind| knowledge::suits_a_run(kind) == Some(false))
        .map(|kind| format!("`{}`", reference::key_of(kind)))
        .collect();
    for key in ["`fan`", "`dancer`", "`picture`", "`text`"] {
        assert!(flagged.iter().any(|k| k == key), "{key}: {flagged:?}");
    }
    assert!(page.contains(&format!("\n{}\n", flagged.join(", "))), "{page}");
    assert!(page.len() <= MAX_GUIDE_BYTES, "{}", page.len());
}

#[test]
fn a_long_answer_keeps_whole_guides_and_names_the_ones_left_for_another_call() {
    // Eight of the longest guides can't all fit one answer.
    let mut kinds: Vec<(usize, String)> = EffectKind::ALL
        .iter()
        .map(|k| (knowledge::effect_text(*k).len(), reference::key_of(k)))
        .collect();
    kinds.sort_by(|a, b| b.cmp(a));
    let longest: Vec<String> = kinds
        .iter()
        .take(MAX_GUIDES_PER_CALL)
        .map(|(_, k)| k.clone())
        .collect();
    let total: usize = kinds.iter().take(MAX_GUIDES_PER_CALL).map(|(n, _)| n).sum();
    let answer = guide(json!({ "effects": longest }));
    assert!(answer.len() <= MAX_GUIDE_ANSWER_BYTES, "{}", answer.len());
    assert!(!answer.contains("cut short"));
    if total > MAX_GUIDE_ANSWER_BYTES {
        assert!(
            answer.contains("Not included"),
            "{}",
            &answer[answer.len() - 200..]
        );
    }
}

#[test]
fn the_guides_are_complete() {
    // Every effect kind has a guide that says what it draws on a string, an arch, a matrix, a
    // tree, and the whole house; names every setting and choice the kind has and none it
    // doesn't; and gives looks that are real effects and light every target they say they suit.
    // Each problem says what to write, and where.
    let problems = check::problems(None, None);
    assert!(
        problems.is_empty(),
        "The Assistant's guides have fallen behind PixelFlow ({} to fix):\n\n{}",
        problems.len(),
        problems.join("\n\n")
    );
}

#[test]
fn a_guide_that_falls_behind_its_effect_is_told_what_to_write() {
    let stage = Stage::new();
    let bars = library::effect_source(EffectKind::Bars);
    assert_eq!(
        check::effect_problems(EffectKind::Bars, Some(bars), &stage),
        Vec::<String>::new()
    );
    let problems = |source: &str| check::effect_problems(EffectKind::Bars, Some(source), &stage).join("\n");

    // No guide at all, or one only begun (a new effect kind, its file holding just a title so
    // the crate builds): everything to write, for this effect, and where.
    let missing = check::effect_problems(EffectKind::Bars, None, &stage).join("\n");
    for words in [
        "The Assistant has no guide for the Bars effect yet",
        "crates/pf-ai/guides/effects/bars.md",
        "`effect_source` in `crates/pf-ai/src/knowledge/library.rs`",
        "--example guide_sheets -- <a scratch folder> --kinds bars --auto",
        "exactly these sections: What it draws, Settings, When to use it, Looks",
        "on a string, an arch, a matrix, a tree, and the whole house",
        "`count`, `speed`, `axis`",
        "what each of these choices looks like or when to pick it: `horizontal`, `vertical`",
        "at least 2 named, ready-to-place looks",
        "--check --kinds bars --guides crates/pf-ai/guides",
        "docs/dev/assistant-guides.md",
    ] {
        assert!(missing.contains(words), "{words}: {missing}");
    }
    assert_eq!(
        problems("# Bars\n"),
        format!(
            "crates/pf-ai/guides/effects/bars.md: a guide needs a sentence under its title saying what it's for.\n{missing}"
        )
    );
    assert_eq!(problems("# Bars\n\nStripes.\n"), missing);
    // An effect without settings is asked for one look, and to say it has none.
    let off = check::effect_problems(EffectKind::Off, None, &stage).join("\n");
    assert!(
        off.contains("It has no settings of its own; say so.") && off.contains("at least 1 named"),
        "{off}"
    );

    // A setting or a choice the effect has gained since the guide was written.
    let without = |name: &str| bars.replace(&format!("`{name}`"), name);
    let said = problems(&without("highlight"));
    assert!(
        said.contains("has settings its guide doesn't mention: `highlight`"),
        "{said}"
    );
    let said = problems(&without("step"));
    assert!(
        said.contains("has choices its guide doesn't mention: `step`"),
        "{said}"
    );
    // A setting the guide names that the effect doesn't have (renamed, or another effect's).
    let said = problems(&bars.replace("- `count`:", "- `count` (with `bands`):"));
    assert!(
        said.contains("names things the Bars effect (and PixelFlow's tools) don't have: `bands`")
            && said.contains("This effect's settings: `count`, `speed`"),
        "{said}"
    );
    // Another kind's setting, written as kind.setting, is fine; a made-up one is not.
    assert_eq!(
        problems(&bars.replace("`twinkle` or", "`twinkle` (its `twinkle.density`) or")),
        ""
    );
    let said = problems(&bars.replace("`twinkle` or", "`twinkle` (its `twinkle.sparkliness`) or"));
    assert!(said.contains("`twinkle.sparkliness`"), "{said}");

    // A look that is no longer a real effect, or isn't this guide's kind.
    let said = problems(&bars.replace("\"count\": 3, \"speed\": 0.3", "\"count\": 300, \"speed\": 0.3"));
    assert!(
        said.contains("the look \"Candy stripe along an outline\" isn't an effect PixelFlow can place: count is 300; use 1 to 100"),
        "{said}"
    );
    let said = problems(&bars.replace(
        "\"kind\": \"bars\", \"settings\": {\"axis\": \"horizontal\", \"count\": 3",
        "\"kind\": \"chase\", \"settings\": {\"speed\": 3",
    ));
    assert!(said.contains("a guide's looks are of its own kind"), "{said}");
    // A look that says it suits a target it lights nothing on.
    let dark = bars.replace(
        "\"colors\": [\"#ff0000\", \"#ffffff\"]}}",
        "\"colors\": [\"#000000\"]}}",
    );
    let said = problems(&dark);
    assert!(
        said.contains("the look \"Candy stripe along an outline\" lights nothing on the string")
            && said.contains("lights nothing on the arch"),
        "{said}"
    );

    // The shape every effect's guide has.
    let said = problems(&bars.replace("## When to use it", "## Uses"));
    assert!(
        said.contains("exactly these, in order: What it draws, Settings, When to use it, Looks"),
        "{said}"
    );
    let said = problems(&bars.replace("# Bars", "# Stripes"));
    assert!(
        said.contains("its title is \"Stripes\"; the effect is called \"Bars\""),
        "{said}"
    );
    let draws = bars.split("## Settings").next().unwrap();
    let no_tree = bars.replace(draws, &draws.replace("tree", "bush"));
    let said = problems(&no_tree);
    assert!(
        said.contains("doesn't say what the Bars effect looks like on: tree"),
        "{said}"
    );
    let said = problems(&format!("{bars}\n{}", "More. ".repeat(2_000)));
    assert!(said.contains("keep it under 12000"), "{said}");
}

#[test]
fn the_instructions_point_at_the_guides_and_the_method_at_every_topic() {
    // The standing instructions carry the method in brief and say where the rest is: the tool,
    // the method's own topic, and the topic for each thing a designer decides.
    let prompt = pf_ai::agent::SYSTEM_PROMPT;
    assert!(
        prompt.contains("get_guide") && prompt.contains("topic method"),
        "{prompt}"
    );
    assert!(prompt.contains("an effect's guide before you use it"));
    let topics = library::topic_keys();
    let named = prompt
        .split_once("about to decide (")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(list, _)| list.split(", ").collect::<Vec<_>>())
        .expect("the instructions list the topics to read");
    assert!(named.len() >= 8, "{named:?}");
    for topic in named {
        assert!(
            topics.contains(&topic),
            "the instructions name a topic \"{topic}\" that isn't one: {topics:?}"
        );
    }
    // The method names every other topic, so reading it says where everything is.
    let method = written_topic("method");
    let missing: Vec<&str> = topics
        .iter()
        .copied()
        .filter(|key| *key != "method" && !method.contains(&format!("`{key}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "The method (crates/pf-ai/guides/topics/method.md) doesn't say when to read these topics: {}. A topic the method never points to is one the Assistant never reads.",
        missing.join(", ")
    );
}

#[test]
fn the_concepts_topic_leaves_every_choice_of_effect_and_palette_to_the_assistant() {
    // Sketching concepts is a way to choose, not a list of choices: the topic names no effect
    // and gives no colors, so whatever a show is made of comes from its own idea.
    let written = written_topic("concepts");
    let kinds = reference::kind_keys();
    let parsed = knowledge::guide::parse(&written).unwrap();
    let named: Vec<&String> = parsed.names.iter().filter(|name| kinds.contains(*name)).collect();
    assert!(named.is_empty(), "topic `concepts` names effects: {named:?}");
    let color = regex::Regex::new("#[0-9a-fA-F]{6}").unwrap();
    assert!(!color.is_match(&written), "topic `concepts` gives colors");
    // The method sends a whole show there before planning, and the topic says when to skip it.
    let method = written_topic("method");
    let (sketch, plan) = (method.find("`concepts`"), method.find("## 4. Plan by section"));
    assert!(sketch.is_some() && plan.is_some() && sketch < plan, "{method}");
    for words in ["no sketches", "`reset_draft`", "`propose_changes`"] {
        assert!(written.contains(words), "{words}");
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
                SettingValue::ShaderFile { .. } => vec![json!(pf_sequence::DEFAULT_SHADER)],
                SettingValue::ShaderInputs { .. } => vec![json!({}), json!({ "speed": 2 })],
                SettingValue::DmxChannels { .. } => vec![json!({}), json!({ "1": 0, "512": 255 })],
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
    let level = &toolbox
        .find("sequence_set_music_level_source")
        .unwrap()
        .spec
        .input_schema;
    let channels = level["$defs"]["MusicLevelSource"].to_string();
    for channel in MusicLevelSource::ALL {
        assert!(
            channels.contains(&format!("\"{}\"", reference::key_of(&channel))),
            "{channels}"
        );
    }
}
