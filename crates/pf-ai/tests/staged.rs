//! The staged build: a brief, a checked design, the plan's steps built (some at once) in small
//! conversations of their own, a review and a refine, all into one draft and one proposal. Every
//! test runs with the scripted provider: nothing is sent anywhere.

use pf_ai::agent::{Flow, TurnStats};
use pf_ai::draft::Draft;
use pf_ai::eval::cli::{stand_in_outcomes, stand_in_outcomes_with};
use pf_ai::eval::{self, scenarios, stand_in};
use pf_ai::provider::{
    AssistantTurn, LlmProvider, Message, ModelInfo, ProviderId, StreamEvent, TurnRequest, Usage,
};
use pf_ai::stage::{self, Stage};
use pf_ai::staged::{Plan, StageName, World, plan::longest_worker, stage_line, step_stage};
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{AiError, ApiKey, Cancel, ChatEvent, ChatSession, Toolbox, TurnReply};
use pf_sequence::Sequence;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

const VERSE_1: (u64, u64) = (8_000, 24_000);
const CHORUS_1: (u64, u64) = (24_000, 40_000);

fn session(parallel: usize) -> ChatSession {
    ChatSession::new()
        .with_flow(Flow::Staged)
        .with_parallel_steps(parallel)
        .with_analyzer(Arc::new(|_: &Path, _: &Cancel| Ok(stage::song())))
}

/// `request` answered on `doc` with `provider`, with the events it streamed.
fn ask(
    session: &mut ChatSession,
    provider: &dyn LlmProvider,
    stage: &Stage,
    doc: &Sequence,
    request: &str,
    cancel: &Cancel,
) -> (Result<TurnReply, AiError>, Vec<ChatEvent>) {
    let mut events = Vec::new();
    let reply = session.run_turn(
        provider,
        &fake_key(),
        "scripted",
        request,
        eval::workspace(stage, doc),
        cancel,
        &mut |e| events.push(e),
    );
    (reply, events)
}

fn row(stage: &Stage, doc: &Sequence, name: &str) -> String {
    stage
        .row(doc, stage.named(name).expect("named"))
        .expect("a row")
        .to_string()
}

fn step(id: &str, sections: &[&str], props: &[&str], tools: &[&str], effects: &[&str]) -> Value {
    json!({
        "id": id,
        "title": id,
        "sections": sections,
        "props": props,
        "tools": tools,
        "effects": effects,
        "do": format!("Build {id}."),
    })
}

fn plan(steps: Vec<Value>) -> Value {
    json!({
        "summary": "Blue verses and a red chorus.",
        "sections": [
            { "label": "Verse", "palette": ["#2a6bff"], "props": ["Mega Tree"], "base": "a blue twinkle" },
            { "label": "Chorus", "palette": ["#ff2a2a"], "props": ["Matrix"], "base": "red" },
        ],
        "roles": [{ "prop": "Mega Tree", "role": "centerpiece" }],
        "steps": steps,
    })
}

fn place(rows: Vec<String>, (from, to): (u64, u64), kind: &str, color: &str) -> (&'static str, Value) {
    (
        "place_effects",
        json!({ "rowIds": rows, "fromMs": from, "toMs": to, "effect": { "kind": kind, "colors": [color] } }),
    )
}

fn plan_build() -> Result<AssistantTurn, AiError> {
    calls(
        "",
        &[(
            "plan_build",
            json!({ "ask": "Blue verses and a red first chorus." }),
        )],
    )
}

/// Every effect in `doc`, without its id (which is new each time one is placed).
fn effects(doc: &Sequence) -> Vec<String> {
    let mut out: Vec<String> = eval::effects(doc)
        .map(|(target, layer, e)| {
            let mut effect = serde_json::to_value(e).unwrap();
            effect.as_object_mut().unwrap().remove("id");
            format!("{target:?}/{layer}/{effect}")
        })
        .collect();
    out.sort();
    out
}

fn draft_doc(session: &ChatSession) -> Sequence {
    session
        .draft()
        .and_then(Draft::sequence)
        .cloned()
        .expect("a draft sequence")
}

#[test]
fn a_plan_that_names_what_is_not_there_goes_back_with_every_problem_then_is_built() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let bad = plan(vec![json!({
        "id": "a",
        "title": "a",
        "sections": ["Chorus 9"],
        "props": ["Garage"],
        "tools": ["propose_changes"],
        "effects": ["sparkleStorm"],
        "do": "x",
        "after": ["nope"],
    })]);
    let good = plan(vec![step(
        "a",
        &["Chorus"],
        &["Matrix"],
        &["place_effects"],
        &["on"],
    )]);
    let provider = ScriptedProvider::new(vec![plan_build()])
        .stage(
            "design",
            vec![
                calls("", &[("submit_plan", bad)]),
                calls("", &[("submit_plan", good)]),
            ],
        )
        .stage(
            &step_stage("a"),
            vec![calls(
                "",
                &[place(
                    vec![row(&stage, &doc, "Matrix")],
                    CHORUS_1,
                    "on",
                    "#ff2a2a",
                )],
            )],
        )
        .stage("refine", vec![says("Nothing to fix.")]);
    let mut chat = session(4);
    let (reply, _) = ask(&mut chat, &provider, &stage, &doc, "Make a show.", &Cancel::new());
    let reply = reply.expect("built");
    assert!(reply.proposal.is_some(), "{reply:?}");
    assert_eq!(reply.text, "Blue verses and a red chorus.");

    let design = provider.seen_in("design");
    assert_eq!(design.len(), 2);
    let Some(Message::ToolResults(results)) = design[1].messages.last() else {
        panic!("the second design request carries the first plan's problems");
    };
    assert!(results[0].is_error);
    for named in ["Chorus 9", "Garage", "propose_changes", "sparkleStorm", "nope"] {
        assert!(
            results[0].content.contains(named),
            "{named}: {}",
            results[0].content
        );
    }
    // The design sees the catalogue and only submit_plan: no tool's full definition.
    assert_eq!(design[0].tools, ["submit_plan"]);
    assert!(design[0].system.contains("`place_effects`"));
    assert!(design[0].system.contains("`twinkle`"));
    assert!(!design[0].system.contains("\"rowIds\""));
    let stats: TurnStats = chat.last_turn();
    assert!(stats.staged);
    // The chat's request, two designs, the step, the refine (none of it at the same time).
    assert_eq!((stats.requests, stats.critical_path), (5, 5));
    assert!(
        draft_doc(&chat)
            .rows
            .iter()
            .any(|r| !r.layers.iter().all(|l| l.effects.is_empty()))
    );
}

#[test]
fn each_step_sees_only_its_tools_its_stretch_of_song_its_rows_and_its_effects_guides() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let steps = vec![
        step(
            "verses",
            &["Verse"],
            &["Mega Tree"],
            &["place_effects"],
            &["twinkle"],
        ),
        step("cues", &["Chorus 1"], &["Matrix"], &["stage_cue"], &[]),
    ];
    let provider = ScriptedProvider::new(vec![plan_build()])
        .stage("design", vec![calls("", &[("submit_plan", plan(steps))])])
        .stage(
            &step_stage("verses"),
            vec![calls(
                "",
                &[place(
                    vec![row(&stage, &doc, "Mega Tree")],
                    VERSE_1,
                    "twinkle",
                    "#2a6bff",
                )],
            )],
        )
        .stage(
            &step_stage("cues"),
            vec![calls(
                "",
                &[(
                    "stage_cue",
                    json!({ "cues": [{ "cue": "hit", "at": 24_000, "targets": ["Matrix"] }] }),
                )],
            )],
        )
        .stage("refine", vec![says("Nothing to fix.")]);
    let mut chat = session(4);
    let (reply, _) = ask(&mut chat, &provider, &stage, &doc, "Make a show.", &Cancel::new());
    reply.expect("built");

    let verses = provider.seen_in(&step_stage("verses"));
    let cues = provider.seen_in(&step_stage("cues"));
    assert_eq!(verses.len(), 1, "a step whose calls all worked asks once");
    assert_eq!(verses[0].tools, ["place_effects"]);
    assert_eq!(cues[0].tools, ["stage_cue"]);
    // Every step's fixed instructions are the same, so a provider's cache keeps them.
    assert_eq!(verses[0].system, cues[0].system);
    assert!(
        !verses[0].system.contains("# Effects"),
        "the catalogue is the design's alone"
    );
    let Message::User(first) = &verses[0].messages[0] else {
        panic!("a step starts with its brief");
    };
    assert!(first.starts_with(&stage_line(&step_stage("verses"))));
    // Its stretch of the song: the verses' parts only.
    let song = first
        .lines()
        .find_map(|l| l.strip_prefix("The song where this step works: "))
        .expect("the song's slice");
    let song: Value = serde_json::from_str(song).unwrap();
    let labels: Vec<&str> = song["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert!(
        !labels.is_empty() && labels.iter().all(|l| l.starts_with("Verse")),
        "{labels:?}"
    );
    // Its rows: the tree's, not the matrix's.
    let rows = first
        .lines()
        .find(|l| l.starts_with("Its rows"))
        .expect("its rows");
    assert!(rows.contains("Mega Tree") && !rows.contains("Matrix"), "{rows}");
    // Its effect's guide, and no other.
    let logs = chat.stages();
    let verses_log = logs.iter().find(|s| s.stage == step_stage("verses")).unwrap();
    assert_eq!(verses_log.given, ["effect:twinkle"]);
    assert!(first.contains("# Twinkle") || first.contains("Twinkle"));
    // The two steps light different props at different times: built at once.
    assert_eq!(chat.last_turn().critical_path, 1 + 1 + 1 + 1);
    assert_eq!(chat.last_turn().requests, 5);
}

#[test]
fn steps_built_at_once_make_the_same_draft_as_steps_built_one_after_another() {
    let scenario = scenarios().into_iter().find(|s| s.key == "whole-song").unwrap();
    let mut drafts = Vec::new();
    let stage = Stage::new();
    for parallel in [1, 4, 4, 2] {
        let provider = stand_in::provider(&scenario, &stage, Flow::Staged);
        let doc = stage.sequence();
        let mut chat = session(parallel);
        let (reply, _) = ask(
            &mut chat,
            &provider,
            &stage,
            &doc,
            scenario.request,
            &Cancel::new(),
        );
        assert!(reply.expect("built").proposal.is_some());
        let built = draft_doc(&chat);
        let tracks: Vec<String> = built.timing_tracks.iter().map(|t| t.name.clone()).collect();
        drafts.push((effects(&built), tracks));
    }
    assert!(!drafts[0].0.is_empty());
    for other in &drafts[1..] {
        assert_eq!(&drafts[0], other);
    }
}

/// A provider that presses Stop as soon as a step of the build asks, as a person would while
/// the steps are running.
struct StopsDuringBuild {
    inner: ScriptedProvider,
    cancel: Cancel,
}

impl LlmProvider for StopsDuringBuild {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn list_models(&self, key: &ApiKey, cancel: &Cancel) -> Result<Vec<ModelInfo>, AiError> {
        self.inner.list_models(key, cancel)
    }

    fn stream_turn(
        &self,
        key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantTurn, AiError> {
        self.stream_turn_with_usage(key, request, cancel, on_event)
            .map(|(turn, _)| turn)
    }

    fn stream_turn_with_usage(
        &self,
        key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<(AssistantTurn, Option<Usage>), AiError> {
        let building = request
            .messages
            .iter()
            .any(|m| matches!(m, Message::User(text) if text.starts_with(&stage_line("build step"))));
        if building {
            self.cancel.cancel();
        }
        self.inner.stream_turn_with_usage(key, request, cancel, on_event)
    }
}

#[test]
fn stop_cancels_the_running_steps_and_nothing_of_them_goes_in() {
    let scenario = scenarios().into_iter().find(|s| s.key == "whole-song").unwrap();
    let stage = Stage::new();
    let doc = stage.sequence();
    let cancel = Cancel::new();
    let provider = StopsDuringBuild {
        inner: stand_in::provider(&scenario, &stage, Flow::Staged),
        cancel: cancel.clone(),
    };
    let mut chat = session(4);
    let (reply, _) = ask(&mut chat, &provider, &stage, &doc, scenario.request, &cancel);
    assert!(matches!(reply, Err(AiError::Cancelled)), "{reply:?}");
    // No step's effects are in the draft, and nothing after the build was asked.
    assert!(eval::effects(&draft_doc(&chat)).next().is_none());
    assert!(provider.inner.seen_in("refine").is_empty());
    // The chat stays well formed: plan_build has its answer, and the next message goes through.
    let Some(Message::ToolResults(results)) = chat.messages().last() else {
        panic!("plan_build is answered");
    };
    assert!(results[0].content.contains("stopped"), "{}", results[0].content);
    let next = ScriptedProvider::new(vec![says("Stopped where you asked.")]);
    let (reply, _) = ask(&mut chat, &next, &stage, &doc, "Thanks.", &Cancel::new());
    assert_eq!(reply.unwrap().text, "Stopped where you asked.");
}

#[test]
fn a_step_that_fails_is_reported_and_the_rest_is_proposed_without_it() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let steps = vec![
        step(
            "verses",
            &["Verse"],
            &["Mega Tree"],
            &["place_effects"],
            &["twinkle"],
        ),
        step("chorus", &["Chorus 1"], &["Matrix"], &["place_effects"], &["on"]),
    ];
    let failure = AiError::Provider {
        provider: ProviderId::Anthropic,
        message: "the service had a problem".into(),
    };
    let provider = ScriptedProvider::new(vec![plan_build()])
        .stage("design", vec![calls("", &[("submit_plan", plan(steps))])])
        .stage(
            &step_stage("verses"),
            vec![calls(
                "",
                &[place(
                    vec![row(&stage, &doc, "Mega Tree")],
                    VERSE_1,
                    "twinkle",
                    "#2a6bff",
                )],
            )],
        )
        .stage(&step_stage("chorus"), vec![Err(failure)])
        .stage("refine", vec![says("Nothing to fix.")]);
    let mut chat = session(4);
    let (reply, _) = ask(&mut chat, &provider, &stage, &doc, "Make a show.", &Cancel::new());
    let reply = reply.expect("the rest is built");
    assert!(reply.proposal.is_some());
    assert!(reply.text.contains("couldn't be built"), "{}", reply.text);
    let built = draft_doc(&chat);
    let on_matrix = eval::effects(&built)
        .filter(|(target, _, _)| Some(*target) == stage.named("Matrix"))
        .count();
    let on_tree = eval::effects(&built)
        .filter(|(target, _, _)| Some(*target) == stage.named("Mega Tree"))
        .count();
    assert_eq!((on_tree > 0, on_matrix), (true, 0));
}

#[test]
fn the_review_of_the_whole_draft_goes_to_a_refine_with_only_the_tools_its_fixes_name() {
    let stage = Stage::new();
    let doc = stage.sequence();
    // One effect on one prop: the review has plenty to say.
    let steps = vec![step(
        "chorus",
        &["Chorus 1"],
        &["Matrix"],
        &["place_effects"],
        &["on"],
    )];
    let provider = ScriptedProvider::new(vec![plan_build()])
        .stage("design", vec![calls("", &[("submit_plan", plan(steps))])])
        .stage(
            &step_stage("chorus"),
            vec![calls(
                "",
                &[place(
                    vec![row(&stage, &doc, "Matrix")],
                    CHORUS_1,
                    "on",
                    "#ff2a2a",
                )],
            )],
        )
        .stage("refine", vec![says("I'll leave it as planned.")]);
    let mut chat = session(4);
    let (reply, events) = ask(&mut chat, &provider, &stage, &doc, "Make a show.", &Cancel::new());
    let reply = reply.expect("built");
    let refine = provider.seen_in("refine");
    assert_eq!(refine.len(), 1);
    let Message::User(first) = &refine[0].messages[0] else {
        panic!("the refine starts with the review");
    };
    assert!(first.contains("The review of the whole draft"));
    assert!(first.contains("The plan in brief"));
    assert!(!refine[0].tools.is_empty());
    for tool in &refine[0].tools {
        assert!(
            !["review_draft", "propose_changes", "plan_build", "reset_draft"].contains(&tool.as_str()),
            "{tool}"
        );
    }
    // The proposal carries the review, and the panel heard every stage in order.
    assert!(reply.proposal.and_then(|p| p.review).is_some());
    let stages: Vec<StageName> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::Stage { stage, .. } => Some(*stage),
            _ => None,
        })
        .collect();
    let mut order = stages.clone();
    order.dedup();
    assert_eq!(
        order,
        [
            StageName::Understanding,
            StageName::Designing,
            StageName::Building,
            StageName::Reviewing,
            StageName::Refining
        ]
    );
}

#[test]
fn a_small_job_stays_in_the_chat_and_the_single_flow_never_offers_plan_build() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let small = || {
        ScriptedProvider::new(vec![
            calls(
                "",
                &[place(
                    vec![row(&stage, &doc, "Matrix")],
                    CHORUS_1,
                    "on",
                    "#ff2a2a",
                )],
            ),
            calls(
                "",
                &[(
                    "propose_changes",
                    json!({ "summary": "The matrix red in the chorus." }),
                )],
            ),
            says("Done."),
        ])
    };
    let provider = small();
    let mut chat = session(4);
    let (reply, _) = ask(
        &mut chat,
        &provider,
        &stage,
        &doc,
        "Make the matrix red in the first chorus.",
        &Cancel::new(),
    );
    assert!(reply.unwrap().proposal.is_some());
    assert!(provider.seen().iter().all(|s| !s.opening().starts_with("Stage:")));
    assert!(provider.tools_offered().contains(&"plan_build".to_string()));
    assert!(provider.seen()[0].system.contains("plan_build"));
    assert!(pf_ai::agent::STAGED_ROUTE.len() <= pf_ai::agent::STAGED_ROUTE_BUDGET_BYTES);
    assert!(!chat.last_turn().staged);
    assert_eq!(chat.last_turn().critical_path, 3);

    let provider = small();
    let mut single = ChatSession::new().with_analyzer(Arc::new(|_: &Path, _: &Cancel| Ok(stage::song())));
    ask(
        &mut single,
        &provider,
        &stage,
        &doc,
        "Make the matrix red.",
        &Cancel::new(),
    )
    .0
    .unwrap();
    assert!(!provider.tools_offered().contains(&"plan_build".to_string()));
    assert_eq!(provider.seen()[0].system, pf_ai::agent::SYSTEM_PROMPT);
}

#[test]
fn with_no_plan_that_fits_the_chat_does_the_job_itself() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let bad = plan(vec![step(
        "a",
        &["Chorus 9"],
        &["Matrix"],
        &["place_effects"],
        &[],
    )]);
    let provider = ScriptedProvider::new(vec![plan_build(), says("I'll build it here instead.")]).stage(
        "design",
        vec![
            calls("", &[("submit_plan", bad.clone())]),
            calls("", &[("submit_plan", bad.clone())]),
            calls("", &[("submit_plan", bad)]),
        ],
    );
    let mut chat = session(4);
    let (reply, _) = ask(&mut chat, &provider, &stage, &doc, "Make a show.", &Cancel::new());
    assert_eq!(reply.unwrap().text, "I'll build it here instead.");
    let Some(Message::ToolResults(results)) = chat.messages().iter().rev().nth(1) else {
        panic!("plan_build is answered before the chat carries on");
    };
    assert!(results[0].is_error && results[0].content.starts_with("Not built in stages"));
    assert_eq!(provider.seen_in("design").len(), 3);
}

#[test]
fn steps_wait_only_for_earlier_steps_that_light_the_same_props_at_the_same_time() {
    let stage = Stage::new();
    let doc = stage.sequence();
    let draft = Draft::new(eval::workspace(&stage, &doc));
    let song = stage::song();
    let world = World::of(&draft, Some(&song), &Toolbox::default());
    let input = plan(vec![
        step("verses", &["Verse"], &["Mega Tree"], &["place_effects"], &[]),
        step("choruses", &["Chorus"], &["Mega Tree"], &["place_effects"], &[]),
        step("matrix", &["Verse 1"], &["Matrix"], &["place_effects"], &[]),
        // Nothing in common with the verses, but it says to wait for them.
        json!({ "id": "outro", "title": "outro", "sections": ["Outro"], "props": ["Matrix"], "tools": ["place_effects"], "do": "x", "after": ["verses"] }),
        step("moments", &["Whole song"], &["all"], &["stage_cue"], &[]),
    ]);
    let plan = Plan::read(&input, &world).expect("a plan that fits");
    assert_eq!(plan.waves(), vec![vec![0, 1, 2], vec![3], vec![4]]);
    assert_eq!(plan.steps[2].spans, vec![VERSE_1]);
    // Four at once take one request's wait; five on two workers, the longest worker's three.
    assert_eq!(longest_worker(&[1, 1, 1, 1], 4), 1);
    assert_eq!(longest_worker(&[1, 1, 1, 1, 1], 2), 3);
}

#[test]
fn the_stand_ins_pass_every_check_in_stages_with_a_shorter_wait_for_the_whole_song() {
    let all = scenarios();
    let staged = stand_in_outcomes_with(&all, Flow::Staged);
    let single = stand_in_outcomes(&all);
    for outcome in &staged {
        let failed: Vec<&str> = outcome
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.what.as_str())
            .collect();
        assert!(failed.is_empty(), "{}: {failed:?}", outcome.scenario);
    }
    let whole = |outcomes: &[eval::Outcome]| {
        outcomes
            .iter()
            .find(|o| o.scenario == "whole-song")
            .map(|o| o.measures.clone())
            .unwrap()
    };
    let (staged_whole, single_whole) = (whole(&staged), whole(&single));
    assert!(staged_whole.staged && !single_whole.staged);
    assert!(staged_whole.critical_path < single_whole.critical_path);
    assert!(staged_whole.traffic.mean_request_tokens() < single_whole.traffic.mean_request_tokens());
    // The transcript has every stage's calls, and the guides the stages were given.
    let outcome = staged.iter().find(|o| o.scenario == "whole-song").unwrap();
    let names: BTreeSet<&str> = outcome.transcript.iter().map(|t| t.name.as_str()).collect();
    for name in [
        "plan_build",
        "submit_plan",
        "place_effects",
        "stage_cue",
        "get_guide",
    ] {
        assert!(names.contains(name), "{name}");
    }
    assert!(outcome.measures.guides.contains(&"method".to_string()));
}
