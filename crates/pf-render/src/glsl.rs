//! The Shader effect: a shader file run over the target's grid (see `pf_shader`).
//!
//! (The renderer's own [`crate::Shader`] is older than this effect and means something else:
//! whichever effect a frame draws. What draws a shader file is called [`Glsl`] here, after the
//! language the files are written in.)
//!
//! The shader sees the target's grid as its picture: `RENDERSIZE` is the grid's columns and rows,
//! and a pixel's `gl_FragCoord` is the middle of its cell, counted from the bottom left. Only
//! the cells that hold a pixel are worked out, a run of them at a time ([`RUN_PIXELS`]), which
//! is what makes a shader affordable on a processor: see `pf_shader`'s machine.
//!
//! **Files.** A shader's file is read and compiled once, by whoever made the [`Shaders`] (the
//! renderer never reads the disk), on a thread of its own the first time it's drawn; the effect
//! draws nothing until it's there. Exports wait for it instead ([`Shaders::waiting`]). A file
//! is compiled once for its contents, however many effects and names point at it. PixelFlow's
//! own shaders (`pixelflow:<name>`) are part of the program and need no library.
//!
//! **Limits.** A shader file is a program from a file, so what it may do is capped by counting
//! (see `pf_shader::Limits`). A frame's shader pixels share one allowance of work
//! (`Limits::frame_work`): the more of them a frame has, across all its Shader effects, the less
//! each may do, so no sequence makes a frame take longer than that allowance does. A pixel that
//! reaches its share is stopped and left clear. Before a frame is drawn a few pixels spread
//! over the grid are tried ([`Glsl::new`]); if any of them is stopped the effect draws nothing
//! at all that frame, so a shader too heavy to finish shows as nothing rather than as a picture
//! with holes. Either way the library is told ([`Shaders::stopped`]), for the effect's settings
//! to say why.
//!
//! **The same everywhere.** What a pixel comes out as depends on the document and the time
//! alone: not on the pixels it's run beside, the threads, or the system (see `pf_shader`). The
//! allowance above is worked out from the frame's plan, which is the document's.

use crate::audio::RenderContext;
use crate::color::Rgba;
use crate::effects::{Canvas, EffectTime, RUN_PIXELS, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{Curve, ShaderInputs, ShaderParams, ShaderValue};
use pf_shader::{BUILT_IN, Header, InputKind, Limits, Machine, Program, Uniforms};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

/// Reads a shader's file, given what its effect's `file` setting says: the file's text, or why
/// it can't be read (in a few plain words, to follow "can't be used: ").
pub type ReadShader = dyn Fn(&str) -> Result<String, String> + Send + Sync;

/// The most shader files kept by name, and compiled programs kept by contents. Past either,
/// everything is dropped and read again as it's drawn: a show has a handful.
const MAX_FILES: usize = 1024;
const MAX_PROGRAMS: usize = 64;
/// The most files read at once (the rest are asked for again with a later frame).
const MAX_READING: usize = 2;
/// How often a renderer that waits looks again for a shader another is reading.
const WAIT: std::time::Duration = std::time::Duration::from_millis(2);
/// The fewest pixels a buffer counts for against a frame's allowance of work. A few pixels on
/// their own cost far more each than the same pixels in a full run (the machine's work is
/// shared along a run), so a small buffer is charged as a quarter of a run.
pub(crate) const LEAST_LOAD: usize = RUN_PIXELS / 4;
/// A shader whose pixels each do at least this much work is worth sharing out among threads a
/// run at a time: a run then costs some tens of microseconds, more than handing it over does.
const SHARE_FROM_WORK: u64 = 1500;
/// The most working memory one thread keeps for the shaders it has run, in bytes.
const BENCH_BYTES: usize = 16 * 1024 * 1024;
/// How many places across and up the grid are tried before a frame is drawn.
const PROBE: usize = 4;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why a shader file can't be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The line of the file it was noticed on (0: no one line).
    pub line: u32,
    /// In plain words, to follow the file's name and "can't be used: ".
    pub why: String,
}

/// A shader file, compiled or refused.
type Compiled = Result<Arc<Program>, Refusal>;

fn compile(source: &str) -> Compiled {
    Program::compile(source, &Limits::default())
        .map(Arc::new)
        .map_err(|e| Refusal {
            line: e.line,
            why: e.message,
        })
}

/// PixelFlow's own shaders, compiled the first time one is drawn.
fn built_in(file: &str) -> Option<&'static Compiled> {
    static COMPILED: OnceLock<Vec<Compiled>> = OnceLock::new();
    let name = pf_shader::built_in(file)?.name;
    let at = BUILT_IN.iter().position(|shader| shader.name == name)?;
    COMPILED
        .get_or_init(|| BUILT_IN.iter().map(|shader| compile(shader.source)).collect())
        .get(at)
}

/// What became of a Shader effect the last time it was drawn and stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    /// When, in milliseconds from the start of the sequence.
    pub at_ms: u64,
    /// The most work each pixel could do in that frame.
    pub allowed: u32,
    /// Whether the whole effect drew nothing (else only the pixels that were stopped).
    pub whole: bool,
}

impl Stopped {
    /// Why, in plain words, for the effect's settings.
    pub fn message(&self) -> String {
        let at = pf_sequence::format_ms(self.at_ms);
        let (what, so) = match self.whole {
            true => ("it does", "so it draws nothing there"),
            false => ("it does, for some pixels,", "so those pixels stay dark"),
        };
        format!(
            "Stopped at {at}: {what} more work than a shader may in one frame, {so}. Try it on a smaller prop, or with fewer shaders playing at once."
        )
    }
}

struct State {
    /// What's known of each file, by what its effect stores.
    files: HashMap<String, Compiled>,
    /// Each file's program, by the file's contents: compiled once however it's named.
    programs: HashMap<String, Compiled>,
    reading: HashSet<String>,
    /// The Shader effects that were stopped, by the seed of each one's id.
    stopped: HashMap<u64, Stopped>,
}

struct Library {
    read: Box<ReadShader>,
    state: Mutex<State>,
    /// How many effects are in `stopped`, so a frame that has none needn't look.
    stops: AtomicUsize,
    /// Told whenever a shader read in the background is there (or couldn't be read).
    arrived: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Library {
    /// Reads and compiles `file`, and keeps what came of it. This is the slow part.
    fn fetch(&self, file: &str) -> Compiled {
        let read = (self.read)(file).map_err(|why| Refusal { line: 0, why });
        let known = match &read {
            Ok(source) => lock(&self.state).programs.get(source).cloned(),
            Err(_) => None,
        };
        let compiled = match (known, read) {
            (Some(known), _) => known,
            (None, Err(refusal)) => Err(refusal),
            (None, Ok(source)) => {
                let compiled = compile(&source);
                let mut state = lock(&self.state);
                if state.programs.len() >= MAX_PROGRAMS {
                    state.programs.clear();
                }
                state.programs.insert(source, compiled.clone());
                compiled
            }
        };
        let mut state = lock(&self.state);
        if state.files.len() >= MAX_FILES {
            state.files.clear();
        }
        state.files.insert(file.to_string(), compiled.clone());
        compiled
    }
}

/// A file being read: no one else reads it meanwhile, however the reading ends.
struct Reading {
    library: Arc<Library>,
    file: String,
}

impl Drop for Reading {
    fn drop(&mut self) {
        lock(&self.library.state).reading.remove(&self.file);
    }
}

/// The shader files Shader effects draw: none of the show's (only PixelFlow's own), or a library
/// shared by every renderer of a show.
#[derive(Clone, Default)]
pub struct Shaders {
    library: Option<Arc<Library>>,
    /// Whether a frame waits for a shader it hasn't got (exports) rather than drawing without it
    /// while it's read (previews).
    wait: bool,
}

impl std::fmt::Debug for Shaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match (&self.library, self.wait) {
            (None, _) => "Shaders(PixelFlow's own)",
            (Some(_), false) => "Shaders(shared)",
            (Some(_), true) => "Shaders(shared, waiting)",
        })
    }
}

/// Two are equal when they're the same library, read the same way ([`Shaders::same`]).
impl PartialEq for Shaders {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}

impl Shaders {
    /// No files: only PixelFlow's own shaders are drawn.
    pub fn none() -> Self {
        Self::default()
    }

    /// Shader files read with `read`, on a thread of their own the first time each is drawn.
    pub fn new(read: impl Fn(&str) -> Result<String, String> + Send + Sync + 'static) -> Self {
        Self {
            library: Some(Arc::new(Library {
                read: Box::new(read),
                state: Mutex::new(State {
                    files: HashMap::new(),
                    programs: HashMap::new(),
                    reading: HashSet::new(),
                    stopped: HashMap::new(),
                }),
                stops: AtomicUsize::new(0),
                arrived: Mutex::new(None),
            })),
            wait: false,
        }
    }

    /// The same shaders for a renderer that waits for each one it draws (an export, which must
    /// not leave any out).
    pub fn waiting(&self) -> Self {
        Self {
            library: self.library.clone(),
            wait: true,
        }
    }

    /// Whether the two are the same library, read the same way.
    pub fn same(&self, other: &Shaders) -> bool {
        self.wait == other.wait
            && match (&self.library, &other.library) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }

    /// Tells `told` whenever a shader read in the background is there or couldn't be read, so
    /// whoever shows frames can draw again (`None`: no one).
    pub fn on_arrival(&self, told: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Some(library) = &self.library {
            *lock(&library.arrived) = told;
        }
    }

    /// Forgets what's known about `file`, so it's read again the next time it's drawn: for a file
    /// that changed, appeared, or went away.
    pub fn forget(&self, file: &str) {
        if let Some(library) = &self.library {
            lock(&library.state).files.remove(file);
        }
    }

    /// Forgets every file (another show's files are other files) and every effect that was
    /// stopped.
    pub fn clear(&self) {
        if let Some(library) = &self.library {
            let mut state = lock(&library.state);
            state.files.clear();
            state.programs.clear();
            state.stopped.clear();
            library.stops.store(0, Ordering::Relaxed);
        }
    }

    /// What's known of `file` without reading anything: PixelFlow's own always, a file of the
    /// show's once it has been read.
    fn known(&self, file: &str) -> Option<Compiled> {
        if pf_shader::is_built_in_name(file) {
            return Some(built_in(file).cloned().unwrap_or_else(|| {
                Err(Refusal {
                    line: 0,
                    why: "this PixelFlow has no shader of its own by that name".into(),
                })
            }));
        }
        lock(&self.library.as_ref()?.state).files.get(file).cloned()
    }

    /// Why `file` can't be drawn, if it was tried and can't.
    pub fn problem(&self, file: &str) -> Option<Refusal> {
        self.known(file)?.err()
    }

    /// The least work each pixel of `file` does, once the file has been read (0 until then):
    /// how heavy a frame with it is, for deciding how to share the frame out.
    pub(crate) fn least_work(&self, file: &str) -> u64 {
        match self.known(file) {
            Some(Ok(program)) => program.least_work(),
            _ => 0,
        }
    }

    /// `file` read and compiled, now, on this thread (reads the disk for a file of the
    /// show's): for whoever shows what a shader is and takes, away from the frames.
    pub fn load(&self, file: &str) -> Result<Arc<Program>, Refusal> {
        if file.trim().is_empty() {
            return Err(Refusal {
                line: 0,
                why: "no shader is chosen".into(),
            });
        }
        if let Some(known) = self.known(file) {
            return known;
        }
        match &self.library {
            Some(library) => library.fetch(file),
            None => Err(Refusal {
                line: 0,
                why: "the show's shader files aren't to hand here".into(),
            }),
        }
    }

    /// `file`'s program for a frame: `None` while it's read (unless this waits), when no file is
    /// named, and when it can't be used.
    pub(crate) fn program(&self, file: &str) -> Option<Arc<Program>> {
        if file.trim().is_empty() {
            return None;
        }
        loop {
            if let Some(known) = self.known(file) {
                return known.ok();
            }
            let library = self.library.as_ref()?;
            let mut state = lock(&library.state);
            if state.files.contains_key(file) {
                continue;
            }
            let mine = !state.reading.contains(file)
                && (self.wait || state.reading.len() < MAX_READING)
                && state.reading.insert(file.to_string());
            drop(state);
            if self.wait {
                if mine {
                    let _reading = Reading {
                        library: Arc::clone(library),
                        file: file.to_string(),
                    };
                    return library.fetch(file).ok();
                }
                // Another renderer is reading it: look again shortly.
                std::thread::sleep(WAIT);
                continue;
            }
            if mine {
                let reading = Reading {
                    library: Arc::clone(library),
                    file: file.to_string(),
                };
                let spawned = std::thread::Builder::new()
                    .name("shader-file".into())
                    .spawn(move || {
                        let _ = reading.library.fetch(&reading.file);
                        let told = lock(&reading.library.arrived).clone();
                        drop(reading);
                        if let Some(told) = told {
                            told();
                        }
                    });
                // With no thread to read it on, it's asked for again with the next frame.
                drop(spawned);
            }
            return None;
        }
    }

    /// Whether the Shader effect whose id's seed is `effect` was stopped when it was last drawn,
    /// and how.
    pub fn stopped(&self, effect: u64) -> Option<Stopped> {
        let library = self.library.as_ref()?;
        if library.stops.load(Ordering::Relaxed) == 0 {
            return None;
        }
        lock(&library.state).stopped.get(&effect).cloned()
    }

    fn note_stopped(&self, effect: u64, stopped: Stopped) {
        if let Some(library) = &self.library {
            let mut state = lock(&library.state);
            // Bounded like the files: a sequence can't make this grow without end.
            if state.stopped.len() >= MAX_FILES && !state.stopped.contains_key(&effect) {
                state.stopped.clear();
            }
            state.stopped.insert(effect, stopped);
            library.stops.store(state.stopped.len(), Ordering::Relaxed);
        }
    }

    /// The effect drew a frame through to the end: it isn't stopped any more.
    fn note_drawn(&self, effect: u64) {
        if let Some(library) = &self.library
            && library.stops.load(Ordering::Relaxed) != 0
        {
            let mut state = lock(&library.state);
            state.stopped.remove(&effect);
            library.stops.store(state.stopped.len(), Ordering::Relaxed);
        }
    }
}

// ---- what a shader file is and takes, for people ------------------------------------------------

/// One choice of a shader's input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShaderChoice {
    /// The number the shader is given, which is what the effect stores.
    pub value: i32,
    pub label: String,
}

/// One input a shader file declares: what the effect's settings show a control for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShaderInputInfo {
    Number {
        name: String,
        label: String,
        min: f32,
        max: f32,
        default: f32,
    },
    Bool {
        name: String,
        label: String,
        default: bool,
    },
    Choice {
        name: String,
        label: String,
        default: i32,
        options: Vec<ShaderChoice>,
    },
    /// Across, then up.
    Point {
        name: String,
        label: String,
        min: [f32; 2],
        max: [f32; 2],
        default: [f32; 2],
    },
    /// Red, green, blue, and alpha, each 0 to 1.
    Color {
        name: String,
        label: String,
        default: [f32; 4],
    },
}

impl ShaderInputInfo {
    pub fn name(&self) -> &str {
        match self {
            ShaderInputInfo::Number { name, .. }
            | ShaderInputInfo::Bool { name, .. }
            | ShaderInputInfo::Choice { name, .. }
            | ShaderInputInfo::Point { name, .. }
            | ShaderInputInfo::Color { name, .. } => name,
        }
    }
}

/// A shader file as the effect's settings show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderInfo {
    /// What an effect's `file` setting holds for it.
    pub file: String,
    /// Its name as people see it.
    pub name: String,
    /// One of PixelFlow's own (not a file of the show's).
    pub built_in: bool,
    pub description: String,
    pub credit: String,
    pub inputs: Vec<ShaderInputInfo>,
    /// Why it can't be drawn, in plain words (none: it can).
    pub problem: Option<String>,
    /// The line of the file the problem is on, when it's on one.
    pub line: Option<u32>,
}

/// The inputs of `header` an effect can set (a button, which is only ever off here, isn't one).
fn inputs_of(header: &Header) -> Vec<ShaderInputInfo> {
    let mut inputs = Vec::with_capacity(header.inputs.len());
    for input in &header.inputs {
        let (name, label) = (input.name.clone(), input.label.clone());
        inputs.push(match &input.kind {
            InputKind::Float { min, max, default } => ShaderInputInfo::Number {
                name,
                label,
                min: *min,
                max: *max,
                default: *default,
            },
            InputKind::Bool { default } => ShaderInputInfo::Bool {
                name,
                label,
                default: *default,
            },
            InputKind::Long {
                values,
                labels,
                default,
            } => ShaderInputInfo::Choice {
                name,
                label,
                default: *default,
                options: values
                    .iter()
                    .enumerate()
                    .map(|(k, &value)| ShaderChoice {
                        value,
                        label: match labels.get(k).filter(|l| !l.is_empty()) {
                            Some(label) => label.clone(),
                            None => value.to_string(),
                        },
                    })
                    .collect(),
            },
            InputKind::Point { min, max, default } => ShaderInputInfo::Point {
                name,
                label,
                min: *min,
                max: *max,
                default: *default,
            },
            InputKind::Color { default } => ShaderInputInfo::Color {
                name,
                label,
                default: *default,
            },
            InputKind::Event | InputKind::Image | InputKind::Audio | InputKind::AudioFft => continue,
        });
    }
    inputs
}

/// Whether `file` (an effect's `file` setting) is written as one of PixelFlow's own shaders
/// is, which are part of the program and no file at all.
pub fn is_built_in_shader(file: &str) -> bool {
    pf_shader::is_built_in_name(file)
}

/// PixelFlow's own shaders, each as an effect's `file` setting names it, in the order they're
/// listed.
pub fn built_in_shaders() -> Vec<String> {
    BUILT_IN
        .iter()
        .map(|shader| format!("{}{}", pf_shader::BUILT_IN_PREFIX, shader.name))
        .collect()
}

/// A shader file's name as people see it: one of PixelFlow's own by its title, a file by its
/// name without the folder or the `.fs`.
pub fn shader_name(file: &str) -> String {
    if let Some(own) = pf_shader::built_in(file) {
        return own.title.to_string();
    }
    let name = pf_model::file_name_of(file);
    let stem = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 5 => stem,
        _ => name.as_str(),
    };
    stem.to_string()
}

impl Shaders {
    /// What `file` is and takes, read and compiled now if it hasn't been (reads the disk for a
    /// file of the show's, so not for a thread that draws frames).
    pub fn describe(&self, file: &str) -> ShaderInfo {
        let mut info = ShaderInfo {
            file: file.to_string(),
            name: shader_name(file),
            built_in: pf_shader::is_built_in_name(file),
            description: String::new(),
            credit: String::new(),
            inputs: Vec::new(),
            problem: None,
            line: None,
        };
        match self.load(file) {
            Ok(program) => {
                let header = program.header();
                info.description = header.description.clone();
                info.credit = header.credit.clone();
                info.inputs = inputs_of(header);
            }
            Err(refusal) => {
                info.line = (refusal.line > 0).then_some(refusal.line);
                info.problem = Some(match refusal.line {
                    0 => refusal.why,
                    line => format!("line {line}: {}", refusal.why),
                });
            }
        }
        info
    }
}

fn plain(n: f32) -> String {
    // Without the noise single precision leaves at the end of a number like 0.1.
    let text = format!("{:.4}", n);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// What is wrong with what an effect gives the inputs of the shader `info` describes, and with
/// the curves on them, in plain words (`None`: nothing; also when the file itself can't be used,
/// which `info.problem` says). The renderer draws regardless, with each value brought inside the
/// file's range and a name the file doesn't have ignored: this is for telling whoever set them.
pub fn shader_input_problem(
    info: &ShaderInfo,
    inputs: &ShaderInputs,
    curves: &BTreeMap<String, Curve>,
) -> Option<String> {
    if info.problem.is_some() {
        return None;
    }
    let find = |name: &str| info.inputs.iter().find(|i| i.name() == name);
    let has = || {
        let names: Vec<&str> = info.inputs.iter().map(ShaderInputInfo::name).collect();
        match names.is_empty() {
            true => format!("{} has no inputs", info.name),
            false => format!("{} has: {}", info.name, names.join(", ")),
        }
    };
    let outside = |name: &str, n: f32, min: f32, max: f32| {
        (!(min..=max).contains(&n))
            .then(|| format!("\"{name}\" is {}; use {} to {}", plain(n), plain(min), plain(max)))
    };
    for (name, value) in &inputs.0 {
        let Some(input) = find(name) else {
            return Some(format!(
                "\"{name}\" isn't one of this shader's inputs ({})",
                has()
            ));
        };
        let problem = match (input, value) {
            (ShaderInputInfo::Number { min, max, .. }, ShaderValue::Number(n)) => {
                outside(name, *n, *min, *max)
            }
            (ShaderInputInfo::Bool { .. }, ShaderValue::Bool(_)) => None,
            (ShaderInputInfo::Choice { options, .. }, ShaderValue::Number(n)) => {
                let chosen = options.iter().any(|o| o.value as f32 == *n);
                (!chosen && !options.is_empty()).then(|| {
                    let list: Vec<String> = options
                        .iter()
                        .map(|o| format!("{} ({})", o.value, o.label))
                        .collect();
                    format!("\"{name}\" is {}; use one of {}", plain(*n), list.join(", "))
                })
            }
            (ShaderInputInfo::Point { min, max, .. }, ShaderValue::Point(at)) => (0..2)
                .find_map(|k| outside(name, at[k], min[k], max[k]))
                .map(|why| format!("{why} (across, then up)")),
            (ShaderInputInfo::Color { .. }, ShaderValue::Color(_)) => None,
            (ShaderInputInfo::Number { .. }, _) => Some(format!("\"{name}\" is a number")),
            (ShaderInputInfo::Bool { .. }, _) => Some(format!("\"{name}\" is on or off: true or false")),
            (ShaderInputInfo::Choice { .. }, _) => Some(format!(
                "\"{name}\" is a choice: give the number of the one chosen"
            )),
            (ShaderInputInfo::Point { .. }, _) => Some(format!("\"{name}\" is a point: give [across, up]")),
            (ShaderInputInfo::Color { .. }, _) => Some(format!(
                "\"{name}\" is a color: give [red, green, blue, alpha], each 0 to 1"
            )),
        };
        if problem.is_some() {
            return problem;
        }
    }
    for (key, curve) in curves {
        let Some(part) = key.strip_prefix("inputs.") else {
            continue;
        };
        let (name, which) = match part.rsplit_once('.') {
            Some((name, "x")) => (name, Some(0)),
            Some((name, "y")) => (name, Some(1)),
            _ => (part, None),
        };
        let range = match (find(name), which) {
            (Some(ShaderInputInfo::Number { min, max, .. }), None) => Some((*min, *max)),
            (Some(ShaderInputInfo::Point { min, max, .. }), Some(k)) => Some((min[k], max[k])),
            (Some(ShaderInputInfo::Point { .. }), None) => {
                return Some(format!(
                    "\"{name}\" is a point: a curve changes \"{name}.x\" or \"{name}.y\""
                ));
            }
            (Some(_), _) => {
                return Some(format!(
                    "\"{name}\" isn't a number, so it can't change over the effect"
                ));
            }
            (None, _) => {
                return Some(format!(
                    "a curve changes \"{name}\", which isn't one of this shader's inputs ({})",
                    has()
                ));
            }
        };
        if let Some((min, max)) = range
            && let Some(why) = [curve.from, curve.to]
                .into_iter()
                .find_map(|n| outside(name, n, min, max))
        {
            return Some(format!("the curve on {why}"));
        }
    }
    None
}

// ---- drawing -------------------------------------------------------------------------------------

/// A shader ready to run for one frame on one buffer.
struct Run {
    program: Arc<Program>,
    uniforms: Uniforms,
    /// The most work each pixel may do this frame.
    work: u32,
    columns: i32,
    rows: i32,
    /// Where a stop is told: the library, the seed of the effect's id, and the frame's time.
    told: Option<(Shaders, u64, u64)>,
}

/// A Shader effect for one frame on one buffer.
pub struct Glsl {
    /// Nothing, when there is nothing to draw: no file, one still being read, one refused, or
    /// one stopped this frame.
    run: Option<Run>,
}

/// What a thread keeps between runs: a machine for each program it has run lately, and room for
/// a run's coordinates and colors. Nothing here outlives a change of what's drawn for long, and
/// none of it is allocated while a frame is drawn once it has been used.
#[derive(Default)]
struct Bench {
    /// Each with the address of the program it was made for (which is what tells them apart: a
    /// machine made for a program since dropped is made over when it doesn't fit).
    machines: Vec<(usize, Machine)>,
    bytes: usize,
    xs: Vec<f32>,
    ys: Vec<f32>,
    out: Vec<[f32; 4]>,
}

thread_local! {
    static BENCH: RefCell<Bench> = RefCell::new(Bench::default());
}

impl Bench {
    /// Runs `run`'s program for the pixels at (`xs`, `ys`) already here, into `out`.
    fn run(&mut self, run: &Run, pixels: usize) -> pf_shader::RunStats {
        let key = Arc::as_ptr(&run.program) as usize;
        let at = match self.machines.iter().position(|(program, _)| *program == key) {
            Some(at) => at,
            None => {
                let machine = Machine::new(&run.program, RUN_PIXELS);
                if self.bytes + machine.bytes() > BENCH_BYTES {
                    self.machines.clear();
                    self.bytes = 0;
                }
                self.bytes += machine.bytes();
                self.machines.push((key, machine));
                self.machines.len() - 1
            }
        };
        self.out.clear();
        self.out.resize(pixels, [0.0; 4]);
        self.machines[at].1.run(
            &run.program,
            &run.uniforms,
            run.work,
            &self.xs[..pixels],
            &self.ys[..pixels],
            &mut self.out,
        )
    }
}

/// A channel as the shader left it, as light: 0 to 1, and nothing for what isn't a number.
#[inline]
fn light(v: f32) -> f32 {
    if v > 0.0 { v.min(1.0) } else { 0.0 }
}

impl Glsl {
    /// Nothing drawn.
    fn nothing() -> Self {
        Self { run: None }
    }

    pub(crate) fn new(
        p: &ShaderParams,
        time: &EffectTime,
        seed: u64,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        let none = Shaders::none();
        let shaders = cx.shaders.unwrap_or(&none);
        let Some(program) = shaders.program(&p.file) else {
            return Self::nothing();
        };
        let (columns, rows) = grid_size(canvas);
        let mut uniforms = program.uniforms();
        for (name, value) in &p.inputs.0 {
            let (numbers, count) = value.numbers();
            program.set_input(&mut uniforms, name, &numbers[..count]);
        }
        program.set_size(&mut uniforms, columns as f32, rows as f32);
        // Time is counted in whole frames, as the effect's frames are, so a frame's picture is
        // the same however the clock that asked for it was rounded.
        let frame_ms = f64::from(time.frame_ms.max(1));
        let lead = (f64::from(p.lead_in) * 1000.0 / frame_ms).round().max(0.0);
        let frame = time.frame() as f64 + lead;
        let speed = f64::from(p.speed);
        let frame_s = frame_ms / 1000.0;
        program.set_time(
            &mut uniforms,
            (frame * frame_s * speed) as f32,
            (frame_s * speed) as f32,
            frame.min(f64::from(i32::MAX)) as i32,
        );
        // Each pixel's share of the frame's allowance: by the shader pixels the whole frame
        // has, or, drawn on its own, by this grid's cells.
        let load = match cx.shader_pixels {
            0 => (columns as usize * rows as usize).max(LEAST_LOAD),
            pixels => pixels,
        };
        let work = program.work_for(load);
        let told = cx
            .shaders
            .map(|shaders| (shaders.clone(), seed, time.start_ms + time.elapsed_ms));
        let run = Run {
            program,
            uniforms,
            work,
            columns,
            rows,
            told,
        };
        if run.stopped_at_once() {
            run.tell(true);
            return Self::nothing();
        }
        if let Some((shaders, effect, _)) = &run.told {
            shaders.note_drawn(*effect);
        }
        Self { run: Some(run) }
    }
}

impl Run {
    /// Whether the shader can't finish this frame: what can be told before drawing it. Every
    /// pixel does at least the program's least work, so less than that allowed stops them all;
    /// and a few pixels spread over the grid are tried, since a shader that is stopped anywhere
    /// is most often stopped nearly everywhere.
    fn stopped_at_once(&self) -> bool {
        if self.program.least_work() > u64::from(self.work) {
            return true;
        }
        BENCH.with_borrow_mut(|bench| {
            bench.xs.clear();
            bench.ys.clear();
            for j in 0..PROBE {
                for i in 0..PROBE {
                    let across = (i as f32 + 0.5) / PROBE as f32;
                    let up = (j as f32 + 0.5) / PROBE as f32;
                    bench.xs.push((across * self.columns as f32).floor() + 0.5);
                    bench.ys.push((up * self.rows as f32).floor() + 0.5);
                }
            }
            bench.run(self, PROBE * PROBE).stopped > 0
        })
    }

    fn tell(&self, whole: bool) {
        if let Some((shaders, effect, at_ms)) = &self.told {
            shaders.note_stopped(
                *effect,
                Stopped {
                    at_ms: *at_ms,
                    allowed: self.work,
                    whole,
                },
            );
        }
    }
}

impl Shade for Glsl {
    const RUNS: bool = true;

    fn shade(&self, px: &Pixel) -> Rgba {
        let mut color = [Rgba::CLEAR];
        self.shade_run(std::slice::from_ref(px), &mut color);
        color[0]
    }

    fn shade_run(&self, pixels: &[Pixel], colors: &mut [Rgba]) {
        let Some(run) = &self.run else {
            colors.fill(Rgba::CLEAR);
            return;
        };
        BENCH.with_borrow_mut(|bench| {
            bench.xs.clear();
            bench.ys.clear();
            for px in pixels {
                let (x, y) = cell_of(px, run.columns, run.rows);
                bench.xs.push(x as f32 + 0.5);
                bench.ys.push(y as f32 + 0.5);
            }
            let stats = bench.run(run, pixels.len());
            if stats.stopped > 0 {
                run.tell(false);
            }
            for (color, c) in colors.iter_mut().zip(&bench.out) {
                *color = Rgba::new(light(c[0]), light(c[1]), light(c[2]), light(c[3]));
            }
        });
    }

    fn share_from(&self) -> Option<usize> {
        let run = self.run.as_ref()?;
        (run.program.least_work() >= SHARE_FROM_WORK).then_some(RUN_PIXELS)
    }
}
