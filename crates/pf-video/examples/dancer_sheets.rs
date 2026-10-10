//! Draws contact sheets of the Dancer effect as PNGs, for looking at the characters the way a
//! small matrix shows them: every cell of the prop blown up into a square, with a grid between.
//!
//! cargo run -p pf-video --example dancer_sheets -- OUT_DIR [--scale N]
//!
//! For each character it writes, into OUT_DIR:
//! - `<character>_<columns>x<rows>.png`: eight frames across a bar of the mix, half a beat apart,
//!   on a 12 × 50 pillar and a 32 × 32 panel (and smaller, wider, and larger props, to check the
//!   detail each size keeps);
//! - `<character>_moves.png`: every move on the pillar, a row each, eight frames across its bar.

use anyhow::{Context, bail};
use image::{Rgb, RgbImage};
use pf_render::{Canvas, Colors, EffectTime, Pixel, RenderContext, Shade, Shader};
use pf_sequence::{DancerCharacter, DancerMove, DancerParams, EffectParams};
use std::path::{Path, PathBuf};

/// Frames across a bar, and the time between them at the steady beat (half a beat).
const FRAMES: u32 = 8;
const STEP_MS: u64 = 250;
const BAR_MS: u64 = 2000;

const GRID: Rgb<u8> = Rgb([38, 38, 44]);
const GAP: Rgb<u8> = Rgb([90, 90, 100]);

const CHARACTERS: [(DancerCharacter, &str); 6] = [
    (DancerCharacter::Skeleton, "skeleton"),
    (DancerCharacter::Ghost, "ghost"),
    (DancerCharacter::Witch, "witch"),
    (DancerCharacter::Santa, "santa"),
    (DancerCharacter::Snowman, "snowman"),
    (DancerCharacter::Elf, "elf"),
];

const MOVES: [DancerMove; 7] = [
    DancerMove::Bounce,
    DancerMove::ArmWave,
    DancerMove::Kick,
    DancerMove::Twist,
    DancerMove::Shuffle,
    DancerMove::Jump,
    DancerMove::HeadBob,
];

/// The props to draw on: columns, rows, and dancers side by side.
const SIZES: [(u32, u32, u32); 7] = [
    (12, 50, 1),
    (32, 32, 1),
    (16, 16, 1),
    (16, 32, 1),
    (36, 16, 2),
    (64, 32, 3),
    (96, 96, 1),
];

/// One frame of the effect on a `columns` × `rows` grid, bottom row first.
fn frame(p: &DancerParams, columns: u32, rows: u32, t_ms: u64) -> Vec<[u8; 3]> {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    let time = EffectTime::within(0, 60_000, t_ms).with_frame_ms(25);
    let shader = Shader::in_context(
        &EffectParams::Dancer(*p),
        &time,
        Colors::new(&[]),
        1,
        Canvas { columns, rows },
        &RenderContext::default(),
    );
    (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .map(|(x, y)| {
            let px = Pixel {
                u: at(x, columns),
                v: at(y, rows),
                index: y * columns + x,
                count: columns * rows,
            };
            shader.shade(&px).to_rgb8()
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

    fn put(&mut self, col: u32, row: u32, cells: &[[u8; 3]]) {
        let s = self.scale;
        let (left, top) = (s + col * (self.columns * s + s), s + row * (self.rows * s + s));
        for y in 0..self.rows {
            for x in 0..self.columns {
                let color = Rgb(cells[(y * self.columns + x) as usize]);
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

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first().map(PathBuf::from) else {
        bail!("usage: dancer_sheets OUT_DIR [--scale N]");
    };
    let scale = args
        .iter()
        .position(|a| a == "--scale")
        .and_then(|at| args.get(at + 1)?.parse().ok())
        .unwrap_or(10u32)
        .clamp(2, 40);
    std::fs::create_dir_all(&out).with_context(|| format!("making {}", out.display()))?;
    for (character, name) in CHARACTERS {
        for (columns, rows, count) in SIZES {
            let p = DancerParams {
                character,
                count,
                ..DancerParams::default()
            };
            // Wide props wrap to two rows of four, so the sheet stays readable.
            let across = if columns > 24 { FRAMES / 2 } else { FRAMES };
            // Large props at a smaller scale, so a frame stays a few hundred pixels across.
            let scale = scale.min((480 / columns.max(rows)).max(2));
            let mut sheet = Sheet::new(columns, rows, across, FRAMES / across, scale);
            for k in 0..FRAMES {
                let cells = frame(&p, columns, rows, u64::from(k) * STEP_MS);
                sheet.put(k % across, k / across, &cells);
            }
            sheet.save(&out.join(format!("{name}_{columns}x{rows}.png")))?;
        }
        // Every move on the pillar, at a smaller scale so the rows fit a screen.
        let (columns, rows) = (12, 50);
        let mut sheet = Sheet::new(columns, rows, FRAMES * 2, MOVES.len() as u32 / 2 + 1, scale / 2);
        for (i, moves) in MOVES.into_iter().enumerate() {
            let p = DancerParams {
                character,
                moves,
                ..DancerParams::default()
            };
            for k in 0..FRAMES {
                let cells = frame(&p, columns, rows, BAR_MS + u64::from(k) * STEP_MS);
                let i = i as u32;
                sheet.put((i % 2) * FRAMES + k, i / 2, &cells);
            }
        }
        sheet.save(&out.join(format!("{name}_moves.png")))?;
    }
    println!("Wrote the contact sheets to {}", out.display());
    Ok(())
}
