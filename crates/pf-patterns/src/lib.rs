//! Test patterns for checking wiring and channel mapping.
//!
//! A [`Target`] picks which pixels to light (a prop, group, port, controller, or the whole
//! show). [`resolve_target`] turns it into frame-buffer ranges in wiring order, and
//! [`render`] paints a [`Pattern`] at a point in time into the show frame.

mod color;
mod pattern;
mod target;

pub use color::Rgbw;
pub use pattern::{Pattern, render};
pub use target::{Target, TargetRange, resolve_target};
