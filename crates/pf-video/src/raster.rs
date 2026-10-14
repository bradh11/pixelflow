//! Drawing a show frame as a picture: the Play screen's flat preview of that moment, fitted into
//! the video.
//!
//! The picture is that preview as it would be in a canvas 1,500 screen pixels wide and the video's
//! shape, drawn with as many picture pixels to a screen pixel as fill the video: the video's width
//! over 1,500, which is 1.28 at 1080p. Every size the preview draws with is in screen pixels, so
//! all of them scale with the video: videos of every size show the same picture, and a 1080p one
//! looks like the preview filling a laptop's window.
//!
//! As in the preview, the display (every prop's pixels, and the layout photo when it's shown) is
//! fitted into the picture with a little room around it, keeping its shape: the rest is black, as
//! in a letterboxed film. Behind the display is the preview's night sky, with the photo over it,
//! dimmed to half its strength so the lights stand out. Over that go the pixels as crisp dots
//! (the `dots` module): each sized from its prop's own spacing, a pixel that's off as the
//! preview's quiet gray dot, lit ones over those. Last comes the glow around the lit ones, as much
//! as the look asks for (the `glow` module). Colors are drawn as they are in the frame, with no
//! gamma change, like the preview.

use crate::dots::{DOT_RADIUS, Dots, Layer, PixelBox, View};
use crate::flood::Floods;
use crate::glow::{Halos, Sums};
use pf_engine::PreviewProp;

/// The preview's night sky behind the display.
const BACKDROP: [u16; 3] = [10, 10, 12];
/// A pixel that's off: the preview's "dark" color, gray at 55% (red, green, blue, and how solid
/// it is of 255).
const UNLIT: [u8; 4] = [70, 70, 70, 140];
/// The width of the preview the picture is drawn as, in screen pixels: about what the Play screen
/// has in a laptop's window.
const PREVIEW_WIDTH: f64 = 1500.0;
/// The room left around the display, how far the night sky reaches past it, and how round the
/// sky's corners are, in screen pixels.
const PAD: f64 = 16.0;
const SKY_MARGIN: f64 = 8.0;
const SKY_CORNER: f64 = 6.0;
/// The fewest and most screen pixels to a layout unit, and how many a display that's a single
/// point is shown at.
const ZOOM: (f64, f64) = (0.5, 2000.0);
const POINT_ZOOM: f64 = 40.0;
/// The smallest and largest radius the view's dot is drawn with, in screen pixels: between them
/// it's the dot a pixel has in the layout.
const DOT: (f64, f64) = (1.2, 4.0);

/// How the picture is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: u32,
    pub height: u32,
    /// The dots' size against the preview's (1 draws them as the preview does).
    pub pixel_size: f32,
    /// How much each lit dot glows, 0 (crisp dots) to 1, as the previews' Glow setting.
    pub glow: f32,
}

/// The layout photo, read from its file, and where it sits in the layout.
#[derive(Debug, Clone)]
pub struct Photo {
    pub image: image::RgbImage,
    /// Its top-left corner and width in layout units (its height follows its shape).
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// 0–1, as on the Layout screen.
    pub opacity: f32,
}

impl Photo {
    /// The layout box the photo covers.
    fn bounds(&self) -> Bounds {
        let aspect = f64::from(self.image.height()) / f64::from(self.image.width().max(1));
        let (x, y, width) = (f64::from(self.x), f64::from(self.y), f64::from(self.width));
        Bounds {
            min_x: x,
            max_x: x + width,
            max_y: y,
            min_y: y - width * aspect,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn union(self, other: Bounds) -> Bounds {
        Bounds {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
}

/// The view that fits `bounds` (the display) in a canvas `width` by `height` screen pixels, as
/// the previews fit it.
fn fit(bounds: Bounds, (width, height): (f64, f64), ratio: f64) -> View {
    let w = (bounds.max_x - bounds.min_x).max(1e-3);
    let h = (bounds.max_y - bounds.min_y).max(1e-3);
    let zoom = ((width - 2.0 * PAD) / w)
        .min((height - 2.0 * PAD) / h)
        .clamp(ZOOM.0, ZOOM.1);
    View {
        cx: (bounds.min_x + bounds.max_x) / 2.0,
        cy: (bounds.min_y + bounds.max_y) / 2.0,
        zoom: if w < 1e-2 && h < 1e-2 { POINT_ZOOM } else { zoom },
        width,
        height,
        ratio,
    }
}

/// What drawing a frame needs besides the picture, kept from frame to frame.
#[derive(Debug, Clone, Default)]
struct Scratch {
    dots: Layer,
    glow: Sums,
}

/// A picture being drawn: red, green and blue for each pixel.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u16>,
    scratch: Scratch,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgb: vec![0; width as usize * height as usize * 3],
            scratch: Scratch::default(),
        }
    }

    /// The color at (x, y), clamped to 0–255.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let at = (y as usize * self.width as usize + x as usize) * 3;
        [0, 1, 2].map(|c| self.rgb[at + c].min(255) as u8)
    }
}

/// Everything about the picture that stays the same from frame to frame: the background (night
/// sky and photo), where each pixel's dot goes and how it's drawn, and its halo. Shared by the
/// threads drawing frames.
#[derive(Debug, Clone)]
pub struct Scene {
    width: u32,
    height: u32,
    background: Vec<u16>,
    view: View,
    /// The largest radius a dot is drawn with, in screen pixels.
    radius: f64,
    dots: Dots,
    halos: Option<Halos>,
    /// The DMX floods' washes, under the dots.
    floods: Option<Floods>,
}

impl Scene {
    /// The scene for `props` (the preview's front-view positions), with `photo` behind them.
    /// `None` when there are no pixels to draw.
    pub fn new(props: &[PreviewProp], photo: Option<&Photo>, look: &Look) -> Option<Self> {
        let (width, height) = (look.width.max(2), look.height.max(2));
        let ratio = f64::from(width) / PREVIEW_WIDTH;
        let size = (
            PREVIEW_WIDTH,
            f64::from(height) * PREVIEW_WIDTH / f64::from(width),
        );
        Self::in_canvas(props, photo, (width, height), size, ratio, look)
    }

    /// The scene as a preview draws it in a canvas of `size` screen pixels with `ratio` picture
    /// pixels to each: a picture `width` by `height`.
    fn in_canvas(
        props: &[PreviewProp],
        photo: Option<&Photo>,
        (width, height): (u32, u32),
        size: (f64, f64),
        ratio: f64,
        look: &Look,
    ) -> Option<Self> {
        let mut bounds: Option<Bounds> = None;
        for prop in props {
            for p in prop.points.as_chunks::<2>().0 {
                let (x, y) = (f64::from(p[0]), f64::from(p[1]));
                let b = Bounds {
                    min_x: x,
                    max_x: x,
                    min_y: y,
                    max_y: y,
                };
                bounds = Some(bounds.map_or(b, |a| a.union(b)));
            }
        }
        let bounds = bounds?;
        let bounds = photo.map_or(bounds, |p| bounds.union(p.bounds()));
        let view = fit(bounds, size, ratio);

        // The dot a pixel has in the layout, in screen pixels, and the view's dot: the same, but
        // never too small to see or so big it's a blob. The look's pixel size scales both.
        let scale = f64::from(look.pixel_size.clamp(0.25, 4.0));
        let lamp = view.zoom * DOT_RADIUS;
        let radius = lamp.clamp(DOT.0, DOT.1) * scale;
        let picture = (width as usize, height as usize);
        let halos = Halos::new(props, &view, picture, (lamp * scale).min(radius), look.glow);
        let dim = |dot: f64| halos.as_ref().map_or(1.0, |halos| halos.dim(dot));
        let dots = Dots::new(props, &view, picture, radius, UNLIT, dim);
        let floods = Floods::new(props, &view);
        let mut scene = Self {
            width,
            height,
            background: Vec::new(),
            view,
            radius,
            dots,
            halos,
            floods,
        };
        scene.background = scene.paint_background(bounds, photo);
        Some(scene)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The largest radius a dot is drawn with, in picture pixels.
    pub fn radius(&self) -> f32 {
        (self.radius * self.view.ratio) as f32
    }

    /// Where the layout point (x, y) lands in the picture (y down).
    pub fn to_picture(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = self.view.place(x.into(), y.into());
        (x as f32, y as f32)
    }

    /// The black letterbox, the night sky behind the display (a little past its edges, its
    /// corners rounded), and the photo over it.
    fn paint_background(&self, bounds: Bounds, photo: Option<&Photo>) -> Vec<u16> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut rgb = vec![0u16; w * h * 3];
        let (left, top) = self.view.place(bounds.min_x, bounds.max_y);
        let (right, bottom) = self.view.place(bounds.max_x, bounds.min_y);
        let margin = SKY_MARGIN * self.view.ratio;
        let (left, top, right, bottom) = (left - margin, top - margin, right + margin, bottom + margin);
        let corner = (SKY_CORNER * self.view.ratio)
            .min((right - left) / 2.0)
            .min((bottom - top) / 2.0);
        let clip = |v: f64, max: usize| (v.max(0.0) as usize).min(max);
        for y in clip(top.floor(), h)..clip(bottom.ceil(), h) {
            for x in clip(left.floor(), w)..clip(right.ceil(), w) {
                // How far the pixel's middle is outside the sky's edge (inside it, below zero):
                // the edge is half a pixel soft.
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let beyond_x = (left + corner - px).max(px - (right - corner));
                let beyond_y = (top + corner - py).max(py - (bottom - corner));
                let outside = (beyond_x.max(0.0).powi(2) + beyond_y.max(0.0).powi(2)).sqrt()
                    + beyond_x.max(beyond_y).min(0.0)
                    - corner;
                let cover = (0.5 - outside).clamp(0.0, 1.0);
                for (c, sky) in rgb[(y * w + x) * 3..][..3].iter_mut().zip(BACKDROP) {
                    *c = (f64::from(sky) * cover).round() as u16;
                }
            }
        }
        let Some(photo) = photo else { return rgb };
        let b = photo.bounds();
        let (left, top) = self.view.place(b.min_x, b.max_y);
        let (right, bottom) = self.view.place(b.max_x, b.min_y);
        let (pw, ph) = ((right - left).round(), (bottom - top).round());
        // The photo is part of what the picture is fitted to, so it's never larger than the
        // picture, unless where the show puts it makes no sense (not a number, or so wide the
        // zoom can't shrink it to fit). Such a photo is left out, not scaled up to that size.
        let drawable = |size: f64, picture: usize| size >= 1.0 && size <= 4.0 * picture as f64;
        if !(drawable(pw, w) && drawable(ph, h)) {
            return rgb;
        }
        let resized = image::imageops::resize(
            &photo.image,
            pw as u32,
            ph as u32,
            image::imageops::FilterType::Triangle,
        );
        let alpha = (photo.opacity.clamp(0.0, 1.0) * 0.5 * 256.0).round() as u32;
        let (px, py) = (left.round() as i64, top.round() as i64);
        for (ix, iy, color) in resized.enumerate_pixels() {
            let (x, y) = (px + i64::from(ix), py + i64::from(iy));
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                continue;
            }
            let at = (y as usize * w + x as usize) * 3;
            for c in 0..3 {
                let under = u32::from(rgb[at + c]);
                rgb[at + c] = ((under * (256 - alpha) + u32::from(color.0[c]) * alpha) / 256) as u16;
            }
        }
        rgb
    }

    /// Draws `frame` (a show frame: prop order, RGB or RGBW per pixel) onto `canvas`.
    pub fn draw(&self, frame: &[u8], canvas: &mut Canvas) {
        canvas.width = self.width;
        canvas.height = self.height;
        canvas.rgb.clear();
        canvas.rgb.extend_from_slice(&self.background);
        if let Some(floods) = &self.floods {
            floods.add(frame, self.width as usize, self.height as usize, &mut canvas.rgb);
        }
        self.dots.stamp(frame, &mut canvas.scratch.dots);
        if let Some(bounds) = self.dots.bounds() {
            lay_over(&canvas.scratch.dots, bounds, self.width as usize, &mut canvas.rgb);
        }
        if let Some(halos) = &self.halos {
            halos.add(frame, &mut canvas.scratch.glow, &mut canvas.rgb);
        }
    }
}

/// Lays the part of the dots' picture in `bounds` over `rgb` (a picture `width` pixels wide), each
/// dot as solid as it is.
fn lay_over(dots: &Layer, bounds: PixelBox, width: usize, rgb: &mut [u16]) {
    for y in bounds.y0..bounds.y1 {
        let row = y * width + bounds.x0..y * width + bounds.x1;
        let under = rgb[row.start * 3..row.end * 3].as_chunks_mut::<3>().0;
        for (under, &dot) in under.iter_mut().zip(&dots.pixels[row]) {
            let solid = dot >> 24;
            if solid == 255 {
                *under = [dot & 255, (dot >> 8) & 255, (dot >> 16) & 255].map(|c| c as u16);
            } else if solid > 0 {
                for (c, under) in under.iter_mut().enumerate() {
                    let over = (dot >> (8 * c)) & 255;
                    *under = ((over * solid + u32::from(*under) * (255 - solid) + 127) / 255) as u16;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::PropId;

    fn prop(points: &[f32], frame_offset: usize) -> PreviewProp {
        PreviewProp {
            prop: PropId::new(),
            frame_offset,
            channels_per_pixel: 3,
            points: points.to_vec(),
            flood: false,
        }
    }

    const LOOK: Look = Look {
        width: 320,
        height: 180,
        pixel_size: 1.0,
        glow: 0.0,
    };
    /// Picture pixels to a screen pixel in a picture as wide as `LOOK`'s.
    const RATIO: f32 = 320.0 / 1500.0;

    fn hd(glow: f32) -> Look {
        Look {
            width: 1920,
            height: 1080,
            pixel_size: 1.0,
            glow,
        }
    }

    fn drawn(scene: &Scene, frame: &[u8]) -> Canvas {
        let mut canvas = Canvas::new(1, 1);
        scene.draw(frame, &mut canvas);
        canvas
    }

    /// The color drawn at a layout point.
    fn color_at(scene: &Scene, canvas: &Canvas, x: f32, y: f32) -> [u8; 3] {
        let (x, y) = scene.to_picture(x, y);
        canvas.pixel(x as u32, y as u32)
    }

    /// Pixels `step` apart in a row from (`x`, `y`).
    fn row(count: usize, step: f32, x: f32, y: f32) -> Vec<f32> {
        (0..count).flat_map(|i| [x + i as f32 * step, y]).collect()
    }

    #[test]
    fn nothing_to_draw_without_pixels() {
        assert!(Scene::new(&[], None, &LOOK).is_none());
        assert!(Scene::new(&[prop(&[], 0)], None, &LOOK).is_none());
    }

    #[test]
    fn a_wide_display_is_letterboxed_top_and_bottom() {
        // Three pixels in a row, 10 units apart: as wide as the picture allows, centered.
        let props = [prop(&[0.0, 0.0, 10.0, 0.0, 20.0, 0.0], 0)];
        let scene = Scene::new(&props, None, &LOOK).unwrap();
        let pad = 16.0 * RATIO;
        let (x0, y0) = scene.to_picture(0.0, 0.0);
        let (x2, _) = scene.to_picture(20.0, 0.0);
        assert!((x0 - pad).abs() < 1e-3, "left edge at the padding: {x0}");
        assert!(
            (x2 - (320.0 - pad)).abs() < 1e-3,
            "right edge at the padding: {x2}"
        );
        assert!((y0 - 90.0).abs() < 1e-3, "centered vertically: {y0}");
        let canvas = drawn(&scene, &[255, 0, 0, 0, 255, 0, 0, 0, 255]);
        // Each pixel's own color at its place; the corners are black letterbox.
        assert_eq!(color_at(&scene, &canvas, 0.0, 0.0), [255, 0, 0]);
        assert_eq!(color_at(&scene, &canvas, 10.0, 0.0), [0, 255, 0]);
        assert_eq!(color_at(&scene, &canvas, 20.0, 0.0), [0, 0, 255]);
        assert_eq!(canvas.pixel(0, 0), [0, 0, 0]);
        assert_eq!(canvas.pixel(319, 179), [0, 0, 0]);
        // Between the dots, the night sky shows.
        assert_eq!(canvas.pixel(120, 90), [10, 10, 12]);
    }

    #[test]
    fn a_tall_display_is_pillarboxed() {
        let props = [prop(&[0.0, 0.0, 0.0, 10.0], 0)];
        let scene = Scene::new(&props, None, &LOOK).unwrap();
        let (x, top) = scene.to_picture(0.0, 10.0);
        let (_, bottom) = scene.to_picture(0.0, 0.0);
        assert!((x - 160.0).abs() < 1e-3);
        assert!(top < bottom, "up in the layout is up in the picture");
        assert!((bottom - top - (180.0 - 2.0 * 16.0 * RATIO)).abs() < 1e-3);
        let canvas = drawn(&scene, &[0, 0, 0, 0, 0, 0]);
        assert_eq!(canvas.pixel(10, 90), [0, 0, 0], "black at the sides");
    }

    #[test]
    fn the_night_sky_has_round_corners() {
        // A display 20 by 10 units in a 1080p picture: the sky reaches 8 screen pixels past it.
        let props = [prop(&[0.0, 0.0, 20.0, 10.0], 0)];
        let scene = Scene::new(&props, None, &hd(0.0)).unwrap();
        let canvas = drawn(&scene, &[0; 6]);
        let (left, top) = scene.to_picture(0.0, 10.0);
        let (left, top) = ((left - 8.0 * 1.28) as u32, (top - 8.0 * 1.28) as u32);
        assert_eq!(
            canvas.pixel(left + 1, top + 1),
            [0, 0, 0],
            "cut off at the corner"
        );
        assert_eq!(
            canvas.pixel(left + 12, top + 1),
            [10, 10, 12],
            "along the top edge"
        );
        assert_eq!(
            canvas.pixel(left + 1, top + 12),
            [10, 10, 12],
            "down the left edge"
        );
        assert_eq!(canvas.pixel(left + 6, top + 6), [10, 10, 12], "inside the corner");
        assert_eq!(canvas.pixel(left - 2, top + 30), [0, 0, 0], "letterbox past it");
    }

    #[test]
    fn a_pixel_thats_off_is_the_previews_quiet_dot() {
        let props = [prop(&[0.0, 0.0, 4.0, 0.0], 0)];
        let scene = Scene::new(&props, None, &hd(0.0)).unwrap();
        // Gray at 55% over the night sky.
        let canvas = drawn(&scene, &[0, 0, 0, 90, 80, 70]);
        assert_eq!(color_at(&scene, &canvas, 0.0, 0.0), [43, 43, 44]);
        // A light at a quarter brightness or more is exactly its color.
        assert_eq!(color_at(&scene, &canvas, 4.0, 0.0), [90, 80, 70]);
        // A fainter one lets that dot show through: barely on, it's all but the quiet dot, not
        // a dot nearly black.
        let faint = drawn(&scene, &[0, 0, 0, 32, 0, 0]);
        assert_eq!(color_at(&scene, &faint, 4.0, 0.0), [53, 22, 22]);
        let barely = drawn(&scene, &[0, 0, 0, 1, 0, 0]);
        assert_eq!(color_at(&scene, &barely, 4.0, 0.0), [43, 42, 43]);
    }

    /// How much of the picture's column `x` is lit red within `reach` rows of row `y`, in picture
    /// pixels.
    fn weight(canvas: &Canvas, x: u32, y: u32, reach: u32) -> f32 {
        let sky = f32::from(BACKDROP[0]);
        (y - reach..=y + reach)
            .map(|y| (f32::from(canvas.pixel(x, y)[0]) - sky).max(0.0) / (255.0 - sky))
            .sum()
    }

    /// The same, for the column through a layout point.
    fn weight_at(scene: &Scene, canvas: &Canvas, x: f32, y: f32, reach: u32) -> f32 {
        let (x, y) = scene.to_picture(x, y);
        weight(canvas, x as u32, y as u32, reach)
    }

    /// A string with a unit between its pixels over one with 3 hundredths, 30 units long.
    fn sparse_over_dense() -> [PreviewProp; 2] {
        [
            prop(&row(31, 1.0, 0.0, 2.0), 0),
            prop(&row(1001, 0.03, 0.0, 0.0), 93),
        ]
    }

    #[test]
    fn each_props_dots_are_sized_from_its_own_spacing() {
        let props = sparse_over_dense();
        let scene = Scene::new(&props, None, &hd(0.0)).unwrap();
        let canvas = drawn(&scene, &[255, 0, 0].repeat(1032));
        // The sparse string's pixels are whole dots, 6 picture pixels wide, with sky between.
        let dot = weight_at(&scene, &canvas, 7.0, 2.0, 8);
        assert!((5.0..=6.5).contains(&dot), "{dot}");
        assert_eq!(color_at(&scene, &canvas, 7.5, 2.0), [10, 10, 12]);
        // The dense one's run together into an even line most of a dot wide, without a gap.
        let (from, y) = scene.to_picture(0.0, 0.0);
        let (to, _) = scene.to_picture(30.0, 0.0);
        for x in from as u32..=to as u32 {
            let line = weight(&canvas, x, y as u32, 8);
            assert!((3.0..=4.2).contains(&line), "column {x}: {line}");
        }
    }

    #[test]
    fn sizes_scale_with_the_video() {
        let props = sparse_over_dense();
        let frame = [255, 0, 0].repeat(1032);
        // In screen pixels: the view's dot, a sparse dot's width and the dense line's.
        let sizes: Vec<[f32; 3]> = [(1280, 720), (1920, 1080), (3840, 2160)]
            .into_iter()
            .map(|(width, height)| {
                let look = Look {
                    width,
                    height,
                    ..hd(0.0)
                };
                let scene = Scene::new(&props, None, &look).unwrap();
                let canvas = drawn(&scene, &frame);
                let ratio = width as f32 / 1500.0;
                let reach = (8.0 * ratio) as u32;
                [
                    scene.radius(),
                    weight_at(&scene, &canvas, 7.0, 2.0, reach),
                    weight_at(&scene, &canvas, 7.0, 0.0, reach),
                ]
                .map(|size| size / ratio)
            })
            .collect();
        let hd = sizes[1];
        for size in &sizes {
            assert!((size[0] - hd[0]).abs() < 1e-3, "the view's dot: {sizes:?}");
            assert!((size[1] - hd[1]).abs() < 0.7, "a sparse dot: {sizes:?}");
            assert!((size[2] - hd[2]).abs() < 0.7, "a dense line: {sizes:?}");
        }
        // The view's dot is the one a pixel has in the layout (a twentieth of a unit), as a
        // preview 1,500 screen pixels wide shows it: 30 units across all but its padding.
        assert!((hd[0] - (1500.0 - 32.0) / 30.0 * 0.05).abs() < 1e-3, "{hd:?}");
    }

    #[test]
    fn with_no_glow_the_picture_is_only_the_crisp_dots() {
        let props = [prop(&[0.0, 0.0, 1.0, 0.0], 0)];
        let scene = Scene::new(&props, None, &hd(0.0)).unwrap();
        let canvas = drawn(&scene, &[255, 0, 0, 255, 0, 0]);
        let (x, y) = scene.to_picture(0.0, 0.0);
        let (x, y) = (x as u32, y as u32);
        assert_eq!(canvas.pixel(x, y), [255, 0, 0]);
        // The dot is 10 picture pixels wide: past its edge there's only the night sky.
        for away in [7, 9, 12, 20] {
            assert_eq!(canvas.pixel(x + away, y), [10, 10, 12], "{away} to its right");
        }
        for away in [7, 9] {
            assert_eq!(canvas.pixel(x, y - away), [10, 10, 12], "{away} above it");
        }
    }

    #[test]
    fn the_glow_level_goes_from_crisp_dots_to_a_wide_halo() {
        let props = [prop(&[0.0, 0.0, 1.0, 0.0], 0)];
        // The red at a lit dot's middle, and above the night sky just past its edge and a little
        // further out, at a glow level.
        let around = |glow: f32| {
            let scene = Scene::new(&props, None, &hd(glow)).unwrap();
            let canvas = drawn(&scene, &[255, 0, 0, 0, 0, 0]);
            let (x, y) = scene.to_picture(0.0, 0.0);
            let (x, y) = (x as u32, y as u32);
            let sky = canvas.pixel(x + 200, y)[0];
            (
                canvas.pixel(x, y)[0],
                canvas.pixel(x + 8, y)[0] - sky,
                canvas.pixel(x + 16, y)[0] - sky,
            )
        };
        assert_eq!(around(0.0), (255, 0, 0));
        let (half_core, half_near, half_far) = around(0.5);
        let (full_core, full_near, full_far) = around(1.0);
        assert!(half_near > 20, "half glow shows: {half_near}");
        assert!(
            full_near > half_near && full_far > half_far,
            "full glow is brighter and wider"
        );
        // The dot under its own halo is still its own color.
        assert!(half_core >= 250 && full_core >= 250, "{half_core}, {full_core}");
        // Out-of-range levels are kept in range rather than refused.
        assert_eq!(around(-1.0), around(0.0));
        assert_eq!(around(9.0), around(1.0));
    }

    #[test]
    fn only_lit_pixels_glow_and_their_glows_add_up() {
        // Two pixels a few picture pixels apart, and one well away.
        let props = [prop(&[0.0, 0.0, 0.05, 0.0, 10.0, 0.0], 0)];
        let scene = Scene::new(&props, None, &hd(0.5)).unwrap();
        let both = drawn(&scene, &[100, 0, 0, 100, 0, 0, 0, 0, 0]);
        let one = drawn(&scene, &[100, 0, 0, 0, 0, 0, 0, 0, 0]);
        // Above the gap between the two, past their dots: both glows reach it.
        let (x, y) = scene.to_picture(0.025, 0.0);
        let (x, y) = (x as u32, y as u32 - 8);
        assert!(one.pixel(x, y)[0] > 15, "{:?}", one.pixel(x, y));
        assert!(both.pixel(x, y)[0] > one.pixel(x, y)[0] + 5);
        // Around the pixel that's off there's only the night sky.
        let (x, y) = scene.to_picture(10.0, 0.0);
        assert_eq!(both.pixel(x as u32, y as u32 - 8), [10, 10, 12]);
        assert_eq!(color_at(&scene, &both, 10.0, 0.0), [43, 43, 44]);
    }

    #[test]
    fn pixel_size_scales_the_dots() {
        let props = [prop(&[0.0, 0.0, 1.0, 0.0], 0)];
        let sized = |pixel_size: f32| {
            let look = Look {
                pixel_size,
                ..hd(0.0)
            };
            let scene = Scene::new(&props, None, &look).unwrap();
            let canvas = drawn(&scene, &[255, 0, 0, 255, 0, 0]);
            (scene.radius(), weight_at(&scene, &canvas, 0.0, 0.0, 40))
        };
        let (small, as_preview, big) = (sized(0.5), sized(1.0), sized(2.0));
        assert!((big.0 / small.0 - 4.0).abs() < 1e-4);
        // The dots as drawn: 5, 10 and 20 picture pixels wide.
        for (drawn, wide) in [(small.1, 5.0), (as_preview.1, 10.0), (big.1, 20.0)] {
            assert!((drawn - wide).abs() < 0.1, "{drawn} for {wide}");
        }
    }

    #[test]
    fn the_photo_is_placed_and_dimmed() {
        // A white photo 20 units wide and 10 tall, at full strength: drawn at half.
        let photo = Photo {
            image: image::RgbImage::from_pixel(40, 20, image::Rgb([255, 255, 255])),
            x: -10.0,
            y: 5.0,
            width: 20.0,
            opacity: 1.0,
        };
        let props = [prop(&[0.0, 0.0], 0)];
        let scene = Scene::new(&props, Some(&photo), &LOOK).unwrap();
        // The photo is the display's extent: it fills the picture's width inside the padding.
        let (left, top) = scene.to_picture(-10.0, 5.0);
        assert!((left - 16.0 * RATIO).abs() < 1e-3);
        let canvas = drawn(&scene, &[0, 0, 0]);
        let inside = canvas.pixel(left.round() as u32 + 3, top.round() as u32 + 3);
        assert_eq!(inside, [132, 132, 133], "half white over the night sky");
        assert_eq!(canvas.pixel(0, 0), [0, 0, 0], "letterbox around it");
        // Without the photo, the display is just the one pixel.
        let bare = Scene::new(&props, None, &LOOK).unwrap();
        assert_eq!(bare.to_picture(0.0, 0.0), (160.0, 90.0));
    }

    #[test]
    fn a_photo_placed_where_it_makes_no_sense_is_left_out() {
        // A show file can say anything for the photo's place: none of these may be scaled up to
        // the size they ask for (a width of a million million units is a million million pixels
        // at the zoom's floor).
        let props = [prop(&[0.0, 0.0], 0)];
        for (x, y, width) in [
            (0.0, 0.0, 1e12),
            (0.0, 0.0, f32::INFINITY),
            (0.0, 0.0, f32::NAN),
            (f32::NAN, 0.0, 20.0),
            (0.0, f32::INFINITY, 20.0),
        ] {
            let photo = Photo {
                image: image::RgbImage::from_pixel(40, 20, image::Rgb([255, 255, 255])),
                x,
                y,
                width,
                opacity: 1.0,
            };
            let scene = Scene::new(&props, Some(&photo), &LOOK).unwrap();
            let canvas = drawn(&scene, &[0, 0, 0]);
            // No white anywhere: only the letterbox, the night sky, and the unlit dot.
            let brightest = canvas.rgb.iter().max().copied().unwrap_or(0);
            assert!(brightest < 100, "({x}, {y}, {width}): {brightest}");
        }
    }

    #[test]
    fn rgbw_pixels_use_their_rgb() {
        let mut p = prop(&[0.0, 0.0, 4.0, 0.0], 0);
        p.channels_per_pixel = 4;
        let scene = Scene::new(&[p], None, &LOOK).unwrap();
        let canvas = drawn(&scene, &[0, 0, 0, 255, 90, 80, 70, 0]);
        assert_eq!(color_at(&scene, &canvas, 4.0, 0.0), [90, 80, 70]);
        assert_eq!(color_at(&scene, &canvas, 0.0, 0.0), [43, 43, 44]);
    }

    #[test]
    fn a_canvas_is_drawn_afresh_for_each_frame_and_each_scene() {
        let line = [prop(&row(200, 0.03, 0.0, 0.0), 0)];
        let other = [
            prop(&row(40, 0.5, 0.0, 3.0), 0),
            prop(&row(9, 1.0, 0.0, 0.0), 120),
        ];
        let (lit, off) = ([200u8; 600], [0u8; 600]);
        let mut canvas = Canvas::new(1, 1);
        for glow in [0.0, 0.5] {
            let first = Scene::new(&line, None, &hd(glow)).unwrap();
            let second = Scene::new(&other, None, &hd(glow)).unwrap();
            first.draw(&lit, &mut canvas);
            first.draw(&off, &mut canvas);
            assert_eq!(
                canvas.rgb,
                drawn(&first, &off).rgb,
                "glow {glow}: after a lit frame"
            );
            second.draw(&lit, &mut canvas);
            assert_eq!(
                canvas.rgb,
                drawn(&second, &lit).rgb,
                "glow {glow}: after another scene"
            );
        }
    }

    /// The scenes both this crate and the window's previews must draw alike: see
    /// `docs/dev/contracts.md`.
    mod shared_scenes {
        use super::*;
        use crate::dots::Way;
        use serde::Deserialize;
        use std::path::PathBuf;

        #[derive(Deserialize)]
        struct Size {
            width: f64,
            height: f64,
        }

        #[derive(Deserialize)]
        struct Fitted {
            cx: f64,
            cy: f64,
            zoom: f64,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wired {
            frame_offset: usize,
            channels_per_pixel: u8,
            points: Vec<[f32; 2]>,
        }

        /// A picture: its size, where the part with anything in it is (the rest is empty), the
        /// colors in that part (each `rrggbbaa`), and for each of its rows which of them each
        /// pixel is.
        #[derive(Deserialize)]
        struct Picture {
            width: u32,
            height: u32,
            left: usize,
            top: usize,
            colors: Vec<String>,
            rows: Vec<String>,
        }

        impl Picture {
            /// Every pixel of the picture, row by row, as [`Layer`] holds them.
            fn pixels(&self) -> Vec<u32> {
                let colors: Vec<u32> = self
                    .colors
                    .iter()
                    .map(|color| u32::from_le_bytes(hex(color).try_into().unwrap()))
                    .collect();
                let mut pixels = vec![0; (self.width * self.height) as usize];
                for (y, row) in self.rows.iter().enumerate() {
                    for (x, color) in row.split(' ').enumerate() {
                        let at = (self.top + y) * self.width as usize + self.left + x;
                        pixels[at] = colors[color.parse::<usize>().unwrap()];
                    }
                }
                pixels
            }
        }

        /// A scene as the window's tests write it: a preview's canvas (its size in screen pixels
        /// and picture pixels to each), the view the preview fits the props in and the largest
        /// radius it draws a dot with, the color of a pixel that's off, the props and a show
        /// frame, and the picture of dots the preview draws.
        #[derive(Deserialize)]
        struct Shared {
            size: Size,
            ratio: f64,
            view: Fitted,
            radius: f64,
            dark: [u8; 4],
            props: Vec<Wired>,
            frame: String,
            picture: Picture,
        }

        fn folder() -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src/api/contracts/pixels")
        }

        fn scenes() -> Vec<(String, Shared)> {
            let mut names: Vec<String> = std::fs::read_dir(folder())
                .expect("app/src/api/contracts/pixels")
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".json"))
                .collect();
            names.sort();
            names
                .into_iter()
                .map(|name| {
                    let text = std::fs::read_to_string(folder().join(&name)).unwrap();
                    let scene = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
                    (name, scene)
                })
                .collect()
        }

        fn hex(text: &str) -> Vec<u8> {
            (0..text.len() / 2)
                .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
                .collect()
        }

        /// A color as the pictures write it: red, green, blue, and how solid it is.
        fn rgba(pixel: u32) -> String {
            let [r, g, b, a] = pixel.to_le_bytes();
            format!("rgba({r}, {g}, {b}, {a})")
        }

        #[test]
        fn the_export_draws_each_scene_exactly_as_the_preview_does() {
            let scenes = scenes();
            let mut ways = Vec::new();
            let mut problems = Vec::new();
            for (name, shared) in &scenes {
                let file = format!("app/src/api/contracts/pixels/{name}");
                let props: Vec<PreviewProp> = shared
                    .props
                    .iter()
                    .map(|p| PreviewProp {
                        prop: PropId::new(),
                        frame_offset: p.frame_offset,
                        channels_per_pixel: p.channels_per_pixel,
                        points: p.points.iter().flatten().copied().collect(),
                        flood: false,
                    })
                    .collect();
                let picture = &shared.picture;
                let (width, height) = (picture.width, picture.height);
                let look = Look {
                    width,
                    height,
                    pixel_size: 1.0,
                    glow: 0.0,
                };
                let size = (shared.size.width, shared.size.height);
                let scene =
                    Scene::in_canvas(&props, None, (width, height), size, shared.ratio, &look).unwrap();
                ways.extend(scene.dots.ways());

                // The same view of the display, the same dot, and the same color for a pixel
                // that's off, as the preview has them.
                let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
                let view = (scene.view.cx, scene.view.cy, scene.view.zoom);
                let fitted = (shared.view.cx, shared.view.cy, shared.view.zoom);
                if !(near(view.0, fitted.0) && near(view.1, fitted.1) && near(view.2, fitted.2)) {
                    problems.push(format!(
                        "{file}: the export fits the display as {view:?}, the preview as {fitted:?}."
                    ));
                }
                if !near(scene.radius, shared.radius) {
                    problems.push(format!(
                        "{file}: the export's largest dot has radius {}, the preview's {}.",
                        scene.radius, shared.radius
                    ));
                }
                if UNLIT != shared.dark {
                    problems.push(format!(
                        "{file}: the export draws a pixel that's off as {UNLIT:?}, the preview as {:?}.",
                        shared.dark
                    ));
                }

                // The same picture, to the last bit of every pixel.
                let expected = picture.pixels();
                let mut layer = Layer::default();
                scene.dots.stamp(&hex(&shared.frame), &mut layer);
                let differ: Vec<usize> = (0..expected.len())
                    .filter(|&i| layer.pixels[i] != expected[i])
                    .collect();
                if let Some(&first) = differ.first() {
                    problems.push(format!(
                        "{file}: the export draws {} of the picture's {} pixels differently from the preview. \
                         The first is at ({}, {}): the preview draws {}, the export {}.",
                        differ.len(),
                        expected.len(),
                        first % width as usize,
                        first / width as usize,
                        rgba(expected[first]),
                        rgba(layer.pixels[first]),
                    ));
                }
            }
            assert!(
                problems.is_empty(),
                "{}\nDecide which side is wrong and fix that side (see docs/dev/contracts.md).",
                problems.join("\n")
            );
            // Between them the scenes draw props every way there is.
            assert!(scenes.len() >= 8, "only {} scenes were found", scenes.len());
            for way in [Way::Apart, Way::Crowded, Way::Merged] {
                assert!(ways.contains(&way), "no scene draws a prop {way:?}");
            }
        }
    }
}
