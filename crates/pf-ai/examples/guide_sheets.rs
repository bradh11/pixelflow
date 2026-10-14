//! Contact sheets for writing the Assistant's guides from what PixelFlow really draws.
//!
//! For each effect kind asked for, draws through the engine, on a made-up display (a roofline, an
//! arch, a matrix, a mega tree, and the whole house as one group: `pf_ai::stage`), a row of frames
//! for each of: the effect at its defaults; every look in its guide; and, with `--auto`, each of
//! its settings pushed low and high and each of its choices. Each row shows three moments (0.4 s,
//! 0.55 s, and 2.6 s into a 4-second effect, so two frames close together show which way things
//! move and the third how it develops) on each kind of target:
//!
//! ```text
//! | row | string ×3 (stacked)  | matrix ×3 | tree ×3 | whole house ×3 |
//! |     | arch ×3 (side by side)|           |         |                |
//! ```
//!
//! Writes `<kind>-<n>.png` (eight rows a sheet) and `<kind>.txt` (what each row is) to the folder
//! given. Nothing is written anywhere else, and nothing is read but the guides.
//!
//! ```text
//! cargo run -p pf-ai --example guide_sheets -- <out folder> [--kinds bars,chase] [--auto]
//!     [--guides crates/pf-ai/guides] [--spec rows.json] [--pictures <folder>]
//! ```
//!
//! - `--guides`: read the guides from this folder as they are on disk now (no rebuild needed
//!   after editing one); otherwise the ones compiled in.
//! - `--spec`: instead of effect kinds, draw the rows in this JSON file: a list of
//!   `{"label", "effect", "under", "target"}`, where `effect` is an effect as `place_effects` takes
//!   one, `under` (optional) another on the layer below it, and `target` (optional) the name of a
//!   prop or group of the stage to draw it on instead ("Arches", "Windows", "Candy Canes", "Tree
//!   Star", "Singing Face"...), the whole house shown. Written as `spec-<n>.png` and `spec.txt`.
//! - `--pictures`: a folder Picture effects read their files from (by the name after `images/`),
//!   besides the stage's own `images/snowman.png`.
//! - `--check`: draw nothing; read the guides (with `--guides`, from disk) and say what is wrong
//!   with them, as the tests would. With `--kinds`, only those effects' guides.
//! - `--reference`: draw nothing; print what `get_guide` adds to each effect's guide by itself
//!   (every setting's key, range, default, choices, and description).

use pf_ai::effect_spec::{self, Context};
use pf_ai::knowledge::{self, Suit, guide, reference};
use pf_ai::stage::{self, Pixel, Stage};
use pf_sequence::{Effect, EffectKind, SettingValue, Target};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// When the rows' effects start (the first chorus: loud, with words sung), and how long they last.
const START_MS: u64 = 24_000;
const LENGTH_MS: u64 = 4_000;
/// The moments drawn, from the start.
const MOMENTS_MS: [u64; 3] = [400, 550, 2_600];
/// Rows on a sheet.
const ROWS_PER_SHEET: usize = 8;

const BACKGROUND: [u8; 3] = [14, 14, 16];
const UNLIT: [u8; 3] = [44, 44, 48];
const LABEL: [u8; 3] = [200, 200, 200];

/// A picture being drawn: its size and pixels (rows from the top).
struct Sheet {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Sheet {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: BACKGROUND.repeat(width * height),
        }
    }

    fn dot(&mut self, x: i32, y: i32, size: i32, color: [u8; 3]) {
        for dy in 0..size {
            for dx in 0..size {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && py >= 0 && (px as usize) < self.width && (py as usize) < self.height {
                    let at = (py as usize * self.width + px as usize) * 3;
                    self.rgb[at..at + 3].copy_from_slice(&color);
                }
            }
        }
    }

    /// A row's number, in a 3×5 font.
    fn number(&mut self, x: i32, y: i32, n: usize) {
        const DIGITS: [[u8; 5]; 10] = [
            [7, 5, 5, 5, 7],
            [2, 6, 2, 2, 7],
            [7, 1, 7, 4, 7],
            [7, 1, 7, 1, 7],
            [5, 5, 7, 1, 1],
            [7, 4, 7, 1, 7],
            [7, 4, 7, 5, 7],
            [7, 1, 2, 2, 2],
            [7, 5, 7, 5, 7],
            [7, 5, 7, 1, 7],
        ];
        for (i, digit) in n.to_string().bytes().enumerate() {
            let glyph = DIGITS[usize::from(digit - b'0')];
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..3 {
                    if bits & (4 >> column) != 0 {
                        self.dot(x + (i as i32 * 4 + column) * 2, y + row as i32 * 2, 2, LABEL);
                    }
                }
            }
        }
    }

    /// One frame of one target in the box at (`x`, `y`): every pixel a dot where it sits, dark
    /// gray when it's off.
    fn frame(&mut self, at: (i32, i32), size: (i32, i32), pixels: &[Pixel], frame: &[u8], dot: i32) {
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in pixels {
            min = [min[0].min(p.x), min[1].min(p.y)];
            max = [max[0].max(p.x), max[1].max(p.y)];
        }
        let span = [(max[0] - min[0]).max(1e-3), (max[1] - min[1]).max(1e-3)];
        let room = [(size.0 - dot) as f32, (size.1 - dot) as f32];
        let scale = (room[0] / span[0]).min(room[1] / span[1]);
        // Centered in its box.
        let pad = [
            (room[0] - span[0] * scale) / 2.0,
            (room[1] - span[1] * scale) / 2.0,
        ];
        // Unlit pixels first, so a lit pixel behind an unlit one (round the back of a tree) shows.
        for pass in [false, true] {
            for p in pixels {
                let color = frame
                    .get(p.offset..p.offset + 3)
                    .map_or([0, 0, 0], |c| [c[0], c[1], c[2]]);
                let lit = color.iter().any(|c| *c > 10);
                if lit != pass {
                    continue;
                }
                let x = at.0 + (pad[0] + (p.x - min[0]) * scale).round() as i32;
                let y = at.1 + size.1 - dot - (pad[1] + (p.y - min[1]) * scale).round() as i32;
                self.dot(x, y, dot, if lit { color } else { UNLIT });
            }
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, stage::png(self.width as u32, self.height as u32, &self.rgb))
            .map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// One row of a sheet: what it is, and its effects (bottom layer first).
struct Row {
    label: String,
    layers: Vec<Effect>,
    /// Drawn on this prop or group's row instead of on each kind of target, the whole house
    /// shown three times across the sheet.
    target: Option<Target>,
}

/// Where each target's frames go in a row, and how big its dots are: (x, y, width, height, dot).
/// The string's three moments are stacked; the others' sit side by side.
fn boxes(suit: Suit, moment: usize) -> (i32, i32, i32, i32, i32) {
    let m = moment as i32;
    match suit {
        Suit::String => (28, m * 9, 150, 7, 2),
        Suit::Arch => (28 + m * 50, 30, 48, 50, 2),
        Suit::Matrix => (190 + m * 88, 18, 84, 44, 2),
        Suit::Tree => (460 + m * 54, 0, 50, 84, 2),
        Suit::House => (630 + m * 268, 4, 262, 78, 1),
    }
}

const ROW_HEIGHT: i32 = 92;
const SHEET_WIDTH: usize = 630 + 3 * 268 + 6;

fn timed(mut effect: Effect) -> Effect {
    effect.start_ms = START_MS;
    effect.end_ms = START_MS + LENGTH_MS;
    effect
}

fn draw(
    stage: &Stage,
    rows: &[Row],
    pictures: &pf_render::Pictures,
    out: &Path,
    name: &str,
) -> Result<(), String> {
    let times: Vec<u64> = MOMENTS_MS.iter().map(|m| START_MS + m).collect();
    let mut legend = format!(
        "{name}: each row is one effect from {START_MS} ms for {LENGTH_MS} ms of the stage's song (120 beats a minute, a chorus).\n\
         Left to right: the roofline string (three moments stacked: +{} ms, +{} ms, +{} ms) over one arch (the same three, side by side); the matrix (40x20); the mega tree (16 strings of 30, seen from the front); the whole house as one group. Unlit pixels are dark gray.\n\n",
        MOMENTS_MS[0], MOMENTS_MS[1], MOMENTS_MS[2]
    );
    for (n, chunk) in rows.chunks(ROWS_PER_SHEET).enumerate() {
        let mut sheet = Sheet::new(SHEET_WIDTH, chunk.len() * ROW_HEIGHT as usize + 6);
        for (i, row) in chunk.iter().enumerate() {
            let number = n * ROWS_PER_SHEET + i + 1;
            // The same row draws the same way on every run (random effects are seeded by id).
            let layers: Vec<Effect> = row
                .layers
                .iter()
                .enumerate()
                .map(|(layer, effect)| {
                    let mut effect = effect.clone();
                    effect.id = stage::steady_id(&format!("{name} {number} {layer}"));
                    effect
                })
                .collect();
            let top = 6 + i as i32 * ROW_HEIGHT;
            sheet.number(4, top + 36, number);
            legend.push_str(&format!("{number} (sheet {}): {}\n", n + 1, row.label));
            if let Some(target) = row.target {
                let doc = stage.with_effects_on(target, &layers);
                let frames = stage.frames(&doc, &times, pictures);
                let pixels = stage.pixels(Suit::House);
                for (m, frame) in frames.iter().enumerate() {
                    sheet.frame((28 + m as i32 * 470, top), (462, 84), &pixels, frame, 2);
                }
                continue;
            }
            for suit in Suit::ALL {
                let doc = stage.with_effects(suit, &layers);
                let frames = stage.frames(&doc, &times, pictures);
                let pixels = stage.pixels(suit);
                for (m, frame) in frames.iter().enumerate() {
                    let (x, y, w, h, dot) = boxes(suit, m);
                    sheet.frame((x, top + y), (w, h), &pixels, frame, dot);
                }
            }
        }
        sheet.save(&out.join(format!("{name}-{}.png", n + 1)))?;
    }
    std::fs::write(out.join(format!("{name}.txt")), legend).map_err(|e| e.to_string())
}

/// An effect from a description, on the stage's timing tracks and pictures.
fn effect_from(stage: &Stage, spec: &Value, more_pictures: &[String]) -> Result<Effect, String> {
    let doc = stage.sequence();
    let mut pictures = vec![stage::PICTURE.to_string()];
    pictures.extend(more_pictures.iter().cloned());
    effect_spec::parse(
        spec,
        &Context {
            tracks: &doc.timing_tracks,
            pictures: Some(&pictures),
        },
    )
    .map(timed)
}

/// A setting pushed low and high, each of its choices, or switched: (what changed, its value).
fn auto_variants(kind: EffectKind) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for setting in reference::info(kind).settings {
        let key = setting.key;
        match setting.value {
            SettingValue::Number {
                min, max, default, ..
            } => {
                let (low, high) = if default > 0.0 && default > min && default < max {
                    ((default * 0.25).max(min), (default * 4.0).min(max))
                } else {
                    (min + (max - min) * 0.2, min + (max - min) * 0.8)
                };
                out.push((format!("{key} = {low}"), json!({ key: low })));
                out.push((format!("{key} = {high}"), json!({ key: high })));
            }
            SettingValue::Int {
                min, max, default, ..
            } => {
                let low = (default / 3).clamp(min, max);
                let high = default.saturating_mul(3).clamp(min, max);
                for v in [low, high] {
                    if v != default {
                        out.push((format!("{key} = {v}"), json!({ key: v })));
                    }
                }
            }
            SettingValue::Bool { default } => {
                out.push((format!("{key} = {}", !default), json!({ key: !default })));
            }
            SettingValue::Choice { default, options } => {
                for option in options.iter().filter(|o| o.value != default) {
                    out.push((format!("{key} = {}", option.value), json!({ key: option.value })));
                }
            }
            SettingValue::TimingTrack { .. } => {
                out.push((format!("{key} = Beats"), json!({ key: "Beats" })));
            }
            SettingValue::Image { .. } => {
                out.push((
                    format!("{key} = {}", stage::PICTURE),
                    json!({ key: stage::PICTURE }),
                ));
            }
            // Each of PixelFlow's own shaders, as it is.
            SettingValue::ShaderFile { .. } => {
                for file in pf_render::built_in_shaders() {
                    out.push((format!("{key} = {file}"), json!({ key: file })));
                }
            }
            SettingValue::Face { .. }
            | SettingValue::Text { .. }
            | SettingValue::ShaderInputs { .. }
            | SettingValue::DmxChannels { .. } => {}
        }
    }
    out
}

/// What an effect needs before any of its settings shows anything: words for Text, a picture
/// for Picture, the sung words for the two that sing.
fn needed(kind: EffectKind) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for setting in reference::info(kind).settings {
        let value = match (&setting.value, kind) {
            (SettingValue::Text { .. }, EffectKind::Text) if setting.key == "text" => json!("HELLO"),
            (SettingValue::Image { .. }, _) => json!(stage::PICTURE),
            (SettingValue::TimingTrack { .. }, EffectKind::Faces) => json!("Lyrics (phonemes)"),
            (SettingValue::TimingTrack { .. }, EffectKind::Sing) => json!("Lyrics (words)"),
            _ => continue,
        };
        out.insert(setting.key.to_string(), value);
    }
    out
}

/// The written guide for `kind`: from `folder` when given, else the one compiled in.
fn guide_for(kind: EffectKind, folder: Option<&Path>) -> Result<Option<knowledge::Guide>, String> {
    let key = reference::key_of(&kind);
    match folder {
        Some(folder) => {
            let path = folder.join("effects").join(format!("{key}.md"));
            match std::fs::read_to_string(&path) {
                Ok(source) => guide::parse(&source)
                    .map(Some)
                    .map_err(|e| format!("{}: {e}", path.display())),
                Err(_) => Ok(None),
            }
        }
        None => Ok(knowledge::effect_guide(kind)),
    }
}

struct Options {
    out: PathBuf,
    kinds: Vec<EffectKind>,
    auto: bool,
    guides: Option<PathBuf>,
    spec: Option<PathBuf>,
    pictures: Option<PathBuf>,
    check: bool,
    reference: bool,
    /// Whether `--kinds` was given (a check then looks at those kinds only).
    chosen: bool,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        out: PathBuf::new(),
        kinds: EffectKind::ALL.to_vec(),
        auto: false,
        guides: None,
        spec: None,
        pictures: None,
        check: false,
        reference: false,
        chosen: false,
    };
    let mut out = None;
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--auto" => options.auto = true,
            "--check" => options.check = true,
            "--reference" => options.reference = true,
            "--guides" => options.guides = Some(PathBuf::from(value("a folder")?)),
            "--spec" => options.spec = Some(PathBuf::from(value("a file")?)),
            "--pictures" => options.pictures = Some(PathBuf::from(value("a folder")?)),
            "--kinds" => {
                options.chosen = true;
                options.kinds = value("effect kinds")?
                    .split(',')
                    .map(|k| reference::kind_named(k.trim()).ok_or(format!("there's no effect kind \"{k}\"")))
                    .collect::<Result<_, _>>()?;
            }
            other if out.is_none() && !other.starts_with("--") => out = Some(PathBuf::from(other)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    if !options.check && !options.reference {
        options.out = out.ok_or("usage: guide_sheets <out folder> [--kinds a,b] [--auto] [--guides <folder>] [--spec rows.json] [--pictures <folder>] | --check [--kinds a,b] [--guides <folder>] | --reference [--kinds a,b]")?;
    }
    Ok(options)
}

fn run() -> Result<(), String> {
    let options = options()?;
    let stage = Stage::new();
    if options.reference {
        for kind in &options.kinds {
            println!("{}", reference::effect(*kind));
        }
        return Ok(());
    }
    if options.check {
        let kinds = options.chosen.then_some(&options.kinds[..]);
        let problems = knowledge::check::problems(options.guides.as_deref(), kinds);
        for problem in &problems {
            println!("{problem}\n");
        }
        return if problems.is_empty() {
            println!("The guides are complete and every look draws.");
            Ok(())
        } else {
            Err(format!("{} to fix.", problems.len()))
        };
    }
    std::fs::create_dir_all(&options.out).map_err(|e| e.to_string())?;
    // Pictures: the stage's own, and any in the folder given (named `images/<file>`).
    let folder = options.pictures.clone();
    let more: Vec<String> = folder
        .as_deref()
        .and_then(|f| std::fs::read_dir(f).ok())
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| format!("images/{}", e.file_name().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    let pictures = pf_render::Pictures::new(move |file| {
        if file == stage::PICTURE {
            return Ok(stage::snowman());
        }
        let folder = folder.as_deref().ok_or("no pictures folder was given")?;
        std::fs::read(folder.join(file.trim_start_matches("images/"))).map_err(|e| e.to_string())
    });
    if let Some(spec) = &options.spec {
        let text = std::fs::read_to_string(spec).map_err(|e| format!("{}: {e}", spec.display()))?;
        let given: Vec<Value> =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", spec.display()))?;
        let mut rows = Vec::new();
        for (i, row) in given.iter().enumerate() {
            let what = |e: String| format!("row {}: {e}", i + 1);
            let mut layers = Vec::new();
            if !row["under"].is_null() {
                layers.push(effect_from(&stage, &row["under"], &more).map_err(what)?);
            }
            layers.push(effect_from(&stage, &row["effect"], &more).map_err(what)?);
            let label = row["label"].as_str().unwrap_or_default();
            let target = match row["target"].as_str() {
                None => None,
                Some(name) => Some(stage.named(name).ok_or(format!(
                    "row {}: the stage has no prop or group called \"{name}\"",
                    i + 1
                ))?),
            };
            let on = row["target"]
                .as_str()
                .map_or(String::new(), |t| format!(", on {t} (the whole house shown)"));
            rows.push(Row {
                label: format!("{label}: {} over {}{on}", row["effect"], row["under"]),
                layers,
                target,
            });
        }
        return draw(&stage, &rows, &pictures, &options.out, "spec");
    }
    for kind in options.kinds {
        let key = reference::key_of(&kind);
        let mut rows = vec![Row {
            label: "defaults (white)".into(),
            layers: vec![timed(Effect::new(kind, 0, 1))],
            target: None,
        }];
        if let Some(guide) = guide_for(kind, options.guides.as_deref())? {
            for look in &guide.looks {
                let suits: Vec<&str> = look.suits.iter().map(|s| s.word()).collect();
                let effect = effect_from(&stage, &look.effect, &more)
                    .map_err(|e| format!("{key}: the look \"{}\": {e}", look.name))?;
                rows.push(Row {
                    label: format!(
                        "look \"{}\" (written for {}): {}",
                        look.name,
                        suits.join(", "),
                        look.effect
                    ),
                    layers: vec![effect],
                    target: None,
                });
            }
        }
        if options.auto {
            let needed = needed(kind);
            let mut variants = auto_variants(kind);
            if !needed.is_empty() {
                variants.insert(0, ("only what it needs to show anything".to_string(), json!({})));
            }
            for (what, changed) in variants {
                let mut settings = needed.clone();
                if let Some(changed) = changed.as_object() {
                    settings.extend(changed.clone());
                }
                let spec =
                    json!({ "kind": key, "settings": settings, "colors": ["#ff2020", "#20ff40", "#3060ff"] });
                let rest = if needed.is_empty() {
                    "the rest default".to_string()
                } else {
                    format!("with {}, the rest default", Value::Object(needed.clone()))
                };
                match effect_from(&stage, &spec, &more) {
                    Ok(effect) => rows.push(Row {
                        label: format!("{what} (red, green, blue; {rest})"),
                        layers: vec![effect],
                        target: None,
                    }),
                    Err(e) => eprintln!("{key}: {what}: {e}"),
                }
            }
        }
        draw(&stage, &rows, &pictures, &options.out, &key)?;
        println!("{key}: {} rows", rows.len());
    }
    Ok(())
}

fn main() {
    if let Err(problem) = run() {
        eprintln!("{problem}");
        std::process::exit(1);
    }
}
