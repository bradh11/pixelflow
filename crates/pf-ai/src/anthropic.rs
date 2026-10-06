//! Anthropic's Messages API: streamed replies with tool use, and the models list.
//!
//! Requests go to `POST /v1/messages` with `stream: true` (`x-api-key` and
//! `anthropic-version: 2023-06-01` headers). Replies arrive as server-sent events:
//! `message_start`, then per content block `content_block_start` / `content_block_delta`
//! (`text_delta`, `input_json_delta`, `thinking_delta`, `signature_delta`) /
//! `content_block_stop`, then `message_delta` (the stop reason) and `message_stop`; `ping` and
//! `error` events may come at any time. The assistant's content blocks (thinking with its
//! signature included) are echoed back unchanged on the next request, and the history is only
//! ever appended to. Models come from `GET /v1/models`.

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

pub const API_VERSION: &str = "2023-06-01";
pub const BASE_URL: &str = "https://api.anthropic.com";
/// Suggested when the user hasn't picked a model.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
/// Models that take server-side fallbacks (`fallbacks: "default"`): when the model's safety
/// classifier declines a request, Anthropic re-runs it on its recommended fallback model.
const FALLBACK_MODELS: &[&str] = &[
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub struct Anthropic {
    transport: Arc<dyn Transport>,
    base_url: String,
    retry: RetryPolicy,
}

impl Anthropic {
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

    fn headers(key: &ApiKey) -> Vec<(&'static str, HeaderValue)> {
        vec![
            ("x-api-key", HeaderValue::Secret(key.clone())),
            ("anthropic-version", HeaderValue::Plain(API_VERSION.into())),
        ]
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
            &|_| true,
            &mut |attempt, wait| {
                on_event(StreamEvent::Retrying {
                    attempt,
                    wait_ms: wait.as_millis() as u64,
                })
            },
        )
        .map_err(|e| send_error(e, key, model))
    }

    fn message_request(&self, key: &ApiKey, body: &Value, fallbacks: bool) -> HttpRequest {
        let mut headers = Self::headers(key);
        headers.push(("content-type", HeaderValue::Plain("application/json".into())));
        headers.push(("accept", HeaderValue::Plain("text/event-stream".into())));
        let mut body = body.clone();
        if fallbacks {
            headers.push(("anthropic-beta", HeaderValue::Plain(FALLBACK_BETA.into())));
            body["fallbacks"] = json!("default");
        }
        HttpRequest {
            method: Method::Post,
            url: format!("{}/v1/messages", self.base_url),
            headers,
            body: Some(body.to_string()),
        }
    }
}

/// The request body for one turn (without the fallback opt-in).
pub fn request_body(request: &TurnRequest<'_>) -> Value {
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
                // Tool input streams as it's generated; it's parsed strictly and checked against
                // the edit types before anything runs.
                "eager_input_streaming": true,
            })
        })
        .collect();
    json!({
        "model": request.model,
        "max_tokens": request.max_tokens,
        "stream": true,
        // Automatic caching for the conversation so far (the prefix only grows)...
        "cache_control": { "type": "ephemeral" },
        // ...plus a fixed breakpoint on the system prompt, which caches the tools (rendered
        // before it) and the system prompt together, whatever happens later in the chat.
        "system": [{ "type": "text", "text": request.system, "cache_control": { "type": "ephemeral" } }],
        "tools": tools,
        "messages": messages(request.messages),
    })
}

/// The conversation in Anthropic's shape. Consecutive user content (tool results, then the
/// next question after a stopped turn) is merged into one user message.
fn messages(history: &[Message]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut push = |role: &str, content: Vec<Value>| {
        if let Some(last) = out.last_mut()
            && last["role"] == role
            && let Some(blocks) = last["content"].as_array_mut()
        {
            blocks.extend(content);
            return;
        }
        out.push(json!({ "role": role, "content": content }));
    };
    for message in history {
        match message {
            Message::User(text) => push("user", vec![json!({ "type": "text", "text": text })]),
            Message::Assistant(turn) => {
                let content = match &turn.native {
                    Some((ProviderId::Anthropic, Value::Array(blocks))) => blocks.clone(),
                    _ => neutral_blocks(turn),
                };
                if !content.is_empty() {
                    push("assistant", content);
                }
            }
            Message::ToolResults(results) => push(
                "user",
                results
                    .iter()
                    .map(|r| {
                        json!({
                            "type": "tool_result",
                            "tool_use_id": r.call_id,
                            "content": r.content,
                            "is_error": r.is_error,
                        })
                    })
                    .collect(),
            ),
        }
    }
    out
}

/// An assistant turn from elsewhere (another provider) as plain content blocks.
fn neutral_blocks(turn: &AssistantTurn) -> Vec<Value> {
    let mut blocks = Vec::new();
    if !turn.text.is_empty() {
        blocks.push(json!({ "type": "text", "text": turn.text }));
    }
    for call in &turn.tool_calls {
        let input = if call.input.is_object() {
            call.input.clone()
        } else {
            json!({})
        };
        blocks.push(json!({ "type": "tool_use", "id": call.id, "name": call.name, "input": input }));
    }
    blocks
}

fn send_error(error: SendError, key: &ApiKey, model: Option<&str>) -> AiError {
    let p = ProviderId::Anthropic;
    match error {
        SendError::Cancelled => AiError::Cancelled,
        SendError::Transport(TransportError::Timeout) => AiError::Timeout(p),
        SendError::Transport(_) => AiError::Network(p),
        SendError::Status(reply) => {
            let (kind, message) = error_parts(&reply.body);
            status_error(reply.status, &kind, &message, key, model)
        }
    }
}

/// `{"type":"error","error":{"type":..., "message":...}}` → (type, message).
fn error_parts(body: &str) -> (String, String) {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = &value["error"];
    (
        error["type"].as_str().unwrap_or_default().to_string(),
        error["message"].as_str().unwrap_or_default().to_string(),
    )
}

fn status_error(status: u16, kind: &str, message: &str, key: &ApiKey, model: Option<&str>) -> AiError {
    let p = ProviderId::Anthropic;
    let lower = message.to_ascii_lowercase();
    let model = model.unwrap_or("").to_string();
    let invalid_request = kind == "invalid_request_error" || (status == 400 && kind.is_empty());
    let unsupported = if invalid_request {
        unsupported_param(message)
    } else {
        None
    };
    let base = match (status, kind) {
        (401, _) | (_, "authentication_error") => AiError::InvalidKey(p),
        (402, _) | (_, "billing_error") => AiError::Billing(p),
        (403, _) | (_, "permission_error") => AiError::PermissionDenied(p),
        (404, _) | (_, "not_found_error") if lower.starts_with("model") => {
            AiError::ModelNotFound { provider: p, model }
        }
        (413, _) | (_, "request_too_large") => AiError::TooLong,
        (429, _) | (_, "rate_limit_error") => AiError::RateLimited(p),
        (529, _) | (_, "overloaded_error") => AiError::Overloaded(p),
        _ if lower.contains("credit balance") => AiError::Billing(p),
        _ if lower.contains("prompt is too long") || lower.contains("context window") => AiError::TooLong,
        _ if invalid_request && says_no_tools(&lower) => AiError::ModelNoTools { provider: p, model },
        _ if unsupported.is_some() => AiError::UnsupportedParameter {
            provider: p,
            model,
            param: unsupported.unwrap_or_default(),
        },
        (500..=599, _) => AiError::Overloaded(p),
        _ => AiError::Provider {
            provider: p,
            message: sanitize(message, Some(key.expose())),
        },
    };
    let head = if kind.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status} {kind}")
    };
    base.with_details(sanitize(&format!("{head}: {message}"), Some(key.expose())))
}

/// Anthropic saying the model itself can't use tools (not that one option or tool is refused).
fn says_no_tools(lower: &str) -> bool {
    [
        "does not support tool use",
        "doesn't support tool use",
        "does not support tools",
        "doesn't support tools",
        "tool use is not supported",
        "tools are not supported",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// The request field a 400 refuses: "`top_k` is not supported ..." or
/// "tools.0.custom.eager_input_streaming: Extra inputs are not permitted".
fn unsupported_param(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    let refused = lower.contains("not supported")
        || lower.contains("unsupported")
        || lower.contains("extra inputs are not permitted");
    if !refused {
        return None;
    }
    let is_path = |text: &str| {
        !text.is_empty()
            && text.len() <= 80
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '[' | ']'))
    };
    if let Some((head, _)) = message.split_once(": ")
        && is_path(head)
    {
        return Some(head.to_string());
    }
    let quoted = message.split('`').nth(1)?;
    is_path(quoted).then(|| quoted.to_string())
}

/// A 400 that names the fallback opt-in: this account (or proxy) doesn't take it, so the
/// request is sent once more without it.
fn rejects_fallbacks(error: &SendError) -> bool {
    match error {
        SendError::Status(reply) if reply.status == 400 => {
            let (_, message) = error_parts(&reply.body);
            message.contains("anthropic-beta") || message.contains("fallbacks")
        }
        _ => false,
    }
}

impl LlmProvider for Anthropic {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn list_models(&self, key: &ApiKey, cancel: &Cancel) -> Result<Vec<ModelInfo>, AiError> {
        let mut models = Vec::new();
        let mut after: Option<String> = None;
        // A few pages at most (one page holds every model today).
        for _ in 0..5 {
            let mut url = format!("{}/v1/models?limit=1000", self.base_url);
            if let Some(after) = &after {
                url.push_str("&after_id=");
                url.push_str(&query_escape(after));
            }
            let request = HttpRequest {
                method: Method::Get,
                url,
                headers: Self::headers(key),
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
            models.extend(data.iter().filter_map(model_info));
            match (page["has_more"].as_bool(), page["last_id"].as_str()) {
                (Some(true), Some(last)) => after = Some(last.to_string()),
                _ => break,
            }
        }
        Ok(order_models(models))
    }

    fn stream_turn(
        &self,
        key: &ApiKey,
        request: &TurnRequest<'_>,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<AssistantTurn, AiError> {
        let body = request_body(request);
        let fallbacks = FALLBACK_MODELS.contains(&request.model);
        let first = self.message_request(key, &body, fallbacks);
        let sent = send_with_retries(
            self.transport.as_ref(),
            &first,
            self.retry,
            cancel,
            &|_| true,
            &mut |attempt, wait| {
                on_event(StreamEvent::Retrying {
                    attempt,
                    wait_ms: wait.as_millis() as u64,
                })
            },
        );
        let response = match sent {
            Ok(response) => response,
            Err(error) if fallbacks && rejects_fallbacks(&error) => {
                let plain = self.message_request(key, &body, false);
                self.send(&plain, key, Some(request.model), cancel, on_event)?
            }
            Err(error) => return Err(send_error(error, key, Some(request.model))),
        };
        read_stream(response.body, key, request.model, cancel, on_event)
    }
}

/// A Claude model from the models list. Every Claude model can chat and use tools; anything
/// else the endpoint lists is skipped.
fn model_info(entry: &Value) -> Option<ModelInfo> {
    let id = entry["id"].as_str()?;
    if !id.starts_with("claude-") {
        return None;
    }
    let name = entry["display_name"].as_str().unwrap_or(id).to_string();
    Some(ModelInfo {
        id: id.to_string(),
        name,
        recommended: id == DEFAULT_MODEL,
    })
}

/// The recommended model first, then as listed (newest first).
fn order_models(mut models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    models.sort_by_key(|m| !m.recommended);
    if !models.iter().any(|m| m.recommended)
        && let Some(first) = models.first_mut()
    {
        first.recommended = true;
    }
    models
}

fn query_escape(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A content block being streamed in.
struct Block {
    value: Value,
    /// Tool input JSON as it arrives.
    partial_json: String,
}

/// Reads one streamed reply.
fn read_stream(
    body: Box<dyn std::io::BufRead + Send>,
    key: &ApiKey,
    model: &str,
    cancel: &Cancel,
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<AssistantTurn, AiError> {
    let p = ProviderId::Anthropic;
    let mut reader = SseReader::new(body);
    let mut blocks: Vec<Block> = Vec::new();
    let mut stop: Option<String> = None;
    let mut finished = false;
    while !finished {
        cancel.check()?;
        let Some(event) = reader.next_event().map_err(|_| AiError::Interrupted(p))? else {
            break;
        };
        let data: Value = serde_json::from_str(&event.data).map_err(|_| AiError::BadResponse(p))?;
        let kind = data["type"].as_str().unwrap_or(event.event.as_str());
        match kind {
            "content_block_start" => {
                let mut value = data["content_block"].clone();
                if !value.is_object() {
                    return Err(AiError::BadResponse(p));
                }
                if value["type"] == "tool_use" {
                    on_event(StreamEvent::ToolStarted {
                        name: value["name"].as_str().unwrap_or_default().to_string(),
                    });
                    value["input"] = json!({});
                }
                // Blocks arrive in order: the next one, or (again) one already started.
                let index = data["index"].as_u64().unwrap_or(blocks.len() as u64);
                let block = Block {
                    value,
                    partial_json: String::new(),
                };
                match usize::try_from(index) {
                    Ok(i) if i < blocks.len() => blocks[i] = block,
                    Ok(i) if i == blocks.len() && i < MAX_BLOCKS => blocks.push(block),
                    _ => return Err(AiError::BadResponse(p)),
                }
            }
            "content_block_delta" => {
                let index = data["index"].as_u64().unwrap_or(0) as usize;
                let Some(block) = blocks.get_mut(index) else {
                    return Err(AiError::BadResponse(p));
                };
                let delta = &data["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        let text = delta["text"].as_str().unwrap_or_default();
                        append(&mut block.value, "text", text);
                        on_event(StreamEvent::Text(text.to_string()));
                    }
                    "input_json_delta" => block
                        .partial_json
                        .push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    "thinking_delta" => append(
                        &mut block.value,
                        "thinking",
                        delta["thinking"].as_str().unwrap_or_default(),
                    ),
                    "signature_delta" => {
                        if let Some(object) = block.value.as_object_mut() {
                            object.insert(
                                "signature".into(),
                                json!(delta["signature"].as_str().unwrap_or_default()),
                            );
                        }
                    }
                    // Citations and anything newer: kept as the block arrived.
                    _ => {}
                }
            }
            "content_block_stop" => {}
            "message_delta" => {
                if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                    stop = Some(reason.to_string());
                }
            }
            "message_stop" => finished = true,
            "error" => {
                let error = &data["error"];
                let kind = error["type"].as_str().unwrap_or_default();
                let message = error["message"].as_str().unwrap_or_default();
                let status = match kind {
                    "overloaded_error" => 529,
                    "rate_limit_error" => 429,
                    _ => 500,
                };
                return Err(status_error(status, kind, message, key, Some(model)));
            }
            // message_start, ping, and anything newer.
            _ => {}
        }
    }
    if !finished {
        return Err(AiError::Interrupted(p));
    }
    finish(blocks, stop)
}

/// Content blocks in one reply, at most.
const MAX_BLOCKS: usize = 1024;

fn append(value: &mut Value, field: &str, more: &str) {
    if let Some(object) = value.as_object_mut() {
        let mut text = object
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        text.push_str(more);
        object.insert(field.to_string(), json!(text));
    }
}

/// Turns the streamed blocks into a turn. After a mid-reply fallback (a `fallback` block marks
/// where the declined model stopped), the declined part's thinking and tool calls are dropped,
/// as Anthropic asks when echoing such a reply back; only what came after it can call tools.
fn finish(blocks: Vec<Block>, stop: Option<String>) -> Result<AssistantTurn, AiError> {
    let last_fallback = blocks.iter().rposition(|b| b.value["type"] == "fallback");
    let mut native = Vec::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for (i, mut block) in blocks.into_iter().enumerate() {
        let kind = block.value["type"].as_str().unwrap_or_default().to_string();
        let before_fallback = last_fallback.is_some_and(|f| i < f);
        if before_fallback && !matches!(kind.as_str(), "text" | "fallback") {
            continue;
        }
        match kind.as_str() {
            "text" => text.push_str(block.value["text"].as_str().unwrap_or_default()),
            "tool_use" => {
                let raw = if block.partial_json.trim().is_empty() {
                    "{}"
                } else {
                    block.partial_json.as_str()
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
                block.value["input"] = if input.is_object() {
                    input.clone()
                } else {
                    json!({})
                };
                tool_calls.push(ToolCall {
                    id: block.value["id"].as_str().unwrap_or_default().to_string(),
                    name: block.value["name"].as_str().unwrap_or_default().to_string(),
                    input,
                    input_error,
                });
            }
            _ => {}
        }
        if !block.value.is_null() {
            native.push(block.value);
        }
    }
    let stop = match stop.as_deref() {
        Some("end_turn") | Some("stop_sequence") | None => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_string()),
    };
    Ok(AssistantTurn {
        text,
        tool_calls,
        stop,
        native: Some((ProviderId::Anthropic, Value::Array(native))),
    })
}
