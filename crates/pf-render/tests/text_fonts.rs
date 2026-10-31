//! The Text effect in a font on the computer. The computers tests run on have different fonts,
//! or none, so these draw in a tiny font made in the test (see `pf_fonts::testing`): its letters
//! are plain boxes, a bar (`I`), and a ring (`O`), at known sizes.

use pf_render::{Canvas, Colors, EffectTime, Pixel, Rgba, Shade, Shader};
use pf_sequence::*;
use std::sync::Once;

const FAMILY: &str = "PixelFlow Render Test Sans";

/// The tiny font, regular only, added to this run's fonts once.
fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        pf_fonts::add_font(pf_fonts::testing::tiny_font(FAMILY, false, false));
    });
}

fn time(elapsed_ms: u64) -> EffectTime {
    EffectTime::within(1000, 11_000, 1000 + elapsed_ms).with_frame_ms(50)
}

/// Every cell's color, top row first (as text is read).
fn draw(p: &TextParams, t: EffectTime, palette: &[Rgb], canvas: Canvas) -> Vec<Vec<Rgba>> {
    install();
    let shader = Shader::new(
        &EffectParams::Text(p.clone()),
        &t,
        Colors::new(palette),
        7,
        canvas,
    );
    (0..canvas.rows)
        .rev()
        .map(|y| {
            (0..canvas.columns)
                .map(|x| {
                    shader.shade(&Pixel {
                        u: x as f32 / (canvas.columns - 1) as f32,
                        v: y as f32 / (canvas.rows - 1) as f32,
                        index: y * canvas.columns + x,
                        count: canvas.columns * canvas.rows,
                    })
                })
                .collect()
        })
        .collect()
}

fn lit(cells: &[Vec<Rgba>]) -> usize {
    cells.iter().flatten().filter(|c| c.a > 0.0).count()
}

/// The columns with anything lit, left to right.
fn columns(cells: &[Vec<Rgba>]) -> Vec<usize> {
    (0..cells[0].len())
        .filter(|&x| cells.iter().any(|row| row[x].a > 0.0))
        .collect()
}

fn font(text: &str, size: u32) -> TextParams {
    TextParams {
        text: text.into(),
        font: FAMILY.into(),
        size,
        ..Default::default()
    }
}

const CANVAS: Canvas = Canvas {
    columns: 40,
    rows: 30,
};

#[test]
fn text_is_drawn_in_the_font_at_its_size() {
    // At 20 pixels the tiny font is 16 above the baseline and 4 below; "I" moves on 7 and "O" 14,
    // so the line is 21 wide and 20 tall, and sits from column 9 and row 5. The bar is drawn 2
    // to 5 across from where its letter starts and the ring 2 to 12, both 14 tall above the
    // baseline (row 21), with the ring's hole 5 to 9 across and 3 to 11 up.
    let cells = draw(&font("IO", 20), time(0), &[Rgb::WHITE], CANVAS);
    let full = |x: usize, y: usize| cells[y][x].a == 1.0;
    assert!((11..14).all(|x| (7..21).all(|y| full(x, y))));
    assert!((18..28).all(|x| (7..10).all(|y| full(x, y))));
    assert!((21..25).all(|x| (10..18).all(|y| !full(x, y) && cells[y][x].a == 0.0)));
    assert_eq!(lit(&cells), 3 * 14 + 10 * 14 - 4 * 8);
    assert!(cells.iter().flatten().all(|c| c.a == 0.0 || c.a == 1.0));
    assert_eq!(cells[10][12].to_rgb8(), [255, 255, 255]);
    // Several palette colors still go letter by letter.
    let two = draw(&font("IO", 20), time(0), &[Rgb::RED, Rgb::GREEN], CANVAS);
    assert_eq!(
        (two[10][12].to_rgb8(), two[8][20].to_rgb8()),
        ([255, 0, 0], [0, 255, 0])
    );
}

#[test]
fn edges_a_letter_covers_partly_show_dimmer() {
    // At 25 pixels the bar runs 2.5 to 6.25 across: half its first column and a quarter of its
    // last are covered.
    let cells = draw(&font("I", 25), time(0), &[Rgb::WHITE], CANVAS);
    let row = &cells[15];
    let shown: Vec<u8> = row
        .iter()
        .filter(|c| c.a > 0.0)
        .map(|c| (c.a * 255.0).round() as u8)
        .collect();
    assert_eq!(shown, vec![128, 255, 255, 255, 64]);
    let dim = row.iter().find(|c| c.a > 0.0).unwrap();
    assert_eq!(dim.to_rgb8(), [128, 128, 128]);
}

#[test]
fn a_font_this_computer_lacks_is_shown_in_the_pixel_font() {
    let missing = TextParams {
        font: "No Such Font For PixelFlow".into(),
        ..font("HI", 8)
    };
    let pixel = TextParams {
        font: String::new(),
        ..missing.clone()
    };
    let canvas = Canvas {
        columns: 13,
        rows: 10,
    };
    let a = draw(&missing, time(0), &[Rgb::WHITE], canvas);
    assert_eq!(a, draw(&pixel, time(0), &[Rgb::WHITE], canvas));
    assert!(lit(&a) > 0);
}

#[test]
fn bold_and_italic_are_made_up_when_the_family_has_neither() {
    let plain = draw(&font("I", 24), time(0), &[Rgb::WHITE], CANVAS);
    let bold = draw(
        &TextParams {
            bold: true,
            ..font("I", 24)
        },
        time(0),
        &[Rgb::WHITE],
        CANVAS,
    );
    assert_eq!(columns(&bold).len(), columns(&plain).len() + 1);
    let italic = draw(
        &TextParams {
            italic: true,
            ..font("I", 24)
        },
        time(0),
        &[Rgb::WHITE],
        CANVAS,
    );
    // Its top leans right of its foot.
    let first = |row: &Vec<Rgba>| row.iter().position(|c| c.a > 0.5);
    let rows: Vec<usize> = (0..CANVAS.rows as usize)
        .filter(|&y| first(&italic[y]).is_some())
        .collect();
    let (top, foot) = (rows[0], rows[rows.len() - 1]);
    assert!(first(&italic[top]).unwrap() >= first(&italic[foot]).unwrap() + 2);
}

#[test]
fn moving_text_in_a_font_moves_as_text_always_has() {
    // Moving left at speed 10 with 50 ms frames is a cell every 40 ms: 5 cells in 200.
    let moving = TextParams {
        movement: TextMovement::Left,
        ..font("IOI", 16)
    };
    let at = |ms| columns(&draw(&moving, time(ms), &[Rgb::WHITE], CANVAS));
    let (before, after) = (at(1000), at(1200));
    assert!(!before.is_empty() && !after.is_empty());
    assert_eq!(before[0] - after[0], 5);
    // Stopping in the middle stops at the same place it sits still.
    let stopped = TextParams {
        to_center: true,
        ..moving.clone()
    };
    let still = draw(&font("IOI", 16), time(0), &[Rgb::WHITE], CANVAS);
    assert_eq!(draw(&stopped, time(9000), &[Rgb::WHITE], CANVAS), still);
}

#[test]
fn lines_and_stacked_letters_go_one_under_another() {
    // Two lines of 20 rows each (the tiny font asks for no gap between lines).
    let tall = Canvas {
        columns: 20,
        rows: 50,
    };
    let two = draw(&font("I\\nI", 20), time(0), &[Rgb::WHITE], tall);
    let lit_rows: Vec<usize> = (0..50).filter(|&y| two[y].iter().any(|c| c.a > 0.0)).collect();
    assert_eq!(lit_rows.len(), 28);
    assert_eq!(lit_rows[14] - lit_rows[0], 20);
    let stacked = TextParams {
        orientation: TextOrientation::StackedDown,
        ..font("II", 20)
    };
    assert_eq!(draw(&stacked, time(0), &[Rgb::WHITE], tall), two);
}
