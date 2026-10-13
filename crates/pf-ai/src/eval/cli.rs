//! The evaluation's command line: what `cargo run -p pf-ai --example assistant_eval` does with
//! its arguments. Kept here, apart from the example, so tests can hold it to its word: without
//! `--provider`, and without `--send`, no provider is made and no key is read; with `--send`,
//! nothing starts without a spending ceiling and prices given explicitly, and no request goes
//! whose worst case the ceiling can't bear.

use super::scenarios::{Scenario, scenarios};
use super::{
    Budget, Outcome, Prices, Spending, Stop, Traffic, billed_line, report, run_with, stand_in, what_is_sent,
};
use crate::agent::{Flow, MAX_OUTPUT_TOKENS};
use crate::provider::{Cancel, LlmProvider, ProviderId};
use crate::secret::ApiKey;
use crate::stage::Stage;
use crate::testing::fake_key;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// Where a real run's key comes from.
pub const KEY_VARIABLE: &str = "PIXELFLOW_EVAL_KEY";

pub const USAGE: &str = "assistant_eval: how good are the Assistant's light shows?

  assistant_eval                          run every scenario with the scripted stand-in (no model, nothing sent)
  assistant_eval --list                   the scenarios
  assistant_eval --provider openai --model <id>
                                          say what a real run would send and roughly cost, and stop
  assistant_eval --provider openai --model <id> --send --max-usd <usd> \\
      --price-in <usd> --price-out <usd> --price-cached <x> [--price-cache-write <x>]
                                          run it against that model, with the key in PIXELFLOW_EVAL_KEY,
                                          never spending more than --max-usd

  --scenario a,b           only these scenarios (names from --list)
  --flow <staged|single>   how big jobs are done: staged (a brief, a design, the parts built at
                           once, a review) or single (one conversation, a tool call at a time);
                           default staged
  --provider <name>        anthropic or openai
  --model <id>             the model, as the provider names it
  --send                   really send (without it, nothing leaves this computer)
  --max-usd <usd>          the most the whole run may spend; --send needs it
  --max-requests <n>       the most requests the whole run may make; default no limit
  --runs <n>               answer each scenario n times (models vary); default 1
  --out <folder>           also write report.md and results.json there
  --price-in <usd>         price of a million tokens sent; --send needs it (estimates assume 3)
  --price-out <usd>        price of a million tokens received; --send needs it (estimates assume 15)
  --price-cached <x>       what a token read from the cache costs, as a share of --price-in;
                           --send needs it (estimates assume 0.1)
  --price-cache-write <x>  what a token written to the cache costs, as a share of --price-in;
                           --send to anthropic needs it (OpenAI doesn't charge for it)";

/// Which prices the arguments set (the rest are the estimates' placeholders).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PricesGiven {
    pub input: bool,
    pub output: bool,
    pub cached: bool,
    pub cache_write: bool,
}

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
    pub prices_given: PricesGiven,
    /// The spending ceiling, in dollars.
    pub max_usd: Option<f64>,
    pub max_requests: Option<usize>,
    pub flow: Flow,
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
            prices_given: PricesGiven::default(),
            max_usd: None,
            max_requests: None,
            flow: Flow::Staged,
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
            "--flow" => {
                let text = value()?;
                options.flow = match text.as_str() {
                    "staged" => Flow::Staged,
                    "single" => Flow::Single,
                    _ => return Err(format!("--flow takes staged or single, not \"{text}\".")),
                };
            }
            "--runs" => {
                let text = value()?;
                options.runs = text
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=20).contains(n))
                    .ok_or_else(|| format!("--runs takes 1 to 20, not \"{text}\"."))?;
            }
            "--out" => options.out = Some(PathBuf::from(value()?)),
            "--price-in" => {
                options.prices.input = number(value()?)?;
                options.prices_given.input = true;
            }
            "--price-out" => {
                options.prices.output = number(value()?)?;
                options.prices_given.output = true;
            }
            "--price-cached" => {
                options.prices.cached = number(value()?)?;
                options.prices_given.cached = true;
            }
            "--price-cache-write" => {
                options.prices.cache_write = number(value()?)?;
                options.prices_given.cache_write = true;
            }
            "--max-usd" => {
                let text = value()?;
                options.max_usd = Some(
                    text.parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .ok_or_else(|| {
                            format!("--max-usd takes an amount of dollars above 0, not \"{text}\".")
                        })?,
                );
            }
            "--max-requests" => {
                let text = value()?;
                options.max_requests =
                    Some(text.parse::<usize>().ok().filter(|n| *n >= 1).ok_or_else(|| {
                        format!("--max-requests takes a whole number from 1, not \"{text}\".")
                    })?);
            }
            other => return Err(format!("Unknown argument \"{other}\".")),
        }
    }
    Ok(options)
}

/// The budget a run with `--send` keeps to, or why it can't start: it needs `--max-usd` and
/// every price it will be charged, given explicitly.
pub fn budget_for(options: &Options, provider: ProviderId) -> Result<Budget, String> {
    let ceiling = options
        .max_usd
        .ok_or("--send needs --max-usd <dollars>: the most this run may spend. Nothing was sent.")?;
    let given = options.prices_given;
    // OpenAI doesn't charge to write its cache (and reports no such tokens).
    let charges_cache_writes = provider == ProviderId::Anthropic;
    let missing: Vec<&str> = [
        (given.input, "--price-in"),
        (given.output, "--price-out"),
        (given.cached, "--price-cached"),
        (given.cache_write || !charges_cache_writes, "--price-cache-write"),
    ]
    .into_iter()
    .filter(|(set, _)| !set)
    .map(|(_, name)| name)
    .collect();
    if !missing.is_empty() {
        return Err(format!(
            "--send needs the prices set explicitly, from {provider}'s price list for this model: missing {}. Nothing was sent.",
            missing.join(", ")
        ));
    }
    let mut prices = options.prices;
    if !charges_cache_writes && !given.cache_write {
        prices.cache_write = 0.0;
    }
    Ok(Budget::new(ceiling, prices, options.max_requests))
}

/// The prices and limits a run keeps to, in words.
fn terms(budget: &Budget, provider: ProviderId) -> String {
    let prices = budget.prices();
    let cache_write = if provider == ProviderId::Anthropic || prices.cache_write > 0.0 {
        format!(
            "${:.4} (a share of {})",
            prices.input * prices.cache_write,
            prices.cache_write
        )
    } else {
        "not charged".to_string()
    };
    format!(
        "Prices for this run, in dollars for a million tokens: sent ${}, received ${}, read from the cache ${:.4} (a share of {}), written to the cache {cache_write}.\n\
         Spending ceiling: ${:.2} for the whole run. Before every request its worst case is worked out (all it sends, at the full input price or the cache-write price if dearer, and its output limit of {MAX_OUTPUT_TOKENS} tokens at the output price); if what has been spent plus that would pass the ceiling, it is not sent and the run stops.{}",
        prices.input,
        prices.output,
        prices.input * prices.cached,
        prices.cached,
        budget.ceiling(),
        budget
            .max_requests()
            .map_or(String::new(), |n| format!(" At most {n} requests.")),
    )
}

/// Makes the real provider and reads the user's key for it. Called once, and only for a run
/// that has both `--provider` and `--send` (and a spending ceiling and prices).
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

/// Every scenario answered by its scripted stand-in, in one conversation.
pub fn stand_in_outcomes(scenarios: &[Scenario]) -> Vec<Outcome> {
    stand_in_outcomes_with(scenarios, Flow::Single)
}

/// Every scenario answered by its scripted stand-in, big jobs done as `flow` says.
pub fn stand_in_outcomes_with(scenarios: &[Scenario], flow: Flow) -> Vec<Outcome> {
    scenarios
        .iter()
        .map(|scenario| {
            let stage = Stage::new();
            let provider = stand_in::provider(scenario, &stage, flow);
            run_with(
                scenario,
                &stage,
                &provider,
                &fake_key(),
                "stand-in",
                &Cancel::new(),
                None,
                flow,
            )
        })
        .collect()
}

fn flow_name(flow: Flow) -> &'static str {
    match flow {
        Flow::Staged => "big jobs in stages",
        Flow::Single => "every job in one conversation",
    }
}

/// What `results.json` holds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Results<'a> {
    model: &'a str,
    prices: &'a Prices,
    /// The ceiling and what was spent, for a run that sent anything.
    max_usd: Option<f64>,
    spending: Option<Spending>,
    /// Scenarios the budget kept from starting.
    not_started: &'a [String],
    outcomes: &'a [Outcome],
}

fn save(folder: &PathBuf, results: &Results<'_>, text: &str) -> Result<(), String> {
    let wrote = |e: std::io::Error| format!("Couldn't write to {}: {e}", folder.display());
    std::fs::create_dir_all(folder).map_err(wrote)?;
    std::fs::write(folder.join("report.md"), text).map_err(wrote)?;
    let json = serde_json::to_string_pretty(results).map_err(|e| e.to_string())?;
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
        let outcomes = stand_in_outcomes_with(&scenarios, options.flow);
        let model = format!("the scripted stand-in (not a model), {}", flow_name(options.flow));
        let model = model.as_str();
        let text = report(&outcomes, model, &options.prices);
        say(text.clone());
        if let Some(folder) = &options.out {
            let results = Results {
                model,
                prices: &options.prices,
                max_usd: None,
                spending: None,
                not_started: &[],
                outcomes: &outcomes,
            };
            save(folder, &results, &text)?;
        }
        return Ok(());
    };
    let model = options
        .model
        .clone()
        .ok_or("--provider needs --model: the model's id, as the provider names it.")?;
    // Money is real from here: no ceiling or no explicit prices, no start.
    let budget = if options.send {
        Some(budget_for(&options, provider)?)
    } else {
        None
    };
    say(what_is_sent(options.flow));
    // What the scripted answers send is the floor: a model reads more guides and takes more
    // steps than a script that already knows what it will do.
    let mut floor = Traffic::default();
    for outcome in stand_in_outcomes_with(&scenarios, options.flow) {
        floor.add(&outcome.measures.traffic);
    }
    let (sent, received) = floor.tokens();
    let (full, cached) = floor.cost(&options.prices);
    let runs = options.runs as f64;
    say(format!(
        "\nRoughly what it costs: the scripted answers to these {} scenarios take {} requests, about {sent} tokens sent and {received} received. A model usually takes two to four times that.\n\
         At ${}/${} a million tokens in/out ({}), {} run{} of these scenarios would be about ${:.2} to ${:.2}, or ${:.2} to ${:.2} if the provider caches what each request repeats.",
        scenarios.len(),
        floor.requests,
        options.prices.input,
        options.prices.output,
        if budget.is_some() {
            "as given".to_string()
        } else {
            format!("assumed: check {provider}'s price list and set --price-in and --price-out")
        },
        options.runs,
        if options.runs == 1 { "" } else { "s" },
        full * 2.0 * runs,
        full * 4.0 * runs,
        cached * 2.0 * runs,
        cached * 4.0 * runs,
    ));
    let Some(budget) = budget else {
        say(format!(
            "\nNothing was sent. To run it against {model}, put your {provider} key in {KEY_VARIABLE} and add --send with --max-usd and the prices (--price-in, --price-out, --price-cached{}).",
            if provider == ProviderId::Anthropic {
                ", --price-cache-write"
            } else {
                ""
            }
        ));
        return Ok(());
    };
    say(format!("\n{}\n", terms(&budget, provider)));
    let (provider_impl, key) = connect(provider)?;
    let mut outcomes = Vec::new();
    let mut not_started = Vec::new();
    for round in 0..options.runs {
        for scenario in &scenarios {
            let name = if options.runs > 1 {
                format!("{} #{}", scenario.key, round + 1)
            } else {
                scenario.key.to_string()
            };
            if budget.stopped().is_some() {
                not_started.push(name);
                continue;
            }
            let stage = Stage::new();
            let mut outcome = run_with(
                scenario,
                &stage,
                provider_impl.as_ref(),
                &key,
                &model,
                &Cancel::new(),
                Some(&budget),
                options.flow,
            );
            outcome.scenario = name;
            let spending = budget.spending();
            say(format!(
                "{}: {} of {} checks, {} requests ({} one after another, {:.1} s){}{}. Spent so far: ${:.4} of ${:.2}.",
                outcome.scenario,
                outcome.passed(),
                outcome.checks.len(),
                outcome.measures.requests,
                outcome.measures.critical_path,
                outcome.measures.wall_ms as f64 / 1000.0,
                outcome
                    .stopped
                    .map_or(String::new(), |s| format!(" (stopped: {})", s.label())),
                outcome
                    .error
                    .as_ref()
                    .map_or(String::new(), |e| format!(" (stopped: {e})")),
                spending.spent,
                budget.ceiling(),
            ));
            outcomes.push(outcome);
        }
    }
    let spending = budget.spending();
    let mut text = report(
        &outcomes,
        &format!("{model} ({provider}), {}", flow_name(options.flow)),
        budget.prices(),
    );
    text.push_str(&format!(
        "\n## Spending\n\n{}\n\n{} requests sent. Ceiling ${:.2}; spent ${:.4}.{}{}\n",
        terms(&budget, provider),
        spending.requests,
        budget.ceiling(),
        spending.spent,
        match (spending.stopped, spending.refused_worst_case) {
            (Some(Stop::Budget), Some(worst)) => format!(
                " Stopped: budget (the next request could have cost up to ${worst:.4}, which would have passed the ceiling)."
            ),
            (Some(stop), _) => format!(" Stopped: {}.", stop.label()),
            (None, _) => String::new(),
        },
        if not_started.is_empty() {
            String::new()
        } else {
            format!(" Not started: {}.", not_started.join(", "))
        },
    ));
    text.push_str(&format!("\n{}\n", billed_line(&spending.billed, budget.prices())));
    say(format!("\n{text}"));
    say(format!(
        "Final total: ${:.4} of the ${:.2} ceiling, from what {provider} reported ({} of {} requests reported their tokens; the rest are counted at their worst case).",
        spending.spent,
        budget.ceiling(),
        spending.billed.reported,
        spending.requests,
    ));
    if let Some(folder) = &options.out {
        let label = format!("{model} ({provider})");
        let results = Results {
            model: &label,
            prices: budget.prices(),
            max_usd: Some(budget.ceiling()),
            spending: Some(spending),
            not_started: &not_started,
            outcomes: &outcomes,
        };
        save(folder, &results, &text)?;
    }
    Ok(())
}
