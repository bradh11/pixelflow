//! The Anthropic provider against hand-written Messages API replies (SSE and JSON). Nothing
//! here touches the network.

use pf_ai::anthropic::Anthropic;
use pf_ai::http::RetryPolicy;
use pf_ai::provider::{AssistantTurn, Message, StopReason, StreamEvent, ToolResult, ToolSpec, TurnRequest};
use pf_ai::testing::{FAKE_KEY, FakeTransport, Reply, fake_key};
use pf_ai::{AiError, Cancel, LlmProvider, ProviderId};
use serde_json::{Value, json};
use std::sync::Arc;

const TOOL_USE: &str = include_str!("fixtures/anthropic_tool_use.sse");
const TEXT: &str = include_str!("fixtures/anthropic_text.sse");
const FALLBACK: &str = include_str!("fixtures/anthropic_fallback.sse");
const MODELS: &str = include_str!("fixtures/anthropic_models.json");

fn setup(replies: Vec<Reply>) -> (Anthropic, Arc<FakeTransport>) {
    let fake = Arc::new(FakeTransport::new(replies));
    (
        Anthropic::new(fake.clone()).with_retry(RetryPolicy::immediate()),
        fake,
    )
}

fn tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "show_rename_show".into(),
        description: "Rename the show.".into(),
        input_schema: json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }),
    }]
}

fn turn(
    provider: &Anthropic,
    model: &str,
    messages: &[Message],
) -> (Result<AssistantTurn, AiError>, Vec<StreamEvent>) {
    let tools = tools();
    let request = TurnRequest {
        model,
        system: "You are a test.",
        tools: &tools,
        messages,
        max_tokens: 1000,
    };
    let mut events = Vec::new();
    let result = provider.stream_turn(&fake_key(), &request, &Cancel::new(), &mut |e| events.push(e));
    (result, events)
}

fn error_body(kind: &str, message: &str) -> String {
    json!({ "type": "error", "error": { "type": kind, "message": message }, "request_id": "req_fixture" })
        .to_string()
}

#[test]
fn a_streamed_tool_call_is_assembled_and_text_streams() {
    let (provider, fake) = setup(vec![Reply::ok(TOOL_USE)]);
    let (result, events) = turn(&provider, "claude-opus-5-5", &[Message::User("Rename it".into())]);
    let turn = result.unwrap();
    assert_eq!(turn.text, "I'll rename the show.");
    assert_eq!(turn.stop, StopReason::ToolUse);
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].id, "toolu_fixture_1");
    assert_eq!(turn.tool_calls[0].name, "show_rename_show");
    assert_eq!(turn.tool_calls[0].input, json!({ "name": "Christmas 2026" }));
    assert_eq!(
        events,
        [
            StreamEvent::Text("I'll rename ".into()),
            StreamEvent::Text("the show.".into()),
            StreamEvent::ToolStarted {
                name: "show_rename_show".into()
            },
        ]
    );

    // The request: streamed, with the key header, version, tools (inputs streamed eagerly).
    let request = &fake.requests()[0];
    assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(request.header("x-api-key").unwrap(), FAKE_KEY);
    assert_eq!(request.header("anthropic-version").unwrap(), "2023-06-01");
    assert!(!format!("{request:?}").contains(FAKE_KEY));
    let body = fake.body(0);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["stream"], true);
    // An explicit breakpoint on the system prompt caches the tools and system prompt together
    // (tools render first); automatic caching covers the growing conversation.
    assert_eq!(
        body["system"],
        json!([{ "type": "text", "text": "You are a test.", "cache_control": { "type": "ephemeral" } }])
    );
    assert_eq!(body["cache_control"], json!({ "type": "ephemeral" }));
    assert_eq!(body["tools"][0]["name"], "show_rename_show");
    assert_eq!(body["tools"][0]["eager_input_streaming"], true);
    assert_eq!(
        body["messages"],
        json!([{ "role": "user", "content": [{ "type": "text", "text": "Rename it" }] }])
    );
    // Opus 5.5 takes server-side fallbacks on refusal.
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(
        request.header("anthropic-beta").unwrap(),
        "server-side-fallback-2026-07-01"
    );
    // Thinking can't be turned off on current models, so it isn't configured at all.
    assert!(body.get("thinking").is_none());
}

#[test]
fn replies_are_echoed_back_unchanged_with_tool_results() {
    let (provider, fake) = setup(vec![Reply::ok(TOOL_USE), Reply::ok(TEXT)]);
    let (first, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Rename it".into())]);
    let first = first.unwrap();
    let history = vec![
        Message::User("Rename it".into()),
        Message::Assistant(first),
        Message::ToolResults(vec![ToolResult {
            call_id: "toolu_fixture_1".into(),
            content: "Done (in your draft).".into(),
            is_error: false,
        }]),
    ];
    let (second, _) = turn(&provider, "claude-opus-5-5", &history);
    assert_eq!(second.unwrap().text, "Done: the proposal is ready.");
    let messages = fake.body(1)["messages"].clone();
    // The thinking block keeps its signature, and the tool call its parsed input.
    assert_eq!(
        messages[1],
        json!({ "role": "assistant", "content": [
            { "type": "thinking", "thinking": "", "signature": "EqQBCkYIARgCIkD-fixture-signature" },
            { "type": "text", "text": "I'll rename the show." },
            { "type": "tool_use", "id": "toolu_fixture_1", "name": "show_rename_show", "input": { "name": "Christmas 2026" } },
        ]})
    );
    assert_eq!(
        messages[2],
        json!({ "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": "toolu_fixture_1", "content": "Done (in your draft).", "is_error": false }
        ]})
    );
}

#[test]
fn after_a_mid_reply_fallback_only_the_fallback_models_calls_count() {
    let (provider, _) = setup(vec![Reply::ok(FALLBACK)]);
    let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Arches?".into())]);
    let turn = result.unwrap();
    assert_eq!(turn.text, "Let me look at your props.");
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].id, "toolu_after");
    let (_, native) = turn.native.unwrap();
    let kinds: Vec<&str> = native
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    // The declined attempt's thinking and tool call are not echoed back.
    assert_eq!(kinds, ["text", "fallback", "text", "tool_use"]);
}

#[test]
fn models_without_server_side_fallbacks_dont_get_the_opt_in() {
    let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
    turn(&provider, "claude-haiku-4-5", &[Message::User("Hi".into())])
        .0
        .unwrap();
    assert!(fake.body(0).get("fallbacks").is_none());
    assert!(fake.requests()[0].header("anthropic-beta").is_none());
}

#[test]
fn an_account_that_refuses_the_fallback_beta_is_asked_again_without_it() {
    let (provider, fake) = setup(vec![
        Reply::status(
            400,
            error_body(
                "invalid_request_error",
                "Unexpected value(s) `server-side-fallback-2026-07-01` for the `anthropic-beta` header.",
            ),
        ),
        Reply::ok(TEXT),
    ]);
    let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
    assert_eq!(result.unwrap().text, "Done: the proposal is ready.");
    assert_eq!(fake.requests().len(), 2);
    assert!(fake.body(1).get("fallbacks").is_none());
}

#[test]
fn errors_become_plain_messages_without_the_key() {
    let p = ProviderId::Anthropic;
    let cases: Vec<(Reply, AiError)> = vec![
        (
            Reply::status(
                401,
                error_body("authentication_error", &format!("invalid x-api-key {FAKE_KEY}")),
            ),
            AiError::InvalidKey(p),
        ),
        (
            Reply::status(404, error_body("not_found_error", "model: claude-nope")),
            AiError::ModelNotFound {
                provider: p,
                model: "claude-opus-5-5".into(),
            },
        ),
        (
            Reply::status(
                400,
                error_body(
                    "invalid_request_error",
                    "Your credit balance is too low to access the Anthropic API.",
                ),
            ),
            AiError::Billing(p),
        ),
        (
            Reply::status(
                400,
                error_body(
                    "invalid_request_error",
                    "prompt is too long: 1200000 tokens > 1000000 maximum",
                ),
            ),
            AiError::TooLong,
        ),
        (
            Reply::status(403, error_body("permission_error", "nope")),
            AiError::PermissionDenied(p),
        ),
        (
            Reply::status(
                400,
                error_body("invalid_request_error", &format!("bad thing near {FAKE_KEY}")),
            ),
            AiError::Provider {
                provider: p,
                message: "bad thing near [your key]".into(),
            },
        ),
    ];
    for (reply, expected) in cases {
        let (provider, fake) = setup(vec![reply]);
        let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
        let error = result.unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains(FAKE_KEY));
        assert!(!format!("{error:?}").contains(FAKE_KEY));
        assert_eq!(fake.requests().len(), 1, "{expected:?} isn't retried");
    }
}

#[test]
fn rate_limits_and_overload_are_retried_then_explained() {
    let (provider, fake) = setup(vec![
        Reply::status(429, error_body("rate_limit_error", "slow down")).with_retry_after(1),
        Reply::status(529, error_body("overloaded_error", "Overloaded")).with_retry_after(1),
        Reply::ok(TEXT),
    ]);
    let (result, events) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
    assert!(result.is_ok());
    assert_eq!(fake.requests().len(), 3);
    assert!(matches!(events[0], StreamEvent::Retrying { attempt: 1, .. }));

    let busy = || Reply::status(529, error_body("overloaded_error", "Overloaded")).with_retry_after(1);
    let (provider, _) = setup(vec![busy(), busy(), busy()]);
    let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
    assert_eq!(result.unwrap_err(), AiError::Overloaded(ProviderId::Anthropic));

    let limited = || Reply::status(429, error_body("rate_limit_error", "slow down"));
    let (provider, _) = setup(vec![limited(), limited(), limited()]);
    let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
    assert_eq!(result.unwrap_err(), AiError::RateLimited(ProviderId::Anthropic));
}

#[test]
fn network_failures_and_dropped_streams() {
    let (provider, fake) = setup(vec![Reply::Unreachable, Reply::Unreachable, Reply::Unreachable]);
    let (result, _) = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())]);
    assert_eq!(result.unwrap_err(), AiError::Network(ProviderId::Anthropic));
    assert_eq!(fake.requests().len(), 3);

    // A timeout after sending may mean the provider is working on it: not sent again.
    let (provider, timed_out) = setup(vec![Reply::Timeout, Reply::Timeout, Reply::Timeout]);
    assert_eq!(
        turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())])
            .0
            .unwrap_err(),
        AiError::Timeout(ProviderId::Anthropic)
    );
    assert_eq!(timed_out.requests().len(), 1);

    // A stream cut off before message_stop is never treated as a whole reply (or retried).
    let cut = TOOL_USE.split("event: message_delta").next().unwrap().to_string();
    let (provider, fake) = setup(vec![Reply::ok(cut), Reply::ok(TEXT)]);
    assert_eq!(
        turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())])
            .0
            .unwrap_err(),
        AiError::Interrupted(ProviderId::Anthropic)
    );
    assert_eq!(fake.requests().len(), 1);

    // An error event mid-stream.
    let overloaded = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\nevent: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
    let (provider, _) = setup(vec![Reply::ok(overloaded)]);
    assert_eq!(
        turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())])
            .0
            .unwrap_err(),
        AiError::Overloaded(ProviderId::Anthropic)
    );
}

#[test]
fn refusals_and_cut_off_replies_are_reported_not_run() {
    let refusal = TEXT.replace("\"end_turn\"", "\"refusal\"");
    let (provider, _) = setup(vec![Reply::ok(refusal)]);
    assert_eq!(
        turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())])
            .0
            .unwrap()
            .stop,
        StopReason::Refusal
    );

    let cut = TOOL_USE
        .replace(
            "\"tool_use\",\"stop_sequence\"",
            "\"max_tokens\",\"stop_sequence\"",
        )
        .replace("tmas 2026\\\"}", "tmas");
    let (provider, _) = setup(vec![Reply::ok(cut)]);
    let turn = turn(&provider, "claude-opus-5-5", &[Message::User("Hi".into())])
        .0
        .unwrap();
    assert_eq!(turn.stop, StopReason::MaxTokens);
    assert!(
        turn.tool_calls[0].input_error.is_some(),
        "a cut-off input is flagged, not parsed leniently"
    );
}

#[test]
fn malformed_streams_are_bad_responses_not_crashes() {
    let start = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n";
    let cases = [
        // A block index far past the blocks so far.
        format!(
            "{start}data: {{\"type\":\"content_block_start\",\"index\":100000000,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n"
        ),
        // A block that isn't an object, then deltas into it.
        format!(
            "{start}data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":\"oops\"}}\n\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"signature_delta\",\"signature\":\"x\"}}}}\n\n"
        ),
        format!(
            "{start}data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":7}}\n\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"x\"}}}}\n\n"
        ),
    ];
    for body in cases {
        let (provider, _) = setup(vec![Reply::ok(body.clone())]);
        let result = turn(&provider, "claude-haiku-4-5", &[Message::User("Hi".into())]).0;
        assert_eq!(
            result.unwrap_err(),
            AiError::BadResponse(ProviderId::Anthropic),
            "{body}"
        );
    }
}

#[test]
fn stop_ends_the_stream() {
    let (provider, _) = setup(vec![Reply::ok(TOOL_USE)]);
    let tools = tools();
    let messages = [Message::User("Hi".into())];
    let request = TurnRequest {
        model: "claude-opus-5-5",
        system: "",
        tools: &tools,
        messages: &messages,
        max_tokens: 100,
    };
    let cancel = Cancel::new();
    let stopper = cancel.clone();
    let result = provider.stream_turn(&fake_key(), &request, &cancel, &mut |event| {
        if matches!(event, StreamEvent::Text(_)) {
            stopper.cancel();
        }
    });
    assert_eq!(result.unwrap_err(), AiError::Cancelled);
}

#[test]
fn the_model_list_is_paged_claude_only_and_suggests_the_default() {
    let second = json!({
        "data": [
            { "type": "model", "id": "claude-opus-4-8", "display_name": "Claude Opus 4.8" },
            { "type": "model", "id": "not-a-claude-model", "display_name": "Something else" }
        ],
        "has_more": false, "first_id": "claude-opus-4-8", "last_id": "not-a-claude-model"
    });
    let (provider, fake) = setup(vec![Reply::ok(MODELS), Reply::ok(second.to_string())]);
    let models = provider.list_models(&fake_key(), &Cancel::new()).unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "claude-opus-5-5",
            "claude-fable-5-1",
            "claude-sonnet-5-5",
            "claude-haiku-4-5-20251001",
            "claude-opus-4-8"
        ]
    );
    assert!(models[0].recommended);
    assert_eq!(models[0].name, "Claude Opus 5.5");
    assert_eq!(models.iter().filter(|m| m.recommended).count(), 1);
    let requests = fake.requests();
    assert_eq!(requests[0].url, "https://api.anthropic.com/v1/models?limit=1000");
    assert_eq!(
        requests[1].url,
        "https://api.anthropic.com/v1/models?limit=1000&after_id=claude-haiku-4-5-20251001"
    );
    assert_eq!(requests[1].header("x-api-key").unwrap(), FAKE_KEY);

    let (provider, _) = setup(vec![Reply::status(
        401,
        error_body("authentication_error", "invalid x-api-key"),
    )]);
    assert_eq!(
        provider.list_models(&fake_key(), &Cancel::new()).unwrap_err(),
        AiError::InvalidKey(ProviderId::Anthropic)
    );
}

#[test]
fn turns_from_another_provider_are_sent_as_plain_blocks() {
    let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
    let other = AssistantTurn {
        text: "Looking".into(),
        tool_calls: vec![pf_ai::provider::ToolCall {
            id: "call_1".into(),
            name: "list_props".into(),
            input: json!({}),
            input_error: None,
        }],
        stop: StopReason::ToolUse,
        native: None,
    };
    let history = [
        Message::User("Hi".into()),
        Message::Assistant(other),
        Message::ToolResults(vec![ToolResult {
            call_id: "call_1".into(),
            content: "[]".into(),
            is_error: false,
        }]),
        // A new question right after tool results (a stopped turn) joins the same user message.
        Message::User("And now?".into()),
    ];
    turn(&provider, "claude-opus-5-5", &history).0.unwrap();
    let messages: Value = fake.body(0)["messages"].clone();
    assert_eq!(messages.as_array().unwrap().len(), 3);
    assert_eq!(messages[1]["content"][1]["type"], "tool_use");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(
        messages[2]["content"][1],
        json!({ "type": "text", "text": "And now?" })
    );
}
