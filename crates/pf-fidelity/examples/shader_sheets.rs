//! Draws contact sheets of PixelFlow's own shaders as PNGs, for looking at them the way props
//! of few pixels show them.
//!
//! ```sh
//! cargo run --release -p pf-fidelity --example shader_sheets -- \
//!     examples/shows/demo.pixelflow.json OUT_DIR --on "Window Matrix" --on "Mega Tree" --on Yard
//! ```
//!
//! The show is a PixelFlow show file or an xLights show folder. For each shader it writes
//! `<name>.png` into OUT_DIR: a row for each prop or group named, six frames across (a second
//! and a half apart). Each pixel of the target is a dot where the effect sees it: across and up
//! the target's own layout, which for a group is how its props stand in the show and for a tree
//! is its strings side by side. Pixels the shader leaves dark are drawn as faint dots, so the
//! prop's shape shows.

use anyhow::Context;
use image::{Rgb, RgbImage};
use pf_fidelity::layout::comparison_show;
use pf_mapping::ChannelMap;
use pf_model::Show;
use pf_render::{
    Canvas, Colors, EffectTime, PixelBuffer, RUN_PIXELS, RenderContext, Rgba, SceneGeometry, Shade, Shader,
    Shaders, shader_name,
};
use pf_sequence::{EffectParams, ShaderParams, Target};
use std::path::Path;

const FRAMES: u32 = 6;
const STEP_MS: u64 = 1500;
const FIRST_MS: u64 = 500;
/// A panel's width, the most it is tall, and the space round it, in picture pixels.
const PANEL: u32 = 240;
const TALLEST: u32 = 300;
const GAP: u32 = 10;
const BACK: Rgb<u8> = Rgb([12, 12, 16]);
const UNLIT: [u8; 3] = [34, 34, 40];

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

/// One frame of `p` on `buffer`: each of its pixels' colors as light.
fn frame(shaders: &Shaders, p: &ShaderParams, buffer: &PixelBuffer, t_ms: u64) -> Vec<[u8; 3]> {
    let time = EffectTime::within(0, 60_000, t_ms).with_frame_ms(25);
    let cx = RenderContext::default().with_shaders(shaders, buffer.len().max(64));
    let canvas = Canvas {
        columns: buffer.columns,
        rows: buffer.rows,
    };
    let shader = Shader::in_context(
        &EffectParams::Shader(p.clone()),
        &time,
        Colors::new(&[]),
        1,
        canvas,
        &cx,
    );
    let mut colors = vec![Rgba::CLEAR; buffer.len()];
    for (run, colors) in buffer
        .pixels
        .chunks(RUN_PIXELS)
        .zip(colors.chunks_mut(RUN_PIXELS))
    {
        shader.shade_run(run, colors);
    }
    colors.into_iter().map(Rgba::to_rgb8).collect()
}

/// How tall a panel is for a target `columns` by `rows`, and the radius of a dot in it: as
/// large as the pixels have room for, were they spread evenly.
fn panel(buffer: &PixelBuffer) -> (u32, i32) {
    let (columns, rows) = (buffer.columns.max(1) as f32, buffer.rows.max(1) as f32);
    let height = ((PANEL as f32 * rows / columns) as u32).clamp(PANEL / 4, TALLEST);
    let room = (PANEL as f32 * height as f32 / buffer.len().max(1) as f32).sqrt();
    (height, ((room * 0.45).round() as i32).clamp(2, 5))
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
    let [show_dir, out] = plain.as_slice() else {
        anyhow::bail!("usage: shader_sheets <show folder or file> <out dir> --on <target>...");
    };
    let (show, map) = open_show(Path::new(show_dir))?;
    let show = &show;
    let geometry = SceneGeometry::new(show, &map);
    let mut targets = Vec::new();
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
        let target = prop
            .or(group)
            .with_context(|| format!("no prop or group called {name}"))?;
        let buffer = geometry.buffer(target);
        println!(
            "{name}: {} pixels on a grid of {} by {}",
            buffer.len(),
            buffer.columns,
            buffer.rows
        );
        targets.push(buffer);
    }
    std::fs::create_dir_all(out)?;
    let shaders = Shaders::none();
    for file in pf_render::built_in_shaders() {
        let p = ShaderParams {
            file: file.clone(),
            ..ShaderParams::default()
        };
        let heights: Vec<u32> = targets.iter().map(|b| panel(b).0).collect();
        let width = GAP + FRAMES * (PANEL + GAP);
        let height = GAP + heights.iter().map(|h| h + GAP).sum::<u32>();
        let mut sheet = RgbImage::from_pixel(width, height, BACK);
        let mut top = GAP;
        for (buffer, &tall) in targets.iter().zip(&heights) {
            let radius = panel(buffer).1;
            for n in 0..FRAMES {
                let colors = frame(&shaders, &p, buffer, FIRST_MS + u64::from(n) * STEP_MS);
                let left = GAP + n * (PANEL + GAP);
                for (px, color) in buffer.pixels.iter().zip(colors) {
                    let color = if color == [0, 0, 0] { UNLIT } else { color };
                    let x = left as i32 + (px.u * (PANEL - 1) as f32).round() as i32;
                    let y = top as i32 + ((1.0 - px.v) * (tall - 1) as f32).round() as i32;
                    for dy in -radius + 1..radius {
                        for dx in -radius + 1..radius {
                            let (x, y) = (x + dx, y + dy);
                            if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
                                sheet.put_pixel(x as u32, y as u32, Rgb(color));
                            }
                        }
                    }
                }
            }
            top += tall + GAP;
        }
        let name = shader_name(&file).to_lowercase().replace(' ', "-");
        let path = Path::new(out).join(format!("{name}.png"));
        sheet
            .save(&path)
            .with_context(|| format!("writing {}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(())
}
