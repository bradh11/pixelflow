//! Draws contact sheets of the Picture effect as PNGs, for looking at a picture or GIF the way a
//! small matrix shows it: every cell of the prop blown up into a square, with a grid between,
//! over a dark checked backdrop so clear parts show as clear.
//!
//! cargo run --release -p pf-video --example picture_sheets -- PICTURE OUT_DIR [--scale N]
//! cargo run --release -p pf-video --example picture_sheets -- --timing
//!
//! For the picture it writes, into OUT_DIR:
//! - `<name>_<columns>x<rows>.png`: eight frames across one pass of the animation, fitted, on a
//!   64 × 32 panel and a 12 × 50 pillar;
//! - `<name>_<columns>x<rows>_ways.png`: one frame fitted, filling, stretched, and pixel for
//!   pixel, then with black made clear, crisp, and on its side.
//!
//! `--timing` instead times a 200-frame GIF made on the spot on a 64 × 32 grid: reading it, and
//! drawing a frame once it's read.

use anyhow::{Context, bail};
use image::{Rgb, RgbImage};
use pf_render::{Canvas, Colors, EffectTime, Pictures, Pixel, RenderContext, Rgba, Shade, Shader};
use pf_sequence::{EffectParams, PictureFit, PictureParams, PictureTiming, PictureTurn};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Frames across a pass of the animation.
const FRAMES: u32 = 8;
/// The effect each sheet draws: one pass of the animation stretched over it.
const LENGTH_MS: u64 = 8_000;

const GRID: Rgb<u8> = Rgb([38, 38, 44]);
const GAP: Rgb<u8> = Rgb([90, 90, 100]);
/// The backdrop behind clear cells, checked so they read as clear.
const BACKDROP: [[u8; 3]; 2] = [[14, 14, 30], [24, 24, 44]];

/// The props to draw on: columns and rows.
const SIZES: [(u32, u32); 2] = [(64, 32), (12, 50)];

/// One frame of the effect on a `columns` × `rows` grid, bottom row first.
fn frame(pictures: &Pictures, p: &PictureParams, columns: u32, rows: u32, t_ms: u64) -> Vec<Rgba> {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    let time = EffectTime::within(0, LENGTH_MS, t_ms).with_frame_ms(25);
    let shader = Shader::in_context(
        &EffectParams::Picture(p.clone()),
        &time,
        Colors::new(&[]),
        1,
        Canvas { columns, rows },
        &RenderContext::default().with_pictures(pictures),
    );
    (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .map(|(x, y)| {
            shader.shade(&Pixel {
                u: at(x, columns),
                v: at(y, rows),
                index: y * columns + x,
                count: columns * rows,
            })
        })
        .collect()
}

/// A sheet of frames, `across` to a row, each cell `scale` pixels square with a grid line.
struct Sheet {
    image: RgbImage,
    columns: u32,
    rows: u32,
    scale: u32,
}

impl Sheet {
    fn new(columns: u32, rows: u32, across: u32, down: u32, scale: u32) -> Self {
        let (w, h) = (columns * scale + scale, rows * scale + scale);
        Self {
            image: RgbImage::from_pixel(across * w + scale, down * h + scale, GAP),
            columns,
            rows,
            scale,
        }
    }

    fn put(&mut self, col: u32, row: u32, cells: &[Rgba]) {
        let s = self.scale;
        let (left, top) = (s + col * (self.columns * s + s), s + row * (self.rows * s + s));
        for y in 0..self.rows {
            for x in 0..self.columns {
                // Over the backdrop, as the layers below would show through.
                let c = cells[(y * self.columns + x) as usize];
                let below = BACKDROP[((x + y) % 2) as usize];
                let a = c.a.clamp(0.0, 1.0);
                let mix = |over: f32, under: u8| {
                    (over.clamp(0.0, 1.0) * 255.0 * a + f32::from(under) * (1.0 - a)).round() as u8
                };
                let color = Rgb([mix(c.r, below[0]), mix(c.g, below[1]), mix(c.b, below[2])]);
                // The grid's bottom row is the picture's last.
                let (px, py) = (left + x * s, top + (self.rows - 1 - y) * s);
                for dy in 0..s {
                    for dx in 0..s {
                        let line = s > 3 && (dx == s - 1 || dy == s - 1);
                        self.image
                            .put_pixel(px + dx, py + dy, if line { GRID } else { color });
                    }
                }
            }
        }
    }

    fn save(&self, path: &Path) -> anyhow::Result<()> {
        self.image
            .save(path)
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// A GIF of `frames` frames, a bright ball crossing a dim striped ground.
fn made_gif(width: u32, height: u32, frames: u32) -> anyhow::Result<Vec<u8>> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut bytes, 30);
        encoder.set_repeat(Repeat::Infinite)?;
        for i in 0..frames {
            let cx = (i as f32 + 0.5) / frames as f32 * width as f32;
            let cy = height as f32 * (0.5 + 0.3 * (i as f32 * 0.2).sin());
            let picture = image::RgbaImage::from_fn(width, height, |x, y| {
                let near = (x as f32 - cx).hypot(y as f32 - cy) < height as f32 * 0.18;
                image::Rgba(match (near, (x / 16 + y / 16) % 2) {
                    (true, _) => [255, 200, 40, 255],
                    (false, 0) => [20, 40, 90, 255],
                    _ => [30, 90, 60, 255],
                })
            });
            let delay = image::Delay::from_numer_denom_ms(50, 1);
            encoder.encode_frame(image::Frame::from_parts(picture, 0, 0, delay))?;
        }
    }
    Ok(bytes)
}

/// Times a 200-frame GIF on a 64 × 32 grid: the first frame (the file is read and its frames
/// kept at the grid's size), then every frame after.
fn timing() -> anyhow::Result<()> {
    let (columns, rows, count) = (64, 32, 200);
    for (width, height) in [(128, 64), (320, 240), (640, 480)] {
        let bytes = made_gif(width, height, count)?;
        let size = bytes.len();
        let pictures = Pictures::new(move |_| Ok(bytes.clone())).waiting();
        let p = PictureParams {
            file: "made.gif".into(),
            ..PictureParams::default()
        };
        let started = Instant::now();
        let first = frame(&pictures, &p, columns, rows, 0);
        let read = started.elapsed();
        // Each frame once (resized to the grid), then all of them again (looked up).
        let step = 50;
        let started = Instant::now();
        let mut lit = first.iter().filter(|c| c.a > 0.0).count();
        for i in 0..count {
            lit += frame(&pictures, &p, columns, rows, u64::from(i) * step)
                .iter()
                .filter(|c| c.a > 0.0)
                .count();
        }
        let fresh = started.elapsed() / count;
        let started = Instant::now();
        let rounds = 20;
        for _ in 0..rounds {
            for i in 0..count {
                lit += frame(&pictures, &p, columns, rows, u64::from(i) * step)
                    .iter()
                    .filter(|c| c.a > 0.0)
                    .count();
            }
        }
        let again = started.elapsed() / (count * rounds);
        println!(
            "{count} frames of {width} x {height} ({:.1} MB as a GIF, {:.0} MB decoded) on {columns} x {rows}: \
             read and kept in {:.0} ms ({:.1} MB kept); a frame's first drawing {:.0} us, again {:.0} us ({lit} cells lit)",
            size as f64 / 1e6,
            f64::from(width * height * 4 * count) / 1e6,
            read.as_secs_f64() * 1e3,
            pictures.kept_bytes() as f64 / 1e6,
            fresh.as_secs_f64() * 1e6,
            again.as_secs_f64() * 1e6,
        );
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--timing") {
        return timing();
    }
    let (Some(picture), Some(out)) = (args.first().map(PathBuf::from), args.get(1).map(PathBuf::from)) else {
        bail!("usage: picture_sheets PICTURE OUT_DIR [--scale N]  |  picture_sheets --timing");
    };
    let scale = args
        .iter()
        .position(|a| a == "--scale")
        .and_then(|at| args.get(at + 1)?.parse().ok())
        .unwrap_or(10u32)
        .clamp(2, 40);
    std::fs::create_dir_all(&out).with_context(|| format!("making {}", out.display()))?;
    let name: String = picture
        .file_stem()
        .map(|n| n.to_string_lossy().replace(' ', "_"))
        .unwrap_or_else(|| "picture".into());
    let pictures = Pictures::new(|file| std::fs::read(file).map_err(|e| e.to_string())).waiting();
    let file = picture.to_string_lossy().into_owned();
    let of = |p: PictureParams| PictureParams {
        file: file.clone(),
        timing: PictureTiming::Stretch,
        ..p
    };
    for (columns, rows) in SIZES {
        // Wide props wrap to two rows of four, so the sheet stays readable.
        let across = if columns > 24 { FRAMES / 2 } else { FRAMES };
        let p = of(PictureParams::default());
        let started = Instant::now();
        let mut sheet = Sheet::new(columns, rows, across, FRAMES / across, scale);
        for k in 0..FRAMES {
            let cells = frame(
                &pictures,
                &p,
                columns,
                rows,
                u64::from(k) * LENGTH_MS / u64::from(FRAMES),
            );
            sheet.put(k % across, k / across, &cells);
        }
        if let Some(why) = pictures.problem(&file) {
            bail!("{} couldn't be read: {why}", picture.display());
        }
        let (width, height) = pictures.size(&file).unwrap_or_default();
        println!(
            "{name} ({width} x {height}) on {columns} x {rows}: {FRAMES} frames in {:.0} ms",
            started.elapsed().as_secs_f64() * 1e3
        );
        sheet.save(&out.join(format!("{name}_{columns}x{rows}.png")))?;

        // One frame each way.
        let ways = [
            PictureParams::default(),
            PictureParams {
                fit: PictureFit::Fill,
                ..PictureParams::default()
            },
            PictureParams {
                fit: PictureFit::Stretch,
                ..PictureParams::default()
            },
            PictureParams {
                fit: PictureFit::Actual,
                ..PictureParams::default()
            },
            PictureParams {
                black_transparent: true,
                black_level: 4.0,
                ..PictureParams::default()
            },
            PictureParams {
                crisp: true,
                ..PictureParams::default()
            },
            PictureParams {
                turn: PictureTurn::Right,
                ..PictureParams::default()
            },
            PictureParams {
                fit: PictureFit::Fill,
                scale: 150.0,
                ..PictureParams::default()
            },
        ];
        let mut sheet = Sheet::new(columns, rows, across, ways.len() as u32 / across, scale);
        for (k, way) in ways.into_iter().enumerate() {
            let cells = frame(&pictures, &of(way), columns, rows, LENGTH_MS * 3 / 8);
            sheet.put(k as u32 % across, k as u32 / across, &cells);
        }
        sheet.save(&out.join(format!("{name}_{columns}x{rows}_ways.png")))?;
    }
    println!("Wrote the contact sheets to {}", out.display());
    Ok(())
}
