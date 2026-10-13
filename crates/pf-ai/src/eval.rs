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
//! key from the environment, and then only within a [`Budget`]: a hard ceiling in dollars, kept
//! before every request from that request's worst case and after it from the usage the provider
//! reports. `docs/dev/assistant-eval.md` says how to run it, what it sends, and what it may cost.

pub mod cli;
pub mod scenarios;
pub mod stand_in;

use crate::agent::{ChatSession, Flow, MAX_STEPS, instructions};
use crate::draft::{OpenDoc, UiContext, Workspace};
use crate::error::AiError;
use crate::knowledge::reference;
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StreamEvent, TurnRequest, Usage,
};
use crate::review::{self, Criteria, Subject};
use crate::secret::ApiKey;
use crate::stage::{self, PICTURE, SONG_FILE, Stage};
use crate::tools::Toolbox;
use crate::{anthropic, openai};
use pf_analysis::Analysis;
use pf_model::PropId;
use pf_sequence::{Effect, Sequence, Target};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

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
    /// Not a call: guides a stage of a staged build was given with its request.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub given: bool,
    /// The conversation it was made in: "chat", or a staged build's stage ("design",
    /// "build step \"verses\"", "refine").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stage: String,
    /// Which request of that conversation made it, from 1 (0 for guides given).
    #[serde(skip_serializing_if = "is_zero")]
    pub request: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
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
    /// What the first request sent: the fixed part (instructions and tool definitions) and
    /// the user's message with its context. Summed when traffic is added up.
    pub first_request_bytes: usize,
}

/// Bytes as tokens, roughly ([`BYTES_PER_TOKEN`]).
pub fn tokens_of(bytes: usize) -> u64 {
    (bytes as f64 / BYTES_PER_TOKEN).round() as u64
}

impl Traffic {
    pub fn add(&mut self, other: &Traffic) {
        self.requests += other.requests;
        self.sent_bytes += other.sent_bytes;
        self.repeated_bytes += other.repeated_bytes;
        self.reply_bytes += other.reply_bytes;
        self.largest_request_bytes = self.largest_request_bytes.max(other.largest_request_bytes);
        self.first_request_bytes += other.first_request_bytes;
    }

    /// Tokens sent and received, roughly ([`BYTES_PER_TOKEN`]).
    pub fn tokens(&self) -> (u64, u64) {
        (tokens_of(self.sent_bytes), tokens_of(self.reply_bytes))
    }

    /// Tokens one request sent on average, roughly.
    pub fn mean_request_tokens(&self) -> u64 {
        tokens_of(self.sent_bytes / self.requests.max(1))
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
/// the defaults are placeholders to be set from the provider's own price list (a run that sends
/// anything takes no defaults: see [`cli`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prices {
    /// Dollars for a million tokens sent.
    pub input: f64,
    /// Dollars for a million tokens received.
    pub output: f64,
    /// What a token read from the provider's cache costs, as a share of `input`.
    pub cached: f64,
    /// What a token written to the provider's cache costs, as a share of `input` (Anthropic
    /// charges more than `input` for these; OpenAI charges nothing extra and reports none).
    pub cache_write: f64,
}

impl Default for Prices {
    fn default() -> Self {
        Self {
            input: 3.0,
            output: 15.0,
            cached: 0.1,
            cache_write: 1.25,
        }
    }
}

impl Prices {
    /// What `usage` costs, in dollars.
    pub fn cost(&self, usage: &Usage) -> f64 {
        let sent = usage.input as f64
            + usage.cache_read as f64 * self.cached
            + usage.cache_write as f64 * self.cache_write;
        (sent * self.input + usage.output as f64 * self.output) / 1e6
    }

    /// The most `sent` tokens in and `received` tokens out can cost, in dollars: every token
    /// sent at the dearest rate a sent token is charged (the full input price, or more where
    /// writing to a cache costs more), every token received at the output price.
    pub fn worst_case(&self, sent: u64, received: u64) -> f64 {
        let rate = 1.0f64.max(self.cached).max(self.cache_write);
        (sent as f64 * rate * self.input + received as f64 * self.output) / 1e6
    }
}

/// Tokens a provider may add around what a request carries (its tool-use preamble, role
/// markers), allowed for in every request's worst case. Generous: the largest of these is a few
/// hundred.
pub const FRAMING_TOKENS: u64 = 4_096;

/// The most tokens one request can be billed for: (sent, received).
///
/// Sent is every byte of the body `provider` would send for `request` (no token is shorter than
/// a byte, and the body's JSON is longer than the text a model reads), plus [`FRAMING_TOKENS`].
/// Received is the request's own output limit, which both providers send with it and which
/// counts reasoning and thinking. A model that may hand a declined request on to a fallback model
/// (Anthropic's server-side fallbacks) could be billed for both, so it counts twice.
pub fn worst_tokens(provider: ProviderId, request: &TurnRequest<'_>) -> (u64, u64) {
    let (body, answers) = match provider {
        ProviderId::Anthropic => (
            anthropic::request_body(request),
            if anthropic::takes_fallbacks(request.model) {
                2
            } else {
                1
            },
        ),
        // With the reasoning settings, the larger of the two bodies.
        ProviderId::Openai => (openai::request_body(request, true), 1),
    };
    let sent = body.to_string().len() as u64 + FRAMING_TOKENS;
    (sent * answers, u64::from(request.max_tokens) * answers)
}

/// What the provider billed for some requests, as it reported it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Billed {
    /// Requests whose reply reported its tokens.
    pub reported: usize,
    /// Those tokens.
    pub tokens: Usage,
    /// Requests that reported nothing (they failed, or the reply left it out). Each is counted
    /// at its worst case ([`worst_tokens`]).
    pub unreported: usize,
    pub unreported_sent: u64,
    pub unreported_received: u64,
}

impl Billed {
    pub fn add(&mut self, other: &Billed) {
        self.reported += other.reported;
        self.tokens.add(&other.tokens);
        self.unreported += other.unreported;
        self.unreported_sent += other.unreported_sent;
        self.unreported_received += other.unreported_received;
    }

    /// What it all costs at `prices`, in dollars: the reported tokens at their rates, and the
    /// requests that reported nothing at their worst case.
    pub fn cost(&self, prices: &Prices) -> f64 {
        prices.cost(&self.tokens) + prices.worst_case(self.unreported_sent, self.unreported_received)
    }
}

/// Why a run stopped before it was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stop {
    /// The next request's worst case would have passed the spending ceiling, so it wasn't sent.
    Budget,
    /// The run had made as many requests as it may.
    RequestLimit,
}

impl Stop {
    pub fn label(self) -> &'static str {
        match self {
            Stop::Budget => "budget",
            Stop::RequestLimit => "request limit",
        }
    }
}

/// What a budgeted run has spent so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spending {
    /// Requests sent.
    pub requests: usize,
    pub billed: Billed,
    /// Dollars: `billed` at the run's prices.
    pub spent: f64,
    pub stopped: Option<Stop>,
    /// The worst case of the request that wasn't sent, when the budget stopped the run.
    pub refused_worst_case: Option<f64>,
    /// The worst cases of the requests sent and not yet answered (a staged build sends several
    /// at once): held against the ceiling until each is charged what it really cost.
    #[serde(skip)]
    pub in_flight: f64,
}

/// A hard ceiling on what a run spends. Before every request, what has been spent, plus the worst
/// cases of the requests still on their way (a staged build sends several at once), plus the
/// most that request could cost ([`worst_tokens`] at the prices) must stay within the ceiling, or
/// it isn't sent and the run stops; after it, what the provider reports it used replaces its
/// worst case (which stands when it reports nothing). So the true total never passes the ceiling.
#[derive(Debug)]
pub struct Budget {
    ceiling: f64,
    prices: Prices,
    max_requests: Option<usize>,
    state: Mutex<Spending>,
}

impl Budget {
    /// `ceiling` dollars at `prices`, and at most `max_requests` requests when given.
    pub fn new(ceiling: f64, prices: Prices, max_requests: Option<usize>) -> Self {
        Self {
            ceiling,
            prices,
            max_requests,
            state: Mutex::default(),
        }
    }

    pub fn ceiling(&self) -> f64 {
        self.ceiling
    }

    pub fn prices(&self) -> &Prices {
        &self.prices
    }

    pub fn max_requests(&self) -> Option<usize> {
        self.max_requests
    }

    pub fn spending(&self) -> Spending {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Why the run stopped, once it has.
    pub fn stopped(&self) -> Option<Stop> {
        self.spending().stopped
    }

    /// Whether a request whose worst case is `worst` dollars may be sent. Once one is refused,
    /// none is.
    fn admit(&self, worst: f64) -> Result<(), Stop> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(stop) = state.stopped {
            return Err(stop);
        }
        if self.max_requests.is_some_and(|most| state.requests >= most) {
            state.stopped = Some(Stop::RequestLimit);
            return Err(Stop::RequestLimit);
        }
        let fits = state.spent + state.in_flight + worst <= self.ceiling;
        if !fits {
            state.stopped = Some(Stop::Budget);
            state.refused_worst_case = Some(worst);
            return Err(Stop::Budget);
        }
        state.requests += 1;
        state.in_flight += worst;
        Ok(())
    }

    /// Charges a request admitted with the worst case `worst` what it was billed.
    fn charge(&self, billed: &Billed, worst: f64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.billed.add(billed);
        state.spent = state.billed.cost(&self.prices);
        state.in_flight = (state.in_flight - worst).max(0.0);
    }
}

/// One request through a [`Metered`] provider.
struct Exchange {
    /// The conversation it belongs to, by its opening line.
    conversation: String,
    sent: usize,
    replied: usize,
    billed: Billed,
    started: Instant,
    ended: Instant,
}

/// A provider that counts what goes through it, and keeps to a [`Budget`] when given one.
pub struct Metered<'a> {
    inner: &'a dyn LlmProvider,
    budget: Option<&'a Budget>,
    exchanges: Mutex<Vec<Exchange>>,
    stopped: Mutex<Option<Stop>>,
}

impl<'a> Metered<'a> {
    pub fn new(inner: &'a dyn LlmProvider) -> Self {
        Self {
            inner,
            budget: None,
            exchanges: Mutex::default(),
            stopped: Mutex::default(),
        }
    }

    /// Sends a request only when `budget` can bear its worst case, and charges it what each
    /// reply reports.
    pub fn with_budget(mut self, budget: &'a Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn traffic(&self) -> Traffic {
        let exchanges = self.exchanges.lock().unwrap_or_else(PoisonError::into_inner);
        let mut traffic = Traffic::default();
        let mut before: BTreeMap<&str, usize> = BTreeMap::new();
        traffic.first_request_bytes = exchanges.first().map_or(0, |e| e.sent);
        for exchange in exchanges.iter() {
            let sent = exchange.sent;
            traffic.requests += 1;
            traffic.sent_bytes += sent;
            // A conversation only grows, so what its last request sent starts this one.
            let last = before.entry(exchange.conversation.as_str()).or_default();
            traffic.repeated_bytes += (*last).min(sent);
            *last = sent;
            traffic.reply_bytes += exchange.replied;
            traffic.largest_request_bytes = traffic.largest_request_bytes.max(sent);
        }
        traffic
    }

    /// What the provider reported for the requests made.
    pub fn billed(&self) -> Billed {
        let mut billed = Billed::default();
        for exchange in self
            .exchanges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            billed.add(&exchange.billed);
        }
        billed
    }

    /// The tokens the first request was billed for, as the provider reported them.
    pub fn first_billed(&self) -> Option<u64> {
        let exchanges = self.exchanges.lock().unwrap_or_else(PoisonError::into_inner);
        let first = exchanges.first()?;
        (first.billed.reported > 0).then(|| first.billed.tokens.sent())
    }

    /// From the first request sent to the last reply, in milliseconds.
    pub fn wall_ms(&self) -> u64 {
        let exchanges = self.exchanges.lock().unwrap_or_else(PoisonError::into_inner);
        let start = exchanges.iter().map(|e| e.started).min();
        let end = exchanges.iter().map(|e| e.ended).max();
        start
            .zip(end)
            .map_or(0, |(s, e)| e.saturating_duration_since(s).as_millis() as u64)
    }

    /// Why this meter refused a request, if it did.
    pub fn stopped(&self) -> Option<Stop> {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The first line of a conversation's first message: what tells a staged build's conversations
/// apart.
fn opening(messages: &[Message]) -> &str {
    messages
        .iter()
        .find_map(|m| match m {
            Message::User(text) => text.lines().next(),
            _ => None,
        })
        .unwrap_or_default()
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
        let (worst_sent, worst_received) = worst_tokens(self.inner.id(), request);
        let mut worst = 0.0;
        if let Some(budget) = self.budget {
            worst = budget.prices().worst_case(worst_sent, worst_received);
            if let Err(stop) = budget.admit(worst) {
                *self.stopped.lock().unwrap_or_else(PoisonError::into_inner) = Some(stop);
                // The chat loop stops as if the user had pressed Stop; nothing was sent.
                return Err(AiError::Cancelled);
            }
        }
        let sent = request_bytes(request);
        let started = Instant::now();
        let answered = self.inner.stream_turn_with_usage(key, request, cancel, on_event);
        let billed = match &answered {
            Ok((_, Some(usage))) => Billed {
                reported: 1,
                tokens: *usage,
                ..Billed::default()
            },
            // No usage (or no reply): what it may have cost is all that can be said.
            _ => Billed {
                unreported: 1,
                unreported_sent: worst_sent,
                unreported_received: worst_received,
                ..Billed::default()
            },
        };
        if let Some(budget) = self.budget {
            budget.charge(&billed, worst);
        }
        let replied = answered.as_ref().map_or(0, |(turn, _)| turn_bytes(turn));
        self.exchanges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Exchange {
                conversation: opening(request.messages).to_string(),
                sent,
                replied,
                billed,
                started,
                ended: Instant::now(),
            });
        answered
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
    /// Model requests the answer took, every stage's included.
    pub requests: usize,
    /// Of those, the most that had to happen one after another: what a person waits through.
    pub critical_path: usize,
    /// Whether the job was built in stages.
    pub staged: bool,
    /// From the first request to the last reply, in milliseconds.
    pub wall_ms: u64,
    pub stopped_at_step_limit: bool,
    pub tool_calls: BTreeMap<String, usize>,
    /// Tool calls refused, each with the start of the reason.
    pub tool_errors: Vec<String>,
    /// The guides asked for in answered calls, in order (a guide asked for again shows twice,
    /// though the second answer only points back at the first).
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
    /// The tokens the provider reported for these requests.
    pub billed: Billed,
    /// The tokens the provider reported for the first request, when it did.
    pub first_request_billed: Option<u64>,
    /// The song's analysis came with the request, in its context block.
    pub song_up_front: bool,
}

impl Measures {
    /// Tool calls made, of every kind, in every stage.
    pub fn calls(&self) -> usize {
        self.tool_calls.values().sum()
    }

    /// Tokens sent with the first request, and with a request on average: as billed when the
    /// provider reported them, else estimated from sizes. The flag says which.
    pub fn request_tokens(&self) -> (u64, u64, bool) {
        let b = &self.billed;
        match self.first_request_billed {
            Some(first) if b.reported > 0 && b.unreported == 0 => {
                (first, b.tokens.sent() / b.reported as u64, true)
            }
            _ => (
                tokens_of(self.traffic.first_request_bytes),
                self.traffic.mean_request_tokens(),
                false,
            ),
        }
    }
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
    /// Set when the run's budget or request limit stopped this scenario before its next request.
    pub stopped: Option<Stop>,
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

fn tool_uses(stage: &str, messages: &[Message]) -> Vec<ToolUse> {
    let mut uses: Vec<(String, ToolUse)> = Vec::new();
    let mut request = 0;
    for message in messages {
        match message {
            Message::Assistant(turn) => {
                request += 1;
                uses.extend(turn.tool_calls.iter().map(|call| {
                    (
                        call.id.clone(),
                        ToolUse {
                            name: call.name.clone(),
                            input: call.input.clone(),
                            answer: String::new(),
                            is_error: false,
                            given: false,
                            stage: stage.to_string(),
                            request,
                        },
                    )
                }));
            }
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

fn measure(
    run: &Run<'_>,
    proposed: bool,
    metered: &Metered<'_>,
    session: &ChatSession,
    cancel: &Cancel,
) -> Measures {
    let messages = session.messages();
    let (traffic, billed) = (metered.traffic(), metered.billed());
    let song_up_front = messages.iter().any(|m| match m {
        Message::User(text) => text.contains(crate::context::SONG_LINE),
        _ => false,
    });
    let (doc, lights, song) = (run.after, run.lights, run.song);
    let mut tool_calls: BTreeMap<String, usize> = BTreeMap::new();
    let mut guides = Vec::new();
    for tool in run.tools {
        if !tool.given {
            *tool_calls.entry(tool.name.clone()).or_default() += 1;
        }
        // Guides asked for and answered (a refused ask sent none).
        if tool.name == "get_guide" && !tool.is_error {
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
    let stats = session.last_turn();
    Measures {
        proposed,
        requests: traffic.requests,
        critical_path: stats.critical_path,
        staged: stats.staged,
        wall_ms: metered.wall_ms(),
        stopped_at_step_limit: traffic.requests >= MAX_STEPS,
        tool_errors: run
            .tools
            .iter()
            .filter(|t| t.is_error && !t.given)
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
        billed,
        first_request_billed: metered.first_billed(),
        song_up_front,
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
    run_within(scenario, stage, provider, key, model, cancel, None)
}

/// [`run`], keeping to `budget` when given: a request it can't bear isn't sent, and the scenario
/// stops there ([`Outcome::stopped`]).
pub fn run_within(
    scenario: &Scenario,
    stage: &Stage,
    provider: &dyn LlmProvider,
    key: &ApiKey,
    model: &str,
    cancel: &Cancel,
    budget: Option<&Budget>,
) -> Outcome {
    run_with(
        scenario,
        stage,
        provider,
        key,
        model,
        cancel,
        budget,
        Flow::Single,
    )
}

/// [`run_within`], the chat doing big jobs as `flow` says.
#[allow(clippy::too_many_arguments)]
pub fn run_with(
    scenario: &Scenario,
    stage: &Stage,
    provider: &dyn LlmProvider,
    key: &ApiKey,
    model: &str,
    cancel: &Cancel,
    budget: Option<&Budget>,
    flow: Flow,
) -> Outcome {
    let song = stage::song();
    let before = (scenario.start)(stage);
    let mut session = ChatSession::new()
        .with_flow(flow)
        .with_analyzer(Arc::new(|_: &Path, _: &Cancel| Ok(stage::song())));
    let metered = match budget {
        Some(budget) => Metered::new(provider).with_budget(budget),
        None => Metered::new(provider),
    };
    let answered = session.run_turn(
        &metered,
        key,
        model,
        scenario.request,
        workspace(stage, &before),
        cancel,
        &mut |_| {},
    );
    let stopped = metered.stopped();
    let (reply, proposed, error) = match answered {
        Ok(reply) => (reply.text, reply.proposal.is_some(), None),
        // What was drafted before the failure is still proposed, and still measured.
        Err(e) => (
            String::new(),
            session.proposal().is_some(),
            // Stopped by the budget: nothing failed.
            stopped.is_none().then(|| e.to_string()),
        ),
    };
    let after = session
        .draft()
        .and_then(|d| d.sequence())
        .cloned()
        .unwrap_or_else(|| before.clone());
    let tools = transcript(&session);
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
    let measures = measure(&seen, proposed, &metered, &session, cancel);
    let checks = (scenario.judge)(&seen, &measures);
    Outcome {
        scenario: scenario.key.to_string(),
        title: scenario.title.to_string(),
        request: scenario.request.to_string(),
        error,
        stopped,
        reply,
        checks,
        measures,
        transcript: tools,
    }
}

/// Every tool call of the last message, the chat's own and then each stage's in turn (a stage's
/// guides given with its request first, marked as given).
pub fn transcript(session: &ChatSession) -> Vec<ToolUse> {
    let mut uses = tool_uses("chat", session.messages());
    for stage in session.stages() {
        let (topics, effects): (Vec<&str>, Vec<&str>) = stage
            .given
            .iter()
            .map(String::as_str)
            .partition(|g| g.starts_with("topic:"));
        if !stage.given.is_empty() {
            uses.push(ToolUse {
                name: "get_guide".into(),
                input: serde_json::json!({
                    "topics": topics.iter().map(|t| t.trim_start_matches("topic:")).collect::<Vec<_>>(),
                    "effects": effects.iter().map(|e| e.trim_start_matches("effect:")).collect::<Vec<_>>(),
                }),
                answer: format!("(given with the {} request)", stage.stage),
                is_error: false,
                given: true,
                stage: stage.stage.clone(),
                request: 0,
            });
        }
        uses.extend(tool_uses(&stage.stage, &stage.messages));
    }
    uses
}

/// What a run sends a model, in words, with the sizes that never change.
pub fn what_is_sent(flow: Flow) -> String {
    let stage = Stage::new();
    let mut specs = Toolbox::default().core_specs();
    if flow == Flow::Staged {
        specs.push(crate::staged::plan_build_tool());
    }
    let tools: usize = specs
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();
    let fixed = instructions(flow).len() + tools;
    let design = crate::staged::catalogue::design_system().len();
    let stages = if flow == Flow::Staged {
        format!(
            "\n - for a job built in stages: a design request with the stages' instructions, a catalogue of every tool and effect, and the show-design guides ({design} bytes, about {} tokens, the same every time), with the brief and the show and song in outline; then a request for each step of the plan, with only its tools, its stretch of the song, its props, and its effects' guides (and what its part still lacks, when it is asked again); then a refine request or two with what of the plan is still dark and the review's fixes;",
            (design as f64 / BYTES_PER_TOKEN).round(),
        )
    } else {
        String::new()
    };
    let props: Vec<&str> = stage.show.props.iter().map(|p| p.name.as_str()).collect();
    let groups: Vec<&str> = stage.show.groups.iter().map(|g| g.name.as_str()).collect();
    let lines = crate::stage::lyric_tracks().first().map_or(0, |t| t.marks.len());
    format!(
        "What a run sends the model's provider:\n\
         - the Assistant's instructions and its {} core tool definitions, with every request ({} bytes, about {} tokens), and more tools in groups when the model asks for them;\n\
         - with the request, the made-up show, sequence, and song below in outline;{}\n\
         - each scenario's request, as a user would type it;\n\
         - whatever the model looks up with its tools: PixelFlow's own guides, and a made-up show and song that exist only in this harness:\n\
         \x20   props: {};\n\
         \x20   groups: {};\n\
         \x20   a song called \"Paper Lanterns\" that was never recorded: its tempo, sections, and moments are written by hand, and so are its {} lines of words.\n\
         Nothing of yours is read or sent: no show, no sequence, no song, no lyrics, no file, no device.\n\
         The key is read from the {} environment variable only for a real run, and goes only to the provider, as the sign-in for each request.",
        specs.len(),
        fixed,
        (fixed as f64 / BYTES_PER_TOKEN).round(),
        stages,
        props.join(", "),
        groups.join(", "),
        lines,
        cli::KEY_VARIABLE,
    )
}

/// What the provider reported for some requests, and what that costs at `prices`, in a sentence.
pub fn billed_line(billed: &Billed, prices: &Prices) -> String {
    let t = &billed.tokens;
    let mut line = format!(
        "Billed, as the provider reported it: {} of {} requests reported their tokens: {} sent ({} at the full price, {} read from the cache, {} written to it) and {} received, ${:.4} at the prices.",
        billed.reported,
        billed.reported + billed.unreported,
        t.sent(),
        t.input,
        t.cache_read,
        t.cache_write,
        t.output,
        prices.cost(t),
    );
    if billed.unreported > 0 {
        line.push_str(&format!(
            " {} reported nothing, and {} counted at the most {} could have cost ({} tokens sent, {} received): ${:.4}.",
            billed.unreported,
            if billed.unreported == 1 { "is" } else { "are" },
            if billed.unreported == 1 { "it" } else { "they" },
            billed.unreported_sent,
            billed.unreported_received,
            prices.worst_case(billed.unreported_sent, billed.unreported_received),
        ));
    }
    line.push_str(&format!(" Total: ${:.4}.", billed.cost(prices)));
    line
}

/// What a person waits for, per scenario: the round trips to the model, the tool calls, and how
/// much each request carried (tokens as billed where the provider reported them, else
/// estimated from sizes and marked `~`).
fn round_trips(outcomes: &[Outcome]) -> String {
    let mut out = String::from(
        "Round trips and request sizes (tokens; ~ estimated from sizes). Requests counts every request; the critical path, those that had to happen one after another (a staged build's steps run at the same time), which is what a person waits through:\n\n| Scenario | Staged | Requests | Critical path | Tool calls | First request | Mean request | Largest request | Wall clock |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    for o in outcomes {
        let m = &o.measures;
        let (first, mean, billed) = m.request_tokens();
        let mark = if billed { "" } else { "~" };
        // A scripted stand-in answers at once: its clock says nothing.
        let wall = if billed {
            format!("{:.1} s", m.wall_ms as f64 / 1000.0)
        } else {
            "-".to_string()
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {mark}{first} | {mark}{mean} | ~{} | {wall} |\n",
            o.scenario,
            if m.staged { "yes" } else { "no" },
            m.requests,
            m.critical_path,
            m.calls(),
            tokens_of(m.traffic.largest_request_bytes),
        ));
    }
    out.push('\n');
    out
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
    let mut billed = Billed::default();
    for outcome in outcomes {
        traffic.add(&outcome.measures.traffic);
        billed.add(&outcome.measures.billed);
    }
    let (sent, received) = traffic.tokens();
    let (full, cached) = traffic.cost(prices);
    out.push_str(&format!(
        "{passed} of {total} checks passed over {} scenarios. Mean review score {}.\n\n",
        outcomes.len(),
        if scores.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.0}", scores.iter().sum::<u32>() as f32 / scores.len() as f32)
        },
    ));
    // A scripted stand-in reports no usage: then there is nothing billed to show.
    let reported = billed.reported > 0;
    if reported {
        out.push_str(&format!("{}\n\n", billed_line(&billed, prices)));
    } else {
        out.push_str("No reply reported the tokens it was billed for.\n\n");
    }
    out.push_str(&format!(
        "Estimated from sizes: {} requests, about {sent} tokens sent and {received} received: about ${full:.2} at ${}/${} a million tokens in/out, or ${cached:.2} if the provider caches what each request repeats (at {} of the price). The prices are assumptions: set them with --price-in, --price-out, --price-cached, --price-cache-write.\n\n",
        traffic.requests,
        prices.input,
        prices.output,
        prices.cached,
    ));
    out.push_str(&round_trips(outcomes));
    out.push_str(
        "| Scenario | Checks | Review | Props | Kinds | Dark | Full | Changes | Moments | Safe | Requests | Tool errors | Cost |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for o in outcomes {
        let m = &o.measures;
        let review = m.review.as_ref();
        out.push_str(&format!(
            "| {}{} | {}/{} | {} | {}/{} | {} | {:.0}% | {:.0}% | {}/{} | {} | {} | {} | {} | {} |\n",
            o.scenario,
            o.stopped
                .map_or(String::new(), |s| format!(" (stopped: {})", s.label())),
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
            if reported {
                format!("${:.4}", m.billed.cost(prices))
            } else {
                "-".to_string()
            },
        ));
    }
    for o in outcomes {
        out.push_str(&format!(
            "\n## {} ({})\n\n> {}\n\n",
            o.title, o.scenario, o.request
        ));
        match o.stopped {
            Some(Stop::Budget) => out.push_str(
                "Stopped: budget. The next request's worst case would have passed the spending ceiling, so it was not sent, and no later scenario was started.\n\n",
            ),
            Some(Stop::RequestLimit) => out.push_str(
                "Stopped: request limit. The run had made as many requests as --max-requests allows; no later scenario was started.\n\n",
            ),
            None => {}
        }
        if let Some(error) = &o.error {
            out.push_str(&format!("Stopped early: {error}\n\n"));
        }
        if reported {
            out.push_str(&format!("{}\n\n", billed_line(&o.measures.billed, prices)));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn billed(input: u64) -> Billed {
        Billed {
            reported: 1,
            tokens: Usage {
                input,
                ..Usage::default()
            },
            ..Billed::default()
        }
    }

    #[test]
    fn requests_on_their_way_hold_their_worst_case_against_the_ceiling() {
        // Steps built at once send several requests before any is charged.
        let budget = Budget::new(1.0, Prices::default(), None);
        assert_eq!(budget.admit(0.6), Ok(()));
        assert_eq!(budget.admit(0.3), Ok(()));
        assert_eq!(budget.admit(0.2), Err(Stop::Budget));

        // Once charged what it really cost, a request's worst case is let go.
        let budget = Budget::new(1.0, Prices::default(), None);
        assert_eq!(budget.admit(0.6), Ok(()));
        budget.charge(&billed(1_000), 0.6);
        assert_eq!(budget.admit(0.6), Ok(()));
        assert!(budget.spending().spent < 0.01);
    }
}
