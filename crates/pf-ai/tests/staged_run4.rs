//! Run 4 of the evaluation, replayed: every call the model made in the two staged builds that
//! failed a check (`tests/fixtures/run4`), each stage's requests as it made them. What a step
//! would do when it is told what it still has to build is added after the model's own turns,
//! from the plan's own words for that step: run 4 never asked for it, so those turns only run when
//! the build asks a step again. Nothing is sent anywhere.

use pf_ai::agent::Flow;
use pf_ai::eval::{self, Outcome, scenarios};
use pf_ai::provider::{AssistantTurn, Message};
use pf_ai::stage::Stage;
use pf_ai::staged::step_stage;
use pf_ai::testing::{ScriptedProvider, calls, fake_key};
use pf_ai::{AiError, Cancel};
use serde_json::{Value, json};

type Turn = Result<AssistantTurn, AiError>;

/// `value` with every "@<name>" made the id of the row that lights `name` on `stage`'s
/// sequence, and every "@prop:<name>" the id of that prop.
fn on_stage(value: &Value, stage: &Stage, doc: &pf_sequence::Sequence) -> Value {
    match value {
        Value::String(text) => {
            if let Some(name) = text.strip_prefix("@prop:") {
                let prop = stage.show.props.iter().find(|p| p.name == name).expect("a prop");
                return json!(prop.id.to_string());
            }
            match text.strip_prefix('@') {
                Some(name) => {
                    let target = stage.named(name).unwrap_or_else(|| panic!("no {name}"));
                    json!(stage.row(doc, target).expect("a row").to_string())
                }
                None => value.clone(),
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| on_stage(v, stage, doc)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), on_stage(v, stage, doc)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn turn(calls_made: &[(String, Value)]) -> Turn {
    let named: Vec<(&str, Value)> = calls_made.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
    calls("", &named)
}

/// The run's turns for `scenario`, by stage, with `more` added after a stage's own.
fn replay(scenario: &str, stage: &Stage, more: &[(&str, Vec<Value>)]) -> ScriptedProvider {
    let path = format!(
        "{}/tests/fixtures/run4/{scenario}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let start = scenarios()
        .into_iter()
        .find(|s| s.key == scenario)
        .unwrap()
        .opening(stage);
    let turns_of = |turns: &Value| -> Vec<Turn> {
        turns
            .as_array()
            .unwrap()
            .iter()
            .map(|calls_made| {
                let calls_made: Vec<(String, Value)> = calls_made
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        (
                            c["name"].as_str().unwrap().to_string(),
                            on_stage(&c["input"], stage, &start),
                        )
                    })
                    .collect();
                turn(&calls_made)
            })
            .collect()
    };
    let mut provider = None;
    for s in fixture["stages"].as_array().unwrap() {
        let name = s["stage"].as_str().unwrap();
        let mut turns = turns_of(&s["turns"]);
        for (stage_name, extra) in more {
            if *stage_name == name {
                for calls_made in extra {
                    turns.extend(turns_of(&json!([calls_made])));
                }
            }
        }
        provider = Some(match provider {
            None => {
                assert_eq!(name, "chat");
                ScriptedProvider::new(turns)
            }
            Some(p) => ScriptedProvider::stage(p, name, turns),
        });
    }
    provider.unwrap()
}

fn run(scenario: &str, provider: &ScriptedProvider, stage: &Stage) -> Outcome {
    let scenario = scenarios().into_iter().find(|s| s.key == scenario).unwrap();
    eval::run_with(
        &scenario,
        stage,
        provider,
        &fake_key(),
        "run 4, replayed",
        &Cancel::new(),
        None,
        Flow::Staged,
    )
}

fn check<'a>(outcome: &'a Outcome, what: &str) -> &'a eval::Check {
    outcome
        .checks
        .iter()
        .find(|c| c.what.starts_with(what))
        .unwrap_or_else(|| panic!("no check {what}"))
}

/// What a request of `stage` was told last.
fn told(provider: &ScriptedProvider, stage: &str, request: usize) -> String {
    let seen = provider.seen_in(stage);
    let Some(asked) = seen.get(request) else {
        panic!("{stage} made only {} requests", seen.len());
    };
    match asked.messages.last() {
        Some(Message::User(text)) => text.clone(),
        other => panic!("{stage} request {request} ends with {other:?}"),
    }
}

/// The plan's "choruses" step: "Add the missing final foundation with the same look"; its
/// "verses": held bases and slow movement; its "vocals": readable hooks in every chorus and the
/// face singing every sung stretch.
fn review_and_fix_more() -> Vec<(&'static str, Vec<Value>)> {
    let on = |rows: &[&str], from: u64, to: u64, layer: u64, effect: Value| {
        let rows: Vec<String> = rows.iter().map(|r| format!("@{r}")).collect();
        json!({ "rowIds": rows, "fromMs": from, "toMs": to, "layer": layer, "effect": effect })
    };
    let warm = json!(["#ff4000", "#ffbf00", "#ffc880"]);
    let verse = json!(["#ff00bf", "#ff4000", "#ffbf00"]);
    let mut verse_items = Vec::new();
    for (from, to) in [(8_000, 24_000), (40_000, 56_000)] {
        verse_items.push(on(
            &["Mega Tree"],
            from,
            to,
            0,
            json!({ "kind": "on", "colors": verse, "brightness": 0.3 }),
        ));
        verse_items.push(on(&["Arches"], from, to, 1, json!({ "kind": "chase", "colors": ["#ff4000", "#ffbf00"], "brightness": 0.4, "settings": { "order": "across" } })));
        verse_items.push(on(
            &["Windows"],
            from,
            to,
            0,
            json!({ "kind": "on", "colors": ["#ffbf00"], "brightness": 0.25 }),
        ));
        verse_items.push(on(
            &["Candy Canes"],
            from,
            to,
            0,
            json!({ "kind": "on", "colors": ["#ff4000"], "brightness": 0.2 }),
        ));
    }
    let hooks: Vec<Value> = [(24_000, 27_500), (56_000, 59_500), (80_000, 83_500)]
        .into_iter()
        .map(|(from, to)| {
            on(&["Matrix"], from, to, 0, json!({ "kind": "text", "colors": ["#ffbf00"], "brightness": 0.55, "settings": { "text": "LIGHT IT UP", "size": 8, "movement": "none" } }))
        })
        .collect();
    let mut vocal_items = hooks;
    vocal_items.push(on(&["Singing Face"], 8_000, 72_000, 0, json!({ "kind": "faces", "colors": ["#ffbf00"], "brightness": 0.5, "settings": { "timingTrack": "Lyrics (phonemes)" } })));
    vocal_items.push(on(&["Singing Face"], 80_000, 92_000, 0, json!({ "kind": "faces", "colors": ["#ffbf00"], "brightness": 0.55, "settings": { "timingTrack": "Lyrics (phonemes)" } })));
    vec![
        (
            "build step \"verses\"",
            vec![json!([{ "name": "place_effects", "input": { "items": verse_items } }])],
        ),
        (
            "build step \"choruses\"",
            vec![
                json!([{ "name": "place_effects", "input": on(&["Whole House"], 80_000, 92_000, 0, json!({ "kind": "on", "colors": warm, "brightness": 0.25, "settings": { "gradient": "horizontal" } })) }]),
            ],
        ),
        (
            "build step \"vocals\"",
            vec![json!([{ "name": "place_effects", "input": { "items": vocal_items } }])],
        ),
    ]
}

fn failed(outcome: &Outcome) -> Vec<&str> {
    outcome
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.what.as_str())
        .collect()
}

#[test]
fn review_and_fix_the_last_chorus_is_built_because_the_chorus_step_is_asked_again() {
    let stage = Stage::new();
    let provider = replay("review-and-fix", &stage, &review_and_fix_more());
    let outcome = run("review-and-fix", &provider, &stage);
    // Run 4: the step changed two effects' brightness and colors, which ended it, and Chorus 3
    // was never placed. Now it is told what its part of the plan still lacks, where.
    let again = told(&provider, &step_stage("choruses"), 2);
    assert!(again.contains("Chorus 3 (80000–92000 ms)"), "{again}");
    let dark = check(&outcome, "The last chorus is no longer dark");
    assert!(dark.passed, "{dark:?}");
    // The verses step's new layer ended it too, with nothing placed; and the vocals step's
    // removing the old words ended it before the new ones went in. Both are asked again.
    let verses = told(&provider, &step_stage("verses"), 3);
    assert!(verses.contains("Verse 1 (8000–24000 ms)"), "{verses}");
    let vocals = told(&provider, &step_stage("vocals"), 2);
    assert!(vocals.contains("place it now"), "{vocals}");
    assert!(failed(&outcome).is_empty(), "{:?}", failed(&outcome));
    assert!(!provider.seen_in("refine").is_empty());
}

#[test]
fn calm_and_big_the_second_chorus_is_built_because_the_chorus_step_is_asked_again() {
    let stage = Stage::new();
    // The plan: "Repeat chorus 1 into chorus 2". Run 4's step placed choruses 1 and 3, which
    // ended it before the repeat.
    let more = vec![(
        "build step \"choruses\"",
        vec![
            json!([{ "name": "repeat_effects", "input": { "fromMs": 24_000, "toMs": 40_000, "startsMs": [56_000] } }]),
        ],
    )];
    let provider = replay("calm-and-big", &stage, &more);
    let outcome = run("calm-and-big", &provider, &stage);
    let again = told(&provider, &step_stage("choruses"), 2);
    assert!(again.contains("Chorus 2 (56000–72000 ms)"), "{again}");
    let house = check(&outcome, "The choruses use the house");
    assert!(house.passed, "{house:?}");
    assert!(failed(&outcome).is_empty(), "{:?}", failed(&outcome));
}
