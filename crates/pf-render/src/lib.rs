//! Renders PixelFlow sequences.
//!
//! [`Renderer`] draws a [`pf_sequence::Sequence`] at any moment into the show frame (prop order,
//! RGB or RGBW per pixel: the same frame test patterns paint and the preview shows). Every target
//! is drawn on a **pixel buffer**: its pixels' front-view positions scaled to its bounding box, so
//! effects work on any shape, and a group draws across all its members as one canvas.
//!
//! Rendering is deterministic: frame N depends only on the document (and the music, for effects
//! that follow it: see [`audio`], and the picture files Picture effects draw: see [`Pictures`]), so
//! seeking and export always give the same picture. [`export`]
//! writes a sequence as an FPP `.fseq` file.
//!
//! A large frame's work is shared out among threads ([`RenderPool`]); its bytes are the same
//! however many draw it. One thing in a frame does still depend on the frames drawn before it:
//! the last decimal places of a blurred effect on a buffer with gaps between its pixels (see
//! `Scene::blur` in `render.rs`), which a few blends turn into whole pixels.

mod adjust;
pub mod audio;
mod blur;
mod butterfly;
mod carried;
mod circles;
mod color;
mod color_shift;
mod dancer;
mod effects;
pub mod export;
pub mod faces;
mod fan;
mod fill;
mod fire;
mod fireworks;
mod garlands;
mod geometry;
mod impact;
mod life;
mod lightning;
mod lines;
mod morph;
mod picture;
mod pinwheel;
mod plasma;
mod pool;
mod pulse;
mod raster;
mod render;
mod shape;
mod shockwave;
mod sim;
mod sing;
mod snowflakes;
mod sparkles;
mod styles;
mod tendril;
mod text;
mod tiles;
mod vumeter;
mod wipe;

pub use audio::{Audio, AudioFill, AudioSource, AudioTrack, RenderContext, follows_music};
pub use color::{Colors, Rgba};
pub use effects::{
    Butterfly, Canvas, Chase, Circles, ColorShift, DEFAULT_FRAME_MS, Dancer, EffectTime, Faces, Fan,
    Fireworks, Garlands, Impact, Life, Lightning, Lines, MAX_METEORS, Morph, Picture, Pinwheel, Plasma,
    Pulse, Shade, Shader, Shape, Sing, Snowflakes, Tendril, Text, VuMeter, Wipe, shade_pixel,
};
pub use geometry::{Members, Pixel, PixelBuffer, SceneGeometry};
pub use picture::{Pictures, ReadPicture};
pub use pool::{RenderPool, default_threads};
pub use render::{FrameTimes, Renderer};
pub use sing::{mouth_open, openness};
