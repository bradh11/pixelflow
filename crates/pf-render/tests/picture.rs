//! The Picture effect: a picture file fitted to a grid each way, an animation's frame for the
//! time, clear parts, scrolling and zooming, and that pictures are read once, kept within a
//! budget, and never crash on a file that's missing or isn't a picture. Every picture here is
//! made in the test.

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageFormat, RgbaImage};
use pf_model::{Corner, Generator, MatrixWiring, Orientation, Prop, ShapeSource, Show};
use pf_render::{Canvas, Colors, EffectTime, Pictures, Pixel, RenderContext, Renderer, Rgba, Shade, Shader};
use pf_sequence::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const FRAME_MS: u32 = 25;

type Px = [u8; 4];
const RED: Px = [255, 0, 0, 255];
const GREEN: Px = [0, 255, 0, 255];
const BLUE: Px = [0, 0, 255, 255];
const WHITE: Px = [255, 255, 255, 255];
const BLACK: Px = [0, 0, 0, 255];
const CLEAR: Px = [0, 0, 0, 0];

/// A PNG of `width` × `height` pixels, each from `at(x, y)` with y counting down from the top.
fn png(width: u32, height: u32, at: impl Fn(u32, u32) -> Px) -> Vec<u8> {
    let image = RgbaImage::from_fn(width, height, |x, y| image::Rgba(at(x, y)));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// A 4 × 4 picture with every pixel its own color: red grows across, green grows down. Its
/// bottom right pixel is clear and the one left of it half clear.
fn patchwork(x: u32, y: u32) -> Px {
    let alpha = match (x, y) {
        (3, 3) => 0,
        (2, 3) => 128,
        _ => 255,
    };
    [40 + 60 * x as u8, 40 + 60 * y as u8, 200, alpha]
}

/// A GIF of whole frames in one color each, shown for the times given (the `image` crate's
/// encoder; times are kept to a hundredth of a second).
fn gif(width: u32, height: u32, frames: &[(Px, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for &(color, ms) in frames {
            let picture = RgbaImage::from_pixel(width, height, image::Rgba(color));
            encoder
                .encode_frame(Frame::from_parts(
                    picture,
                    0,
                    0,
                    Delay::from_numer_denom_ms(ms, 1),
                ))
                .unwrap();
        }
    }
    bytes
}

/// Pictures by name, read at once (as an export reads them), and how many times each was read.
fn library(files: Vec<(&str, Vec<u8>)>) -> (Pictures, Arc<AtomicUsize>) {
    let files: HashMap<String, Vec<u8>> = files.into_iter().map(|(n, b)| (n.to_string(), b)).collect();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&reads);
    let pictures = Pictures::new(move |file| {
        count.fetch_add(1, Ordering::SeqCst);
        files.get(file).cloned().ok_or("it isn't there".to_string())
    });
    (pictures.waiting(), reads)
}

/// What a Picture drew on a grid: each cell's color, top row first.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    columns: u32,
    rows: u32,
    cells: Vec<Rgba>,
}

impl Drawn {
    /// The cell `x` across and `row` down from the top, as bytes (color, then coverage).
    fn at(&self, x: u32, row: u32) -> Px {
        let c = self.cells[(row * self.columns + x) as usize];
        let byte = |v: f32| (v * 255.0).round() as u8;
        [byte(c.r), byte(c.g), byte(c.b), byte(c.a)]
    }

    fn rows(&self) -> Vec<Vec<Px>> {
        (0..self.rows)
            .map(|row| (0..self.columns).map(|x| self.at(x, row)).collect())
            .collect()
    }

    fn lit(&self) -> usize {
        self.cells.iter().filter(|c| c.a > 0.0).count()
    }
}

/// The effect drawn on a `columns` × `rows` grid `t_ms` into an effect `length_ms` long.
fn draw_for(
    pictures: &Pictures,
    p: &PictureParams,
    (columns, rows): (u32, u32),
    t_ms: u64,
    length_ms: u64,
    palette: &[Rgb],
) -> Drawn {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    let time = EffectTime::within(0, length_ms, t_ms).with_frame_ms(FRAME_MS);
    let cx = RenderContext::default().with_pictures(pictures);
    let shader = Shader::in_context(
        &EffectParams::Picture(p.clone()),
        &time,
        Colors::new(palette),
        7,
        Canvas { columns, rows },
        &cx,
    );
    let cells = (0..rows)
        .flat_map(|row| (0..columns).map(move |x| (x, row)))
        .map(|(x, row)| {
            shader.shade(&Pixel {
                u: at(x, columns),
                // The grid's top row is the highest.
                v: at(rows - 1 - row, rows),
                index: row * columns + x,
                count: columns * rows,
            })
        })
        .collect();
    Drawn { columns, rows, cells }
}

fn draw(pictures: &Pictures, p: &PictureParams, grid: (u32, u32), t_ms: u64) -> Drawn {
    draw_for(pictures, p, grid, t_ms, 1000, &[])
}

fn of(file: &str) -> PictureParams {
    PictureParams {
        file: file.to_string(),
        crisp: true,
        ..PictureParams::default()
    }
}

#[test]
fn a_picture_is_fitted_filled_stretched_or_shown_pixel_for_pixel() {
    let (pictures, _) = library(vec![("p.png", png(4, 4, patchwork))]);
    // Fitted on a grid twice as wide: its own size, in the middle, with room either side.
    let fit = draw(&pictures, &of("p.png"), (8, 4), 0);
    for row in 0..4 {
        for x in 0..8 {
            let want = if (2..6).contains(&x) {
                patchwork(x - 2, row)
            } else {
                CLEAR
            };
            let want = if want[3] == 0 { CLEAR } else { want };
            assert_eq!(fit.at(x, row), want, "fit at {x}, {row}");
        }
    }
    // Filling it: twice the size, its top and bottom cut off.
    let fill = draw(
        &pictures,
        &PictureParams {
            fit: PictureFit::Fill,
            ..of("p.png")
        },
        (8, 4),
        0,
    );
    for row in 0..4 {
        for x in 0..8 {
            assert_eq!(
                fill.at(x, row),
                patchwork(x / 2, (row + 2) / 2),
                "fill at {x}, {row}"
            );
        }
    }
    // Stretched: twice as wide, no taller.
    let stretch = draw(
        &pictures,
        &PictureParams {
            fit: PictureFit::Stretch,
            ..of("p.png")
        },
        (8, 4),
        0,
    );
    assert_eq!(stretch.at(1, 2), patchwork(0, 2));
    assert_eq!(stretch.at(6, 0), patchwork(3, 0));
    assert_eq!(stretch.at(7, 3), CLEAR, "its clear corner");
    assert_eq!(stretch.lit(), 30);
    // Pixel for pixel on a grid too small for it: the middle shows.
    let actual = PictureParams {
        fit: PictureFit::Actual,
        ..of("p.png")
    };
    let middle = draw(&pictures, &actual, (2, 2), 0);
    assert_eq!(
        middle.rows(),
        vec![
            vec![patchwork(1, 1), patchwork(2, 1)],
            vec![patchwork(1, 2), patchwork(2, 2)]
        ]
    );
    // And on a large one it stays 4 × 4, where fitting would enlarge it.
    assert_eq!(draw(&pictures, &actual, (16, 16), 0).lit(), 15);
    assert_eq!(draw(&pictures, &of("p.png"), (16, 16), 0).lit(), 15 * 16);
    // Scale is on top of the fit: twice the size pixel for pixel fills the 8 × 4 grid as Fill does.
    let doubled = draw(
        &pictures,
        &PictureParams {
            scale: 200.0,
            ..actual.clone()
        },
        (8, 4),
        0,
    );
    assert_eq!(doubled, fill);
    // An odd gap puts the spare cell on the right and at the top, as xLights centers it.
    let odd = draw(&pictures, &actual, (7, 7), 0);
    assert_eq!(odd.at(1, 2), patchwork(0, 0));
    assert_eq!((odd.at(0, 2), odd.at(1, 1)), (CLEAR, CLEAR));
    assert_eq!(
        (odd.at(3, 5), odd.at(4, 5)),
        (patchwork(2, 3), CLEAR),
        "its last row"
    );
}

#[test]
fn offsets_move_it_by_a_share_of_the_prop_or_by_cells() {
    let (pictures, _) = library(vec![("p.png", png(4, 4, patchwork))]);
    // A quarter of 8 columns to the right: two cells.
    let right = draw(
        &pictures,
        &PictureParams {
            x_offset: 25.0,
            ..of("p.png")
        },
        (8, 4),
        0,
    );
    assert_eq!((right.at(3, 0), right.at(4, 0)), (CLEAR, patchwork(0, 0)));
    assert_eq!(right.at(7, 0), patchwork(3, 0));
    // Half of 4 rows up: its lower half shows at the top, the rest is off the prop.
    let up = draw(
        &pictures,
        &PictureParams {
            y_offset: 50.0,
            ..of("p.png")
        },
        (8, 4),
        0,
    );
    assert_eq!((up.at(2, 0), up.at(2, 1)), (patchwork(0, 2), patchwork(0, 3)));
    assert_eq!((up.at(2, 2), up.at(2, 3)), (CLEAR, CLEAR));
    // In cells: one to the left and one down.
    let cells = draw(
        &pictures,
        &PictureParams {
            x_offset: -1.0,
            y_offset: -1.0,
            pixel_offsets: true,
            ..of("p.png")
        },
        (8, 4),
        0,
    );
    assert_eq!(cells.at(1, 1), patchwork(0, 0));
    assert_eq!((cells.at(1, 0), cells.at(0, 1)), (CLEAR, CLEAR));
    assert_eq!(cells.at(4, 3), patchwork(3, 2));
}

#[test]
fn a_large_picture_is_averaged_down_so_a_small_matrix_keeps_its_detail() {
    // 16 × 16 in blocks of four: red, green, blue, and white quarters of each 8 × 8 tile.
    let blocks = |x: u32, y: u32| match ((x / 4) % 2, (y / 4) % 2) {
        (0, 0) => RED,
        (1, 0) => GREEN,
        (0, 1) => BLUE,
        _ => WHITE,
    };
    // One-pixel stripes, black and white: nearest-pixel sampling sees only one of the two.
    let stripes = |x: u32, _: u32| if x.is_multiple_of(2) { WHITE } else { BLACK };
    let (pictures, _) = library(vec![
        ("blocks.png", png(16, 16, blocks)),
        ("stripes.png", png(16, 16, stripes)),
    ]);
    let smooth = |file: &str| PictureParams {
        crisp: false,
        ..of(file)
    };
    // A cell per block: each exactly its block's color.
    let small = draw(&pictures, &smooth("blocks.png"), (4, 4), 0);
    assert_eq!(
        small.rows(),
        vec![
            vec![RED, GREEN, RED, GREEN],
            vec![BLUE, WHITE, BLUE, WHITE],
            vec![RED, GREEN, RED, GREEN],
            vec![BLUE, WHITE, BLUE, WHITE],
        ]
    );
    // A cell per tile: the average of its four blocks.
    let tiny = draw(&pictures, &smooth("blocks.png"), (2, 2), 0);
    assert!(
        tiny.rows().iter().flatten().all(|&c| c == [128, 128, 128, 255]),
        "{tiny:?}"
    );
    // Stripes average to grey; crisp, they're all white (every sample lands on a white pixel).
    let grey = draw(&pictures, &smooth("stripes.png"), (4, 4), 0);
    assert!(
        grey.rows().iter().flatten().all(|&c| c == [128, 128, 128, 255]),
        "{grey:?}"
    );
    let crisp = draw(&pictures, &of("stripes.png"), (4, 4), 0);
    let first = crisp.at(0, 0);
    assert!(first == WHITE || first == BLACK);
    assert!(crisp.rows().iter().flatten().all(|&c| c == first));
    // On a 12 × 50 pillar a wide picture is fitted across and averaged, never cut off.
    let pillar = draw(&pictures, &smooth("blocks.png"), (12, 50), 0);
    assert_eq!(pillar.lit(), 12 * 12);
    assert_eq!((pillar.at(0, 18), pillar.at(0, 31)), (CLEAR, CLEAR));
    assert_ne!(pillar.at(0, 19), CLEAR);
    assert_ne!(pillar.at(11, 30), CLEAR);
}

#[test]
fn enlarging_blends_unless_pixels_are_kept_crisp() {
    let two = |x: u32, _: u32| if x == 0 { BLACK } else { [200, 200, 200, 255] };
    let (pictures, _) = library(vec![("two.png", png(2, 1, two))]);
    let stretch = |crisp| PictureParams {
        fit: PictureFit::Stretch,
        crisp,
        ..of("two.png")
    };
    assert_eq!(
        draw(&pictures, &stretch(true), (4, 1), 0).rows()[0],
        vec![BLACK, BLACK, [200, 200, 200, 255], [200, 200, 200, 255]]
    );
    assert_eq!(
        draw(&pictures, &stretch(false), (4, 1), 0).rows()[0],
        vec![
            BLACK,
            [50, 50, 50, 255],
            [150, 150, 150, 255],
            [200, 200, 200, 255]
        ]
    );
}

#[test]
fn an_animation_shows_the_frame_for_the_time_by_its_own_frame_times() {
    // Red for 100 ms, green for 300, blue for 50.
    let (pictures, _) = library(vec![(
        "a.gif",
        gif(4, 4, &[(RED, 100), (GREEN, 300), (BLUE, 50)]),
    )]);
    let color = |p: &PictureParams, t_ms: u64, length_ms: u64| {
        let drawn = draw_for(&pictures, p, (4, 4), t_ms, length_ms, &[]);
        assert_eq!(drawn.lit(), 16);
        drawn.at(1, 1)
    };
    // Looping at its own speed, whatever the effect's length.
    let looping = of("a.gif");
    for length in [2_000, 60_000] {
        let at = |t| color(&looping, t, length);
        assert_eq!(
            [0, 75, 100, 375, 400, 425].map(at),
            [RED, RED, GREEN, GREEN, BLUE, BLUE]
        );
        assert_eq!([450, 550, 850, 900, 1000].map(at), [RED, GREEN, BLUE, RED, GREEN]);
    }
    // Once, then its last frame stays.
    let once = PictureParams {
        timing: PictureTiming::Once,
        ..of("a.gif")
    };
    let at = |t| color(&once, t, 60_000);
    assert_eq!(
        [0, 100, 400, 450, 475, 30_000].map(at),
        [RED, GREEN, BLUE, BLUE, BLUE, BLUE]
    );
    // Stretched: one pass over the effect, each frame keeping its share (2/9, 6/9, 1/9).
    let stretched = PictureParams {
        timing: PictureTiming::Stretch,
        ..of("a.gif")
    };
    let at = |t| color(&stretched, t, 9_000);
    assert_eq!(
        [0, 1_975, 2_000, 7_975, 8_000, 8_975].map(at),
        [RED, RED, GREEN, GREEN, BLUE, BLUE]
    );
    // Faster: twice its own speed looping, and two passes stretched.
    let fast = PictureParams {
        play_speed: 2.0,
        ..of("a.gif")
    };
    let at = |t| color(&fast, t, 60_000);
    assert_eq!(
        [0, 50, 175, 200, 225, 275].map(at),
        [RED, GREEN, GREEN, BLUE, RED, GREEN]
    );
    let twice = PictureParams {
        play_speed: 2.0,
        ..stretched.clone()
    };
    let at = |t| color(&twice, t, 9_000);
    assert_eq!(
        [0, 1_000, 4_000, 4_500, 5_500, 8_975].map(at),
        [RED, GREEN, BLUE, RED, GREEN, BLUE]
    );
    // Starting on its third frame.
    let later = PictureParams {
        start_frame: 3,
        ..of("a.gif")
    };
    let at = |t| color(&later, t, 60_000);
    assert_eq!([0, 25, 50, 150, 450].map(at), [BLUE, BLUE, RED, GREEN, BLUE]);
    // A still picture is the same whatever the animation settings say.
    let (stills, _) = library(vec![("p.png", png(4, 4, patchwork))]);
    let still = draw(&stills, &of("p.png"), (4, 4), 0);
    for timing in [PictureTiming::Once, PictureTiming::Stretch] {
        let p = PictureParams {
            timing,
            play_speed: 3.0,
            start_frame: 9,
            ..of("p.png")
        };
        assert_eq!(draw(&stills, &p, (4, 4), 777), still);
    }
}

/// A patch of a GIF's frame: where it goes (left, top), its size (width, height), its palette
/// indexes (0 red, 1 green, 2 blue, 3 clear), and how it's cleared afterwards.
type Patch<'a> = (u16, u16, u16, u16, &'a [u8], gif::DisposalMethod);

/// A GIF written frame by frame with the `gif` crate, so frames can be patches that leave the
/// rest of the picture as it was. Each shows for a tenth of a second.
fn patched_gif(size: u16, frames: &[Patch]) -> Vec<u8> {
    let palette = [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9];
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, size, size, &palette).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for &(left, top, width, height, indexes, dispose) in frames {
            let mut frame = gif::Frame::from_indexed_pixels(width, height, indexes.to_vec(), Some(3));
            frame.left = left;
            frame.top = top;
            frame.dispose = dispose;
            frame.delay = 10;
            encoder.write_frame(&frame).unwrap();
        }
    }
    bytes
}

#[test]
fn a_gifs_frames_are_composed_as_its_file_says() {
    use gif::DisposalMethod::{Background, Keep, Previous};
    let bytes = patched_gif(
        4,
        &[
            // All red, kept.
            (0, 0, 4, 4, &[0; 16], Keep),
            // A green 2 × 2 patch in the middle with one clear pixel, kept: red shows around and
            // through it.
            (1, 1, 2, 2, &[1, 1, 1, 3], Keep),
            // A blue pixel in the corner, put back to how it was afterwards.
            (0, 0, 1, 1, &[2], Previous),
            // A blue row along the bottom, cleared afterwards.
            (0, 3, 4, 1, &[2, 2, 2, 2], Background),
            // Nothing new: a clear pixel.
            (3, 0, 1, 1, &[3], Keep),
        ],
    );
    let (pictures, _) = library(vec![("patches.gif", bytes)]);
    let frame = |n: u64| draw(&pictures, &of("patches.gif"), (4, 4), n * 100).rows();
    assert_eq!(frame(0), vec![vec![RED; 4]; 4]);
    let patched = vec![
        vec![RED; 4],
        vec![RED, GREEN, GREEN, RED],
        vec![RED, GREEN, RED, RED],
        vec![RED; 4],
    ];
    assert_eq!(frame(1), patched);
    let mut cornered = patched.clone();
    cornered[0][0] = BLUE;
    assert_eq!(frame(2), cornered);
    // The corner is back; the bottom row is blue.
    let mut bottom = patched.clone();
    bottom[3] = vec![BLUE; 4];
    assert_eq!(frame(3), bottom);
    // The bottom row was cleared, not put back.
    let mut cleared = patched.clone();
    cleared[3] = vec![CLEAR; 4];
    assert_eq!(frame(4), cleared);
    // And it loops to all red.
    assert_eq!(frame(5), vec![vec![RED; 4]; 4]);
}

#[test]
fn clear_parts_stay_clear_and_black_can_be_made_clear() {
    let shapes = |x: u32, y: u32| match (x, y) {
        (0, _) => BLACK,
        (1, _) => [6, 6, 6, 255],
        (2, 0) => CLEAR,
        (2, _) => [0, 200, 0, 128],
        _ => RED,
    };
    let (pictures, _) = library(vec![("s.png", png(4, 2, shapes))]);
    // As it is: black is drawn black (it covers), half-clear is half-clear, clear is clear.
    let plain = draw(&pictures, &of("s.png"), (4, 2), 0);
    assert_eq!(
        plain.rows(),
        vec![
            vec![BLACK, [6, 6, 6, 255], CLEAR, RED],
            vec![BLACK, [6, 6, 6, 255], [0, 200, 0, 128], RED],
        ]
    );
    // Black made clear: only true black, until the level says how dark still counts.
    let keyed = PictureParams {
        black_transparent: true,
        ..of("s.png")
    };
    assert_eq!(
        draw(&pictures, &keyed, (4, 2), 0).rows()[1],
        vec![CLEAR, [6, 6, 6, 255], [0, 200, 0, 128], RED]
    );
    let darker = PictureParams {
        black_level: 3.0,
        ..keyed.clone()
    };
    assert_eq!(
        draw(&pictures, &darker, (4, 2), 0).rows()[1],
        vec![CLEAR, CLEAR, [0, 200, 0, 128], RED]
    );
    // The level alone does nothing.
    let level_only = PictureParams {
        black_level: 50.0,
        ..of("s.png")
    };
    assert_eq!(draw(&pictures, &level_only, (4, 2), 0), plain);
    // Shrunk, a cell half black and half red is red at half coverage, not dark red.
    let halves = |x: u32, _: u32| if x == 0 { BLACK } else { RED };
    let (pictures, _) = library(vec![("h.png", png(2, 2, halves))]);
    let smooth = PictureParams {
        crisp: false,
        black_transparent: true,
        ..of("h.png")
    };
    assert_eq!(draw(&pictures, &smooth, (1, 1), 0).at(0, 0), [255, 0, 0, 128]);
}

#[test]
fn a_scrolling_picture_crosses_the_prop_and_comes_round_when_it_wraps() {
    let (pictures, _) = library(vec![("p.png", png(4, 4, patchwork))]);
    // One trip a second: from just off one side to just off the other, 4 + 8 cells.
    let scroll = |movement, wrap| PictureParams {
        movement,
        wrap,
        move_speed: 1.0,
        fit: PictureFit::Actual,
        ..of("p.png")
    };
    let left = scroll(PictureMovement::Left, false);
    let columns = |p: &PictureParams, t: u64| -> Vec<Option<u32>> {
        // Which of the picture's columns shows in each grid column (by its top row's red).
        let drawn = draw_for(&pictures, p, (8, 4), t, 60_000, &[]);
        (0..8)
            .map(|x| {
                let c = drawn.at(x, 0);
                (c[3] > 0).then(|| (u32::from(c[0]) - 40) / 60)
            })
            .collect()
    };
    let s = Some;
    assert_eq!(columns(&left, 0), vec![None; 8], "it starts just off the right");
    assert_eq!(
        columns(&left, 250),
        vec![None, None, None, None, None, s(0), s(1), s(2)]
    );
    assert_eq!(
        columns(&left, 500),
        vec![None, None, s(0), s(1), s(2), s(3), None, None]
    );
    assert_eq!(
        columns(&left, 975),
        vec![s(3), None, None, None, None, None, None, None]
    );
    assert_eq!(columns(&left, 1000), vec![None; 8], "and crosses again");
    assert_eq!(columns(&left, 1250), columns(&left, 250));
    // To the right, it comes in from the left.
    let right = scroll(PictureMovement::Right, false);
    assert_eq!(
        columns(&right, 250),
        vec![s(1), s(2), s(3), None, None, None, None, None]
    );
    // Wrapping, what leaves the left edge comes back on the right, and it never runs out.
    let wrapped = scroll(PictureMovement::Left, true);
    assert_eq!(
        columns(&wrapped, 500),
        vec![None, None, s(0), s(1), s(2), s(3), None, None]
    );
    assert_eq!(
        columns(&wrapped, 1000),
        vec![None, None, None, None, s(0), s(1), s(2), s(3)]
    );
    assert_eq!(
        columns(&wrapped, 1500),
        vec![s(2), s(3), None, None, None, None, s(0), s(1)]
    );
    assert!(
        (0..200).all(|k| columns(&wrapped, k * 137).iter().flatten().count() == 4),
        "all of it shows at every moment"
    );
    // A still picture pushed off the edge comes back too, when it wraps.
    let pushed = PictureParams {
        wrap: true,
        x_offset: 3.0,
        pixel_offsets: true,
        fit: PictureFit::Actual,
        ..of("p.png")
    };
    assert_eq!(
        columns(&pushed, 0),
        vec![s(3), None, None, None, None, s(0), s(1), s(2)]
    );
    // Up and down: the rows that show (by the left column's green).
    let rows = |p: &PictureParams, t: u64| -> Vec<Option<u32>> {
        let drawn = draw_for(&pictures, p, (8, 4), t, 60_000, &[]);
        (0..4)
            .map(|row| {
                let c = drawn.at(2, row);
                (c[3] > 0).then(|| (u32::from(c[1]) - 40) / 60)
            })
            .collect()
    };
    let up = scroll(PictureMovement::Up, false);
    assert_eq!(rows(&up, 0), vec![None; 4], "it starts just below");
    assert_eq!(rows(&up, 250), vec![None, None, s(0), s(1)]);
    assert_eq!(rows(&up, 500), vec![s(0), s(1), s(2), s(3)]);
    assert_eq!(rows(&up, 750), vec![s(2), s(3), None, None]);
    let down = scroll(PictureMovement::Down, false);
    assert_eq!(rows(&down, 250), vec![s(2), s(3), None, None]);
    assert_eq!(rows(&down, 750), vec![None, None, s(0), s(1)]);
    // Twice as fast is twice as far.
    let fast = PictureParams {
        move_speed: 2.0,
        ..left.clone()
    };
    assert_eq!(columns(&fast, 250), columns(&left, 500));
    // No speed: it waits at the start.
    let stopped = PictureParams {
        move_speed: 0.0,
        ..left
    };
    assert_eq!(columns(&stopped, 5_000), vec![None; 8]);
}

#[test]
fn it_zooms_in_and_out_over_the_effect_and_pans_across_what_overhangs() {
    let (pictures, _) = library(vec![
        ("p.png", png(4, 4, |_, _| WHITE)),
        ("wide.png", png(8, 4, |x, _| [40 + 20 * x as u8, 0, 0, 255])),
    ]);
    let zoom = |movement| PictureParams {
        movement,
        ..of("p.png")
    };
    let size = |p: &PictureParams, t: u64| draw_for(&pictures, p, (8, 8), t, 1000, &[]).lit();
    let zoom_in = zoom(PictureMovement::ZoomIn);
    // From nothing to its fitted size (the whole 8 × 8 grid) at the effect's last frame.
    assert_eq!(size(&zoom_in, 0), 0);
    assert_eq!(size(&zoom_in, 975), 64);
    let sizes: Vec<usize> = (0..40).map(|k| size(&zoom_in, k * 25)).collect();
    assert!(sizes.windows(2).all(|w| w[0] <= w[1]), "{sizes:?}");
    // Halfway it's 4 × 4, in the middle.
    let half = draw_for(&pictures, &zoom_in, (8, 8), 500, 1000, &[]);
    assert_eq!(half.lit(), 16);
    assert!((2..6).all(|i| half.at(i, 2) == WHITE && half.at(i, 5) == WHITE));
    // Zooming out is the same backwards.
    let zoom_out = zoom(PictureMovement::ZoomOut);
    assert_eq!((size(&zoom_out, 0), size(&zoom_out, 975)), (64, 0));
    assert_eq!(size(&zoom_out, 475), size(&zoom_in, 500));
    // A slow pan: filling a 4 × 4 grid, the 8 × 4 picture overhangs by four columns, and the
    // grid slides from its left end to its right end.
    let pan = PictureParams {
        movement: PictureMovement::Pan,
        fit: PictureFit::Fill,
        ..of("wide.png")
    };
    let first_column =
        |t: u64| (u32::from(draw_for(&pictures, &pan, (4, 4), t, 1000, &[]).at(0, 0)[0]) - 40) / 20;
    assert_eq!((first_column(0), first_column(975)), (0, 4));
    let seen: Vec<u32> = (0..40).map(|k| first_column(k * 25)).collect();
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert_eq!(seen[20], 2);
    // With nothing overhanging there's nowhere to pan: it stays put.
    let fitted = PictureParams {
        fit: PictureFit::Fit,
        ..pan.clone()
    };
    assert_eq!(
        draw_for(&pictures, &fitted, (4, 4), 0, 1000, &[]),
        draw_for(&pictures, &fitted, (4, 4), 900, 1000, &[])
    );
}

#[test]
fn it_turns_by_quarter_turns_and_takes_the_palettes_tint_when_asked() {
    // Red, green, blue left to right.
    let stripe = |x: u32, _: u32| [RED, GREEN, BLUE][x as usize];
    let (pictures, _) = library(vec![("rgb.png", png(3, 1, stripe))]);
    let turned = |turn| PictureParams {
        turn,
        fit: PictureFit::Actual,
        ..of("rgb.png")
    };
    assert_eq!(
        draw(&pictures, &turned(PictureTurn::None), (3, 3), 0).rows(),
        vec![vec![CLEAR; 3], vec![RED, GREEN, BLUE], vec![CLEAR; 3]]
    );
    // A quarter turn to the right: its left end is now at the top.
    assert_eq!(
        draw(&pictures, &turned(PictureTurn::Right), (3, 3), 0).rows(),
        vec![
            vec![CLEAR, RED, CLEAR],
            vec![CLEAR, GREEN, CLEAR],
            vec![CLEAR, BLUE, CLEAR]
        ]
    );
    assert_eq!(
        draw(&pictures, &turned(PictureTurn::Half), (3, 3), 0).rows()[1],
        vec![BLUE, GREEN, RED]
    );
    assert_eq!(
        draw(&pictures, &turned(PictureTurn::Left), (3, 3), 0).rows(),
        vec![
            vec![CLEAR, BLUE, CLEAR],
            vec![CLEAR, GREEN, CLEAR],
            vec![CLEAR, RED, CLEAR]
        ]
    );
    // Turned on its side, a wide picture fits a tall prop.
    let tall = PictureParams {
        turn: PictureTurn::Right,
        ..of("rgb.png")
    };
    let drawn = draw(&pictures, &tall, (2, 6), 0);
    assert_eq!(drawn.lit(), 12);
    assert_eq!(
        (drawn.at(0, 0), drawn.at(1, 3), drawn.at(0, 5)),
        (RED, GREEN, BLUE)
    );
    // Its own colors unless it's tinted: then each is dimmed by the first palette color.
    let palette = [Rgb::new(255, 128, 0)];
    let plain = draw_for(&pictures, &turned(PictureTurn::None), (3, 1), 0, 1000, &palette);
    assert_eq!(plain.rows()[0], vec![RED, GREEN, BLUE]);
    let tinted = PictureParams {
        tint: true,
        ..turned(PictureTurn::None)
    };
    let drawn = draw_for(&pictures, &tinted, (3, 1), 0, 1000, &palette);
    assert_eq!(drawn.rows()[0], vec![RED, [0, 128, 0, 255], [0, 0, 0, 255]]);
}

#[test]
fn the_same_time_gives_the_same_picture_in_any_order() {
    let (pictures, _) = library(vec![
        (
            "a.gif",
            gif(6, 6, &[(RED, 70), (GREEN, 130), (BLUE, 90), (WHITE, 40)]),
        ),
        ("p.png", png(4, 4, patchwork)),
    ]);
    let moving = [
        PictureParams {
            movement: PictureMovement::Left,
            wrap: true,
            move_speed: 0.7,
            ..of("a.gif")
        },
        PictureParams {
            movement: PictureMovement::ZoomIn,
            crisp: false,
            ..of("p.png")
        },
        PictureParams {
            timing: PictureTiming::Stretch,
            movement: PictureMovement::Up,
            turn: PictureTurn::Left,
            ..of("a.gif")
        },
    ];
    for p in &moving {
        let first: Vec<Drawn> = (0..60)
            .map(|k| draw_for(&pictures, p, (9, 7), k * 131, 8_000, &[]))
            .collect();
        assert!(first.windows(2).any(|w| w[0] != w[1]), "it moves");
        // Again, backwards, and with other pictures drawn in between: no frame leans on another.
        for k in (0..60).rev() {
            draw(&pictures, &moving[(k as usize) % 3], (5, 5), k * 17);
            assert_eq!(
                draw_for(&pictures, p, (9, 7), k * 131, 8_000, &[]),
                first[k as usize]
            );
        }
        // A library of its own gives the same frames: nothing depends on what was read before.
        let (fresh, _) = library(vec![
            (
                "a.gif",
                gif(6, 6, &[(RED, 70), (GREEN, 130), (BLUE, 90), (WHITE, 40)]),
            ),
            ("p.png", png(4, 4, patchwork)),
        ]);
        for k in [41, 3, 59, 20] {
            assert_eq!(
                draw_for(&fresh, p, (9, 7), k * 131, 8_000, &[]),
                first[k as usize]
            );
        }
    }
}

#[test]
fn a_picture_is_read_once_and_pictures_are_kept_within_a_budget() {
    // Reading is counted: drawing the same picture again, at any time, size, or setting, never
    // reads its file again.
    let (pictures, reads) = library(vec![
        ("a.gif", gif(8, 8, &[(RED, 100), (GREEN, 100), (BLUE, 100)])),
        ("p.png", png(4, 4, patchwork)),
    ]);
    for k in 0..50 {
        draw(&pictures, &of("a.gif"), (8, 8), k * 40);
        draw(
            &pictures,
            &PictureParams {
                scale: 50.0 + k as f32,
                movement: PictureMovement::Left,
                ..of("a.gif")
            },
            (8, 8),
            k * 40,
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    draw(&pictures, &of("p.png"), (8, 8), 0);
    draw(&pictures, &of("a.gif"), (8, 8), 0);
    assert_eq!(reads.load(Ordering::SeqCst), 2);

    // A budget of 64 KB of frames: each 64 × 64 picture takes 16 KB (the most one may), so
    // four are kept and a fifth pushes the one drawn longest ago out.
    let files: Vec<(String, Vec<u8>)> = (0..8u8)
        .map(|i| {
            (
                format!("{i}.png"),
                png(64, 64, move |x, y| [x as u8, y as u8, i, 255]),
            )
        })
        .collect();
    let reads = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = Arc::clone(&reads);
    let pictures = Pictures::with_budget(
        move |file| {
            log.lock().unwrap().push(file.to_string());
            files
                .iter()
                .find(|(name, _)| name == file)
                .map(|(_, bytes)| bytes.clone())
                .ok_or("it isn't there".to_string())
        },
        64 * 1024,
    )
    .waiting();
    let show = |i: u8| {
        let p = PictureParams {
            fit: PictureFit::Actual,
            ..of(&format!("{i}.png"))
        };
        let drawn = draw(&pictures, &p, (64, 64), 0);
        assert_eq!(drawn.at(10, 20), [10, 20, i, 255], "picture {i}");
        assert!(
            pictures.kept_bytes() <= 64 * 1024 + 16 * 1024,
            "{}",
            pictures.kept_bytes()
        );
    };
    for i in 0..4 {
        show(i);
    }
    assert_eq!(reads.lock().unwrap().len(), 4);
    // All four are still there.
    for i in 0..4 {
        show(i);
    }
    assert_eq!(reads.lock().unwrap().len(), 4);
    // Four more: the first four are pushed out one by one, and the total never passes the budget.
    for i in 4..8 {
        show(i);
    }
    assert_eq!(reads.lock().unwrap().len(), 8);
    // The last four are kept; the first is read again when it's drawn again.
    for i in 4..8 {
        show(i);
    }
    assert_eq!(reads.lock().unwrap().len(), 8);
    show(0);
    assert_eq!(reads.lock().unwrap().last().map(String::as_str), Some("0.png"));
    assert_eq!(reads.lock().unwrap().len(), 9);
}

#[test]
fn a_long_animation_is_kept_small_for_the_prop_it_is_on() {
    // 200 frames of 128 × 128 would be 13 MB at full size. For a 64 × 32 grid they're kept at
    // 32 × 32 (fitted, it's 32 tall): under a megabyte.
    let frames: Vec<(Px, u32)> = (0..200u32)
        .map(|i| ([i as u8, 255 - i as u8, 99, 255], 40))
        .collect();
    let (pictures, reads) = library(vec![("long.gif", gif(128, 128, &frames))]);
    let p = PictureParams {
        crisp: false,
        ..of("long.gif")
    };
    let first = draw_for(&pictures, &p, (64, 32), 0, 60_000, &[]);
    assert_eq!(first.lit(), 32 * 32);
    let kept = pictures.kept_bytes();
    assert!((200 * 32 * 32 * 4..1_000_000).contains(&kept), "{kept} bytes");
    // Every frame is there, at its time.
    let reds: Vec<u8> = (0..200u64)
        .map(|i| draw_for(&pictures, &p, (64, 32), i * 40, 60_000, &[]).at(20, 10)[0])
        .collect();
    assert_eq!(reds, (0..200).collect::<Vec<u8>>());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(pictures.size("long.gif"), Some((128, 128)));
}

#[test]
fn a_file_that_is_missing_or_is_not_a_picture_draws_nothing() {
    let (pictures, reads) = library(vec![
        ("notes.png", b"Dear Santa, this is not a picture.".to_vec()),
        ("cut.png", png(8, 8, patchwork_any)[..40].to_vec()),
        ("empty.gif", Vec::new()),
        ("p.png", png(4, 4, patchwork)),
    ]);
    for file in ["gone.gif", "notes.png", "cut.png", "empty.gif"] {
        let drawn = draw(&pictures, &of(file), (8, 8), 0);
        assert_eq!(drawn.lit(), 0, "{file}");
        let why = pictures.problem(file).unwrap_or_default();
        assert!(!why.is_empty(), "{file}");
    }
    assert_eq!(pictures.problem("gone.gif").as_deref(), Some("it isn't there"));
    assert_eq!(pictures.problem("p.png"), None, "not looked at yet");
    // A file that couldn't be read isn't tried again with every frame.
    let before = reads.load(Ordering::SeqCst);
    for t in 0..20 {
        draw(&pictures, &of("gone.gif"), (8, 8), t * 25);
    }
    assert_eq!(reads.load(Ordering::SeqCst), before);
    // Until it's forgotten (it came back, or was chosen again).
    pictures.forget("gone.gif");
    draw(&pictures, &of("gone.gif"), (8, 8), 0);
    assert_eq!(reads.load(Ordering::SeqCst), before + 1);
    // An effect with no picture chosen reads nothing and draws nothing.
    let before = reads.load(Ordering::SeqCst);
    assert_eq!(draw(&pictures, &of(""), (8, 8), 0).lit(), 0);
    assert_eq!(draw(&pictures, &of("   "), (8, 8), 0).lit(), 0);
    assert_eq!(reads.load(Ordering::SeqCst), before);
    // Without any pictures at all (a renderer no one gave any), nothing is drawn either.
    assert_eq!(draw(&Pictures::none(), &of("p.png"), (8, 8), 0).lit(), 0);
    // And the good one still draws.
    assert_eq!(draw(&pictures, &of("p.png"), (4, 4), 0).lit(), 15);
    assert_eq!(pictures.size("p.png"), Some((4, 4)));
}

fn patchwork_any(x: u32, y: u32) -> Px {
    [x as u8 * 30, y as u8 * 30, 0, 255]
}

#[test]
fn a_preview_draws_without_a_picture_until_it_has_been_read() {
    let files: HashMap<String, Vec<u8>> = [("p.png".to_string(), png(4, 4, patchwork))].into();
    let pictures = Pictures::new(move |file| files.get(file).cloned().ok_or("it isn't there".to_string()));
    let arrived = Arc::new(AtomicUsize::new(0));
    let told = Arc::clone(&arrived);
    pictures.on_arrival(Some(Arc::new(move || {
        told.fetch_add(1, Ordering::SeqCst);
    })));
    // The first frame doesn't wait for the file: it's read on a thread of its own.
    assert_eq!(draw(&pictures, &of("p.png"), (4, 4), 0).lit(), 0);
    for _ in 0..500 {
        if arrived.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        arrived.load(Ordering::SeqCst),
        1,
        "whoever shows frames is told once it's there"
    );
    let drawn = draw(&pictures, &of("p.png"), (4, 4), 0);
    assert_eq!(drawn.lit(), 15);
    // The same pictures, waited for, are the same pictures: nothing is read twice.
    assert_eq!(draw(&pictures.waiting(), &of("p.png"), (4, 4), 0), drawn);
    // A missing one tells too, so a "couldn't read it" note can show.
    draw(&pictures, &of("gone.png"), (4, 4), 0);
    for _ in 0..500 {
        if arrived.load(Ordering::SeqCst) > 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(pictures.problem("gone.png").as_deref(), Some("it isn't there"));
}

/// A 4 × 6 pillar wired up one strand and down the next, like the user's 12 × 50 ones.
fn pillar_show() -> Show {
    let pillar = Prop::new(
        "Pillar",
        ShapeSource::Generator(Generator::Matrix {
            columns: 4,
            rows: 6,
            width: 0.4,
            height: 0.6,
            wiring: MatrixWiring {
                start: Corner::BottomLeft,
                orientation: Orientation::Vertical,
                serpentine: true,
            },
        }),
    );
    let mut show = Show::new("t");
    show.props.push(pillar);
    show
}

#[test]
fn it_renders_on_a_matrix_prop_over_the_layers_below() {
    let show = pillar_show();
    let (pictures, _) = library(vec![
        ("p.png", png(4, 4, patchwork)),
        ("a.gif", gif(4, 6, &[(RED, 100), (GREEN, 100)])),
    ]);
    let mut seq = Sequence::new("s", 4_000);
    seq.frame_ms = FRAME_MS;
    let mut row = Row::new(Target::Prop(show.props[0].id));
    // A dim blue wash below; the picture above it, pixel for pixel.
    row.layers[0].effects = vec![Effect::new(EffectKind::On, 0, 4_000).with_palette([Rgb::new(0, 0, 100)])];
    let p = PictureParams {
        fit: PictureFit::Actual,
        ..of("p.png")
    };
    row.layers.push(Layer {
        effects: vec![
            Effect::new(EffectKind::Picture, 0, 2_000).with_params(EffectParams::Picture(p.clone())),
            Effect::new(EffectKind::Picture, 2_000, 4_000).with_params(EffectParams::Picture(of("a.gif"))),
        ],
    });
    seq.rows.push(row);
    assert_eq!(validate_sequence(&seq, &show), vec![]);
    let mut renderer = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    renderer.set_pictures(pictures.clone());
    let mut frame = vec![0u8; renderer.frame_len()];
    renderer.render(&seq, 500, &mut frame);
    // The prop's pixel at a column and a row down from the top.
    let pixel = |frame: &[u8], x: usize, row: usize| -> [u8; 3] {
        let up = 5 - row;
        let n = x * 6 + if x.is_multiple_of(2) { up } else { 5 - up };
        [frame[n * 3], frame[n * 3 + 1], frame[n * 3 + 2]]
    };
    let drawn = draw(&pictures, &p, (4, 6), 500);
    assert_eq!(drawn.at(0, 1), patchwork(0, 0), "4 × 4 in the middle of 4 × 6");
    for row in 0..6 {
        for x in 0..4 {
            let [r, g, b, a] = drawn.at(x as u32, row as u32);
            // Covered: the picture. Clear: the wash. Half clear: half of each.
            let want = match a {
                255 => [r, g, b],
                0 => [0, 0, 100],
                _ => [r / 2, g / 2, b / 2 + 50],
            };
            let got = pixel(&frame, x, row);
            assert!(
                got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 1),
                "at {x}, {row}: {got:?}, not {want:?}"
            );
        }
    }
    assert_eq!(
        pixel(&frame, 0, 0),
        [0, 0, 100],
        "the wash shows above the picture"
    );
    assert_eq!(pixel(&frame, 3, 4), [0, 0, 100], "and through its clear corner");
    // The animation on the same prop: red, then green, the same at any time asked for twice.
    let mut at = |t: u64| {
        let mut frame = vec![0u8; renderer.frame_len()];
        renderer.render(&seq, t, &mut frame);
        frame
    };
    let (red, green) = (at(2_050), at(2_150));
    assert!(red.chunks(3).all(|px| px == [255, 0, 0]));
    assert!(green.chunks(3).all(|px| px == [0, 255, 0]));
    assert_eq!(at(2_050), red);
    // A renderer no one gave pictures draws the wash alone, and one whose picture is gone too.
    let mut bare = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    let mut frame = vec![0u8; bare.frame_len()];
    bare.render(&seq, 500, &mut frame);
    assert!(frame.chunks(3).all(|px| px == [0, 0, 100]));
    let point_at = |seq: &mut Sequence, file: &str| {
        let EffectParams::Picture(p) = &mut seq.rows[0].layers[1].effects[0].params else {
            unreachable!()
        };
        p.file = file.to_string();
    };
    point_at(&mut seq, "images/gone.png");
    renderer.render(&seq, 500, &mut frame);
    assert!(frame.chunks(3).all(|px| px == [0, 0, 100]));
    // An effect with no picture chosen says so.
    point_at(&mut seq, "");
    let issues = validate_sequence(&seq, &show);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0].message.contains("Picture") && issues[0].message.contains("no picture"),
        "{}",
        issues[0].message
    );
}

/// A chunk of a RIFF file: its name, its size, its bytes, and a byte to make them even.
fn chunk(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// An animated WebP of whole frames in one color each, put together from the `image` crate's
/// still (lossless) WebPs, which it can write but not animate.
fn animated_webp(size: u32, frames: &[(Px, u32)]) -> Vec<u8> {
    let u24 = |v: u32| v.to_le_bytes()[..3].to_vec();
    let mut body = b"WEBP".to_vec();
    // Animated, with clear parts; the canvas size, less one.
    body.extend(chunk(
        b"VP8X",
        &[vec![0x12, 0, 0, 0], u24(size - 1), u24(size - 1)].concat(),
    ));
    // Clear behind; loop for ever.
    body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
    for &(color, ms) in frames {
        let picture = RgbaImage::from_pixel(size, size, image::Rgba(color));
        let mut still = Cursor::new(Vec::new());
        picture.write_to(&mut still, ImageFormat::WebP).unwrap();
        let still = still.into_inner();
        // Past "RIFF", its size, and "WEBP": the picture's own chunk.
        let picture = &still[12..];
        assert_eq!(&picture[..4], b"VP8L");
        // Where it goes, its size less one, its time, and "takes the place of what's there".
        let mut frame = [u24(0), u24(0), u24(size - 1), u24(size - 1), u24(ms), vec![2]].concat();
        frame.extend(picture);
        body.extend(chunk(b"ANMF", &frame));
    }
    let mut file = b"RIFF".to_vec();
    file.extend((body.len() as u32).to_le_bytes());
    file.extend(body);
    file
}

#[test]
fn stills_of_every_kind_and_animated_webps_are_read() {
    let picture = RgbaImage::from_fn(4, 4, |x, y| image::Rgba(patchwork(x, y)));
    let encoded = |format: ImageFormat| {
        let mut bytes = Cursor::new(Vec::new());
        match format {
            // JPEG has no clear parts.
            ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(picture.clone())
                .into_rgb8()
                .write_to(&mut bytes, format)
                .unwrap(),
            _ => picture.write_to(&mut bytes, format).unwrap(),
        }
        bytes.into_inner()
    };
    let (pictures, _) = library(vec![
        ("p.png", encoded(ImageFormat::Png)),
        ("p.bmp", encoded(ImageFormat::Bmp)),
        ("p.webp", encoded(ImageFormat::WebP)),
        ("p.jpg", encoded(ImageFormat::Jpeg)),
        // Named for what it isn't: the contents decide.
        ("really-a.gif", encoded(ImageFormat::Png)),
        ("still.gif", gif(4, 4, &[(GREEN, 100)])),
        (
            "a.webp",
            animated_webp(4, &[(RED, 100), ([0, 0, 255, 128], 300), (GREEN, 50)]),
        ),
    ]);
    let png = draw(&pictures, &of("p.png"), (4, 4), 0);
    assert_eq!(png.at(1, 2), patchwork(1, 2));
    for exact in ["p.bmp", "p.webp", "really-a.gif"] {
        assert_eq!(draw(&pictures, &of(exact), (4, 4), 0), png, "{exact}");
    }
    // A JPEG is close (its blue is the same everywhere), and covers everywhere.
    let jpg = draw(&pictures, &of("p.jpg"), (4, 4), 0);
    assert_eq!(jpg.lit(), 16);
    assert!(
        jpg.rows().iter().flatten().all(|c| c[2].abs_diff(200) < 40),
        "{jpg:?}"
    );
    assert_eq!(draw(&pictures, &of("still.gif"), (4, 4), 5_000).at(0, 0), GREEN);
    // An animated WebP plays by its own frame times, clear parts and all.
    let at = |t: u64| draw_for(&pictures, &of("a.webp"), (4, 4), t, 60_000, &[]).at(2, 2);
    assert_eq!(
        [0, 75, 100, 375, 400, 425, 450].map(at),
        [RED, RED, [0, 0, 255, 128], [0, 0, 255, 128], GREEN, GREEN, RED]
    );
}

#[test]
fn renderers_that_wait_share_one_reading_and_a_preview_keeps_what_it_has_meanwhile() {
    // Eight export threads draw the same picture at once: it's read once, and all of them get it.
    let bytes = png(4, 4, patchwork);
    let reads = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&reads);
    let pictures = Pictures::new(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(60));
        Ok(bytes.clone())
    });
    let lit: Vec<usize> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| draw(&pictures.waiting(), &of("p.png"), (4, 4), 0).lit()))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    assert_eq!(lit, [15; 8]);
    assert_eq!(reads.load(Ordering::SeqCst), 1);

    // A preview whose picture is wanted larger (fitted, then filling) draws the frames it has
    // until the larger ones are read, rather than going dark.
    let wide = png(
        64,
        16,
        |x, _| if (x / 2).is_multiple_of(2) { WHITE } else { BLACK },
    );
    let arrived = Arc::new(AtomicUsize::new(0));
    let told = Arc::clone(&arrived);
    let pictures = Pictures::new(move |_| Ok(wide.clone()));
    pictures.on_arrival(Some(Arc::new(move || {
        told.fetch_add(1, Ordering::SeqCst);
    })));
    let wait_for = |n: usize| {
        for _ in 0..500 {
            if arrived.load(Ordering::SeqCst) >= n {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the picture never came");
    };
    let smooth = |fit| PictureParams {
        fit,
        crisp: false,
        ..of("wide.png")
    };
    assert_eq!(draw(&pictures, &smooth(PictureFit::Fit), (8, 8), 0).lit(), 0);
    wait_for(1);
    assert_eq!(draw(&pictures, &smooth(PictureFit::Fit), (8, 8), 0).lit(), 16);
    let meanwhile = draw(&pictures, &smooth(PictureFit::Fill), (8, 8), 0);
    assert_eq!(meanwhile.lit(), 64, "drawn at once, from the frames it has");
    wait_for(2);
    let sharp = draw(&pictures, &smooth(PictureFit::Fill), (8, 8), 0);
    assert_eq!(
        sharp,
        draw(&pictures.waiting(), &smooth(PictureFit::Fill), (8, 8), 0)
    );
    assert_ne!(sharp, meanwhile, "then from the ones read for its new size");
}

#[test]
fn a_sequences_pictures_are_read_ahead_so_playing_finds_them_ready() {
    let show = pillar_show();
    let names = ["a.png", "b.png", "c.png"];
    let colors = [RED, GREEN, BLUE];
    let files: HashMap<String, Vec<u8>> = names
        .iter()
        .zip(colors)
        .map(|(name, color)| (name.to_string(), png(4, 6, move |_, _| color)))
        .collect();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&reads);
    let pictures = Pictures::new(move |file| {
        count.fetch_add(1, Ordering::SeqCst);
        files.get(file).cloned().ok_or("it isn't there".to_string())
    });
    let arrived = Arc::new(AtomicUsize::new(0));
    let told = Arc::clone(&arrived);
    pictures.on_arrival(Some(Arc::new(move || {
        told.fetch_add(1, Ordering::SeqCst);
    })));
    // Three pictures, one after another, and one that isn't there.
    let mut seq = Sequence::new("s", 4_000);
    seq.frame_ms = FRAME_MS;
    let mut row = Row::new(Target::Prop(show.props[0].id));
    row.layers[0].effects = names
        .iter()
        .chain(&["gone.png"])
        .enumerate()
        .map(|(i, name)| {
            Effect::new(EffectKind::Picture, i as u64 * 1000, (i as u64 + 1) * 1000)
                .with_params(EffectParams::Picture(of(name)))
        })
        .collect();
    seq.rows.push(row);
    let mut renderer = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    renderer.set_pictures(pictures.clone());
    assert!(renderer.read_pictures_ahead(&seq));
    for _ in 0..500 {
        if arrived.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        arrived.load(Ordering::SeqCst),
        1,
        "told once, when all of them are there"
    );
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    // The first frame of each effect has its picture: nothing is read as it plays.
    for (i, color) in colors.iter().enumerate() {
        let mut frame = vec![0u8; renderer.frame_len()];
        renderer.render(&seq, i as u64 * 1000, &mut frame);
        assert!(
            frame.chunks(3).all(|px| px == &color[..3]),
            "{}: {frame:?}",
            names[i]
        );
    }
    let mut frame = vec![0u8; renderer.frame_len()];
    renderer.render(&seq, 3_500, &mut frame);
    assert!(frame.iter().all(|&b| b == 0), "the missing one draws nothing");
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    // Asked again, there's nothing left to read.
    assert!(renderer.read_pictures_ahead(&seq));
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(
        (reads.load(Ordering::SeqCst), arrived.load(Ordering::SeqCst)),
        (4, 1)
    );
}
