//! The OpenAI provider against hand-written Responses API replies (SSE and JSON). Nothing here
//! touches the network.

use pf_ai::http::RetryPolicy;
use pf_ai::openai::{OpenAi, is_chat_tool_model};
use pf_ai::provider::{
    AssistantTurn, Message, StopReason, StreamEvent, ToolCall, ToolResult, ToolSpec, TurnRequest,
};
use pf_ai::testing::{FAKE_KEY, FakeTransport, Reply, fake_key};
use pf_ai::{AiError, Cancel, LlmProvider, ProviderId};
use serde_json::{Value, json};
use std::sync::Arc;

const TEXT: &str = include_str!("fixtures/openai_responses_text.sse");
const TOOL_CALL: &str = include_str!("fixtures/openai_responses_tool_call.sse");
const PARALLEL: &str = include_str!("fixtures/openai_responses_parallel_calls.sse");
const REASONING: &str = include_str!("fixtures/openai_responses_reasoning.sse");
const INCOMPLETE: &str = include_str!("fixtures/openai_responses_incomplete.sse");
const MODELS: &str = include_str!("fixtures/openai_models.json");

fn setup(replies: Vec<Reply>) -> (OpenAi, Arc<FakeTransport>) {
    let fake = Arc::new(FakeTransport::new(replies));
    (
        OpenAi::new(fake.clone()).with_retry(RetryPolicy::immediate()),
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

fn turn_with(
    provider: &OpenAi,
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

fn turn(provider: &OpenAi, messages: &[Message]) -> (Result<AssistantTurn, AiError>, Vec<StreamEvent>) {
    turn_with(provider, "gpt-4.1", messages)
}

fn hi() -> Vec<Message> {
    vec![Message::User("Hi".into())]
}

fn error_body(code: &str, param: Option<&str>, message: &str) -> String {
    json!({ "error": { "message": message, "type": "invalid_request_error", "param": param, "code": code } })
        .to_string()
}

#[test]
fn a_streamed_text_reply_goes_to_the_responses_endpoint() {
    let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
    let (result, events) = turn(&provider, &[Message::User("Rename it".into())]);
    let turn = result.unwrap();
    assert_eq!(turn.text, "All set.");
    assert_eq!(turn.stop, StopReason::EndTurn);
    assert!(turn.tool_calls.is_empty());
    assert_eq!(
        events,
        [StreamEvent::Text("All ".into()), StreamEvent::Text("set.".into())]
    );

    let request = &fake.requests()[0];
    assert_eq!(request.url, "https://api.openai.com/v1/responses");
    assert_eq!(
        request.header("authorization").unwrap(),
        format!("Bearer {FAKE_KEY}")
    );
    assert_eq!(request.header("accept").unwrap(), "text/event-stream");
    assert!(!format!("{request:?}").contains(FAKE_KEY));
    let body = fake.body(0);
    assert!(!body.to_string().contains(FAKE_KEY));
    assert_eq!(body["model"], "gpt-4.1");
    assert_eq!(body["stream"], true);
    // Nothing is kept on OpenAI's side: every request carries the whole conversation.
    assert_eq!(body["store"], false);
    assert_eq!(body["max_output_tokens"], 1000);
    assert_eq!(body["instructions"], "You are a test.");
    assert_eq!(
        body["input"],
        json!([{ "type": "message", "role": "user", "content": "Rename it" }])
    );
    assert_eq!(
        body["tools"],
        json!([{
            "type": "function",
            "name": "show_rename_show",
            "description": "Rename the show.",
            "parameters": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
            "strict": false,
        }])
    );
    // Not a reasoning model: no reasoning settings.
    assert!(body.get("reasoning").is_none());
    assert!(body.get("include").is_none());
}

#[test]
fn a_single_tool_call_is_assembled_from_argument_deltas() {
    let (provider, _) = setup(vec![Reply::ok(TOOL_CALL)]);
    let (result, events) = turn(&provider, &hi());
    let turn = result.unwrap();
    assert_eq!(turn.text, "");
    assert_eq!(turn.stop, StopReason::ToolUse);
    assert_eq!(
        turn.tool_calls,
        [ToolCall {
            id: "call_fixture_single".into(),
            name: "list_props".into(),
            input: json!({ "nameContains": "arch" }),
            input_error: None,
        }]
    );
    assert_eq!(
        events,
        [StreamEvent::ToolStarted {
            name: "list_props".into()
        }]
    );
}

#[test]
fn parallel_tool_calls_keep_their_order_and_text() {
    let (provider, _) = setup(vec![Reply::ok(PARALLEL)]);
    let (result, events) = turn(&provider, &hi());
    let turn = result.unwrap();
    assert_eq!(turn.text, "Renaming it now.");
    assert_eq!(turn.stop, StopReason::ToolUse);
    assert_eq!(
        turn.tool_calls,
        [
            ToolCall {
                id: "call_fixture_a".into(),
                name: "show_rename_show".into(),
                input: json!({ "name": "Christmas 2026" }),
                input_error: None,
            },
            ToolCall {
                id: "call_fixture_b".into(),
                name: "list_props".into(),
                input: json!({}),
                input_error: None,
            },
        ]
    );
    assert_eq!(events[0], StreamEvent::Text("Renaming ".into()));
    assert!(events.contains(&StreamEvent::ToolStarted {
        name: "show_rename_show".into()
    }));
    assert!(events.contains(&StreamEvent::ToolStarted {
        name: "list_props".into()
    }));
}

#[test]
fn tool_results_go_back_as_input_items_after_the_calls_verbatim() {
    let (provider, fake) = setup(vec![Reply::ok(PARALLEL), Reply::ok(TEXT)]);
    let first = turn(&provider, &[Message::User("Rename it".into())]).0.unwrap();
    let history = vec![
        Message::User("Rename it".into()),
        Message::Assistant(first),
        Message::ToolResults(vec![
            ToolResult {
                call_id: "call_fixture_a".into(),
                content: "Done (in your draft).".into(),
                is_error: false,
            },
            ToolResult {
                call_id: "call_fixture_b".into(),
                content: "There's no prop with that id.".into(),
                is_error: true,
            },
        ]),
    ];
    assert_eq!(turn(&provider, &history).0.unwrap().text, "All set.");
    let input = fake.body(1)["input"].clone();
    let input = input.as_array().unwrap();
    assert_eq!(input.len(), 6, "{input:#?}");
    assert_eq!(input[0]["role"], "user");
    // The reply's output items, exactly as they came.
    assert_eq!(input[1]["type"], "message");
    assert_eq!(input[1]["id"], "msg_fixture_par");
    assert_eq!(input[1]["content"][0]["text"], "Renaming it now.");
    assert_eq!(
        input[2],
        json!({ "id": "fc_fixture_a", "type": "function_call", "status": "completed", "arguments": "{\"name\":\"Christmas 2026\"}", "call_id": "call_fixture_a", "name": "show_rename_show" })
    );
    assert_eq!(input[3]["call_id"], "call_fixture_b");
    assert_eq!(
        input[4],
        json!({ "type": "function_call_output", "call_id": "call_fixture_a", "output": "Done (in your draft)." })
    );
    assert_eq!(
        input[5],
        json!({ "type": "function_call_output", "call_id": "call_fixture_b", "output": "Error: There's no prop with that id." })
    );
}

#[test]
fn a_turn_from_the_other_provider_is_sent_as_plain_items() {
    let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
    let history = vec![
        Message::User("Rename it".into()),
        Message::Assistant(AssistantTurn {
            text: "On it.".into(),
            tool_calls: vec![ToolCall {
                id: "toolu_1".into(),
                name: "show_rename_show".into(),
                input: json!({ "name": "X" }),
                input_error: None,
            }],
            stop: StopReason::ToolUse,
            native: Some((ProviderId::Anthropic, json!([{ "type": "thinking" }]))),
        }),
        Message::ToolResults(vec![ToolResult {
            call_id: "toolu_1".into(),
            content: "Done.".into(),
            is_error: false,
        }]),
    ];
    turn(&provider, &history).0.unwrap();
    let input = fake.body(0)["input"].clone();
    assert_eq!(
        input[1],
        json!({ "type": "message", "role": "assistant", "content": "On it." })
    );
    assert_eq!(
        input[2],
        json!({ "type": "function_call", "call_id": "toolu_1", "name": "show_rename_show", "arguments": "{\"name\":\"X\"}" })
    );
    assert_eq!(input[3]["type"], "function_call_output");
}

#[test]
fn a_reasoning_model_gets_effort_and_its_reasoning_is_replayed() {
    let (provider, fake) = setup(vec![Reply::ok(REASONING), Reply::ok(TEXT)]);
    let first = turn_with(&provider, "gpt-6.1-sol", &hi()).0.unwrap();
    assert_eq!(first.stop, StopReason::ToolUse);
    assert_eq!(first.tool_calls[0].name, "get_show_overview");
    let body = fake.body(0);
    assert_eq!(body["reasoning"], json!({ "effort": "medium" }));
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));

    let history = vec![
        Message::User("Hi".into()),
        Message::Assistant(first),
        Message::ToolResults(vec![ToolResult {
            call_id: "call_fixture_think".into(),
            content: "{}".into(),
            is_error: false,
        }]),
    ];
    turn_with(&provider, "gpt-6.1-sol", &history).0.unwrap();
    let input = fake.body(1)["input"].clone();
    // The reasoning item comes back with its encrypted content, before its function call.
    assert_eq!(input[1]["type"], "reasoning");
    assert_eq!(input[1]["id"], "rs_fixture_think");
    assert_eq!(
        input[1]["encrypted_content"],
        "gAAAAAB-fixture-encrypted-reasoning=="
    );
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input[3]["type"], "function_call_output");

    for model in [
        "o3",
        "o4-mini",
        "gpt-5",
        "gpt-5.1-mini",
        "gpt-6.1-sol",
        "gpt-5-codex",
    ] {
        let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
        turn_with(&provider, model, &hi()).0.unwrap();
        assert_eq!(fake.body(0)["reasoning"]["effort"], "medium", "{model}");
    }
    for model in ["gpt-4.1", "gpt-4o", "gpt-5-chat-latest", "gpt-3.5-turbo"] {
        let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
        turn_with(&provider, model, &hi()).0.unwrap();
        assert!(fake.body(0).get("reasoning").is_none(), "{model}");
    }
}

#[test]
fn a_model_that_refuses_reasoning_settings_is_asked_again_without_them() {
    let refused = Reply::status(
        400,
        error_body(
            "unsupported_parameter",
            Some("reasoning.effort"),
            "Unsupported parameter: 'reasoning.effort' is not supported with this model.",
        ),
    );
    let (provider, fake) = setup(vec![refused, Reply::ok(TEXT)]);
    assert_eq!(
        turn_with(&provider, "gpt-5-mystery", &hi()).0.unwrap().text,
        "All set."
    );
    assert_eq!(fake.requests().len(), 2);
    assert!(fake.body(0).get("reasoning").is_some());
    assert!(fake.body(1).get("reasoning").is_none());
    assert!(fake.body(1).get("include").is_none());

    // It's remembered: the next steps go without them at once, not a refusal each time.
    fake.push(Reply::ok(TEXT));
    turn_with(&provider, "gpt-5-mystery", &hi()).0.unwrap();
    assert_eq!(fake.requests().len(), 3);
    assert!(fake.body(2).get("reasoning").is_none());
    // Other models still get them.
    fake.push(Reply::ok(TEXT));
    turn_with(&provider, "gpt-5.1", &hi()).0.unwrap();
    assert!(fake.body(3).get("reasoning").is_some());
}

#[test]
fn reasoning_is_replayed_only_to_the_model_that_made_it() {
    let (provider, fake) = setup(vec![Reply::ok(REASONING), Reply::ok(TEXT), Reply::ok(TEXT)]);
    let first = turn_with(&provider, "gpt-6.1-sol", &hi()).0.unwrap();
    let history = vec![
        Message::User("Hi".into()),
        Message::Assistant(first),
        Message::ToolResults(vec![ToolResult {
            call_id: "call_fixture_think".into(),
            content: "{}".into(),
            is_error: false,
        }]),
    ];
    // The user switched models mid-chat: the other model's encrypted reasoning stays out; the
    // call and its answer go back.
    turn_with(&provider, "gpt-4.1", &history).0.unwrap();
    let kinds = |n: usize| -> Vec<String> {
        fake.body(n)["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["type"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    assert_eq!(kinds(1), ["message", "function_call", "function_call_output"]);
    // The same model gets it back.
    turn_with(&provider, "gpt-6.1-sol", &history).0.unwrap();
    assert_eq!(
        kinds(2),
        ["message", "reasoning", "function_call", "function_call_output"]
    );

    // A reasoning item without its encrypted content can't be replayed statelessly at all.
    let bare = REASONING.replace(
        ",\"encrypted_content\":\"gAAAAAB-fixture-encrypted-reasoning==\"",
        "",
    );
    let (provider, fake) = setup(vec![Reply::ok(bare), Reply::ok(TEXT)]);
    let first = turn_with(&provider, "gpt-6.1-sol", &hi()).0.unwrap();
    let history = vec![
        Message::User("Hi".into()),
        Message::Assistant(first),
        Message::ToolResults(vec![ToolResult {
            call_id: "call_fixture_think".into(),
            content: "{}".into(),
            is_error: false,
        }]),
    ];
    turn_with(&provider, "gpt-6.1-sol", &history).0.unwrap();
    assert!(
        fake.body(1)["input"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["type"] != "reasoning")
    );
}

#[test]
fn hitting_the_output_limit_mid_call_is_a_max_tokens_stop() {
    let (provider, _) = setup(vec![Reply::ok(INCOMPLETE)]);
    let turn = turn(&provider, &hi()).0.unwrap();
    assert_eq!(turn.stop, StopReason::MaxTokens);
    assert!(turn.tool_calls[0].input_error.is_some());
}

#[test]
fn refusals_and_safety_stops() {
    let refusal = [
        json!({ "type": "response.output_item.added", "output_index": 0, "item": { "id": "msg_r", "type": "message", "role": "assistant", "content": [] } }),
        json!({ "type": "response.refusal.delta", "item_id": "msg_r", "output_index": 0, "content_index": 0, "delta": "I can't help with that." }),
        json!({ "type": "response.completed", "response": { "status": "completed", "output": [] } }),
    ];
    let (provider, _) = setup(vec![Reply::ok(sse(&refusal))]);
    assert_eq!(turn(&provider, &hi()).0.unwrap().stop, StopReason::Refusal);

    let filtered = [
        json!({ "type": "response.incomplete", "response": { "status": "incomplete", "incomplete_details": { "reason": "content_filter" }, "output": [] } }),
    ];
    let (provider, _) = setup(vec![Reply::ok(sse(&filtered))]);
    assert_eq!(turn(&provider, &hi()).0.unwrap().stop, StopReason::Refusal);
}

fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect()
}

#[test]
fn errors_become_plain_messages_with_details_and_without_the_key() {
    let p = ProviderId::Openai;
    let model = || "gpt-4.1".to_string();
    let cases: Vec<(Reply, AiError)> = vec![
        (
            Reply::status(
                401,
                error_body(
                    "invalid_api_key",
                    None,
                    "Incorrect API key provided: sk-test-****-key.",
                ),
            ),
            AiError::InvalidKey(p),
        ),
        (
            Reply::status(
                429,
                error_body("insufficient_quota", None, "You exceeded your current quota."),
            ),
            AiError::Billing(p),
        ),
        (
            Reply::status(
                429,
                error_body("credit_balance_exhausted", None, "Credit balance exhausted."),
            ),
            AiError::Billing(p),
        ),
        (
            Reply::status(
                403,
                error_body("", None, "Country, region, or territory not supported"),
            ),
            AiError::PermissionDenied(p),
        ),
        (
            Reply::status(
                404,
                error_body(
                    "model_not_found",
                    Some("model"),
                    "The model `gpt-4.1` does not exist or you do not have access to it.",
                ),
            ),
            AiError::ModelNotFound {
                provider: p,
                model: model(),
            },
        ),
        (
            Reply::status(
                400,
                error_body(
                    "unsupported_parameter",
                    Some("tools"),
                    "Unsupported parameter: 'tools' is not supported with this model.",
                ),
            ),
            AiError::ModelNoTools {
                provider: p,
                model: model(),
            },
        ),
        (
            Reply::status(
                400,
                error_body(
                    "unsupported_value",
                    Some("tools[0].type"),
                    "Unsupported value: 'function' is not supported with this model.",
                ),
            ),
            AiError::ModelNoTools {
                provider: p,
                model: model(),
            },
        ),
        (
            Reply::status(
                400,
                error_body(
                    "unsupported_parameter",
                    Some("max_output_tokens"),
                    "Unsupported parameter: 'max_output_tokens' is not supported with this model.",
                ),
            ),
            AiError::UnsupportedParameter {
                provider: p,
                model: model(),
                param: "max_output_tokens".into(),
            },
        ),
        (
            Reply::status(
                400,
                error_body(
                    "context_length_exceeded",
                    Some("input"),
                    "Your input exceeds the context window of this model.",
                ),
            ),
            AiError::TooLong,
        ),
        (
            Reply::status(500, error_body("server_error", None, "The server had an error.")),
            AiError::Overloaded(p),
        ),
        (
            Reply::status(
                400,
                error_body("", None, &format!("Something odd about {FAKE_KEY}")),
            ),
            AiError::Provider {
                provider: p,
                message: "Something odd about [your key]".into(),
            },
        ),
    ];
    for (reply, expected) in cases {
        let (provider, fake) = setup(vec![reply]);
        let error = turn(&provider, &hi()).0.unwrap_err();
        assert_eq!(error.root(), &expected);
        let details = error.details().expect("details for a provider error");
        assert!(details.starts_with("HTTP "), "{details}");
        assert!(!error.to_string().contains("sk-test"), "{error}");
        assert!(!details.contains(FAKE_KEY), "{details}");
        assert_eq!(fake.requests().len(), 1, "{expected:?} isn't retried");
    }
}

#[test]
fn an_unsupported_parameter_that_mentions_tools_is_not_a_model_without_tools() {
    // What Chat Completions answered for the user's model: it names function tools, but the
    // parameter it refuses is another one, so it must not read as "this model can't use tools".
    let (provider, _) = setup(vec![Reply::status(
        400,
        error_body(
            "unsupported_parameter",
            Some("reasoning_effort"),
            "Function tools with reasoning_effort are not supported for gpt-6.1-sol in /v1/chat/completions. Please use /v1/responses instead.",
        ),
    )]);
    let error = turn(&provider, &hi()).0.unwrap_err();
    assert_eq!(
        error.root(),
        &AiError::UnsupportedParameter {
            provider: ProviderId::Openai,
            model: "gpt-4.1".into(),
            param: "reasoning_effort".into(),
        }
    );
    assert!(
        error
            .details()
            .unwrap()
            .contains("Function tools with reasoning_effort")
    );

    // Neither is a refused tool option (only `tools` itself, or a tool's type, means no tools).
    let (provider, _) = setup(vec![Reply::status(
        400,
        error_body(
            "unsupported_parameter",
            Some("parallel_tool_calls"),
            "Unsupported parameter: 'parallel_tool_calls' is not supported with this model.",
        ),
    )]);
    assert!(matches!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        AiError::UnsupportedParameter { param, .. } if param == "parallel_tool_calls"
    ));
}

#[test]
fn errors_in_the_stream_are_classified_too() {
    let failed = [json!({
        "type": "response.failed",
        "response": { "status": "failed", "error": { "code": "server_error", "message": "The server had an error while processing your request." }, "output": [] }
    })];
    let (provider, _) = setup(vec![Reply::ok(sse(&failed))]);
    let error = turn(&provider, &hi()).0.unwrap_err();
    assert_eq!(error.root(), &AiError::Overloaded(ProviderId::Openai));
    assert!(error.details().unwrap().contains("server_error"));

    let limited =
        [json!({ "type": "error", "code": "rate_limit_exceeded", "message": "Slow down.", "param": null })];
    let (provider, _) = setup(vec![Reply::ok(sse(&limited))]);
    assert_eq!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        &AiError::RateLimited(ProviderId::Openai)
    );

    let policy = [json!({
        "type": "response.failed",
        "response": { "status": "failed", "error": { "code": "bio_policy", "message": "Blocked." }, "output": [] }
    })];
    let (provider, _) = setup(vec![Reply::ok(sse(&policy))]);
    assert_eq!(turn(&provider, &hi()).0.unwrap_err().root(), &AiError::Refused);
}

#[test]
fn retries_timeouts_cancel_and_a_dropped_stream() {
    let limited = || {
        Reply::status(429, error_body("rate_limit_exceeded", None, "Rate limit reached")).with_retry_after(1)
    };
    let (provider, fake) = setup(vec![limited(), Reply::ok(TEXT)]);
    assert!(turn(&provider, &hi()).0.is_ok());
    assert_eq!(fake.requests().len(), 2);

    let (provider, _) = setup(vec![limited(), limited(), limited()]);
    assert_eq!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        &AiError::RateLimited(ProviderId::Openai)
    );

    // Out of credit comes as a 429 too, but waiting won't help.
    let (provider, fake) = setup(vec![
        Reply::status(
            429,
            error_body("insufficient_quota", None, "You exceeded your current quota."),
        )
        .with_retry_after(1),
        Reply::ok(TEXT),
    ]);
    assert_eq!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        &AiError::Billing(ProviderId::Openai)
    );
    assert_eq!(fake.requests().len(), 1);

    // A request that may have reached OpenAI is never sent twice.
    for (reply, expected) in [
        (Reply::Timeout, AiError::Timeout(ProviderId::Openai)),
        (Reply::Failed, AiError::Network(ProviderId::Openai)),
        (Reply::status(503, "{}"), AiError::Overloaded(ProviderId::Openai)),
    ] {
        let (provider, fake) = setup(vec![reply, Reply::ok(TEXT)]);
        assert_eq!(turn(&provider, &hi()).0.unwrap_err().root(), &expected);
        assert_eq!(fake.requests().len(), 1);
    }

    let (provider, _) = setup(vec![Reply::Unreachable, Reply::Unreachable, Reply::Unreachable]);
    assert_eq!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        &AiError::Network(ProviderId::Openai)
    );

    let cut = PARALLEL
        .split("event: response.completed")
        .next()
        .unwrap()
        .to_string();
    let (provider, _) = setup(vec![Reply::ok(cut)]);
    assert_eq!(
        turn(&provider, &hi()).0.unwrap_err().root(),
        &AiError::Interrupted(ProviderId::Openai)
    );

    let (provider, fake) = setup(vec![Reply::ok(TEXT)]);
    let cancel = Cancel::new();
    cancel.cancel();
    let tools = tools();
    let messages = hi();
    let request = TurnRequest {
        model: "gpt-4.1",
        system: "s",
        tools: &tools,
        messages: &messages,
        max_tokens: 10,
    };
    assert_eq!(
        provider
            .stream_turn(&fake_key(), &request, &cancel, &mut |_| {})
            .unwrap_err(),
        AiError::Cancelled
    );
    assert!(fake.requests().is_empty());
}

#[test]
fn the_model_list_keeps_models_that_can_call_function_tools() {
    let (provider, fake) = setup(vec![Reply::ok(MODELS)]);
    let models = provider.list_models(&fake_key(), &Cancel::new()).unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    // Suggested first (the newest full-size general GPT), then newest first.
    assert_eq!(
        ids,
        [
            "gpt-5.1",
            "gpt-5.1-mini",
            "gpt-5-pro",
            "gpt-5-codex",
            "o4-mini",
            "gpt-4.1"
        ]
    );
    assert!(models[0].recommended);
    assert_eq!(fake.requests()[0].url, "https://api.openai.com/v1/models");
    assert_eq!(
        fake.requests()[0].header("authorization").unwrap(),
        format!("Bearer {FAKE_KEY}")
    );
}

#[test]
fn the_filter_rule() {
    for id in [
        "gpt-5.1",
        "gpt-6.1-sol",
        "gpt-4.1-nano",
        "gpt-4o",
        "o3",
        "o4-mini",
        "o1",
        "gpt-3.5-turbo",
        // Responses-only families that call function tools.
        "gpt-5-pro",
        "o3-pro",
        "gpt-5-codex",
        "gpt-5.1-codex-max",
        "codex-mini-latest",
    ] {
        assert!(is_chat_tool_model(id), "{id} is offered");
    }
    for id in [
        "gpt-4o-audio-preview",
        "gpt-realtime",
        "gpt-4o-mini-tts",
        "gpt-4o-transcribe",
        "gpt-image-1",
        "gpt-3.5-turbo-instruct",
        "gpt-4o-search-preview",
        "o3-deep-research",
        "computer-use-preview",
        "o1-mini",
        "o1-preview",
        "text-embedding-3-large",
        "omni-moderation-latest",
        "whisper-1",
        "dall-e-3",
        "davinci-002",
        "chatgpt-4o-latest",
    ] {
        assert!(!is_chat_tool_model(id), "{id} is not offered");
    }
}
