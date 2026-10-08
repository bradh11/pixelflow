//! Sequence file (JSON) reading and writing with schema migrations.

use crate::{CURRENT_SCHEMA_VERSION, MAX_SEQUENCE_BYTES, Sequence, limits};
use serde_json::Value;

/// Errors from reading or writing a sequence file.
#[derive(Debug, thiserror::Error)]
pub enum SequenceError {
    #[error("it isn't a PixelFlow sequence, or it's damaged ({0})")]
    Json(serde_json::Error),
    #[error("the sequence file has no schemaVersion field")]
    MissingSchemaVersion,
    #[error("the sequence file's schema version {0} is not valid")]
    InvalidSchemaVersion(String),
    #[error("{0}")]
    LimitExceeded(String),
    #[error(
        "the sequence file uses schema version {found}, but this version of PixelFlow only supports up to {supported}; update PixelFlow to open it"
    )]
    UnsupportedSchemaVersion { found: u64, supported: u32 },
}

impl From<serde_json::Error> for SequenceError {
    fn from(err: serde_json::Error) -> Self {
        SequenceError::Json(err)
    }
}

type Migration = fn(Value) -> Result<Value, SequenceError>;

/// `MIGRATIONS[i]` upgrades a document from schema version `i + 1` to `i + 2`.
const MIGRATIONS: &[Migration] = &[v1_to_v2, v2_to_v3, v3_to_v4];

/// Version 2 only adds submodel targets, so version 1 documents are already valid.
fn v1_to_v2(doc: Value) -> Result<Value, SequenceError> {
    Ok(doc)
}

/// Version 3 only adds effect settings that default to off (sparkles, blur) and more blends, so
/// version 2 documents are already valid.
fn v2_to_v3(doc: Value) -> Result<Value, SequenceError> {
    Ok(doc)
}

/// Version 4 only adds settings that change over an effect (`curves`, none when missing), so
/// version 3 documents are already valid.
fn v3_to_v4(doc: Value) -> Result<Value, SequenceError> {
    Ok(doc)
}

const _: () = assert!(
    MIGRATIONS.len() + 1 == CURRENT_SCHEMA_VERSION as usize,
    "every schema version bump needs a migration"
);

/// Parses a sequence file, upgrading older schema versions and enforcing size limits.
pub fn sequence_from_json(text: &str) -> Result<Sequence, SequenceError> {
    if text.len() > MAX_SEQUENCE_BYTES {
        return Err(SequenceError::LimitExceeded(format!(
            "The sequence file is {} MB; PixelFlow reads sequence files up to {} MB.",
            text.len() / (1024 * 1024),
            MAX_SEQUENCE_BYTES / (1024 * 1024)
        )));
    }
    let mut doc: Value = serde_json::from_str(text)?;
    let raw = doc
        .get("schemaVersion")
        .ok_or(SequenceError::MissingSchemaVersion)?;
    let version = match raw.as_u64() {
        Some(v) if v > 0 => v,
        _ => return Err(SequenceError::InvalidSchemaVersion(raw.to_string())),
    };
    if version > u64::from(CURRENT_SCHEMA_VERSION) {
        return Err(SequenceError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    for migrate in &MIGRATIONS[(version - 1) as usize..] {
        doc = migrate(doc)?;
    }
    doc["schemaVersion"] = Value::from(CURRENT_SCHEMA_VERSION);
    let mut seq: Sequence = serde_json::from_value(doc)?;
    seq.sanitize_settings();
    seq.tidy_timing_tracks();
    if let Some(problem) = limits::limit_problems(&seq).into_iter().next() {
        return Err(SequenceError::LimitExceeded(problem));
    }
    Ok(seq)
}

/// Serializes a sequence as pretty-printed JSON.
pub fn sequence_to_json(seq: &Sequence) -> Result<String, SequenceError> {
    Ok(serde_json::to_string_pretty(seq)?)
}

/// Checks a sequence built in memory exactly as opening a sequence file would (size limits
/// included) and returns the checked copy.
pub fn check_sequence(seq: &Sequence) -> Result<Sequence, SequenceError> {
    if let Some(problem) = limits::limit_problems(seq).into_iter().next() {
        return Err(SequenceError::LimitExceeded(problem));
    }
    sequence_from_json(&sequence_to_json(seq)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use pf_model::{GroupId, PropId};

    fn sample() -> Sequence {
        let mut seq = Sequence::new("Carol of the Bells", 180_000);
        seq.audio = Some("Carol.mp3".into());
        seq.frame_ms = 50;
        seq.timing_tracks.push(TimingTrack::new(
            "Beats",
            TimingKind::Beats,
            vec![Mark::new(0, 500, ""), Mark::new(500, 1000, "2")],
        ));
        let mut row = Row::new(Target::Prop(PropId::new()));
        let mut chase = Effect::new(EffectKind::Chase, 1000, 5000)
            .with_palette([Rgb::RED, Rgb::GREEN])
            .with_params(EffectParams::Chase(ChaseParams {
                bands: 4,
                bounce: true,
                ..ChaseParams::default()
            }));
        chase.blend = Blend::Add;
        chase.fade_in_ms = 250;
        chase.sparkles = 54;
        chase.sparkle_color = Rgb::new(255, 0, 0);
        chase.blur = 7;
        chase.curves.insert("speed".into(), Curve::ramp(0.5, 4.0));
        chase.curves.insert(
            "sparkles".into(),
            Curve::custom(0.0, 100.0, vec![[0.0, 0.0], [0.5, 1.0], [0.5, 0.0]]),
        );
        chase.fade_out_ms = 500;
        row.layers[0].effects.push(chase);
        row.layers.push(Layer {
            effects: vec![Effect::new(EffectKind::Twinkle, 0, 180_000)],
        });
        seq.rows.push(row);
        let mut group_row = Row::new(Target::Group(GroupId::new()));
        for (i, kind) in EffectKind::ALL.into_iter().enumerate() {
            let start = i as u64 * 1000;
            group_row.layers[0]
                .effects
                .push(Effect::new(kind, start, start + 1000));
        }
        seq.rows.push(group_row);
        seq.rows.push(Row::new(Target::Region {
            prop: PropId::new(),
            region: pf_model::RegionId::new(),
        }));
        seq
    }

    #[test]
    fn sequences_round_trip_through_json() {
        let seq = sample();
        let text = sequence_to_json(&seq).unwrap();
        assert_eq!(sequence_from_json(&text).unwrap(), seq);
        assert_eq!(check_sequence(&seq).unwrap(), seq);
    }

    #[test]
    fn the_file_format_is_readable_json() {
        let text = sequence_to_json(&sample()).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["schemaVersion"], CURRENT_SCHEMA_VERSION);
        assert_eq!(json["frameMs"], 50);
        let chase = &json["rows"][0]["layers"][0]["effects"][0];
        assert_eq!(chase["params"]["kind"], "chase");
        assert_eq!(chase["params"]["bands"], 4);
        assert_eq!(
            chase["palette"]["colors"],
            serde_json::json!(["#ff0000", "#00ff00"])
        );
        assert_eq!(chase["blend"], "add");
        assert_eq!(chase["fadeInMs"], 250);
        assert_eq!(chase["sparkles"], 54);
        assert_eq!(chase["sparkleColor"], "#ff0000");
        assert_eq!(chase["blur"], 7);
        assert_eq!(
            chase["curves"]["speed"],
            serde_json::json!({ "shape": "ramp", "from": 0.5, "to": 4.0 })
        );
        assert_eq!(
            chase["curves"]["sparkles"]["points"][1],
            serde_json::json!([0.5, 1.0])
        );
        assert!(
            json["rows"][0]["layers"][1]["effects"][0].get("curves").is_none(),
            "no curves, no field"
        );
        assert!(json["rows"][1]["target"]["group"].is_string());
        assert!(json["rows"][2]["target"]["region"]["region"].is_string());
    }

    #[test]
    fn minimal_files_fill_in_defaults() {
        let text = r#"{ "schemaVersion": 1, "name": "x", "durationMs": 1000, "rows": [
            { "id": "11111111-0000-4000-8000-000000000001",
              "target": { "prop": "22222222-0000-4000-8000-000000000001" },
              "layers": [ { "effects": [ { "id": "33333333-0000-4000-8000-000000000001",
                  "startMs": 0, "endMs": 500, "params": { "kind": "on" } } ] } ] } ] }"#;
        let seq = sequence_from_json(text).unwrap();
        assert_eq!(seq.frame_ms, 25);
        let effect = &seq.rows[0].layers[0].effects[0];
        assert_eq!(effect.palette, Palette::default());
        assert_eq!(effect.blend, Blend::Normal);
        assert_eq!(effect.params, EffectParams::On(OnParams::default()));
        assert_eq!(
            (effect.sparkles, effect.sparkle_color, effect.blur),
            (0, Rgb::WHITE, 0),
            "files from before sparkles and blur open with them off"
        );
        assert!(
            effect.curves.is_empty(),
            "files from before curves open without them"
        );
    }

    #[test]
    fn version_3_files_open_unchanged_as_version_4() {
        let text = r#"{ "schemaVersion": 3, "name": "x", "durationMs": 1000, "rows": [
            { "id": "11111111-0000-4000-8000-000000000001",
              "target": { "prop": "22222222-0000-4000-8000-000000000001" },
              "layers": [ { "effects": [ { "id": "33333333-0000-4000-8000-000000000001",
                  "startMs": 0, "endMs": 500, "params": { "kind": "chase", "speed": 2 },
                  "sparkles": 10, "blur": 3 } ] } ] } ] }"#;
        let seq = sequence_from_json(text).unwrap();
        assert_eq!(seq.schema_version, 4);
        let effect = &seq.rows[0].layers[0].effects[0];
        assert!(effect.curves.is_empty());
        assert_eq!((effect.sparkles, effect.blur), (10, 3));
        let again = sequence_from_json(&sequence_to_json(&seq).unwrap()).unwrap();
        assert_eq!(again, seq);
    }

    #[test]
    fn curves_are_fitted_to_their_settings_on_open_and_refused_in_memory() {
        let text = r#"{ "schemaVersion": 4, "name": "x", "durationMs": 1000, "rows": [
            { "id": "11111111-0000-4000-8000-000000000001",
              "target": { "prop": "22222222-0000-4000-8000-000000000001" },
              "layers": [ { "effects": [ { "id": "33333333-0000-4000-8000-000000000001",
                  "startMs": 0, "endMs": 500, "params": { "kind": "chase" },
                  "curves": {
                    "speed": { "shape": "sine", "from": -5, "to": 500, "cycles": 3 },
                    "bands": { "shape": "ramp", "from": 1, "to": 8 },
                    "direction": { "shape": "ramp", "from": 0, "to": 1 },
                    "lasers": { "shape": "ramp", "from": 0, "to": 1 }
                  } } ] } ] } ] }"#;
        let seq = sequence_from_json(text).unwrap();
        let effect = &seq.rows[0].layers[0].effects[0];
        assert_eq!(
            effect.curves.keys().collect::<Vec<_>>(),
            ["bands", "speed"],
            "curves on settings that aren't numbers are dropped"
        );
        let speed = &effect.curves["speed"];
        assert_eq!(
            (speed.shape, speed.from, speed.to, speed.cycles),
            (CurveShape::Sine, 0.0, 50.0, 3.0)
        );

        let mut bad = seq.clone();
        bad.rows[0].layers[0].effects[0]
            .curves
            .insert("speed".into(), Curve::ramp(0.0, 70.0));
        assert_eq!(
            limit_problems(&bad),
            vec![
                "The Chase effect at 0:00.000 has a setting PixelFlow can't use: Speed's curve ends at 70; use 0 to 50."
                    .to_string()
            ]
        );
        bad.rows[0].layers[0].effects[0]
            .curves
            .insert("bounce".into(), Curve::ramp(0.0, 1.0));
        assert!(limit_problems(&bad)[0].contains("'bounce' can't change over the effect"));
    }

    #[test]
    fn every_blend_reads_and_writes_by_its_key() {
        for blend in Blend::ALL {
            let json = serde_json::to_value(blend).unwrap();
            assert_eq!(json, blend.key());
            assert_eq!(serde_json::from_value::<Blend>(json).unwrap(), blend);
        }
    }

    #[test]
    fn sparkles_and_blur_beyond_their_range_are_clamped_on_open_and_refused_in_memory() {
        let text = r#"{ "schemaVersion": 3, "name": "x", "durationMs": 1000, "rows": [
            { "id": "11111111-0000-4000-8000-000000000001",
              "target": { "prop": "22222222-0000-4000-8000-000000000001" },
              "layers": [ { "effects": [ { "id": "33333333-0000-4000-8000-000000000001",
                  "startMs": 0, "endMs": 500, "params": { "kind": "on" },
                  "sparkles": 5000, "blur": 99 } ] } ] } ] }"#;
        let seq = sequence_from_json(text).unwrap();
        let effect = &seq.rows[0].layers[0].effects[0];
        assert_eq!((effect.sparkles, effect.blur), (MAX_SPARKLES, MAX_BLUR));

        let mut bad = seq.clone();
        bad.rows[0].layers[0].effects[0].blur = 20;
        assert_eq!(
            limit_problems(&bad),
            vec![
                "The On effect at 0:00.000 has a setting PixelFlow can't use: Blur is 20; use 0 to 14."
                    .to_string()
            ]
        );
    }

    #[test]
    fn rejects_missing_invalid_and_future_versions() {
        assert!(matches!(
            sequence_from_json(r#"{ "name": "x" }"#),
            Err(SequenceError::MissingSchemaVersion)
        ));
        assert!(matches!(
            sequence_from_json(r#"{ "schemaVersion": "1", "name": "x" }"#),
            Err(SequenceError::InvalidSchemaVersion(_))
        ));
        let err = sequence_from_json(r#"{ "schemaVersion": 7, "name": "x" }"#).unwrap_err();
        assert!(err.to_string().contains("update PixelFlow"), "{err}");
        assert!(matches!(sequence_from_json("[1, 2"), Err(SequenceError::Json(_))));
    }

    #[test]
    fn limits_are_enforced_with_plain_messages() {
        let long = r#"{ "schemaVersion": 1, "name": "x", "durationMs": 99999999999 }"#;
        let err = sequence_from_json(long).unwrap_err();
        assert!(matches!(err, SequenceError::LimitExceeded(_)));
        assert!(err.to_string().contains("at most 4 hours"), "{err}");

        let fast = r#"{ "schemaVersion": 1, "name": "x", "durationMs": 1000, "frameMs": 0 }"#;
        let err = sequence_from_json(fast).unwrap_err();
        assert!(err.to_string().contains("frames are 0 ms apart"), "{err}");

        let mut seq = Sequence::new("x", 1000);
        let mut row = Row::new(Target::Prop(PropId::new()));
        row.layers[0].effects = (0..=MAX_EFFECTS as u64)
            .map(|i| Effect::new(EffectKind::Off, i, i + 1))
            .collect();
        seq.rows.push(row);
        let err = check_sequence(&seq).unwrap_err();
        assert!(err.to_string().contains("at most 200000"), "{err}");

        let mut seq = Sequence::new("x", 1000);
        seq.name = "n".repeat(MAX_TEXT_LEN + 1);
        assert!(matches!(
            check_sequence(&seq),
            Err(SequenceError::LimitExceeded(_))
        ));

        let huge = " ".repeat(MAX_SEQUENCE_BYTES + 1);
        let err = sequence_from_json(&huge).unwrap_err();
        assert!(err.to_string().contains("up to 64 MB"), "{err}");
    }

    #[test]
    fn out_of_range_settings_are_clamped_on_open_so_the_file_saves_and_reopens() {
        // 1e39 is too big for an f32: it reads as infinity, which JSON can't write back.
        let text = r#"{ "schemaVersion": 1, "name": "x", "durationMs": 1000, "rows": [
            { "id": "11111111-0000-4000-8000-000000000001",
              "target": { "prop": "22222222-0000-4000-8000-000000000001" },
              "layers": [ { "effects": [
                { "id": "33333333-0000-4000-8000-000000000001", "startMs": 0, "endMs": 500,
                  "params": { "kind": "chase", "speed": 1e39, "width": -1e39, "bands": 4000000000 } },
                { "id": "33333333-0000-4000-8000-000000000002", "startMs": 500, "endMs": 900,
                  "params": { "kind": "ripple", "spacing": 0 } } ] } ] } ] }"#;
        let seq = sequence_from_json(text).unwrap();
        let EffectParams::Chase(chase) = seq.rows[0].layers[0].effects[0].params else {
            panic!("a chase")
        };
        assert_eq!((chase.speed, chase.width, chase.bands), (50.0, 0.0, 1000));
        let EffectParams::Ripple(ripple) = seq.rows[0].layers[0].effects[1].params else {
            panic!("a ripple")
        };
        assert_eq!(ripple.spacing, 0.01);
        let saved = sequence_to_json(&seq).unwrap();
        let json: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            json["rows"][0]["layers"][0]["effects"][0]["params"]["speed"],
            50.0
        );
        assert_eq!(sequence_from_json(&saved).unwrap(), seq);
    }

    #[test]
    fn sequences_built_in_memory_with_bad_settings_are_refused_plainly() {
        let mut seq = Sequence::new("x", 1000);
        let mut row = Row::new(Target::Prop(PropId::new()));
        row.layers[0]
            .effects
            .push(
                Effect::new(EffectKind::Twinkle, 1000, 2000).with_params(EffectParams::Twinkle(
                    TwinkleParams {
                        rate: f32::INFINITY,
                        ..TwinkleParams::default()
                    },
                )),
            );
        seq.rows.push(row);
        let problems = limit_problems(&seq);
        assert_eq!(
            problems,
            vec![
                "The Twinkle effect at 0:01.000 has a setting PixelFlow can't use: Rate isn't a usable number; use 0 to 50."
                    .to_string()
            ]
        );
        assert!(matches!(
            check_sequence(&seq),
            Err(SequenceError::LimitExceeded(_))
        ));
        let mut fixed = seq.clone();
        fixed.sanitize_settings();
        assert!(limit_problems(&fixed).is_empty());
        assert_eq!(check_sequence(&fixed).unwrap(), fixed);
    }

    #[test]
    fn deeply_nested_garbage_is_an_error_not_a_crash() {
        let text = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(matches!(sequence_from_json(&text), Err(SequenceError::Json(_))));
    }
}
