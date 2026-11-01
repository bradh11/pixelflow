//! Times a short 1080p export of made-up shows of several sizes, for checking that drawing the
//! frames stays fast: a house's worth of outlines, window frames, trees and canes, and the rest of
//! the pixels in matrices, every prop playing an effect that keeps most of its pixels lit. With
//! `--heads`, that many moving heads stand in a row in front of it, circling and fanned so their
//! beams sweep across each other.
//!
//! cargo run --release -p pf-video --example export_speed -- [PIXELS ...] \
//!     [--seconds SECONDS] [--glow LEVEL] [--height 720|1080|2160] [--heads N] [--keep FOLDER]

use anyhow::Context;
use pf_engine::{Edit, Engine};
use pf_model::{DmxFixture, Generator, MatrixWiring, Prop, Rgb, ShapeSource, TreeStyle, Vec3, dmx_layout};
use pf_sequence::{Effect, EffectKind, EffectParams, HeadPattern, MovingHeadParams, Row, Target};
use pf_video::{VideoOptions, export_video};
use std::path::PathBuf;

/// A prop's shape and the room it takes in the layout (its origin is the middle of its base).
struct Piece {
    shape: Generator,
    width: f32,
    height: f32,
}

/// Matrix sizes, largest first.
const MATRICES: [(u32, u32); 5] = [(256, 128), (128, 64), (96, 48), (64, 32), (48, 16)];

/// The shapes of a show of about `pixels` pixels: three tenths of them in outlines and window
/// frames, a tenth in trees and canes, the rest in matrices.
fn pieces(pixels: u32) -> Vec<Piece> {
    let mut list = Vec::new();
    let mut left = pixels * 3 / 10;
    while left >= 150 {
        if list.len() % 3 == 2 {
            list.push(Piece {
                shape: Generator::WindowFrame {
                    top: 40,
                    sides: 35,
                    bottom: 40,
                    width: 1.2,
                    height: 1.05,
                    start: Default::default(),
                    counter_clockwise: false,
                },
                width: 1.2,
                height: 1.05,
            });
        } else {
            // A roof line: pixels 30 mm apart.
            list.push(Piece {
                shape: Generator::Line {
                    nodes: 150,
                    length: 4.5,
                },
                width: 4.5,
                height: 0.3,
            });
        }
        left -= 150;
    }
    let mut left = pixels / 10;
    while left >= 180 {
        if left >= 800 {
            list.push(Piece {
                shape: Generator::tree(16, 50, 2.5, 0.8, 0.05, TreeStyle::Round),
                width: 1.6,
                height: 2.5,
            });
            left -= 800;
        }
        list.push(Piece {
            shape: Generator::CandyCanes {
                canes: 6,
                nodes_per_cane: 30,
                width: 2.4,
                height: 1.0,
                cane_height: 1.0,
                reverse: false,
                sticks: false,
                alternate_nodes: false,
                skew_deg: 0.0,
                start_right: false,
            },
            width: 2.4,
            height: 1.0,
        });
        left -= 180;
    }
    let used: u32 = list.iter().map(|p| p.shape.node_count()).sum();
    let mut left = pixels.saturating_sub(used);
    for (columns, rows) in MATRICES {
        // Panels are 10 mm apart; pixel-node matrices 25 mm.
        let pitch = if columns >= 128 { 0.01 } else { 0.025 };
        while left >= columns * rows {
            list.push(Piece {
                shape: Generator::Matrix {
                    columns,
                    rows,
                    width: columns as f32 * pitch,
                    height: rows as f32 * pitch,
                    wiring: MatrixWiring::default(),
                },
                width: columns as f32 * pitch,
                height: rows as f32 * pitch,
            });
            left -= columns * rows;
        }
    }
    list
}

/// The pieces as props, packed left to right in rows.
fn props(pixels: u32) -> Vec<Prop> {
    let scene_width = 18.0 + 12.0 * (pixels as f32 / 20_000.0).sqrt();
    let (mut x, mut y, mut row_height) = (0.0f32, 0.0f32, 0.0f32);
    pieces(pixels)
        .into_iter()
        .enumerate()
        .map(|(n, piece)| {
            if x > 0.0 && x + piece.width > scene_width {
                x = 0.0;
                y += row_height + 0.4;
                row_height = 0.0;
            }
            let centered = matches!(
                piece.shape,
                Generator::Matrix { .. } | Generator::WindowFrame { .. } | Generator::Line { .. }
            );
            let base = if centered { y + piece.height / 2.0 } else { y };
            let mut prop = Prop::new(format!("Prop {}", n + 1), ShapeSource::Generator(piece.shape));
            prop.transform.position = Vec3::new(x + piece.width / 2.0, base, 0.0);
            x += piece.width + 0.4;
            row_height = row_height.max(piece.height);
            prop
        })
        .collect()
}

/// `count` 16-bit RGBW moving heads in a row `width` wide along the front of the scene.
fn heads(count: u32, width: f32) -> Vec<Prop> {
    let layout = dmx_layout("head-rgbw-16").expect("the library's 16-bit RGBW head");
    (0..count)
        .map(|k| {
            let mut head = Prop::new(
                format!("Head {}", k + 1),
                ShapeSource::Generator(Generator::Line {
                    nodes: 1,
                    length: 0.5,
                }),
            );
            let step = if count > 1 {
                width / (count - 1) as f32
            } else {
                0.0
            };
            head.transform.position = Vec3::new(k as f32 * step, 0.3, 1.0);
            head.dmx = Some(DmxFixture::from_layout(layout, 1, (1 + k * 16) as u16));
            head
        })
        .collect()
}

/// A Moving Head effect for head `k` of a row: leaning out, circling, every other one turned the
/// other way so their beams cross.
fn sweep(k: usize, duration_ms: u64) -> Effect {
    let colors = [
        Rgb::new(255, 128, 0),
        Rgb::new(160, 0, 255),
        Rgb::new(0, 200, 255),
    ];
    let params = MovingHeadParams {
        pan: if k % 2 == 0 { -35.0 } else { 35.0 },
        tilt: 25.0,
        pattern: HeadPattern::Circle,
        width: 50.0,
        height: 20.0,
        speed: 0.5,
        ..MovingHeadParams::default()
    };
    Effect::new(EffectKind::MovingHead, 0, duration_ms)
        .with_params(EffectParams::MovingHead(params))
        .with_palette(vec![colors[k % colors.len()]])
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| -> Option<&String> {
        let at = args.iter().position(|a| a == name)?;
        args.get(at + 1)
    };
    let seconds: f64 = value("--seconds").and_then(|v| v.parse().ok()).unwrap_or(4.0);
    let glow: f32 = value("--glow").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let height: u32 = value("--height").and_then(|v| v.parse().ok()).unwrap_or(1080);
    let keep = value("--keep").map(PathBuf::from);
    let head_count: u32 = value("--heads").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut sizes: Vec<u32> = Vec::new();
    let mut skip = false;
    for arg in &args {
        if skip {
            skip = false;
        } else if arg.starts_with("--") {
            skip = true;
        } else {
            sizes.push(
                arg.parse()
                    .with_context(|| format!("{arg} isn't a pixel count"))?,
            );
        }
    }
    if sizes.is_empty() {
        sizes = vec![5_000, 50_000, 200_000];
    }

    let kinds = [
        (
            EffectKind::ColorWash,
            [Rgb::new(255, 40, 0), Rgb::new(0, 80, 255)],
        ),
        (
            EffectKind::Twinkle,
            [Rgb::new(255, 255, 255), Rgb::new(255, 180, 0)],
        ),
        (EffectKind::Chase, [Rgb::new(0, 255, 60), Rgb::new(200, 0, 0)]),
        (
            EffectKind::Shimmer,
            [Rgb::new(160, 0, 255), Rgb::new(0, 200, 200)],
        ),
    ];
    let duration_ms = (seconds * 1000.0) as u64;
    for pixels in sizes {
        let data = tempfile::tempdir()?;
        let mut engine = Engine::new(data.path());
        let mut props = props(pixels);
        let count: u32 = props.iter().map(Prop::node_count).sum();
        let mut rows: Vec<Row> = props
            .iter()
            .enumerate()
            .map(|(n, prop)| {
                let (kind, colors) = &kinds[n % kinds.len()];
                let mut row = Row::new(Target::Prop(prop.id));
                row.layers[0]
                    .effects
                    .push(Effect::new(*kind, 0, duration_ms).with_palette(colors.to_vec()));
                row
            })
            .collect();
        let width = props
            .iter()
            .map(|p| p.transform.position.x)
            .fold(0.0f32, f32::max);
        for (k, head) in heads(head_count, width).into_iter().enumerate() {
            let mut row = Row::new(Target::Prop(head.id));
            row.layers[0].effects.push(sweep(k, duration_ms));
            rows.push(row);
            props.push(head);
        }
        engine.apply(props.into_iter().map(|prop| Edit::AddProp { prop }).collect())?;
        engine.new_sequence_doc_with_rows("Speed", duration_ms, None, rows)?;
        let job = engine.sequence_export()?;
        let options = VideoOptions {
            width: height * 16 / 9,
            height,
            fps: 30,
            start_ms: 0,
            end_ms: None,
            photo: false,
            pixel_size: 1.0,
            glow,
            encoder: pf_video::Encoder::OpenH264FromSource,
        };
        let out = match &keep {
            Some(folder) => folder.join(format!(
                "speed-{count}-glow{:.0}-heads{head_count}.mp4",
                glow * 100.0
            )),
            None => data.path().join("speed.mp4"),
        };
        let mut rendering = None;
        let started = std::time::Instant::now();
        let summary = export_video(&job, &options, &out, |p| {
            if p.stage == pf_video::Stage::Rendering {
                rendering = Some(started.elapsed());
            }
            true
        })?;
        let drawn = rendering.unwrap_or_default().as_secs_f64();
        println!(
            "{count} pixels, {head_count} heads, {}p, glow {glow}: {} frames in {:.2} s ({:.1} frames a second; drawing and encoding them {:.2} s, {:.1} a second)",
            summary.height,
            summary.frames,
            summary.elapsed_ms as f64 / 1000.0,
            summary.frames as f64 * 1000.0 / summary.elapsed_ms.max(1) as f64,
            drawn,
            summary.frames as f64 / drawn.max(1e-3),
        );
    }
    Ok(())
}
