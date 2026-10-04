//! Real-time output of show frames to pixel controllers over sACN (E1.31) and DDP.

mod clock;
pub mod ddp;
mod gather;
mod health;
mod lut;
mod plan;
pub mod sacn;
mod settings;
mod transport;

pub use clock::FrameClock;
pub use gather::render_controller;
pub use health::ControllerState;
pub use lut::build_lut;
pub use plan::{ControllerPlan, DDP_PORT, GatherSpan, OutputPlan, SACN_PORT, Wire, build_plan, wire_order};
pub use settings::OutputSettings;
pub use transport::{Recorded, RecordingTransport, Transport, UdpTransport};
