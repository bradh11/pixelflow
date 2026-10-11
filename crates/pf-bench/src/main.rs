//! `pf-bench`: PixelFlow's end-to-end scale benchmark. See `docs/benchmarks.md`.

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use pf_bench::effects::{ALONE_SECONDS, breakdown};
use pf_bench::measure::{Mode, Options};
use pf_bench::pipeline::{Controllers, Workload};
use pf_bench::report::{Report, SequenceInfo, show_info, verdict};
use pf_bench::run::run;
use pf_bench::sequence::{DEFAULT_SECONDS, Variant, synthetic_sequence};
use pf_bench::show::{TIERS, parse_tier, synthetic_show, tier_label};
use pf_bench::sink::SinkKind;
use pf_bench::system::{load_average, optimized};
use pf_output::OutputSettings;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[global_allocator]
static ALLOCATOR: pf_bench::alloc::Counting = pf_bench::alloc::Counting;

/// What one invocation does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum What {
    /// One frame after another on one thread, as fast as they go: what each stage costs.
    Asap,
    /// In real time on playback's two threads: late and dropped frames, and jitter.
    Paced,
    /// In real time in the engine itself, watched from outside: a check on the paced runs.
    Engine,
    /// No run: which effects the render time goes on.
    Effects,
}

impl What {
    fn label(self) -> &'static str {
        match self {
            What::Asap => "asap",
            What::Paced => "paced",
            What::Engine => "engine",
            What::Effects => "effects",
        }
    }
}

#[derive(Parser)]
#[command(
    name = "pf-bench",
    version,
    about = "PixelFlow's end-to-end scale benchmark: synthetic shows through render, mapping, packetizing, and sending",
    after_help = "Everything is synthetic, and nothing is ever sent off this machine. See docs/benchmarks.md."
)]
struct Cli {
    /// Pixels in the synthetic show: 20k, 100k, 200k, or any count from 500.
    #[arg(long, default_value = "200k", value_parser = parse_tier)]
    tier: u32,
    /// Which synthetic sequence plays.
    #[arg(long, value_enum, default_value = "typical")]
    variant: Variant,
    /// How the pipeline is driven.
    #[arg(long, value_enum, default_value = "paced")]
    mode: What,
    /// Where the packets go.
    #[arg(long, value_enum, default_value = "null")]
    sink: SinkKind,
    /// Seconds of the sequence to measure.
    #[arg(long, default_value_t = 60)]
    seconds: u32,
    /// Seconds to play first, reported apart and left out of everything else.
    #[arg(long, default_value_t = 2)]
    warmup_seconds: u32,
    /// How long the synthetic sequence is (a longer run plays it again from the top).
    #[arg(long, default_value_t = DEFAULT_SECONDS)]
    sequence_seconds: u32,
    /// Output passes a second in a paced run, instead of live playback's twice the frame rate.
    #[arg(long)]
    send_hz: Option<u16>,
    /// Leave out the music and the picture, as the engine check has to (to compare with it).
    #[arg(long)]
    without_media: bool,
    /// Run the whole matrix: every tier, both sequences, flat out and paced, both sinks. Each
    /// run is a process of its own, so one can't warm up or fill memory for the next.
    #[arg(long)]
    all: bool,
    /// The tiers `--all` runs.
    #[arg(long, value_delimiter = ',', value_parser = parse_tier, default_values_t = TIERS)]
    tiers: Vec<u32>,
    /// With `--all`: also say which effects the time goes on, at the largest tier.
    #[arg(long)]
    effects: bool,
    /// With `--all`: also play each tier in the engine itself.
    #[arg(long)]
    engine_check: bool,
    /// Before each run, wait (up to two minutes) for the machine's one-minute load average to
    /// fall below this, so the run doesn't share the machine with something else.
    #[arg(long)]
    calm: Option<f64>,
    /// Print the show and its sequences, and run nothing.
    #[arg(long)]
    describe: bool,
    /// Print a report saved with `--json` as Markdown again, and run nothing.
    #[arg(long, value_name = "FILE")]
    render: Option<PathBuf>,
    /// Write the results as JSON here.
    #[arg(long)]
    json: Option<PathBuf>,
    /// Write the results as Markdown here (they are printed either way).
    #[arg(long)]
    markdown: Option<PathBuf>,
}

/// What `tier`'s show and its sequences are made of.
fn describe(tier: u32, seconds: u32, variants: &[Variant], report: &mut Report) -> Result<()> {
    let show = synthetic_show(tier);
    let (map, _) = pf_mapping::map_show(&show.show);
    let plan = pf_output::build_plan(&show.show, &map);
    let wiring = Controllers::new(plan, &OutputSettings::default())?.wiring();
    for &variant in variants {
        report.sequences.push(SequenceInfo {
            tier: tier_label(tier),
            summary: synthetic_sequence(&show, variant, seconds).summary,
        });
    }
    report.shows.push(show_info(show.composition(), wiring));
    Ok(())
}

/// One run (or one tier's effects), as a report of its own.
fn one(cli: &Cli) -> Result<Report> {
    let mut report = Report::new();
    let said = |what: &str| eprintln!("  {what}");
    let workload = Workload::new(cli.tier, cli.variant, cli.sequence_seconds)?;
    let workload = if cli.without_media || cli.mode == What::Engine {
        workload.without_media()
    } else {
        workload
    };
    if let Some(limit) = cli.calm {
        wait_for_calm(limit);
    }
    let mode = match cli.mode {
        What::Asap => Mode::Asap,
        What::Paced => Mode::Paced,
        What::Engine => Mode::Engine,
        What::Effects => {
            describe(cli.tier, cli.sequence_seconds, &Variant::ALL, &mut report)?;
            report
                .effects
                .push(breakdown(&workload, cli.seconds, ALONE_SECONDS, &said));
            return Ok(report);
        }
    };
    describe(cli.tier, cli.sequence_seconds, &[cli.variant], &mut report)?;
    let options = Options {
        seconds: cli.seconds,
        warmup_seconds: cli.warmup_seconds,
        send_hz: cli.send_hz,
    };
    let result = run(&workload, mode, cli.sink, options)?;
    said(&format!("{}: {}", verdict(&result), result.verdict.summary));
    report.runs.push(result);
    Ok(report)
}

/// Waits until the machine's one-minute load average is under `limit`, or two minutes.
fn wait_for_calm(limit: f64) {
    for waited in 0..24 {
        match load_average() {
            Some(load) if load >= limit => {
                if waited == 0 {
                    eprintln!("  waiting for the machine to go quiet (its load average is {load:.1})");
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
            _ => return,
        }
    }
}

/// Runs this program again with `args`, and reads the report it writes.
fn child(args: &[String], scratch: &Path) -> Result<Report> {
    let exe = std::env::current_exe().context("couldn't find the benchmark's own program")?;
    let status = Command::new(exe)
        .args(args)
        .arg("--json")
        .arg(scratch)
        .stdout(Stdio::null())
        .status()
        .context("couldn't start a run")?;
    if !status.success() {
        bail!("the run `pf-bench {}` failed", args.join(" "));
    }
    let text = std::fs::read_to_string(scratch).context("the run left no results")?;
    serde_json::from_str(&text).context("the run's results couldn't be read")
}

/// Every run of the matrix, each in a process of its own.
fn all(cli: &Cli) -> Result<Report> {
    let scratch = std::env::temp_dir().join(format!("pf-bench-{}.json", std::process::id()));
    let mut report = Report::new();
    let mut common = vec![
        "--seconds".to_string(),
        cli.seconds.to_string(),
        "--warmup-seconds".to_string(),
        cli.warmup_seconds.to_string(),
        "--sequence-seconds".to_string(),
        cli.sequence_seconds.to_string(),
    ];
    if let Some(limit) = cli.calm {
        common.extend(["--calm".to_string(), limit.to_string()]);
    }
    let mut runs: Vec<Vec<String>> = Vec::new();
    for &tier in &cli.tiers {
        for variant in Variant::ALL {
            let modes: &[What] = if cli.engine_check {
                &[What::Asap, What::Paced, What::Engine]
            } else {
                &[What::Asap, What::Paced]
            };
            for &mode in modes {
                for sink in SinkKind::ALL {
                    // The engine check is of the player, whatever it sends into: once is enough.
                    if mode == What::Engine && sink == SinkKind::Loopback {
                        continue;
                    }
                    let mut args = vec![
                        "--tier".to_string(),
                        tier_label(tier),
                        "--variant".to_string(),
                        variant.label().to_string(),
                        "--mode".to_string(),
                        mode.label().to_string(),
                        "--sink".to_string(),
                        sink.label().to_string(),
                    ];
                    args.extend(common.iter().cloned());
                    runs.push(args);
                }
            }
        }
    }
    if cli.effects {
        let largest = cli.tiers.iter().copied().max().unwrap_or(cli.tier);
        let mut args = vec![
            "--tier".to_string(),
            tier_label(largest),
            "--mode".to_string(),
            "effects".to_string(),
        ];
        args.extend(common.iter().cloned());
        runs.push(args);
    }
    for (n, args) in runs.iter().enumerate() {
        eprintln!("[{}/{}] pf-bench {}", n + 1, runs.len(), args.join(" "));
        let result = child(args, &scratch);
        let _ = std::fs::remove_file(&scratch);
        report.merge(result?);
    }
    Ok(report)
}

fn work(cli: &Cli) -> Result<()> {
    if !optimized() {
        eprintln!(
            "This is an unoptimized build: its timings say nothing about PixelFlow. Use `cargo run --release -p pf-bench`."
        );
    }
    let report = if let Some(saved) = &cli.render {
        let text =
            std::fs::read_to_string(saved).with_context(|| format!("couldn't read {}", saved.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} isn't a saved report", saved.display()))?
    } else if cli.describe {
        let mut report = Report::new();
        let tiers = if cli.all { &cli.tiers[..] } else { &[cli.tier][..] };
        for &tier in tiers {
            describe(tier, cli.sequence_seconds, &Variant::ALL, &mut report)?;
        }
        report
    } else if cli.all {
        all(cli)?
    } else {
        one(cli)?
    };
    if let Some(path) = &cli.json {
        std::fs::write(path, report.json()).with_context(|| format!("couldn't write {}", path.display()))?;
    }
    let markdown = report.markdown();
    if let Some(path) = &cli.markdown {
        std::fs::write(path, &markdown).with_context(|| format!("couldn't write {}", path.display()))?;
    }
    println!("{markdown}");
    Ok(())
}

fn main() -> ExitCode {
    match work(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pf-bench: {error:#}");
            ExitCode::FAILURE
        }
    }
}
