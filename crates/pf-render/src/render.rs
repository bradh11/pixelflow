//! Drawing a whole sequence frame into the show frame.
//!
//! A frame is drawn in four steps. When there is enough to draw, the second and fourth are
//! shared out among the threads of a [`RenderPool`]:
//!
//! 1. **Plan** (one thread). Which rows and effects play at this moment, the buffer each draws
//!    on, and which tiles each row has pixels in. Buffers, blur grids, and the lists of where a
//!    buffer falls among the tiles are built the first time they're needed and kept.
//! 2. **Make ready** (every effect at once). Each effect is made ready for the frame: its
//!    settings at this moment, its shader (the effects that carry on from the frame before move
//!    on a frame, each behind a lock of its own), and for a blurred effect its colors on the
//!    buffer's grid, blurred there when the grid is all pixels. No effect looks at another here.
//! 3. **Blur in turn** (one blur after another, a large one's passes shared out). A blur of a
//!    grid with gaps between its pixels reads what the blurs before it left in their working
//!    space, so those are done in the sequence's order, as they always were (see `Scene::blur`).
//! 4. **Draw** (every tile at once). The show's pixels are cut into tiles (see `tiles.rs`). For
//!    each tile, each row with pixels there is drawn in the sequence's order: it starts clear (or
//!    from what the rows before left), its effects are mixed in from the bottom layer up, and
//!    it's laid over the frame. Then the tile's pixels are written out as bytes.
//!
//! Everything the last step does to a pixel reads and writes that pixel alone, and each pixel is
//! in one tile; the steps before it either don't look at one another or keep their order. So a
//! pixel goes through the same arithmetic in the same order however the work is shared out: a
//! frame's bytes don't depend on how many threads drew it, or on which drew what. One thread
//! draws the same way, effect after effect and tile after tile.

use crate::adjust::Adjust;
use crate::audio::{Audio, AudioSource, AudioTrack, RenderContext};
use crate::blur::{Grid, PIECE_OF_CELLS, SHARE_FROM_CELLS, Working, in_pieces};
use crate::color::{Acc, Colors, Rgba, to_u8, write_pixel};
use crate::effects::{Canvas, EffectTime, Shade, Shader, ShaderVisitor};
use crate::faces::FaceProp;
use crate::geometry::{Pixel, PixelBuffer, SceneGeometry};
use crate::picture::Pictures;
use crate::pool::RenderPool;
use crate::sim::{Sims, may_be_simulated};
use crate::sparkles::Sparkles;
use crate::tiles::{PartSpread, Spread, Tile, Tiles};
use pf_mapping::ChannelMap;
use pf_model::{BufferTransform, PropId, RenderStyle, Show};
use pf_sequence::{Blend, Effect, EffectId, EffectParams, OnParams, RowBlending, Sequence, Target};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A target laid out in a render style, turned or flipped.
type BufferKey = (Target, RenderStyle, BufferTransform);

/// What an effect's frame-by-frame state is kept by: the effect, the buffer it draws on, and the
/// part of a per-model buffer (or [`WHOLE`]).
type SimKey = (EffectId, BufferKey, usize);

/// The whole of a buffer (rather than one of its parts).
const WHOLE: usize = usize::MAX;

/// Drawn flat out, a frame with fewer pixels to draw than this (counting a pixel once for each
/// effect on it) is drawn on the caller's thread: it takes a millisecond at most there, and
/// sharing it out among the shared threads saves a part of that for several times the work.
/// Measured in `docs/benchmarks.md`.
const SHARE_FROM: usize = 60_000;

/// Played by a clock, a frame with fewer pixels to draw than this is drawn on the caller's
/// thread: one thread has it done in half a frame's time or less, and a frame shared out is done
/// only when the slowest of several threads is. On a machine busy with other work that is later
/// than one thread, and now and then much later (`docs/benchmarks.md`, "Played by a clock").
const SHARE_LIVE_FROM: usize = 1_000_000;

/// Which threads a renderer draws on.
#[derive(Debug, Clone)]
enum Threads {
    /// The shared pool, for frames with at least this many pixels to draw.
    Shared(usize),
    /// Always these (none: the caller's thread).
    Own(Option<Arc<RenderPool>>),
}

impl Threads {
    /// The threads a frame with `work` pixels to draw is shared out among (none: it's drawn on
    /// the caller's).
    fn of(&self, work: usize) -> Option<Arc<RenderPool>> {
        match self {
            Self::Shared(from) if work >= *from => RenderPool::shared(),
            Self::Shared(_) => None,
            Self::Own(pool) => pool.clone(),
        }
    }
}

/// A target's buffer in one render style, and where its pixels fall among the tiles.
#[derive(Debug, Clone)]
struct Laid {
    key: BufferKey,
    buffer: PixelBuffer,
    /// Where the buffer falls among the tiles; made the first time a row draws on the target.
    spread: Option<Spread>,
    /// For a per-model buffer: where each member falls; made the first time an effect draws on
    /// the members.
    parts: Option<PartSpread>,
}

/// Renders sequences for one show and channel map. Pixel positions are worked out once, when the
/// renderer is made (make a new one when the show changes); pixel buffers for targets are built
/// the first time a target is drawn and kept, or all at once by [`Renderer::lay_out_ahead`].
#[derive(Debug, Clone)]
pub struct Renderer {
    geometry: SceneGeometry,
    /// The show's pixels, cut into the pieces a frame is drawn in.
    tiles: Tiles,
    /// Each target's buffer in each render style its effects use. All of a target's buffers list
    /// the same pixels in the same order.
    laid: Vec<Laid>,
    laid_ids: HashMap<BufferKey, u32>,
    /// Where each target's faces sit in its buffer (built the first time a Faces effect draws on
    /// the target, like the buffers).
    faces: HashMap<Target, Vec<FaceProp>>,
    /// Each buffer (or part of a per-model buffer) laid on a grid, for blurred effects (built the
    /// first time a blurred effect draws on it, like the buffers).
    grids: Vec<Grid>,
    grid_ids: HashMap<(u32, usize), u32>,
    /// The blurs' working space, used by every blurred effect in turn (see `Scene::blur`).
    blur_space: BlurSpace,
    /// The frame being built, one entry per show pixel.
    show_acc: Vec<Acc>,
    /// The props whose own rows have drawn on the frame being built (see
    /// [`RowBlending::Replace`]).
    drew: Vec<PropId>,
    /// The effects worked out frame by frame (falling snow, Lines, Life, Tendril, VU Meter),
    /// kept from frame to frame by effect and the buffer (or part) they draw on.
    sims: Sims<SimKey>,
    /// The music effects follow (none, or still on its way: they draw as in silence).
    audio: AudioSource,
    /// The pictures Picture effects draw (none: they draw nothing).
    pictures: Pictures,
    /// What the frame being built draws, its lists kept from frame to frame.
    plan: Plan,
    /// Working space threads borrow while they draw, kept from frame to frame.
    space: Workspace,
    threads: Threads,
    /// How long the frame drawn last took, step by step.
    times: FrameTimes,
}

impl Renderer {
    pub fn new(show: &Show, map: &ChannelMap) -> Self {
        Self::from_geometry(SceneGeometry::new(show, map))
    }

    pub fn from_geometry(geometry: SceneGeometry) -> Self {
        Self {
            show_acc: vec![Acc::ZERO; geometry.pixel_count()],
            tiles: Tiles::new(&geometry),
            geometry,
            laid: Vec::new(),
            laid_ids: HashMap::new(),
            faces: HashMap::new(),
            grids: Vec::new(),
            grid_ids: HashMap::new(),
            blur_space: BlurSpace::default(),
            drew: Vec::new(),
            sims: Sims::default(),
            audio: AudioSource::none(),
            pictures: Pictures::none(),
            plan: Plan::default(),
            space: Workspace::default(),
            threads: Threads::Shared(SHARE_LIVE_FROM),
            times: FrameTimes::default(),
        }
    }

    /// Says whether frames are wanted as soon as they can be had (an export) rather than by a
    /// clock (playback, which a renderer is made for). Flat out, every frame worth the handing
    /// over is shared out among [`RenderPool::shared`]'s threads; by a clock, only a frame one
    /// thread wouldn't have ready in time. The frames are the same either way. A renderer given
    /// threads of its own ([`Renderer::set_pool`]) keeps to them.
    pub fn set_flat_out(&mut self, flat_out: bool) {
        if let Threads::Shared(from) = &mut self.threads {
            *from = if flat_out { SHARE_FROM } else { SHARE_LIVE_FROM };
        }
    }

    /// Draws every frame on `pool`'s threads from now on, however little there is to draw, or
    /// on the caller's thread alone (`None`). For tests and measurements: left alone, a renderer
    /// shares [`RenderPool::shared`] and keeps light frames on the caller's thread. The frames
    /// are the same either way.
    pub fn set_pool(&mut self, pool: Option<Arc<RenderPool>>) {
        self.threads = Threads::Own(pool);
    }

    /// Cuts the show into tiles of at most `pixels` pixels from now on, rather than the usual
    /// size (see `tiles.rs`). For tests and measurements: however finely the show is cut, the
    /// frames are the same.
    pub fn set_tile_pixels(&mut self, pixels: usize) {
        self.tiles = Tiles::of(&self.geometry, pixels);
        // Where each buffer falls among the tiles is worked out again when it's next drawn on.
        for laid in &mut self.laid {
            laid.spread = None;
            laid.parts = None;
        }
    }

    /// The music the sequences drawn from now on follow (see [`crate::audio`]).
    pub fn set_audio(&mut self, audio: AudioSource) {
        self.audio = audio;
    }

    pub fn audio(&self) -> &AudioSource {
        &self.audio
    }

    /// The pictures the sequences drawn from now on show (see [`Pictures`]).
    pub fn set_pictures(&mut self, pictures: Pictures) {
        self.pictures = pictures;
    }

    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    /// Starts reading the pictures `seq`'s Picture effects draw that aren't here yet, in the
    /// background, so each is there by the time its effect is first drawn (played live, an
    /// effect would otherwise show without its picture for a moment). False when it's still
    /// reading an earlier lot: ask again with a later frame.
    pub fn read_pictures_ahead(&mut self, seq: &Sequence) -> bool {
        let mut wanted = Vec::new();
        for row in &seq.rows {
            for effect in row.layers.iter().flat_map(|layer| &layer.effects) {
                let EffectParams::Picture(p) = &effect.params else {
                    continue;
                };
                if p.file.trim().is_empty() {
                    continue;
                }
                let laid = self.laid((row.target, effect.render_style, effect.buffer_transform));
                let buffer = &self.laid[laid as usize].buffer;
                // A per-model style draws the picture on each member's own grid.
                let whole = buffer.parts.is_empty().then_some(buffer);
                for buffer in whole.into_iter().chain(buffer.parts.iter().map(|p| &p.buffer)) {
                    if !buffer.pixels.is_empty() {
                        wanted.push((p.file.clone(), crate::picture::wanted(p, canvas_of(buffer)).0));
                    }
                }
            }
        }
        self.pictures.read_ahead(wanted)
    }

    /// Builds what drawing `seq` needs of the show now, rather than the first time each effect
    /// is drawn: every target's buffer in every render style the sequence uses, where each falls
    /// among the tiles, and the grids its blurred effects are drawn on. Played live, a sequence
    /// then never stops to build one in the middle of a frame. Nothing is drawn differently.
    pub fn lay_out_ahead(&mut self, seq: &Sequence) {
        for row in &seq.rows {
            let mut effects = row.layers.iter().flat_map(|layer| &layer.effects).peekable();
            if effects.peek().is_none() {
                continue;
            }
            let base = self.base(row.target);
            if self.laid[base as usize].buffer.is_empty() {
                continue;
            }
            for effect in effects {
                let laid = self.laid((row.target, effect.render_style, effect.buffer_transform));
                let faces = matches!(effect.params, EffectParams::Faces(_));
                if faces {
                    self.faces_of(row.target, laid);
                }
                let parts = self.parts(laid, base, faces);
                // A blur that changes over the effect finds its grid's nearby cells as it goes.
                if effect.blur > 0 {
                    for part in parts_of(parts) {
                        self.grid(laid, part, blur_amount(effect.blur));
                    }
                }
            }
        }
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
        let track = self.audio.track().cloned();
        let pictures = self.pictures.clone();
        let cx = RenderContext::new(
            track
                .as_deref()
                .map(|t| Audio::new(t, seq.frame_ms).following(seq.music_level_source)),
            &seq.timing_tracks,
            seq.frame_ms,
        )
        .with_pictures(&pictures);
        let started = Instant::now();
        self.plan(seq, t_ms, &cx);
        let planned = started.elapsed();
        let pool = self.threads.of(self.plan.work);
        let scene = Scene {
            geometry: &self.geometry,
            tiles: &self.tiles,
            laid: &self.laid,
            faces: &self.faces,
            grids: &self.grids,
            sims: &self.sims,
            space: &self.space,
            seq,
            t_ms,
            cx,
        };
        let (plan, show) = (&mut self.plan, &mut self.show_acc[..]);
        let blur = &mut self.blur_space;
        // A frame whose props' bytes aren't in pixel order is written afterwards, in one go.
        let bytes = if scene.tiles.in_frame_order {
            &mut *frame
        } else {
            &mut []
        };
        let times = match &pool {
            Some(pool) => pool.run(|| scene.draw(plan, blur, show, bytes, true)),
            None => scene.draw(plan, blur, show, bytes, false),
        };
        self.times = FrameTimes {
            plan: planned,
            threads: pool.as_ref().map_or(1, |pool| pool.threads()),
            ..times
        };
        self.sims.sweep();
        if !self.tiles.in_frame_order {
            self.write(frame);
        }
    }

    /// How long each step of the frame drawn last took, and how many threads drew it.
    pub fn frame_times(&self) -> FrameTimes {
        self.times
    }

    /// The buffer for `key`, built the first time it's asked for.
    fn laid(&mut self, key: BufferKey) -> u32 {
        if let Some(&laid) = self.laid_ids.get(&key) {
            return laid;
        }
        let laid = self.laid.len() as u32;
        self.laid.push(Laid {
            key,
            buffer: self.geometry.styled_buffer(key.0, key.1, key.2),
            spread: None,
            parts: None,
        });
        self.laid_ids.insert(key, laid);
        laid
    }

    /// A target's own buffer, the one its rows are built on, with where it falls among the tiles.
    fn base(&mut self, target: Target) -> u32 {
        let base = self.laid((target, RenderStyle::Default, BufferTransform::None));
        let laid = &mut self.laid[base as usize];
        if laid.spread.is_none() {
            laid.spread = Some(Spread::new(&laid.buffer.global, &self.tiles));
        }
        base
    }

    /// How many parts an effect on `laid` draws on, one after another (none: the whole buffer,
    /// as a Faces effect always does), making sure it's known where each falls among the tiles.
    /// `base` is the target's own buffer.
    fn parts(&mut self, laid: u32, base: u32, faces: bool) -> usize {
        let parts = self.laid[laid as usize].buffer.parts.len();
        if faces || parts == 0 {
            return 0;
        }
        if self.laid[laid as usize].parts.is_none()
            && let Some(spread) = &self.laid[base as usize].spread
        {
            let spread = PartSpread::new(&self.laid[laid as usize].buffer, spread, &self.tiles);
            self.laid[laid as usize].parts = Some(spread);
        }
        parts
    }

    /// The grid a blurred effect on `laid` (or one of its parts) is drawn on, with the cells a
    /// blur of `amount` carries to its pixels worked out.
    fn grid(&mut self, laid: u32, part: usize, amount: u32) -> u32 {
        let grid = match self.grid_ids.get(&(laid, part)) {
            Some(&grid) => grid,
            None => {
                let buffer = &self.laid[laid as usize].buffer;
                let buffer = buffer.parts.get(part).map_or(buffer, |part| &part.buffer);
                let grid = self.grids.len() as u32;
                self.grids.push(Grid::new(buffer));
                self.grid_ids.insert((laid, part), grid);
                grid
            }
        };
        self.grids[grid as usize].near_pixels(amount);
        grid
    }

    /// Makes sure it's known where `target`'s faces sit in its buffer (any of its buffers: they
    /// list the same pixels in the same order).
    fn faces_of(&mut self, target: Target, laid: u32) {
        let (geometry, buffer) = (&self.geometry, &self.laid[laid as usize].buffer);
        self.faces
            .entry(target)
            .or_insert_with(|| crate::faces::face_props(geometry, target, buffer));
    }

    /// Works out what the frame at `t_ms` draws: the rows playing, their effects from the bottom
    /// layer up, and the rows each tile has pixels of.
    fn plan(&mut self, seq: &Sequence, t_ms: u64, cx: &RenderContext) {
        self.plan.clear(self.tiles.tiles.len());
        self.drew.clear();
        if t_ms >= seq.duration_ms {
            return;
        }
        for (r, row) in seq.rows.iter().enumerate() {
            let mut active = row
                .layers
                .iter()
                .enumerate()
                .flat_map(|(l, layer)| {
                    let effects = layer.effects.iter().enumerate();
                    effects.map(move |(e, effect)| (l as u32, e as u32, effect))
                })
                .filter(|(_, _, effect)| effect.is_active_at(t_ms))
                .peekable();
            if active.peek().is_none() {
                continue;
            }
            let base = self.base(row.target);
            let len = self.laid[base as usize].buffer.len();
            if len == 0 {
                continue;
            }
            // Whether the row starts from what the rows before it drew on its pixels, and
            // mixes with that.
            let onto = match seq.row_blending {
                RowBlending::Cover => false,
                RowBlending::Blend => true,
                RowBlending::Replace => {
                    matches!(row.target, Target::Region { prop, .. } if self.drew.contains(&prop))
                }
            };
            if let Target::Prop(prop) = row.target {
                self.drew.push(prop);
            }
            let first_op = self.plan.ops.len() as u32;
            // Layers draw bottom (first) to top (last). With nothing below it to mix with,
            // the lowest effect drawn covers, whatever its blend (as in xLights).
            for (n, (layer, place, source)) in active.enumerate() {
                let blend = if n == 0 && !onto {
                    Blend::Normal
                } else {
                    source.blend
                };
                let key: BufferKey = (row.target, source.render_style, source.buffer_transform);
                let laid = self.laid(key);
                let faces = matches!(source.params, EffectParams::Faces(_));
                if faces {
                    self.faces_of(row.target, laid);
                }
                // A per-model style: the effect draws on each member's own buffer.
                let parts = self.parts(laid, base, faces);
                let blur = blur_at(source, t_ms, cx);
                let simulated = !faces && may_be_simulated(&source.params);
                let first_unit = self.plan.units.len() as u32;
                for part in parts_of(parts) {
                    let blur = (blur > 0).then(|| {
                        let amount = blur_amount(blur);
                        let grid = self.grid(laid, part, amount);
                        Blurred {
                            grid,
                            amount,
                            working: self.grids[grid as usize].working(amount),
                        }
                    });
                    // Two effects with one id on one buffer share their state: see `Scene::draw`.
                    if simulated && !self.sims.reserve((source.id, key, part)) {
                        self.plan.shared_state = true;
                    }
                    let cells = blur.map_or(0, |blur| self.grids[blur.grid as usize].cells.len());
                    self.plan.unit(part, blur, cells, simulated, faces);
                }
                self.plan.ops.push(Op {
                    at: (r as u32, layer, place),
                    laid,
                    blend,
                    faces,
                    whole: parts == 0,
                    units: (first_unit, self.plan.units.len() as u32),
                    now: None,
                    time: EffectTime::within(source.start_ms, source.end_ms, t_ms),
                    fade: 0.0,
                    draws: false,
                    sparkles: None,
                    adjust: None,
                });
                self.plan.work += len;
            }
            let planned = self.plan.rows.len() as u32;
            self.plan.rows.push(PlannedRow {
                base,
                onto,
                ops: (first_op, self.plan.ops.len() as u32),
            });
            let spread = self.laid[base as usize].spread.as_ref();
            for part in spread.into_iter().flat_map(|spread| &spread.tiles) {
                self.plan.tiles[part.tile as usize].push(RowInTile {
                    row: planned,
                    runs: part.runs,
                    pixels: part.pixels,
                });
            }
        }
    }

    /// Writes the whole frame from the pixels as built, prop by prop.
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

/// xLights' Blur setting for PixelFlow's `blur` (one more).
fn blur_amount(blur: u32) -> u32 {
    blur.min(pf_sequence::MAX_BLUR) + 1
}

/// `effect`'s blur at `t_ms`: as set, unless a curve changes it over the effect.
fn blur_at(effect: &Effect, t_ms: u64, cx: &RenderContext) -> u32 {
    if effect.curves.contains_key("blur") {
        cx.effect_at(effect, t_ms).blur
    } else {
        effect.blur
    }
}

/// What an effect draws on, one after another: the whole buffer, or each of its `parts`.
fn parts_of(parts: usize) -> impl Iterator<Item = usize> {
    (parts == 0).then_some(WHOLE).into_iter().chain(0..parts)
}

/// The grid an effect draws on for a buffer.
fn canvas_of(buffer: &PixelBuffer) -> Canvas {
    Canvas {
        columns: buffer.columns,
        rows: buffer.rows,
    }
}

/// How much of the effect shows at `t_ms`, from its fade-in and fade-out, stepped frame by frame
/// as xLights steps them (`PixelBufferClass::HandleLayerTransitions`): a fade lasts a whole
/// number of frames; fading in, the effect's first frame is already one step up; fading out,
/// its last frame is dark; and where the two overlap, it shows their average.
pub(crate) fn fade_level(effect: &Effect, t_ms: u64, frame_ms: u32) -> f32 {
    let frame = u64::from(frame_ms.max(1));
    let (fade_in, fade_out) = (
        u64::from(effect.fade_in_ms) / frame,
        u64::from(effect.fade_out_ms) / frame,
    );
    let at = t_ms / frame;
    let since = at.saturating_sub(effect.start_ms / frame);
    let left = (effect.end_ms.saturating_sub(1) / frame).saturating_sub(at);
    let fading_in = (since < fade_in).then(|| (since + 1) as f32 / fade_in as f32);
    let fading_out = (left < fade_out).then(|| left as f32 / fade_out as f32);
    match (fading_in, fading_out) {
        (Some(up), Some(down)) => (up + down) / 2.0,
        (Some(level), None) | (None, Some(level)) => level,
        (None, None) => 1.0,
    }
}

/// One row playing in the frame being built.
#[derive(Debug, Clone, Copy)]
struct PlannedRow {
    /// The target's own buffer: the row's pixels, in its order.
    base: u32,
    /// Whether it starts from what the rows before drew on its pixels.
    onto: bool,
    /// Its effects, bottom layer first, as places in [`Plan`]'s ops.
    ops: (u32, u32),
}

/// One row's part of one tile.
#[derive(Debug, Clone, Copy)]
struct RowInTile {
    row: u32,
    /// The runs of the row's buffer that fall in the tile (places in its [`Spread`]'s runs).
    runs: (u32, u32),
    pixels: u32,
}

/// One effect playing in the frame being built.
struct Op {
    /// Where it is in the sequence: its row, its layer, and its place in the layer.
    at: (u32, u32, u32),
    /// The buffer it draws on.
    laid: u32,
    /// Its blend, or Normal for the lowest effect on a row that starts clear.
    blend: Blend,
    faces: bool,
    /// Whether it draws on the whole buffer rather than on each part of a per-model one.
    whole: bool,
    /// What it draws on, as places in [`Plan`]'s units: the whole buffer, or each part.
    units: (u32, u32),
    // Worked out when the effect is made ready.
    /// The effect with its settings as they are at this moment, when curves change them.
    now: Option<Effect>,
    time: EffectTime,
    /// How much of it shows, from its fades.
    fade: f32,
    /// False when it's fully faded under a blend that then changes nothing.
    draws: bool,
    sparkles: Option<Sparkles>,
    adjust: Option<Adjust>,
}

/// How an effect on one buffer is blurred.
#[derive(Debug, Clone, Copy)]
struct Blurred {
    /// The grid it's blurred on.
    grid: u32,
    /// By how much (xLights' Blur setting).
    amount: u32,
    /// What the blur does with its working space, which decides when it's blurred (see
    /// `Scene::blur`).
    working: Working,
}

/// An effect on one buffer: the whole of its target's, or one member's of a per-model one.
struct Unit {
    /// The part of the buffer (or [`WHOLE`]).
    part: usize,
    blur: Option<Blurred>,
    /// Whether it may carry on from the frame before (see [`Sims`]).
    simulated: bool,
    // Worked out when the effect is made ready.
    shader: Option<Shader>,
    halves: Halves,
    /// A blurred effect's color in each cell of its grid.
    cells: Vec<Rgba>,
    /// The working space of a blur that fills it ([`Working::Filled`]), as the blur left it, and
    /// more for its passes to be shared out among threads.
    working: Vec<[f32; 4]>,
    turned: Vec<[f32; 4]>,
    /// A Faces effect's lit pixels, until its shader takes them.
    lit: Vec<Option<Rgba>>,
}

/// What one frame draws. The lists are filled by [`Renderer::plan`] and kept from frame to
/// frame, with the memory the effects' colors took.
#[derive(Default)]
struct Plan {
    rows: Vec<PlannedRow>,
    ops: Vec<Op>,
    units: Vec<Unit>,
    /// For each tile, the rows with pixels in it, in the sequence's order.
    tiles: Vec<Vec<RowInTile>>,
    /// Pixels to draw, counting a pixel once for each effect on it.
    work: usize,
    /// Two of the frame's effects share frame-by-frame state (see [`Sims::reserve`]).
    shared_state: bool,
    spare_cells: Vec<Vec<Rgba>>,
    spare_working: Vec<Vec<[f32; 4]>>,
    spare_lit: Vec<Vec<Option<Rgba>>>,
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plan({} rows, {} effects)", self.rows.len(), self.ops.len())
    }
}

/// A renderer's copy plans its own frames.
impl Clone for Plan {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Plan {
    /// Empties the lists for a frame of `tiles` tiles, keeping their memory.
    fn clear(&mut self, tiles: usize) {
        self.rows.clear();
        self.ops.clear();
        for unit in self.units.drain(..) {
            if unit.cells.capacity() > 0 {
                self.spare_cells.push(unit.cells);
            }
            for working in [unit.working, unit.turned] {
                if working.capacity() > 0 {
                    self.spare_working.push(working);
                }
            }
            let lit = match unit.shader {
                Some(Shader::Faces(faces)) => faces.into_lit(),
                _ => unit.lit,
            };
            if lit.capacity() > 0 {
                self.spare_lit.push(lit);
            }
        }
        self.tiles.resize_with(tiles, Vec::new);
        self.tiles.iter_mut().for_each(Vec::clear);
        self.work = 0;
        self.shared_state = false;
    }

    /// Adds an effect on one buffer: blurred on `blur`'s grid of `cells` cells, when it's blurred.
    fn unit(&mut self, part: usize, blur: Option<Blurred>, cells: usize, simulated: bool, faces: bool) {
        // What these lists held never shows: every cell read is written first.
        let filled = blur.is_some_and(|blur| blur.working == Working::Filled);
        let space = |spare: &mut Vec<Vec<[f32; 4]>>, wanted: bool, len: usize| match wanted {
            true => fitting(spare, len),
            false => Vec::new(),
        };
        let working = space(&mut self.spare_working, filled, 2 * cells);
        let turned = space(
            &mut self.spare_working,
            filled && cells >= SHARE_FROM_CELLS,
            cells,
        );
        let cells = match blur {
            Some(_) => fitting(&mut self.spare_cells, cells),
            None => Vec::new(),
        };
        let lit = match faces {
            true => self.spare_lit.pop().unwrap_or_default(),
            false => Vec::new(),
        };
        self.units.push(Unit {
            part,
            blur,
            simulated,
            shader: None,
            halves: Halves::None,
            cells,
            working,
            turned,
            lit,
        });
    }
}

/// Takes a list with room for `len` out of `spare`: the smallest that has it, else the largest
/// (which grows), so the lists stay about the sizes a frame's blurred effects need between them.
fn fitting<T>(spare: &mut Vec<Vec<T>>, len: usize) -> Vec<T> {
    let size = |i: &usize| spare[*i].capacity();
    let all = 0..spare.len();
    let fits = all.clone().filter(|i| size(i) >= len).min_by_key(size);
    match fits.or_else(|| all.max_by_key(size)) {
        Some(i) => spare.swap_remove(i),
        None => Vec::new(),
    }
}

/// A tile's part of a row being built, and of one member of a per-model row.
#[derive(Debug, Default)]
struct RowSpace {
    row: Vec<Acc>,
    part: Vec<Acc>,
}

/// The blurs' working space.
#[derive(Debug, Clone, Default)]
struct BlurSpace {
    /// Two grids' worth, which every blur works in. What one blur leaves here the next ones take
    /// in (see `Scene::blur`), so there is one of these, never emptied.
    grids: Vec<[f32; 4]>,
    /// A third grid's worth, for a pass shared out among threads. What it held never shows.
    turned: Vec<[f32; 4]>,
    /// The blurs that worked in a space of their own and whose leavings aren't in `grids` yet,
    /// in the order they're drawn: the unit, and how much of the start of its space it filled.
    left: Vec<(usize, usize)>,
}

impl BlurSpace {
    /// Takes in what the blurs in `left` left in their own spaces, as if each had worked here in
    /// turn. Each filled the start of the space, so what's left of one is what the ones after it
    /// didn't cover.
    fn take_in(&mut self, units: &[Unit]) {
        let most = self.left.iter().map(|&(_, filled)| filled).max().unwrap_or(0);
        if self.grids.len() < most {
            self.grids.resize(most, [0.0; 4]);
        }
        let mut covered = 0;
        for &(unit, filled) in self.left.iter().rev() {
            if filled > covered {
                self.grids[covered..filled].copy_from_slice(&units[unit].working[covered..filled]);
                covered = filled;
            }
        }
        self.left.clear();
    }
}

/// Working space the threads drawing a frame borrow and give back. What it holds from one use
/// never shows in the next: only its memory is kept.
#[derive(Debug, Default)]
struct Workspace {
    rows: Mutex<Vec<RowSpace>>,
}

/// A renderer's copy gathers its own.
impl Clone for Workspace {
    fn clone(&self) -> Self {
        Self::default()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Workspace {
    fn row(&self) -> RowSpace {
        lock(&self.rows).pop().unwrap_or_default()
    }

    fn give_row(&self, space: RowSpace) {
        lock(&self.rows).push(space);
    }
}

/// How long each step of drawing a frame took (see this module's notes), for measuring where a
/// frame's time goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameTimes {
    /// Working out what the frame draws.
    pub plan: Duration,
    /// Making its effects ready, with the blurs that don't have to wait their turn.
    pub ready: Duration,
    /// The blurs that have to be done in turn.
    pub blur: Duration,
    /// Drawing the tiles and writing their bytes.
    pub tiles: Duration,
    /// The threads the frame was drawn on (one: the caller's).
    pub threads: usize,
}

/// Does both halves of a piece of work: at the same time, on the pool's threads, when `together`
/// (it's then called on one of them), else one after the other.
fn both(together: bool, left: impl FnOnce() + Send, right: impl FnOnce() + Send) {
    if together {
        rayon::join(left, right);
    } else {
        left();
        right();
    }
}

/// Everything the threads drawing a frame read.
struct Scene<'a> {
    geometry: &'a SceneGeometry,
    tiles: &'a Tiles,
    laid: &'a [Laid],
    faces: &'a HashMap<Target, Vec<FaceProp>>,
    grids: &'a [Grid],
    sims: &'a Sims<SimKey>,
    space: &'a Workspace,
    seq: &'a Sequence,
    t_ms: u64,
    /// The music and timing tracks the effects follow.
    cx: RenderContext<'a>,
}

impl Scene<'_> {
    /// Makes the planned effects ready, then draws the tiles into `show` and writes them to
    /// `frame`: on the pool's threads when `shared` (it's then called on one of them), else on
    /// this one. Says how long each step took.
    fn draw(
        &self,
        plan: &mut Plan,
        blur: &mut BlurSpace,
        show: &mut [Acc],
        frame: &mut [u8],
        shared: bool,
    ) -> FrameTimes {
        // Effects that share frame-by-frame state find it as the one before them left it, so
        // they're made ready in the order they're drawn.
        let together = shared && !plan.shared_state;
        let started = Instant::now();
        self.make_ready(&mut plan.ops, &mut plan.units, 0, together, shared);
        let ready = Instant::now();
        self.blur(&plan.ops, &mut plan.units, blur, shared);
        let blurred = Instant::now();
        self.draw_tiles(plan, 0..self.tiles.tiles.len(), show, frame, 0, shared);
        FrameTimes {
            ready: ready - started,
            blur: blurred - ready,
            tiles: blurred.elapsed(),
            ..FrameTimes::default()
        }
    }

    /// Makes `ops` ready, each with its units (`units`, which start at place `first` in the
    /// plan's): halving the list until one effect is left, the halves at the same time, when
    /// `together`; else one after another. `shared` says whether there are threads to share one
    /// effect's work with.
    fn make_ready(&self, ops: &mut [Op], units: &mut [Unit], first: u32, together: bool, shared: bool) {
        if !together || ops.len() <= 1 {
            for op in ops {
                self.ready_op(op);
                for unit in &mut units[(op.units.0 - first) as usize..(op.units.1 - first) as usize] {
                    self.ready_unit(op, unit, shared);
                }
            }
            return;
        }
        let (left, right) = ops.split_at_mut(ops.len() / 2);
        let middle = right[0].units.0;
        let (left_units, right_units) = units.split_at_mut((middle - first) as usize);
        rayon::join(
            move || self.make_ready(left, left_units, first, together, shared),
            move || self.make_ready(right, right_units, middle, together, shared),
        );
    }

    /// The effect as the sequence has it.
    fn source(&self, op: &Op) -> &Effect {
        let (row, layer, place) = op.at;
        &self.seq.rows[row as usize].layers[layer as usize].effects[place as usize]
    }

    /// Works out what every buffer the effect draws on shares: its settings at this moment, how
    /// much of it shows, its sparkles, and its color adjustments.
    fn ready_op(&self, op: &mut Op) {
        let now = self.cx.effect_at(self.source(op), self.t_ms);
        let effect = &*now;
        let frame_ms = self.seq.frame_ms;
        op.time = EffectTime::within(effect.start_ms, effect.end_ms, self.t_ms).with_frame_ms(frame_ms);
        op.fade = fade_level(effect, self.t_ms, frame_ms);
        // A fully faded effect changes nothing, except under the blends that black out what's
        // below where the effect is unlit.
        let plain = matches!(
            op.blend,
            Blend::Normal | Blend::Add | Blend::Max | Blend::Multiply
        );
        op.draws = !(op.fade <= 0.0 && plain);
        let c = effect.sparkle_color;
        // An effect's own Transparency hides the effect and not its sparkles: they go by the
        // effect as it is without it (see `ready_unit`) and take it off where they don't flash.
        let transparency = match &effect.params {
            EffectParams::On(p) => p.transparency,
            _ => 0.0,
        };
        op.sparkles = Sparkles::new(
            effect.sparkles,
            [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0),
            effect.id.seed(),
            op.time.elapsed_ms / u64::from(frame_ms.max(1)),
        )
        .map(|sparkles| sparkles.see_through(transparency));
        op.adjust = Adjust::new(effect, op.blend);
        op.now = match now {
            Cow::Owned(now) => Some(now),
            Cow::Borrowed(_) => None,
        };
    }

    /// Makes the effect ready to draw on one buffer: its shader there and, when it's blurred, what
    /// it draws on the buffer's grid.
    fn ready_unit(&self, op: &Op, unit: &mut Unit, shared: bool) {
        let source = self.source(op);
        let effect = op.now.as_ref().unwrap_or(source);
        let laid = &self.laid[op.laid as usize];
        let buffer = laid
            .buffer
            .parts
            .get(unit.part)
            .map_or(&laid.buffer, |p| &p.buffer);
        let canvas = canvas_of(buffer);
        unit.halves = Halves::new(op.blend, canvas);
        // An effect that carries on from the frame before moves on a frame even while it's
        // faded out.
        let simulated = match unit.simulated {
            true => self.sims.shader(
                &(source.id, laid.key, unit.part),
                source,
                &op.time,
                canvas,
                &self.cx,
            ),
            false => None,
        };
        if !op.draws {
            return;
        }
        let shader = match (&effect.params, simulated) {
            (EffectParams::Faces(p), _) if op.faces => {
                let faces = self.faces.get(&laid.key.0).map_or(&[][..], Vec::as_slice);
                let mut lit = std::mem::take(&mut unit.lit);
                crate::faces::lit_pixels(
                    p,
                    effect,
                    self.t_ms,
                    self.seq,
                    self.geometry,
                    faces,
                    buffer.len(),
                    &mut lit,
                );
                Shader::Faces(crate::effects::Faces::new(lit))
            }
            (_, Some(simulated)) => simulated,
            (params, None) => {
                // Drawn without its own Transparency when its sparkles take that off.
                let whole;
                let params = match params {
                    EffectParams::On(p) if op.sparkles.is_some_and(|s| s.takes_transparency()) => {
                        whole = EffectParams::On(OnParams {
                            transparency: 0.0,
                            ..*p
                        });
                        &whole
                    }
                    _ => params,
                };
                Shader::in_context(
                    params,
                    &op.time,
                    Colors::new(&effect.palette.colors),
                    effect.id.seed(),
                    canvas,
                    &self.cx.with_members(buffer.members.as_ref()),
                )
            }
        };
        if let Some(blur) = unit.blur {
            // Blurred: drawn on the grid and blurred there. Only the cells the blur carries to a
            // pixel are drawn; the rest stay as they were.
            let grid = &self.grids[blur.grid as usize];
            shader.with(Cells {
                grid,
                near: grid.near(blur.amount),
                cells: &mut unit.cells,
                shared,
            });
            // A blur that fills its working space needn't wait its turn (see `blur`): it's
            // blurred here, in a space of its own.
            if blur.working == Working::Filled {
                crate::blur::blur_near(
                    &mut unit.cells,
                    grid.columns,
                    grid.rows,
                    blur.amount,
                    op.blend != Blend::Normal,
                    &mut unit.working,
                    grid.near(blur.amount),
                    shared.then_some(&mut unit.turned),
                );
            }
        }
        unit.shader = Some(shader);
    }

    /// Blurs the blurred effects' grids that have to be blurred in turn, and leaves the blurs'
    /// one working space as it would be had every blur worked in it, one after another in the
    /// order they're drawn. With threads to share with (`shared`), the passes over a large grid
    /// are shared out among them.
    ///
    /// The order matters to a blur of a grid with gaps between its pixels. With the cells just
    /// past the ends of each run of cells it works on, it takes in whatever the blur before it
    /// left there. In exact arithmetic that goes out again as the sum moves along; in floating
    /// point a trace stays, and some of what's drawn after (a blend that asks whether a pixel is
    /// lit at all, a color adjustment that lights unlit pixels) turns a trace into a pixel on or
    /// off. So a frame's bytes depend on what each of those blurs finds in the space, and they
    /// find what they always did.
    ///
    /// A blur of a grid that's all pixels reads nothing it didn't write, so it was blurred when
    /// its effect was made ready, at the same time as the others. All it owes is what it left:
    /// it filled the start of its own space, twice its grid's cells, and that's copied into the
    /// one space before the next blur that reads it (and at the end, for the next frame's).
    fn blur(&self, ops: &[Op], units: &mut [Unit], space: &mut BlurSpace, shared: bool) {
        space.left.clear();
        for op in ops.iter().filter(|op| op.draws) {
            for index in op.units.0 as usize..op.units.1 as usize {
                let Some(blur) = units[index].blur else {
                    continue;
                };
                let grid = &self.grids[blur.grid as usize];
                match blur.working {
                    Working::Untouched => {}
                    Working::Filled => space.left.push((index, 2 * grid.cells.len())),
                    Working::Read => {
                        space.take_in(units);
                        crate::blur::blur_near(
                            &mut units[index].cells,
                            grid.columns,
                            grid.rows,
                            blur.amount,
                            op.blend != Blend::Normal,
                            &mut space.grids,
                            grid.near(blur.amount),
                            shared.then_some(&mut space.turned),
                        );
                    }
                }
            }
        }
        space.take_in(units);
    }

    /// Draws tiles `tiles`, whose pixels are `show` and whose bytes start at place `at` of the
    /// frame with `frame`: halving the run of tiles until one is left, the halves at the same
    /// time when `shared`. No two tiles share a pixel or a byte, so each half has its own.
    fn draw_tiles(
        &self,
        plan: &Plan,
        tiles: std::ops::Range<usize>,
        show: &mut [Acc],
        frame: &mut [u8],
        at: usize,
        shared: bool,
    ) {
        match tiles.len() {
            0 => {}
            1 => self.draw_tile(plan, tiles.start, show, frame, at),
            len => {
                let middle = tiles.start + len / 2;
                let (first, next) = (&self.tiles.tiles[tiles.start], &self.tiles.tiles[middle]);
                let (left_show, right_show) = show.split_at_mut(next.first - first.first);
                // A frame shorter than the show's ends where it ends.
                let cut = next.bytes.max(at).min(at + frame.len());
                let (left_frame, right_frame) = frame.split_at_mut(cut - at);
                let (left, right) = (tiles.start..middle, middle..tiles.end);
                both(
                    shared,
                    move || self.draw_tiles(plan, left, left_show, left_frame, at, shared),
                    move || self.draw_tiles(plan, right, right_show, right_frame, cut, shared),
                );
            }
        }
    }

    /// Draws one tile: every row with pixels in it, in the sequence's order, then its bytes.
    fn draw_tile(&self, plan: &Plan, index: usize, show: &mut [Acc], frame: &mut [u8], at: usize) {
        let tile = &self.tiles.tiles[index];
        let show = &mut show[..tile.len];
        show.fill(Acc::ZERO);
        let rows = &plan.tiles[index];
        if !rows.is_empty() {
            let mut space = self.space.row();
            for row in rows {
                self.draw_row(plan, tile, index as u32, row, show, &mut space);
            }
            self.space.give_row(space);
        }
        for prop in &self.geometry.props[tile.props.clone()] {
            let cpp = usize::from(prop.channels_per_pixel);
            if cpp < 3 {
                continue;
            }
            // The prop's pixels in the tile.
            let first = tile.first.max(prop.first_pixel);
            let end = (tile.first + tile.len).min(prop.first_pixel + prop.points.len());
            for pixel in first..end {
                let acc = show[pixel - tile.first];
                let start = prop.frame_offset + (pixel - prop.first_pixel) * cpp;
                let bytes = start
                    .checked_sub(at)
                    .and_then(|start| frame.get_mut(start..start + cpp));
                if let Some(bytes) = bytes {
                    write_pixel(bytes, [to_u8(acc.r), to_u8(acc.g), to_u8(acc.b)]);
                }
            }
        }
    }

    /// Draws one row's part of a tile and lays it over what the rows before drew there.
    fn draw_row(
        &self,
        plan: &Plan,
        tile: &Tile,
        index: u32,
        part: &RowInTile,
        show: &mut [Acc],
        space: &mut RowSpace,
    ) {
        let row = &plan.rows[part.row as usize];
        let base = &self.laid[row.base as usize];
        let Some(spread) = &base.spread else {
            return;
        };
        let runs = spread.runs(part.runs);
        let global = &base.buffer.global;
        let pixel = |g: u32| g as usize - tile.first;
        let RowSpace { row: acc, part: own } = space;
        acc.clear();
        if row.onto {
            // Each pixel's own color: xLights keeps what's there on a layer below the row's own,
            // laid out as a single line so no two pixels share a cell.
            for &(from, to) in runs {
                acc.extend(global[from as usize..to as usize].iter().map(|&g| show[pixel(g)]));
            }
        } else {
            acc.resize(part.pixels as usize, Acc::ZERO);
        }
        for op in &plan.ops[row.ops.0 as usize..row.ops.1 as usize] {
            if !op.draws {
                continue;
            }
            let laid = &self.laid[op.laid as usize];
            let units = &plan.units[op.units.0 as usize..op.units.1 as usize];
            if op.whole {
                let mut done = 0;
                for &(from, to) in runs {
                    let (from, to) = (from as usize, to as usize);
                    let acc = &mut acc[done..done + (to - from)];
                    done += to - from;
                    // As far as the buffer goes, should it list fewer pixels than the row's own.
                    let to = to.min(laid.buffer.pixels.len());
                    if from < to {
                        let pixels = &laid.buffer.pixels[from..to];
                        op.mix(&units[0], self.grids, pixels, from, &mut acc[..to - from]);
                    }
                }
                continue;
            }
            // A per-model style: each member in the tile takes its pixels from the row, draws on
            // them as its own buffer lays them out, and puts them back.
            let Some(parts) = &laid.parts else {
                continue;
            };
            for member in parts.in_tile(index) {
                let (unit, places) = (&units[member.part as usize], &parts.at[member.part as usize]);
                let pixels = &laid.buffer.parts[member.part as usize].buffer.pixels;
                let runs = parts.runs(member.runs);
                own.clear();
                for &(from, to) in runs {
                    let places = &places[from as usize..to as usize];
                    own.extend(places.iter().map(|&place| acc[place as usize]));
                }
                let mut done = 0;
                for &(from, to) in runs {
                    let (from, to) = (from as usize, to as usize);
                    let own = &mut own[done..done + (to - from)];
                    op.mix(unit, self.grids, &pixels[from..to], from, own);
                    done += to - from;
                }
                let runs = runs
                    .iter()
                    .flat_map(|&(from, to)| &places[from as usize..to as usize]);
                for (&place, &mixed) in runs.zip(own.iter()) {
                    acc[place as usize] = mixed;
                }
            }
        }
        let covers = self.seq.row_blending == RowBlending::Cover;
        let mut done = 0;
        for &(from, to) in runs {
            let (from, to) = (from as usize, to as usize);
            for (&g, &top) in global[from..to].iter().zip(&acc[done..]) {
                let under = &mut show[pixel(g)];
                if !covers {
                    // xLights' rows write every pixel they have, lit or not.
                    *under = top;
                } else if top.a > 0.0 {
                    under.cover_with(top);
                }
            }
            done += to - from;
        }
    }
}

impl Op {
    /// Mixes the effect onto `acc`, the layers below it at `pixels` (which start at place `from`
    /// of the unit's buffer): shaded (or its blurred colors taken), its colors adjusted around
    /// its sparkles, faded, and mixed in by its blend.
    fn mix(&self, unit: &Unit, grids: &[Grid], pixels: &[Pixel], from: usize, acc: &mut [Acc]) {
        let Some(shader) = &unit.shader else {
            return;
        };
        let Some(blur) = unit.blur else {
            shader.with(Fill {
                pixels,
                acc,
                blend: self.blend,
                fade: self.fade,
                sparkles: self.sparkles,
                adjust: self.adjust,
                halves: unit.halves,
            });
            return;
        };
        // Blurred: each pixel takes its cell's color.
        let Some(cells) = grids[blur.grid as usize].cell_of.get(from..from + pixels.len()) else {
            return;
        };
        for ((px, &cell), acc) in pixels.iter().zip(cells).zip(acc) {
            let mut color = unit.cells[cell as usize];
            if let Some(a) = &self.adjust {
                color = a.shift(color);
            }
            if let Some(s) = &self.sparkles {
                color = s.apply(color, px.index);
            }
            if let Some(a) = &self.adjust {
                color = a.level(color);
            }
            acc.blend(color, self.blend, self.fade, unit.halves.first(px));
        }
    }
}

/// Shades the cells of a grid near its pixels (every cell when `near` is `None`): a large grid's
/// in pieces at the same time, when there are threads to share with (`shared`).
struct Cells<'a> {
    grid: &'a Grid,
    near: Option<&'a crate::blur::Near>,
    cells: &'a mut Vec<Rgba>,
    shared: bool,
}

impl ShaderVisitor<()> for Cells<'_> {
    #[inline]
    fn visit<S: Shade + Sync>(self, shader: &S) {
        match self.near {
            None => {
                self.cells.clear();
                self.cells
                    .extend(self.grid.cells.iter().map(|px| shader.shade(px)));
            }
            Some(near) => {
                // The other cells needn't be clear: nothing reads them.
                self.cells.resize(self.grid.cells.len(), Rgba::CLEAR);
                let grid = self.grid;
                let piece = (self.shared && near.cells.len() >= SHARE_FROM_CELLS).then_some(PIECE_OF_CELLS);
                in_pieces(
                    &near.cells,
                    self.cells,
                    0,
                    piece,
                    &|&cell| cell as usize,
                    &|part, out, at| {
                        for &cell in part {
                            let cell = cell as usize;
                            out[cell - at] = shader.shade(&grid.cells[cell]);
                        }
                    },
                );
            }
        }
    }
}

/// Shades each pixel and mixes it in.
struct Fill<'a> {
    pixels: &'a [Pixel],
    acc: &'a mut [Acc],
    blend: Blend,
    fade: f32,
    sparkles: Option<Sparkles>,
    adjust: Option<Adjust>,
    halves: Halves,
}

impl ShaderVisitor<()> for Fill<'_> {
    #[inline]
    fn visit<S: Shade + Sync>(self, shader: &S) {
        let Some(adjust) = self.adjust else {
            for (px, acc) in self.pixels.iter().zip(self.acc.iter_mut()) {
                let mut color: Rgba = shader.shade(px);
                if let Some(s) = &self.sparkles {
                    color = s.apply(color, px.index);
                }
                acc.blend(color, self.blend, self.fade, self.halves.first(px));
            }
            return;
        };
        for (px, acc) in self.pixels.iter().zip(self.acc.iter_mut()) {
            let mut color = adjust.shift(shader.shade(px));
            if let Some(s) = &self.sparkles {
                color = s.apply(color, px.index);
            }
            acc.blend(adjust.level(color), self.blend, self.fade, self.halves.first(px));
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
