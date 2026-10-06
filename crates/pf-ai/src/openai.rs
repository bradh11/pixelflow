//! OpenAI's Chat Completions API: streamed replies with function calling, and the models list.
//!
//! Requests go to `POST /v1/chat/completions` with `stream: true` (`Authorization: Bearer`).
//! Replies arrive as unnamed server-sent events, each a `chat.completion.chunk` whose
//! `choices[0].delta` carries `content` and `tool_calls` fragments (by `index`; the first
//! fragment has the call's `id` and `function.name`, later ones add to `function.arguments`),
//! and `finish_reason` at the end; the stream ends with `data: [DONE]`.
//!
//! **Which models are offered.** `GET /v1/models` lists every model the key can use without
//! saying which can chat or call functions, so PixelFlow keeps only chat-capable families and
//! drops the rest by name (see [`is_chat_tool_model`]): ids must start with `gpt-` or be an
//! `o`-series reasoning model (`o1`, `o3`, `o4-mini`, ...), and ids naming audio, realtime,
//! speech, transcription, images, embeddings, moderation, search, instruct, or Responses-only
//! (`-pro`, `codex`, `deep-research`, `computer-use`) variants are dropped, as are `o1-mini` and
//! `o1-preview` (no function calling). Anything that slips through is caught when used: OpenAI's
//! "doesn't support tools" reply becomes a plain "pick another model" message.

use crate::error::{AiError, sanitize};
use crate::http::{
    HeaderValue, HttpRequest, Method, RetryPolicy, SendError, Transport, TransportError, send_with_retries,
};
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StopReason, StreamEvent, ToolCall,
    TurnRequest,
};
use crate::secret::ApiKey;
use crate::sse::SseReader;
use serde_json::{Value, json};
use std::sync::Arc;

pub const BASE_URL: &str = "https://api.openai.com";

pub struct OpenAi {
    transport: Arc<dyn Transport>,
    base_url: String,
    retry: RetryPolicy,
}

impl OpenAi {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            base_url: BASE_URL.to_string(),
            retry: RetryPolicy::default(),
        }
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    fn send(
        &self,
        request: &HttpRequest,
        key: &ApiKey,
        model: Option<&str>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<crate::http::HttpResponse, AiError> {
        send_with_retries(
            self.transport.as_ref(),
            request,
            self.retry,
            cancel,
            // "You exceeded your current quota" comes as a 429 but waiting won't help.
            &|reply| !reply.body.contains("insufficient_quota"),
            &mut |attempt, wait| {
                on_event(StreamEvent::Retrying {
                    attempt,
                    wait_ms: wait.as_millis() as u64,
                })
            },
        )
        .map_err(|e| send_error(e, key, model))
    }
}

fn auth(key: &ApiKey) -> (&'static str, HeaderValue) {
    (
        "authorization",
        HeaderValue::SecretWithPrefix("Bearer ", key.clone()),
    )
}

/// The request body for one turn.
pub fn request_body(request: &TurnRequest<'_>) -> Value {
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect();
    let mut messages = vec![json!({ "role": "system", "content": request.system })];
    messages.extend(history(request.messages));
    json!({
        "model": request.model,
        "stream": true,
        "max_completion_tokens": request.max_tokens,
        "messages": messages,
        "tools": tools,
    })
}

fn history(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message {
            Message::User(text) => out.push(json!({ "role": "user", "content": text })),
            Message::Assistant(turn) => {
                if turn.text.is_empty() && turn.tool_calls.is_empty() {
                    continue;
                }
                let mut entry = json!({
                    "role": "assistant",
                    "content": if turn.text.is_empty() { Value::Null } else { json!(turn.text) },
                });
                if !turn.tool_calls.is_empty() {
                    entry["tool_calls"] = turn
                        .tool_calls
                        .iter()
                        .map(|call| {
                            let arguments = if call.input.is_object() {
                                call.input.to_string()
                            } else {
                                "{}".to_string()
                            };
                            json!({
                                "id": call.id,
                                "type": "function",
                                "function": { "name": call.name, "arguments": arguments },
                            })
                        })
                        .collect();
                }
                out.push(entry);
            }
            Message::ToolResults(results) => {
                for result in results {
                    let content = if result.is_error {
                        format!("Error: {}", result.content)
                    } else {
                        result.content.clone()
                    };
                    out.push(json!({ "role": "tool", "tool_call_id": result.call_id, "content": content }));
                }
            }
        }
    }
    out
}

fn send_error(error: SendError, key: &ApiKey, model: Option<&str>) -> AiError {
    let p = ProviderId::Openai;
    match error {
        SendError::Cancelled => AiError::Cancelled,
        SendError::Transport(TransportError::Timeout) => AiError::Timeout(p),
        SendError::Transport(_) => AiError::Network(p),
        SendError::Status(reply) => {
            let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
            status_error(reply.status, &value["error"], key, model)
        }
    }
}

/// OpenAI's `{"error": {"message", "type", "code"}}` as a plain message.
fn status_error(status: u16, error: &Value, key: &ApiKey, model: Option<&str>) -> AiError {
    let p = ProviderId::Openai;
    let message = error["message"].as_str().unwrap_or_default();
    let code = error["code"].as_str().unwrap_or_default();
    let lower = message.to_ascii_lowercase();
    let model = model.unwrap_or("").to_string();
    let no_tools = (lower.contains("tool") || lower.contains("function"))
        && (lower.contains("not supported")
            || lower.contains("does not support")
            || lower.contains("unsupported"));
    match status {
        401 => AiError::InvalidKey(p),
        _ if code == "insufficient_quota" || code == "billing_hard_limit_reached" => AiError::Billing(p),
        _ if code == "model_not_found" => AiError::ModelNotFound { provider: p, model },
        _ if no_tools || lower.contains("not a chat model") => AiError::ModelNoTools { provider: p, model },
        _ if code == "context_length_exceeded" || lower.contains("maximum context length") => {
            AiError::TooLong
        }
        403 => AiError::PermissionDenied(p),
        404 => AiError::ModelNotFound { provider: p, model },
        413 => AiError::TooLong,
        429 => AiError::RateLimited(p),
        500..=599 => AiError::Overloaded(p),
        _ => AiError::Provider {
            provider: p,
            message: sanitize(message, Some(key.expose())),
        },
    }
}

/// Whether a model id from `GET /v1/models` is a chat model that can call functions. See the
/// module docs for the rule.
pub fn is_chat_tool_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    let o_series = id.len() >= 2 && id.starts_with('o') && id.as_bytes()[1].is_ascii_digit();
    if !(id.starts_with("gpt-") || o_series) {
        return false;
    }
    const DROPPED: &[&str] = &[
        "audio",
        "realtime",
        "tts",
        "transcribe",
        "whisper",
        "image",
        "dall-e",
        "embedding",
        "moderation",
        "search",
        "instruct",
        "-pro",
        "codex",
        "deep-research",
        "computer-use",
    ];
    if DROPPED.iter().any(|word| id.contains(word)) {
        return false;
    }
    !(id.starts_with("o1-mini") || id.starts_with("o1-preview"))
}

impl LlmProvider for OpenAi {
    fn id(&self) -> ProviderId {
        ProviderId::Openai
    }

    fn list_models(&self, key: &ApiKey, cancel: &Cancel) -> Result<Vec<ModelInfo>, AiError> {
        let request = HttpRequest {
            method: Method::Get,
            url: format!("{}/v1/models", self.base_url),
            headers: vec![auth(key)],
            body: None,
        };
        let mut response = self.send(&request, key, None, cancel, &mut |_| {})?;
        let mut body = String::new();
        std::io::Read::read_to_string(
            &mut std::io::Read::take(&mut response.body, crate::http::MAX_LIST_BODY),
            &mut body,
        )
        .map_err(|_| AiError::Interrupted(self.id()))?;
        let page: Value = serde_json::from_str(&body).map_err(|_| AiError::BadResponse(self.id()))?;
        let data = page["data"].as_array().ok_or(AiError::BadResponse(self.id()))?;
        let mut models: Vec<(i64, String)> = data
            .iter()
            .filter_map(|m| Some((m["created"].as_i64().unwrap_or(0), m["id"].as_str()?.to_string())))
            .filter(|(_, id)| is_chat_tool_model(id))
            .collect();
        // Newest first; the suggested one is the newest full-size GPT model.
        models.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let recommended = models
            .iter()
            .position(|(_, id)| id.starts_with("gpt-") && !id.contains("mini") && !id.contains("nano"))
            .unwrap_or(0);
        let mut out: Vec<ModelInfo> = models
            .into_iter()
            .enumerate()
            .map(|(i, (_, id))| ModelInfo {
                name: id.clone(),
                id,
                recommended: i == recommended,
            })
            .collect();
        out.sort_by_key(|m| !m.recommended);
        Ok(out)
    }

    fn stream_turn(
        &self,
        key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantTurn, AiError> {
        let http = HttpRequest {
            method: Method::Post,
            url: format!("{}/v1/chat/completions", self.base_url),
            headers: vec![
                auth(key),
                ("content-type", HeaderValue::Plain("application/json".into())),
                ("accept", HeaderValue::Plain("text/event-stream".into())),
            ],
            body: Some(request_body(request).to_string()),
        };
        let response = self.send(&http, key, Some(request.model), cancel, on_event)?;
        read_stream(response.body, key, request.model, cancel, on_event)
    }
}

/// A tool call being streamed in.
#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

fn read_stream(
    body: Box<dyn std::io::BufRead + Send>,
    key: &ApiKey,
    model: &str,
    cancel: &Cancel,
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<AssistantTurn, AiError> {
    let p = ProviderId::Openai;
    let mut reader = SseReader::new(body);
    let mut text = String::new();
    let mut calls: Vec<PartialCall> = Vec::new();
    let mut finish: Option<String> = None;
    let mut done = false;
    let mut refusal = String::new();
    while !done {
        cancel.check()?;
        let Some(event) = reader.next_event().map_err(|_| AiError::Interrupted(p))? else {
            break;
        };
        if event.data.trim() == "[DONE]" {
            done = true;
            continue;
        }
        let chunk: Value = serde_json::from_str(&event.data).map_err(|_| AiError::BadResponse(p))?;
        if chunk["error"].is_object() {
            return Err(status_error(500, &chunk["error"], key, Some(model)));
        }
        let Some(choice) = chunk["choices"].get(0) else {
            continue; // the usage chunk, and anything newer
        };
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str()
            && !piece.is_empty()
        {
            text.push_str(piece);
            on_event(StreamEvent::Text(piece.to_string()));
        }
        if let Some(piece) = delta["refusal"].as_str() {
            refusal.push_str(piece);
        }
        if let Some(fragments) = delta["tool_calls"].as_array() {
            for fragment in fragments {
                let index = fragment["index"].as_u64().unwrap_or(calls.len() as u64) as usize;
                if index > 128 {
                    return Err(AiError::BadResponse(p));
                }
                while calls.len() <= index {
                    calls.push(PartialCall::default());
                }
                let call = &mut calls[index];
                if let Some(id) = fragment["id"].as_str() {
                    call.id = id.to_string();
                }
                if let Some(name) = fragment["function"]["name"].as_str() {
                    call.name.push_str(name);
                    on_event(StreamEvent::ToolStarted {
                        name: call.name.clone(),
                    });
                }
                if let Some(arguments) = fragment["function"]["arguments"].as_str() {
                    call.arguments.push_str(arguments);
                }
            }
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            finish = Some(reason.to_string());
        }
    }
    if !done && finish.is_none() {
        return Err(AiError::Interrupted(p));
    }
    let tool_calls = calls
        .into_iter()
        .filter(|c| !c.name.is_empty())
        .map(|c| {
            let raw = if c.arguments.trim().is_empty() {
                "{}"
            } else {
                c.arguments.as_str()
            };
            let (input, input_error) = match serde_json::from_str::<Value>(raw) {
                Ok(value) if value.is_object() => (value, None),
                Ok(_) => (
                    Value::Null,
                    Some("The tool input must be a JSON object.".to_string()),
                ),
                Err(e) => (
                    Value::Null,
                    Some(format!("The tool input wasn't valid JSON ({e}).")),
                ),
            };
            ToolCall {
                id: c.id,
                name: c.name,
                input,
                input_error,
            }
        })
        .collect::<Vec<_>>();
    let stop = match finish.as_deref() {
        _ if !refusal.is_empty() => StopReason::Refusal,
        Some("stop") | None => StopReason::EndTurn,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_string()),
    };
    Ok(AssistantTurn {
        text,
        tool_calls,
        stop,
        native: None,
    })
}
