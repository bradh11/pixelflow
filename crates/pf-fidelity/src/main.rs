//! `pf-fidelity`: measures how an xLights sequence imported into PixelFlow looks next to the
//! frames xLights rendered.

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use pf_fidelity::compare::{Options, Prepared, Request};
use pf_fidelity::corpus::{self, MANIFEST_VARIABLE, Manifest};
use pf_fidelity::metrics::Limits;
use pf_fidelity::report::{headline, sequence_markdown};
use pf_fidelity::scorecard::Scorecard;
use pf_fidelity::strips;
use pf_fidelity::{calibrate, fingerprint, labels, madeup, regress};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "pf-fidelity",
    version,
    about = "Measures how an xLights sequence imported into PixelFlow looks next to the frames xLights rendered"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The three files of one comparison.
#[derive(Args)]
struct Files {
    /// The xLights show folder the reference was rendered on (holding xlights_rgbeffects.xml).
    #[arg(long)]
    show: PathBuf,
    /// The xLights sequence.
    #[arg(long)]
    xsq: PathBuf,
    /// The .fseq xLights rendered from it.
    #[arg(long)]
    fseq: PathBuf,
}

#[derive(Args)]
struct How {
    /// A pixel matches when every channel is within this much (of 255).
    #[arg(long, default_value_t = Limits::default().tolerance)]
    tolerance: u8,
    /// A pixel is lit when its brightest channel is above this.
    #[arg(long, default_value_t = Limits::default().lit)]
    lit: u8,
    /// Compare each PixelFlow frame with xLights' frame this many later.
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    frame_offset: i64,
    /// Don't read the music: effects that follow it are set apart, not scored.
    #[arg(long)]
    no_music: bool,
    /// Leave out a prop wherever the reference keeps it dark this long (milliseconds) while
    /// PixelFlow lights it: xLights hadn't rendered it. 0 scores everything.
    #[arg(long, default_value_t = Limits::default().dark_run_ms)]
    dark_run_ms: u32,
    /// Judge every effect moment by moment against xLights' render alone, whatever chance
    /// decides in it: the numbers from before effects that draw on chance were held against
    /// PixelFlow's own renders from other seeds.
    #[arg(long)]
    by_moment: bool,
    /// How many more times to render the sequence from other seeds, for the effects that draw
    /// on chance to be held against. More is steadier and slower.
    #[arg(long, default_value_t = pf_fidelity::compare::OTHER_SEEDS)]
    seeds: u32,
}

impl How {
    fn options(&self) -> Options {
        Options {
            limits: Limits {
                tolerance: self.tolerance,
                lit: self.lit,
                dark_run_ms: self.dark_run_ms,
                ..Limits::default()
            },
            frame_offset: self.frame_offset,
            skip_music: self.no_music,
            by_moment: self.by_moment,
            seeds: Some(self.seeds.max(1)),
            ..Options::default()
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Compare one sequence and print its scorecard (or write it with --out).
    Score {
        #[command(flatten)]
        files: Files,
        #[command(flatten)]
        how: How,
        /// What the scorecard calls the sequence (its file's name otherwise).
        #[arg(long)]
        label: Option<String>,
        /// Write the scorecard here as <label>.json and <label>.md.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Compare every sequence a manifest lists, and score them together.
    Corpus {
        /// The manifest (JSON); the PF_FIDELITY_CORPUS environment variable otherwise.
        manifest: Option<PathBuf>,
        #[command(flatten)]
        how: How,
        /// Write every scorecard and the corpus's here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Hold the importer's table of expected looks against a scorecard (a sequence's or a
    /// corpus's JSON): what each kind of effect earned, and where the table says otherwise.
    Labels {
        /// The scorecard, as `score --out` or `corpus --out` wrote it.
        scorecard: PathBuf,
    },
    /// Hold the look score against PixelFlow's own renders, where the answer is known: every
    /// seeded effect against itself from another seed, and against itself with one setting
    /// wrong. No xLights files are needed.
    Calibrate {
        /// How long each effect runs, in seconds.
        #[arg(long, default_value_t = calibrate::Settings::default().seconds)]
        seconds: u32,
        /// Pairs of seeds each line is scored over.
        #[arg(long, default_value_t = calibrate::Settings::default().pairs)]
        pairs: u32,
        /// Milliseconds a frame.
        #[arg(long, default_value_t = calibrate::Settings::default().frame_ms)]
        frame_ms: u32,
        /// How many more renders from other seeds an effect that draws on chance is held
        /// against.
        #[arg(long, default_value_t = calibrate::Settings::default().seeds)]
        seeds: u32,
        /// Make one table alone: same, wrong, short, or timed.
        #[arg(long, value_parser = table)]
        only: Option<calibrate::Table>,
        /// Write the tables here as calibration.md.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write the made-up corpus kept in the repository: its show folder and sequences, as
    /// `madeup.rs` makes them. Its `.fseq` references come from xLights (corpus/README.md).
    CorpusFiles {
        /// The corpus folder to write.
        #[arg(long, default_value = madeup::CORPUS_DIR)]
        dir: PathBuf,
    },
    /// Render each sequence of the corpus with PixelFlow alone and hold it to its fingerprint:
    /// fails when what PixelFlow draws has changed. Needs no xLights reference.
    Fingerprint {
        /// The corpus folder (holding manifest.json).
        #[arg(long, default_value = madeup::CORPUS_DIR)]
        corpus: PathBuf,
        /// Write the fingerprint of what PixelFlow draws now: accepts a change made on purpose.
        #[arg(long)]
        accept: bool,
    },
    /// Score each sequence of the corpus that has an xLights reference, and hold every effect's
    /// look to the baseline: fails when an import has got materially worse. Passes, and says
    /// so, while the corpus has no references.
    Regress {
        /// The corpus folder (holding manifest.json).
        #[arg(long, default_value = madeup::CORPUS_DIR)]
        corpus: PathBuf,
        /// Make the baseline again from the import as it is now: accepts a change made on
        /// purpose, or records the references when they first arrive.
        #[arg(long)]
        accept: bool,
        /// With --accept: how many scorings, each from other seeds, the baseline's spread is
        /// measured over.
        #[arg(long, default_value_t = regress::RUNS)]
        runs: u32,
    },
    /// Draw frames as PNGs: xLights' above, PixelFlow's below.
    Strips {
        #[command(flatten)]
        files: Files,
        #[command(flatten)]
        how: How,
        /// Draw this effect (its number in the scorecard), frames spread over it. Repeatable.
        #[arg(long)]
        effect: Vec<usize>,
        /// Draw from this moment (milliseconds) instead of an effect.
        #[arg(long)]
        at: Option<u64>,
        /// With --at: milliseconds between the frames drawn (one frame otherwise).
        #[arg(long)]
        every: Option<u64>,
        /// The prop, group, or Prop/Submodel to draw (an effect's own target otherwise).
        #[arg(long)]
        target: Option<String>,
        /// How many frames to draw.
        #[arg(long, default_value_t = 8)]
        frames: u32,
        /// Screen pixels per light (sized to fit otherwise).
        #[arg(long, default_value_t = 0)]
        scale: u32,
        /// Also print each frame's mean color, lit pixels, and error on both sides.
        #[arg(long)]
        levels: bool,
        /// The folder to write the PNGs to.
        #[arg(long)]
        out: PathBuf,
    },
}

/// One of the calibration's tables, by name.
fn table(name: &str) -> Result<calibrate::Table, String> {
    match name {
        "same" => Ok(calibrate::Table::Same),
        "wrong" => Ok(calibrate::Table::Wrong),
        "short" => Ok(calibrate::Table::Short),
        "timed" => Ok(calibrate::Table::Timed),
        other => Err(format!("{other} isn't a table: same, wrong, short, or timed")),
    }
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

/// Prints a report, and adds it to the job's summary when run in GitHub Actions.
fn report(markdown: &str) {
    print!("{markdown}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        let file = std::fs::OpenOptions::new().append(true).create(true).open(path);
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{markdown}");
        }
    }
}

/// A line GitHub Actions shows on the run's page (`kind` is notice, warning, or error); nothing
/// elsewhere.
fn annotate(kind: &str, title: &str, message: &str) {
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        println!("::{kind} title={title}::{message}");
    }
}

/// The corpus's manifest.
fn corpus_manifest(corpus: &std::path::Path) -> Result<Manifest> {
    let path = corpus.join("manifest.json");
    let manifest = Manifest::read(&path)?;
    if manifest.sequences.is_empty() {
        bail!("{} lists no sequences", path.display());
    }
    Ok(manifest)
}

fn run_fingerprint(corpus: &std::path::Path, accept: bool) -> Result<ExitCode> {
    let manifest = corpus_manifest(corpus)?;
    let started = std::time::Instant::now();
    let now = fingerprint::take(&manifest, |label| eprintln!("{label}: rendered"))?;
    eprintln!("rendered in {:.1} s", started.elapsed().as_secs_f64());
    let kept = fingerprint::read(corpus)?;
    if accept {
        let checked = kept.as_ref().map(|k| fingerprint::check(k, &now));
        fingerprint::write(corpus, &fingerprint::accepted(kept.as_ref(), now))?;
        if let Some(checked) = checked {
            print!("{}", fingerprint::markdown(&checked));
        }
        println!("Wrote {}", corpus.join(fingerprint::FILE).display());
        return Ok(ExitCode::SUCCESS);
    }
    let Some(kept) = kept else {
        bail!(
            "{} has no {}: make it with `pf-fidelity fingerprint --accept`",
            corpus.display(),
            fingerprint::FILE
        );
    };
    let checked = fingerprint::check(&kept, &now);
    report(&fingerprint::markdown(&checked));
    if checked.passed() {
        Ok(ExitCode::SUCCESS)
    } else {
        annotate(
            "error",
            "PixelFlow's render of the corpus changed",
            "See the table in the log. If the change is meant, run `cargo run --release -p pf-fidelity -- fingerprint --accept` and commit fingerprint.json.",
        );
        Ok(ExitCode::from(1))
    }
}

fn run_regress(corpus: &std::path::Path, accept: bool, runs: u32) -> Result<ExitCode> {
    let manifest = corpus_manifest(corpus)?;
    let (with, without) = regress::references(&manifest);
    if accept {
        if with.is_empty() {
            regress::write(corpus, &regress::empty())?;
            println!(
                "No sequence of the corpus has an xLights reference yet: wrote an empty {}.",
                regress::FILE
            );
            return Ok(ExitCode::SUCCESS);
        }
        let started = std::time::Instant::now();
        let baseline = regress::measure(&with, runs, |label, run| {
            eprintln!("{label}: scoring {run} of {runs}")
        })?;
        regress::write(corpus, &baseline)?;
        println!(
            "Wrote {} from {runs} scorings of {} sequences in {:.0} s.",
            corpus.join(regress::FILE).display(),
            baseline.sequences.len(),
            started.elapsed().as_secs_f64()
        );
        if let Some((spread, effect)) = regress::widest_spread(&baseline) {
            println!(
                "The widest spread of one effect's look over the scorings: {spread:.1} points ({effect})."
            );
        }
        if !without.is_empty() {
            println!("Not in it, for want of a reference: {}.", without.join(", "));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let baseline = regress::read(corpus)?;
    let expected = baseline.as_ref().map_or(0, |b| b.sequences.len());
    if with.is_empty() && expected == 0 {
        let message = format!(
            "The corpus has no xLights references yet: none of its {} sequences has its .fseq, so no \
             import was held against xLights. Render them as crates/pf-fidelity/corpus/README.md says.",
            manifest.sequences.len()
        );
        report(&format!("## The corpus next to xLights' renders\n\n{message}\n"));
        annotate("notice", "No xLights references yet", &message);
        return Ok(ExitCode::SUCCESS);
    }
    let started = std::time::Instant::now();
    let mut cards = Vec::new();
    for entry in &with {
        let card = regress::score(entry, regress::IDS)?;
        eprintln!("{}: scored", card.sequence.label);
        cards.push(card);
    }
    eprintln!("scored in {:.1} s", started.elapsed().as_secs_f64());
    let checked = regress::check(baseline.as_ref(), &cards, &without);
    let margin = baseline.as_ref().map_or(regress::MARGIN, |b| b.margin);
    report(&regress::markdown(&checked, margin));
    if checked.passed() {
        Ok(ExitCode::SUCCESS)
    } else {
        annotate(
            "error",
            "An import looks worse next to xLights",
            "See the table in the log. If the change is meant, run `cargo run --release -p pf-fidelity -- regress --accept` and commit baseline.json.",
        );
        Ok(ExitCode::from(1))
    }
}

/// One line on how a sequence scored.
fn summary(card: &Scorecard) -> String {
    let (coverage, look) = (&card.coverage, &card.look);
    // What was measured and has a look that says too little is not judged, and said so.
    let little = &look.says_little;
    let said_little = if little.effects == 0 {
        String::new()
    } else {
        format!(
            "; {} more ({:.0} s, {:.0}% of the effect-time) not judged: their looks say too little",
            little.effects,
            little.seconds,
            little.share * 100.0
        )
    };
    // DMX fixtures are compared by their channels, and said apart.
    let fixtures = card.dmx.verdict.map_or_else(String::new, |verdict| {
        format!(
            "; {} of {} DMX fixtures compared: they {}",
            card.dmx.judged,
            card.dmx.of,
            verdict.phrase(true)
        )
    });
    match look.score {
        Some(score) => format!(
            "{} (look score {score:.0}, {} of {} effects judged, {} of {} pixels measured{said_little}){fixtures}",
            headline(look),
            look.judged,
            look.of,
            coverage.pixels_measured,
            coverage.pixels
        ),
        None => format!("nothing could be judged{said_little}{fixtures}"),
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Score {
            files,
            how,
            label,
            out,
        } => {
            let request = Request {
                show: &files.show,
                xsq: &files.xsq,
                fseq: &files.fseq,
            };
            let options = Options {
                label,
                ..how.options()
            };
            let card = pf_fidelity::compare(request, options)?;
            match out {
                Some(dir) => {
                    let name = corpus::file_name(&card.sequence.label);
                    for path in corpus::write_scorecard(&dir, &name, &card)? {
                        println!("{}", path.display());
                    }
                    println!("{}: {}", card.sequence.label, summary(&card));
                }
                None => print!("{}", sequence_markdown(&card)),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Corpus { manifest, how, out } => {
            let path = manifest
                .or_else(|| std::env::var_os(MANIFEST_VARIABLE).map(PathBuf::from))
                .with_context(|| format!("name a manifest, or set {MANIFEST_VARIABLE}"))?;
            let manifest = Manifest::read(&path)?;
            if manifest.sequences.is_empty() {
                bail!("{} lists no sequences", path.display());
            }
            let run = corpus::run(&manifest, &how.options(), |label, result| match result {
                Ok(card) => println!("{label}: {}", summary(card)),
                Err(e) => println!("{label}: not compared: {e}"),
            });
            match out {
                Some(dir) => {
                    for path in corpus::write_run(&dir, &run)? {
                        println!("{}", path.display());
                    }
                }
                None => print!("{}", pf_fidelity::report::corpus_markdown(&run.card)),
            }
            Ok(if run.card.failures.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Command::Labels { scorecard } => {
            let text = std::fs::read_to_string(&scorecard)
                .with_context(|| format!("could not read {}", scorecard.display()))?;
            let rows =
                labels::read(&text).with_context(|| format!("{} isn't a scorecard", scorecard.display()))?;
            let checked = labels::check(&rows);
            print!("{}", labels::markdown(&checked));
            Ok(if checked.iter().any(|c| c.disagrees()) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Calibrate {
            seconds,
            pairs,
            frame_ms,
            seeds,
            only,
            out,
        } => {
            let settings = calibrate::Settings {
                seconds: seconds.max(1),
                pairs: pairs.max(1),
                frame_ms: frame_ms.max(1),
                seeds,
                only,
            };
            let tables = calibrate::markdown(&calibrate::run(settings, |line| eprintln!("{line}")));
            match out {
                Some(dir) => {
                    std::fs::create_dir_all(&dir)
                        .with_context(|| format!("could not create {}", dir.display()))?;
                    let path = dir.join("calibration.md");
                    std::fs::write(&path, tables)
                        .with_context(|| format!("could not write {}", path.display()))?;
                    println!("{}", path.display());
                }
                None => print!("{tables}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::CorpusFiles { dir } => {
            for (path, bytes) in madeup::files() {
                let path = dir.join(path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("could not create {}", parent.display()))?;
                }
                std::fs::write(&path, bytes)
                    .with_context(|| format!("could not write {}", path.display()))?;
                println!("{}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Fingerprint { corpus, accept } => run_fingerprint(&corpus, accept),
        Command::Regress { corpus, accept, runs } => run_regress(&corpus, accept, runs),
        Command::Strips {
            files,
            how,
            effect,
            at,
            every,
            target,
            frames,
            scale,
            levels,
            out,
        } => {
            let request = Request {
                show: &files.show,
                xsq: &files.xsq,
                fseq: &files.fseq,
            };
            let mut prepared = Prepared::open(request, how.options())?;
            let named = match &target {
                Some(name) => Some(
                    strips::find_target(prepared.show(), name)
                        .with_context(|| format!("the show has no prop, group, or submodel called {name}"))?,
                ),
                None => None,
            };
            // What to draw: (file name, target, frames).
            let mut wanted = Vec::new();
            for index in effect {
                let entry = prepared
                    .effects()
                    .get(index)
                    .with_context(|| format!("the sequence has no effect {index}"))?;
                let on = named.unwrap_or(entry.target);
                let list = strips::effect_frames(&prepared, index, frames).unwrap_or_default();
                wanted.push((format!("effect-{index}.png"), on, list));
            }
            if let Some(at) = at {
                let on = named.context("--at needs --target")?;
                let frame_ms = u64::from(prepared.sequence().frame_ms.max(1));
                let step = (every.unwrap_or(0) / frame_ms).max(1);
                let count = if every.is_some() { u64::from(frames) } else { 1 };
                let list = (0..count).map(|k| (at / frame_ms + k * step) as u32).collect();
                wanted.push((format!("at-{at}.png"), on, list));
            }
            if wanted.is_empty() {
                bail!("name what to draw with --effect or --at");
            }
            std::fs::create_dir_all(&out).with_context(|| format!("could not create {}", out.display()))?;
            for (name, on, list) in wanted {
                let image = strips::draw(&mut prepared, on, &list, scale)?;
                let path = out.join(name);
                image
                    .save(&path)
                    .with_context(|| format!("could not write {}", path.display()))?;
                println!("{} (frames {list:?})", path.display());
                if levels {
                    println!("  frame      ms | PixelFlow mean r g b, lit | xLights mean r g b, lit | error");
                    let frame_ms = u64::from(prepared.sequence().frame_ms);
                    for l in strips::levels(&mut prepared, on, &list)? {
                        let [r, g, b] = l.pixelflow;
                        let [xr, xg, xb] = l.xlights;
                        println!(
                            "  {:>5} {:>7} | {r:>5.1} {g:>5.1} {b:>5.1} {:>5} | {xr:>5.1} {xg:>5.1} {xb:>5.1} {:>5} | {:>5.1}",
                            l.frame,
                            u64::from(l.frame) * frame_ms,
                            l.lit_pixelflow,
                            l.lit_xlights,
                            l.error
                        );
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
