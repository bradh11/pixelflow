//! PixelFlow show data model.
//!
//! Pure data types (no I/O beyond JSON text conversion) describing a show:
//! props and their shapes, regions, groups, controllers, ports, and wiring.

mod controller;
mod ids;
mod io;
mod primitives;
mod prop;
mod region;
mod shape;
mod show;

pub use controller::{AdapterKind, Controller, Port, PortSlot, Protocol, SacnConfig, UniverseSize};
pub use ids::{ControllerId, GroupId, PropId};
pub use io::{ModelError, show_from_json, show_to_json};
pub use primitives::{ColorOrder, Transform, Vec3};
pub use prop::{Group, Prop};
pub use region::{FaceDefinition, NodeRange, Phoneme, Region, RegionKind};
pub use shape::{Corner, Generator, MatrixWiring, Orientation, Provenance, ShapeSource};
pub use show::{CURRENT_SCHEMA_VERSION, Show, ShowSettings};
