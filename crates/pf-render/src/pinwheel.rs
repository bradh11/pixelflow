//! The Pinwheel effect: arms of the palette colors turning around a center point.
//!
//! Cleared for a rebuild from its behaviour specification. Until that lands it draws nothing.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::PinwheelParams;

pub struct Pinwheel;

impl Pinwheel {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(_p: &PinwheelParams, _time: &EffectTime, _colors: Colors, _canvas: Canvas) -> Self {
        Self
    }
}

impl Shade for Pinwheel {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        Rgba::CLEAR
    }
}
