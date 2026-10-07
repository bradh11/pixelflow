//! Camera pixel mapping: place pixels by flashing a coded sequence on them and decoding a video.
//!
//! 1. [`CodeSpec`] says what each pixel shows in each slot of the sequence (the test-pattern
//!    engine plays it, numbering pixels in wiring order).
//! 2. The window decodes the video (the platform's own decoder) and measures each frame's
//!    brightness; [`find_sync`] finds where the sequence starts.
//! 3. The window averages the frames in each slot's [`slot_windows`] into one [`Image`] per slot,
//!    and [`decode`] finds the pixels in them and reads each one's number.
//! 4. [`plan`] lines the found pixels up with the layout, lists what looks wrong
//!    ([`Anomaly`]), and gives each prop its measured points.

mod align;
mod code;
mod decode;
mod detect;
mod image;
mod plan;
mod sync;

pub use align::Similarity;
pub use code::{Base, CHECK_DIGITS, CodeSpec, DEFAULT_SLOT_SECONDS, PREAMBLE, Slot, Symbol};
pub use decode::{Decoded, Found, Unreadable, decode};
pub use image::{Image, ImageError};
pub use plan::{Anomaly, GeneratorFit, Owner, Plan, PropInput, PropPlan, corrected_color_order, plan};
pub use sync::{Sample, SyncFound, find_sync, slot_windows};
