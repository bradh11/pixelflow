//! Renders PixelFlow sequences.
//!
//! [`Renderer`] draws a [`pf_sequence::Sequence`] at any moment into the show frame (prop order,
//! RGB or RGBW per pixel: the same frame test patterns paint and the preview shows). Every target
//! is drawn on a **pixel buffer**: its pixels' front-view positions scaled to its bounding box, so
//! effects work on any shape, and a group draws across all its members as one canvas.
//!
//! Rendering is deterministic: frame N depends only on the document (and the music, for effects
//! that follow it: see [`audio`]), so seeking and export always give the same picture. [`export`]
//! writes a sequence as an FPP `.fseq` file.

pub mod audio;
mod blur;
mod butterfly;
mod circles;
mod color;
mod effects;
pub mod export;
pub mod faces;
mod fan;
mod garlands;
mod geometry;
mod life;
mod lines;
mod morph;
mod pinwheel;
mod plasma;
mod raster;
mod render;
mod shape;
mod sim;
mod snowflakes;
mod sparkles;
mod styles;
mod tendril;
mod text;
mod vumeter;

pub use audio::{Audio, AudioFill, AudioSource, AudioTrack, RenderContext};
pub use color::{Colors, Rgba};
pub use effects::{
    Butterfly, Canvas, Circles, DEFAULT_FRAME_MS, EffectTime, Faces, Fan, Garlands, Life, Lines, MAX_METEORS,
    Morph, Pinwheel, Plasma, Shade, Shader, Shape, Snowflakes, Tendril, Text, VuMeter, shade_pixel,
};
pub use geometry::{Pixel, PixelBuffer, SceneGeometry};
pub use render::Renderer;
