//! Real-time output of show frames to pixel controllers over sACN (E1.31) and DDP, and to DMX
//! nodes over sACN and Art-Net (and USB DMX interfaces, see [`usb`]).
//!
//! [`build_plan`] turns a show and its channel map into an [`OutputPlan`]: for every
//! controller, how to gather its channels out of the show frame (color order, reverse,
//! brightness, gamma) and how to address them on the wire. [`start_output`] runs the plan
//! on a dedicated thread with a fixed frame clock, reading the latest frame from a
//! [`pf_frame::FrameReader`] and sending packets through a [`Transport`].

pub mod artnet;
mod clock;
pub mod ddp;
mod engine;
mod gather;
mod health;
mod lut;
mod plan;
pub mod sacn;
mod serial;
mod settings;
mod transport;
pub mod usb;

pub use artnet::ART_NET_PORT;
pub use clock::FrameClock;
pub use engine::{ControllerStats, OutputHandle, OutputStats, start_output};
pub use gather::render_controller;
pub use health::ControllerState;
pub use lut::build_lut;
pub use plan::{
    ControllerPlan, DDP_PORT, GatherSpan, OutputPlan, PassthroughRoute, SACN_PORT, USB_DESTINATION, Wire,
    build_offline_plan, build_passthrough_plan, build_plan, hold_motors, motor_channels, wire_order,
};
pub use serial::SystemSerialPorts;
pub use settings::OutputSettings;
pub use transport::{Recorded, RecordedSerial, RecordingTransport, Transport, UdpTransport};
pub use usb::{FakeSerialPorts, PortChoice, SerialPort, SerialPorts};
