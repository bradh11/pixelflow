//! The evaluation's command line: what `cargo run -p pf-ai --example assistant_eval` does with
//! its arguments. Kept here, apart from the example, so tests can hold it to its word: without
//! `--provider`, and without `--send`, no provider is made and no key is read.

use super::scenarios::{Scenario, scenarios};
use super::{Outcome, Prices, Traffic, report, run, stand_in, what_is_sent};
use crate::provider::{Cancel, LlmProvider, ProviderId};
use crate::secret::ApiKey;
use crate::stage::Stage;
use crate::testing::{ScriptedProvider, fake_key};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// Where a real run's key comes from.
pub const KEY_VARIABLE: &str = "PIXELFLOW_EVAL_KEY";

pub const USAGE: &str = "assistant_eval: how good are the Assistant's light shows?

  assistant_eval                          run every scenario with the scripted stand-in (no model, nothing sent)
  assistant_eval --list                   the scenarios
  assistant_eval --provider anthropic --model <id>
                                          say what a real run would send and roughly cost, and stop
  assistant_eval --provider anthropic --model <id> --send
                                          run it against that model, with the key in PIXELFLOW_EVAL_KEY

  --scenario a,b       only these scenarios (names from --list)
  --provider <name>    anthropic or openai
  --model <id>         the model, as the provider names it
  --send               really send (without it, nothing leaves this computer)
  --runs <n>           answer each scenario n times (models vary); default 1
  --out <folder>       also write report.md and results.json there
  --price-in <usd>     assumed price of a million tokens sent; default 3
  --price-out <usd>    assumed price of a million tokens received; default 15
  --price-cached <x>   what a cached token costs, as a share of --price-in; default 0.1";

/// What the arguments ask for.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub help: bool,
    pub list: bool,
    /// The scenarios to run (empty: all of them).
    pub only: Vec<String>,
    pub provider: Option<ProviderId>,
    pub model: Option<String>,
    pub send: bool,
    pub runs: usize,
    pub out: Option<PathBuf>,
    pub prices: Prices,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            help: false,
            list: false,
            only: Vec::new(),
            provider: None,
            model: None,
            send: false,
            runs: 1,
            out: None,
            prices: Prices::default(),
        }
    }
}

pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value."))
        };
        let number = |text: String| {
            text.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or_else(|| format!("{arg} takes a number, not \"{text}\"."))
        };
        match arg.as_str() {
            "-h" | "--help" => options.help = true,
            "--list" => options.list = true,
            "--send" => options.send = true,
            "--scenario" => options
                .only
                .extend(value()?.split(',').map(|s| s.trim().to_string())),
            "--provider" => {
                let name = value()?;
                options.provider = Some(
                    ProviderId::ALL
                        .into_iter()
                        .find(|p| p.account() == name.to_lowercase())
                        .ok_or_else(|| format!("There's no provider \"{name}\"; use anthropic or openai."))?,
                );
            }
            "--model" => options.model = Some(value()?),
            "--runs" => {
                let text = value()?;
                options.runs = text
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=20).contains(n))
                    .ok_or_else(|| format!("--runs takes 1 to 20, not \"{text}\"."))?;
            }
            "--out" => options.out = Some(PathBuf::from(value()?)),
            "--price-in" => options.prices.input = number(value()?)?,
            "--price-out" => options.prices.output = number(value()?)?,
            "--price-cached" => options.prices.cached = number(value()?)?,
            other => return Err(format!("Unknown argument \"{other}\".")),
        }
    }
    Ok(options)
}

/// Makes the real provider and reads the user's key for it. Called once, and only for a run
/// that has both `--provider` and `--send`.
pub type Connect<'a> = dyn FnMut(ProviderId) -> Result<(Arc<dyn LlmProvider>, ApiKey), String> + 'a;

fn chosen(only: &[String]) -> Result<Vec<Scenario>, String> {
    let all = scenarios();
    let keys: Vec<&str> = all.iter().map(|s| s.key).collect();
    if let Some(unknown) = only.iter().find(|name| !keys.contains(&name.as_str())) {
        return Err(format!(
            "There's no scenario \"{unknown}\"; choose from {}.",
            keys.join(", ")
        ));
    }
    Ok(all
        .into_iter()
        .filter(|s| only.is_empty() || only.iter().any(|name| name == s.key))
        .collect())
}

/// Every scenario answered by its scripted stand-in.
pub fn stand_in_outcomes(scenarios: &[Scenario]) -> Vec<Outcome> {
    scenarios
        .iter()
        .map(|scenario| {
            let stage = Stage::new();
            let provider = ScriptedProvider::new(stand_in::script(scenario, &stage));
            run(
                scenario,
                &stage,
                &provider,
                &fake_key(),
                "stand-in",
                &Cancel::new(),
            )
        })
        .collect()
}

fn save(folder: &PathBuf, outcomes: &[Outcome], text: &str) -> Result<(), String> {
    let wrote = |e: std::io::Error| format!("Couldn't write to {}: {e}", folder.display());
    std::fs::create_dir_all(folder).map_err(wrote)?;
    std::fs::write(folder.join("report.md"), text).map_err(wrote)?;
    let json = serde_json::to_string_pretty(outcomes).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("results.json"), json).map_err(wrote)
}

/// Runs the command line, writing what it has to say to `out`. Returns the exit status.
pub fn main(args: &[String], connect: &mut Connect<'_>, out: &mut dyn Write) -> i32 {
    match command(args, connect, out) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(out, "{message}\n\n{USAGE}");
            2
        }
    }
}

fn command(args: &[String], connect: &mut Connect<'_>, out: &mut dyn Write) -> Result<(), String> {
    let options = parse(args)?;
    let mut say = |text: String| {
        let _ = writeln!(out, "{text}");
    };
    if options.help {
        say(USAGE.to_string());
        return Ok(());
    }
    let scenarios = chosen(&options.only)?;
    if options.list {
        for s in &scenarios {
            say(format!(
                "{}: {}\n    \"{}\"\n    open: {}",
                s.key, s.title, s.request, s.opens
            ));
        }
        return Ok(());
    }
    let Some(provider) = options.provider else {
        say("Stand-in run: every answer is scripted. No model is called and nothing is sent.\n".into());
        let outcomes = stand_in_outcomes(&scenarios);
        let text = report(&outcomes, "the scripted stand-in (not a model)", &options.prices);
        say(text.clone());
        if let Some(folder) = &options.out {
            save(folder, &outcomes, &text)?;
        }
        return Ok(());
    };
    let model = options
        .model
        .clone()
        .ok_or("--provider needs --model: the model's id, as the provider names it.")?;
    say(what_is_sent());
    // What the scripted answers send is the floor: a model reads more guides and takes more
    // steps than a script that already knows what it will do.
    let mut floor = Traffic::default();
    for outcome in stand_in_outcomes(&scenarios) {
        floor.add(&outcome.measures.traffic);
    }
    let (sent, received) = floor.tokens();
    let (full, cached) = floor.cost(&options.prices);
    let runs = options.runs as f64;
    say(format!(
        "\nRoughly what it costs: the scripted answers to these {} scenarios take {} requests, about {sent} tokens sent and {received} received. A model usually takes two to four times that.\n\
         At ${}/${} a million tokens in/out (assumed: check {provider}'s price list and set --price-in and --price-out), {} run{} of these scenarios would be about ${:.2} to ${:.2}, or ${:.2} to ${:.2} if the provider caches what each request repeats.",
        scenarios.len(),
        floor.requests,
        options.prices.input,
        options.prices.output,
        options.runs,
        if options.runs == 1 { "" } else { "s" },
        full * 2.0 * runs,
        full * 4.0 * runs,
        cached * 2.0 * runs,
        cached * 4.0 * runs,
    ));
    if !options.send {
        say(format!(
            "\nNothing was sent. To run it against {model}, put your {provider} key in {KEY_VARIABLE} and add --send."
        ));
        return Ok(());
    }
    let (provider_impl, key) = connect(provider)?;
    let mut outcomes = Vec::new();
    for round in 0..options.runs {
        for scenario in &scenarios {
            let stage = Stage::new();
            let mut outcome = run(
                scenario,
                &stage,
                provider_impl.as_ref(),
                &key,
                &model,
                &Cancel::new(),
            );
            if options.runs > 1 {
                outcome.scenario = format!("{} #{}", outcome.scenario, round + 1);
            }
            say(format!(
                "{}: {} of {} checks, {} requests{}",
                outcome.scenario,
                outcome.passed(),
                outcome.checks.len(),
                outcome.measures.requests,
                outcome
                    .error
                    .as_ref()
                    .map_or(String::new(), |e| format!(" (stopped: {e})")),
            ));
            outcomes.push(outcome);
        }
    }
    let text = report(&outcomes, &format!("{model} ({provider})"), &options.prices);
    say(format!("\n{text}"));
    if let Some(folder) = &options.out {
        save(folder, &outcomes, &text)?;
    }
    Ok(())
}
