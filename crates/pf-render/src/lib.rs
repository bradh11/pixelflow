//! Renders PixelFlow sequences.
//!
//! [`Renderer`] draws a [`pf_sequence::Sequence`] at any moment into the show frame (prop order,
//! RGB or RGBW per pixel: the same frame test patterns paint and the preview shows). Every target
//! is drawn on a **pixel buffer**: its pixels' front-view positions scaled to its bounding box, so
//! effects work on any shape, and a group draws across all its members as one canvas.
//!
//! Rendering is deterministic: frame N depends only on the document, so seeking and export
//! always give the same picture. [`export`] writes a sequence as an FPP `.fseq` file.

mod color;
mod effects;
pub mod export;
mod geometry;
mod render;

pub use color::{Colors, Rgba};
pub use effects::{Canvas, EffectTime, MAX_METEORS, Shade, Shader, shade_pixel};
pub use geometry::{Pixel, PixelBuffer, SceneGeometry};
pub use render::Renderer;
