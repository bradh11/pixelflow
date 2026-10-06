//! PixelFlow show data model.
//!
//! Pure data types (no I/O beyond JSON text conversion) describing a show:
//! props and their shapes, regions, groups, controllers, ports, and wiring.

mod color;
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

pub use color::Rgb;
pub use controller::{
    AdapterKind, Controller, Port, PortSlot, Protocol, SacnConfig, SequenceChannels, UniverseSize,
};
pub use ids::{ControllerId, GroupId, PropId, RegionId, SequenceId};
pub use io::{ModelError, check_show, show_from_json, show_to_json};
pub use issue::{Issue, IssueCode, Severity, ValidationReport};
pub use limits::{
    MAX_ICICLE_DROP_LIGHTS, MAX_ICICLE_DROPS, MAX_NULL_PIXELS, MAX_POLY_VERTICES, MAX_PROP_NODES,
    MAX_REGION_ENTRIES, MAX_REGIONS_PER_PROP, MAX_SEQUENCE_OFFSET_MS, MAX_SHOW_PIXELS, MAX_SPINNER_ARMS,
    MAX_SPINNER_HOLLOW, MAX_STAR_POINTS,
};
pub use primitives::{ColorOrder, Transform, Vec3};
pub use prop::{Group, GroupMember, Prop};
pub use region::{
    BufferStyle, FaceColors, FaceDefinition, LineLayout, NodeRange, NodeRun, Phoneme, Region, RegionKind,
    RegionRef, SubmodelLine, format_line, parse_line,
};
pub use shape::{
    Corner, CubeStart, CubeStyle, Generator, MatrixWiring, Orientation, PolySegment, Provenance, ShapeSource,
    StrandStyle,
};
pub use show::{Background, CURRENT_SCHEMA_VERSION, HouseModel, SequenceEntry, Show, ShowSettings};
pub use validate::validate_show;
