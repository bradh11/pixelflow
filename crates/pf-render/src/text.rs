//! The Text effect, after xLights' (`TextEffect::RenderTextLine` in `src-core/effects/TextEffect.cpp`),
//! in PixelFlow's own pixel font: the text is laid out in the middle of the target's grid, one
//! line under another and each line centered, then moved as xLights moves it. xLights counts
//! `speed` a frame-worth of time and moves the text a cell for every 8 of the count, starting
//! just off one side and crossing until it's just off the other.
//!
//! The font is the classic 5 × 7 one (with room for descenders, 8 tall), scaled to the letter
//! height. Each pixel works out which letter and dot it falls on, so nothing is drawn ahead.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{TextCountdown, TextMovement, TextOrientation, TextParams};

/// The font: 5 columns per character (bit 0 the top row, bit 7 the bottom), for ASCII 32 to 126.
/// Anything else shows as a box.
const FONT: [[u8; 5]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], // space
    [0x00, 0x00, 0x5F, 0x00, 0x00], // !
    [0x00, 0x07, 0x00, 0x07, 0x00], // "
    [0x14, 0x7F, 0x14, 0x7F, 0x14], // #
    [0x24, 0x2A, 0x7F, 0x2A, 0x12], // $
    [0x23, 0x13, 0x08, 0x64, 0x62], // %
    [0x36, 0x49, 0x56, 0x20, 0x50], // &
    [0x00, 0x08, 0x07, 0x03, 0x00], // '
    [0x00, 0x1C, 0x22, 0x41, 0x00], // (
    [0x00, 0x41, 0x22, 0x1C, 0x00], // )
    [0x2A, 0x1C, 0x7F, 0x1C, 0x2A], // *
    [0x08, 0x08, 0x3E, 0x08, 0x08], // +
    [0x00, 0x80, 0x70, 0x30, 0x00], // ,
    [0x08, 0x08, 0x08, 0x08, 0x08], // -
    [0x00, 0x00, 0x60, 0x60, 0x00], // .
    [0x20, 0x10, 0x08, 0x04, 0x02], // /
    [0x3E, 0x51, 0x49, 0x45, 0x3E], // 0
    [0x00, 0x42, 0x7F, 0x40, 0x00], // 1
    [0x72, 0x49, 0x49, 0x49, 0x46], // 2
    [0x21, 0x41, 0x49, 0x4D, 0x33], // 3
    [0x18, 0x14, 0x12, 0x7F, 0x10], // 4
    [0x27, 0x45, 0x45, 0x45, 0x39], // 5
    [0x3C, 0x4A, 0x49, 0x49, 0x31], // 6
    [0x41, 0x21, 0x11, 0x09, 0x07], // 7
    [0x36, 0x49, 0x49, 0x49, 0x36], // 8
    [0x46, 0x49, 0x49, 0x29, 0x1E], // 9
    [0x00, 0x00, 0x14, 0x00, 0x00], // :
    [0x00, 0x40, 0x34, 0x00, 0x00], // ;
    [0x00, 0x08, 0x14, 0x22, 0x41], // <
    [0x14, 0x14, 0x14, 0x14, 0x14], // =
    [0x00, 0x41, 0x22, 0x14, 0x08], // >
    [0x02, 0x01, 0x59, 0x09, 0x06], // ?
    [0x3E, 0x41, 0x5D, 0x59, 0x4E], // @
    [0x7C, 0x12, 0x11, 0x12, 0x7C], // A
    [0x7F, 0x49, 0x49, 0x49, 0x36], // B
    [0x3E, 0x41, 0x41, 0x41, 0x22], // C
    [0x7F, 0x41, 0x41, 0x41, 0x3E], // D
    [0x7F, 0x49, 0x49, 0x49, 0x41], // E
    [0x7F, 0x09, 0x09, 0x09, 0x01], // F
    [0x3E, 0x41, 0x41, 0x51, 0x73], // G
    [0x7F, 0x08, 0x08, 0x08, 0x7F], // H
    [0x00, 0x41, 0x7F, 0x41, 0x00], // I
    [0x20, 0x40, 0x41, 0x3F, 0x01], // J
    [0x7F, 0x08, 0x14, 0x22, 0x41], // K
    [0x7F, 0x40, 0x40, 0x40, 0x40], // L
    [0x7F, 0x02, 0x1C, 0x02, 0x7F], // M
    [0x7F, 0x04, 0x08, 0x10, 0x7F], // N
    [0x3E, 0x41, 0x41, 0x41, 0x3E], // O
    [0x7F, 0x09, 0x09, 0x09, 0x06], // P
    [0x3E, 0x41, 0x51, 0x21, 0x5E], // Q
    [0x7F, 0x09, 0x19, 0x29, 0x46], // R
    [0x26, 0x49, 0x49, 0x49, 0x32], // S
    [0x03, 0x01, 0x7F, 0x01, 0x03], // T
    [0x3F, 0x40, 0x40, 0x40, 0x3F], // U
    [0x1F, 0x20, 0x40, 0x20, 0x1F], // V
    [0x3F, 0x40, 0x38, 0x40, 0x3F], // W
    [0x63, 0x14, 0x08, 0x14, 0x63], // X
    [0x03, 0x04, 0x78, 0x04, 0x03], // Y
    [0x61, 0x59, 0x49, 0x4D, 0x43], // Z
    [0x00, 0x7F, 0x41, 0x41, 0x41], // [
    [0x02, 0x04, 0x08, 0x10, 0x20], // backslash
    [0x00, 0x41, 0x41, 0x41, 0x7F], // ]
    [0x04, 0x02, 0x01, 0x02, 0x04], // ^
    [0x40, 0x40, 0x40, 0x40, 0x40], // _
    [0x00, 0x03, 0x07, 0x08, 0x00], // `
    [0x20, 0x54, 0x54, 0x78, 0x40], // a
    [0x7F, 0x28, 0x44, 0x44, 0x38], // b
    [0x38, 0x44, 0x44, 0x44, 0x28], // c
    [0x38, 0x44, 0x44, 0x28, 0x7F], // d
    [0x38, 0x54, 0x54, 0x54, 0x18], // e
    [0x00, 0x08, 0x7E, 0x09, 0x02], // f
    [0x18, 0xA4, 0xA4, 0x9C, 0x78], // g
    [0x7F, 0x08, 0x04, 0x04, 0x78], // h
    [0x00, 0x44, 0x7D, 0x40, 0x00], // i
    [0x20, 0x40, 0x40, 0x3D, 0x00], // j
    [0x7F, 0x10, 0x28, 0x44, 0x00], // k
    [0x00, 0x41, 0x7F, 0x40, 0x00], // l
    [0x7C, 0x04, 0x78, 0x04, 0x78], // m
    [0x7C, 0x08, 0x04, 0x04, 0x78], // n
    [0x38, 0x44, 0x44, 0x44, 0x38], // o
    [0xFC, 0x18, 0x24, 0x24, 0x18], // p
    [0x18, 0x24, 0x24, 0x18, 0xFC], // q
    [0x7C, 0x08, 0x04, 0x04, 0x08], // r
    [0x48, 0x54, 0x54, 0x54, 0x24], // s
    [0x04, 0x04, 0x3F, 0x44, 0x24], // t
    [0x3C, 0x40, 0x40, 0x20, 0x7C], // u
    [0x1C, 0x20, 0x40, 0x20, 0x1C], // v
    [0x3C, 0x40, 0x30, 0x40, 0x3C], // w
    [0x44, 0x28, 0x10, 0x28, 0x44], // x
    [0x4C, 0x90, 0x90, 0x90, 0x7C], // y
    [0x44, 0x64, 0x54, 0x4C, 0x44], // z
    [0x00, 0x08, 0x36, 0x41, 0x00], // {
    [0x00, 0x00, 0x77, 0x00, 0x00], // |
    [0x00, 0x41, 0x36, 0x08, 0x00], // }
    [0x02, 0x01, 0x02, 0x04, 0x02], // ~
];
/// What a character the font doesn't have shows as.
const BOX: [u8; 5] = [0x7F, 0x41, 0x41, 0x41, 0x7F];
/// A character's width and height in font dots, and the gap after it.
const GLYPH_W: i32 = 5;
const GLYPH_H: i32 = 8;
const ADVANCE: i32 = GLYPH_W + 1;

fn glyph(c: char) -> [u8; 5] {
    let code = c as u32;
    if (32..127).contains(&code) {
        FONT[(code - 32) as usize]
    } else {
        BOX
    }
}

/// One line of text: its characters, the palette color of each, and where it starts across the
/// text block (in grid cells).
#[derive(Debug, Clone)]
struct Line {
    chars: Vec<(char, usize)>,
    left: i32,
}

pub struct Text {
    colors: Colors,
    width: i32,
    height: i32,
    lines: Vec<Line>,
    /// Grid cells per font dot.
    scale: f32,
    /// The text block's top left, in cells from the grid's top left (rows counting down).
    left: i32,
    top: i32,
}

/// xLights' back-and-forth count (`zigzag`): up to `range` and back, over and over.
fn zigzag(value: i64, range: i64) -> i64 {
    let range = range.max(1);
    if (value / range) & 1 == 1 {
        value % range
    } else {
        range - value % range - 1
    }
}

/// What the text says this frame: the text itself, or a countdown from the number in it.
fn message(p: &TextParams, time: &EffectTime, state: i64) -> String {
    let text = p.text.replace("\\n", "\n");
    if p.countdown == TextCountdown::None {
        return text;
    }
    let fps = (1000 / time.frame_ms.max(1)).max(1) as i64;
    let frame = time.frame() as i64;
    // xLights starts the count on every frame its own count is still 0.
    let speed = i64::from(p.speed);
    let started = if speed == 0 || state == 0 {
        frame
    } else {
        let per = speed * i64::from(time.frame_ms);
        (49 / per).min(frame)
    };
    let left = |seconds: i64| ((seconds * fps + fps - 1 - (frame - started)) / fps).max(0);
    match p.countdown {
        TextCountdown::Seconds => left(text.trim().parse::<i64>().unwrap_or(0)).to_string(),
        _ => {
            let (prefix, middle, suffix) = match text.split('/').collect::<Vec<_>>()[..] {
                [_] => ("", text.as_str(), ""),
                [first, .., last] => {
                    let start = first.len() + 1;
                    let end = text.len() - last.len() - 1;
                    (first, &text[start..end.max(start)], last)
                }
                [] => ("", "", ""),
            };
            let parts: Vec<&str> = middle.split(':').collect();
            let number = |s: &str| s.trim().parse::<i64>().unwrap_or(0);
            let seconds = match parts[..] {
                [s] => number(s),
                [m, s] => number(m) * 60 + number(s),
                _ => return "Invalid Format".to_string(),
            };
            let seconds = left(seconds);
            format!("{prefix} {} : {:02}{suffix}", seconds / 60, seconds % 60)
        }
    }
}

impl Text {
    pub fn new(p: &TextParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let speed = i64::from(p.speed.min(100));
        let state = (time.frame() as i64)
            .saturating_mul(speed)
            .saturating_mul(i64::from(time.frame_ms))
            / 50;
        let mut msg = message(p, time, state);
        match p.orientation {
            TextOrientation::Across => {}
            TextOrientation::StackedDown => msg = msg.chars().flat_map(|c| [c, '\n']).collect(),
            TextOrientation::StackedUp => msg = msg.chars().rev().flat_map(|c| [c, '\n']).collect(),
        }
        if msg.ends_with('\n') && p.orientation != TextOrientation::Across {
            msg.pop();
        }
        let scale = p.size.clamp(4, 100) as f32 / GLYPH_H as f32;
        let dots = |n: i32| (n as f32 * scale).round() as i32;
        let line_width = |chars: usize| dots((chars as i32 * ADVANCE - 1).max(0));

        // Palette colors letter by letter (or word by word); one color for all with one.
        let many = colors.len() > 1;
        let mut k = 0usize;
        let mut lines: Vec<Line> = Vec::new();
        for text in msg.split('\n') {
            let chars: Vec<char> = text.chars().collect();
            let mut colored = Vec::with_capacity(chars.len());
            for (i, &c) in chars.iter().enumerate() {
                colored.push((c, if many { k } else { 0 }));
                let next_starts_word = chars.get(i + 1).is_some_and(|&n| n != ' ');
                if (p.color_per_word && c == ' ' && next_starts_word) || (!p.color_per_word && c != ' ') {
                    k += 1;
                }
            }
            if p.color_per_word && !chars.is_empty() {
                k += 1;
            }
            lines.push(Line {
                chars: colored,
                left: 0,
            });
        }
        let block_w = lines.iter().map(|l| line_width(l.chars.len())).max().unwrap_or(0);
        let block_h = dots(GLYPH_H) * lines.len() as i32;
        for line in &mut lines {
            line.left = (block_w - line_width(line.chars.len())) / 2;
        }
        // Leading or trailing spaces, which text stopping in the middle leaves out.
        let spaces = |n: usize| dots(n as i32 * ADVANCE);
        let extra_left = spaces(msg.chars().take_while(|&c| c == ' ').count());
        let extra_right = spaces(msg.chars().rev().take_while(|&c| c == ' ').count());

        let (w, h) = (i64::from(width), i64::from(height));
        let (tw, th) = (i64::from(block_w), i64::from(block_h));
        let xlimit = (w + tw) * 8 + 1;
        let ylimit = (h + th) * 8 + 1;
        let total_h = h + th;
        let (start_x, start_y) = (i64::from(p.start_x as i32), i64::from(p.start_y as i32));
        let (offset_left, offset_top) = if p.pixel_offsets {
            (start_x, -start_y)
        } else {
            (start_x * w / 100, -start_y * h / 100)
        };
        let center = p.to_center;
        let once = p.no_repeat && !center;
        let s8 = state / 8;
        let across = |s: i64| xlimit / 16 - s % xlimit / 8;
        let back = |s: i64| s % xlimit / 8 - xlimit / 16;
        let rising = |s: i64| ylimit / 16 - s % ylimit / 8;
        let sinking = |s: i64| s % ylimit / 8 - ylimit / 16;
        use TextMovement as M;
        let (dx, dy) = match p.movement {
            M::Vector => {
                let position = time.cycle_position(1.0);
                let (ex, ey) = if p.pixel_offsets {
                    (i64::from(p.end_x as i32), -i64::from(p.end_y as i32))
                } else {
                    (
                        i64::from(p.end_x as i32) * w / 100,
                        -i64::from(p.end_y as i32) * h / 100,
                    )
                };
                (
                    (offset_left as f64 + (ex - offset_left) as f64 * position) as i64,
                    (offset_top as f64 + (ey - offset_top) as f64 * position) as i64,
                )
            }
            M::Left if once && state > xlimit => (-xlimit, offset_top),
            M::Left if center => ((xlimit / 16 - s8).max(-extra_left as i64 / 2), offset_top),
            M::Left => (across(state), offset_top),
            M::Right if once && state > xlimit => (xlimit, offset_top),
            M::Right if center => ((s8 - xlimit / 16).min(extra_right as i64 / 2), offset_top),
            M::Right => (back(state), offset_top),
            M::Up if once && state > ylimit => (offset_left, -ylimit),
            M::Up if center => (offset_left, (ylimit / 16 - s8).max(0)),
            M::Up => (offset_left, rising(state)),
            M::Down if once && state > ylimit => (offset_left, ylimit),
            M::Down if center => (offset_left, (s8 - ylimit / 16).min(0)),
            M::Down => (offset_left, sinking(state)),
            M::UpLeft | M::DownLeft | M::UpRight | M::DownRight
                if once && (state > ylimit || state > xlimit) =>
            {
                let x = if matches!(p.movement, M::UpLeft | M::DownLeft) {
                    -xlimit
                } else {
                    xlimit
                };
                let y = if matches!(p.movement, M::UpLeft | M::UpRight) {
                    -ylimit
                } else {
                    ylimit
                };
                (x, y)
            }
            M::UpLeft | M::DownLeft | M::UpRight | M::DownRight => {
                // These take the start offsets as cells, as xLights does.
                let x = if matches!(p.movement, M::UpLeft | M::DownLeft) {
                    if center {
                        (xlimit / 16 - s8 + start_x).max(0)
                    } else {
                        across(state) + start_x
                    }
                } else if center {
                    (s8 - xlimit / 16 - start_x).min(0)
                } else {
                    back(state) - start_x
                };
                let y = if matches!(p.movement, M::UpLeft | M::UpRight) {
                    if center {
                        (ylimit / 16 - s8 - start_y).max(0)
                    } else {
                        rising(state) - start_y
                    }
                } else if center {
                    (s8 - ylimit / 16 + start_y).min(0)
                } else {
                    sinking(state) + start_y
                };
                (x, y)
            }
            M::Wavy if center => (
                (s8 - xlimit / 16).min(extra_right as i64 / 2),
                (zigzag(state / 4, total_h) / 2 - total_h / 4).max(-extra_left as i64 / 2),
            ),
            M::Wavy => (across(state), zigzag(state / 4, total_h) / 2 - total_h / 4),
            M::LeftRight => {
                if p.no_repeat && state > xlimit {
                    (-xlimit, offset_top)
                } else {
                    let half = (xlimit / 2).max(1);
                    let n = state % xlimit;
                    let x = if n <= half {
                        xlimit / 8 - (n * (xlimit / 4)) / half
                    } else {
                        -xlimit / 8 + ((n - half) * (xlimit / 4)) / half
                    };
                    (x, offset_top)
                }
            }
            M::UpDown => {
                if p.no_repeat && state > ylimit {
                    (offset_left, -ylimit)
                } else {
                    let half = (ylimit / 2).max(1);
                    let n = state % ylimit;
                    let y = if n <= half {
                        ylimit / 16 - (n * (ylimit / 8)) / half
                    } else {
                        -(ylimit / 16) + ((n - half) * (ylimit / 8)) / half
                    };
                    (offset_left, y)
                }
            }
            M::None => (offset_left, offset_top),
        };
        let clamp = |v: i64| v.clamp(-(1 << 30), 1 << 30) as i32;
        Self {
            colors,
            width,
            height,
            lines,
            scale,
            left: clamp(dx) + (width - block_w) / 2,
            top: clamp(dy) + (height - block_h) / 2,
        }
    }
}

impl Shade for Text {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.width, self.height);
        // Rows count down from the top, as text is laid out.
        let (col, row) = (x - self.left, self.height - 1 - y - self.top);
        if col < 0 || row < 0 {
            return Rgba::CLEAR;
        }
        let line_h = (GLYPH_H as f32 * self.scale).round().max(1.0) as i32;
        let Some(line) = self.lines.get((row / line_h) as usize) else {
            return Rgba::CLEAR;
        };
        let dot_x = ((col - line.left) as f32 / self.scale).floor() as i32;
        let dot_y = (((row % line_h) as f32) / self.scale).floor() as i32;
        if dot_x < 0 || !(0..GLYPH_H).contains(&dot_y) {
            return Rgba::CLEAR;
        }
        let (n, dot) = (dot_x / ADVANCE, dot_x % ADVANCE);
        let Some(&(c, k)) = line.chars.get(n as usize) else {
            return Rgba::CLEAR;
        };
        if dot >= GLYPH_W || glyph(c)[dot as usize] & (1 << dot_y) == 0 {
            return Rgba::CLEAR;
        }
        Rgba::opaque(self.colors.get(k as u64))
    }
}
