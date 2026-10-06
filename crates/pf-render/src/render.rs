//! Drawing a whole sequence frame into the show frame.

use crate::color::{Acc, Colors, Rgba, to_u8, write_pixel};
use crate::effects::{Canvas, EffectTime, Shade, Shader, ShaderVisitor};
use crate::geometry::{PixelBuffer, SceneGeometry};
use pf_mapping::ChannelMap;
use pf_model::Show;
use pf_sequence::{Blend, Effect, EffectParams, Sequence, Target};
use std::collections::HashMap;

/// Renders sequences for one show and channel map. Pixel positions are worked out once, when the
/// renderer is made (make a new one when the show changes); pixel buffers for targets are built
/// the first time a target is drawn and kept.
#[derive(Debug, Clone)]
pub struct Renderer {
    geometry: SceneGeometry,
    buffers: HashMap<Target, PixelBuffer>,
    /// Where each target's faces sit in its buffer (built the first time a Faces effect draws on
    /// the target, like `buffers`).
    faces: HashMap<Target, Vec<crate::faces::FaceProp>>,
    /// The Faces effect's lit pixels for one frame, reused from frame to frame.
    face_lit: Vec<Option<Rgba>>,
    /// The frame being built, one entry per show pixel.
    show_acc: Vec<Acc>,
    /// One row being built.
    row_acc: Vec<Acc>,
}

impl Renderer {
    pub fn new(show: &Show, map: &ChannelMap) -> Self {
        Self::from_geometry(SceneGeometry::new(show, map))
    }

    pub fn from_geometry(geometry: SceneGeometry) -> Self {
        Self {
            show_acc: vec![Acc::ZERO; geometry.pixel_count()],
            geometry,
            buffers: HashMap::new(),
            faces: HashMap::new(),
            face_lit: Vec::new(),
            row_acc: Vec::new(),
        }
    }

    pub fn geometry(&self) -> &SceneGeometry {
        &self.geometry
    }

    /// Show-frame bytes (prop order, RGB/RGBW per pixel).
    pub fn frame_len(&self) -> usize {
        self.geometry.frame_len()
    }

    /// Draws frame `index` of the sequence (at `index * frame_ms`).
    pub fn render_frame(&mut self, seq: &Sequence, index: u64, frame: &mut [u8]) {
        let t = index.saturating_mul(u64::from(seq.frame_ms.max(1)));
        self.render(seq, t, frame);
    }

    /// Draws the sequence as it looks at `t_ms` into `frame` (a show frame: prop order, RGB or
    /// RGBW per pixel). Every prop pixel is written; unlit pixels are black, and so is everything
    /// at or past the end of the sequence.
    pub fn render(&mut self, seq: &Sequence, t_ms: u64, frame: &mut [u8]) {
        self.show_acc.fill(Acc::ZERO);
        if t_ms < seq.duration_ms {
            for row in &seq.rows {
                let mut active = row
                    .layers
                    .iter()
                    .flat_map(|layer| &layer.effects)
                    .filter(|e| e.is_active_at(t_ms))
                    .peekable();
                if active.peek().is_none() {
                    continue;
                }
                let buffer = self
                    .buffers
                    .entry(row.target)
                    .or_insert_with(|| self.geometry.buffer(row.target));
                if buffer.is_empty() {
                    continue;
                }
                self.row_acc.clear();
                self.row_acc.resize(buffer.len(), Acc::ZERO);
                let canvas = Canvas {
                    columns: buffer.columns,
                    rows: buffer.rows,
                };
                // Layers draw bottom (first) to top (last).
                for effect in active {
                    let EffectParams::Faces(p) = &effect.params else {
                        draw_effect(effect, t_ms, canvas, buffer, None, &mut self.row_acc);
                        continue;
                    };
                    let faces = self
                        .faces
                        .entry(row.target)
                        .or_insert_with(|| crate::faces::face_props(&self.geometry, row.target, buffer));
                    let mut lit = std::mem::take(&mut self.face_lit);
                    crate::faces::lit_pixels(
                        p,
                        effect,
                        t_ms,
                        seq,
                        &self.geometry,
                        faces,
                        buffer.len(),
                        &mut lit,
                    );
                    let shader = Shader::Faces(crate::effects::Faces::new(lit));
                    draw_effect(effect, t_ms, canvas, buffer, Some(&shader), &mut self.row_acc);
                    if let Shader::Faces(faces) = shader {
                        self.face_lit = faces.into_lit();
                    }
                }
                for (&global, &top) in buffer.global.iter().zip(&self.row_acc) {
                    if top.a > 0.0
                        && let Some(pixel) = self.show_acc.get_mut(global as usize)
                    {
                        pixel.cover_with(top);
                    }
                }
            }
        }
        self.write(frame);
    }

    fn write(&self, frame: &mut [u8]) {
        for prop in &self.geometry.props {
            let cpp = usize::from(prop.channels_per_pixel);
            if cpp < 3 {
                continue;
            }
            let pixels = &self.show_acc[prop.first_pixel..prop.first_pixel + prop.points.len()];
            for (node, acc) in pixels.iter().enumerate() {
                let start = prop.frame_offset + node * cpp;
                if let Some(pixel) = frame.get_mut(start..start + cpp) {
                    write_pixel(pixel, [to_u8(acc.r), to_u8(acc.g), to_u8(acc.b)]);
                }
            }
        }
    }
}

/// How much of the effect shows at `t_ms`, from its fade-in and fade-out.
pub(crate) fn fade_level(effect: &Effect, t_ms: u64) -> f32 {
    let mut level = 1.0f32;
    if effect.fade_in_ms > 0 {
        let since = t_ms.saturating_sub(effect.start_ms) as f32;
        level = level.min(since / effect.fade_in_ms as f32);
    }
    if effect.fade_out_ms > 0 {
        let left = effect.end_ms.saturating_sub(t_ms) as f32;
        level = level.min(left / effect.fade_out_ms as f32);
    }
    level.clamp(0.0, 1.0)
}

/// Draws one effect onto a row; `shader` is given when the renderer had to work it out (Faces).
fn draw_effect(
    effect: &Effect,
    t_ms: u64,
    canvas: Canvas,
    buffer: &PixelBuffer,
    shader: Option<&Shader>,
    acc: &mut [Acc],
) {
    let fade = fade_level(effect, t_ms);
    if fade <= 0.0 {
        return;
    }
    let time = EffectTime::within(effect.start_ms, effect.end_ms, t_ms);
    let made;
    let shader = match shader {
        Some(shader) => shader,
        None => {
            made = Shader::new(
                &effect.params,
                &time,
                Colors::new(&effect.palette.colors),
                effect.id.seed(),
                canvas,
            );
            &made
        }
    };
    struct Fill<'a> {
        buffer: &'a PixelBuffer,
        acc: &'a mut [Acc],
        blend: Blend,
        fade: f32,
    }
    impl ShaderVisitor<()> for Fill<'_> {
        #[inline]
        fn visit<S: Shade>(self, shader: &S) {
            for (px, acc) in self.buffer.pixels.iter().zip(self.acc.iter_mut()) {
                let color: Rgba = shader.shade(px);
                acc.blend(color, self.blend, self.fade);
            }
        }
    }
    shader.with(Fill {
        buffer,
        acc,
        blend: effect.blend,
        fade,
    });
}
