//! Drawing a whole sequence frame into the show frame.

use crate::audio::{Audio, AudioSource, AudioTrack, RenderContext};
use crate::blur::Grid;
use crate::color::{Acc, Colors, Rgba, to_u8, write_pixel};
use crate::effects::{Canvas, EffectTime, Shade, Shader, ShaderVisitor};
use crate::geometry::{Pixel, PixelBuffer, SceneGeometry};
use crate::sim::Sims;
use crate::sparkles::Sparkles;
use pf_mapping::ChannelMap;
use pf_model::{BufferTransform, RenderStyle, Show};
use pf_sequence::{Blend, Effect, EffectId, EffectParams, Sequence, Target};
use std::collections::HashMap;
use std::sync::Arc;

/// A target laid out in a render style, turned or flipped.
type BufferKey = (Target, RenderStyle, BufferTransform);

/// The grid of a whole buffer (rather than one of its parts), in [`Renderer`]'s grids.
const WHOLE: usize = usize::MAX;

/// Renders sequences for one show and channel map. Pixel positions are worked out once, when the
/// renderer is made (make a new one when the show changes); pixel buffers for targets are built
/// the first time a target is drawn and kept.
#[derive(Debug, Clone)]
pub struct Renderer {
    geometry: SceneGeometry,
    /// Each target's buffer in each render style its effects use. All of a target's buffers list
    /// the same pixels in the same order.
    buffers: HashMap<BufferKey, PixelBuffer>,
    /// Where each target's faces sit in its buffer (built the first time a Faces effect draws on
    /// the target, like `buffers`).
    faces: HashMap<Target, Vec<crate::faces::FaceProp>>,
    /// The Faces effect's lit pixels for one frame, reused from frame to frame.
    face_lit: Vec<Option<Rgba>>,
    /// Each buffer (or part of a per-model buffer) laid on a grid, for blurred effects (built the
    /// first time a blurred effect draws on it, like `buffers`).
    grids: HashMap<(BufferKey, usize), Grid>,
    /// A blurred effect's grid cells, and the blur's working space, reused from effect to effect.
    cells: Vec<Rgba>,
    blur_scratch: Vec<[f32; 4]>,
    /// The frame being built, one entry per show pixel.
    show_acc: Vec<Acc>,
    /// One row being built.
    row_acc: Vec<Acc>,
    /// One member of a per-model buffer being drawn.
    part_acc: Vec<Acc>,
    /// The effects worked out frame by frame (falling snow, Lines, Life, Tendril, VU Meter),
    /// kept from frame to frame by effect and the buffer (or part) they draw on.
    sims: Sims<(EffectId, BufferKey, usize)>,
    /// The music effects follow (none, or still on its way: they draw as in silence).
    audio: AudioSource,
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
            grids: HashMap::new(),
            cells: Vec::new(),
            blur_scratch: Vec::new(),
            row_acc: Vec::new(),
            part_acc: Vec::new(),
            sims: Sims::default(),
            audio: AudioSource::none(),
        }
    }

    /// The music the sequences drawn from now on follow (see [`crate::audio`]).
    pub fn set_audio(&mut self, audio: AudioSource) {
        self.audio = audio;
    }

    pub fn audio(&self) -> &AudioSource {
        &self.audio
    }

    /// The music's audio track, once it's there.
    pub fn audio_track(&self) -> Option<Arc<AudioTrack>> {
        self.audio.track().cloned()
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
        let track = self.audio.track().cloned();
        let cx = RenderContext::new(
            track.as_deref().map(|t| Audio::new(t, seq.frame_ms)),
            &seq.timing_tracks,
            seq.frame_ms,
        );
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
                let base: BufferKey = (row.target, RenderStyle::Default, BufferTransform::None);
                let len = self
                    .buffers
                    .entry(base)
                    .or_insert_with(|| self.geometry.buffer(row.target))
                    .len();
                if len == 0 {
                    continue;
                }
                self.row_acc.clear();
                self.row_acc.resize(len, Acc::ZERO);
                // Layers draw bottom (first) to top (last). The lowest effect drawn covers,
                // whatever its blend: there is nothing below it to mix with (as in xLights).
                for (n, source) in active.enumerate() {
                    // Settings that change over the effect, at this moment.
                    let effect = &*cx.effect_at(source, t_ms);
                    let time =
                        EffectTime::within(effect.start_ms, effect.end_ms, t_ms).with_frame_ms(seq.frame_ms);
                    let blend = if n == 0 { Blend::Normal } else { effect.blend };
                    let key: BufferKey = (row.target, effect.render_style, effect.buffer_transform);
                    let buffer = &*self
                        .buffers
                        .entry(key)
                        .or_insert_with(|| self.geometry.styled_buffer(key.0, key.1, key.2));
                    if let EffectParams::Faces(p) = &effect.params {
                        let grid = (effect.blur > 0).then(|| {
                            let grid = self
                                .grids
                                .entry((key, WHOLE))
                                .or_insert_with(|| Grid::new(buffer));
                            grid.near_pixels(blur_amount(effect));
                            &*grid
                        });
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
                        let draw = Draw {
                            effect,
                            blend,
                            t_ms,
                            frame_ms: seq.frame_ms,
                            canvas: canvas_of(buffer),
                            buffer,
                            grid,
                            cells: &mut self.cells,
                            blur_scratch: &mut self.blur_scratch,
                            acc: &mut self.row_acc,
                            cx: &cx,
                        };
                        draw.run(Some(&shader));
                        if let Shader::Faces(faces) = shader {
                            self.face_lit = faces.into_lit();
                        }
                        continue;
                    }
                    if buffer.parts.is_empty() {
                        let grid = (effect.blur > 0).then(|| {
                            let grid = self
                                .grids
                                .entry((key, WHOLE))
                                .or_insert_with(|| Grid::new(buffer));
                            grid.near_pixels(blur_amount(effect));
                            &*grid
                        });
                        let simulated =
                            self.sims
                                .shader((source.id, key, WHOLE), source, &time, canvas_of(buffer), &cx);
                        let draw = Draw {
                            effect,
                            blend,
                            t_ms,
                            frame_ms: seq.frame_ms,
                            canvas: canvas_of(buffer),
                            buffer,
                            grid,
                            cells: &mut self.cells,
                            blur_scratch: &mut self.blur_scratch,
                            acc: &mut self.row_acc,
                            cx: &cx,
                        };
                        draw.run(simulated.as_ref());
                        continue;
                    }
                    // A per-model style: the effect draws on each member's own buffer.
                    for (i, part) in buffer.parts.iter().enumerate() {
                        self.part_acc.clear();
                        self.part_acc
                            .extend(part.slots.iter().map(|&s| self.row_acc[s as usize]));
                        let grid = (effect.blur > 0).then(|| {
                            let grid = self
                                .grids
                                .entry((key, i))
                                .or_insert_with(|| Grid::new(&part.buffer));
                            grid.near_pixels(blur_amount(effect));
                            &*grid
                        });
                        let simulated = self.sims.shader(
                            (source.id, key, i),
                            source,
                            &time,
                            canvas_of(&part.buffer),
                            &cx,
                        );
                        let draw = Draw {
                            effect,
                            blend,
                            t_ms,
                            frame_ms: seq.frame_ms,
                            canvas: canvas_of(&part.buffer),
                            buffer: &part.buffer,
                            grid,
                            cells: &mut self.cells,
                            blur_scratch: &mut self.blur_scratch,
                            acc: &mut self.part_acc,
                            cx: &cx,
                        };
                        draw.run(simulated.as_ref());
                        for (&s, &acc) in part.slots.iter().zip(&self.part_acc) {
                            self.row_acc[s as usize] = acc;
                        }
                    }
                }
                let buffer = &self.buffers[&base];
                for (&global, &top) in buffer.global.iter().zip(&self.row_acc) {
                    if top.a > 0.0
                        && let Some(pixel) = self.show_acc.get_mut(global as usize)
                    {
                        pixel.cover_with(top);
                    }
                }
            }
        }
        self.sims.sweep();
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

/// xLights' Blur setting for an effect (PixelFlow's blur plus one).
fn blur_amount(effect: &Effect) -> u32 {
    effect.blur.min(pf_sequence::MAX_BLUR) + 1
}

/// The grid an effect draws on for a buffer.
fn canvas_of(buffer: &PixelBuffer) -> Canvas {
    Canvas {
        columns: buffer.columns,
        rows: buffer.rows,
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

/// One effect being drawn onto a row.
struct Draw<'a> {
    effect: &'a Effect,
    /// The effect's blend, or Normal for the lowest effect on the row.
    blend: Blend,
    t_ms: u64,
    frame_ms: u32,
    canvas: Canvas,
    buffer: &'a PixelBuffer,
    /// The target's grid, for a blurred effect.
    grid: Option<&'a Grid>,
    cells: &'a mut Vec<Rgba>,
    blur_scratch: &'a mut Vec<[f32; 4]>,
    acc: &'a mut [Acc],
    /// The music and timing tracks the effect follows.
    cx: &'a RenderContext<'a>,
}

impl Draw<'_> {
    /// Draws the effect: shaded (on the grid and blurred, when it has blur), sparkled, faded, and
    /// mixed with the layers below. `shader` is given when the renderer had to work it out
    /// (Faces).
    fn run(self, shader: Option<&Shader>) {
        let effect = self.effect;
        let fade = fade_level(effect, self.t_ms);
        // A fully faded effect changes nothing, except under the blends that black out what's
        // below where the effect is unlit.
        if fade <= 0.0
            && matches!(
                self.blend,
                Blend::Normal | Blend::Add | Blend::Max | Blend::Multiply
            )
        {
            return;
        }
        let time = EffectTime::within(effect.start_ms, effect.end_ms, self.t_ms).with_frame_ms(self.frame_ms);
        let made;
        let shader = match shader {
            Some(shader) => shader,
            None => {
                made = Shader::in_context(
                    &effect.params,
                    &time,
                    Colors::new(&effect.palette.colors),
                    effect.id.seed(),
                    self.canvas,
                    &self.cx.with_members(self.buffer.members.as_ref()),
                );
                &made
            }
        };
        let c = effect.sparkle_color;
        let sparkles = Sparkles::new(
            effect.sparkles,
            [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0),
            effect.id.seed(),
            time.elapsed_ms / u64::from(self.frame_ms.max(1)),
        );
        let halves = Halves::new(self.blend, self.canvas);
        let Some(grid) = self.grid else {
            shader.with(Fill {
                buffer: self.buffer,
                acc: self.acc,
                blend: self.blend,
                fade,
                sparkles,
                halves,
            });
            return;
        };
        // Blurred: drawn on the grid, blurred there, and each pixel takes its cell's color. Only
        // the cells the blur carries to a pixel are drawn; the rest stay clear.
        shader.with(Cells {
            grid,
            near: grid.near(blur_amount(effect)),
            cells: &mut *self.cells,
        });
        crate::blur::blur_near(
            self.cells,
            grid.columns,
            grid.rows,
            blur_amount(effect),
            self.blend != Blend::Normal,
            self.blur_scratch,
            grid.near(blur_amount(effect)),
        );
        let pixels = self.buffer.pixels.iter().zip(&grid.cell_of);
        for ((px, &cell), acc) in pixels.zip(self.acc.iter_mut()) {
            let mut color = self.cells[cell as usize];
            if let Some(s) = &sparkles {
                color = s.apply(color, px.index);
            }
            acc.blend(color, self.blend, fade, halves.first(px));
        }
    }
}

/// Shades the cells of a grid near its pixels (every cell when `near` is `None`).
struct Cells<'a> {
    grid: &'a Grid,
    near: Option<&'a crate::blur::Near>,
    cells: &'a mut Vec<Rgba>,
}

impl ShaderVisitor<()> for Cells<'_> {
    #[inline]
    fn visit<S: Shade>(self, shader: &S) {
        match self.near {
            None => {
                self.cells.clear();
                self.cells
                    .extend(self.grid.cells.iter().map(|px| shader.shade(px)));
            }
            Some(near) => {
                // The other cells needn't be clear: the blur never carries them to a pixel.
                self.cells.resize(self.grid.cells.len(), Rgba::CLEAR);
                for &cell in &near.cells {
                    let cell = cell as usize;
                    self.cells[cell] = shader.shade(&self.grid.cells[cell]);
                }
            }
        }
    }
}

/// Shades each pixel and mixes it in.
struct Fill<'a> {
    buffer: &'a PixelBuffer,
    acc: &'a mut [Acc],
    blend: Blend,
    fade: f32,
    sparkles: Option<Sparkles>,
    halves: Halves,
}

impl ShaderVisitor<()> for Fill<'_> {
    #[inline]
    fn visit<S: Shade>(self, shader: &S) {
        for (px, acc) in self.buffer.pixels.iter().zip(self.acc.iter_mut()) {
            let mut color: Rgba = shader.shade(px);
            if let Some(s) = &self.sparkles {
                color = s.apply(color, px.index);
            }
            acc.blend(color, self.blend, self.fade, self.halves.first(px));
        }
    }
}

/// Which pixels are on the bottom (or left) half of the target, for the half blends. As in
/// xLights, a pixel is in the first half when its row (column) on the target's grid is below
/// half the rows (columns), rounded down, so a target one row high has no bottom half.
#[derive(Debug, Clone, Copy)]
enum Halves {
    None,
    Bottom(u32),
    Left(u32),
}

impl Halves {
    fn new(blend: Blend, canvas: Canvas) -> Self {
        match blend {
            Blend::BottomHalf => Halves::Bottom(canvas.rows.max(1)),
            Blend::LeftHalf => Halves::Left(canvas.columns.max(1)),
            _ => Halves::None,
        }
    }

    #[inline]
    fn first(self, px: &Pixel) -> bool {
        let cell = |at: f32, cells: u32| (at.clamp(0.0, 1.0) * (cells - 1) as f32).round() as u32;
        match self {
            Halves::None => false,
            Halves::Bottom(rows) => cell(px.v, rows) < rows / 2,
            Halves::Left(columns) => cell(px.u, columns) < columns / 2,
        }
    }
}
