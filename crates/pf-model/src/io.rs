//! Show file (JSON) reading and writing with schema migrations.

use crate::{CURRENT_SCHEMA_VERSION, Show, limits};
use serde_json::Value;

/// Errors from reading or writing a show file.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("the show file is not valid: {0}")]
    Json(serde_json::Error),
    #[error("the show file has no schemaVersion field")]
    MissingSchemaVersion,
    #[error("the show file's schema version {0} is not valid")]
    InvalidSchemaVersion(String),
    #[error("{0}")]
    LimitExceeded(String),
    #[error(
        "the show file uses schema version {found}, but this version of PixelFlow only supports up to {supported}; update PixelFlow to open it"
    )]
    UnsupportedSchemaVersion { found: u64, supported: u32 },
}

impl From<serde_json::Error> for ModelError {
    fn from(err: serde_json::Error) -> Self {
        ModelError::Json(err)
    }
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
    let raw = doc.get("schemaVersion").ok_or(ModelError::MissingSchemaVersion)?;
    let version = match raw.as_u64() {
        Some(v) if v > 0 => v,
        _ => return Err(ModelError::InvalidSchemaVersion(raw.to_string())),
    };
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
    let show: Show = serde_json::from_value(doc)?;
    if let Some(problem) = limits::check_limits(&show).into_iter().next() {
        return Err(ModelError::LimitExceeded(problem));
    }
    Ok(show)
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
            show_from_json(r#"{ "schemaVersion": 99, "name": "x" }"#),
            Err(ModelError::UnsupportedSchemaVersion { found: 99, .. })
        ));
    }

    #[test]
    fn present_but_invalid_schema_versions_are_not_reported_as_missing() {
        for (raw, shown) in [("0", "0"), ("\"1\"", "\"1\""), ("1.5", "1.5"), ("1.0", "1.0")] {
            let text = format!(r#"{{ "schemaVersion": {raw}, "name": "x" }}"#);
            match show_from_json(&text) {
                Err(ModelError::InvalidSchemaVersion(found)) => assert_eq!(found, shown, "{raw}"),
                other => panic!("{raw}: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn malformed_json_is_reported() {
        assert!(matches!(show_from_json("{ nope"), Err(ModelError::Json(_))));
    }

    #[test]
    fn json_error_detail_is_shown_once_and_is_not_a_source() {
        let err = show_from_json(r#"{ "schemaVersion": 1, "name": "x", "props": 5 }"#).unwrap_err();
        assert!(std::error::Error::source(&err).is_none());
        let text = err.to_string();
        assert_eq!(text.matches("invalid type").count(), 1, "{text}");
    }

    fn line_prop(name: &str, nodes: u32) -> String {
        format!(
            r#"{{ "id": "{}", "name": "{name}", "shape": {{ "source": "generator", "type": "line", "nodes": {nodes}, "length": 1.0 }} }}"#,
            crate::PropId::new().0
        )
    }

    fn show_json(props: &[String], controllers: &str) -> String {
        format!(
            r#"{{ "schemaVersion": 1, "name": "x", "props": [{}], "controllers": [{controllers}] }}"#,
            props.join(",")
        )
    }

    fn controller_json(null_pixels: u32, prop_id: &str) -> String {
        format!(
            r#"{{ "id": "{}", "name": "C", "address": "1.2.3.4", "protocol": {{ "type": "ddp" }},
              "ports": [{{ "number": 1, "slots": [{{ "prop": "{prop_id}", "nullPixels": {null_pixels} }}] }}] }}"#,
            crate::ControllerId::new().0
        )
    }

    #[test]
    fn oversized_props_are_rejected_at_load() {
        let err = show_from_json(&show_json(&[line_prop("Mega", 4_294_967_295)], "")).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)));
        assert!(err.to_string().contains("'Mega' has 4294967295 pixels"), "{err}");
        assert!(err.to_string().contains("at most 1000000 per prop"));
        assert!(show_from_json(&show_json(&[line_prop("Max", crate::MAX_PROP_NODES)], "")).is_ok());
        assert!(matches!(
            show_from_json(&show_json(&[line_prop("Over", crate::MAX_PROP_NODES + 1)], "")),
            Err(ModelError::LimitExceeded(_))
        ));
    }

    #[test]
    fn total_show_pixels_are_limited() {
        let ten: Vec<String> = (0..10).map(|i| line_prop(&format!("P{i}"), 1_000_000)).collect();
        assert!(show_from_json(&show_json(&ten, "")).is_ok());
        let mut eleven = ten;
        eleven.push(line_prop("One more", 1));
        let err = show_from_json(&show_json(&eleven, "")).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)));
        assert!(err.to_string().contains("in total"), "{err}");
    }

    #[test]
    fn null_pixels_are_limited() {
        let prop = line_prop("A", 5);
        let id = prop.split('"').nth(3).unwrap().to_string();
        let ok = show_json(
            std::slice::from_ref(&prop),
            &controller_json(crate::MAX_NULL_PIXELS, &id),
        );
        assert!(show_from_json(&ok).is_ok());
        let bad = show_json(&[prop], &controller_json(4_294_967_295, &id));
        let err = show_from_json(&bad).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)));
        assert!(err.to_string().contains("at most 1000"), "{err}");
    }

    #[test]
    fn custom_grid_cell_count_must_match_dimensions() {
        let grid = |cols: u32, rows: u32, cells: &str| {
            format!(
                r#"{{ "id": "{}", "name": "Grid", "shape": {{ "source": "generator", "type": "customGrid", "columns": {cols}, "rows": {rows}, "cells": {cells} }} }}"#,
                crate::PropId::new().0
            )
        };
        assert!(show_from_json(&show_json(&[grid(2, 1, "[1,2]")], "")).is_ok());
        for bad in [grid(2, 1, "[4294967295]"), grid(4294967295, 4294967295, "[1]")] {
            assert!(matches!(
                show_from_json(&show_json(&[bad], "")),
                Err(ModelError::LimitExceeded(_))
            ));
        }
    }

    fn generator_prop(name: &str, body: &str) -> String {
        format!(
            r#"{{ "id": "{}", "name": "{name}", "shape": {{ "source": "generator", {body} }} }}"#,
            crate::PropId::new().0
        )
    }

    #[test]
    fn star_points_and_tree_strings_are_limited() {
        let star = |points: u64| {
            generator_prop(
                "Porch Star",
                &format!(
                    r#""type": "star", "points": {points}, "nodes": 1, "outerRadius": 1.0, "innerRadius": 0.5"#
                ),
            )
        };
        assert!(show_from_json(&show_json(&[star(u64::from(crate::MAX_STAR_POINTS))], "")).is_ok());
        let err = show_from_json(&show_json(&[star(50_000_000)], "")).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)));
        assert!(
            err.to_string().contains("'Porch Star' has 50000000 points"),
            "{err}"
        );
        assert!(err.to_string().contains("at most 100"), "{err}");

        let tree = generator_prop(
            "Big Tree",
            r#""type": "tree", "strings": 4000000000, "nodesPerString": 0, "height": 1.0, "baseRadius": 1.0, "topRadius": 0.1"#,
        );
        let err = show_from_json(&show_json(&[tree], "")).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)));
        assert!(
            err.to_string().contains("'Big Tree' has 4000000000 strings"),
            "{err}"
        );
    }

    #[test]
    fn oversized_matrix_message_reports_the_real_count() {
        let matrix = generator_prop(
            "Wall",
            r#""type": "matrix", "columns": 100000, "rows": 100000, "width": 1.0, "height": 1.0"#,
        );
        let err = show_from_json(&show_json(&[matrix], "")).unwrap_err();
        assert!(err.to_string().contains("10000000000"), "{err}");
    }
}
