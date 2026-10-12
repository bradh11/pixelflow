//! The evaluation harness, held to its word without a model: a careful scripted answer to each
//! scenario passes that scenario's checks and an idle one fails them; what goes to the model is
//! counted; and the command line makes no provider and reads no key unless it is told to send.

use pf_ai::eval::cli::{self, KEY_VARIABLE, stand_in_outcomes};
use pf_ai::eval::{self, Metered, Prices, Traffic, scenarios, stand_in};
use pf_ai::stage::Stage;
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{ApiKey, Cancel, LlmProvider, ProviderId};
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
    // Every request carries the instructions and the tools; the second repeats the first and
    // adds the two guides it asked for.
    let fixed = pf_ai::agent::SYSTEM_PROMPT.len();
    assert!(traffic.largest_request_bytes > fixed + 40_000, "{traffic:?}");
    assert!(
        traffic.repeated_bytes > fixed + 40_000 && traffic.repeated_bytes < traffic.sent_bytes / 2 + 1_000
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
    };
    let prices = Prices {
        input: 2.0,
        output: 10.0,
        cached: 0.5,
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
        "Nothing was sent. To run it against some-model, put your Anthropic key in PIXELFLOW_EVAL_KEY and add --send.",
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
        status == 0 && said.contains("--send               really send"),
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
