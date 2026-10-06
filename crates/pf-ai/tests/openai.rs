//! The OpenAI provider against hand-written Chat Completions replies (SSE and JSON). Nothing
//! here touches the network.

use pf_ai::http::RetryPolicy;
use pf_ai::openai::{OpenAi, is_chat_tool_model};
use pf_ai::provider::{
    AssistantTurn, Message, StopReason, StreamEvent, ToolCall, ToolResult, ToolSpec, TurnRequest,
};
use pf_ai::testing::{FAKE_KEY, FakeTransport, Reply, fake_key};
use pf_ai::{AiError, Cancel, LlmProvider, ProviderId};
use serde_json::json;
use std::sync::Arc;

const TOOL_CALLS: &str = include_str!("fixtures/openai_tool_calls.sse");
const TEXT: &str = include_str!("fixtures/openai_text.sse");
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

fn turn(provider: &OpenAi, messages: &[Message]) -> (Result<AssistantTurn, AiError>, Vec<StreamEvent>) {
    let tools = tools();
    let request = TurnRequest {
        model: "gpt-5.1",
        system: "You are a test.",
        tools: &tools,
        messages,
        max_tokens: 1000,
    };
    let mut events = Vec::new();
    let result = provider.stream_turn(&fake_key(), &request, &Cancel::new(), &mut |e| events.push(e));
    (result, events)
}

fn error_body(code: &str, message: &str) -> String {
    json!({ "error": { "message": message, "type": "invalid_request_error", "param": null, "code": code } })
        .to_string()
}

#[test]
fn streamed_tool_calls_are_assembled_by_index() {
    let (provider, fake) = setup(vec![Reply::ok(TOOL_CALLS)]);
    let (result, events) = turn(&provider, &[Message::User("Rename it".into())]);
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
        name: "list_props".into()
    }));

    let request = &fake.requests()[0];
    assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
    assert_eq!(
        request.header("authorization").unwrap(),
        format!("Bearer {FAKE_KEY}")
    );
    assert!(!format!("{request:?}").contains(FAKE_KEY));
    let body = fake.body(0);
    assert_eq!(body["model"], "gpt-5.1");
    assert_eq!(body["stream"], true);
    assert_eq!(
        body["messages"][0],
        json!({ "role": "system", "content": "You are a test." })
    );
    assert_eq!(
        body["messages"][1],
        json!({ "role": "user", "content": "Rename it" })
    );
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "show_rename_show");
    assert_eq!(
        body["tools"][0]["function"]["parameters"]["required"],
        json!(["name"])
    );
}

#[test]
fn tool_calls_and_results_go_back_in_openais_shape() {
    let (provider, fake) = setup(vec![Reply::ok(TOOL_CALLS), Reply::ok(TEXT)]);
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
    let messages = fake.body(1)["messages"].clone();
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["content"], "Renaming it now.");
    assert_eq!(messages[2]["tool_calls"][0]["id"], "call_fixture_a");
    assert_eq!(
        messages[2]["tool_calls"][0]["function"],
        json!({ "name": "show_rename_show", "arguments": "{\"name\":\"Christmas 2026\"}" })
    );
    assert_eq!(
        messages[3],
        json!({ "role": "tool", "tool_call_id": "call_fixture_a", "content": "Done (in your draft)." })
    );
    assert_eq!(messages[4]["content"], "Error: There's no prop with that id.");
}

#[test]
fn errors_become_plain_messages_without_the_key() {
    let p = ProviderId::Openai;
    let model = || "gpt-5.1".to_string();
    let cases: Vec<(Reply, AiError)> = vec![
        (
            Reply::status(
                401,
                error_body("invalid_api_key", "Incorrect API key provided: sk-test-****-key."),
            ),
            AiError::InvalidKey(p),
        ),
        (
            Reply::status(
                429,
                error_body("insufficient_quota", "You exceeded your current quota."),
            ),
            AiError::Billing(p),
        ),
        (
            Reply::status(
                404,
                error_body(
                    "model_not_found",
                    "The model `gpt-5.1` does not exist or you do not have access to it.",
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
                error_body("unsupported_parameter", "tools is not supported in this model."),
            ),
            AiError::ModelNoTools {
                provider: p,
                model: model(),
            },
        ),
        (
            Reply::status(
                404,
                error_body(
                    "",
                    "This is not a chat model and thus not supported in the v1/chat/completions endpoint.",
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
                    "context_length_exceeded",
                    "This model's maximum context length is 128000 tokens.",
                ),
            ),
            AiError::TooLong,
        ),
        (
            Reply::status(400, error_body("", &format!("Something odd about {FAKE_KEY}"))),
            AiError::Provider {
                provider: p,
                message: "Something odd about [your key]".into(),
            },
        ),
    ];
    for (reply, expected) in cases {
        let (provider, fake) = setup(vec![reply]);
        let error = turn(&provider, &[Message::User("Hi".into())]).0.unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("sk-test"), "{error}");
        assert_eq!(fake.requests().len(), 1, "{expected:?} isn't retried");
    }
}

#[test]
fn rate_limits_are_retried_and_a_dropped_stream_is_reported() {
    let limited = || Reply::status(429, error_body("rate_limit_exceeded", "Rate limit reached"));
    let (provider, fake) = setup(vec![limited(), Reply::ok(TEXT)]);
    assert!(turn(&provider, &[Message::User("Hi".into())]).0.is_ok());
    assert_eq!(fake.requests().len(), 2);

    let (provider, _) = setup(vec![limited(), limited(), limited()]);
    assert_eq!(
        turn(&provider, &[Message::User("Hi".into())]).0.unwrap_err(),
        AiError::RateLimited(ProviderId::Openai)
    );

    let cut = TOOL_CALLS
        .split("\"finish_reason\":\"tool_calls\"")
        .next()
        .unwrap();
    let cut = cut.rsplit_once("data:").unwrap().0.to_string();
    let (provider, _) = setup(vec![Reply::ok(cut)]);
    assert_eq!(
        turn(&provider, &[Message::User("Hi".into())]).0.unwrap_err(),
        AiError::Interrupted(ProviderId::Openai)
    );

    let (provider, _) = setup(vec![Reply::Unreachable, Reply::Unreachable, Reply::Unreachable]);
    assert_eq!(
        turn(&provider, &[Message::User("Hi".into())]).0.unwrap_err(),
        AiError::Network(ProviderId::Openai)
    );
}

#[test]
fn refusals_and_length_stops() {
    let refusal = "data: {\"choices\":[{\"index\":0,\"delta\":{\"refusal\":\"I can't help with that.\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let (provider, _) = setup(vec![Reply::ok(refusal)]);
    assert_eq!(
        turn(&provider, &[Message::User("Hi".into())]).0.unwrap().stop,
        StopReason::Refusal
    );

    let length = TOOL_CALLS
        .replace("\"finish_reason\":\"tool_calls\"", "\"finish_reason\":\"length\"")
        .replace("\\\"Christmas 2026\\\"}", "\\\"Chris");
    let (provider, _) = setup(vec![Reply::ok(length)]);
    let turn = turn(&provider, &[Message::User("Hi".into())]).0.unwrap();
    assert_eq!(turn.stop, StopReason::MaxTokens);
    assert!(turn.tool_calls[0].input_error.is_some());
}

#[test]
fn the_model_list_keeps_chat_models_that_can_call_tools() {
    let (provider, fake) = setup(vec![Reply::ok(MODELS)]);
    let models = provider.list_models(&fake_key(), &Cancel::new()).unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    // Suggested first (the newest full-size GPT), then newest first.
    assert_eq!(ids, ["gpt-5.1", "gpt-5.1-mini", "o4-mini", "gpt-4.1"]);
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
        "gpt-4.1-nano",
        "gpt-4o",
        "o3",
        "o4-mini",
        "o1",
        "gpt-3.5-turbo",
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
        "gpt-5-pro",
        "o3-pro",
        "gpt-5-codex",
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
