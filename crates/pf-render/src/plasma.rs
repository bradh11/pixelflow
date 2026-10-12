//! The Plasma effect: soft blobs of color drifting and merging.
//!
//! Cleared for a rebuild from its behaviour specification. Until that lands it draws nothing.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::PlasmaParams;

pub struct Plasma;

impl Plasma {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(_p: &PlasmaParams, _time: &EffectTime, _colors: Colors, _canvas: Canvas) -> Self {
        Self
    }
}

impl Shade for Plasma {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        Rgba::CLEAR
    }
}
