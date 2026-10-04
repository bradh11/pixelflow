//! PixelFlow show data model.
//!
//! Pure data types (no I/O beyond JSON text conversion) describing a show:
//! props and their shapes, regions, groups, controllers, ports, and wiring.

mod controller;
mod ids;
mod io;
mod issue;
mod limits;
mod primitives;
mod prop;
mod region;
mod shape;
mod show;
mod validate;

pub use controller::{AdapterKind, Controller, Port, PortSlot, Protocol, SacnConfig, UniverseSize};
pub use ids::{ControllerId, GroupId, PropId};
pub use io::{ModelError, show_from_json, show_to_json};
pub use issue::{Issue, IssueCode, Severity, ValidationReport};
pub use limits::{MAX_NULL_PIXELS, MAX_PROP_NODES, MAX_SHOW_PIXELS, MAX_STAR_POINTS};
pub use primitives::{ColorOrder, Transform, Vec3};
pub use prop::{Group, Prop};
pub use region::{FaceDefinition, NodeRange, Phoneme, Region, RegionKind};
pub use shape::{Corner, Generator, MatrixWiring, Orientation, Provenance, ShapeSource};
pub use show::{CURRENT_SCHEMA_VERSION, Show, ShowSettings};
pub use validate::validate_show;
