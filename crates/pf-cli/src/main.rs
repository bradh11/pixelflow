//! `pixelflow` command-line tool.

mod report;
mod test_pattern;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pf_model::Show;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "pixelflow", version, about = "PixelFlow command-line tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a show file for problems. Exits 1 if any errors are found.
    Validate {
        /// Path to a .pixelflow.json show file.
        show: PathBuf,
    },
    /// Print how props are wired to controller channels and universes.
    Map {
        /// Path to a .pixelflow.json show file.
        show: PathBuf,
        /// Print the full channel map as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Send a test pattern to the show's controllers. Exits 1 if the show has errors.
    TestPattern(test_pattern::Args),
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Validate { show } => {
            let show = load(&show)?;
            let mut report = pf_model::validate_show(&show);
            let (map, wiring) = pf_mapping::map_show(&show);
            report.extend(wiring);
            print!("{}", report::summary(&show, &map));
            print!("{}", report::issues(&report));
            Ok(if report.has_errors() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Map { show, json } => {
            let show = load(&show)?;
            let (map, _) = pf_mapping::map_show(&show);
            if json {
                println!("{}", serde_json::to_string_pretty(&map)?);
            } else {
                print!("{}", report::channel_map(&show, &map));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::TestPattern(args) => {
            let show = load(&args.show)?;
            test_pattern::run(&show, &args)
        }
    }
}

fn load(path: &Path) -> Result<Show> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    pf_model::show_from_json(&text).with_context(|| format!("could not load {}", path.display()))
}
