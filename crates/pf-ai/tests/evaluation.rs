//! The evaluation harness, held to its word without a model: a careful scripted answer to each
//! scenario passes that scenario's checks and an idle one fails them; what goes to the model is
//! counted; the command line makes no provider and reads no key unless it is told to send; and a
//! run that sends keeps to a hard spending ceiling, from the usage the provider reports.

use pf_ai::agent::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT};
use pf_ai::eval::cli::{self, KEY_VARIABLE, stand_in_outcomes};
use pf_ai::eval::{self, Billed, Budget, Metered, Prices, Stop, Traffic, scenarios, stand_in, worst_tokens};
use pf_ai::provider::{Message, TurnRequest, Usage};
use pf_ai::stage::Stage;
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{ApiKey, Cancel, LlmProvider, ProviderId, Toolbox, anthropic, openai};
use serde_json::json;
use std::sync::Arc;

fn failures(outcome: &eval::Outcome) -> Vec<String> {
    outcome
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| format!("{}: {} ({})", outcome.scenario, c.what, c.saw))
        .collect()
}

#[test]
fn a_careful_answer_passes_every_check() {
    let all = scenarios();
    let outcomes = stand_in_outcomes(&all);
    assert_eq!(outcomes.len(), all.len());
    let failed: Vec<String> = outcomes.iter().flat_map(failures).collect();
    let refused: Vec<String> = outcomes
        .iter()
        .flat_map(|o| {
            o.measures
                .tool_errors
                .iter()
                .map(|e| format!("{}: {e}", o.scenario))
        })
        .collect();
    assert!(
        failed.is_empty() && refused.is_empty(),
        "failed:\n{}\nrefused:\n{}",
        failed.join("\n"),
        refused.join("\n")
    );
    for outcome in &outcomes {
        assert!(
            outcome.error.is_none() && outcome.checks.len() >= 4,
            "{outcome:?}"
        );
        assert!(!outcome.measures.stopped_at_step_limit, "{}", outcome.scenario);
    }

    // The whole-song answer, in the plain measures the report shows.
    let whole = &outcomes[0].measures;
    assert!(
        whole.props_used >= 9 && whole.effect_kinds.len() >= 5,
        "{whole:?}"
    );
    assert!(
        whole.chorus_light > whole.verse_light && whole.dark_share > 0.0,
        "{whole:?}"
    );
    assert!(whole.guides.iter().any(|g| g == "method"), "{:?}", whole.guides);
    let review = whole
        .review
        .as_ref()
        .expect("a sequence with effects is reviewed");
    assert!(review.flashes_safe && review.score >= 70, "{review:?}");

    // The report says what was run, each scenario's checks, and what it would cost.
    let report = eval::report(
        &outcomes,
        "the scripted stand-in (not a model)",
        &Prices::default(),
    );
    for words in [
        "# Assistant evaluation: the scripted stand-in (not a model)",
        "checks passed over 9 scenarios",
        "| whole-song |",
        "## A whole song from nothing (whole-song)",
        "> Make a light show for this song.",
        "- pass It proposes its changes for the user to apply",
        "Guides read: method,",
        "The prices are assumptions",
    ] {
        assert!(report.contains(words), "{words}:\n{report}");
    }
    assert!(!report.contains("FAIL"), "{report}");
}

#[test]
fn an_idle_answer_fails_every_scenario() {
    for scenario in scenarios() {
        let stage = Stage::new();
        let provider = ScriptedProvider::new(stand_in::idle());
        let outcome = eval::run(&scenario, &stage, &provider, &fake_key(), "idle", &Cancel::new());
        let failed = failures(&outcome).len();
        assert!(
            failed * 2 >= outcome.checks.len(),
            "{}: only {failed} of {} checks failed",
            scenario.key,
            outcome.checks.len()
        );
        assert!(
            !outcome.measures.proposed && outcome.measures.requests == 1,
            "{}",
            scenario.key
        );
    }
}

#[test]
fn a_refused_tool_call_and_a_failing_provider_are_reported_not_fatal() {
    let scenario = scenarios().into_iter().find(|s| s.key == "dancers").unwrap();
    let stage = Stage::new();
    // A dancer that doesn't exist, then the provider failing: the run still ends with a report.
    let provider = ScriptedProvider::new(vec![
        calls(
            "",
            &[(
                "place_effects",
                json!({ "rowIds": ["not-a-row"], "fromMs": 24_000, "toMs": 40_000, "effect": { "kind": "dancer" } }),
            )],
        ),
        Err(pf_ai::AiError::Refused),
    ]);
    let outcome = eval::run(
        &scenario,
        &stage,
        &provider,
        &fake_key(),
        "broken",
        &Cancel::new(),
    );
    assert!(outcome.error.is_some(), "{outcome:?}");
    assert_eq!(
        outcome.measures.tool_errors.len(),
        1,
        "{:?}",
        outcome.measures.tool_errors
    );
    assert!(outcome.measures.tool_errors[0].starts_with("place_effects: "));
    assert_eq!(outcome.measures.tool_calls["place_effects"], 1);
    let report = eval::report(&[outcome], "broken", &Prices::default());
    assert!(
        report.contains("Stopped early: ") && report.contains("Refused: place_effects: "),
        "{report}"
    );
}

#[test]
fn what_goes_to_the_model_is_counted() {
    let scenario = scenarios().into_iter().find(|s| s.key == "question").unwrap();
    let stage = Stage::new();
    let provider = ScriptedProvider::new(stand_in::script(&scenario, &stage));
    let outcome = eval::run(
        &scenario,
        &stage,
        &provider,
        &fake_key(),
        "stand-in",
        &Cancel::new(),
    );
    let traffic = outcome.measures.traffic;
    assert_eq!(traffic.requests, 2);
    // The report sets the stand-in's first request and mean request side by side, estimated.
    let report = eval::report(std::slice::from_ref(&outcome), "stand-in", &Prices::default());
    let row = format!(
        "| question | 2 | 1 | ~{} | ~{} | ~{} |",
        eval::tokens_of(traffic.first_request_bytes),
        traffic.mean_request_tokens(),
        eval::tokens_of(traffic.largest_request_bytes)
    );
    assert!(report.contains(&row), "{row}\n{report}");
    assert!(traffic.first_request_bytes > 0 && traffic.first_request_bytes < traffic.largest_request_bytes);
    // Every request carries the instructions and the core tools; the second repeats the first
    // and adds the two guides it asked for.
    let core: usize = Toolbox::default()
        .core_specs()
        .iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();
    let fixed = pf_ai::agent::SYSTEM_PROMPT.len() + core;
    assert!(traffic.first_request_bytes > fixed, "{traffic:?}");
    assert!(traffic.largest_request_bytes > fixed + 10_000, "{traffic:?}");
    assert!(
        traffic.repeated_bytes == traffic.first_request_bytes
            && traffic.repeated_bytes < traffic.sent_bytes / 2 + 1_000
    );
    assert!(
        traffic.reply_bytes > 300 && traffic.reply_bytes < 1_000,
        "{traffic:?}"
    );

    // The arithmetic of a price, on round numbers: a million tokens each way.
    let round = Traffic {
        requests: 2,
        sent_bytes: 3_500_000,
        repeated_bytes: 1_750_000,
        reply_bytes: 3_500_000,
        largest_request_bytes: 0,
        first_request_bytes: 0,
    };
    let prices = Prices {
        input: 2.0,
        output: 10.0,
        cached: 0.5,
        cache_write: 1.25,
    };
    assert_eq!(round.tokens(), (1_000_000, 1_000_000));
    let (full, cached) = round.cost(&prices);
    assert!(
        (full - 12.0).abs() < 1e-9 && (cached - 11.5).abs() < 1e-9,
        "{full} {cached}"
    );

    // A meter passes everything through and remembers the sizes.
    let scripted = ScriptedProvider::new(vec![says("Hello.")]);
    let metered = Metered::new(&scripted);
    assert_eq!(metered.id(), scripted.id());
    assert_eq!(metered.traffic(), Traffic::default());
}

/// Runs the command line with `connect` standing in for the real provider.
fn command(
    args: &[&str],
    scripted: Vec<Result<pf_ai::provider::AssistantTurn, pf_ai::AiError>>,
) -> (i32, String, usize) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut connected = 0;
    let mut turns = Some(scripted);
    let mut connect = |_: ProviderId| -> Result<(Arc<dyn LlmProvider>, ApiKey), String> {
        connected += 1;
        Ok((
            Arc::new(ScriptedProvider::new(turns.take().unwrap_or_default())),
            fake_key(),
        ))
    };
    let mut out = Vec::new();
    let status = cli::main(&args, &mut connect, &mut out);
    (status, String::from_utf8(out).unwrap(), connected)
}

#[test]
fn the_command_line_sends_nothing_unless_told_to() {
    // With no provider: the stand-in, and no provider is ever made.
    let (status, said, connected) = command(&["--scenario", "question"], Vec::new());
    assert_eq!((status, connected), (0, 0), "{said}");
    assert!(
        said.starts_with("Stand-in run: every answer is scripted. No model is called and nothing is sent.")
    );
    assert!(said.contains("4 of 4 checks passed over 1 scenarios"), "{said}");

    // With a provider but without --send: what would go, what it might cost, and a stop.
    let (status, said, connected) = command(
        &[
            "--scenario",
            "question",
            "--provider",
            "anthropic",
            "--model",
            "some-model",
            "--price-in",
            "1",
            "--price-out",
            "2",
        ],
        Vec::new(),
    );
    assert_eq!((status, connected), (0, 0), "{said}");
    for words in [
        "What a run sends the model's provider:",
        "props: Roofline, Window Left, Window Right, Matrix, Mega Tree, Tree Star, Arch 1",
        "Nothing of yours is read or sent",
        KEY_VARIABLE,
        "Roughly what it costs: the scripted answers to these 1 scenarios take 2 requests",
        "At $1/$2 a million tokens in/out (assumed: check Anthropic's price list",
        "Nothing was sent. To run it against some-model, put your Anthropic key in PIXELFLOW_EVAL_KEY and add --send with --max-usd and the prices (--price-in, --price-out, --price-cached, --price-cache-write).",
    ] {
        assert!(said.contains(words), "{words}:\n{said}");
    }
    assert!(!said.contains("# Assistant evaluation"), "{said}");

    // With --send: the provider is made once, and the run is reported under the model's name.
    let (status, said, connected) = command(
        &[
            "--scenario",
            "question",
            "--provider",
            "openai",
            "--model",
            "some-model",
            "--send",
            "--runs",
            "2",
            "--max-usd",
            "10",
            "--price-in",
            "1",
            "--price-out",
            "2",
            "--price-cached",
            "0.1",
        ],
        vec![says("They are different.")],
    );
    assert_eq!((status, connected), (0, 1), "{said}");
    assert!(said.contains("question #1: 2 of 4 checks, 1 requests"), "{said}");
    assert!(
        said.contains("# Assistant evaluation: some-model (OpenAI)"),
        "{said}"
    );
}

#[test]
fn the_command_line_says_what_it_takes() {
    let (status, said, _) = command(&["--list"], Vec::new());
    assert_eq!(status, 0);
    for scenario in scenarios() {
        assert!(
            said.contains(&format!("{}: {}", scenario.key, scenario.title)),
            "{said}"
        );
        assert!(said.contains(scenario.request), "{said}");
    }
    for (args, words) in [
        (
            vec!["--scenario", "karaoke"],
            "There's no scenario \"karaoke\"; choose from whole-song, dancers,",
        ),
        (
            vec!["--provider", "acme"],
            "There's no provider \"acme\"; use anthropic or openai.",
        ),
        (vec!["--provider", "openai"], "--provider needs --model"),
        (vec!["--runs", "0"], "--runs takes 1 to 20, not \"0\"."),
        (
            vec!["--price-in", "cheap"],
            "--price-in takes a number, not \"cheap\".",
        ),
        (vec!["--model"], "--model needs a value."),
        (vec!["--everything"], "Unknown argument \"--everything\"."),
    ] {
        let (status, said, connected) = command(&args, Vec::new());
        assert_eq!((status, connected), (2, 0), "{args:?}: {said}");
        assert!(
            said.contains(words) && said.contains("assistant_eval --list"),
            "{args:?}: {said}"
        );
    }
    let (status, said, _) = command(&["--help"], Vec::new());
    assert!(
        status == 0
            && said.contains("--send                   really send")
            && said.contains("--max-usd <usd>          the most the whole run may spend; --send needs it"),
        "{said}"
    );
}

#[test]
fn every_scenario_starts_from_a_sound_made_up_sequence() {
    let stage = Stage::new();
    let mut keys = Vec::new();
    for scenario in scenarios() {
        assert!(!keys.contains(&scenario.key), "{} twice", scenario.key);
        keys.push(scenario.key);
        let outcome = {
            let provider = ScriptedProvider::new(vec![says("Nothing to do.")]);
            eval::run(&scenario, &stage, &provider, &fake_key(), "idle", &Cancel::new())
        };
        // The request went as typed, on the Sequence screen of the stage's show and song.
        assert_eq!(outcome.request, scenario.request);
        assert!(!scenario.opens.is_empty() && !scenario.title.is_empty());
        assert_eq!(outcome.measures.props, stage.show.props.len());
    }
}

/// A PNG's width and height, from its header.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let number = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (number(16), number(20))
}

#[test]
fn a_run_keeps_each_draft_with_the_stage_to_open_in_the_app() {
    let folder = tempfile::tempdir().unwrap();
    let out = folder.path().join("run");
    let (status, said, connected) = command(
        &[
            "--scenario",
            "dancers,question",
            "--out",
            out.to_str().unwrap(),
            "--no-render",
        ],
        Vec::new(),
    );
    assert_eq!((status, connected), (0, 0), "{said}");
    let show_folder = out.join(eval::drafts::SHOW_FOLDER);
    let show = show_folder.join(eval::drafts::SHOW_FILE);
    assert!(
        said.contains(&format!("Written to {}:", out.display()))
            && said.contains("show/Paper Lanterns stage.pixelflow.json: the made-up stage")
            && said.contains("show/dancers.pfseq.json: the dancers draft"),
        "{said}"
    );
    // The show and each draft open as the app opens them: the show with its picture beside it,
    // the draft playing the song's sound, kept beside it too.
    let opened = pf_engine::load_show(&show).unwrap();
    assert_eq!(opened.props.len(), Stage::new().show.props.len());
    assert!(show_folder.join(pf_ai::stage::PICTURE).is_file());
    let sound = std::fs::read(show_folder.join(eval::drafts::SONG_SOUND)).unwrap();
    assert_eq!(&sound[..4], b"RIFF");
    let dancers = pf_engine::load_sequence(&show_folder.join("dancers.pfseq.json")).unwrap();
    assert_eq!(dancers.audio.as_deref(), Some(eval::drafts::SONG_SOUND));
    assert!(
        eval::effects(&dancers).any(|(_, _, e)| e.kind() == pf_sequence::EffectKind::Dancer),
        "the skeletons are in the saved draft"
    );
    // Every scenario played on the stage that was saved: each row lights a prop or group of it.
    let question = pf_engine::load_sequence(&show_folder.join("question.pfseq.json")).unwrap();
    for doc in [&dancers, &question] {
        for row in &doc.rows {
            let found = match row.target {
                pf_sequence::Target::Group(id) => opened.groups.iter().any(|g| g.id == id),
                pf_sequence::Target::Prop(id) | pf_sequence::Target::Region { prop: id, .. } => {
                    opened.props.iter().any(|p| p.id == id)
                }
            };
            assert!(found, "{:?} is on the saved stage", row.target);
        }
    }
    // A question changes nothing: what was open is kept as it was.
    let effects_of = |doc: &pf_sequence::Sequence| -> Vec<(pf_sequence::EffectId, u64, u64)> {
        eval::effects(doc)
            .map(|(_, _, e)| (e.id, e.start_ms, e.end_ms))
            .collect()
    };
    assert_eq!(
        effects_of(&question),
        effects_of(&eval::scenarios::christmas(&Stage::new()))
    );
    // Without rendering, nothing is drawn.
    assert!(!out.join(eval::drafts::VIDEO_FOLDER).exists());
}

#[test]
fn a_kept_draft_is_drawn_as_a_video_and_a_sheet_of_its_sections() {
    let folder = tempfile::tempdir().unwrap();
    let stage = Stage::new();
    let outcomes = cli::stand_in_outcomes_on(&[scenario("dancers")], &stage);
    let small = eval::drafts::Rendering {
        width: 160,
        height: 90,
        fps: 10,
        end_ms: Some(1_000),
        frame_width: 192,
        frame_height: 108,
    };
    let said = eval::drafts::keep(folder.path(), &stage, &outcomes, "the stand-in", Some(&small)).unwrap();
    let video = folder.path().join("videos/dancers.mp4");
    let sheet = folder.path().join("sheets/dancers.png");
    assert!(
        said.iter()
            .any(|line| line.contains("videos/dancers.mp4, sheets/dancers.png")),
        "{said:?}"
    );
    let mp4 = std::fs::read(&video).unwrap();
    assert_eq!(&mp4[4..8], b"ftyp");
    // A frame for each of the song's eight sections, two across, captioned.
    let (width, height) = png_size(&std::fs::read(&sheet).unwrap());
    let sections = pf_ai::stage::song().sections.len() as u32;
    assert_eq!(sections, 8);
    assert_eq!(width, 14 + 2 * (192 + 14));
    assert!(height > sections / 2 * 108, "{height}");
    // The engine's working folder is gone.
    let left: Vec<String> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!left.iter().any(|name| name.starts_with('.')), "{left:?}");
}

// Spending: a run that sends keeps to a hard ceiling, from what the provider reports.

fn prices() -> Prices {
    Prices {
        input: 1.0,
        output: 2.0,
        cached: 0.1,
        cache_write: 1.25,
    }
}

fn used(input: u64, cache_read: u64, cache_write: u64, output: u64) -> Option<Usage> {
    Some(Usage {
        input,
        cache_read,
        cache_write,
        output,
    })
}

fn scenario(key: &str) -> eval::Scenario {
    scenarios().into_iter().find(|s| s.key == key).unwrap()
}

#[test]
fn a_ramp_on_the_props_a_model_named_grows_over_a_look_that_lit_everything() {
    // The call a model made in a real run: a ramp on four of the props over a verse whose look
    // lights the whole house. The second bar came out barely brighter than the first, every
    // prop lit in both (0.36 then 0.38). The ramp now takes its props to dark and brings them
    // back bar by bar, brighter each time.
    let ramp = json!({ "cues": [{ "at": 20_000, "until": 24_000, "cue": "ramp", "hit": false,
        "intensity": 0.75, "colors": ["#ff0000", "#00a030"],
        "targets": ["Roofline", "Mega Tree", "Arches", "Windows"] }] });
    let provider = ScriptedProvider::new(vec![
        calls("", &[("stage_cue", ramp)]),
        calls(
            "",
            &[("propose_changes", json!({ "summary": "A two-bar build." }))],
        ),
        says("Ready."),
    ]);
    let outcome = eval::run(
        &scenario("build"),
        &Stage::new(),
        &provider,
        &fake_key(),
        "scripted",
        &Cancel::new(),
    );
    let grows = outcome
        .checks
        .iter()
        .find(|c| c.what.starts_with("It grows"))
        .unwrap();
    assert!(grows.passed, "{grows:?}");
    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
}

#[test]
fn every_chorus_has_sung_words_to_put_up_the_last_one_too() {
    // A model said the last chorus had no lyric timings and left it without words: the
    // made-up song now sings it, and the check wants words in it.
    let song = pf_ai::stage::song();
    let last = song.sections.iter().rfind(|s| s.label == "Chorus").unwrap();
    let doc = Stage::new().sequence();
    let lines = doc.timing_tracks.iter().find(|t| t.name == "Lyrics").unwrap();
    assert!(
        lines
            .marks
            .iter()
            .any(|m| m.start_ms >= last.start_ms && m.start_ms < last.end_ms)
    );
    let check = &scenario("sing-along");
    let provider = ScriptedProvider::new(stand_in::idle());
    let outcome = eval::run(
        check,
        &Stage::new(),
        &provider,
        &fake_key(),
        "idle",
        &Cancel::new(),
    );
    assert!(
        outcome
            .checks
            .iter()
            .any(|c| c.what.contains("the last chorus's too (5 of 6 or more)")),
        "{:?}",
        outcome.checks
    );
}

/// The request the chat loop sent as `messages` (with the core tools: the scripts add no
/// group), rebuilt to work out its worst case.
fn worst_of(provider: ProviderId, model: &str, messages: &[Message]) -> (u64, u64) {
    let tools = Toolbox::default().core_specs();
    let request = TurnRequest {
        model,
        system: SYSTEM_PROMPT,
        tools: &tools,
        messages,
        max_tokens: MAX_OUTPUT_TOKENS,
    };
    worst_tokens(provider, &request)
}

/// Runs the command line with `provider` standing in for the real one.
fn command_on(args: &[&str], provider: Arc<ScriptedProvider>) -> (i32, String, usize) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut connected = 0;
    let mut connect = |_: ProviderId| -> Result<(Arc<dyn LlmProvider>, ApiKey), String> {
        connected += 1;
        Ok((provider.clone(), fake_key()))
    };
    let mut out = Vec::new();
    let status = cli::main(&args, &mut connect, &mut out);
    (status, String::from_utf8(out).unwrap(), connected)
}

const PRICED: [&str; 8] = [
    "--max-usd",
    "10",
    "--price-in",
    "1",
    "--price-out",
    "2",
    "--price-cached",
    "0.1",
];

#[test]
fn a_run_that_sends_needs_a_ceiling_and_explicit_prices_or_it_doesnt_start() {
    let base = [
        "--scenario",
        "question",
        "--provider",
        "openai",
        "--model",
        "some-model",
        "--send",
    ];
    for (extra, words) in [
        (vec![], "--send needs --max-usd <dollars>"),
        (
            vec!["--max-usd", "10"],
            "missing --price-in, --price-out, --price-cached. Nothing was sent.",
        ),
        (
            vec!["--max-usd", "10", "--price-in", "1", "--price-out", "2"],
            "missing --price-cached. Nothing was sent.",
        ),
        (
            vec!["--max-usd", "0"],
            "--max-usd takes an amount of dollars above 0, not \"0\".",
        ),
        (
            vec!["--max-requests", "0"],
            "--max-requests takes a whole number from 1, not \"0\".",
        ),
    ] {
        let args: Vec<&str> = base.iter().copied().chain(extra.iter().copied()).collect();
        let provider = Arc::new(ScriptedProvider::new(vec![says("Hi.")]));
        let (status, said, connected) = command_on(&args, provider.clone());
        // Refused before any provider is made or any key is read.
        assert_eq!((status, connected), (2, 0), "{args:?}: {said}");
        assert!(said.contains(words), "{args:?}: {said}");
        assert!(provider.requests().is_empty());
    }

    // Anthropic charges for writing its cache, so that price is needed too.
    let mut args = vec![
        "--scenario",
        "question",
        "--provider",
        "anthropic",
        "--model",
        "some-model",
        "--send",
    ];
    args.extend(PRICED);
    let (status, said, connected) = command_on(&args, Arc::new(ScriptedProvider::new(Vec::new())));
    assert_eq!((status, connected), (2, 0), "{said}");
    assert!(said.contains("missing --price-cache-write."), "{said}");

    // Ceiling and prices without --send: still nothing made, no key read.
    let mut args = vec![
        "--scenario",
        "question",
        "--provider",
        "openai",
        "--model",
        "some-model",
    ];
    args.extend(PRICED);
    let provider = Arc::new(ScriptedProvider::new(vec![says("Hi.")]));
    let (status, said, connected) = command_on(&args, provider.clone());
    assert_eq!((status, connected), (0, 0), "{said}");
    assert!(provider.requests().is_empty());
    assert!(!said.contains("Prices for this run"), "{said}");

    // With them all, it starts, and says first what it will charge itself.
    args.push("--send");
    let provider =
        Arc::new(ScriptedProvider::new(vec![says("They differ.")]).reporting(vec![used(1_000, 0, 0, 20)]));
    let (status, said, connected) = command_on(&args, provider.clone());
    assert_eq!((status, connected), (0, 1), "{said}");
    let terms = said.find("Prices for this run").expect("the prices are printed");
    assert!(
        terms < said.find("question: ").expect("the scenario ran"),
        "{said}"
    );
    for words in [
        "Prices for this run, in dollars for a million tokens: sent $1, received $2, read from the cache $0.1000 (a share of 0.1), written to the cache not charged.",
        "Spending ceiling: $10.00 for the whole run.",
        "output limit of 32000 tokens",
        "question: 2 of 4 checks, 1 requests. Spent so far: $0.0010 of $10.00.",
        "Final total: $0.0010 of the $10.00 ceiling",
    ] {
        assert!(said.contains(words), "{words}:\n{said}");
    }
}

#[test]
fn a_request_whose_worst_case_would_pass_the_ceiling_is_never_sent() {
    let whole = scenario("whole-song");
    let stage = Stage::new();
    let turns = stand_in::script(&whole, &stage).len();
    // Every reply reports a heavy request: 40,000 tokens in, 2,000 out ($0.044 at these prices).
    let heavy = used(40_000, 0, 0, 2_000).unwrap();
    let mut stopped_at = Vec::new();
    for ceiling in [0.05, 0.15, 0.3, 0.45, 100.0] {
        let provider =
            ScriptedProvider::new(stand_in::script(&whole, &stage)).reporting(vec![Some(heavy); turns]);
        let budget = Budget::new(ceiling, prices(), None);
        let outcome = eval::run_within(
            &whole,
            &stage,
            &provider,
            &fake_key(),
            "some-model",
            &Cancel::new(),
            Some(&budget),
        );
        let spending = budget.spending();
        let sent = provider.requests();
        assert_eq!(sent.len(), spending.requests, "{ceiling}");
        assert_eq!(outcome.measures.requests, sent.len());
        // The true total is what the replies reported, and never passes the ceiling.
        let mut reported = Usage::default();
        for _ in 0..sent.len() {
            reported.add(&heavy);
        }
        assert!((spending.spent - prices().cost(&reported)).abs() < 1e-12);
        assert!(spending.spent <= ceiling, "{ceiling}: {spending:?}");
        // Every request sent fit, at its worst case, in what was left.
        let mut spent = 0.0;
        for messages in &sent {
            let (s, r) = worst_of(ProviderId::Anthropic, "some-model", messages);
            assert!(spent + prices().worst_case(s, r) <= ceiling, "{ceiling}");
            spent += prices().cost(&heavy);
        }
        match outcome.stopped {
            Some(Stop::Budget) => {
                // The one not sent wouldn't have: it was refused, cleanly.
                let worst = spending.refused_worst_case.unwrap();
                assert!(spending.spent + worst > ceiling, "{ceiling}");
                assert!(sent.len() < turns && outcome.error.is_none(), "{outcome:?}");
                assert_eq!(budget.stopped(), Some(Stop::Budget));
                stopped_at.push(sent.len());
            }
            other => {
                assert_eq!(other, None);
                assert_eq!(sent.len(), turns, "{ceiling}");
            }
        }
    }
    // The smallest ceiling can't bear even one request's worst case (32,000 tokens out alone
    // cost $0.064 here); the larger ones stop later; the largest doesn't stop.
    assert_eq!(stopped_at.first(), Some(&0), "{stopped_at:?}");
    assert!(
        stopped_at.len() >= 3 && stopped_at.windows(2).all(|w| w[0] < w[1]),
        "{stopped_at:?}"
    );
}

#[test]
fn a_reply_without_usage_is_counted_at_its_worst_case() {
    let question = scenario("question");
    let stage = Stage::new();
    let light = used(1_000, 3_000, 500, 100);
    let provider = ScriptedProvider::new(stand_in::script(&question, &stage)).reporting(vec![light, None]);
    let budget = Budget::new(100.0, prices(), None);
    let outcome = eval::run_within(
        &question,
        &stage,
        &provider,
        &fake_key(),
        "some-model",
        &Cancel::new(),
        Some(&budget),
    );
    let sent = provider.requests();
    assert_eq!(sent.len(), 2);
    // The harness's requests carry the chat loop's own output limit.
    assert_eq!(provider.max_tokens_seen(), [MAX_OUTPUT_TOKENS; 2]);
    let (worst_sent, worst_received) = worst_of(ProviderId::Anthropic, "some-model", &sent[1]);
    assert_eq!(worst_received, u64::from(MAX_OUTPUT_TOKENS));
    let billed = outcome.measures.billed;
    assert_eq!(
        billed,
        Billed {
            reported: 1,
            tokens: light.unwrap(),
            unreported: 1,
            unreported_sent: worst_sent,
            unreported_received: worst_received,
        }
    );
    let expected = prices().cost(&light.unwrap()) + prices().worst_case(worst_sent, worst_received);
    assert!((budget.spending().spent - expected).abs() < 1e-12);
    // Cache writes cost more than plain input here, so the worst case charges every token sent
    // at the cache-write price.
    assert!(
        (prices().worst_case(1_000_000, 0) - 1.25).abs() < 1e-12
            && (prices().worst_case(0, 1_000_000) - 2.0).abs() < 1e-12
    );

    // A reply that fails says nothing about usage either: it too is counted at its worst case.
    let provider = ScriptedProvider::new(vec![Err(pf_ai::AiError::Overloaded(ProviderId::Openai))]);
    let budget = Budget::new(100.0, prices(), None);
    let outcome = eval::run_within(
        &question,
        &stage,
        &provider,
        &fake_key(),
        "some-model",
        &Cancel::new(),
        Some(&budget),
    );
    assert!(outcome.error.is_some() && outcome.stopped.is_none());
    assert_eq!(budget.spending().billed.unreported, 1);
    assert!(budget.spending().spent >= prices().worst_case(0, u64::from(MAX_OUTPUT_TOKENS)));
}

#[test]
fn the_worst_case_is_everything_sent_and_the_output_limit() {
    let messages = vec![Message::User("Make a light show for this song.".into())];
    let tools = Toolbox::default().specs();
    let request = TurnRequest {
        model: "gpt-5.1",
        system: SYSTEM_PROMPT,
        tools: &tools,
        messages: &messages,
        max_tokens: MAX_OUTPUT_TOKENS,
    };
    let body = openai::request_body(&request, true).to_string().len() as u64;
    assert_eq!(
        worst_tokens(ProviderId::Openai, &request),
        (body + eval::FRAMING_TOKENS, u64::from(MAX_OUTPUT_TOKENS))
    );
    // Every byte of text a model reads is in that.
    assert!(body > eval::request_bytes(&request) as u64);
    // A model whose declined requests Anthropic may answer again with another counts twice.
    let anthropic_request = TurnRequest {
        model: "claude-opus-5-5",
        ..request
    };
    let body = anthropic::request_body(&anthropic_request).to_string().len() as u64;
    assert!(anthropic::takes_fallbacks("claude-opus-5-5"));
    assert_eq!(
        worst_tokens(ProviderId::Anthropic, &anthropic_request),
        (
            2 * (body + eval::FRAMING_TOKENS),
            2 * u64::from(MAX_OUTPUT_TOKENS)
        )
    );
}

#[test]
fn the_report_and_results_show_the_tokens_billed_and_what_they_cost() {
    let folder = tempfile::tempdir().unwrap();
    let out = folder.path().join("run");
    let mut args = vec![
        "--scenario",
        "question",
        "--provider",
        "anthropic",
        "--model",
        "some-model",
        "--send",
        "--price-cache-write",
        "1.25",
        "--out",
        out.to_str().unwrap(),
        "--no-render",
    ];
    args.extend(PRICED);
    let stage = Stage::new();
    let provider = Arc::new(
        ScriptedProvider::new(stand_in::script(&scenario("question"), &stage))
            .reporting(vec![used(1_000, 500, 200, 50), used(1_000, 500, 200, 50)]),
    );
    let (status, said, connected) = command_on(&args, provider);
    assert_eq!((status, connected), (0, 1), "{said}");
    // (2,000 + 1,000 × 0.1 + 400 × 1.25) × $1 + 100 × $2, a million tokens at a time.
    for words in [
        "Billed, as the provider reported it: 2 of 2 requests reported their tokens: 3400 sent (2000 at the full price, 1000 read from the cache, 400 written to it) and 100 received, $0.0028 at the prices. Total: $0.0028.",
        "| question | 4/4 |",
        "| $0.0028 |",
        "## Spending",
        "2 requests sent. Ceiling $10.00; spent $0.0028.",
        "Final total: $0.0028 of the $10.00 ceiling, from what Anthropic reported (2 of 2 requests",
        "written to the cache $1.2500 (a share of 1.25)",
        "Estimated from sizes:",
        // The round trips a person waits through, and what each request carried: the first
        // as billed (1,000 + 500 + 200), and the mean of the two (3,400 / 2).
        "| Scenario | Requests | Tool calls | First request | Mean request | Largest request |",
        "| question | 2 | 1 | 1700 | 1700 | ~",
    ] {
        assert!(said.contains(words), "{words}:\n{said}");
    }
    let report = std::fs::read_to_string(out.join("report.md")).unwrap();
    assert!(report.contains("## Spending") && report.contains("Total: $0.0028."));
    let results: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("results.json")).unwrap()).unwrap();
    assert_eq!(results["maxUsd"], 10.0);
    assert_eq!(results["prices"]["cacheWrite"], 1.25);
    assert_eq!(results["spending"]["requests"], 2);
    assert_eq!(
        results["spending"]["billed"]["tokens"],
        json!({ "input": 2000, "cacheRead": 1000, "cacheWrite": 400, "output": 100 })
    );
    assert!((results["spending"]["spent"].as_f64().unwrap() - 0.0028).abs() < 1e-12);
    assert_eq!(results["outcomes"][0]["measures"]["billed"]["reported"], 2);
    assert_eq!(results["outcomes"][0]["stopped"], serde_json::Value::Null);
    // The sizes are still there beside them.
    assert!(
        results["outcomes"][0]["measures"]["traffic"]["sentBytes"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn once_the_budget_or_the_request_limit_stops_a_scenario_no_other_starts() {
    let base = [
        "--scenario",
        "dancers,question",
        "--provider",
        "openai",
        "--model",
        "some-model",
        "--send",
        "--price-in",
        "1",
        "--price-out",
        "2",
        "--price-cached",
        "0.1",
    ];
    // A ceiling no request's worst case fits under: nothing is sent at all.
    let mut args = base.to_vec();
    args.extend(["--max-usd", "0.05"]);
    let provider = Arc::new(ScriptedProvider::new(vec![says("Done.")]));
    let (status, said, _) = command_on(&args, provider.clone());
    assert_eq!(status, 0, "{said}");
    assert!(provider.requests().is_empty(), "{said}");
    for words in [
        "0 requests (stopped: budget). Spent so far: $0.0000 of $0.05.",
        "| dancers (stopped: budget) |",
        "Stopped: budget. The next request's worst case would have passed the spending ceiling",
        "Stopped: budget (the next request could have cost up to $",
        "Not started: question.",
        "0 requests sent. Ceiling $0.05; spent $0.0000.",
    ] {
        assert!(said.contains(words), "{words}:\n{said}");
    }
    assert!(!said.contains("question: "), "{said}");

    // At most one request: the first scenario stops after it, the second never starts.
    let mut args = base.to_vec();
    args.extend(["--max-usd", "10", "--max-requests", "1"]);
    let stage = Stage::new();
    let provider = Arc::new(
        ScriptedProvider::new(stand_in::script(&scenario("dancers"), &stage))
            .reporting(vec![used(500, 0, 0, 10)]),
    );
    let (status, said, _) = command_on(&args, provider.clone());
    assert_eq!(status, 0, "{said}");
    assert_eq!(provider.requests().len(), 1, "{said}");
    for words in [
        "(stopped: request limit)",
        "Stopped: request limit.",
        "Not started: question.",
        "At most 1 requests.",
    ] {
        assert!(said.contains(words), "{words}:\n{said}");
    }
}
