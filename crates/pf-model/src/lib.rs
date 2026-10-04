//! PixelFlow show data model.
//!
//! Pure data types (no I/O beyond JSON text conversion) describing a show:
//! props and their shapes, regions, groups, controllers, ports, and wiring.

mod ids;
mod primitives;

pub use ids::{ControllerId, GroupId, PropId};
pub use primitives::{ColorOrder, Transform, Vec3};
