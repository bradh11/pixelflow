//! The provider-neutral conversation: messages, tool calls, streamed events, and the
//! [`LlmProvider`] trait that Anthropic and OpenAI implement.

use crate::error::AiError;
use crate::secret::ApiKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Which AI company the user brings a key for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Anthropic,
    Openai,
}

impl ProviderId {
    pub const ALL: [ProviderId; 2] = [ProviderId::Anthropic, ProviderId::Openai];

    /// The name people know it by.
    pub fn label(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "Anthropic",
            ProviderId::Openai => "OpenAI",
        }
    }

    /// The account name its key is saved under in the OS credential store.
    pub fn account(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "anthropic",
            ProviderId::Openai => "openai",
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A chat model the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    /// What to show in the picker.
    pub name: String,
    /// The one PixelFlow suggests when nothing is picked yet.
    pub recommended: bool,
}

/// A tool the model may call: a name, what it does, and a JSON Schema for its input.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// The model asking to run a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
    /// Set when the model's input wasn't valid JSON (then `input` is `null`): the call is answered
    /// with this as an error instead of being run.
    pub input_error: Option<String>,
}

/// Why the model stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    /// The reply hit the output limit (a tool call may be cut off: never run it).
    MaxTokens,
    /// The model (or the provider's safety system) declined.
    Refusal,
    Other(String),
}

/// One reply from the model.
#[derive(Debug, Clone, PartialEq)]
pub struct AssistantTurn {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop: StopReason,
    /// The reply as the provider sent it (e.g. Anthropic content blocks with thinking
    /// signatures), echoed back unchanged on the next request to the same provider.
    pub native: Option<(ProviderId, Value)>,
}

/// A tool's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// The conversation, in order. Only ever appended to while a provider is answering, so a
/// provider's cached prefix (and any thinking it carries) stays valid.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    User(String),
    Assistant(AssistantTurn),
    ToolResults(Vec<ToolResult>),
}

/// Everything one model request needs.
#[derive(Debug, Clone, Copy)]
pub struct TurnRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub tools: &'a [ToolSpec],
    pub messages: &'a [Message],
    pub max_tokens: u32,
}

/// What arrives while a reply streams in.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// More of the reply's text.
    Text(String),
    /// The model started a tool call.
    ToolStarted { name: String },
    /// The provider was busy; trying again after a pause.
    Retrying { attempt: u32, wait_ms: u64 },
}

/// Lets the user stop a request in progress (the panel's Stop button).
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// `Err(Cancelled)` once stopped, so loops can `?` it.
    pub fn check(&self) -> Result<(), AiError> {
        if self.is_cancelled() {
            Err(AiError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// An AI provider: lists its chat models and streams replies with tool calls. Every call is
/// made from Rust with the user's key; the window never sees the key or talks to the provider.
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// The models this key can use that can chat and call tools, best first.
    fn list_models(&self, key: &ApiKey, cancel: &Cancel) -> Result<Vec<ModelInfo>, AiError>;

    /// Streams one reply, calling `on_event` as text and tool calls arrive.
    fn stream_turn(
        &self,
        key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantTurn, AiError>;
}
