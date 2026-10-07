//! `pixelflow` command-line tool.

/// `print!` that returns an error instead of panicking when stdout is closed (`| head`).
macro_rules! out {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        write!(std::io::stdout(), $($arg)*)
    }};
}

/// `println!` that returns an error instead of panicking when stdout is closed (`| head`).
macro_rules! outln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        writeln!(std::io::stdout(), $($arg)*)
    }};
}

mod devices;
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
    /// Find FPP, Falcon, and WLED controllers on the local network (read-only).
    Discover(devices::DiscoverArgs),
    /// Show a controller's identity and configuration, and what importing it would add (read-only).
    Device(devices::DeviceArgs),
    /// Import an xLights show folder: report what comes in, and optionally save it as a show file.
    Xlights {
        /// The xLights show folder (holding xlights_rgbeffects.xml).
        folder: PathBuf,
        /// Save the imported show here.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Import an xLights sequence (.xsq) onto a show: report what comes in, and optionally save
    /// it as a PixelFlow sequence file.
    XlightsSequence {
        /// The xLights sequence (.xsq).
        file: PathBuf,
        /// The show it plays on (props and groups are matched by their xLights names).
        #[arg(long)]
        show: PathBuf,
        /// Save the imported sequence here (.pfseq.json).
        #[arg(long)]
        save: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let result = run(Cli::parse()).and_then(|code| {
        use std::io::Write as _;
        std::io::stdout().flush()?;
        Ok(code)
    });
    match result {
        Ok(code) => code,
        // Whoever was reading the output stopped (`pixelflow map show | head`): nothing is wrong.
        Err(err) if is_closed_pipe(&err) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn is_closed_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|e| e.downcast_ref::<std::io::Error>())
        .any(|e| e.kind() == std::io::ErrorKind::BrokenPipe)
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Validate { show } => {
            let show = load(&show)?;
            let mut report = pf_model::validate_show(&show);
            let (map, wiring) = pf_mapping::map_show(&show);
            report.extend(wiring);
            out!("{}", report::summary(&show, &map))?;
            out!("{}", report::issues(&report))?;
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
                outln!("{}", serde_json::to_string_pretty(&map)?)?;
            } else {
                out!("{}", report::channel_map(&show, &map))?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::TestPattern(args) => {
            let show = load(&args.show)?;
            test_pattern::run(&show, &args)
        }
        Command::Discover(args) => devices::discover(&args),
        Command::Device(args) => devices::device(&args),
        Command::Xlights { folder, save } => {
            let imported = pf_xlights::import_folder(&folder)?;
            let s = &imported.summary;
            outln!(
                "{}: {} props, {} pixels, {} controllers, {} props wired, {} groups",
                imported.show.name,
                s.props,
                s.pixels,
                s.controllers,
                s.wired,
                s.groups
            )?;
            for note in &imported.notes {
                outln!("  - {note}")?;
            }
            if let Some(path) = save {
                // Checked as opening the file will check it, so a saved import always opens.
                let show =
                    pf_model::check_show(&imported.show).context("the imported show can't be saved")?;
                let json = pf_model::show_to_json(&show)?;
                std::fs::write(&path, json).with_context(|| format!("could not save {}", path.display()))?;
                outln!("Saved {}", path.display())?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::XlightsSequence { file, show, save } => {
            let show = load(&show)?;
            let imported = pf_xlights::import_sequence_file(&file, &show, pf_audio::find_audio)?;
            let (seq, s) = (&imported.sequence, &imported.summary);
            outln!(
                "{}: {} long, {} rows, {} effects ({} exact, {} approximated, {} placeholders, {} not imported), {} timing tracks, {} marks ({} lyrics, {} not imported)",
                seq.name,
                pf_sequence::format_ms(seq.duration_ms),
                s.rows,
                s.effects,
                s.exact,
                s.approximate,
                s.placeholders,
                s.skipped,
                s.timing_tracks,
                s.marks,
                s.lyric_marks,
                s.marks_skipped
            )?;
            match &seq.audio {
                Some(audio) => outln!("Music: {audio}")?,
                None => outln!("Music: none")?,
            }
            for note in &imported.notes {
                outln!("  - {note}")?;
            }
            if let Some(path) = save {
                // Checked as opening the file will check it, so a saved import always opens.
                let checked =
                    pf_sequence::check_sequence(seq).context("the imported sequence can't be saved")?;
                let json = pf_sequence::sequence_to_json(&checked)?;
                std::fs::write(&path, json).with_context(|| format!("could not save {}", path.display()))?;
                outln!("Saved {}", path.display())?;
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn load(path: &Path) -> Result<Show> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    pf_model::show_from_json(&text).with_context(|| format!("could not load {}", path.display()))
}
