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
const MIGRATIONS: &[Migration] = &[
    v1_to_v2, v2_to_v3, v3_to_v4, v4_to_v5, v5_to_v6, v6_to_v7, v7_to_v8, v8_to_v9, v9_to_v10, v10_to_v11,
    v11_to_v12, v12_to_v13, v13_to_v14, v14_to_v15, v15_to_v16,
];

/// Version 2 only adds the `falcon` adapter value, so version 1 documents are already valid.
fn v1_to_v2(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 3 only adds the optional `sequenceChannels`, so version 2 documents are already valid.
fn v2_to_v3(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 4 only adds the optional `sequences` list, so version 3 documents are already valid.
fn v3_to_v4(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 5 only adds the optional `background` photo, so version 4 documents are already valid.
fn v4_to_v5(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 6 only adds the optional `houseModel`, so version 5 documents are already valid.
fn v5_to_v6(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 7 reworks regions for submodels: each gets an `id`, and a `nodes` region's `ranges`
/// become one line of runs (`lines`), drawn as a row with the default buffer style.
fn v6_to_v7(mut doc: Value) -> Result<Value, ModelError> {
    let Some(props) = doc.get_mut("props").and_then(Value::as_array_mut) else {
        return Ok(doc);
    };
    for prop in props {
        let Some(regions) = prop.get_mut("regions").and_then(Value::as_array_mut) else {
            continue;
        };
        for region in regions.iter_mut().filter_map(Value::as_object_mut) {
            region
                .entry("id")
                .or_insert_with(|| Value::from(crate::RegionId::new().to_string()));
            if region.get("kind").and_then(Value::as_str) != Some("nodes") {
                continue;
            }
            let Some(ranges) = region.remove("ranges") else {
                continue;
            };
            let mut line = Vec::new();
            for range in ranges.as_array().into_iter().flatten() {
                let bound = |key: &str| range.get(key).and_then(Value::as_u64);
                if let (Some(start), Some(end)) = (bound("start"), bound("end"))
                    && end > start
                    && end <= u64::from(u32::MAX) + 1
                {
                    line.push(serde_json::json!({ "first": start, "last": end - 1 }));
                }
            }
            region.insert("lines".into(), Value::from(vec![Value::from(line)]));
        }
    }
    Ok(doc)
}

/// Version 8 lets file paths be relative to the show file (and keep bytes that aren't UTF-8).
/// Version 7 files hold full paths, which version 8 reads the same way, so they're already valid.
fn v7_to_v8(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 9 only adds new prop shapes, so version 8 documents are already valid.
fn v8_to_v9(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 10 only adds settings to the arch, circle, star and tree shapes (each left out reads
/// as before), so version 9 documents are already valid.
fn v9_to_v10(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 11 lets an sACN universe carry any number of channels from 1 to 512. Version 10 files
/// hold 510 or 512, which version 11 reads the same way, so they're already valid.
fn v10_to_v11(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 12 gives each group a `layout` and `gridSize` for effects, and the show an optional
/// `layoutArea`. Groups before it get xLights' defaults: the minimal grid, 400 cells.
fn v11_to_v12(mut doc: Value) -> Result<Value, ModelError> {
    if let Some(groups) = doc.get_mut("groups").and_then(Value::as_array_mut) {
        for group in groups.iter_mut().filter_map(Value::as_object_mut) {
            group
                .entry("layout")
                .or_insert_with(|| Value::from("minimalGrid"));
            group
                .entry("gridSize")
                .or_insert_with(|| Value::from(crate::DEFAULT_GRID_SIZE));
        }
    }
    Ok(doc)
}

/// Version 13 only adds a prop's `ownLayout` (laid out as its shape is when missing, as every
/// prop was), so version 12 documents are already valid.
fn v12_to_v13(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 14 only adds DMX: a prop's `dmx`, a controller's `kind` (pixels when missing, as every
/// controller was) and the `artNet` and `dmxUsb` protocols, so version 13 documents are already
/// valid.
fn v13_to_v14(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 15 only adds moving heads and servos: new DMX channel roles, a channel's `motor` and
/// `slots`, and a fixture's `beam`, each optional. A version 14 pan or tilt channel held its
/// value and nothing more; it reads the same way, with the usual motor for its role (which only
/// matters to what moves it by degrees, and to how it is drawn). So version 14 documents are
/// already valid.
fn v14_to_v15(doc: Value) -> Result<Value, ModelError> {
    Ok(doc)
}

/// Version 16 adds what a fixture's light looks like: a strobe channel's `shutter` (what its
/// values do: closed, open or a strobe), the `zoom` role and a zoom channel's `zoom`, each
/// optional. A strobe channel without `shutter` now does the usual: open from 0 to 9, a strobe
/// above. Before 16 the previews drew every fixture lit whatever its strobe held, so a strobe
/// channel holding 10 or more (an xLights shutter held at its open value, say) is given a
/// `shutter` that is open at every value: it looks as it did until its own ranges are set.
fn v15_to_v16(mut doc: Value) -> Result<Value, ModelError> {
    let channels = doc
        .get_mut("props")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|prop| prop.pointer_mut("/dmx/channels").and_then(Value::as_array_mut))
        .flatten();
    for channel in channels.filter_map(Value::as_object_mut) {
        let strobe = channel.get("role").and_then(Value::as_str) == Some("strobe");
        let held = channel.get("value").and_then(Value::as_u64).unwrap_or(0);
        if strobe && held >= 10 && !channel.contains_key("shutter") {
            channel.insert(
                "shutter".into(),
                serde_json::json!([{ "from": 0, "does": "open" }]),
            );
        }
    }
    Ok(doc)
}

/// Development builds of schema 7 (never released) kept a group's submodels in a separate
/// `submodels` list, drawn after the whole props. Group members are now one ordered list, so
/// those submodels join the end of `members`, keeping the order they were drawn in.
fn join_group_submodels(doc: &mut Value) {
    let Some(groups) = doc.get_mut("groups").and_then(Value::as_array_mut) else {
        return;
    };
    for group in groups.iter_mut().filter_map(Value::as_object_mut) {
        let Some(Value::Array(submodels)) = group.remove("submodels") else {
            continue;
        };
        let members = group.entry("members").or_insert_with(|| Value::Array(Vec::new()));
        if let Some(members) = members.as_array_mut() {
            members.extend(submodels);
        }
    }
}

const _: () = assert!(
    MIGRATIONS.len() + 1 == CURRENT_SCHEMA_VERSION as usize,
    "every schema version bump needs a migration"
);

/// Parses a show file, upgrading older schema versions to the current one.
pub fn show_from_json(text: &str) -> Result<Show, ModelError> {
    show_from_value(serde_json::from_str(text)?)
}

/// Parses a show file and the folder it says it was saved in (`savedIn`, path text), when it
/// says. Relative file paths in the show start in that folder if they aren't found next to the
/// file now: the show file may have moved on its own.
pub fn show_file_from_json(text: &str) -> Result<(Show, Option<String>), ModelError> {
    let mut doc: Value = serde_json::from_str(text)?;
    let saved_in = doc
        .as_object_mut()
        .and_then(|o| o.remove("savedIn"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.is_empty());
    Ok((show_from_value(doc)?, saved_in))
}

/// A show file: the show, then the folder it is being saved in (`savedIn`, path text), if given.
pub fn show_file_to_json(show: &Show, saved_in: Option<&str>) -> Result<String, ModelError> {
    #[derive(serde::Serialize)]
    struct ShowFile<'a> {
        #[serde(flatten)]
        show: &'a Show,
        #[serde(rename = "savedIn", skip_serializing_if = "Option::is_none")]
        saved_in: Option<&'a str>,
    }
    Ok(serde_json::to_string_pretty(&ShowFile { show, saved_in })?)
}

fn show_from_value(mut doc: Value) -> Result<Show, ModelError> {
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
    join_group_submodels(&mut doc);
    doc["schemaVersion"] = Value::from(CURRENT_SCHEMA_VERSION);
    let show: Show = serde_json::from_value(doc)?;
    if let Some(problem) = limits::check_limits(&show).into_iter().next() {
        return Err(ModelError::LimitExceeded(problem.message));
    }
    Ok(show)
}

/// Serializes a show as pretty-printed JSON.
pub fn show_to_json(show: &Show) -> Result<String, ModelError> {
    Ok(serde_json::to_string_pretty(show)?)
}

/// Checks a show built in memory (an import, for example) exactly as opening a show file would,
/// size limits included, and returns the checked copy.
pub fn check_show(show: &Show) -> Result<Show, ModelError> {
    show_from_json(&show_to_json(show)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Controller, Generator, Port, PortSlot, Prop, Protocol, SequenceChannels, ShapeSource};

    fn sample_show() -> Show {
        let mut show = Show::new("Round Trip");
        let prop = Prop::new("Arch", ShapeSource::Generator(Generator::arch(50, 4.0, 2.0)));
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
    fn version_7_to_15_files_open_unchanged_and_save_as_version_16() {
        // A show with no new prop shapes, saved now, then labelled as written by older versions:
        // 7 (full paths), 8 (relative paths and `savedIn`) and 9 (new prop shapes, none used here).
        let mut show = sample_show();
        show.sequences
            .push(crate::SequenceEntry::new("Medley", "Medley.fseq"));
        let text = show_file_to_json(&show, Some("/Shows/Home")).unwrap();
        assert!(text.contains("\"schemaVersion\": 16"), "written as version 16");
        assert!(
            !text.contains("\"dmx\""),
            "nothing DMX is written for a show without it"
        );
        assert!(!text.contains("\"kind\""), "nor a controller's kind");
        for version in [7, 8, 9, 10, 11, 12, 13, 14, 15, 16] {
            let old = text.replace("\"schemaVersion\": 16", &format!("\"schemaVersion\": {version}"));
            let (read, saved_in) = show_file_from_json(&old).unwrap();
            assert_eq!(read, show, "version {version} reads as it was");
            assert_eq!(read.schema_version, 16);
            assert_eq!(saved_in.as_deref(), Some("/Shows/Home"), "version {version}");
            let saved: Value = serde_json::from_str(&show_to_json(&read).unwrap()).unwrap();
            assert_eq!(saved["schemaVersion"], 16);
        }
    }

    #[test]
    fn version_14_fixtures_with_pan_and_tilt_read_as_they_were_and_save_as_16() {
        // As version 14 wrote a flood and a head set up by hand: pan and tilt only held a value.
        let text = r#"{
          "schemaVersion": 14,
          "name": "Heads",
          "props": [{
            "id": "6a1d1f5e-2b0c-4d5e-9f8a-1b2c3d4e5f60",
            "name": "Spot",
            "shape": { "source": "generator", "type": "line", "nodes": 1, "length": 0.5 },
            "dmx": {
              "address": 9,
              "channels": [
                { "name": "Pan", "role": "pan", "value": 100 },
                { "name": "Tilt", "role": "tilt" },
                { "name": "Dimmer", "role": "dimmer" },
                { "name": "Wheel", "role": "colorWheel", "value": 30 }
              ]
            }
          }]
        }"#;
        let show = show_from_json(text).unwrap();
        assert_eq!(show.schema_version, 16);
        let dmx = show.props[0].dmx.as_ref().unwrap();
        assert_eq!(dmx.channels[0].value, 100, "held where it was");
        assert_eq!(dmx.channels[0].motor, None, "nothing new is written in");
        assert_eq!(
            dmx.channels[0].motor(),
            Some(crate::DmxMotor::centred(540.0)),
            "the usual pan"
        );
        assert!(dmx.channels[3].slots.is_empty());
        assert_eq!(dmx.beam, None);
        assert!(dmx.is_head());
        assert_eq!(dmx.beam(), Some(crate::DmxBeam::default()));
        let saved: Value = serde_json::from_str(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(saved["schemaVersion"], 16);
        assert_eq!(
            saved["props"][0]["dmx"]["channels"][0],
            serde_json::json!({ "name": "Pan", "role": "pan", "value": 100 })
        );
    }

    #[test]
    fn version_15_strobes_holding_a_strobe_value_stay_open_and_the_rest_read_as_they_were() {
        // As version 15 wrote an imported flood (its shutter held open at 255), a library one
        // (its strobe at 0) and one set by hand just below a strobe.
        let text = r#"{
          "schemaVersion": 15,
          "name": "Floods",
          "props": [
            { "id": "6a1d1f5e-2b0c-4d5e-9f8a-1b2c3d4e5f61", "name": "Imported",
              "shape": { "source": "generator", "type": "line", "nodes": 1, "length": 0.5 },
              "dmx": { "address": 1, "channels": [
                { "name": "Red", "role": "red" }, { "name": "Strobe", "role": "strobe", "value": 255 } ] } },
            { "id": "6a1d1f5e-2b0c-4d5e-9f8a-1b2c3d4e5f62", "name": "Library",
              "shape": { "source": "generator", "type": "line", "nodes": 1, "length": 0.5 },
              "dmx": { "address": 3, "layout": "dimmer-rgb-strobe", "channels": [
                { "name": "Dimmer", "role": "dimmer" }, { "name": "Red", "role": "red" },
                { "name": "Green", "role": "green" }, { "name": "Blue", "role": "blue" },
                { "name": "Strobe", "role": "strobe" } ] } },
            { "id": "6a1d1f5e-2b0c-4d5e-9f8a-1b2c3d4e5f63", "name": "Slow",
              "shape": { "source": "generator", "type": "line", "nodes": 1, "length": 0.5 },
              "dmx": { "address": 8, "channels": [
                { "name": "Red", "role": "red" }, { "name": "Strobe", "role": "strobe", "value": 9 } ] } }
          ]
        }"#;
        let show = show_from_json(text).unwrap();
        assert_eq!(show.schema_version, 16);
        let strobe = |p: usize, c: usize| &show.props[p].dmx.as_ref().unwrap().channels[c];
        assert_eq!(
            strobe(0, 1).shutter,
            vec![crate::DmxShutterRange {
                from: 0,
                does: crate::DmxShutter::Open
            }],
            "open at every value, as it was drawn"
        );
        assert!(strobe(1, 4).shutter.is_empty(), "the usual: open at 0");
        assert!(strobe(2, 1).shutter.is_empty(), "the usual: open at 9");
        assert_eq!(strobe(1, 4).shutter(), Some(crate::USUAL_SHUTTER));
        // Saved as 16, it reads the same again.
        let again = show_from_json(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(again, show);
    }

    #[test]
    fn moving_heads_and_servos_round_trip() {
        let mut show = Show::new("Heads");
        let mut head = Prop::new(
            "Spot",
            ShapeSource::Generator(Generator::Line {
                nodes: 1,
                length: 0.5,
            }),
        );
        let mut fixture = crate::DmxFixture::from_layout(crate::dmx_layout("head-spot-16").unwrap(), 1, 20);
        fixture.channels[2].motor = Some(crate::DmxMotor {
            range_deg: 270.0,
            home_deg: 100.0,
            min_deg: Some(-90.0),
            max_deg: Some(120.0),
            reverse: true,
        });
        fixture.channels[7].slots = vec![
            crate::DmxSlot {
                value: 0,
                color: Some(crate::Rgb::WHITE),
                name: Some("Open".into()),
            },
            crate::DmxSlot {
                value: 18,
                color: Some(crate::Rgb::RED),
                name: None,
            },
        ];
        fixture.beam = Some(crate::DmxBeam {
            angle_deg: 22.5,
            length: 9.0,
        });
        fixture.channels[6].shutter = vec![
            crate::DmxShutterRange {
                from: 0,
                does: crate::DmxShutter::Closed,
            },
            crate::DmxShutterRange {
                from: 8,
                does: crate::DmxShutter::Open,
            },
            crate::DmxShutterRange {
                from: 16,
                does: crate::DmxShutter::Strobe,
            },
        ];
        let mut zoom = crate::DmxChannel::new(crate::DmxRole::Zoom).holding(60);
        zoom.zoom = Some(crate::DmxZoom {
            from_deg: 40.0,
            to_deg: 8.0,
        });
        fixture.channels.push(zoom);
        head.dmx = Some(fixture);
        let mut servos = Prop::new(
            "Jaw",
            ShapeSource::Generator(Generator::Line {
                nodes: 2,
                length: 0.5,
            }),
        );
        servos.dmx = Some(crate::DmxFixture::from_layout(
            crate::dmx_layout("servo-16").unwrap(),
            2,
            40,
        ));
        show.props.extend([head, servos]);
        let text = show_to_json(&show).unwrap();
        assert!(text.contains("\"rangeDeg\": 270.0"), "{text}");
        assert!(text.contains("\"reverse\": true"));
        assert!(text.contains("\"color\": \"#ff0000\""));
        assert!(text.contains("\"angleDeg\": 22.5"));
        assert!(text.contains("\"role\": \"servoFine\""));
        assert!(text.contains("\"does\": \"strobe\""));
        assert!(text.contains("\"toDeg\": 8.0"));
        assert_eq!(show_from_json(&text).unwrap(), show);
    }

    #[test]
    fn dmx_fixtures_and_controllers_round_trip() {
        let mut show = Show::new("Floods");
        let mut flood = Prop::new(
            "Flood",
            ShapeSource::Generator(Generator::Line {
                nodes: 1,
                length: 0.5,
            }),
        );
        flood.dmx = Some(crate::DmxFixture::from_layout(
            crate::dmx_layout("dimmer-rgbw-strobe-mode").unwrap(),
            1,
            8,
        ));
        let mut node = Controller::dmx(
            "Node",
            "192.0.2.40",
            Protocol::ArtNet(crate::ArtNetConfig::default()),
            1,
        );
        node.ports[0].slots.push(PortSlot::new(flood.id));
        show.props.push(flood);
        show.controllers.push(node);
        show.controllers
            .push(Controller::dmx("USB", "/dev/cu.usbserial-1", Protocol::DmxUsb, 1));
        let text = show_to_json(&show).unwrap();
        let read = show_from_json(&text).unwrap();
        assert_eq!(read, show);
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["props"][0]["dmx"]["address"], 8);
        assert_eq!(json["props"][0]["dmx"]["channels"][0]["role"], "dimmer");
        assert_eq!(json["controllers"][0]["kind"], "dmx");
        assert_eq!(json["controllers"][0]["protocol"]["type"], "artNet");
        assert_eq!(crate::validate_show(&read).issues, vec![]);
    }

    #[test]
    fn version_11_groups_get_the_minimal_grid_and_keep_their_layout_once_set() {
        let prop = crate::PropId::new();
        let old = format!(
            r#"{{ "schemaVersion": 11, "name": "x",
                 "props": [{{ "id": "{prop}", "name": "Arch",
                              "shape": {{ "source": "generator", "type": "arch", "nodes": 10, "width": 2, "height": 1 }} }}],
                 "groups": [{{ "id": "{}", "name": "All", "members": ["{prop}"] }}] }}"#,
            crate::GroupId::new()
        );
        let mut show = show_from_json(&old).unwrap();
        let group = &show.groups[0];
        assert_eq!(group.layout, crate::GroupLayout::MinimalGrid, "xLights' default");
        assert_eq!(group.grid_size, 400);
        assert_eq!(show.layout_area, None);
        let saved: Value = serde_json::from_str(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(saved["groups"][0]["layout"], "minimalGrid");
        assert_eq!(saved["groups"][0]["gridSize"], 400);

        show.groups[0].layout = crate::GroupLayout::HorizontalPerModel;
        show.groups[0].grid_size = 250;
        show.layout_area = Some(crate::LayoutArea {
            width: 19.0,
            height: 16.0,
        });
        let read = show_from_json(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(read, show);
        let saved: Value = serde_json::from_str(&show_to_json(&read).unwrap()).unwrap();
        assert_eq!(saved["groups"][0]["layout"], "horizontalPerModel");
        assert_eq!(saved["layoutArea"]["width"], 19.0);
    }

    #[test]
    fn version_10_universe_sizes_read_as_they_were_and_any_size_from_1_to_512_saves() {
        let sacn = |size: u16| {
            let mut show = sample_show();
            show.controllers[0].protocol = Protocol::Sacn(crate::SacnConfig {
                start_universe: Some(7),
                universe_size: crate::UniverseSize::new(u32::from(size)).unwrap(),
                ..crate::SacnConfig::default()
            });
            show
        };
        // Version 10 only knew 510 and 512.
        for size in [510, 512] {
            let show = sacn(size);
            let text = show_to_json(&show)
                .unwrap()
                .replace("\"schemaVersion\": 11", "\"schemaVersion\": 10");
            let read = show_from_json(&text).unwrap();
            assert_eq!(read, show, "{size}");
            let saved: Value = serde_json::from_str(&show_to_json(&read).unwrap()).unwrap();
            assert_eq!(saved["controllers"][0]["protocol"]["universeSize"], size);
        }
        for size in [1, 15, 270, 426] {
            let show = sacn(size);
            assert_eq!(
                show_from_json(&show_to_json(&show).unwrap()).unwrap(),
                show,
                "{size}"
            );
        }
        let bad = show_to_json(&sacn(510))
            .unwrap()
            .replace("\"universeSize\": 510", "\"universeSize\": 513");
        let err = show_from_json(&bad).unwrap_err().to_string();
        assert!(
            err.contains("A universe carries 1 to 512 channels, so 513"),
            "{err}"
        );
    }

    #[test]
    fn version_9_files_keep_the_new_prop_shapes() {
        let mut show = Show::new("Shapes");
        for (name, shape) in [
            (
                "Roof",
                Generator::PolyLine {
                    vertices: vec![crate::Vec3::ZERO, crate::Vec3::new(2.0, 1.0, 0.0)],
                    segments: vec![crate::PolySegment::straight(20)],
                    spread_nodes: None,
                },
            ),
            (
                "Canes",
                Generator::CandyCanes {
                    canes: 3,
                    nodes_per_cane: 18,
                    width: 3.0,
                    height: 1.0,
                    cane_height: 1.0,
                    reverse: false,
                    sticks: false,
                    alternate_nodes: false,
                    skew_deg: 0.0,
                    start_right: false,
                },
            ),
            (
                "Cube",
                Generator::Cube {
                    width: 3,
                    height: 3,
                    depth: 3,
                    spacing: 0.2,
                    start: crate::CubeStart::BackTopLeft,
                    style: crate::CubeStyle::StackedFrontBack,
                    strand_style: crate::StrandStyle::NoZigZag,
                    strand_per_layer: true,
                },
            ),
        ] {
            show.props.push(Prop::new(name, ShapeSource::Generator(shape)));
        }
        let text = show_to_json(&show).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["schemaVersion"], CURRENT_SCHEMA_VERSION);
        assert_eq!(saved["props"][0]["shape"]["type"], "polyLine");
        let back = show_from_json(&text).unwrap();
        assert_eq!(back.props, show.props);
    }

    #[test]
    fn version_1_files_upgrade_to_the_current_version() {
        let v1 = r#"{ "schemaVersion": 1, "name": "Old", "controllers": [
            { "id": "33333333-0000-4000-8000-000000000001", "name": "C", "address": "10.0.0.1",
              "adapter": "fpp", "protocol": { "type": "ddp" } } ] }"#;
        let show = show_from_json(v1).unwrap();
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.name, "Old");
        let saved: Value = serde_json::from_str(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(saved["schemaVersion"], CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn controllers_keep_their_sequence_channels() {
        let v2 = r#"{ "schemaVersion": 2, "name": "Old", "controllers": [
            { "id": "33333333-0000-4000-8000-000000000001", "name": "C", "address": "10.0.0.1",
              "protocol": { "type": "ddp" } } ] }"#;
        let mut show = show_from_json(v2).unwrap();
        assert_eq!(show.controllers[0].sequence_channels, None);
        show.controllers[0].sequence_channels = Some(SequenceChannels {
            start: 1,
            count: 6147,
            raw_ddp_offsets: false,
        });
        let saved: Value = serde_json::from_str(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(
            saved["controllers"][0]["sequenceChannels"],
            serde_json::json!({ "start": 1, "count": 6147 })
        );
        let again = show_from_json(&show_to_json(&show).unwrap()).unwrap();
        assert_eq!(
            again.controllers[0].sequence_channels,
            Some(SequenceChannels {
                start: 1,
                count: 6147,
                raw_ddp_offsets: false,
            })
        );
    }

    #[test]
    fn version_4_files_open_without_a_background_and_keep_one_once_set() {
        let v4 = r#"{ "schemaVersion": 4, "name": "Old", "sequences": [] }"#;
        let mut show = show_from_json(v4).unwrap();
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.background, None);
        show.background = Some(crate::Background {
            opacity: 0.5,
            ..crate::Background::new("/photos/house.jpg", -12.5, 9.0, 25.0)
        });
        let text = show_to_json(&show).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            saved["background"],
            serde_json::json!({ "path": "/photos/house.jpg", "x": -12.5, "y": 9.0, "width": 25.0, "opacity": 0.5 })
        );
        assert_eq!(show_from_json(&text).unwrap(), show);
    }

    #[test]
    fn version_5_files_open_without_a_house_model_and_keep_one_once_set() {
        let v5 = r#"{ "schemaVersion": 5, "name": "Old", "background": null }"#;
        let mut show = show_from_json(v5).unwrap();
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.house_model, None);
        let saved: Value = serde_json::from_str(&show_to_json(&show).unwrap()).unwrap();
        assert!(saved.get("houseModel").is_none(), "no model, nothing written");

        show.house_model = Some(crate::HouseModel {
            position: crate::Vec3::new(1.0, 0.0, -4.5),
            rotation_deg: crate::Vec3::new(0.0, 90.0, 0.0),
            scale: 0.3048,
            opacity: 0.6,
            ..crate::HouseModel::new("/models/house.glb")
        });
        let text = show_to_json(&show).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["houseModel"]["path"], "/models/house.glb");
        assert_eq!(saved["houseModel"]["rotationDeg"]["y"], 90.0);
        assert_eq!(show_from_json(&text).unwrap(), show);

        let sparse = r#"{ "schemaVersion": 6, "name": "x", "houseModel": { "path": "/h.obj" } }"#;
        assert_eq!(
            show_from_json(sparse).unwrap().house_model,
            Some(crate::HouseModel::new("/h.obj"))
        );
    }

    #[test]
    fn show_files_remember_the_folder_they_were_saved_in() {
        let show = sample_show();
        let text = show_file_to_json(&show, Some("/Shows/Home 2024")).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["savedIn"], "/Shows/Home 2024");
        assert!(
            text.find("\"schemaVersion\"").unwrap() < text.find("\"savedIn\"").unwrap(),
            "the show reads first"
        );
        let (read, saved_in) = show_file_from_json(&text).unwrap();
        assert_eq!(read, show);
        assert_eq!(saved_in.as_deref(), Some("/Shows/Home 2024"));

        // Files without it (version 7, or written without a folder) read as before.
        let plain = show_file_to_json(&show, None).unwrap();
        assert!(!plain.contains("savedIn"));
        assert_eq!(show_file_from_json(&plain).unwrap(), (show.clone(), None));
        // A savedIn that isn't text is ignored rather than refusing the show.
        let odd = text.replace("\"/Shows/Home 2024\"", "5");
        assert_eq!(show_file_from_json(&odd).unwrap(), (show, None));
    }

    #[test]
    fn version_7_full_paths_read_as_they_are() {
        let v7 = r#"{ "schemaVersion": 7, "name": "Old",
            "sequences": [ { "id": "77777777-0000-4000-8000-000000000001", "name": "Medley",
              "path": "/Shows/Medley.fseq", "audio": "/Shows/Medley.mp3" } ],
            "background": { "path": "/Shows/house.jpg", "x": 0, "y": 0, "width": 10 } }"#;
        let show = show_from_json(v7).unwrap();
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.sequences[0].path, "/Shows/Medley.fseq");
        assert_eq!(show.sequences[0].audio.as_deref(), Some("/Shows/Medley.mp3"));
        assert_eq!(show.background.unwrap().path, "/Shows/house.jpg");
        let saved: Value =
            serde_json::from_str(&show_to_json(&show_from_json(v7).unwrap()).unwrap()).unwrap();
        assert_eq!(saved["schemaVersion"], CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn version_6_regions_become_submodel_lines_with_ids() {
        let v6 = r#"{ "schemaVersion": 6, "name": "Old", "props": [
            { "id": "00000000-0000-4000-8000-000000000001", "name": "Arch",
              "shape": { "source": "generator", "type": "line", "nodes": 20, "length": 1.0 },
              "regions": [
                { "name": "Left", "kind": "nodes", "ranges": [ { "start": 0, "end": 5 }, { "start": 9, "end": 9 }, { "start": 10, "end": 12 } ] },
                { "name": "Face", "kind": "face", "mouths": { "O": [ { "start": 12, "end": 14 } ] } }
              ] } ] }"#;
        let show = show_from_json(v6).unwrap();
        let regions = &show.props[0].regions;
        assert_eq!(regions.len(), 2);
        assert_ne!(regions[0].id, regions[1].id);
        assert_eq!(
            regions[0].kind,
            crate::RegionKind::Nodes {
                lines: vec![vec![
                    Some(crate::NodeRun::new(0, 4)),
                    Some(crate::NodeRun::new(10, 11))
                ]],
                layout: crate::LineLayout::Horizontal,
                buffer: crate::BufferStyle::Default,
            }
        );
        let crate::RegionKind::Face(face) = &regions[1].kind else {
            panic!("a face")
        };
        assert_eq!(
            face.mouths[&crate::Phoneme::O],
            vec![crate::NodeRange::new(12, 14)]
        );
        // The ids are kept once saved.
        let text = show_to_json(&show).unwrap();
        assert_eq!(show_from_json(&text).unwrap(), show);
    }

    #[test]
    fn groups_keep_props_and_submodels_in_one_ordered_list() {
        let mut show = sample_show();
        show.props.push(Prop::new(
            "Other",
            ShapeSource::Generator(Generator::Line {
                nodes: 5,
                length: 1.0,
            }),
        ));
        let prop = &mut show.props[0];
        let region = crate::Region::nodes("Left", vec![vec![Some(crate::NodeRun::new(0, 9))]]);
        let member = crate::RegionRef {
            prop: prop.id,
            region: region.id,
        };
        prop.regions.push(region);
        let (arch, other) = (show.props[0].id, show.props[1].id);
        let mut group = crate::Group::new("Mixed");
        group.members = vec![arch.into(), member.into(), other.into()];
        show.groups.push(group);
        let text = show_to_json(&show).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        // A whole prop is its id, as before; a submodel names its prop and region.
        assert_eq!(saved["groups"][0]["members"][0], arch.to_string());
        assert_eq!(
            saved["groups"][0]["members"][1]["region"],
            member.region.to_string()
        );
        assert_eq!(saved["groups"][0]["members"][2], other.to_string());
        assert!(!text.contains("\"submodels\""), "{text}");
        assert_eq!(show_from_json(&text).unwrap(), show);
    }

    #[test]
    fn version_6_group_members_read_as_props() {
        let v6 = r#"{ "schemaVersion": 6, "name": "Old",
            "groups": [ { "id": "44444444-0000-4000-8000-000000000001", "name": "G",
              "members": [ "55555555-0000-4000-8000-000000000001", "55555555-0000-4000-8000-000000000002" ] } ] }"#;
        let show = show_from_json(v6).unwrap();
        let ids: Vec<String> = show.groups[0]
            .members
            .iter()
            .map(|m| match m {
                crate::GroupMember::Prop(id) => id.to_string(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            ids,
            [
                "55555555-0000-4000-8000-000000000001",
                "55555555-0000-4000-8000-000000000002"
            ]
        );
    }

    /// Development builds of schema 7 kept submodel members in a separate `submodels` list;
    /// those files still read, with the submodels after the whole props as they were drawn.
    #[test]
    fn early_version_7_submodel_lists_join_the_members() {
        let text = r#"{ "schemaVersion": 7, "name": "Dev",
            "groups": [ { "id": "44444444-0000-4000-8000-000000000001", "name": "G",
              "members": [ "55555555-0000-4000-8000-000000000001" ],
              "submodels": [ { "prop": "55555555-0000-4000-8000-000000000002",
                               "region": "66666666-0000-4000-8000-000000000001" } ] } ] }"#;
        let show = show_from_json(text).unwrap();
        let members = &show.groups[0].members;
        assert_eq!(members.len(), 2);
        assert!(matches!(members[0], crate::GroupMember::Prop(_)));
        assert!(
            matches!(members[1], crate::GroupMember::Region(r) if r.region.to_string() == "66666666-0000-4000-8000-000000000001")
        );
    }

    #[test]
    fn huge_submodels_are_refused_at_load() {
        let mut show = sample_show();
        let everything = vec![Some(crate::NodeRun::new(0, 49)); 300_000];
        show.props[0]
            .regions
            .push(crate::Region::nodes("Big", vec![everything]));
        let err = check_show(&show).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)), "{err}");
        assert!(err.to_string().contains("list 15000000 pixels"), "{err}");
        show.props[0].regions = (0..=crate::MAX_REGIONS_PER_PROP)
            .map(|i| crate::Region::nodes(format!("R{i}"), vec![]))
            .collect();
        let err = check_show(&show).unwrap_err();
        assert!(err.to_string().contains("at most 1000 per prop"), "{err}");
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
    fn shows_built_in_memory_are_checked_like_files() {
        let mut show = Show::new("Imported");
        let prop = Prop::new(
            "A",
            ShapeSource::Generator(Generator::Line {
                nodes: 5,
                length: 1.0,
            }),
        );
        let mut slot = PortSlot::new(prop.id);
        show.props.push(prop);
        slot.null_pixels = crate::MAX_NULL_PIXELS;
        let mut port = Port::new(1);
        port.slots.push(slot);
        let mut controller = Controller::new("C", "192.0.2.1", Protocol::Ddp);
        controller.ports.push(port);
        show.controllers.push(controller);
        assert_eq!(check_show(&show).unwrap(), show);
        show.controllers[0].ports[0].slots[0].null_pixels += 1;
        let err = check_show(&show).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)), "{err}");
    }

    #[test]
    fn sequence_offsets_stay_within_ten_seconds_and_ids_are_unique() {
        use crate::SequenceEntry;
        let mut show = Show::new("Music");
        let mut medley = SequenceEntry::new("Medley", "/shows/medley.fseq");
        medley.offset_ms = -crate::MAX_SEQUENCE_OFFSET_MS;
        show.sequences.push(medley.clone());
        assert_eq!(check_show(&show).unwrap(), show);
        show.sequences[0].offset_ms = crate::MAX_SEQUENCE_OFFSET_MS + 1;
        let err = check_show(&show).unwrap_err();
        assert!(matches!(err, ModelError::LimitExceeded(_)), "{err}");
        assert!(err.to_string().contains("'Medley'"), "{err}");
        assert!(err.to_string().contains("10000 ms"), "{err}");

        show.sequences[0].offset_ms = 0;
        let mut copy = medley.clone();
        copy.name = "Medley again".into();
        show.sequences.push(copy);
        let err = check_show(&show).unwrap_err();
        assert!(err.to_string().contains("'Medley again'"), "{err}");
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
