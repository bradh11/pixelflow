//! Whether the Assistant makes good light shows, measured: realistic requests, each answered
//! through the real chat loop on a made-up display ([`crate::stage`]) with a made-up song, then
//! scored by the design review and by plain measures (props with a part, kinds of effect, time
//! spent dark, changes where the music changes, moments hit, flashing).
//!
//! Nothing here is anyone's own: the display, the song's analysis, its words, and the requests
//! are invented, so a run sends a model nothing from a user's show, songs, or files.
//!
//! [`run`] answers one [`Scenario`] with any [`LlmProvider`]. The tests, and the example without
//! a provider, use the scripted stand-ins in [`stand_in`]: no network. The example
//! (`cargo run -p pf-ai --example assistant_eval`) calls a real model only when told to, with a
//! key from the environment. `docs/dev/assistant-eval.md` says how to run it and what it sends.

pub mod cli;
pub mod scenarios;
pub mod stand_in;

use crate::agent::{ChatSession, MAX_STEPS, SYSTEM_PROMPT};
use crate::draft::{OpenDoc, UiContext, Workspace};
use crate::error::AiError;
use crate::knowledge::reference;
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StreamEvent, TurnRequest,
};
use crate::review::{self, Criteria, Subject};
use crate::secret::ApiKey;
use crate::stage::{self, PICTURE, SONG_FILE, Stage};
use crate::tools::Toolbox;
use pf_analysis::Analysis;
use pf_model::PropId;
use pf_sequence::{Effect, Sequence, Target};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

pub use scenarios::{Scenario, scenarios};

/// How far apart the frames are that the plain measures are taken from.
const STEP_MS: u64 = 250;
/// A channel this bright is lit.
const LIT: u8 = 12;
/// A prop lit for this many samples (two seconds) has a part in the show.
const PART_SAMPLES: usize = 8;
/// Bytes to a token, roughly: prose runs nearer four, JSON nearer three.
pub const BYTES_PER_TOKEN: f64 = 3.5;
/// The most of a tool's answer kept in a transcript.
const TRANSCRIPT_CHARS: usize = 600;

/// One thing a good answer does, and whether this one did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub what: String,
    pub passed: bool,
    /// What was measured ("9 of 12").
    pub saw: String,
}

pub(crate) fn check(what: &str, passed: bool, saw: impl Into<String>) -> Check {
    Check {
        what: what.to_string(),
        passed,
        saw: saw.into(),
    }
}

/// One tool call the model made, and how it was answered.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUse {
    pub name: String,
    pub input: Value,
    /// The start of the answer.
    pub answer: String,
    pub is_error: bool,
}

/// What went to the model and came back, in bytes of text (instructions, tool definitions, and
/// the conversation; not the few hundred bytes of framing each provider adds).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traffic {
    /// Requests made.
    pub requests: usize,
    /// Everything sent, over all requests.
    pub sent_bytes: usize,
    /// The part of that which the request before had already sent (what a provider's cache can
    /// spare).
    pub repeated_bytes: usize,
    /// Everything the model wrote back: its words and its tool calls.
    pub reply_bytes: usize,
    pub largest_request_bytes: usize,
}

impl Traffic {
    pub fn add(&mut self, other: &Traffic) {
        self.requests += other.requests;
        self.sent_bytes += other.sent_bytes;
        self.repeated_bytes += other.repeated_bytes;
        self.reply_bytes += other.reply_bytes;
        self.largest_request_bytes = self.largest_request_bytes.max(other.largest_request_bytes);
    }

    /// Tokens sent and received, roughly ([`BYTES_PER_TOKEN`]).
    pub fn tokens(&self) -> (u64, u64) {
        let tokens = |bytes: usize| (bytes as f64 / BYTES_PER_TOKEN).round() as u64;
        (tokens(self.sent_bytes), tokens(self.reply_bytes))
    }

    /// What it costs at `prices`, in dollars: at full price, and when the provider charges the
    /// repeated part of each request at its cached rate.
    pub fn cost(&self, prices: &Prices) -> (f64, f64) {
        let tokens = |bytes: usize| bytes as f64 / BYTES_PER_TOKEN;
        let out = tokens(self.reply_bytes) * prices.output;
        let full = tokens(self.sent_bytes) * prices.input + out;
        let fresh = self.sent_bytes.saturating_sub(self.repeated_bytes);
        let cached = (tokens(fresh) + tokens(self.repeated_bytes) * prices.cached) * prices.input + out;
        (full / 1e6, cached / 1e6)
    }
}

/// What tokens are assumed to cost. These are the caller's figures, not facts PixelFlow knows:
/// the defaults are placeholders to be set from the provider's own price list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    /// Dollars for a million tokens sent.
    pub input: f64,
    /// Dollars for a million tokens received.
    pub output: f64,
    /// What a token the provider has cached costs, as a share of `input`.
    pub cached: f64,
}

impl Default for Prices {
    fn default() -> Self {
        Self {
            input: 3.0,
            output: 15.0,
            cached: 0.1,
        }
    }
}

/// A provider that counts what goes through it.
pub struct Metered<'a> {
    inner: &'a dyn LlmProvider,
    exchanges: Mutex<Vec<(usize, usize)>>,
}

impl<'a> Metered<'a> {
    pub fn new(inner: &'a dyn LlmProvider) -> Self {
        Self {
            inner,
            exchanges: Mutex::default(),
        }
    }

    pub fn traffic(&self) -> Traffic {
        let exchanges = self.exchanges.lock().unwrap_or_else(PoisonError::into_inner);
        let mut traffic = Traffic::default();
        let mut before = 0;
        for (sent, replied) in exchanges.iter() {
            traffic.requests += 1;
            traffic.sent_bytes += sent;
            // A conversation only grows, so what the last request sent starts this one.
            traffic.repeated_bytes += before.min(*sent);
            traffic.reply_bytes += replied;
            traffic.largest_request_bytes = traffic.largest_request_bytes.max(*sent);
            before = *sent;
        }
        traffic
    }
}

fn turn_bytes(turn: &AssistantTurn) -> usize {
    turn.text.len()
        + turn
            .tool_calls
            .iter()
            .map(|c| c.name.len() + c.input.to_string().len())
            .sum::<usize>()
}

/// The text one request carries: instructions, tool definitions, and the conversation so far.
pub fn request_bytes(request: &TurnRequest<'_>) -> usize {
    let tools: usize = request
        .tools
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();
    let messages: usize = request
        .messages
        .iter()
        .map(|m| match m {
            Message::User(text) => text.len(),
            Message::Assistant(turn) => turn_bytes(turn),
            Message::ToolResults(results) => results.iter().map(|r| r.content.len()).sum(),
        })
        .sum();
    request.system.len() + tools + messages
}

impl LlmProvider for Metered<'_> {
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
        let sent = request_bytes(request);
        let turn = self.inner.stream_turn(key, request, cancel, on_event);
        let replied = turn.as_ref().map_or(0, turn_bytes);
        self.exchanges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((sent, replied));
        turn
    }
}

/// How lit the stage is through a sequence, sampled every [`STEP_MS`].
#[derive(Debug, Clone, PartialEq)]
pub struct Lights {
    /// The mean brightness of every pixel, 0–1, at each sample.
    level: Vec<f32>,
    /// How many props are lit at each sample.
    lit: Vec<usize>,
    /// Samples each prop is lit in.
    by_prop: Vec<(PropId, usize)>,
}

impl Lights {
    pub fn of(stage: &Stage, doc: &Sequence) -> Self {
        let layout: Vec<(PropId, usize, usize, usize)> = pf_engine::preview_props_of(&stage.show)
            .into_iter()
            .map(|p| {
                (
                    p.prop,
                    p.frame_offset,
                    usize::from(p.channels_per_pixel.max(1)),
                    p.points.len() / 2,
                )
            })
            .collect();
        let times: Vec<u64> = (0..doc.duration_ms / STEP_MS)
            .map(|i| i * STEP_MS + STEP_MS / 2)
            .collect();
        let mut by_prop: Vec<(PropId, usize)> = layout.iter().map(|p| (p.0, 0)).collect();
        let (mut level, mut lit) = (Vec::new(), Vec::new());
        for frame in stage.frames(doc, &times, &stage::pictures()) {
            let (mut sum, mut pixels, mut props) = (0.0f32, 0usize, 0usize);
            for (i, (_, offset, step, count)) in layout.iter().enumerate() {
                let mut any = false;
                for pixel in 0..*count {
                    let at = offset + pixel * step;
                    let brightest = frame
                        .get(at..at + (*step).min(3))
                        .and_then(|c| c.iter().copied().max())
                        .unwrap_or(0);
                    sum += f32::from(brightest) / 255.0;
                    any |= brightest > LIT;
                }
                pixels += count;
                if any {
                    props += 1;
                    by_prop[i].1 += 1;
                }
            }
            level.push(sum / pixels.max(1) as f32);
            lit.push(props);
        }
        Self { level, lit, by_prop }
    }

    fn samples(&self, from_ms: u64, to_ms: u64) -> std::ops::Range<usize> {
        let end = (to_ms.div_ceil(STEP_MS) as usize).min(self.level.len());
        ((from_ms / STEP_MS) as usize).min(end)..end
    }

    /// The mean brightness over the spans, 0–1.
    pub fn level(&self, spans: &[(u64, u64)]) -> f32 {
        let values: Vec<f32> = spans
            .iter()
            .flat_map(|(a, b)| self.level[self.samples(*a, *b)].iter().copied())
            .collect();
        values.iter().sum::<f32>() / values.len().max(1) as f32
    }

    /// The mean number of props lit over the spans.
    pub fn lit(&self, spans: &[(u64, u64)]) -> f32 {
        let values: Vec<usize> = spans
            .iter()
            .flat_map(|(a, b)| self.lit[self.samples(*a, *b)].iter().copied())
            .collect();
        values.iter().sum::<usize>() as f32 / values.len().max(1) as f32
    }

    /// The share of the sequence with as many props lit as `wanted` says.
    pub fn share(&self, wanted: impl Fn(usize) -> bool) -> f32 {
        self.lit.iter().filter(|n| wanted(**n)).count() as f32 / self.lit.len().max(1) as f32
    }

    pub fn props(&self) -> usize {
        self.by_prop.len()
    }

    /// Props lit long enough to have a part.
    pub fn props_used(&self) -> usize {
        self.by_prop.iter().filter(|(_, n)| *n >= PART_SAMPLES).count()
    }
}

/// What the review made of a sequence.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scored {
    pub score: u32,
    pub criteria: Criteria,
    /// The top moments emphasised, and how many there were.
    pub moments_hit: usize,
    pub moments: usize,
    pub flashes_safe: bool,
    /// What the review still wants fixed.
    pub fixes: Vec<String>,
}

/// The plain measures of one answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measures {
    pub proposed: bool,
    /// Model requests the answer took.
    pub requests: usize,
    pub stopped_at_step_limit: bool,
    pub tool_calls: BTreeMap<String, usize>,
    /// Tool calls refused, each with the start of the reason.
    pub tool_errors: Vec<String>,
    /// The guides read, in order.
    pub guides: Vec<String>,
    pub effects: usize,
    pub effect_kinds: Vec<String>,
    pub props_used: usize,
    pub props: usize,
    /// The share of the song with nothing lit.
    pub dark_share: f32,
    /// The share of the song with three quarters of the props or more lit.
    pub full_share: f32,
    /// Section starts where the look is new, and how many section starts there are.
    pub section_changes: usize,
    pub section_starts: usize,
    /// The most colors any one section uses.
    pub most_colors_in_a_section: usize,
    /// Mean brightness (0–1) and props lit, in the verses and in the choruses.
    pub verse_light: f32,
    pub chorus_light: f32,
    pub verse_props: f32,
    pub chorus_props: f32,
    pub review: Option<Scored>,
    /// The review of what was open before the request, when anything was.
    pub review_before: Option<u32>,
    pub traffic: Traffic,
}

/// Everything a scenario's checks can look at.
pub struct Run<'a> {
    pub stage: &'a Stage,
    pub song: &'a Analysis,
    pub before: &'a Sequence,
    pub after: &'a Sequence,
    pub reply: &'a str,
    pub tools: &'a [ToolUse],
    pub lights: &'a Lights,
}

/// One scenario answered: its checks, measures, and what the model did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub scenario: String,
    pub title: String,
    pub request: String,
    /// Why the chat stopped early, if it did (the provider failing, say).
    pub error: Option<String>,
    pub reply: String,
    pub checks: Vec<Check>,
    pub measures: Measures,
    pub transcript: Vec<ToolUse>,
}

impl Outcome {
    pub fn passed(&self) -> usize {
        self.checks.iter().filter(|c| c.passed).count()
    }
}

/// Every effect in `doc`, with the row target and layer it's on.
pub fn effects(doc: &Sequence) -> impl Iterator<Item = (Target, usize, &Effect)> {
    doc.rows.iter().flat_map(|row| {
        row.layers
            .iter()
            .enumerate()
            .flat_map(move |(l, layer)| layer.effects.iter().map(move |e| (row.target, l, e)))
    })
}

/// The parts of the song called `label` ("Verse", "Chorus").
pub fn parts(song: &Analysis, label: &str) -> Vec<(u64, u64)> {
    song.sections
        .iter()
        .filter(|s| s.label.starts_with(label))
        .map(|s| (s.start_ms, s.end_ms))
        .collect()
}

/// What plays at `at_ms`: each effect's row, layer, kind, and colors.
fn look_at(doc: &Sequence, at_ms: u64) -> BTreeSet<String> {
    effects(doc)
        .filter(|(_, _, e)| e.start_ms <= at_ms && at_ms < e.end_ms)
        .map(|(target, layer, e)| {
            format!(
                "{target:?}/{layer}/{}/{:?}",
                reference::key_of(&e.kind()),
                e.palette.colors
            )
        })
        .collect()
}

fn tool_uses(messages: &[Message]) -> Vec<ToolUse> {
    let mut uses: Vec<(String, ToolUse)> = Vec::new();
    for message in messages {
        match message {
            Message::Assistant(turn) => uses.extend(turn.tool_calls.iter().map(|call| {
                (
                    call.id.clone(),
                    ToolUse {
                        name: call.name.clone(),
                        input: call.input.clone(),
                        answer: String::new(),
                        is_error: false,
                    },
                )
            })),
            Message::ToolResults(results) => {
                for result in results {
                    // The latest call with this id that has no answer yet.
                    if let Some((_, tool)) = uses
                        .iter_mut()
                        .rev()
                        .find(|(id, tool)| *id == result.call_id && tool.answer.is_empty())
                    {
                        tool.answer = result.content.chars().take(TRANSCRIPT_CHARS).collect();
                        tool.is_error = result.is_error;
                    }
                }
            }
            Message::User(_) => {}
        }
    }
    uses.into_iter().map(|(_, tool)| tool).collect()
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn scored(
    stage: &Stage,
    doc: &Sequence,
    user: &Sequence,
    song: &Analysis,
    cancel: &Cancel,
) -> Option<Scored> {
    let reviewed = review::review(
        &Subject {
            show: &stage.show,
            doc,
            user: Some(user),
            analysis: Some(song),
            audio: &stage::audio(),
            pictures: &stage::pictures(),
        },
        cancel,
    )
    .ok()?;
    Some(Scored {
        score: reviewed.score,
        moments_hit: reviewed.top_moments.0,
        moments: reviewed.top_moments.1,
        flashes_safe: reviewed.flashes_safe,
        fixes: reviewed
            .fixes
            .iter()
            .map(|f| format!("{} {}", f.at, f.what))
            .collect(),
        criteria: reviewed.criteria,
    })
}

fn measure(run: &Run<'_>, proposed: bool, traffic: Traffic, cancel: &Cancel) -> Measures {
    let (doc, lights, song) = (run.after, run.lights, run.song);
    let mut tool_calls: BTreeMap<String, usize> = BTreeMap::new();
    let mut guides = Vec::new();
    for tool in run.tools {
        *tool_calls.entry(tool.name.clone()).or_default() += 1;
        if tool.name == "get_guide" {
            guides.extend(names(&tool.input["topics"]));
            guides.extend(names(&tool.input["effects"]));
        }
    }
    let kinds: BTreeSet<String> = effects(doc)
        .map(|(_, _, e)| reference::key_of(&e.kind()))
        .collect();
    let starts: Vec<u64> = song.sections.iter().skip(1).map(|s| s.start_ms).collect();
    // The look half a second before each new section against the look a bar and a half into
    // it, once anything staged on the downbeat has died away.
    let changes = starts
        .iter()
        .filter(|at| look_at(doc, at.saturating_sub(500)) != look_at(doc, **at + 1_500))
        .count();
    let most_colors = song
        .sections
        .iter()
        .map(|s| {
            effects(doc)
                .filter(|(_, _, e)| e.start_ms < s.end_ms && e.end_ms > s.start_ms)
                .flat_map(|(_, _, e)| e.palette.colors.iter().map(|c| (c.r, c.g, c.b)))
                .collect::<BTreeSet<_>>()
                .len()
        })
        .max()
        .unwrap_or(0);
    let (verses, choruses) = (parts(song, "Verse"), parts(song, "Chorus"));
    let has_effects = |doc: &Sequence| effects(doc).next().is_some();
    Measures {
        proposed,
        requests: traffic.requests,
        stopped_at_step_limit: traffic.requests >= MAX_STEPS,
        tool_errors: run
            .tools
            .iter()
            .filter(|t| t.is_error)
            .map(|t| format!("{}: {}", t.name, t.answer.chars().take(200).collect::<String>()))
            .collect(),
        tool_calls,
        guides,
        effects: effects(doc).count(),
        effect_kinds: kinds.into_iter().collect(),
        props_used: lights.props_used(),
        props: lights.props(),
        dark_share: lights.share(|lit| lit == 0),
        full_share: lights.share(|lit| lit * 4 >= lights.props() * 3),
        section_changes: changes,
        section_starts: starts.len(),
        most_colors_in_a_section: most_colors,
        verse_light: lights.level(&verses),
        chorus_light: lights.level(&choruses),
        verse_props: lights.lit(&verses),
        chorus_props: lights.lit(&choruses),
        review: has_effects(doc)
            .then(|| scored(run.stage, doc, run.before, song, cancel))
            .flatten(),
        review_before: has_effects(run.before)
            .then(|| scored(run.stage, run.before, run.before, song, cancel))
            .flatten()
            .map(|s| s.score),
        traffic,
    }
}

/// What a scenario opens: the stage with `doc` as its sequence, on the Sequence screen.
pub fn workspace(stage: &Stage, doc: &Sequence) -> Workspace {
    Workspace {
        show: stage.show.clone(),
        revision: 0,
        show_generation: 0,
        sequence: Some(OpenDoc {
            id: 1,
            doc: doc.clone(),
        }),
        music: Some(PathBuf::from(SONG_FILE)),
        audio: stage::audio(),
        pictures: stage::pictures(),
        images: vec![PICTURE.to_string()],
        shaders: pf_render::built_in_shaders()
            .iter()
            .map(|file| pf_render::Shaders::none().describe(file))
            .collect(),
        context: UiContext {
            screen: Some("sequence".into()),
            ..UiContext::default()
        },
    }
}

/// Answers `scenario` on `stage` with `provider` through the chat loop, as a user's message
/// would be, and scores what came of it. The song is never analyzed: its analysis is the stage's
/// own.
pub fn run(
    scenario: &Scenario,
    stage: &Stage,
    provider: &dyn LlmProvider,
    key: &ApiKey,
    model: &str,
    cancel: &Cancel,
) -> Outcome {
    let song = stage::song();
    let before = (scenario.start)(stage);
    let mut session = ChatSession::new().with_analyzer(Arc::new(|_: &Path, _: &Cancel| Ok(stage::song())));
    let metered = Metered::new(provider);
    let answered = session.run_turn(
        &metered,
        key,
        model,
        scenario.request,
        workspace(stage, &before),
        cancel,
        &mut |_| {},
    );
    let (reply, proposed, error) = match answered {
        Ok(reply) => (reply.text, reply.proposal.is_some(), None),
        // What was drafted before the failure is still proposed, and still measured.
        Err(e) => (String::new(), session.proposal().is_some(), Some(e.to_string())),
    };
    let after = session
        .draft()
        .and_then(|d| d.sequence())
        .cloned()
        .unwrap_or_else(|| before.clone());
    let tools = tool_uses(session.messages());
    let lights = Lights::of(stage, &after);
    let seen = Run {
        stage,
        song: &song,
        before: &before,
        after: &after,
        reply: &reply,
        tools: &tools,
        lights: &lights,
    };
    let measures = measure(&seen, proposed, metered.traffic(), cancel);
    let checks = (scenario.judge)(&seen, &measures);
    Outcome {
        scenario: scenario.key.to_string(),
        title: scenario.title.to_string(),
        request: scenario.request.to_string(),
        error,
        reply,
        checks,
        measures,
        transcript: tools,
    }
}

/// What a run sends a model, in words, with the sizes that never change.
pub fn what_is_sent() -> String {
    let stage = Stage::new();
    let specs = Toolbox::default().specs();
    let tools: usize = specs
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();
    let fixed = SYSTEM_PROMPT.len() + tools;
    let props: Vec<&str> = stage.show.props.iter().map(|p| p.name.as_str()).collect();
    let groups: Vec<&str> = stage.show.groups.iter().map(|g| g.name.as_str()).collect();
    format!(
        "What a run sends the model's provider:\n\
         - the Assistant's instructions and its {} tool definitions, with every request ({} bytes, about {} tokens);\n\
         - each scenario's request, as a user would type it;\n\
         - whatever the model looks up with its tools: PixelFlow's own guides, and a made-up show and song that exist only in this harness:\n\
         \x20   props: {};\n\
         \x20   groups: {};\n\
         \x20   a song called \"Paper Lanterns\" that was never recorded: its tempo, sections, and moments are written by hand, and so are its eight lines of words.\n\
         Nothing of yours is read or sent: no show, no sequence, no song, no lyrics, no file, no device.\n\
         The key is read from the {} environment variable only for a real run, and goes only to the provider, as the sign-in for each request.",
        specs.len(),
        fixed,
        (fixed as f64 / BYTES_PER_TOKEN).round(),
        props.join(", "),
        groups.join(", "),
        cli::KEY_VARIABLE,
    )
}

/// The outcomes as a report to read: a line per scenario, then each one's checks.
pub fn report(outcomes: &[Outcome], model: &str, prices: &Prices) -> String {
    let mut out = format!("# Assistant evaluation: {model}\n\n");
    let (passed, total) = outcomes
        .iter()
        .fold((0, 0), |(p, t), o| (p + o.passed(), t + o.checks.len()));
    let scores: Vec<u32> = outcomes
        .iter()
        .filter_map(|o| o.measures.review.as_ref().map(|r| r.score))
        .collect();
    let mut traffic = Traffic::default();
    for outcome in outcomes {
        traffic.add(&outcome.measures.traffic);
    }
    let (sent, received) = traffic.tokens();
    let (full, cached) = traffic.cost(prices);
    out.push_str(&format!(
        "{passed} of {total} checks passed over {} scenarios. Mean review score {}.\n\
         {} requests, about {sent} tokens sent and {received} received: about ${full:.2} at ${}/${} a million tokens in/out, or ${cached:.2} if the provider caches what each request repeats (at {} of the price). The prices are assumptions: set them with --price-in, --price-out, --price-cached.\n\n",
        outcomes.len(),
        if scores.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.0}", scores.iter().sum::<u32>() as f32 / scores.len() as f32)
        },
        traffic.requests,
        prices.input,
        prices.output,
        prices.cached,
    ));
    out.push_str(
        "| Scenario | Checks | Review | Props | Kinds | Dark | Full | Changes | Moments | Safe | Requests | Tool errors |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for o in outcomes {
        let m = &o.measures;
        let review = m.review.as_ref();
        out.push_str(&format!(
            "| {} | {}/{} | {} | {}/{} | {} | {:.0}% | {:.0}% | {}/{} | {} | {} | {} | {} |\n",
            o.scenario,
            o.passed(),
            o.checks.len(),
            review.map_or("-".to_string(), |r| r.score.to_string()),
            m.props_used,
            m.props,
            m.effect_kinds.len(),
            m.dark_share * 100.0,
            m.full_share * 100.0,
            m.section_changes,
            m.section_starts,
            review.map_or("-".to_string(), |r| format!("{}/{}", r.moments_hit, r.moments)),
            review.map_or("-", |r| if r.flashes_safe { "yes" } else { "NO" }),
            m.requests,
            m.tool_errors.len(),
        ));
    }
    for o in outcomes {
        out.push_str(&format!(
            "\n## {} ({})\n\n> {}\n\n",
            o.title, o.scenario, o.request
        ));
        if let Some(error) = &o.error {
            out.push_str(&format!("Stopped early: {error}\n\n"));
        }
        for c in &o.checks {
            out.push_str(&format!(
                "- {} {} ({})\n",
                if c.passed { "pass" } else { "FAIL" },
                c.what,
                c.saw
            ));
        }
        let m = &o.measures;
        if !m.guides.is_empty() {
            out.push_str(&format!("\nGuides read: {}.\n", m.guides.join(", ")));
        }
        if !m.effect_kinds.is_empty() {
            out.push_str(&format!(
                "{} effects of {} kinds: {}.\n",
                m.effects,
                m.effect_kinds.len(),
                m.effect_kinds.join(", ")
            ));
        }
        if let Some(review) = &m.review {
            let before = m
                .review_before
                .map_or(String::new(), |s| format!(" (it was {s} before)"));
            out.push_str(&format!("Review {}{before}.", review.score));
            if !review.fixes.is_empty() {
                out.push_str(" Still to fix:\n");
                for fix in &review.fixes {
                    out.push_str(&format!("  - {fix}\n"));
                }
            } else {
                out.push('\n');
            }
        }
        for error in &m.tool_errors {
            out.push_str(&format!("Refused: {error}\n"));
        }
        if !o.reply.is_empty() {
            let said: String = o.reply.chars().take(400).collect();
            out.push_str(&format!("\nIt said: {}\n", said.replace('\n', " ")));
        }
    }
    out
}
