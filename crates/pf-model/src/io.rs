//! Show file (JSON) reading and writing with schema migrations.

use crate::{CURRENT_SCHEMA_VERSION, Show};
use serde_json::Value;

/// Errors from reading or writing a show file.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("the show file is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the show file has no schemaVersion field")]
    MissingSchemaVersion,
    #[error("the show file's schema version {0} is not valid")]
    InvalidSchemaVersion(u64),
    #[error(
        "the show file uses schema version {found}, but this version of PixelFlow only supports up to {supported}; update PixelFlow to open it"
    )]
    UnsupportedSchemaVersion { found: u64, supported: u32 },
}

type Migration = fn(Value) -> Result<Value, ModelError>;

/// `MIGRATIONS[i]` upgrades a document from schema version `i + 1` to `i + 2`.
const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(
    MIGRATIONS.len() + 1 == CURRENT_SCHEMA_VERSION as usize,
    "every schema version bump needs a migration"
);

/// Parses a show file, upgrading older schema versions to the current one.
pub fn show_from_json(text: &str) -> Result<Show, ModelError> {
    let mut doc: Value = serde_json::from_str(text)?;
    let version = doc
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ModelError::MissingSchemaVersion)?;
    if version == 0 {
        return Err(ModelError::InvalidSchemaVersion(version));
    }
    if version > u64::from(CURRENT_SCHEMA_VERSION) {
        return Err(ModelError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    for migrate in &MIGRATIONS[(version - 1) as usize..] {
        doc = migrate(doc)?;
    }
    doc["schemaVersion"] = Value::from(CURRENT_SCHEMA_VERSION);
    Ok(serde_json::from_value(doc)?)
}

/// Serializes a show as pretty-printed JSON.
pub fn show_to_json(show: &Show) -> Result<String, ModelError> {
    Ok(serde_json::to_string_pretty(show)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource};

    fn sample_show() -> Show {
        let mut show = Show::new("Round Trip");
        let prop = Prop::new(
            "Arch",
            ShapeSource::Generator(Generator::Arch {
                nodes: 50,
                width: 4.0,
                height: 2.0,
            }),
        );
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(prop.id));
        let mut controller = Controller::new("WLED", "10.0.0.5", Protocol::Ddp);
        controller.ports.push(port);
        show.props.push(prop);
        show.controllers.push(controller);
        show
    }

    #[test]
    fn show_round_trips_through_json() {
        let show = sample_show();
        let text = show_to_json(&show).unwrap();
        assert_eq!(show_from_json(&text).unwrap(), show);
    }

    #[test]
    fn rejects_missing_zero_and_future_versions() {
        assert!(matches!(
            show_from_json(r#"{ "name": "x" }"#),
            Err(ModelError::MissingSchemaVersion)
        ));
        assert!(matches!(
            show_from_json(r#"{ "schemaVersion": 0, "name": "x" }"#),
            Err(ModelError::InvalidSchemaVersion(0))
        ));
        assert!(matches!(
            show_from_json(r#"{ "schemaVersion": 99, "name": "x" }"#),
            Err(ModelError::UnsupportedSchemaVersion { found: 99, .. })
        ));
    }

    #[test]
    fn malformed_json_is_reported() {
        assert!(matches!(show_from_json("{ nope"), Err(ModelError::Json(_))));
    }
}
