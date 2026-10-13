//! The staged build: a job that builds or reworks a sequence, done as a designer works rather
//! than in one long conversation.
//!
//! 1. **Understand**: the chat's own first request, with the person's words, the show, the
//!    sequence, and the song (as every message has them), ends in `plan_build` with a brief of
//!    what is asked. A question or a small edit never gets here: the model does it in the chat.
//! 2. **Design**: one request that sees the brief, the song, the display, and a catalogue of every
//!    tool and effect in a line each with the show-design guides, but no tool's full definition.
//!    It answers with a terse plan ([`plan::Plan`]), which is checked; one that names a part of
//!    the song, a prop, an effect, or a tool that isn't there, leaves a part to nobody, or keeps
//!    bigger choruses small goes back with the problems.
//! 3. **Build**: each step of the plan in a small conversation of its own: its part of the plan,
//!    the plan in brief, only the stretch of song and the props it touches, the guides of the
//!    effects it uses, and only the full definitions of the tools it names. Steps that don't
//!    touch the same props at the same time run at once ([`PARALLEL_STEPS`] at most); each works
//!    on its own copy of the draft as the earlier waves left it, and what it drafted is added to
//!    the draft in the plan's order, so the result doesn't depend on which finishes first. A step
//!    isn't over because it changed something: it is over when the parts it builds have what the
//!    plan has playing there ([`coverage`]), or when it says it is done; until then it is told
//!    what is still dark, where. A blackout cue that would darken a part the plan lights is
//!    held to its moment.
//! 4. **Review and refine**: the design review of the whole draft, and the plan checked against
//!    it. Under [`REFINED_BELOW`], or with any part of the plan still dark, a refine conversation
//!    gets the plan in brief, what is dark (times, props, rows), and the review, with the tools to
//!    place effects as well as change them; a second one if the first leaves the plan dark. The
//!    chat then proposes the draft as it always does.
//!
//! Each stage's fixed instructions come first in its requests and are the same for every request
//! of that stage, so a provider's cache keeps them.

pub mod catalogue;
pub mod coverage;
pub mod plan;

pub use plan::{Brief, Plan, Step, World};

use crate::agent::{ChatEvent, MAX_OUTPUT_TOKENS};
use crate::diff::target_name;
use crate::draft::Draft;
use crate::error::AiError;
use crate::knowledge::{self, GuidesRead, reference};
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, StopReason, StreamEvent, ToolCall, ToolResult, ToolSpec,
    TurnRequest,
};
use crate::run::{Outcome, run_tool};
use crate::secret::ApiKey;
use crate::song::{Analyzer, Song};
use crate::tools::Toolbox;
use coverage::Expect;
use pf_analysis::Analysis;
use pf_model::PropId;
use pf_sequence::Target;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

/// Steps built at the same time, at most.
pub const PARALLEL_STEPS: usize = 4;
/// Requests one step may make.
pub const MAX_STEP_REQUESTS: usize = 5;
/// Plans the design may send before the job goes back to the chat.
pub const MAX_DESIGN_TRIES: usize = 3;
/// Requests one refine may make.
pub const MAX_REFINE_REQUESTS: usize = 3;
/// Refines, at most: a second when the first leaves part of the plan dark.
pub const MAX_REFINES: usize = 2;
/// A draft the review scores this or more, with every part of the plan lit, isn't refined.
pub const REFINED_BELOW: u32 = 95;
/// Tools every refine has, whatever the review names: it must be able to place what is missing,
/// not only change what is there.
pub const REFINE_TOOLS: [&str; 5] = [
    "list_sequence_effects",
    "place_effects",
    "repeat_effects",
    "change_effects",
    "stage_cue",
];
/// Existing effects listed for a step, at most.
const MAX_EFFECTS_LISTED: usize = 60;
/// Rows listed for a step, at most.
const MAX_ROWS_LISTED: usize = 80;

/// What the first line of each stage's conversation says: it names the stage (and the step).
pub fn stage_line(stage: &str) -> String {
    format!("Stage: {stage}")
}

/// The stage line of a step's conversation.
pub fn step_stage(id: &str) -> String {
    format!("build step {}", serde_json::to_string(id).unwrap_or_default())
}

/// The stages the panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StageName {
    Understanding,
    Designing,
    Building,
    Reviewing,
    Refining,
}

/// One stage's conversation, kept for the evaluation's transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct StageLog {
    /// "design", "build step \"verses\"", "refine".
    pub stage: String,
    pub messages: Vec<Message>,
    /// The guides its request came with ("topic:method", "effect:on").
    pub given: Vec<String>,
}

/// A staged build done.
#[derive(Debug, Clone)]
pub struct Built {
    pub plan: Plan,
    /// Requests the stages made (the chat's own not counted).
    pub requests: usize,
    /// Of those, the most that had to happen one after another: what the person waited through.
    pub critical_path: usize,
    /// What the person should know: steps that failed, changes left out.
    pub notes: Vec<String>,
    pub log: Vec<StageLog>,
}

/// What the stages did before they stopped.
#[derive(Debug, Clone, Default)]
pub struct Partial {
    pub requests: usize,
    pub critical_path: usize,
    pub log: Vec<StageLog>,
}

/// Why a staged build didn't finish.
#[derive(Debug)]
pub enum Failure {
    /// The provider failed, or the person pressed Stop. What earlier waves built stays in the
    /// draft; nothing of a step that was running goes in.
    Ai(AiError, Partial),
    /// No plan that fits came of the design: the job goes back to the chat.
    Design(String, Partial),
}

/// What every stage works with.
pub struct Ctx<'a> {
    pub provider: &'a dyn LlmProvider,
    pub key: &'a ApiKey,
    pub model: &'a str,
    pub toolbox: &'a Toolbox,
    pub analyzer: &'a Analyzer,
    /// The open sequence's music.
    pub music: Option<&'a Path>,
    /// The song's analysis, by file, as the chat has it.
    pub song: Option<(PathBuf, Arc<Analysis>)>,
    pub cancel: &'a Cancel,
    /// Steps built at the same time, at most.
    pub workers: usize,
}

impl Ctx<'_> {
    fn analysis(&self) -> Option<Arc<Analysis>> {
        let (path, analysis) = self.song.as_ref()?;
        (Some(path.as_path()) == self.music).then(|| Arc::clone(analysis))
    }

    fn ask(
        &self,
        system: &str,
        tools: &[ToolSpec],
        messages: &[Message],
        on_retry: &mut dyn FnMut(u64),
    ) -> Result<AssistantTurn, AiError> {
        let request = TurnRequest {
            model: self.model,
            system,
            tools,
            messages,
            max_tokens: MAX_OUTPUT_TOKENS,
        };
        let mut forward = |event: StreamEvent| {
            if let StreamEvent::Retrying { wait_ms, .. } = event {
                on_retry(wait_ms);
            }
        };
        match self
            .provider
            .stream_turn(self.key, &request, self.cancel, &mut forward)
        {
            Ok(turn) if turn.stop == StopReason::Refusal => Err(AiError::Refused),
            other => other,
        }
    }
}

/// `plan_build`: the chat's way into the staged build.
pub fn plan_build_tool() -> ToolSpec {
    ToolSpec {
        name: "plan_build".into(),
        description: "Hands a job that builds or reworks a sequence (a whole song, a new look for several parts, calmer or bigger parts) to PixelFlow's staged build: a design, the parts built at once, a review; the person then sees the proposal. Call it alone, with a brief.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "ask": { "type": "string", "description": "What the person wants, in a sentence or two." },
                "sections": { "type": "array", "items": { "type": "string" }, "description": "The parts of the song the job is about, by label; none for all of it." },
                "keep": { "type": "string", "description": "What must stay as it is." },
                "style": { "type": "string", "description": "What the person said about the look: mood, colors, props." },
            },
            "required": ["ask"],
        }),
    }
}

/// `submit_plan`: the design stage's answer, terse: short fields with their lengths, lists with
/// their sizes, so the plan is a page and not an essay. The reply to the person is written from
/// it by the program ([`Plan::reply`]).
pub fn submit_plan_tool() -> ToolSpec {
    let list = |items: Value, most: usize| json!({ "type": "array", "items": items, "maxItems": most });
    let text = |most: usize| json!({ "type": "string", "maxLength": most });
    let names = || json!({ "type": "array", "items": { "type": "string" } });
    let step_tools: Vec<String> = Toolbox::default()
        .tools()
        .iter()
        .map(|t| t.spec.name.clone())
        .filter(|name| plan::step_tool(name))
        .collect();
    ToolSpec {
        name: "submit_plan".into(),
        description: "The design, terse: each part of the song (props that play, palette, level, motion, a few words of look; or keep, saying why it stays), the props' roles, the moments as cues, and the steps that build it.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string", "maxLength": plan::MAX_TEXT, "description": "One or two plain sentences the person reads; no ids." },
                "sections": list(json!({
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "A part's label, a label's root (every chorus), or a numbered one (\"Chorus 3\")." },
                        "props": names(),
                        "palette": list(json!({ "type": "string" }), plan::MAX_PALETTE),
                        "level": { "type": "number", "minimum": 0, "maximum": 1 },
                        "motion": { "enum": plan::MOTIONS },
                        "look": text(plan::MAX_TEXT),
                        "keep": { "type": "string", "maxLength": plan::MAX_TEXT, "description": "Only for a part left as it is: why." },
                    },
                    "required": ["label", "props"],
                }), 16),
                "roles": list(json!({
                    "type": "object",
                    "properties": { "prop": { "type": "string" }, "role": text(plan::MAX_TEXT / 2) },
                    "required": ["prop", "role"],
                }), 16),
                "moments": list(json!({
                    "type": "object",
                    "properties": {
                        "at": { "type": "integer" },
                        "until": { "type": "integer" },
                        "cue": { "enum": crate::cues::CUES },
                        "props": names(),
                    },
                    "required": ["at", "cue"],
                }), plan::MAX_MOMENTS),
                "vocals": text(plan::MAX_TEXT),
                "steps": list(json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "sections": names(),
                        "props": names(),
                        "tools": { "type": "array", "items": { "enum": step_tools } },
                        "effects": list(json!({ "enum": reference::kind_keys() }), plan::MAX_STEP_EFFECTS),
                        "topics": list(json!({ "enum": knowledge::library::topic_keys() }), plan::MAX_STEP_TOPICS),
                        "do": text(plan::MAX_DO),
                        "after": names(),
                    },
                    "required": ["id", "sections", "props", "tools", "do"],
                }), plan::MAX_PLAN_STEPS),
            },
            "required": ["sections", "steps"],
        }),
    }
}

/// Builds `brief` in stages on `draft`. Progress goes to `on_event`; what each stage said is in
/// the result's log.
pub fn build(
    ctx: &Ctx<'_>,
    brief: &Brief,
    draft: &mut Draft,
    on_event: &mut dyn FnMut(ChatEvent),
) -> Result<Built, Failure> {
    let mut done = Partial::default();
    on_event(stage_event(
        StageName::Designing,
        "Designing the show".into(),
        0,
        0,
    ));
    let world = World::of(draft, ctx.analysis().as_deref(), ctx.toolbox);
    let designed = design(ctx, brief, &world, draft, on_event);
    done.add(designed.stage, designed.requests, designed.requests);
    let plan = match designed.plan {
        Ok(plan) => plan,
        Err(DesignStop::Failed(why)) => return Err(Failure::Design(why, done)),
        Err(DesignStop::Ai(error)) => return Err(Failure::Ai(error, done)),
    };
    let (steps, failed) = match build_steps(ctx, &plan, &world, draft, on_event) {
        Ok(steps) => (steps, None),
        Err((error, steps)) => (steps, Some(error)),
    };
    done.requests += steps.requests;
    done.critical_path += steps.critical_path;
    done.log.extend(steps.log);
    let mut notes = steps.notes;
    if let Some(error) = failed {
        return Err(Failure::Ai(error, done));
    }
    on_event(stage_event(
        StageName::Reviewing,
        "Reviewing the draft".into(),
        0,
        0,
    ));
    // The plan checked against the draft: every part it builds, with the props it has playing.
    let planned = coverage::planned(&plan, &world, &brief.scope(&world));
    let mut gaps = coverage::check(draft, &planned, ctx.cancel);
    for pass in 0..MAX_REFINES {
        // A second refine only for what the first left dark.
        if pass > 0 && gaps.is_empty() {
            break;
        }
        let Some(refined) = refine(ctx, &plan, &world, (&planned, &gaps), pass, draft, on_event) else {
            break;
        };
        done.add(refined.stage, refined.requests, refined.requests);
        match refined.error {
            Some(AiError::Cancelled) => return Err(Failure::Ai(AiError::Cancelled, done)),
            // What the steps built stands; only the last touches are missing.
            Some(error) => {
                notes.push(format!(
                    "The last touches after the review couldn't be made ({error})."
                ));
                break;
            }
            None => {}
        }
        gaps = coverage::check(draft, &planned, ctx.cancel);
    }
    if !gaps.is_empty() {
        notes.push(coverage::sentence(&gaps, &world));
    }
    Ok(Built {
        plan,
        requests: done.requests,
        critical_path: done.critical_path,
        notes,
        log: done.log,
    })
}

impl Partial {
    fn add(&mut self, stage: StageLog, requests: usize, waited: usize) {
        self.log.push(stage);
        self.requests += requests;
        self.critical_path += waited;
    }
}

/// Whether a request counts as sent: anything but one stopped before it went (Stop, or the
/// evaluation's budget refusing it).
fn sent(asked: &Result<AssistantTurn, AiError>) -> usize {
    usize::from(!matches!(asked, Err(AiError::Cancelled)))
}

fn stage_event(stage: StageName, label: String, done: usize, total: usize) -> ChatEvent {
    ChatEvent::Stage {
        stage,
        label,
        done,
        total,
    }
}

fn retrying(ms: u64) -> ChatEvent {
    ChatEvent::Retrying {
        seconds: ms.div_ceil(1000),
    }
}

/// What the design came to: its conversation, the requests it made, and the plan or why there
/// is none.
struct Designed {
    stage: StageLog,
    requests: usize,
    plan: Result<Plan, DesignStop>,
}

enum DesignStop {
    /// No plan that fits after every try.
    Failed(String),
    Ai(AiError),
}

/// The design's request: the brief, then the show, sequence, and song as the chat's context
/// block gives them, and the review of what is there when there is anything.
fn design_message(ctx: &Ctx<'_>, brief: &Brief, draft: &mut Draft) -> String {
    let analysis = ctx.analysis();
    let mut lines = vec![
        stage_line("design"),
        format!("Brief: {}", crate::context::data(&brief.to_json())),
        "<context>".to_string(),
    ];
    lines.extend(crate::context::describe(draft, analysis.as_deref()));
    let has_effects = draft.sequence().is_some_and(|doc| doc.effects().next().is_some());
    if has_effects && let Ok(Some(review)) = draft.review(analysis.as_deref(), ctx.cancel) {
        lines.push(format!(
            "The review of the sequence as it is now: {}",
            crate::context::data(&serde_json::from_str(&review.to_model()).unwrap_or(Value::Null))
        ));
    }
    lines.push("</context>".to_string());
    lines.join("\n")
}

fn design(
    ctx: &Ctx<'_>,
    brief: &Brief,
    world: &World,
    draft: &mut Draft,
    on_event: &mut dyn FnMut(ChatEvent),
) -> Designed {
    let tools = [submit_plan_tool()];
    let mut stage = StageLog {
        stage: "design".into(),
        messages: vec![Message::User(design_message(ctx, brief, draft))],
        given: catalogue::DESIGN_TOPICS
            .iter()
            .map(|t| format!("topic:{t}"))
            .collect(),
    };
    let mut requests = 0;
    let stop = |stage, requests, why| Designed {
        stage,
        requests,
        plan: Err(why),
    };
    for _ in 0..MAX_DESIGN_TRIES {
        if let Err(error) = ctx.cancel.check() {
            return stop(stage, requests, DesignStop::Ai(error));
        }
        let asked = ctx.ask(catalogue::design_system(), &tools, &stage.messages, &mut |ms| {
            on_event(retrying(ms))
        });
        requests += sent(&asked);
        let turn = match asked {
            Ok(turn) => turn,
            Err(error) => return stop(stage, requests, DesignStop::Ai(error)),
        };
        let calls = turn.tool_calls.clone();
        stage.messages.push(Message::Assistant(turn));
        if calls.is_empty() {
            stage.messages.push(Message::User(
                "Answer with one call to submit_plan: the plan, as its input describes.".into(),
            ));
            continue;
        }
        let mut accepted = None;
        let results: Vec<ToolResult> = calls
            .iter()
            .map(|call| {
                let (content, is_error) = if call.name != "submit_plan" {
                    ("Only submit_plan is answered here.".to_string(), true)
                } else if let Some(problem) = &call.input_error {
                    (problem.clone(), true)
                } else if accepted.is_some() {
                    ("One plan is enough.".to_string(), true)
                } else {
                    match Plan::read_for(&call.input, world, Some(brief)) {
                        Ok(plan) => {
                            accepted = Some(plan);
                            (
                                "The plan is accepted: the steps are being built.".to_string(),
                                false,
                            )
                        }
                        Err(problems) => (
                            format!(
                                "Nothing was built yet. Fix these and send the whole plan again:\n- {}",
                                problems.join("\n- ")
                            ),
                            true,
                        ),
                    }
                };
                ToolResult {
                    call_id: call.id.clone(),
                    content,
                    is_error,
                }
            })
            .collect();
        stage.messages.push(Message::ToolResults(results));
        if let Some(plan) = accepted {
            return Designed {
                stage,
                requests,
                plan: Ok(plan),
            };
        }
    }
    stop(
        stage,
        requests,
        DesignStop::Failed(format!(
            "the design didn't come to a plan that fits after {MAX_DESIGN_TRIES} tries"
        )),
    )
}

/// What the build of the steps came to.
#[derive(Default)]
struct Steps {
    requests: usize,
    critical_path: usize,
    log: Vec<StageLog>,
    notes: Vec<String>,
}

/// One step built.
struct StepOutcome {
    /// Its copy of the draft, with what it drafted.
    draft: Draft,
    requests: usize,
    log: StageLog,
    error: Option<AiError>,
}

fn build_steps(
    ctx: &Ctx<'_>,
    plan: &Plan,
    world: &World,
    draft: &mut Draft,
    on_event: &mut dyn FnMut(ChatEvent),
) -> Result<Steps, (AiError, Steps)> {
    let analysis = ctx.analysis();
    // The song's tracks first, so every step finds "Sections", "Bars", and "Beats" there and
    // none adds its own copy.
    if let Some(analysis) = &analysis {
        let tracks: Vec<String> = crate::song::DEFAULT_TRACKS.map(String::from).to_vec();
        let _ = crate::song::add_timing(Some(analysis), draft, &tracks);
    }
    let song = analysis
        .as_ref()
        .map(|a| crate::song::describe(a, draft.base().sequence.as_ref().map(|s| &s.doc)));
    let total = plan.steps.len();
    let mut steps = Steps::default();
    let mut done = 0;
    let mut logs: Vec<Option<StageLog>> = vec![None; total];
    let mut failed: Vec<(usize, AiError)> = Vec::new();
    let build = Build {
        plan,
        world,
        song: song.as_ref(),
    };
    for wave in plan.waves() {
        if let Err(error) = ctx.cancel.check() {
            steps.log = logs.into_iter().flatten().collect();
            return Err((error, steps));
        }
        let snapshot = draft.clone();
        let outcomes = run_wave(ctx, &build, &snapshot, &wave, &mut done, on_event);
        let loads: Vec<usize> = outcomes.iter().map(|o| o.requests).collect();
        steps.critical_path += plan::longest_worker(&loads, ctx.workers);
        steps.requests += loads.iter().sum::<usize>();
        // Stopped: nothing of this wave goes in.
        if let Err(error) = ctx.cancel.check() {
            for (&i, outcome) in wave.iter().zip(outcomes) {
                logs[i] = Some(outcome.log);
            }
            steps.log = logs.into_iter().flatten().collect();
            return Err((error, steps));
        }
        for (&i, outcome) in wave.iter().zip(outcomes) {
            logs[i] = Some(outcome.log);
            if let Some(error) = outcome.error {
                failed.push((i, error));
                continue;
            }
            let left_out = merge(draft, &snapshot, &outcome.draft);
            if left_out > 0 {
                steps.notes.push(format!(
                    "{} of the changes for {} didn't fit with what was built before them and were left out.",
                    left_out,
                    plan.steps[i].label()
                ));
            }
        }
    }
    steps.log = logs.into_iter().flatten().collect();
    if !failed.is_empty() && failed.len() == total {
        let (_, error) = failed.swap_remove(0);
        return Err((error, steps));
    }
    for (i, error) in failed {
        steps.notes.push(format!(
            "{} couldn't be built ({error}); the rest is in the proposal.",
            plan.steps[i].label()
        ));
    }
    Ok(steps)
}

/// What every step of a build shares: the plan, what it is checked against, and the song.
struct Build<'a> {
    plan: &'a Plan,
    world: &'a World,
    song: Option<&'a Value>,
}

/// Adds what `built` drafted on top of `base` to `into`, in order: all at once when it fits,
/// else one change at a time, leaving out those that don't. Answers how many were left out.
fn merge(into: &mut Draft, base: &Draft, built: &Draft) -> usize {
    let edits = built.sequence_edits_since(base).to_vec();
    if edits.is_empty() {
        return 0;
    }
    let mut left_out = 0;
    if into.edit_sequence_batch(edits.clone()).is_err() {
        for edit in edits {
            if into.edit_sequence(edit).is_err() {
                left_out += 1;
            }
        }
    }
    into.carry_over(base, built);
    left_out
}

/// What the main thread hears from the steps of a wave.
enum Note {
    Started(usize),
    Finished(usize),
    Retrying(u64),
}

/// Builds the steps of `wave` (indices into the plan), [`Ctx::workers`] at a time, each on its own
/// copy of `snapshot`. The outcomes come back in the wave's order, whatever order they finished in.
fn run_wave(
    ctx: &Ctx<'_>,
    build: &Build<'_>,
    snapshot: &Draft,
    wave: &[usize],
    done: &mut usize,
    on_event: &mut dyn FnMut(ChatEvent),
) -> Vec<StepOutcome> {
    let plan = build.plan;
    let slots: Vec<Mutex<Option<StepOutcome>>> = wave.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let workers = ctx.workers.clamp(1, wave.len().max(1));
    let total = plan.steps.len();
    let (tx, rx) = mpsc::channel::<Note>();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (slots, next) = (&slots, &next);
            scope.spawn(move || {
                loop {
                    let k = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&i) = wave.get(k) else {
                        break;
                    };
                    let _ = tx.send(Note::Started(i));
                    let outcome = run_step(ctx, build, i, snapshot, &mut |ms| {
                        let _ = tx.send(Note::Retrying(ms));
                    });
                    *slots[k].lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
                    let _ = tx.send(Note::Finished(i));
                }
            });
        }
        drop(tx);
        let mut running: BTreeSet<usize> = BTreeSet::new();
        for note in rx {
            match note {
                Note::Started(i) => {
                    running.insert(i);
                }
                Note::Finished(i) => {
                    running.remove(&i);
                    *done += 1;
                }
                Note::Retrying(ms) => {
                    on_event(retrying(ms));
                    continue;
                }
            }
            let now: Vec<String> = running.iter().map(|&i| plan.steps[i].label()).collect();
            let label = if now.is_empty() {
                "Building".to_string()
            } else {
                format!("Building {}", now.join("; "))
            };
            on_event(stage_event(StageName::Building, label, *done, total));
        }
    });
    slots
        .into_iter()
        .zip(wave)
        .map(|(slot, &i)| {
            slot.into_inner()
                .unwrap_or_else(PoisonError::into_inner)
                .unwrap_or_else(|| StepOutcome {
                    draft: snapshot.clone(),
                    requests: 0,
                    log: StageLog {
                        stage: step_stage(&plan.steps[i].id),
                        messages: Vec::new(),
                        given: Vec::new(),
                    },
                    error: Some(AiError::Cancelled),
                })
        })
        .collect()
}

/// The full definitions of `names`, in the toolbox's order.
fn specs(toolbox: &Toolbox, names: &[String]) -> Vec<ToolSpec> {
    toolbox
        .tools()
        .iter()
        .filter(|t| names.contains(&t.spec.name))
        .map(|t| t.spec.clone())
        .collect()
}

/// What a step that changed things, and whose calls all worked, is told when it isn't done.
const PLACE_NOW: &str = "Your changes are in. If this step places anything (its do says what), place it now, all in one reply; if the step is complete, answer without calling a tool.";

fn run_step(
    ctx: &Ctx<'_>,
    build: &Build<'_>,
    index: usize,
    snapshot: &Draft,
    on_retry: &mut dyn FnMut(u64),
) -> StepOutcome {
    let plan = build.plan;
    let step = &plan.steps[index];
    let mut draft = snapshot.clone();
    let (first, given) = step_message(plan, index, &draft, build.song);
    let tools = specs(ctx.toolbox, &step.tools);
    let expects = coverage::for_step(plan, build.world, index);
    let guard = Guard::new(plan, build.world);
    // A step that builds parts is over when they have what the plan has playing there; one over
    // the whole song when it has placed something (or has nothing to place with).
    let mut nudged = false;
    let mut follow = |draft: &Draft, placed: bool| -> Option<String> {
        if !expects.is_empty() {
            let gaps = coverage::check(draft, &expects, ctx.cancel);
            return (!gaps.is_empty()).then(|| {
                format!(
                    "Your calls worked, but the plan's look for your part of the song isn't there yet:\n{}\nPlace it now, all in one reply (place_effects with items, or repeat_effects from a part you built); then the step is over.",
                    coverage::describe(&gaps, build.world, draft)
                )
            });
        }
        if placed || nudged || !step.lays_looks() {
            return None;
        }
        nudged = true;
        Some(PLACE_NOW.to_string())
    };
    let worked = work(
        ctx,
        &Talk {
            system: catalogue::build_system(),
            tools: &tools,
            most: MAX_STEP_REQUESTS,
            given: &given,
            guard: Some(&guard),
        },
        first,
        &mut draft,
        &mut follow,
        on_retry,
    );
    StepOutcome {
        draft,
        requests: worked.requests,
        log: StageLog {
            stage: step_stage(&step.id),
            messages: worked.messages,
            given,
        },
        error: worked.error,
    }
}

/// Whether `[from, to)` meets any of `spans`.
fn within(spans: &[(u64, u64)], from: u64, to: u64) -> bool {
    spans.iter().any(|(a, b)| from < *b && *a < to.max(from + 1))
}

/// The stretch of the song a step works in: its parts, the moments, accents, bars, and words in
/// them, from the song as `analyze_song` gives it.
fn song_slice(song: &Value, spans: &[(u64, u64)]) -> Value {
    let at = |v: &Value| v.as_u64().unwrap_or(0);
    let keep = |list: &Value, start: &dyn Fn(&Value) -> (u64, u64)| -> Vec<Value> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter(|item| {
                let (from, to) = start(item);
                within(spans, from, to)
            })
            .cloned()
            .collect()
    };
    let sections = keep(&song["sections"], &|s| (at(&s["startMs"]), at(&s["endMs"])));
    let moments = keep(&song["moments"], &|m| (at(&m[0]), at(&m[0])));
    let accents = keep(&song["accents"], &|a| (at(&a["atMs"]), at(&a["atMs"])));
    let digits = |key: &str| -> Vec<char> { song[key].as_str().unwrap_or_default().chars().collect() };
    let (energy, bass, drums) = (digits("barEnergy"), digits("barBass"), digits("barDrums"));
    let bars: Vec<Value> = song["barsMs"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, ms)| within(spans, at(ms), at(ms)))
        .map(|(i, ms)| {
            let digit = |d: &[char]| d.get(i).map(|c| c.to_string()).unwrap_or_default();
            json!([ms, format!("{}{}{}", digit(&energy), digit(&bass), digit(&drums))])
        })
        .collect();
    let mut slice = json!({
        "tempoBpm": song["tempoBpm"],
        "sectionsFrom": song["sectionsFrom"],
        "sections": sections,
        "moments": moments,
        "accents": accents,
        "bars": bars,
        "eachBar": "[ms, energy, bass, drums digits 0-9]",
    });
    if let Some(lyrics) = song.get("lyrics") {
        let mut words = lyrics.clone();
        words["lines"] = json!(keep(&lyrics["lines"], &|l| (at(&l[0]), at(&l[0]))));
        slice["lyrics"] = words;
    }
    slice
}

/// The first message of a step's conversation, and the guides it gives.
fn step_message(plan: &Plan, index: usize, draft: &Draft, song: Option<&Value>) -> (String, Vec<String>) {
    let step = &plan.steps[index];
    let show = draft.show();
    let mut lines = vec![
        stage_line(&step_stage(&step.id)),
        format!(
            "Step {} of {}: {}",
            index + 1,
            plan.steps.len(),
            crate::context::data(&json!(step.title))
        ),
        format!("Do: {}", crate::context::data(&json!(step.todo))),
        format!(
            "Its parts of the song: {}; its props: {}.",
            crate::context::data(&json!(step.label())),
            crate::context::data(&json!(step.props))
        ),
        format!("The plan in brief: {}", crate::context::data(&plan.outline())),
        "<context>".to_string(),
    ];
    if let Some(song) = song {
        lines.push(format!(
            "The song where this step works: {}",
            crate::context::data(&song_slice(song, &step.spans))
        ));
    }
    let named_groups: BTreeSet<String> = step.props.iter().map(|p| p.to_lowercase()).collect();
    let all = named_groups.contains(plan::ALL_PROPS);
    if let Some(doc) = draft.sequence() {
        let rows: Vec<&pf_sequence::Row> = doc
            .rows
            .iter()
            .filter(|row| match row.target {
                Target::Prop(id) | Target::Region { prop: id, .. } => step.lights.contains(&id),
                Target::Group(id) => {
                    all || show
                        .groups
                        .iter()
                        .find(|g| g.id == id)
                        .is_some_and(|g| named_groups.contains(&g.name.to_lowercase()))
                }
            })
            .take(MAX_ROWS_LISTED)
            .collect();
        let listed: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut line = vec![json!(row.id), json!(target_name(show, row.target))];
                if let Some(prop) = row.target.prop().and_then(|id| show.prop(id)) {
                    line.push(json!(crate::run::role_name(prop)));
                    line.push(json!(crate::run::grid(prop).map(|(c, r)| format!("{c}x{r}"))));
                }
                line.push(json!(row.layers.len()));
                Value::Array(line)
            })
            .collect();
        lines.push(format!(
            "Its rows, each [rowId, lights, role, grid, layers]: {}",
            crate::context::data(&json!(listed))
        ));
        let mut there: Vec<Value> = Vec::new();
        let mut more = 0;
        for row in &rows {
            for (layer, l) in row.layers.iter().enumerate() {
                for effect in l
                    .effects
                    .iter()
                    .filter(|e| within(&step.spans, e.start_ms, e.end_ms))
                {
                    if there.len() >= MAX_EFFECTS_LISTED {
                        more += 1;
                        continue;
                    }
                    there.push(json!([
                        row.id,
                        effect.id,
                        layer,
                        reference::key_of(&effect.kind()),
                        effect.start_ms,
                        effect.end_ms,
                        effect.palette.colors
                    ]));
                }
            }
        }
        lines.push(format!(
            "What its rows have there now, each [rowId, id, layer, kind, startMs, endMs, colors]: {}{}",
            crate::context::data(&json!(there)),
            if more > 0 {
                format!(" …and {more} more (list_sequence_effects lists them).")
            } else {
                String::new()
            }
        ));
        let tracks: Vec<&str> = doc.timing_tracks.iter().map(|t| t.name.as_str()).collect();
        lines.push(format!("Timing tracks: {}", crate::context::data(&json!(tracks))));
    }
    lines.push("</context>".to_string());
    // The guides the step works from: each effect's drawing and looks, and the topics it names.
    let mut given = Vec::new();
    let mut guides = Vec::new();
    for key in &step.topics {
        if let Some(text) = knowledge::topic_text(key) {
            guides.push(text);
            given.push(format!("topic:{key}"));
        }
    }
    for key in &step.effects {
        if let Some(kind) = reference::kind_named(key) {
            guides.push(knowledge::effect_parts(kind, &STEP_PARTS));
            given.push(format!("effect:{key}"));
        }
    }
    if !guides.is_empty() {
        lines.push(format!("\nGuides:\n\n{}", guides.join("\n\n---\n\n")));
    }
    (lines.join("\n"), given)
}

/// The parts of an effect's guide a step is given.
const STEP_PARTS: [&str; 2] = ["draws", "looks"];

/// What keeps a cue from erasing the plan: the parts the plan lights, and its own moments. A
/// blackout over a part the plan has lit is held to its moment there (a bar into the part), so
/// the part's look wins outside it; one the plan stages itself (a blackout among its moments, at
/// that time) is left as it is.
struct Guard<'a> {
    plan: &'a Plan,
    world: &'a World,
    planned: Vec<Expect>,
}

impl<'a> Guard<'a> {
    fn new(plan: &'a Plan, world: &'a World) -> Self {
        Self {
            plan,
            world,
            planned: coverage::planned(plan, world, &[(0, world.duration_ms.max(1))]),
        }
    }

    /// The props a cue's targets name (none: every prop), or `None` for a target only the cue
    /// itself can read ("left", "outlines").
    fn props(&self, cue: &Value) -> Option<BTreeSet<PropId>> {
        let Some(targets) = cue["targets"].as_array() else {
            return Some(self.world.all_props().clone());
        };
        let mut props = BTreeSet::new();
        for target in targets {
            props.extend(self.world.props(target.as_str()?)?);
        }
        Some(props)
    }

    /// Whether the plan stages a blackout itself at `at`.
    fn planned_blackout(&self, at: u64, until: u64) -> bool {
        let bar = self.world.bar_ms;
        self.plan.moments.iter().any(|m| {
            let (a, b) = m.window(bar);
            m.cue == "blackout" && a.abs_diff(at) <= bar && until <= b + bar
        })
    }

    /// `input` (a `stage_cue` call's) with every blackout held to its moment where it would darken
    /// a part the plan lights, and what was changed, in words.
    fn hold(&self, input: &Value) -> (Value, Vec<String>) {
        let mut input = input.clone();
        let mut said = Vec::new();
        let bar = self.world.bar_ms;
        let Some(cues) = input["cues"].as_array_mut() else {
            return (input, said);
        };
        for cue in cues.iter_mut() {
            if cue["cue"] != "blackout" {
                continue;
            }
            let (Some(at), Some(until)) = (cue["at"].as_u64(), cue["until"].as_u64()) else {
                continue;
            };
            let Some(props) = self.props(cue) else {
                continue;
            };
            if until <= at || self.planned_blackout(at, until) {
                continue;
            }
            // The earliest part it would darken for more than its moment (a bar from where it
            // meets the part).
            let held = self
                .planned
                .iter()
                .filter(|e| e.span.0 < until && at < e.span.1 && !e.props.is_disjoint(&props))
                .map(|e| (at.max(e.span.0) + bar, e))
                .filter(|(end, e)| *end < until.min(e.span.1))
                .min_by_key(|(end, _)| *end);
            if let Some((end, part)) = held {
                cue["until"] = json!(end);
                let names: Vec<&str> = part
                    .props
                    .intersection(&props)
                    .take(6)
                    .map(|p| self.world.prop_name(*p))
                    .collect();
                said.push(format!(
                    "The blackout at {at} ms now ends at {end} ms, a bar into {} ({}–{} ms), where the plan has {} lit: a blackout darkens its moment, and the part's look wins after it.",
                    part.part,
                    part.span.0,
                    part.span.1,
                    names.join(", ")
                ));
            }
        }
        (input, said)
    }
}

/// A drafting conversation's fixed part: its instructions, its tools, the requests it may make,
/// the guides its first message gave, and what keeps its cues to the plan.
struct Talk<'a> {
    system: &'a str,
    tools: &'a [ToolSpec],
    most: usize,
    given: &'a [String],
    guard: Option<&'a Guard<'a>>,
}

/// A conversation that drafts: what it said, and how many requests it made.
struct Worked {
    messages: Vec<Message>,
    requests: usize,
    error: Option<AiError>,
}

/// Runs a drafting conversation on `draft` until it answers without calls, or it has made
/// `most` requests, or a reply's calls all worked, changed the draft, and `follow` has nothing
/// more to say (it is asked with the draft and whether the reply placed anything new; what it
/// says is the next message). Only the talk's tools may be called.
fn work(
    ctx: &Ctx<'_>,
    talk: &Talk<'_>,
    first: String,
    draft: &mut Draft,
    follow: &mut dyn FnMut(&Draft, bool) -> Option<String>,
    on_retry: &mut dyn FnMut(u64),
) -> Worked {
    let allowed: Vec<&str> = talk.tools.iter().map(|t| t.name.as_str()).collect();
    let mut worked = Worked {
        messages: vec![Message::User(first)],
        requests: 0,
        error: None,
    };
    let mut cache = ctx.song.clone();
    // What the first message gave is read already: asked for again, it is pointed back to.
    let mut read = GuidesRead::default();
    for given in talk.given {
        match given.strip_prefix("effect:") {
            Some(effect) => {
                for part in STEP_PARTS {
                    read.mark(format!("effect:{effect}#{part}"));
                }
            }
            None => read.mark(given.clone()),
        }
    }
    for _ in 0..talk.most {
        if let Err(error) = ctx.cancel.check() {
            worked.error = Some(error);
            return worked;
        }
        let asked = ctx.ask(talk.system, talk.tools, &worked.messages, on_retry);
        worked.requests += sent(&asked);
        let turn = match asked {
            Ok(turn) => turn,
            Err(error) => {
                worked.error = Some(error);
                return worked;
            }
        };
        let calls = turn.tool_calls.clone();
        let cut_off = turn.stop == StopReason::MaxTokens;
        worked.messages.push(Message::Assistant(turn));
        if calls.is_empty() {
            break;
        }
        let mut song = Song {
            music: ctx.music,
            cache: &mut cache,
            analyzer: ctx.analyzer,
            cancel: ctx.cancel,
        };
        let results: Vec<ToolResult> = calls
            .iter()
            .map(|call| {
                answer(
                    ctx.toolbox,
                    &allowed,
                    talk.guard,
                    call,
                    cut_off,
                    draft,
                    &mut song,
                    &mut read,
                )
            })
            .collect();
        let refused = results.iter().any(|r| r.is_error);
        let worked_calls = || calls.iter().zip(&results).filter(|(_, r)| !r.is_error);
        let drafted = worked_calls().any(|(call, _)| plan::drafts(&call.name));
        let placed = worked_calls().any(|(call, _)| plan::places(&call.name));
        worked.messages.push(Message::ToolResults(results));
        if drafted && !refused {
            match follow(draft, placed) {
                None => break,
                Some(more) => worked.messages.push(Message::User(more)),
            }
        }
    }
    worked
}

/// One call in a stage's conversation, answered from its own tools only.
#[allow(clippy::too_many_arguments)]
fn answer(
    toolbox: &Toolbox,
    allowed: &[&str],
    guard: Option<&Guard<'_>>,
    call: &ToolCall,
    cut_off: bool,
    draft: &mut Draft,
    song: &mut Song<'_>,
    read: &mut GuidesRead,
) -> ToolResult {
    let reply = |content: String, is_error: bool| ToolResult {
        call_id: call.id.clone(),
        content,
        is_error,
    };
    if cut_off {
        return reply(
            "Your reply was cut off before this call was complete, so nothing was run. Make smaller changes per call.".into(),
            true,
        );
    }
    if !allowed.contains(&call.name.as_str()) {
        return reply(
            format!(
                "This stage has only these tools: {}. Nothing was run.",
                allowed.join(", ")
            ),
            true,
        );
    }
    if call.name == "get_guide" && call.input_error.is_none() {
        return match knowledge::answer_once(&call.input, read) {
            Ok(content) => reply(content, false),
            Err(content) => reply(content, true),
        };
    }
    let mut held = Vec::new();
    let mut call = call.clone();
    if call.name == "stage_cue"
        && call.input_error.is_none()
        && let Some(guard) = guard
    {
        let (input, said) = guard.hold(&call.input);
        call.input = input;
        held = said;
    }
    match run_tool(toolbox, &call, draft, song) {
        Outcome::Answer {
            mut content,
            is_error,
        } => {
            if !is_error && !held.is_empty() {
                content.push('\n');
                content.push_str(&held.join("\n"));
            }
            reply(content, is_error)
        }
        _ => reply("That isn't done in this stage.".into(), true),
    }
}

/// What the refine came to: its conversation, the requests it made, and the error that
/// stopped it, if one did.
struct Refined {
    stage: StageLog,
    requests: usize,
    error: Option<AiError>,
}

/// A refine (`pass` 0 the first): the review of the whole draft, and what of the plan
/// (`planned`) is still dark (`gaps`), made good in one small conversation that has the plan in brief, the tools to place effects as well
/// as change them ([`REFINE_TOOLS`]), and those the review's fixes name. `None` when nothing
/// needs it: the review scores [`REFINED_BELOW`] or more and the plan is all there.
fn refine(
    ctx: &Ctx<'_>,
    plan: &Plan,
    world: &World,
    (planned, gaps): (&[Expect], &[coverage::Gap]),
    pass: usize,
    draft: &mut Draft,
    on_event: &mut dyn FnMut(ChatEvent),
) -> Option<Refined> {
    let log = |messages| StageLog {
        stage: if pass == 0 {
            "refine".into()
        } else {
            format!("refine {}", pass + 1)
        },
        messages,
        given: Vec::new(),
    };
    let stopped = |error| Refined {
        stage: log(Vec::new()),
        requests: 0,
        error: Some(error),
    };
    if let Err(error) = ctx.cancel.check() {
        return Some(stopped(error));
    }
    let analysis = ctx.analysis();
    let review = match draft.review(analysis.as_deref(), ctx.cancel) {
        Ok(Some(review)) => review,
        Ok(None) => return None,
        // The review stops only when the person pressed Stop.
        Err(_) => return ctx.cancel.check().err().map(stopped),
    };
    let fixable = review
        .fixes
        .iter()
        .flat_map(|f| &f.calls)
        .any(|call| plan::step_tool(call.tool));
    if gaps.is_empty() && (review.score >= REFINED_BELOW || !fixable) {
        return None;
    }
    let mut named: Vec<String> = REFINE_TOOLS.iter().map(|t| t.to_string()).collect();
    for call in review.fixes.iter().flat_map(|f| &f.calls) {
        if plan::step_tool(call.tool) && !named.iter().any(|n| n == call.tool) {
            named.push(call.tool.to_string());
        }
    }
    let mut what = Vec::new();
    if !gaps.is_empty() {
        what.push(format!(
            "{} {} of the plan dark",
            gaps.len(),
            if gaps.len() == 1 { "part" } else { "parts" }
        ));
    }
    if !review.fixes.is_empty() {
        what.push(format!(
            "{} {}",
            review.fixes.len(),
            if review.fixes.len() == 1 { "fix" } else { "fixes" }
        ));
    }
    on_event(stage_event(
        StageName::Refining,
        format!("Refining: {}", what.join(", ")),
        0,
        0,
    ));
    let mut first = vec![
        stage_line("refine"),
        format!("The plan in brief: {}", crate::context::data(&plan.outline())),
    ];
    if !gaps.is_empty() {
        first.push(format!(
            "What the plan has playing that the draft doesn't light (place these first: they are the plan, not built):\n{}",
            coverage::describe(gaps, world, draft)
        ));
    }
    first.push(format!(
        "The review of the whole draft: {}",
        crate::context::data(&serde_json::from_str(&review.to_model()).unwrap_or(Value::Null))
    ));
    let tools = specs(ctx.toolbox, &named);
    let guard = Guard::new(plan, world);
    // Over when the plan is all there; told what is still dark until then.
    let mut follow = |draft: &Draft, _placed: bool| -> Option<String> {
        let left = coverage::check(draft, planned, ctx.cancel);
        (!left.is_empty()).then(|| {
            format!(
                "Your calls worked. Still dark where the plan has something playing:\n{}\nPlace it now, all in one reply; then you're done.",
                coverage::describe(&left, world, draft)
            )
        })
    };
    let worked = work(
        ctx,
        &Talk {
            system: catalogue::refine_system(),
            tools: &tools,
            most: MAX_REFINE_REQUESTS,
            given: &[],
            guard: Some(&guard),
        },
        first.join("\n"),
        draft,
        &mut follow,
        &mut |ms| on_event(retrying(ms)),
    );
    Some(Refined {
        stage: log(worked.messages),
        requests: worked.requests,
        error: worked.error,
    })
}
