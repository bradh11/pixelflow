//! Drawing a show frame as a picture: the 2D preview's front view, fitted into the video.
//!
//! As in the sequencer's preview, the display (every prop's pixels, and the layout photo when it's
//! shown) is fitted into the picture with a little room around it, keeping its shape: the rest is
//! black, as in a letterboxed film. Behind the display is the preview's night sky, with the photo
//! over it, dimmed to half its strength so the lights stand out. Each lit pixel is a round dot in
//! its color with a soft glow around it, added onto what's below (overlapping glows brighten
//! towards white, as real bulbs do); an unlit pixel is a faint gray dot, as the preview shows
//! pixels that are off while a sequence plays. Colors are drawn as they are in the frame, with no
//! gamma change, like the preview.

use pf_engine::PreviewProp;

/// The preview's night sky behind the display.
const BACKDROP: [u16; 3] = [10, 10, 12];
/// An unlit pixel: the preview's "dark" color, gray at 55%.
const UNLIT: u16 = 70;
const UNLIT_ALPHA: u32 = 141;
/// How far a dot's glow reaches, in core radii (its Gaussian width), and how strong it is next to
/// the core (out of 256).
const GLOW_WIDTH: f32 = 1.8;
const GLOW_STRENGTH: f32 = 0.35;
/// The picture height the preview's sizes are given for (a preview about this tall on a Retina
/// screen draws at twice its size in device pixels).
const PREVIEW_HEIGHT: f32 = 540.0;

/// How the picture is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: u32,
    pub height: u32,
    /// The dots' size against the preview's (1 draws them as the preview does).
    pub pixel_size: f32,
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
    /// The layout box the photo covers: (min x, min y, max x, max y).
    fn bounds(&self) -> Bounds {
        let aspect = self.image.height() as f32 / self.image.width().max(1) as f32;
        Bounds {
            min_x: self.x,
            max_x: self.x + self.width,
            max_y: self.y,
            min_y: self.y - self.width * aspect,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
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

/// A picture being drawn: RGB, a `u16` per channel so glows can add up past full brightness
/// before they're clamped.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u16>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgb: vec![0; width as usize * height as usize * 3],
        }
    }

    /// The color at (x, y), clamped to 0–255.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let at = (y as usize * self.width as usize + x as usize) * 3;
        [0, 1, 2].map(|c| self.rgb[at + c].min(255) as u8)
    }
}

/// A dot's weights around its center, out of 256: (dx, dy, core, glow). The core covers what's
/// below it; the glow adds onto it.
#[derive(Debug, Clone)]
struct Sprite(Vec<(i32, i32, u32, u32)>);

impl Sprite {
    /// A round dot of `radius` with soft edges, and a glow around it when `glow`.
    fn new(radius: f32, glow: bool) -> Self {
        let sigma = radius * GLOW_WIDTH;
        let reach = if glow {
            (radius + 2.2 * sigma).ceil()
        } else {
            (radius + 1.0).ceil()
        } as i32;
        let mut weights = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let core = (radius + 0.5 - d).clamp(0.0, 1.0);
                let halo = if glow {
                    (1.0 - core) * GLOW_STRENGTH * (-(d / sigma) * (d / sigma)).exp()
                } else {
                    0.0
                };
                let (core, halo) = ((core * 256.0).round() as u32, (halo * 256.0).round() as u32);
                if core > 0 || halo > 0 {
                    weights.push((dx, dy, core, halo));
                }
            }
        }
        Self(weights)
    }
}

/// One light pixel: where it is in the picture and where its color is in a show frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Dot {
    x: i32,
    y: i32,
    at: usize,
}

/// Everything about the picture that stays the same from frame to frame: the background (night
/// sky and photo) and where each pixel's dot goes. Shared by the threads drawing frames.
#[derive(Debug, Clone)]
pub struct Scene {
    width: u32,
    height: u32,
    background: Vec<u16>,
    dots: Vec<Dot>,
    lit: Sprite,
    unlit: Sprite,
    radius: f32,
    /// Layout to picture: picture x = cx + (x - ox) * zoom, picture y = cy - (y - oy) * zoom.
    zoom: f32,
    center: (f32, f32),
    origin: (f32, f32),
}

impl Scene {
    /// The scene for `props` (the preview's front-view positions), with `photo` behind them.
    /// `None` when there are no pixels to draw.
    pub fn new(props: &[PreviewProp], photo: Option<&Photo>, look: &Look) -> Option<Self> {
        let (width, height) = (look.width.max(2), look.height.max(2));
        let mut bounds: Option<Bounds> = None;
        for prop in props {
            for p in prop.points.as_chunks::<2>().0 {
                let b = Bounds {
                    min_x: p[0],
                    max_x: p[0],
                    min_y: p[1],
                    max_y: p[1],
                };
                bounds = Some(bounds.map_or(b, |a| a.union(b)));
            }
        }
        let bounds = bounds?;
        let bounds = photo.map_or(bounds, |p| bounds.union(p.bounds()));

        // Sizes as the preview gives them for a picture PREVIEW_HEIGHT tall.
        let scale = height as f32 / PREVIEW_HEIGHT;
        let pad = 16.0 * scale;
        let span_x = (bounds.max_x - bounds.min_x).max(1e-3);
        let span_y = (bounds.max_y - bounds.min_y).max(1e-3);
        let zoom = ((width as f32 - 2.0 * pad) / span_x)
            .min((height as f32 - 2.0 * pad) / span_y)
            .clamp(1e-3, 1e5);
        let radius = (zoom * 0.05 / scale).clamp(1.2, 4.0) * scale * look.pixel_size.clamp(0.25, 4.0);
        let mut scene = Self {
            width,
            height,
            background: Vec::new(),
            dots: Vec::new(),
            lit: Sprite::new(radius, true),
            unlit: Sprite::new(radius, false),
            radius,
            zoom,
            center: (width as f32 / 2.0, height as f32 / 2.0),
            origin: (
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
            ),
        };
        scene.background = scene.paint_background(bounds, photo, 8.0 * scale);
        for prop in props {
            for (n, p) in prop.points.as_chunks::<2>().0.iter().enumerate() {
                let (x, y) = scene.to_picture(p[0], p[1]);
                scene.dots.push(Dot {
                    x: x.round() as i32,
                    y: y.round() as i32,
                    at: prop.frame_offset + n * usize::from(prop.channels_per_pixel),
                });
            }
        }
        Some(scene)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The dots' core radius, in picture pixels.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Where the layout point (x, y) lands in the picture (y down).
    pub fn to_picture(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.center.0 + (x - self.origin.0) * self.zoom,
            self.center.1 - (y - self.origin.1) * self.zoom,
        )
    }

    /// The black letterbox, the night sky behind the display (`margin` past its edges), and the
    /// photo over it.
    fn paint_background(&self, bounds: Bounds, photo: Option<&Photo>, margin: f32) -> Vec<u16> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut rgb = vec![0u16; w * h * 3];
        let (left, top) = self.to_picture(bounds.min_x, bounds.max_y);
        let (right, bottom) = self.to_picture(bounds.max_x, bounds.min_y);
        let clip = |v: f32, max: usize| (v.round().max(0.0) as usize).min(max);
        let (x0, x1) = (clip(left - margin, w), clip(right + margin, w));
        let (y0, y1) = (clip(top - margin, h), clip(bottom + margin, h));
        for y in y0..y1 {
            for x in x0..x1 {
                rgb[(y * w + x) * 3..][..3].copy_from_slice(&BACKDROP);
            }
        }
        let Some(photo) = photo else { return rgb };
        let b = photo.bounds();
        let (left, top) = self.to_picture(b.min_x, b.max_y);
        let (right, bottom) = self.to_picture(b.max_x, b.min_y);
        let (pw, ph) = ((right - left).round(), (bottom - top).round());
        if pw < 1.0 || ph < 1.0 {
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
        let color = |dot: &Dot| frame.get(dot.at..dot.at + 3).map(|c| [c[0], c[1], c[2]]);
        // Unlit dots first, so lit glows go over them.
        for dot in &self.dots {
            if color(dot).is_none_or(|c| c == [0, 0, 0]) {
                self.stamp(canvas, dot, &self.unlit, |px, core, _| {
                    let a = core * UNLIT_ALPHA / 256;
                    for c in px.iter_mut() {
                        *c = ((u32::from(*c) * (256 - a) + u32::from(UNLIT) * a) / 256) as u16;
                    }
                });
            }
        }
        for dot in &self.dots {
            let Some(rgb) = color(dot).filter(|c| *c != [0, 0, 0]) else {
                continue;
            };
            self.stamp(canvas, dot, &self.lit, |px, core, glow| {
                for (c, v) in px.iter_mut().zip(rgb) {
                    let v = u32::from(v);
                    let covered = (u32::from(*c) * (256 - core) + v * core) / 256;
                    *c = (covered + v * glow / 256).min(u32::from(u16::MAX)) as u16;
                }
            });
        }
    }

    fn stamp(
        &self,
        canvas: &mut Canvas,
        dot: &Dot,
        sprite: &Sprite,
        mut paint: impl FnMut(&mut [u16], u32, u32),
    ) {
        let (w, h) = (self.width as i32, self.height as i32);
        for &(dx, dy, core, glow) in &sprite.0 {
            let (x, y) = (dot.x + dx, dot.y + dy);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let at = (y as usize * w as usize + x as usize) * 3;
            paint(&mut canvas.rgb[at..at + 3], core, glow);
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
        }
    }

    const LOOK: Look = Look {
        width: 320,
        height: 180,
        pixel_size: 1.0,
    };

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
        let pad = 16.0 * 180.0 / 540.0;
        let (x0, y0) = scene.to_picture(0.0, 0.0);
        let (x2, _) = scene.to_picture(20.0, 0.0);
        assert!((x0 - pad).abs() < 1e-3, "left edge at the padding: {x0}");
        assert!(
            (x2 - (320.0 - pad)).abs() < 1e-3,
            "right edge at the padding: {x2}"
        );
        assert!((y0 - 90.0).abs() < 1e-3, "centered vertically: {y0}");
        let mut canvas = Canvas::new(1, 1);
        scene.draw(&[255, 0, 0, 0, 255, 0, 0, 0, 255], &mut canvas);
        // Each pixel's own color at its center; the corners are black letterbox.
        assert_eq!(canvas.pixel(x0.round() as u32, 90), [255, 0, 0]);
        assert_eq!(canvas.pixel(160, 90), [0, 255, 0]);
        assert_eq!(canvas.pixel(x2.round() as u32, 90), [0, 0, 255]);
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
        assert!((bottom - top - (180.0 - 2.0 * 16.0 / 3.0)).abs() < 1e-3);
        let mut canvas = Canvas::new(1, 1);
        scene.draw(&[0, 0, 0, 0, 0, 0], &mut canvas);
        assert_eq!(canvas.pixel(10, 90), [0, 0, 0], "black at the sides");
    }

    #[test]
    fn dots_glow_and_add_up() {
        let props = [prop(&[0.0, 0.0, 1.0, 0.0, 2.0, 0.0], 0)];
        let look = Look {
            width: 640,
            height: 360,
            pixel_size: 1.0,
        };
        let scene = Scene::new(&props, None, &look).unwrap();
        let r = scene.radius();
        assert!(
            (r - 4.0 * 360.0 / 540.0).abs() < 1e-3,
            "the preview's largest dot, at this size: {r}"
        );
        let mut canvas = Canvas::new(1, 1);
        scene.draw(&[200, 0, 0, 0, 0, 0, 200, 0, 0], &mut canvas);
        let (x, y) = scene.to_picture(0.0, 0.0);
        let (x, y) = (x.round() as u32, y.round() as u32);
        assert_eq!(canvas.pixel(x, y), [200, 0, 0]);
        // Just outside the core: dimmer, but glowing above the night sky.
        let glow = canvas.pixel(x + r.ceil() as u32, y);
        assert!(glow[0] > 20 && glow[0] < 200, "{glow:?}");
        // The unlit middle pixel is a faint gray dot.
        let (mx, _) = scene.to_picture(1.0, 0.0);
        let middle = canvas.pixel(mx.round() as u32, y);
        assert!(middle[1] > 30 && middle[1] < 50, "{middle:?}");
    }

    #[test]
    fn overlapping_glows_add_up() {
        // Two pixels a few picture pixels apart: between them, both glows add.
        let props = [prop(&[0.0, 0.0, 0.15, 0.0, 10.0, 0.0], 0)];
        let look = Look {
            width: 640,
            height: 360,
            pixel_size: 1.0,
        };
        let scene = Scene::new(&props, None, &look).unwrap();
        let (mut both, mut one) = (Canvas::new(1, 1), Canvas::new(1, 1));
        scene.draw(&[100, 0, 0, 100, 0, 0, 0, 0, 0], &mut both);
        scene.draw(&[100, 0, 0, 0, 0, 0, 0, 0, 0], &mut one);
        let (x, y) = scene.to_picture(0.075, 0.0);
        let (between, y) = (x.round() as u32, y.round() as u32);
        assert!(both.pixel(between, y)[0] > one.pixel(between, y)[0] + 5);
    }

    #[test]
    fn pixel_size_scales_the_dots() {
        let props = [prop(&[0.0, 0.0, 1.0, 0.0], 0)];
        let small = Scene::new(
            &props,
            None,
            &Look {
                pixel_size: 0.5,
                ..LOOK
            },
        )
        .unwrap();
        let big = Scene::new(
            &props,
            None,
            &Look {
                pixel_size: 2.0,
                ..LOOK
            },
        )
        .unwrap();
        assert!((big.radius() / small.radius() - 4.0).abs() < 1e-4);
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
        assert!((left - 16.0 / 3.0).abs() < 1e-3);
        let mut canvas = Canvas::new(1, 1);
        scene.draw(&[0, 0, 0], &mut canvas);
        let inside = canvas.pixel(left.round() as u32 + 3, top.round() as u32 + 3);
        assert_eq!(inside, [132, 132, 133], "half white over the night sky");
        assert_eq!(canvas.pixel(0, 0), [0, 0, 0], "letterbox around it");
        // Without the photo, the display is just the one pixel.
        let bare = Scene::new(&props, None, &LOOK).unwrap();
        assert_eq!(bare.to_picture(0.0, 0.0), (160.0, 90.0));
    }

    #[test]
    fn rgbw_pixels_use_their_rgb() {
        let mut p = prop(&[0.0, 0.0, 4.0, 0.0], 0);
        p.channels_per_pixel = 4;
        let scene = Scene::new(&[p], None, &LOOK).unwrap();
        let mut canvas = Canvas::new(1, 1);
        scene.draw(&[0, 0, 0, 255, 9, 8, 7, 0], &mut canvas);
        let (x, y) = scene.to_picture(4.0, 0.0);
        assert_eq!(canvas.pixel(x.round() as u32, y.round() as u32), [9, 8, 7]);
    }
}
