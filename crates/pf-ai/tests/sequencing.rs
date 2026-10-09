//! Making a sequence with the assistant: asking for a song when none is open, reading the song
//! (beats, bars, sections), and the high-level tools that place many effects at once. All of it
//! lands in the draft, locked to the music before it's proposed; Apply is one sequence undo step;
//! nothing reaches files, output, or devices.

use pf_ai::provider::Message;
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{AiError, Cancel, ChatEvent, ChatSession, TurnReply, UiContext, Workspace, apply_proposal};
use pf_analysis::{Analysis, BarEnergy, Confidence, Event, EventKind};
use pf_engine::{Edit, Engine, SequenceEdit};
use pf_model::{Generator, Group, GroupMember, Prop, ShapeSource};
use pf_sequence::{EffectKind, Mark, Row, Target, TimingKind, TimingTrack};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn line(name: &str) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 2.0,
        }),
    )
}

struct Setup {
    engine: Engine,
    dir: tempfile::TempDir,
    recorded: pf_output::Recorded,
    /// Row ids: A, B, C, then the group.
    rows: Vec<String>,
}

/// A show with three props and a group of them; with `song`, a new sequence on it with a row
/// for each.
fn setup(song: Option<&str>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = pf_output::RecordingTransport::new();
    let mut engine = Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone())));
    let props: Vec<Prop> = ["A", "B", "C"].into_iter().map(line).collect();
    let mut group = Group::new("All");
    group.members = props.iter().map(|p| GroupMember::Prop(p.id)).collect();
    let mut edits: Vec<Edit> = props.iter().map(|p| Edit::AddProp { prop: p.clone() }).collect();
    edits.push(Edit::AddGroup { group: group.clone() });
    engine.apply(edits).unwrap();
    let mut rows: Vec<Row> = props.iter().map(|p| Row::new(Target::Prop(p.id))).collect();
    rows.push(Row::new(Target::Group(group.id)));
    let ids = rows.iter().map(|r| r.id.to_string()).collect();
    if let Some(song) = song {
        engine
            .new_sequence_doc_with_rows("Jingle", 32_000, Some(song), rows)
            .unwrap();
    }
    Setup {
        engine,
        dir,
        recorded,
        rows: ids,
    }
}

/// A 32 s song at 120 BPM: quiet for 8 s, loud for 16 s, quiet for 8 s; a build into the loud
/// part, a drop at its start, and a hit in it.
fn song_analysis() -> Analysis {
    let beats: Vec<u64> = (0..64).map(|i| i * 500).collect();
    let energy: Vec<f32> = (0..32)
        .map(|s| if (8..24).contains(&s) { 0.9 } else { 0.2 })
        .collect();
    let bar_energy = (0..16)
        .map(|b| {
            let e = if (4..12).contains(&b) { 0.9 } else { 0.2 };
            BarEnergy {
                overall: e,
                low: e - 0.1,
                mid: e,
                high: e,
            }
        })
        .collect();
    let event = |time_ms, kind, strength, duration_ms| Event {
        time_ms,
        kind,
        strength,
        duration_ms,
    };
    Analysis {
        duration_ms: 32_000,
        tempo_bpm: Some(120.0),
        bars: beats.iter().copied().step_by(4).collect(),
        onsets: beats.clone(),
        beats,
        energy,
        events: vec![
            event(4_000, EventKind::Build, 0.6, Some(4_000)),
            event(8_000, EventKind::Drop, 1.0, None),
            event(16_250, EventKind::Hit, 0.8, None),
        ],
        bar_energy,
        confidence: Confidence {
            tempo: 0.9,
            downbeat: 0.7,
            sections: 0.0,
        },
        ..Analysis::default()
    }
}

/// A session whose song analysis is `song_analysis`, counting how often it runs.
fn session() -> (ChatSession, Arc<AtomicUsize>) {
    let runs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&runs);
    let session = ChatSession::new().with_analyzer(Arc::new(move |_: &Path, _: &Cancel| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(song_analysis())
    }));
    (session, runs)
}

fn ask(
    session: &mut ChatSession,
    provider: &ScriptedProvider,
    engine: &Engine,
    text: &str,
) -> (Result<TurnReply, AiError>, Vec<ChatEvent>) {
    let workspace = Workspace::from_engine(engine, UiContext::default());
    let mut events = Vec::new();
    let reply = session.run_turn(
        provider,
        &fake_key(),
        "scripted",
        text,
        workspace,
        &Cancel::new(),
        &mut |e| events.push(e),
    );
    (reply, events)
}

fn results_in(provider: &ScriptedProvider, n: usize) -> Vec<(String, bool)> {
    match provider.requests()[n].last() {
        Some(Message::ToolResults(results)) => {
            results.iter().map(|r| (r.content.clone(), r.is_error)).collect()
        }
        other => panic!("expected tool results, got {other:?}"),
    }
}

fn first_user_message(provider: &ScriptedProvider) -> String {
    match &provider.requests()[0][0] {
        Message::User(text) => text.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn with_no_sequence_open_the_assistant_offers_to_choose_a_song() {
    let s = setup(None);
    let provider = ScriptedProvider::new(vec![
        calls(
            "You don't have a sequence open yet.",
            &[
                (
                    "place_effects",
                    json!({ "rowIds": [], "fromMs": 0, "toMs": 1000, "effect": { "kind": "on" } }),
                ),
                ("ask_for_song", json!({})),
            ],
        ),
        says("Pick a song and I'll take it from there."),
    ]);
    let (mut session, runs) = session();
    let (reply, events) = ask(
        &mut session,
        &provider,
        &s.engine,
        "can you create a compelling sequence for me?",
    );
    let reply = reply.unwrap();
    assert!(first_user_message(&provider).contains("Open sequence: none"));
    let results = results_in(&provider, 1);
    assert!(
        results[0].1 && results[0].0.contains("No sequence is open"),
        "{results:?}"
    );
    assert!(!results[1].1);
    assert!(results[1].0.contains("Choose a song"), "{}", results[1].0);
    assert!(reply.choose_song, "the reply offers the song picker");
    assert!(events.contains(&ChatEvent::ChooseSong));
    assert!(reply.proposal.is_none());
    assert_eq!(runs.load(Ordering::SeqCst), 0);
    // Nothing was created: the user picks the song, and the app makes the sequence.
    assert!(s.engine.sequence_document().is_none());

    // Then the chat carries on with the new sequence (nothing was drafted, so nothing "dropped").
    let mut engine = s.engine;
    engine
        .new_sequence_doc("Jingle", 32_000, Some("/music/jingle.mp3"))
        .unwrap();
    let provider = ScriptedProvider::new(vec![says("Listening to it now.")]);
    ask(&mut session, &provider, &engine, "I chose \"Jingle\".")
        .0
        .unwrap();
    let text = first_user_message(&provider);
    let sent = provider.requests()[0]
        .iter()
        .rev()
        .find_map(|m| match m {
            Message::User(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        sent.contains("Open sequence: \"Jingle\" (0:32.000, with a song)"),
        "{sent}"
    );
    assert!(!sent.contains("dropped"), "{sent}");
    assert!(
        text.contains("Open sequence: none"),
        "the first message is kept as it was"
    );
}

#[test]
fn the_song_is_analyzed_once_off_the_engine_and_summarized() {
    let s = setup(Some("/music/song.mp3"));
    let provider = ScriptedProvider::new(vec![
        calls("", &[("analyze_song", json!({}))]),
        says("A bright 120 BPM song."),
        calls("", &[("analyze_song", json!({}))]),
        says("Same song."),
    ]);
    let (mut session, runs) = session();
    let (reply, events) = ask(&mut session, &provider, &s.engine, "What's the song like?");
    reply.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::Activity { label } if label == "Listening to the song"))
    );
    let (text, is_error) = results_in(&provider, 1).remove(0);
    assert!(!is_error, "{text}");
    let summary: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(summary["tempoBpm"], 120.0);
    assert_eq!(summary["durationMs"], 32_000);
    assert_eq!(summary["beats"], 64);
    assert_eq!(summary["barsMs"].as_array().unwrap().len(), 16);
    let labels: Vec<&str> = summary["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Intro", "High 1", "Outro"]);
    assert_eq!(summary["sections"][1]["startMs"], 8_000);
    assert_eq!(summary["sections"][1]["level"], "high");
    assert_eq!(summary["sections"][0]["group"], "A");
    let accents: Vec<(&str, u64)> = summary["accents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["kind"].as_str().unwrap(), a["atMs"].as_u64().unwrap()))
        .collect();
    assert_eq!(accents, [("build", 4_000), ("drop", 8_000), ("hit", 16_250)]);
    assert_eq!(summary["accents"][0]["forMs"], 4_000);
    assert!(summary["accents"][1].get("forMs").is_none());
    assert_eq!(summary["barEnergy"], "2222888888882222", "a digit per bar");
    assert_eq!(summary["barBass"], "1111777777771111");
    assert_eq!(summary["confidence"]["downbeat"], 0.7);

    ask(&mut session, &provider, &s.engine, "And again?").0.unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 1, "kept for the chat");
}

#[test]
fn a_sequence_without_a_song_says_so_and_stop_ends_an_analysis() {
    let mut s = setup(None);
    s.engine.new_sequence_doc("Quiet", 10_000, None).unwrap();
    let provider = ScriptedProvider::new(vec![calls("", &[("analyze_song", json!({}))]), says("No song.")]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Beats?").0.unwrap();
    let (text, is_error) = results_in(&provider, 1).remove(0);
    assert!(is_error && text.contains("has no song"), "{text}");

    let s = setup(Some("/music/song.mp3"));
    let mut session = ChatSession::new().with_analyzer(Arc::new(|_: &Path, cancel: &Cancel| {
        // The user presses Stop while the song is being analyzed.
        cancel.cancel();
        Err("The analysis was stopped.".to_string())
    }));
    let provider = ScriptedProvider::new(vec![calls("", &[("analyze_song", json!({}))]), says("never")]);
    let workspace = Workspace::from_engine(&s.engine, UiContext::default());
    let cancel = Cancel::new();
    let result = session.run_turn(
        &provider,
        &fake_key(),
        "scripted",
        "Analyze it",
        workspace,
        &cancel,
        &mut |_| {},
    );
    assert_eq!(result.unwrap_err(), AiError::Cancelled);
}

#[test]
fn timing_tracks_come_from_the_song() {
    let s = setup(Some("/music/song.mp3"));
    let provider = ScriptedProvider::new(vec![
        calls("", &[("add_song_timing", json!({}))]),
        calls(
            "",
            &[(
                "add_song_timing",
                json!({ "tracks": ["bars", "onsets", "accents"] }),
            )],
        ),
        says("Timing is in."),
    ]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Add beats").0.unwrap();
    let first: Value = serde_json::from_str(&results_in(&provider, 1)[0].0).unwrap();
    let names: Vec<&str> = first
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Beats", "Bars", "Sections", "Accents"]);
    assert_eq!(first[0]["marks"], 64);
    assert_eq!(first[2]["marks"], 3);
    assert_eq!(first[3]["marks"], 3, "the build, the drop, and the hit");
    let second: Value = serde_json::from_str(&results_in(&provider, 2)[0].0).unwrap();
    assert_eq!(second[0]["name"], "Bars");
    assert_eq!(second[0]["added"], false, "already there: reused");
    assert_eq!(second[1]["name"], "Onsets");
    assert_eq!(second[2]["name"], "Accents");
    assert_eq!(second[2]["added"], false);

    let draft = session.draft().unwrap().sequence().unwrap();
    let kinds: Vec<TimingKind> = draft.timing_tracks.iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        [
            TimingKind::Beats,
            TimingKind::Bars,
            TimingKind::Sections,
            TimingKind::Custom,
            TimingKind::Custom
        ],
        "Accents, then Onsets"
    );
    assert!(
        s.engine.sequence_document().unwrap().timing_tracks.is_empty(),
        "draft only"
    );
}

/// Where the placed effects are, per row: (start, end) in ms, sorted.
fn placed(session: &ChatSession, row: &str, layer: usize) -> Vec<(u64, u64)> {
    let doc = session.draft().unwrap().sequence().unwrap();
    let row = doc.rows.iter().find(|r| r.id.to_string() == row).unwrap();
    let mut out: Vec<(u64, u64)> = row
        .layers
        .get(layer)
        .map(|l| l.effects.iter().map(|e| (e.start_ms, e.end_ms)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

#[test]
fn effects_go_on_many_rows_at_once_along_timing_marks() {
    let s = setup(Some("/music/song.mp3"));
    let [a, b, c, group] = [0, 1, 2, 3].map(|i| s.rows[i].clone());
    let twinkle = json!({ "kind": "twinkle", "settings": { "density": 0.5 }, "colors": ["#ff0000", "#00ff00"], "fadeInMs": 100 });
    let provider = ScriptedProvider::new(vec![
        calls("", &[("add_song_timing", json!({}))]),
        calls(
            "",
            &[
                // Every row, each bar of the intro.
                (
                    "place_effects",
                    json!({ "rowIds": [a, b, c], "fromMs": 0, "toMs": 8000, "track": "Bars", "effect": twinkle }),
                ),
                // One row after another, beat by beat.
                (
                    "place_effects",
                    json!({ "rowIds": [a, b, c], "fromMs": 8000, "toMs": 10000, "track": "Beats", "spread": "sweep", "effect": { "kind": "on" } }),
                ),
                // Taking turns, two beats each.
                (
                    "place_effects",
                    json!({ "rowIds": [a, b], "fromMs": 10000, "toMs": 14000, "track": "Beats", "marksEach": 2, "spread": "alternate", "effect": { "kind": "strobe" } }),
                ),
                // Joining in one by one.
                (
                    "place_effects",
                    json!({ "rowIds": [a, b, c], "fromMs": 14000, "toMs": 20000, "track": "Bars", "spread": "build", "effect": { "kind": "shimmer" } }),
                ),
                // One effect over the range on the group's row, above the others.
                (
                    "place_effects",
                    json!({ "rowIds": [group], "fromMs": 0, "toMs": 32000, "layer": 1, "effect": { "kind": "colorWash", "colors": ["#0000ff", "#ffffff"] } }),
                ),
            ],
        ),
        calls(
            "",
            &[
                // Overlaps the intro: refused, nothing changes.
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 4000, "toMs": 6000, "effect": { "kind": "fire" } }),
                ),
                // Unless it replaces what's there.
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 4000, "toMs": 6000, "replace": true, "effect": { "kind": "fire" } }),
                ),
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 0, "toMs": 1000, "effect": { "kind": "glitter" } }),
                ),
                (
                    "place_effects",
                    json!({ "rowIds": ["not-a-row"], "fromMs": 0, "toMs": 1000, "effect": { "kind": "on" } }),
                ),
            ],
        ),
        calls(
            "",
            &[(
                "propose_changes",
                json!({ "summary": "A light show for Jingle." }),
            )],
        ),
        says("Have a look."),
    ]);
    let (mut session, _) = session();
    let (reply, events) = ask(&mut session, &provider, &s.engine, "Make it compelling");
    let reply = reply.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::Activity { label } if label == "Drafting: place effects"))
    );

    let placing = results_in(&provider, 2);
    assert!(placing.iter().all(|(_, err)| !err), "{placing:?}");
    assert!(
        placing[0].0.contains("Placed 12 Twinkle effects on 3 rows"),
        "{}",
        placing[0].0
    );
    // The intro, bar by bar, on every row (fades in).
    assert_eq!(
        placed(&session, &c, 0)[..4],
        [(0, 2000), (2000, 4000), (4000, 6000), (6000, 8000)]
    );
    // Sweep: beat j on row j % 3.
    let sweep: Vec<Vec<(u64, u64)>> = [&a, &b, &c]
        .iter()
        .map(|r| {
            placed(&session, r, 0)
                .into_iter()
                .filter(|e| (8000..10000).contains(&e.0))
                .collect()
        })
        .collect();
    assert_eq!(sweep[0], [(8000, 8500), (9500, 10000)]);
    assert_eq!(sweep[1], [(8500, 9000)]);
    assert_eq!(sweep[2], [(9000, 9500)]);
    // Alternate, two beats each: A takes slots 0 and 2, B slots 1 and 3.
    let turns = |r: &str| -> Vec<(u64, u64)> {
        placed(&session, r, 0)
            .into_iter()
            .filter(|e| (10000..14000).contains(&e.0))
            .collect()
    };
    assert_eq!(turns(&a), [(10000, 11000), (12000, 13000)]);
    assert_eq!(turns(&b), [(11000, 12000), (13000, 14000)]);
    // Build: row i joins at bar i and stays.
    let joined = |r: &str| {
        placed(&session, r, 0)
            .into_iter()
            .filter(|e| (14000..20000).contains(&e.0))
            .count()
    };
    assert_eq!([joined(&a), joined(&b), joined(&c)], [3, 2, 1]);
    assert_eq!(placed(&session, &group, 1), [(0, 32000)]);

    let fixing = results_in(&provider, 3);
    assert!(
        fixing[0].1 && fixing[0].0.contains("already has effects"),
        "{}",
        fixing[0].0
    );
    assert!(!fixing[1].1, "{}", fixing[1].0);
    assert!(
        fixing[2].1 && fixing[2].0.contains("effect kind"),
        "{}",
        fixing[2].0
    );
    assert!(fixing[3].1 && fixing[3].0.contains("row"), "{}", fixing[3].0);
    let doc = session.draft().unwrap().sequence().unwrap();
    let row_a = doc.rows.iter().find(|r| r.id.to_string() == a).unwrap();
    let at = |ms: u64| {
        row_a.layers[0]
            .effects
            .iter()
            .find(|e| e.start_ms == ms)
            .map(|e| e.kind())
    };
    assert_eq!(at(4000), Some(EffectKind::Fire), "replaced");
    assert_eq!(at(2000), Some(EffectKind::Twinkle), "the rest of the intro stays");

    // What a placed effect looks like.
    let doc = session.draft().unwrap().sequence().unwrap();
    let row_c = doc.rows.iter().find(|r| r.id.to_string() == c).unwrap();
    let first = &row_c.layers[0].effects[0];
    assert_eq!(first.kind(), EffectKind::Twinkle);
    assert_eq!(first.palette.colors.len(), 2);
    assert_eq!(first.fade_in_ms, 100);
    assert_eq!(serde_json::to_value(&first.params).unwrap()["density"], 0.5);

    // The proposal: per section, and a timeline to look at.
    let proposal = reply.proposal.unwrap();
    assert!(proposal.changes_sequence);
    let sections: Vec<&str> = proposal.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(sections, ["Intro", "High 1", "Outro"]);
    assert!(proposal.sections[0].added >= 12);
    assert!(proposal.sections[0].kinds.contains(&"Twinkle".to_string()));
    let timeline = proposal.timeline.as_ref().unwrap();
    assert_eq!(timeline.duration_ms, 32_000);
    assert_eq!(timeline.rows.len(), 4);
    assert_eq!(timeline.rows[3].name, "group All");
    assert_eq!(timeline.rows[3].effects[0].color, "#0000ff");
    assert_eq!(timeline.sections.len(), 3);

    // Apply: one sequence undo step, and still no output, packets, or files.
    let before = s.engine.sequence_document().unwrap().clone();
    let files: Vec<_> = std::fs::read_dir(s.dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let mut engine = s.engine;
    apply_proposal(&mut engine, session.proposal().unwrap()).unwrap();
    let after = engine.sequence_document().unwrap();
    assert_eq!(after.effect_count(), 12 + 4 + 4 + 6 + 1);
    assert_eq!(after.timing_tracks.len(), 4);
    engine.undo_sequence().unwrap();
    assert_eq!(
        engine.sequence_document().unwrap(),
        &before,
        "one undo takes it all back"
    );
    assert!(!engine.output_status().running);
    assert!(s.recorded.lock().unwrap().is_empty(), "no packets");
    let now: Vec<_> = std::fs::read_dir(s.dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(now, files, "no files written");
}

#[test]
fn a_pattern_repeats_across_the_song() {
    let s = setup(Some("/music/song.mp3"));
    let [a, b] = [0, 1].map(|i| s.rows[i].clone());
    let provider = ScriptedProvider::new(vec![
        calls(
            "",
            &[
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 0, "toMs": 1000, "effect": { "kind": "chase" } }),
                ),
                (
                    "place_effects",
                    json!({ "rowIds": [b], "fromMs": 1000, "toMs": 2000, "effect": { "kind": "meteors" } }),
                ),
                (
                    "repeat_effects",
                    json!({ "fromMs": 0, "toMs": 2000, "startsMs": [4000, 8000, 31000] }),
                ),
                (
                    "repeat_effects",
                    json!({ "fromMs": 0, "toMs": 2000, "startsMs": [500] }),
                ),
                (
                    "repeat_effects",
                    json!({ "fromMs": 0, "toMs": 2000, "startsMs": [12000], "rowIds": [b] }),
                ),
            ],
        ),
        says("Repeated."),
    ]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Repeat the opening")
        .0
        .unwrap();
    let results = results_in(&provider, 1);
    assert!(!results[2].1, "{}", results[2].0);
    assert!(results[2].0.contains("Copied 5 effects"), "{}", results[2].0);
    // A copy past the end is cut at the end (a meteor that would start after it is left out).
    assert_eq!(
        placed(&session, &a, 0),
        [(0, 1000), (4000, 5000), (8000, 9000), (31000, 32000)]
    );
    assert_eq!(
        placed(&session, &b, 0)[..3],
        [(1000, 2000), (5000, 6000), (9000, 10000)]
    );
    assert!(
        results[3].1 && results[3].0.contains("already has effects"),
        "{}",
        results[3].0
    );
    assert!(!results[4].1);
    assert_eq!(placed(&session, &b, 0).last(), Some(&(13000, 14000)));
    assert!(placed(&session, &a, 0).iter().all(|e| e.0 != 12000), "only row B");
}

#[test]
fn a_start_past_the_end_is_refused_not_a_crash() {
    let s = setup(Some("/music/song.mp3"));
    let a = s.rows[0].clone();
    let provider = ScriptedProvider::new(vec![
        calls(
            "",
            &[
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 0, "toMs": 1000, "effect": { "kind": "on" } }),
                ),
                (
                    "repeat_effects",
                    json!({ "fromMs": 0, "toMs": 1000, "startsMs": [4000, u64::MAX] }),
                ),
            ],
        ),
        says("Fixed."),
    ]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Repeat").0.unwrap();
    let (text, is_error) = results_in(&provider, 1).remove(1);
    assert!(is_error && text.contains("past the end"), "{text}");
    assert_eq!(placed(&session, &a, 0), [(0, 1000)], "nothing copied");
}

/// An OpenAI Responses stream with one function call (then nothing else).
fn openai_call(name: &str, input: &Value) -> String {
    let item = json!({ "id": "fc_1", "type": "function_call", "status": "completed", "call_id": "call_1", "name": name, "arguments": input.to_string() });
    [
        json!({ "type": "response.output_item.added", "output_index": 0, "item": { "id": "fc_1", "type": "function_call", "call_id": "call_1", "name": name, "arguments": "" } }),
        json!({ "type": "response.function_call_arguments.delta", "output_index": 0, "item_id": "fc_1", "delta": input.to_string() }),
        json!({ "type": "response.output_item.done", "output_index": 0, "item": item }),
        json!({ "type": "response.completed", "response": { "status": "completed", "output": [item] } }),
    ]
    .iter()
    .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
    .collect()
}

fn openai_text(text: &str) -> String {
    let item = json!({ "id": "msg_1", "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": text }] });
    [
        json!({ "type": "response.output_text.delta", "output_index": 0, "item_id": "msg_1", "delta": text }),
        json!({ "type": "response.output_item.done", "output_index": 0, "item": item }),
        json!({ "type": "response.completed", "response": { "status": "completed", "output": [item] } }),
    ]
    .iter()
    .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
    .collect()
}

/// An Anthropic stream with one tool call, or with text only.
fn anthropic_reply(text: &str, call: Option<(&str, &Value)>) -> String {
    let mut out = String::from("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n");
    let (block, delta, stop) = match call {
        Some((name, input)) => (
            json!({ "type": "tool_use", "id": "toolu_1", "name": name, "input": {} }),
            json!({ "type": "input_json_delta", "partial_json": input.to_string() }),
            "tool_use",
        ),
        None => (
            json!({ "type": "text", "text": "" }),
            json!({ "type": "text_delta", "text": text }),
            "end_turn",
        ),
    };
    for event in [
        json!({ "type": "content_block_start", "index": 0, "content_block": block }),
        json!({ "type": "content_block_delta", "index": 0, "delta": delta }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({ "type": "message_delta", "delta": { "stop_reason": stop } }),
        json!({ "type": "message_stop" }),
    ] {
        out += &format!("event: {}\ndata: {event}\n\n", event["type"].as_str().unwrap());
    }
    out
}

#[test]
fn misspelled_effect_settings_are_refused_through_either_provider() {
    use pf_ai::http::RetryPolicy;
    use pf_ai::testing::{FakeTransport, Reply};
    let s = setup(Some("/music/song.mp3"));
    let place = json!({
        "rowIds": [s.rows[0]], "fromMs": 0, "toMs": 1000,
        "effect": { "kind": "chase", "settings": { "speed": 2.0, "sped": 4 } },
    });
    let colour = json!({
        "rowIds": [s.rows[0]], "fromMs": 0, "toMs": 1000,
        "effect": { "kind": "chase", "colour": "#ff0000" },
    });
    let add = json!({
        "row": s.rows[1], "layer": 0,
        "effect": { "id": pf_sequence::EffectId::new(), "startMs": 0, "endMs": 1000, "params": { "kind": "twinkle", "twinkles": 3 } },
    });
    for (tool, input, key, hint) in [
        ("place_effects", &place, "sped", "list_effect_kinds"),
        ("place_effects", &colour, "colour", "colors"),
        (
            "sequence_add_effect",
            &add,
            "effect.params.twinkles",
            "list_effect_kinds",
        ),
    ] {
        // OpenAI: the refusal goes back as a function_call_output.
        let fake = Arc::new(FakeTransport::new(vec![
            Reply::ok(openai_call(tool, input)),
            Reply::ok(openai_text("Let me look the settings up.")),
        ]));
        let openai = pf_ai::openai::OpenAi::new(fake.clone()).with_retry(RetryPolicy::immediate());
        let mut session = ChatSession::new();
        let workspace = Workspace::from_engine(&s.engine, UiContext::default());
        session
            .run_turn(
                &openai,
                &fake_key(),
                "gpt-4.1",
                "Chase it",
                workspace,
                &Cancel::new(),
                &mut |_| {},
            )
            .unwrap();
        let input_items = fake.body(1)["input"].clone();
        let output = input_items
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["type"] == "function_call_output")
            .unwrap()["output"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            output.starts_with("Error:") && output.contains(key) && output.contains(hint),
            "{tool}: {output}"
        );
        assert!(
            session.draft().unwrap().sequence().unwrap().effect_count() == 0,
            "nothing placed"
        );

        // Anthropic: an is_error tool_result.
        let fake = Arc::new(FakeTransport::new(vec![
            Reply::ok(anthropic_reply("", Some((tool, input)))),
            Reply::ok(anthropic_reply("Let me look the settings up.", None)),
        ]));
        let anthropic = pf_ai::anthropic::Anthropic::new(fake.clone()).with_retry(RetryPolicy::immediate());
        let mut session = ChatSession::new();
        let workspace = Workspace::from_engine(&s.engine, UiContext::default());
        session
            .run_turn(
                &anthropic,
                &fake_key(),
                "claude-haiku-4-5",
                "Chase it",
                workspace,
                &Cancel::new(),
                &mut |_| {},
            )
            .unwrap();
        let messages = fake.body(1)["messages"].clone();
        let result = &messages.as_array().unwrap().last().unwrap()["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["is_error"], true);
        let text = result["content"].as_str().unwrap();
        assert!(text.contains(key) && text.contains(hint), "{tool}: {text}");
    }
}

#[test]
fn a_sloppy_draft_is_locked_to_the_music_before_the_user_sees_it() {
    let s = setup(Some("/music/song.mp3"));
    let [a, b] = [0, 1].map(|i| s.rows[i].clone());
    let provider = ScriptedProvider::new(vec![
        calls(
            "",
            &[
                // The intro and the loud part, each edge a little off its section's.
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 130, "toMs": 7_880, "effect": { "kind": "twinkle" } }),
                ),
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 8_210, "toMs": 23_700, "effect": { "kind": "colorWash" } }),
                ),
                // A flash near the hit, and a beat-long pulse already on the beat.
                (
                    "place_effects",
                    json!({ "rowIds": [b], "fromMs": 16_190, "toMs": 16_340, "effect": { "kind": "strobe" } }),
                ),
                (
                    "place_effects",
                    json!({ "rowIds": [b], "fromMs": 17_500, "toMs": 18_000, "effect": { "kind": "on" } }),
                ),
            ],
        ),
        calls(
            "",
            &[("propose_changes", json!({ "summary": "Looks per section." }))],
        ),
        says("Have a look."),
    ]);
    let (mut session, runs) = session();
    let (reply, _) = ask(&mut session, &provider, &s.engine, "Make a show");
    let proposal = reply.unwrap().proposal.unwrap();
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "the song is analyzed to lock to it"
    );
    assert_eq!(placed(&session, &a, 0), [(0, 8_000), (8_000, 24_000)]);
    // The flash starts on the hit (its end, between beats, stays); the pulse doesn't move.
    assert_eq!(placed(&session, &b, 0), [(16_250, 16_340), (17_500, 18_000)]);
    assert_eq!(proposal.locked_edges, 5);
    let json = serde_json::to_value(&proposal).unwrap();
    assert_eq!(json["lockedEdges"], 5);

    // Applied, it's still one undo step.
    let mut engine = s.engine;
    apply_proposal(&mut engine, session.proposal().unwrap()).unwrap();
    let row = &engine.sequence_document().unwrap().rows[0];
    assert_eq!(row.layers[0].effects[0].start_ms, 0);
    engine.undo_sequence().unwrap();
    assert_eq!(engine.sequence_document().unwrap().effect_count(), 0);
}

#[test]
fn the_users_own_sections_and_accents_win_over_the_analysis() {
    let mut s = setup(Some("/music/song.mp3"));
    let a = s.rows[0].clone();
    let sections = TimingTrack::new(
        "Sections",
        TimingKind::Sections,
        vec![
            Mark::new(0, 9_250, "Intro"),
            Mark::new(9_250, 20_000, "Chorus 1"),
            Mark::new(20_000, 26_000, "Verse"),
            Mark::new(26_000, 32_000, "Chorus 2"),
        ],
    );
    let accents = TimingTrack::new(
        "Accents",
        TimingKind::Custom,
        vec![
            Mark::new(12_120, 12_400, "Hit"),
            Mark::new(14_000, 16_000, "Break"),
        ],
    );
    s.engine
        .edit_sequence(vec![
            SequenceEdit::AddTimingTrack { track: sections },
            SequenceEdit::AddTimingTrack { track: accents },
        ])
        .unwrap();
    let provider = ScriptedProvider::new(vec![
        calls("", &[("analyze_song", json!({}))]),
        calls(
            "",
            &[(
                "place_effects",
                json!({ "rowIds": [a], "fromMs": 9_400, "toMs": 12_080, "effect": { "kind": "fire" } }),
            )],
        ),
        calls(
            "",
            &[("propose_changes", json!({ "summary": "Fire in the chorus." }))],
        ),
        says("Done."),
    ]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Fire in the chorus")
        .0
        .unwrap();
    let summary: Value = serde_json::from_str(&results_in(&provider, 1)[0].0).unwrap();
    assert_eq!(summary["sectionsFrom"], "user");
    assert_eq!(summary["accentsFrom"], "user");
    let sections: Vec<(&str, &str, u64)> = summary["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["label"].as_str().unwrap(),
                s["group"].as_str().unwrap(),
                s["startMs"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        [
            ("Intro", "A", 0),
            ("Chorus 1", "B", 9_250),
            ("Verse", "C", 20_000),
            ("Chorus 2", "B", 26_000)
        ],
        "a chorus that comes back is the same group"
    );
    assert_eq!(summary["accents"][0], json!({ "atMs": 12_120, "kind": "hit" }));
    assert_eq!(summary["accents"][1]["kind"], "break");
    assert_eq!(summary["accents"][1]["forMs"], 2_000);
    // Locked to the user's section start (off the detected beat grid) and their hit.
    assert_eq!(placed(&session, &a, 0), [(9_250, 12_120)]);
}

#[test]
fn lyrics_are_summarized_and_their_words_can_be_accented() {
    let mut s = setup(Some("/music/song.mp3"));
    let [a, b] = [0, 1].map(|i| s.rows[i].clone());
    // Made-up lyrics.
    let lines = TimingTrack::new(
        "Lyrics",
        TimingKind::Lyrics,
        vec![
            Mark::new(2_100, 5_900, "Paper lanterns glowing"),
            Mark::new(10_120, 13_900, "Lanterns on the snowy rooftops"),
        ],
    );
    let words = [
        (2_100, 2_800, "Paper"),
        (2_800, 3_900, "lanterns"),
        (3_900, 5_900, "glowing"),
        (10_120, 11_000, "Lanterns"),
        (11_000, 11_400, "on"),
        (11_400, 11_800, "the"),
        (11_800, 12_600, "snowy"),
        (12_600, 13_900, "rooftops"),
    ];
    let words = TimingTrack::new(
        "Lyrics (words)",
        TimingKind::Words,
        words.iter().map(|&(s, e, w)| Mark::new(s, e, w)).collect(),
    );
    let vocals = TimingTrack::new(
        "Vocals",
        TimingKind::Custom,
        vec![
            Mark::new(2_100, 5_900, "Vocals"),
            Mark::new(10_120, 13_900, "Vocals"),
        ],
    );
    s.engine
        .edit_sequence(
            [lines, words, vocals]
                .into_iter()
                .map(|track| SequenceEdit::AddTimingTrack { track })
                .collect(),
        )
        .unwrap();
    let provider = ScriptedProvider::new(vec![
        calls("", &[("analyze_song", json!({}))]),
        calls(
            "",
            &[
                // Every "lanterns", on its word.
                (
                    "place_effects",
                    json!({ "rowIds": [a], "fromMs": 0, "toMs": 32_000, "track": "Lyrics", "match": "lanterns", "effect": { "kind": "strobe" } }),
                ),
                // A sloppy start, near where a word is sung.
                (
                    "place_effects",
                    json!({ "rowIds": [b], "fromMs": 10_300, "toMs": 11_000, "effect": { "kind": "on" } }),
                ),
            ],
        ),
        calls(
            "",
            &[("propose_changes", json!({ "summary": "Lanterns flash." }))],
        ),
        says("Done."),
    ]);
    let (mut session, _) = session();
    ask(&mut session, &provider, &s.engine, "Flash on lanterns")
        .0
        .unwrap();
    let summary: Value = serde_json::from_str(&results_in(&provider, 1)[0].0).unwrap();
    assert_eq!(
        summary["lyrics"],
        json!({
            "track": "Lyrics",
            "wordsTrack": "Lyrics (words)",
            "lines": [[2_100, "Paper lanterns glowing"], [10_120, "Lanterns on the snowy rooftops"]],
            "vocalsMs": [[2_100, 5_900], [10_120, 13_900]],
        })
    );
    assert_eq!(placed(&session, &a, 0), [(2_800, 3_900), (10_120, 11_000)]);
    // Locked onto the sung word, not the beat 200 ms away.
    assert_eq!(placed(&session, &b, 0), [(10_120, 11_000)]);
}
