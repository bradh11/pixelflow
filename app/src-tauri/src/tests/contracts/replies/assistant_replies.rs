//! Replies from the assistant and Find lyrics, with a credential store in memory and the
//! providers' replies scripted: no key leaves the test, and nothing is asked of anyone.

use super::sequence_replies::{row, row_id};
use super::*;
use pf_ai::testing::{FAKE_KEY, Reply};

/// One streamed Anthropic reply: optional text, then optional tool calls.
fn sse(text: &str, tools: &[(&str, Value)]) -> String {
    let mut out = String::from("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n");
    let mut index = 0;
    let mut block = |start: Value, delta: Value| {
        let block = format!(
            "event: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\n",
            json!({ "type": "content_block_start", "index": index, "content_block": start }),
            json!({ "type": "content_block_delta", "index": index, "delta": delta }),
            json!({ "type": "content_block_stop", "index": index }),
        );
        index += 1;
        block
    };
    if !text.is_empty() {
        out += &block(
            json!({ "type": "text", "text": "" }),
            json!({ "type": "text_delta", "text": text }),
        );
    }
    for (i, (name, input)) in tools.iter().enumerate() {
        out += &block(
            json!({ "type": "tool_use", "id": format!("toolu_{i}"), "name": name, "input": {} }),
            json!({ "type": "input_json_delta", "partial_json": input.to_string() }),
        );
    }
    let stop = if tools.is_empty() { "end_turn" } else { "tool_use" };
    out + &format!(
        "event: message_delta\ndata: {}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
        json!({ "type": "message_delta", "delta": { "stop_reason": stop } })
    )
}

fn send(message: &str) -> Value {
    json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": message,
            "context": { "screen": "sequence", "selectedProps": [prop_id(1)], "selectedEffects": [], "playheadMs": 500 } })
}

/// Made-up lyrics, as LRCLIB answers a search.
fn lrclib_reply() -> String {
    json!([
        { "id": 1, "trackName": "Lantern Song", "artistName": "Lantern Band", "albumName": "Made Up", "duration": 4.0,
          "instrumental": false, "plainLyrics": "Paper lanterns glowing\nSnowy rooftops shine",
          "syncedLyrics": "[00:00.50]Paper lanterns glowing\n[00:02.00]Snowy rooftops shine\n[00:03.50]" },
        { "id": 2, "trackName": "Lantern Song (Live)", "artistName": "Lantern Band", "albumName": "Made Up Live", "duration": 4.5,
          "instrumental": false, "plainLyrics": "Paper lanterns glowing\nSnowy rooftops shining", "syncedLyrics": null },
    ])
    .to_string()
}

/// A new sequence whose music is a silent 4 s WAV named like the song.
fn sequence_with_song(shell: &Shell) {
    let song = shell.dir.path().join("01 - Lantern Song.wav");
    std::fs::write(&song, pf_audio::wav_bytes(&vec![0.0; 64_000], 16_000)).unwrap();
    shell.ok(
        "new_sequence_doc",
        json!({ "name": "Lantern Song", "durationMs": 4000, "audio": song, "rows": [row(1, json!({ "prop": prop_id(1) }))] }),
    );
}

#[test]
fn assistant_replies() {
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("assistant", shell.stable());
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": strip(1, "Strip", 4) }] }),
    );

    // Keys go in and never come back: only where they're kept.
    fixtures.reply(
        "a credential store",
        "assistant.KeyStorage",
        &shell.ok("ai_key_storage", json!({})),
    );
    fixtures.reply(
        "no key yet",
        "boolean",
        &shell.ok("has_api_key", json!({ "provider": "anthropic" })),
    );
    fixtures.reply(
        "no key, so nowhere it's kept",
        "assistant.KeyLocation | null",
        &shell.ok("api_key_location", json!({ "provider": "anthropic" })),
    );
    fixtures.error(
        "a turn without a key",
        "ai_send",
        "{ message: string; details: string | null }",
        &shell.refused("ai_send", send("hi")),
    );
    fixtures.reply(
        "a key saved",
        "assistant.KeyLocation",
        &shell.ok("set_api_key", json!({ "provider": "anthropic", "key": FAKE_KEY })),
    );
    fixtures.reply(
        "a key kept for the session",
        "assistant.KeyLocation",
        &shell.ok(
            "use_api_key_for_session",
            json!({ "provider": "openai", "key": FAKE_KEY }),
        ),
    );
    fixtures.reply(
        "where a key is kept",
        "assistant.KeyLocation | null",
        &shell.ok("api_key_location", json!({ "provider": "openai" })),
    );
    let without = Shell::with_keys(pf_ai::MemoryStore::unavailable());
    fixtures.reply(
        "no credential store",
        "assistant.KeyStorage",
        &without.ok("ai_key_storage", json!({})),
    );

    shell.anthropic.push(Reply::ok(
        json!({ "data": [
            { "type": "model", "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5" },
            { "type": "model", "id": "claude-haiku-5", "display_name": "Claude Haiku 5" },
        ], "has_more": false })
        .to_string(),
    ));
    fixtures.reply(
        "a provider's models",
        "assistant.ModelInfo[]",
        &shell.ok("list_ai_models", json!({ "provider": "anthropic" })),
    );
    shell.anthropic.push(Reply::status(
        401,
        json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key" } }).to_string(),
    ));
    let refused = shell.refused("list_ai_models", json!({ "provider": "anthropic" }));
    assert!(refused["details"].is_string(), "{refused}");
    fixtures.error(
        "a provider that refused the key",
        "list_ai_models",
        "{ message: string; details: string | null }",
        &refused,
    );

    // No sequence open: the assistant offers the song picker.
    shell.anthropic.push(Reply::ok(sse(
        "You don't have a sequence open yet.",
        &[("ask_for_song", json!({}))],
    )));
    shell
        .anthropic
        .push(Reply::ok(sse("Pick a song and I'll build it.", &[])));
    let asked = shell.ok("ai_send", send("Make me a sequence"));
    assert_eq!(
        (&asked["chooseSong"], &asked["proposal"]),
        (&json!(true), &Value::Null)
    );
    fixtures.reply("a reply that asks for a song", "assistant.TurnReply", &asked);

    // A change to the show, proposed, then applied.
    let mut tree = strip(2, "Mega Tree", 100);
    tree["shape"] = json!({ "source": "generator", "type": "tree", "strings": 4, "nodesPerString": 25, "height": 3,
        "baseRadius": 1, "topRadius": 0.1 });
    shell.anthropic.push(Reply::ok(sse(
        "Adding a tree.",
        &[
            ("show_rename_show", json!({ "name": "Christmas" })),
            ("show_add_prop", json!({ "prop": tree })),
        ],
    )));
    shell.anthropic.push(Reply::ok(sse(
        "",
        &[(
            "propose_changes",
            json!({ "summary": "Renames the show and adds a tree." }),
        )],
    )));
    shell.anthropic.push(Reply::ok(sse("Take a look.", &[])));
    let drafted = shell.ok("ai_send", send("Call the show Christmas and add a tree"));
    let proposal = &drafted["proposal"];
    assert_eq!(
        (&proposal["changesShow"], &proposal["changesSequence"]),
        (&json!(true), &json!(false))
    );
    assert!(!proposal["changedProps"].as_array().unwrap().is_empty());
    // (A proposal's id is made at random.)
    fixtures.reply(
        "a reply with a change to the show to review",
        "assistant.TurnReply",
        &drafted,
    );
    fixtures.reply(
        "nothing replaced the show, so the proposal stands",
        "boolean",
        &shell.ok("ai_sync", json!({})),
    );
    let applied = shell.ok("ai_apply", json!({ "id": proposal["id"] }));
    assert!(applied["snapshot"].is_object() && applied["sequence"].is_null());
    fixtures.reply("a show proposal applied", "assistant.Applied", &applied);

    // A change to the open sequence.
    sequence_with_song(&shell);
    shell.anthropic.push(Reply::ok(sse(
        "",
        &[(
            "place_effects",
            json!({ "rowIds": [row_id(1)], "fromMs": 0, "toMs": 2000, "effect": { "kind": "twinkle", "colors": ["#ff0000"] } }),
        )],
    )));
    shell
        .anthropic
        .push(Reply::ok(sse("", &[("review_draft", json!({}))])));
    shell.anthropic.push(Reply::ok(sse(
        "",
        &[("propose_changes", json!({ "summary": "Twinkles the strip." }))],
    )));
    shell.anthropic.push(Reply::ok(sse("Here it is.", &[])));
    let drafted = shell.ok("ai_send", send("Twinkle the strip"));
    let proposal = &drafted["proposal"];
    assert_eq!(proposal["changesSequence"], true, "{drafted}");
    assert!(
        proposal["timeline"].is_object() && !proposal["sections"].as_array().unwrap().is_empty(),
        "{proposal}"
    );
    fixtures.reply(
        "a reply with a change to the sequence to review",
        "assistant.TurnReply",
        &drafted,
    );
    let applied = shell.ok("ai_apply", json!({ "id": proposal["id"] }));
    assert!(applied["sequence"].is_object(), "{applied}");
    fixtures.reply("a sequence proposal applied", "assistant.Applied", &applied);
    // Everything a proposal's card can show at once (built: cues and a review come from a song
    // with moments in it).
    let card = pf_ai::ProposalView {
        id: "proposal-1".into(),
        summary: "Stages the chorus.".into(),
        diff: pf_ai::Diff {
            changes: vec![pf_ai::Change {
                section: pf_ai::Section::Controller,
                action: pf_ai::Action::Changed,
                name: "Porch".into(),
                id: Some("33333333-0000-4000-8000-000000000001".into()),
                details: vec!["address: \"192.0.2.10\" → \"192.0.2.99\"".into()],
                warnings: vec!["This changes where light data is sent.".into()],
            }],
        },
        changed_props: vec![prop_id(1)],
        changes_show: true,
        changes_sequence: true,
        sections: vec![pf_ai::summary::SectionSummary {
            label: "Chorus".into(),
            start_ms: 0,
            end_ms: 2000,
            rows: 1,
            added: 2,
            changed: 1,
            removed: 1,
            kinds: vec!["Twinkle".into()],
        }],
        timeline: Some(pf_ai::summary::Timeline {
            duration_ms: 4000,
            sections: vec![pf_ai::summary::TimelineSection {
                label: "Chorus".into(),
                start_ms: 0,
                end_ms: 2000,
            }],
            rows: vec![pf_ai::summary::TimelineRow {
                name: "Strip".into(),
                effects: vec![pf_ai::summary::TimelineEffect {
                    start_ms: 0,
                    end_ms: 2000,
                    color: "#ff0000".into(),
                }],
            }],
            more_rows: 3,
        }),
        locked_edges: 4,
        cues: Some("Staged 2 cues: 1 hit, 1 word pop.".into()),
        review: Some(pf_ai::review::ReviewView {
            score: 92,
            line: "Review: 92/100 · all 12 top moments emphasised · flashes safe".into(),
            items: vec!["0:02.000 nothing marks the drop".into()],
        }),
    };
    fixtures.reply(
        "a proposal with cues, a review, and a warning (built)",
        "assistant.ProposalView",
        &wire(&card),
    );
    // What arrives while a reply streams in, one of each kind (built: see the events for real ones).
    use pf_ai::ChatEvent as E;
    let events = vec![
        E::Text {
            text: "Adding a tree.".into(),
        },
        E::Activity {
            label: "Looking at your props".into(),
        },
        E::Retrying { seconds: 4 },
        E::Proposal { proposal: card },
        E::ChooseSong,
    ];
    assert_every_variant(
        "chat event",
        5,
        events.iter().map(|e| match e {
            E::Text { .. } => 0,
            E::Activity { .. } => 1,
            E::Retrying { .. } => 2,
            E::Proposal { .. } => 3,
            E::ChooseSong => 4,
        }),
    );
    fixtures.all_of("assistant.ChatEvent", Some("kind"), &wire(&events));

    // Find lyrics: not without the assistant set up; then published lyrics, timed by line.
    fixtures.reply(
        "lyrics need the assistant set up",
        "sequencer.LyricsGate",
        &shell.ok("lyrics_gate", json!({ "provider": null })),
    );
    fixtures.reply(
        "lyrics with a key that can't hear the song",
        "sequencer.LyricsGate",
        &shell.ok("lyrics_gate", json!({ "provider": "anthropic" })),
    );
    shell.ok("set_api_key", json!({ "provider": "openai", "key": FAKE_KEY }));
    fixtures.reply(
        "lyrics with a key that can hear the song",
        "sequencer.LyricsGate",
        &shell.ok("lyrics_gate", json!({ "provider": "openai" })),
    );
    shell.lrclib.push(Reply::ok(lrclib_reply()));
    let found = shell.ok(
        "find_lyrics",
        json!({ "provider": "anthropic", "upload": false, "language": "en", "fresh": false, "align": false }),
    );
    assert_eq!(found["candidates"].as_array().unwrap().len(), 2, "{found}");
    assert!(found["chosen"].is_number());
    fixtures.reply("published lyrics found", "sequencer.LyricsFound", &found);
    // Heard by OpenAI too: the words get their own times, locked to the vocals.
    shell.lrclib.push(Reply::ok(lrclib_reply()));
    shell.whisper.push(Reply::ok(
        json!({ "text": "paper lanterns glowing", "words": [
            { "word": "paper", "start": 0.6, "end": 0.9 },
            { "word": "lanterns", "start": 0.9, "end": 1.4 },
            { "word": "glowing", "start": 1.4, "end": 1.9 },
        ] })
        .to_string(),
    ));
    let heard_too = shell.ok(
        "find_lyrics",
        json!({ "provider": "openai", "upload": true, "language": "en", "fresh": true, "align": false }),
    );
    fixtures.reply("lyrics found and heard", "sequencer.LyricsFound", &heard_too);
    let pasted = shell.ok(
        "choose_lyrics",
        json!({ "choice": { "pasted": "Paper lanterns glowing" }, "align": false }),
    );
    assert_eq!(
        (&pasted["pasted"], &pasted["chosen"]),
        (&json!(true), &Value::Null)
    );
    fixtures.reply("pasted lyrics lined up instead", "sequencer.LyricsFound", &pasted);
    let other = shell.ok(
        "choose_lyrics",
        json!({ "choice": { "candidate": 2 }, "align": false }),
    );
    assert_eq!(other["chosen"], 2);
    fixtures.reply(
        "another of the published lyrics chosen",
        "sequencer.LyricsFound",
        &other,
    );
    let doc = shell.ok("get_sequence_doc", json!({}));
    let lyrics = doc["sequence"]["timingTracks"][0]["id"].clone();
    fixtures.reply(
        "the words locked onto the vocals again",
        "sequencer.LyricsRetimed",
        &shell.ok("retime_lyrics", json!({ "track": lyrics })),
    );
    fixtures.reply(
        "the alignment model, not downloaded",
        "assistant.AlignModels",
        &shell.ok("alignment_models", json!({})),
    );
    fixtures.finish();
}

/// The events a chat turn and Find lyrics send, heard on `shell`; the names of those recorded.
pub(super) fn event_payloads(shell: &Shell, fixtures: &mut Fixtures) -> Vec<&'static str> {
    shell.ok("set_api_key", json!({ "provider": "anthropic", "key": FAKE_KEY }));
    shell.anthropic.push(Reply::ok(sse(
        "Renaming it.",
        &[("show_rename_show", json!({ "name": "Christmas" }))],
    )));
    shell.anthropic.push(Reply::ok(sse(
        "",
        &[("propose_changes", json!({ "summary": "Renames the show." }))],
    )));
    shell.anthropic.push(Reply::ok(sse("Take a look.", &[])));
    let (_, turn) = heard(&shell.app, assistant::ASSISTANT_EVENT, || {
        shell.ok("ai_send", send("Call the show Christmas"))
    });
    let payload = "{ turn: number; event: assistant.ChatEvent }";
    for kind in ["text", "activity", "proposal"] {
        let event = turn
            .iter()
            .find(|e| e["event"]["kind"] == kind)
            .unwrap_or_else(|| panic!("no {kind} in {turn:?}"));
        fixtures.event(
            &format!("a chat turn's {kind}"),
            assistant::ASSISTANT_EVENT,
            payload,
            event,
        );
    }

    sequence_with_song(shell);
    shell.lrclib.push(Reply::ok(lrclib_reply()));
    let (_, steps) = heard(&shell.app, lyrics::LYRICS_PROGRESS_EVENT, || {
        shell.ok("find_lyrics", json!({ "provider": "anthropic", "upload": false, "language": "en", "fresh": true, "align": false }))
    });
    let payload = "{ label: string; fraction: number | null }";
    let start = steps
        .iter()
        .find(|s| s["fraction"].is_null())
        .unwrap_or_else(|| panic!("no step starts in {steps:?}"));
    fixtures.event(
        "Find lyrics starting a step",
        lyrics::LYRICS_PROGRESS_EVENT,
        payload,
        start,
    );
    let part = lyrics::LyricsProgress {
        label: "Lining up the words",
        fraction: Some(0.5),
    };
    fixtures.event(
        "Find lyrics part way through a song (built)",
        lyrics::LYRICS_PROGRESS_EVENT,
        payload,
        &wire(&part),
    );
    vec![assistant::ASSISTANT_EVENT, lyrics::LYRICS_PROGRESS_EVENT]
}
