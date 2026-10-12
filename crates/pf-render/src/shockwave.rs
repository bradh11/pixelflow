//! The Shockwave effect: a ring growing (or shrinking) from a center point over each cycle.
//!
//! Cleared for a rebuild from its behaviour specification. Until that lands it draws nothing.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::ShockwaveParams;

pub struct Shockwave;

impl Shockwave {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(_p: &ShockwaveParams, _time: &EffectTime, _colors: Colors, _canvas: Canvas) -> Self {
        Self
    }
}

impl Shade for Shockwave {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        Rgba::CLEAR
    }
}
