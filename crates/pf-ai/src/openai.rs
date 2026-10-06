//! OpenAI's Responses API: streamed replies with function tools, and the models list.
//!
//! **Requests** go to `POST /v1/responses` (`Authorization: Bearer`) with `stream: true` and
//! `store: false`: OpenAI keeps nothing, and every request carries the whole conversation as
//! `input` items. The system prompt is `instructions`; tools are function tools
//! (`{type: "function", name, description, parameters, strict: false}`: the edit schemas have
//! optional fields, and every input is checked against the engine's types anyway); each step's
//! output is capped with `max_output_tokens`. Reasoning models (see [`is_reasoning_model`]) also
//! get `reasoning: {effort: "medium"}` and `include: ["reasoning.encrypted_content"]`, so their
//! reasoning can be replayed without being stored; a model that refuses either setting is asked
//! once more without them (a 400 means nothing ran), and is sent without them from then on.
//!
//! **History.** A user message is a `message` item; a reply's output items (reasoning with its
//! encrypted content, messages, function calls) go back exactly as they came, as OpenAI asks
//! ("any reasoning items returned in model responses with tool calls must also be passed back
//! with tool call outputs"), except that reasoning goes back only to the model that made it and
//! only with its encrypted content (after a switch of model mid-chat, the new model couldn't
//! read it); each tool result is a `function_call_output` item with its
//! `call_id`. A turn from the other provider goes back as a plain assistant message and
//! `function_call` items.
//!
//! **Replies** are server-sent events named by `type`: `response.output_text.delta` (text),
//! `response.refusal.delta`, `response.output_item.added` / `.done` (each output item:
//! `reasoning`, `message`, or `function_call` with its `call_id` and `name`),
//! `response.function_call_arguments.delta` / `.done` (arguments by `item_id`), then one of
//! `response.completed`, `response.incomplete` (`incomplete_details.reason`, e.g.
//! `max_output_tokens`), or `response.failed` (`response.error`); an `error` event may come at
//! any time. A stream that ends before one of those three is an interrupted reply.
//!
//! **Errors** (`{"error": {"message", "type", "param", "code"}}`) become plain messages; the
//! provider's own words, sanitized, ride along as details. "This model can't use tools" is only
//! said for `unsupported_parameter` / `unsupported_value` naming `tools` itself (or a tool's
//! `type`); any other refused parameter is named as such.
//!
//! **Which models are offered.** `GET /v1/models` lists every model the key can use without
//! saying which can call function tools, so PixelFlow keeps the families that can, by name (see
//! [`is_chat_tool_model`]): ids starting with `gpt-` or `codex-`, or an `o`-series reasoning
//! model (`o1`, `o3`, `o4-mini`, ...), Responses-only ones (`-pro`, `codex`) included. Dropped:
//! ids naming audio, realtime, speech, transcription, images, embeddings, moderation, search,
//! instruct, deep research, or computer use (none call function tools), and `o1-mini` /
//! `o1-preview`. Anything that slips through is caught when used, by the error rule above.

use crate::error::{AiError, sanitize};
use crate::http::{
    ErrorReply, HeaderValue, HttpRequest, Method, RetryPolicy, SendError, Transport, TransportError,
    send_with_retries,
};
use crate::provider::{
    AssistantTurn, Cancel, LlmProvider, Message, ModelInfo, ProviderId, StopReason, StreamEvent, ToolCall,
    TurnRequest,
};
use crate::secret::ApiKey;
use crate::sse::SseReader;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

pub const BASE_URL: &str = "https://api.openai.com";

/// The reasoning effort asked of reasoning models: every current one accepts it.
pub const REASONING_EFFORT: &str = "medium";

/// Error codes that mean the account can't pay (some come as a 429, but waiting won't help).
const BILLING_CODES: &[&str] = &[
    "insufficient_quota",
    "billing_hard_limit_reached",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
];

/// `response.error` codes that mean the request was declined for safety.
const POLICY_CODES: &[&str] = &["bio_policy", "misalignment_policy_violation", "invalid_prompt"];

pub struct OpenAi {
    transport: Arc<dyn Transport>,
    base_url: String,
    retry: RetryPolicy,
    /// Models that refused the reasoning settings: they're sent without them from then on, not
    /// refused (and asked again) on every step.
    no_reasoning: Mutex<HashSet<String>>,
}

impl OpenAi {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            base_url: BASE_URL.to_string(),
            retry: RetryPolicy::default(),
            no_reasoning: Mutex::default(),
        }
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    fn send_raw(
        &self,
        request: &HttpRequest,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<crate::http::HttpResponse, SendError> {
        send_with_retries(
            self.transport.as_ref(),
            request,
            self.retry,
            cancel,
            &|reply| !BILLING_CODES.iter().any(|code| reply.body.contains(code)),
            &mut |attempt, wait| {
                on_event(StreamEvent::Retrying {
                    attempt,
                    wait_ms: wait.as_millis() as u64,
                })
            },
        )
    }

    fn responses_request(&self, key: &ApiKey, body: &Value) -> HttpRequest {
        HttpRequest {
            method: Method::Post,
            url: format!("{}/v1/responses", self.base_url),
            headers: vec![
                auth(key),
                ("content-type", HeaderValue::Plain("application/json".into())),
                ("accept", HeaderValue::Plain("text/event-stream".into())),
            ],
            body: Some(body.to_string()),
        }
    }
}

fn auth(key: &ApiKey) -> (&'static str, HeaderValue) {
    (
        "authorization",
        HeaderValue::SecretWithPrefix("Bearer ", key.clone()),
    )
}

/// Whether a model reasons before answering (and takes `reasoning` settings): the `o` series,
/// `codex-` models, and GPT-5 and later, except their `-chat` variants.
pub fn is_reasoning_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    if is_o_series(&id) || id.starts_with("codex-") {
        return true;
    }
    let Some(rest) = id.strip_prefix("gpt-") else {
        return false;
    };
    let major: String = rest.chars().take_while(char::is_ascii_digit).collect();
    major.parse::<u32>().is_ok_and(|n| n >= 5) && !id.contains("-chat")
}

fn is_o_series(id: &str) -> bool {
    id.len() >= 2 && id.starts_with('o') && id.as_bytes()[1].is_ascii_digit()
}

/// The request body for one turn. `reasoning` adds the reasoning settings.
pub fn request_body(request: &TurnRequest<'_>, reasoning: bool) -> Value {
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
                "strict": false,
            })
        })
        .collect();
    let mut body = json!({
        "model": request.model,
        "stream": true,
        "store": false,
        "instructions": request.system,
        "input": input_items(request.messages, request.model),
        "tools": tools,
        "max_output_tokens": request.max_tokens,
    });
    if reasoning {
        body["reasoning"] = json!({ "effort": REASONING_EFFORT });
        body["include"] = json!(["reasoning.encrypted_content"]);
    }
    body
}

/// The conversation as Responses input items, for `model`. A reply's reasoning goes back only to
/// the model that made it (another can't read its encrypted content), and only with its
/// encrypted content (stateless replay needs it).
fn input_items(messages: &[Message], model: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        match message {
            Message::User(text) => out.push(json!({ "type": "message", "role": "user", "content": text })),
            Message::Assistant(turn) => match &turn.native {
                Some((ProviderId::Openai, native)) if native["items"].is_array() => {
                    let same_model = native["model"] == model;
                    out.extend(
                        native["items"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|item| {
                                item["type"] != "reasoning"
                                    || (same_model
                                        && item["encrypted_content"].as_str().is_some_and(|c| !c.is_empty()))
                            })
                            .cloned(),
                    );
                }
                _ => {
                    if !turn.text.is_empty() {
                        out.push(json!({ "type": "message", "role": "assistant", "content": turn.text }));
                    }
                    for call in &turn.tool_calls {
                        let arguments = if call.input.is_object() {
                            call.input.to_string()
                        } else {
                            "{}".to_string()
                        };
                        out.push(json!({
                            "type": "function_call",
                            "call_id": call.id,
                            "name": call.name,
                            "arguments": arguments,
                        }));
                    }
                }
            },
            Message::ToolResults(results) => {
                for result in results {
                    let output = if result.is_error {
                        format!("Error: {}", result.content)
                    } else {
                        result.content.clone()
                    };
                    out.push(json!({ "type": "function_call_output", "call_id": result.call_id, "output": output }));
                }
            }
        }
    }
    out
}

/// An OpenAI error object's parts.
struct ErrorParts<'a> {
    code: &'a str,
    kind: &'a str,
    param: &'a str,
    message: &'a str,
}

impl<'a> ErrorParts<'a> {
    fn of(error: &'a Value) -> Self {
        Self {
            code: error["code"].as_str().unwrap_or_default(),
            kind: error["type"].as_str().unwrap_or_default(),
            param: error["param"].as_str().unwrap_or_default(),
            message: error["message"].as_str().unwrap_or_default(),
        }
    }

    /// The parameter refused, for `unsupported_parameter` / `unsupported_value`.
    fn unsupported(&self) -> Option<&'a str> {
        matches!(self.code, "unsupported_parameter" | "unsupported_value")
            .then_some(self.param)
            .filter(|p| !p.is_empty())
    }
}

/// `tools`, `tools[3]`, or `tools[3].type`: refusing one of these refuses function tools; any
/// other tool option (`tool_choice`, `parallel_tool_calls`, `tools[0].strict`) is just an option.
fn names_function_tools(param: &str) -> bool {
    let Some(rest) = param.strip_prefix("tools") else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let Some(index_end) = rest.strip_prefix('[').and_then(|r| r.find(']')) else {
        return false;
    };
    let after = &rest[index_end + 2..];
    after.is_empty() || after == ".type"
}

/// What the user sees under "Details": the status and OpenAI's own words, sanitized.
fn details(status: Option<u16>, parts: &ErrorParts<'_>, key: &ApiKey) -> String {
    let mut head = match status {
        Some(status) => format!("HTTP {status}"),
        None => "In the reply".to_string(),
    };
    let code = if parts.code.is_empty() {
        parts.kind
    } else {
        parts.code
    };
    if !code.is_empty() {
        head.push(' ');
        head.push_str(code);
    }
    if !parts.param.is_empty() {
        head.push_str(&format!(" ({})", parts.param));
    }
    sanitize(&format!("{head}: {}", parts.message), Some(key.expose()))
}

/// An OpenAI error as a plain message (with its details). `status` is the HTTP status, or
/// `None` for an error inside a stream.
fn classify(status: Option<u16>, error: &Value, key: &ApiKey, model: &str) -> AiError {
    let p = ProviderId::Openai;
    let parts = ErrorParts::of(error);
    let model = model.to_string();
    let lower = parts.message.to_ascii_lowercase();
    let code_status = match parts.code {
        "rate_limit_exceeded" | "slow_down" => Some(429),
        "server_error" | "server_is_overloaded" => Some(500),
        _ => None,
    };
    let unsupported = parts.unsupported();
    let base = match status.or(code_status) {
        Some(401) => AiError::InvalidKey(p),
        _ if BILLING_CODES.contains(&parts.code) || parts.kind == "insufficient_quota" => AiError::Billing(p),
        _ if parts.code == "model_not_found" => AiError::ModelNotFound { provider: p, model },
        _ if POLICY_CODES.contains(&parts.code) && status.is_none() => AiError::Refused,
        _ if unsupported.is_some_and(names_function_tools) => AiError::ModelNoTools { provider: p, model },
        _ if unsupported.is_some() => AiError::UnsupportedParameter {
            provider: p,
            model,
            param: unsupported.unwrap_or_default().chars().take(80).collect(),
        },
        _ if parts.code == "context_length_exceeded" || lower.contains("context window") => AiError::TooLong,
        Some(403) => AiError::PermissionDenied(p),
        Some(404) => AiError::ModelNotFound { provider: p, model },
        Some(413) => AiError::TooLong,
        Some(429) => AiError::RateLimited(p),
        Some(500..=599) => AiError::Overloaded(p),
        _ => AiError::Provider {
            provider: p,
            message: sanitize(parts.message, Some(key.expose())),
        },
    };
    base.with_details(details(status, &parts, key))
}

fn send_error(error: SendError, key: &ApiKey, model: &str) -> AiError {
    let p = ProviderId::Openai;
    match error {
        SendError::Cancelled => AiError::Cancelled,
        SendError::Transport(TransportError::Timeout) => AiError::Timeout(p),
        SendError::Transport(_) => AiError::Network(p),
        SendError::Status(reply) => status_error(&reply, key, model),
    }
}

fn status_error(reply: &ErrorReply, key: &ApiKey, model: &str) -> AiError {
    let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    classify(Some(reply.status), &value["error"], key, model)
}

/// A 400 refusing the reasoning settings (`reasoning`, `reasoning.effort`, or `include`): the
/// request is sent again without them.
fn refuses_reasoning(error: &SendError) -> bool {
    let SendError::Status(reply) = error else {
        return false;
    };
    if reply.status != 400 {
        return false;
    }
    let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    ErrorParts::of(&value["error"])
        .unsupported()
        .is_some_and(|param| param.starts_with("reasoning") || param.starts_with("include"))
}

/// Whether a model id from `GET /v1/models` can call function tools. See the module docs for
/// the rule.
pub fn is_chat_tool_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    if !(id.starts_with("gpt-") || id.starts_with("codex-") || is_o_series(&id)) {
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
        "deep-research",
        "computer-use",
    ];
    if DROPPED.iter().any(|word| id.contains(word)) {
        return false;
    }
    !(id.starts_with("o1-mini") || id.starts_with("o1-preview"))
}

/// Suggested when nothing is picked: a full-size general GPT (not mini, nano, pro, or codex).
fn suggestable(id: &str) -> bool {
    id.starts_with("gpt-") && !["mini", "nano", "-pro", "codex"].iter().any(|w| id.contains(w))
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
        let mut response = self
            .send_raw(&request, cancel, &mut |_| {})
            .map_err(|e| send_error(e, key, ""))?;
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
        // Newest first.
        models.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let recommended = models.iter().position(|(_, id)| suggestable(id)).unwrap_or(0);
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
        let refused = self
            .no_reasoning
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(request.model);
        let reasoning = is_reasoning_model(request.model) && !refused;
        let first = self.responses_request(key, &request_body(request, reasoning));
        let response = match self.send_raw(&first, cancel, on_event) {
            Ok(response) => response,
            Err(error) if reasoning && refuses_reasoning(&error) => {
                self.no_reasoning
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(request.model.to_string());
                let plain = self.responses_request(key, &request_body(request, false));
                self.send_raw(&plain, cancel, on_event)
                    .map_err(|e| send_error(e, key, request.model))?
            }
            Err(error) => return Err(send_error(error, key, request.model)),
        };
        read_stream(response.body, key, request.model, cancel, on_event)
    }
}

/// An output item being streamed in.
struct Item {
    value: Value,
    /// Function-call arguments as they arrive.
    arguments: String,
}

/// Output items in one reply, at most.
const MAX_ITEMS: usize = 1024;

/// How a reply ended.
enum End {
    Completed,
    Incomplete(String),
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
    let mut items: Vec<Option<Item>> = Vec::new();
    let mut text = String::new();
    let mut refusal = String::new();
    let mut end: Option<(End, Value)> = None;
    while end.is_none() {
        cancel.check()?;
        let Some(event) = reader.next_event().map_err(|_| AiError::Interrupted(p))? else {
            break;
        };
        let data: Value = serde_json::from_str(&event.data).map_err(|_| AiError::BadResponse(p))?;
        let kind = data["type"].as_str().unwrap_or(event.event.as_str());
        let index = || -> Result<usize, AiError> {
            data["output_index"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .filter(|&i| i < MAX_ITEMS)
                .ok_or(AiError::BadResponse(p))
        };
        match kind {
            "response.output_item.added" | "response.output_item.done" => {
                let at = index()?;
                let value = data["item"].clone();
                if !value.is_object() {
                    return Err(AiError::BadResponse(p));
                }
                if items.len() <= at {
                    items.resize_with(at + 1, || None);
                }
                let added = kind.ends_with("added");
                if added && value["type"] == "function_call" {
                    on_event(StreamEvent::ToolStarted {
                        name: value["name"].as_str().unwrap_or_default().to_string(),
                    });
                }
                let arguments = items[at].take().map(|i| i.arguments).unwrap_or_default();
                items[at] = Some(Item { value, arguments });
            }
            "response.output_text.delta" => {
                let piece = data["delta"].as_str().unwrap_or_default();
                if !piece.is_empty() {
                    text.push_str(piece);
                    on_event(StreamEvent::Text(piece.to_string()));
                }
            }
            "response.refusal.delta" => refusal.push_str(data["delta"].as_str().unwrap_or_default()),
            "response.function_call_arguments.delta" => {
                let at = index()?;
                match items.get_mut(at).and_then(Option::as_mut) {
                    Some(item) => item
                        .arguments
                        .push_str(data["delta"].as_str().unwrap_or_default()),
                    None => return Err(AiError::BadResponse(p)),
                }
            }
            "response.completed" => end = Some((End::Completed, data["response"].clone())),
            "response.incomplete" => {
                let reason = data["response"]["incomplete_details"]["reason"]
                    .as_str()
                    .unwrap_or("incomplete")
                    .to_string();
                end = Some((End::Incomplete(reason), data["response"].clone()));
            }
            "response.failed" => {
                let error = &data["response"]["error"];
                return Err(classify(None, error, key, model));
            }
            "error" => {
                let error = if data["error"].is_object() {
                    &data["error"]
                } else {
                    &data
                };
                return Err(classify(None, error, key, model));
            }
            // response.created, response.in_progress, content parts, text and argument "done"
            // events (the items' own "done" carries the same), reasoning summaries, and anything
            // newer.
            _ => {}
        }
    }
    let Some((end, response)) = end else {
        return Err(AiError::Interrupted(p));
    };
    let streamed: Vec<Item> = items.into_iter().flatten().collect();
    finish(streamed, &response, text, refusal, end, model)
}

/// Turns the reply's output items into a turn. The final response's `output` (when it has one)
/// is what goes back next time; the streamed items fill in when it's left out.
fn finish(
    items: Vec<Item>,
    response: &Value,
    text: String,
    refusal: String,
    end: End,
    model: &str,
) -> Result<AssistantTurn, AiError> {
    let mut native: Vec<Value> = Vec::new();
    let mut tool_calls = Vec::new();
    let output = response["output"].as_array().filter(|o| !o.is_empty());
    let mut refused = !refusal.is_empty();
    for (i, item) in items.iter().enumerate() {
        let value = output
            .and_then(|o| {
                o.iter()
                    .find(|v| v["id"].is_string() && v["id"] == item.value["id"])
            })
            .or_else(|| output.and_then(|o| o.get(i)))
            .unwrap_or(&item.value);
        if value["type"] == "message"
            && value["content"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|part| part["type"] == "refusal"))
        {
            refused = true;
        }
        if value["type"] == "function_call" {
            let streamed = if value["arguments"].as_str().is_some_and(|a| !a.is_empty()) {
                value["arguments"].as_str().unwrap_or_default()
            } else {
                item.arguments.as_str()
            };
            let raw = if streamed.trim().is_empty() {
                "{}"
            } else {
                streamed
            };
            let (input, input_error) = match serde_json::from_str::<Value>(raw) {
                Ok(input) if input.is_object() => (input, None),
                Ok(_) => (
                    Value::Null,
                    Some("The tool input must be a JSON object.".to_string()),
                ),
                Err(e) => (
                    Value::Null,
                    Some(format!("The tool input wasn't valid JSON ({e}).")),
                ),
            };
            tool_calls.push(ToolCall {
                id: value["call_id"].as_str().unwrap_or_default().to_string(),
                name: value["name"].as_str().unwrap_or_default().to_string(),
                input,
                input_error,
            });
        }
        native.push(value.clone());
    }
    let stop = match end {
        _ if refused => StopReason::Refusal,
        End::Incomplete(reason) if reason == "max_output_tokens" => StopReason::MaxTokens,
        End::Incomplete(reason) if reason == "content_filter" => StopReason::Refusal,
        End::Incomplete(reason) => StopReason::Other(reason),
        End::Completed if !tool_calls.is_empty() => StopReason::ToolUse,
        End::Completed => StopReason::EndTurn,
    };
    Ok(AssistantTurn {
        text,
        tool_calls,
        stop,
        native: Some((ProviderId::Openai, json!({ "model": model, "items": native }))),
    })
}
