//! Automatic channel mapping: props and wiring to frame layout, controller channels,
//! and sACN universes.

mod layout;
mod universes;

pub use layout::{
    Addressing, ChannelAddress, ChannelMap, ControllerOutput, OutputSpan, PixelLocation, PropLayout,
    UniverseSpan,
};
