//! Real-time output of show frames to pixel controllers over sACN (E1.31) and DDP.

pub mod ddp;
mod gather;
mod lut;
mod plan;
pub mod sacn;
mod settings;

pub use gather::render_controller;
pub use lut::build_lut;
pub use plan::{ControllerPlan, DDP_PORT, GatherSpan, OutputPlan, SACN_PORT, Wire, build_plan, wire_order};
pub use settings::OutputSettings;
