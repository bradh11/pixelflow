//! Test doubles: recorded HTTP replies and a scripted provider. Nothing here touches the network.

use crate::error::AiError;
use crate::http::{HttpRequest, HttpResponse, Transport, TransportError};
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StopReason, StreamEvent, ToolCall,
    TurnRequest, Usage,
};
use crate::secret::ApiKey;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// An obviously fake key for tests.
pub const FAKE_KEY: &str = "sk-test-not-a-key";

pub fn fake_key() -> ApiKey {
    ApiKey::new(FAKE_KEY).expect("valid")
}

/// One recorded reply.
#[derive(Debug, Clone)]
pub enum Reply {
    Response {
        status: u16,
        body: String,
        retry_after: Option<Duration>,
    },
    Unreachable,
    Timeout,
    /// Timed out before the request was sent.
    ConnectTimeout,
    /// The connection broke (perhaps after the request was sent).
    Failed,
}

impl Reply {
    pub fn ok(body: impl Into<String>) -> Self {
        Self::status(200, body)
    }

    pub fn status(status: u16, body: impl Into<String>) -> Self {
        Reply::Response {
            status,
            body: body.into(),
            retry_after: None,
        }
    }

    pub fn with_retry_after(self, seconds: u64) -> Self {
        match self {
            Reply::Response { status, body, .. } => Reply::Response {
                status,
                body,
                retry_after: Some(Duration::from_secs(seconds)),
            },
            other => other,
        }
    }
}

/// Answers requests with recorded replies, in order, and remembers every request.
#[derive(Debug, Default)]
pub struct FakeTransport {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl FakeTransport {
    pub fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::default(),
        }
    }

    /// Adds replies for later requests.
    pub fn push(&self, reply: Reply) {
        self.replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(reply);
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The JSON body of request `index`.
    pub fn body(&self, index: usize) -> Value {
        let requests = self.requests();
        serde_json::from_str(
            requests[index]
                .body
                .as_ref()
                .and_then(|b| b.text())
                .unwrap_or("null"),
        )
        .expect("JSON body")
    }
}

impl Transport for FakeTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        let reply = self
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Reply::Unreachable);
        match reply {
            Reply::Response {
                status,
                body,
                retry_after,
            } => Ok(HttpResponse {
                status,
                retry_after,
                body: Box::new(Cursor::new(body.into_bytes())),
            }),
            Reply::Unreachable => Err(TransportError::Unreachable),
            Reply::Timeout => Err(TransportError::Timeout),
            Reply::ConnectTimeout => Err(TransportError::ConnectTimeout),
            Reply::Failed => Err(TransportError::Failed),
        }
    }
}

type Scripted = Result<AssistantTurn, AiError>;

/// One request as a [`ScriptedProvider`] saw it.
#[derive(Debug, Clone)]
pub struct Seen {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<String>,
    pub max_tokens: u32,
}

impl Seen {
    /// The first line of the conversation's first message ("Stage: design" in a stage's).
    pub fn opening(&self) -> &str {
        opening(&self.messages)
    }
}

fn opening(messages: &[Message]) -> &str {
    messages
        .iter()
        .find_map(|m| match m {
            Message::User(text) => text.lines().next(),
            _ => None,
        })
        .unwrap_or_default()
}

/// A provider that answers with scripted turns, and remembers what it was asked. A staged
/// build's stages each have their own script ([`ScriptedProvider::stage`]), found by the
/// conversation's first line, so steps built at the same time each get their own answers
/// whatever order they ask in; every other conversation takes the main script in order.
#[derive(Debug, Default)]
pub struct ScriptedProvider {
    turns: Mutex<VecDeque<Scripted>>,
    /// Each stage's script, by its conversation's first line.
    stages: Mutex<Vec<(String, VecDeque<Scripted>)>>,
    /// What each reply reports it used, in order (none once these run out).
    usage: Mutex<VecDeque<Option<Usage>>>,
    seen: Mutex<Vec<Seen>>,
}

impl ScriptedProvider {
    pub fn new(turns: Vec<Scripted>) -> Self {
        Self {
            turns: Mutex::new(turns.into()),
            ..Self::default()
        }
    }

    /// Answers the conversation of `stage` ("design", [`crate::staged::step_stage`], "refine")
    /// with `turns`, in order.
    pub fn stage(self, stage: &str, turns: Vec<Scripted>) -> Self {
        self.stages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((crate::staged::stage_line(stage), turns.into()));
        self
    }

    /// Every request, as seen, in the order they came.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The requests of `stage`'s conversation.
    pub fn seen_in(&self, stage: &str) -> Vec<Seen> {
        let line = crate::staged::stage_line(stage);
        self.seen().into_iter().filter(|s| s.opening() == line).collect()
    }

    /// Reports `usage` with each reply, in order, as a real provider would (`None`: that reply
    /// reports nothing, as when a provider leaves it out).
    pub fn reporting(self, usage: Vec<Option<Usage>>) -> Self {
        *self.usage.lock().unwrap_or_else(PoisonError::into_inner) = usage.into();
        self
    }

    /// The output limit each request asked for.
    pub fn max_tokens_seen(&self) -> Vec<u32> {
        self.seen().into_iter().map(|s| s.max_tokens).collect()
    }

    /// The conversation as sent with each request.
    pub fn requests(&self) -> Vec<Vec<Message>> {
        self.seen().into_iter().map(|s| s.messages).collect()
    }

    /// Tool names offered with the last request.
    pub fn tools_offered(&self) -> Vec<String> {
        self.tools_offered_each().pop().unwrap_or_default()
    }

    /// Tool names offered with each request, in order.
    pub fn tools_offered_each(&self) -> Vec<Vec<String>> {
        self.seen().into_iter().map(|s| s.tools).collect()
    }

    /// The next scripted turn for a conversation opening with `line`.
    fn next_turn(&self, line: &str) -> Scripted {
        let mut stages = self.stages.lock().unwrap_or_else(PoisonError::into_inner);
        let turn = match stages.iter_mut().find(|(key, _)| key == line) {
            Some((_, turns)) => turns.pop_front(),
            None => self
                .turns
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop_front(),
        };
        turn.unwrap_or_else(|| says("(the script ran out)"))
    }
}

/// A reply with text only.
pub fn says(text: &str) -> Result<AssistantTurn, AiError> {
    Ok(AssistantTurn {
        text: text.to_string(),
        tool_calls: Vec::new(),
        stop: StopReason::EndTurn,
        native: None,
    })
}

/// A reply calling tools: `(name, input)` pairs, numbered call ids.
pub fn calls(text: &str, tools: &[(&str, Value)]) -> Result<AssistantTurn, AiError> {
    Ok(AssistantTurn {
        text: text.to_string(),
        tool_calls: tools
            .iter()
            .enumerate()
            .map(|(i, (name, input))| ToolCall {
                id: format!("call_{i}_{name}"),
                name: name.to_string(),
                input: input.clone(),
                input_error: None,
            })
            .collect(),
        stop: StopReason::ToolUse,
        native: None,
    })
}

impl LlmProvider for ScriptedProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn list_models(&self, _key: &ApiKey, _cancel: &Cancel) -> Result<Vec<ModelInfo>, AiError> {
        Ok(vec![ModelInfo {
            id: "scripted".into(),
            name: "Scripted".into(),
            recommended: true,
        }])
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
        _key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<(AssistantTurn, Option<Usage>), AiError> {
        cancel.check()?;
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Seen {
                system: request.system.to_string(),
                messages: request.messages.to_vec(),
                tools: request.tools.iter().map(|t| t.name.clone()).collect(),
                max_tokens: request.max_tokens,
            });
        let usage = self
            .usage
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .flatten();
        let turn = self.next_turn(opening(request.messages))?;
        if !turn.text.is_empty() {
            on_event(StreamEvent::Text(turn.text.clone()));
        }
        for call in &turn.tool_calls {
            on_event(StreamEvent::ToolStarted {
                name: call.name.clone(),
            });
        }
        Ok((turn, usage))
    }
}
