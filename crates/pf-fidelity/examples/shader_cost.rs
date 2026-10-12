//! Spike: what an xLights sequence's Shader effects cost to draw, and whether they come out the
//! same bytes however they are drawn.
//!
//! ```sh
//! PF_SHADER_DIR=path/to/shaders cargo run --release -p pf-fidelity --example shader_cost -- \
//!     path/to/show path/to/song.xsq
//! ```
//!
//! For one effect of each shader on each target it is used on, with nothing else in the
//! sequence: the time a frame takes on one thread and on several, less the time the same frame
//! takes with a plain fill in the shader's place; the instructions run per pixel; and whether
//! the frames are identical across threads, tile sizes, and batch sizes. Then the difference the
//! platform's math library makes, and the whole sequence with and without its shaders.
//! Shaders are named by letter, in the order the sequence first uses them.

use pf_fidelity::layout::comparison_show;
use pf_render::glsl::{set_shader_batch, set_shader_effects, set_shader_platform_math, take_shader_counts};
use pf_render::{RenderPool, Renderer, SceneGeometry};
use pf_sequence::{EffectId, Sequence, Target};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Frames timed for each effect.
const SAMPLES: u64 = 48;
/// Times each set of frames is drawn; the quickest is kept.
const PASSES: usize = 5;

struct Subject {
    letter: char,
    effect: EffectId,
    target: Target,
    start_ms: u64,
    end_ms: u64,
}

/// `seq` with only the effect `keep`.
fn only(seq: &Sequence, keep: EffectId) -> Sequence {
    let mut alone = seq.clone();
    for row in &mut alone.rows {
        for layer in &mut row.layers {
            layer.effects.retain(|e| e.id == keep);
        }
    }
    alone
}

/// The frames an effect is timed on: spread through it, clear of its ends.
fn frames(subject: &Subject, frame_ms: u64) -> Vec<u64> {
    let (first, last) = (
        (subject.start_ms + 1000) / frame_ms,
        subject.end_ms.saturating_sub(1000) / frame_ms,
    );
    (0..SAMPLES)
        .map(|k| first + (last - first) * k / (SAMPLES - 1))
        .collect()
}

/// Draws `frames` of `seq` and gives the bytes of every frame, one after another, and the
/// quickest time a pass over all of them took.
fn draw(renderer: &mut Renderer, seq: &Sequence, frames: &[u64]) -> (Vec<u8>, Duration) {
    let mut frame = vec![0u8; renderer.frame_len()];
    let mut all = Vec::with_capacity(frame.len() * frames.len());
    let mut quickest = Duration::MAX;
    for pass in 0..=PASSES {
        let started = Instant::now();
        for &index in frames {
            renderer.render_frame(seq, index, &mut frame);
            if pass == 0 {
                all.extend_from_slice(&frame);
            }
        }
        // The first pass lays buffers out and warms the machine: not timed.
        if pass > 0 {
            quickest = quickest.min(started.elapsed());
        }
    }
    (all, quickest)
}

fn differing(a: &[u8], b: &[u8]) -> (usize, u8) {
    let mut count = 0;
    let mut most = 0;
    for (&x, &y) in a.iter().zip(b) {
        if x != y {
            count += 1;
            most = most.max(x.abs_diff(y));
        }
    }
    (count, most)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, show, xsq] = args.as_slice() else {
        anyhow::bail!("usage: shader_cost <show folder> <sequence.xsq>");
    };
    let comparison = comparison_show(Path::new(show))?;
    let import = pf_xlights::import_sequence_file(Path::new(xsq), &comparison.show, |_, _| None)?;
    let seq = &import.sequence;
    let frame_ms = u64::from(seq.frame_ms.max(1));
    let mut notes = Vec::new();
    pf_fidelity::shaders::install(&import, &mut notes);
    for note in &notes {
        println!("{note}");
    }
    let geometry = SceneGeometry::new(&comparison.show, &comparison.map);

    // One effect of each shader on each target.
    let mut files: Vec<&str> = Vec::new();
    let mut subjects: Vec<Subject> = Vec::new();
    for imported in import.effects.iter().filter(|e| !e.shader_settings.is_empty()) {
        let file = imported
            .shader_settings
            .iter()
            .find(|(k, _)| k == "E_0FILEPICKERCTRL_IFS")
            .map_or("", |(_, v)| v.rsplit(['/', '\\']).next().unwrap_or(v));
        let at = files.iter().position(|f| *f == file).unwrap_or_else(|| {
            files.push(file);
            files.len() - 1
        });
        let letter = char::from(b'A' + at as u8);
        for row in &seq.rows {
            for effect in row.layers.iter().flat_map(|l| &l.effects) {
                let new = !subjects
                    .iter()
                    .any(|s| s.letter == letter && s.target == row.target);
                if effect.id == imported.id && new {
                    subjects.push(Subject {
                        letter,
                        effect: effect.id,
                        target: row.target,
                        start_ms: effect.start_ms,
                        end_ms: effect.end_ms,
                    });
                }
            }
        }
    }

    let mut targets: Vec<Target> = Vec::new();
    println!("\n== one effect alone: time a frame, less a plain fill in its place ==");
    for subject in &subjects {
        let buffer = geometry.buffer(subject.target);
        let pixels = buffer.pixels.len();
        let number = targets
            .iter()
            .position(|t| *t == subject.target)
            .unwrap_or_else(|| {
                targets.push(subject.target);
                targets.len() - 1
            })
            + 1;
        let alone = only(seq, subject.effect);
        let frames = frames(subject, frame_ms);
        println!(
            "\nshader {} on target {number}: {pixels} pixels on a grid of {} by {} ({} cells), {} frames timed",
            subject.letter,
            buffer.columns,
            buffer.rows,
            u64::from(buffer.columns) * u64::from(buffer.rows),
            frames.len()
        );
        let per_frame = |d: Duration| d.as_secs_f64() * 1e6 / frames.len() as f64;
        let mut reference: Option<Vec<u8>> = None;
        let mut one_thread_us = 0.0;
        for threads in [1usize, 2, 4, 8] {
            let mut renderer = Renderer::new(&comparison.show, &comparison.map);
            renderer.set_pool(if threads == 1 {
                None
            } else {
                RenderPool::new(threads)
            });
            // The plain fill the import puts in the shader's place.
            set_shader_effects(HashMap::new());
            let (_, fill) = draw(&mut renderer, &alone, &frames);
            pf_fidelity::shaders::install(&import, &mut Vec::new());
            take_shader_counts();
            let (bytes, shaded) = draw(&mut renderer, &alone, &frames);
            let (shaded_pixels, work, stopped) = take_shader_counts();
            let own = per_frame(shaded) - per_frame(fill);
            if threads == 1 {
                one_thread_us = own;
            }
            let same = match &reference {
                None => {
                    reference = Some(bytes);
                    "the reference".to_string()
                }
                Some(first) => {
                    let (count, most) = differing(first, &bytes);
                    if count == 0 {
                        "identical bytes".to_string()
                    } else {
                        format!("{count} BYTES DIFFER (by up to {most})")
                    }
                }
            };
            println!(
                "  {threads} thread(s): {:8.1} us a frame with the shader, {:6.1} with a fill; the shader's own {:8.1} us a frame, {:6.1} ns a pixel; {:.0} instructions a pixel, {stopped} stopped; {same}",
                per_frame(shaded),
                per_frame(fill),
                own,
                own * 1000.0 / pixels as f64,
                work as f64 / shaded_pixels.max(1) as f64,
            );
        }
        let reference = reference.unwrap_or_default();
        // The same frames with the show cut into other tiles, and other batch sizes.
        for tile in [64usize, 128, 256, 512, 100_000] {
            let mut renderer = Renderer::new(&comparison.show, &comparison.map);
            renderer.set_pool(RenderPool::new(8));
            renderer.set_tile_pixels(tile);
            let (bytes, shaded) = draw(&mut renderer, &alone, &frames);
            let (count, _) = differing(&reference, &bytes);
            println!(
                "  tiles of {tile} pixels on 8 threads: {:8.1} us a frame; {count} bytes differ",
                per_frame(shaded)
            );
        }
        for batch in [1usize, 4, 16, 64, 256, 1024] {
            set_shader_batch(batch);
            let mut renderer = Renderer::new(&comparison.show, &comparison.map);
            renderer.set_pool(None);
            set_shader_effects(HashMap::new());
            let (_, fill) = draw(&mut renderer, &alone, &frames);
            pf_fidelity::shaders::install(&import, &mut Vec::new());
            set_shader_batch(batch);
            let (bytes, shaded) = draw(&mut renderer, &alone, &frames);
            let (count, _) = differing(&reference, &bytes);
            let own = per_frame(shaded) - per_frame(fill);
            println!(
                "  {batch:4} pixels at a time, 1 thread: {:8.1} us a frame, {:7.1} ns a pixel ({:.2}x the usual); {count} bytes differ",
                own,
                own * 1000.0 / pixels as f64,
                own / one_thread_us,
            );
        }
        set_shader_batch(pf_render::glsl::USUAL_BATCH);
        // What the platform's math library would change.
        let mut renderer = Renderer::new(&comparison.show, &comparison.map);
        renderer.set_pool(None);
        set_shader_platform_math(true);
        let (bytes, platform) = draw(&mut renderer, &alone, &frames);
        set_shader_platform_math(false);
        let (_, ours) = draw(&mut renderer, &alone, &frames);
        let (count, most) = differing(&reference, &bytes);
        let lit = reference.iter().filter(|&&b| b != 0).count();
        println!(
            "  the platform's sin, cos, exp, pow and the like in place of PixelFlow's own: {count} of {} bytes differ ({lit} lit), by up to {most}; {:.1} us a frame against {:.1}",
            reference.len(),
            per_frame(platform),
            per_frame(ours),
        );
    }

    println!("\n== the whole sequence, every frame, as an export draws it ==");
    let total = seq.whole_frame_count();
    let all: Vec<u64> = (0..total).collect();
    let mut frame_bytes: Option<Vec<u8>> = None;
    for threads in [1usize, 4, 8] {
        let mut renderer = Renderer::new(&comparison.show, &comparison.map);
        renderer.set_pool(if threads == 1 {
            None
        } else {
            RenderPool::new(threads)
        });
        let mut frame = vec![0u8; renderer.frame_len()];
        let mut time = |renderer: &mut Renderer, keep: bool| -> (Duration, Duration, Vec<u8>) {
            let mut kept = Vec::new();
            let mut slowest = Duration::ZERO;
            let started = Instant::now();
            for &index in &all {
                let one = Instant::now();
                renderer.render_frame(seq, index, &mut frame);
                slowest = slowest.max(one.elapsed());
                if keep {
                    kept.extend_from_slice(&frame);
                }
            }
            (started.elapsed(), slowest, kept)
        };
        set_shader_effects(HashMap::new());
        let (without, _, _) = time(&mut renderer, false);
        pf_fidelity::shaders::install(&import, &mut Vec::new());
        take_shader_counts();
        let (with, slowest, bytes) = time(&mut renderer, true);
        let (pixels, work, stopped) = take_shader_counts();
        let same = match &frame_bytes {
            None => {
                frame_bytes = Some(bytes);
                "the reference".to_string()
            }
            Some(first) => {
                let (count, _) = differing(first, &bytes);
                format!("{count} bytes differ from 1 thread")
            }
        };
        println!(
            "  {threads} thread(s): {total} frames in {:.2} s with shaders, {:.2} s with fills in their place ({:.2} s more); slowest frame {:.2} ms; {pixels} shader pixels, {:.0} instructions each, {stopped} stopped; {same}",
            with.as_secs_f64(),
            without.as_secs_f64(),
            with.as_secs_f64() - without.as_secs_f64(),
            slowest.as_secs_f64() * 1000.0,
            work as f64 / pixels.max(1) as f64,
        );
    }
    Ok(())
}
