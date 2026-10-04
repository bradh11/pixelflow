//! PixelFlow show data model.
//!
//! Pure data types (no I/O beyond JSON text conversion) describing a show:
//! props and their shapes, regions, groups, controllers, ports, and wiring.

mod ids;
mod primitives;
mod prop;
mod region;
mod shape;

pub use ids::{ControllerId, GroupId, PropId};
pub use primitives::{ColorOrder, Transform, Vec3};
pub use prop::{Group, Prop};
pub use region::{FaceDefinition, NodeRange, Phoneme, Region, RegionKind};
pub use shape::{Corner, Generator, MatrixWiring, Orientation, Provenance, ShapeSource};
