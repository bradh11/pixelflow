//! Spike: an effect drawn by a shader file (see `pf-shader`).
//!
//! Nothing in a sequence says an effect is a shader yet. A table set from outside
//! ([`set_shader_effects`]) names the effects drawn this way, by the seed of their ids, and the
//! renderer draws those with the shader in place of whatever their settings say. It is here to
//! measure the idea on real sequences, not to be kept.
//!
//! The shader sees the target's grid as its picture: `RENDERSIZE` is the grid's columns and rows,
//! and a pixel's `gl_FragCoord` is the middle of its cell, counted from the bottom left. Only
//! the cells that hold a pixel are worked out, a run of them at a time.

use crate::color::Rgba;
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_shader::{Machine, Program, Uniforms};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// A shader and the settings one effect runs it with.
#[derive(Debug, Clone)]
pub struct ShaderSetup {
    pub program: Arc<Program>,
    /// The inputs the effect sets, by name (see `Program::set_input`).
    pub inputs: Vec<(String, Vec<f32>)>,
    /// How fast the shader's time runs: 1 is a second a second.
    pub speed: f32,
    /// Frames the shader is already into when the effect starts.
    pub lead_in_frames: u32,
}

static TABLE: OnceLock<RwLock<HashMap<u64, Arc<ShaderSetup>>>> = OnceLock::new();
/// Pixels run at a time. Measured on real shaders: one at a time costs 25 to 45 times what 64
/// do, and 256 about half what 64 do; past that there is little in it.
pub const USUAL_BATCH: usize = 256;
static BATCH: AtomicUsize = AtomicUsize::new(USUAL_BATCH);
static PLATFORM_MATH: AtomicBool = AtomicBool::new(false);
/// Pixels shaded, the instructions run for them, and pixels stopped at the work limit.
static PIXELS: AtomicU64 = AtomicU64::new(0);
static WORK: AtomicU64 = AtomicU64::new(0);
static STOPPED: AtomicU64 = AtomicU64::new(0);

fn table() -> &'static RwLock<HashMap<u64, Arc<ShaderSetup>>> {
    TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// The effects drawn by a shader from here on, by the seed of each one's id.
pub fn set_shader_effects(effects: HashMap<u64, Arc<ShaderSetup>>) {
    *table().write().unwrap_or_else(|e| e.into_inner()) = effects;
}

/// How many pixels a shader is run for at a time (for measuring).
pub fn set_shader_batch(pixels: usize) {
    BATCH.store(pixels.max(1), Ordering::Relaxed);
}

/// Whether shaders ask the platform's math library for `sin` and the like (for measuring).
pub fn set_shader_platform_math(platform: bool) {
    PLATFORM_MATH.store(platform, Ordering::Relaxed);
}

/// Pixels shaded by shaders since the last call, the instructions run for them, and how many
/// were stopped at the work limit.
pub fn take_shader_counts() -> (u64, u64, u64) {
    (
        PIXELS.swap(0, Ordering::Relaxed),
        WORK.swap(0, Ordering::Relaxed),
        STOPPED.swap(0, Ordering::Relaxed),
    )
}

pub(crate) fn shader_for(seed: u64) -> Option<Arc<ShaderSetup>> {
    table()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&seed)
        .cloned()
}

/// A shader effect for one frame on one buffer.
pub struct Glsl {
    setup: Arc<ShaderSetup>,
    uniforms: Uniforms,
    columns: i32,
    rows: i32,
}

/// What a thread keeps between runs: a machine for each program it has run, and room to work.
#[derive(Default)]
struct Bench {
    machines: Vec<(usize, usize, Machine)>,
    xs: Vec<f32>,
    ys: Vec<f32>,
    out: Vec<[f32; 4]>,
}

thread_local! {
    static BENCH: RefCell<Bench> = RefCell::new(Bench::default());
}

impl Glsl {
    pub(crate) fn new(setup: Arc<ShaderSetup>, time: &EffectTime, canvas: Canvas) -> Self {
        let (columns, rows) = grid_size(canvas);
        if time.elapsed_ms == 0 && std::env::var_os("PF_SHADER_DEBUG").is_some() {
            eprintln!(
                "shader effect at {} ms for {} ms on a grid of {columns} by {rows}",
                time.start_ms, time.length_ms
            );
        }
        let program = &setup.program;
        let mut uniforms = program.uniforms();
        for (name, value) in &setup.inputs {
            program.set_input(&mut uniforms, name, value);
        }
        program.set_size(&mut uniforms, columns as f32, rows as f32);
        // Time is counted in whole frames, as the effect's frames are.
        let frame_s = f64::from(time.frame_ms.max(1)) / 1000.0;
        let frame = time.frame() + u64::from(setup.lead_in_frames);
        let speed = if setup.speed.is_finite() {
            f64::from(setup.speed)
        } else {
            1.0
        };
        let seconds = frame as f64 * frame_s * speed;
        program.set_time(
            &mut uniforms,
            seconds as f32,
            (frame_s * speed) as f32,
            i32::try_from(frame).unwrap_or(i32::MAX),
        );
        Self {
            setup,
            uniforms,
            columns,
            rows,
        }
    }
}

/// A channel as the shader left it, as light: 0 to 1, and nothing for what isn't a number.
#[inline]
fn light(v: f32) -> f32 {
    if v > 0.0 { v.min(1.0) } else { 0.0 }
}

impl Shade for Glsl {
    const BATCH: bool = true;

    fn shade(&self, px: &Pixel) -> Rgba {
        let mut out = Vec::with_capacity(1);
        self.shade_run(std::slice::from_ref(px), &mut out);
        out.first().copied().unwrap_or(Rgba::CLEAR)
    }

    fn shade_run(&self, pixels: &[Pixel], colors: &mut Vec<Rgba>) {
        colors.clear();
        let program = &*self.setup.program;
        let width = BATCH.load(Ordering::Relaxed);
        BENCH.with_borrow_mut(|bench| {
            let Bench {
                machines,
                xs,
                ys,
                out,
            } = bench;
            xs.clear();
            ys.clear();
            for px in pixels {
                let (x, y) = cell_of(px, self.columns, self.rows);
                xs.push(x as f32 + 0.5);
                ys.push(y as f32 + 0.5);
            }
            out.clear();
            out.resize(pixels.len(), [0.0; 4]);
            let key = (Arc::as_ptr(&self.setup.program) as usize, width);
            let at = match machines.iter().position(|(p, w, _)| (*p, *w) == key) {
                Some(at) => at,
                None => {
                    // A thread keeps the machines of the few shaders a frame has, no more.
                    if machines.len() >= 16 {
                        machines.clear();
                    }
                    machines.push((key.0, key.1, Machine::new(program, width)));
                    machines.len() - 1
                }
            };
            let machine = &mut machines[at].2;
            machine.set_platform_math(PLATFORM_MATH.load(Ordering::Relaxed));
            let stats = machine.run(program, &self.uniforms, xs, ys, out);
            PIXELS.fetch_add(pixels.len() as u64, Ordering::Relaxed);
            WORK.fetch_add(stats.work, Ordering::Relaxed);
            STOPPED.fetch_add(stats.stopped, Ordering::Relaxed);
            colors.extend(
                out.iter()
                    .map(|c| Rgba::new(light(c[0]), light(c[1]), light(c[2]), light(c[3]))),
            );
        });
    }
}
