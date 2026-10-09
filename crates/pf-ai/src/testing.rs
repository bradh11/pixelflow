//! Test doubles: recorded HTTP replies and a scripted provider. Nothing here touches the network.

use crate::error::AiError;
use crate::http::{HttpRequest, HttpResponse, Transport, TransportError};
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StopReason, StreamEvent, ToolCall,
    TurnRequest,
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

/// A provider that answers with scripted turns, and remembers what it was asked.
#[derive(Debug, Default)]
pub struct ScriptedProvider {
    turns: Mutex<VecDeque<Result<AssistantTurn, AiError>>>,
    seen: Mutex<Vec<Vec<Message>>>,
    tools_seen: Mutex<Vec<String>>,
}

impl ScriptedProvider {
    pub fn new(turns: Vec<Result<AssistantTurn, AiError>>) -> Self {
        Self {
            turns: Mutex::new(turns.into()),
            ..Self::default()
        }
    }

    /// The conversation as sent with each request.
    pub fn requests(&self) -> Vec<Vec<Message>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Tool names offered with the last request.
    pub fn tools_offered(&self) -> Vec<String> {
        self.tools_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
        _key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantTurn, AiError> {
        cancel.check()?;
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.messages.to_vec());
        *self.tools_seen.lock().unwrap_or_else(PoisonError::into_inner) =
            request.tools.iter().map(|t| t.name.clone()).collect();
        let turn = self
            .turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| says("(the script ran out)"))?;
        if !turn.text.is_empty() {
            on_event(StreamEvent::Text(turn.text.clone()));
        }
        for call in &turn.tool_calls {
            on_event(StreamEvent::ToolStarted {
                name: call.name.clone(),
            });
        }
        Ok(turn)
    }
}
