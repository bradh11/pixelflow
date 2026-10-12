//! What Shader effects cost to draw, and whether they come out the same bytes on any number of
//! threads.
//!
//! ```sh
//! # PixelFlow's own shaders, with two built-in effects beside them, on props of a show (a
//! # PixelFlow show file, or an xLights show folder):
//! cargo run --release -p pf-fidelity --example shader_cost -- \
//!     examples/shows/demo.pixelflow.json --on "Mega Tree" --on "Window Matrix"
//! # An xLights sequence's Shader effects, each on the target it is on:
//! cargo run --release -p pf-fidelity --example shader_cost -- path/to/show path/to/song.xsq
//! ```
//!
//! For each effect, with nothing else in the sequence: the time a frame takes on one thread and
//! on several, less the time the same frame takes with a plain fill in its place; the work
//! done for each pixel against what a pixel may do in that frame; and whether the frames are
//! identical across threads. A sequence's shaders are named by letter in the order it first uses
//! them, and its targets by number, so nothing of the sequence's own is printed.
//! `docs/benchmarks.md` quotes the results.

use pf_fidelity::layout::comparison_show;
use pf_mapping::ChannelMap;
use pf_model::Show;
use pf_render::{
    Canvas, EffectTime, Pictures, RenderContext, RenderPool, Renderer, SceneGeometry, Shaders,
    shader_frame_cost, shader_name,
};
use pf_sequence::{Effect, EffectKind, EffectParams, Layer, Row, Sequence, ShaderParams, Target};
use std::path::Path;
use std::time::{Duration, Instant};

/// Frames timed for each effect.
const SAMPLES: u64 = 48;
/// Times each set of frames is drawn; the quickest is kept.
const PASSES: usize = 5;

struct Subject {
    name: String,
    on: String,
    target: Target,
    effect: Effect,
}

/// The show at `path`, with where its channels go: an xLights show folder, or a PixelFlow show
/// file.
fn open_show(path: &Path) -> anyhow::Result<(Show, ChannelMap)> {
    if path.is_dir() {
        let comparison = comparison_show(path)?;
        return Ok((comparison.show, comparison.map));
    }
    let show = pf_model::show_from_json(&std::fs::read_to_string(path)?)?;
    let map = pf_mapping::map_show(&show).0;
    Ok((show, map))
}

/// Where `item` is in `list`, added to its end if it isn't there.
fn place<T: PartialEq>(list: &mut Vec<T>, item: T) -> usize {
    match list.iter().position(|i| *i == item) {
        Some(at) => at,
        None => {
            list.push(item);
            list.len() - 1
        }
    }
}

/// A sequence of `effect` alone on `target`.
fn alone(target: Target, effect: &Effect, frame_ms: u32) -> Sequence {
    let mut seq = Sequence::new("cost", effect.end_ms.max(1000));
    seq.frame_ms = frame_ms;
    let mut row = Row::new(target);
    row.layers = vec![Layer {
        effects: vec![effect.clone()],
    }];
    seq.rows.push(row);
    seq
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
        // The first pass lays buffers out and warms the machines: not timed.
        if pass > 0 {
            quickest = quickest.min(started.elapsed());
        }
    }
    (all, quickest)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut plain = Vec::new();
    let mut on = Vec::new();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--on" => on.extend(words.next().cloned()),
            _ => plain.push(word.as_str()),
        }
    }
    let Some(show_dir) = plain.first() else {
        anyhow::bail!("usage: shader_cost <show folder or file> [<sequence.xsq>] [--on <target>]...");
    };
    let (show, map) = open_show(Path::new(show_dir))?;
    let show = &show;
    let geometry = SceneGeometry::new(show, &map);
    let pictures = Pictures::none()
        .with_shaders(Shaders::new(|file| pf_shader::read_shader_file(Path::new(file))))
        .waiting();
    let mut frame_ms = 25;
    let mut subjects: Vec<Subject> = Vec::new();
    match plain.get(1) {
        Some(xsq) => {
            let import = pf_xlights::import_sequence_file(Path::new(xsq), show, |_, _| None)?;
            frame_ms = import.sequence.frame_ms;
            let (mut files, mut targets): (Vec<String>, Vec<Target>) = (Vec::new(), Vec::new());
            for row in &import.sequence.rows {
                for effect in row.layers.iter().flat_map(|l| &l.effects) {
                    let EffectParams::Shader(p) = &effect.params else {
                        continue;
                    };
                    let letter = char::from(b'A' + place(&mut files, p.file.clone()) as u8);
                    let number = place(&mut targets, row.target) + 1;
                    let (name, on) = (format!("shader {letter}"), format!("target {number}"));
                    if !subjects.iter().any(|s| s.name == name && s.on == on) {
                        subjects.push(Subject {
                            name,
                            on,
                            target: row.target,
                            effect: effect.clone(),
                        });
                    }
                }
            }
            println!(
                "{} Shader effects came across of {} effects; {} shader files, {} targets",
                import
                    .sequence
                    .effects()
                    .filter(|e| e.kind() == EffectKind::Shader)
                    .count(),
                import.sequence.effect_count(),
                files.len(),
                targets.len()
            );
        }
        None => {
            for name in &on {
                let prop = show
                    .props
                    .iter()
                    .find(|p| &p.name == name)
                    .map(|p| Target::Prop(p.id));
                let group = show
                    .groups
                    .iter()
                    .find(|g| &g.name == name)
                    .map(|g| Target::Group(g.id));
                let Some(target) = prop.or(group) else {
                    anyhow::bail!("the show has no prop or group called {name}");
                };
                let mut kinds: Vec<(String, EffectParams)> = pf_render::built_in_shaders()
                    .into_iter()
                    .map(|file| {
                        let params = EffectParams::Shader(ShaderParams {
                            file: file.clone(),
                            ..ShaderParams::default()
                        });
                        (shader_name(&file), params)
                    })
                    .collect();
                for kind in [EffectKind::Plasma, EffectKind::Butterfly] {
                    kinds.push((format!("({})", kind.label()), EffectParams::default_for(kind)));
                }
                for (shader, params) in kinds {
                    subjects.push(Subject {
                        name: shader,
                        on: name.clone(),
                        target,
                        effect: Effect::new(EffectKind::On, 0, 10_000).with_params(params),
                    });
                }
            }
        }
    }

    for subject in &subjects {
        let buffer = geometry.buffer(subject.target);
        let pixels = buffer.len();
        if pixels == 0 {
            continue;
        }
        let seq = alone(subject.target, &subject.effect, frame_ms);
        let fill = Effect::new(EffectKind::On, subject.effect.start_ms, subject.effect.end_ms);
        let filled = alone(subject.target, &fill, frame_ms);
        let (first, last) = (
            (subject.effect.start_ms + 500) / u64::from(frame_ms),
            subject.effect.end_ms.saturating_sub(500) / u64::from(frame_ms),
        );
        let frames: Vec<u64> = (0..SAMPLES)
            .map(|k| first + last.saturating_sub(first) * k / (SAMPLES - 1))
            .collect();
        println!(
            "\n{} on {}: {pixels} pixels on a grid of {} by {}",
            subject.name, subject.on, buffer.columns, buffer.rows
        );
        // The work each pixel does, halfway through, with the frame's allowance shared among
        // this effect's pixels alone (as it is when nothing else in the frame is a shader).
        if let EffectParams::Shader(p) = &subject.effect.params {
            let shaders = pictures.shaders();
            let middle = (subject.effect.start_ms + subject.effect.end_ms) / 2;
            let time = EffectTime::within(subject.effect.start_ms, subject.effect.end_ms, middle)
                .with_frame_ms(frame_ms);
            let cx = RenderContext::default().with_shaders(&shaders, pixels.max(64));
            let canvas = Canvas {
                columns: buffer.columns,
                rows: buffer.rows,
            };
            let cost = shader_frame_cost(p, &time, canvas, &cx, &buffer.pixels);
            println!(
                "  work a pixel: {:.0} of {} allowed there ({:.1}%); {} of {pixels} pixels stopped",
                cost.work as f64 / pixels as f64,
                cost.allowed,
                100.0 * cost.work as f64 / pixels as f64 / f64::from(cost.allowed.max(1)),
                cost.stopped,
            );
        }
        let mut reference: Option<Vec<u8>> = None;
        for threads in [1usize, 2, 4, 8] {
            let mut renderer = Renderer::from_geometry(geometry.clone());
            renderer.set_pictures(pictures.clone());
            renderer.set_pool(match threads {
                1 => None,
                n => RenderPool::new(n),
            });
            let (bytes, took) = draw(&mut renderer, &seq, &frames);
            let (_, fill_took) = draw(&mut renderer, &filled, &frames);
            let frame_us = took.as_secs_f64() * 1e6 / SAMPLES as f64;
            let fill_us = fill_took.as_secs_f64() * 1e6 / SAMPLES as f64;
            let own_us = (frame_us - fill_us).max(0.0);
            let same = match &reference {
                None => {
                    let lit = bytes.iter().filter(|&&b| b > 0).count();
                    reference = Some(bytes);
                    format!("the reference ({lit} bytes lit)")
                }
                Some(reference) if *reference == bytes => "identical bytes".to_string(),
                Some(reference) => {
                    let differ = reference.iter().zip(&bytes).filter(|(a, b)| a != b).count();
                    format!("{differ} BYTES DIFFER")
                }
            };
            println!(
                "  {threads} thread(s): {frame_us:9.1} us a frame, {fill_us:6.1} with a fill; its own {own_us:9.1} us a frame, {:7.1} ns a pixel; {same}",
                own_us * 1e3 / pixels as f64,
            );
        }
    }
    Ok(())
}
