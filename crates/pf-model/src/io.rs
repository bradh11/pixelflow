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
    v1_to_v2, v2_to_v3, v3_to_v4, v4_to_v5, v5_to_v6, v6_to_v7, v7_to_v8, v8_to_v9,
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
        return Err(ModelError::LimitExceeded(problem));
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
    fn version_7_and_8_files_open_unchanged_and_save_as_version_9() {
        // A show with no new prop shapes, saved now, then labelled as written by older versions:
        // 7 (full paths) and 8 (relative paths and `savedIn`, which still has no new shapes).
        let mut show = sample_show();
        show.sequences
            .push(crate::SequenceEntry::new("Medley", "Medley.fseq"));
        let text = show_file_to_json(&show, Some("/Shows/Haas")).unwrap();
        assert!(text.contains("\"schemaVersion\": 9"), "written as version 9");
        for version in [7, 8, 9] {
            let old = text.replace("\"schemaVersion\": 9", &format!("\"schemaVersion\": {version}"));
            let (read, saved_in) = show_file_from_json(&old).unwrap();
            assert_eq!(read, show, "version {version} reads as it was");
            assert_eq!(read.schema_version, 9);
            assert_eq!(saved_in.as_deref(), Some("/Shows/Haas"), "version {version}");
            let saved: Value = serde_json::from_str(&show_to_json(&read).unwrap()).unwrap();
            assert_eq!(saved["schemaVersion"], 9);
        }
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
        assert_eq!(saved["schemaVersion"], 9);
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
        let text = show_file_to_json(&show, Some("/Shows/Haas 2024")).unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["savedIn"], "/Shows/Haas 2024");
        assert!(
            text.find("\"schemaVersion\"").unwrap() < text.find("\"savedIn\"").unwrap(),
            "the show reads first"
        );
        let (read, saved_in) = show_file_from_json(&text).unwrap();
        assert_eq!(read, show);
        assert_eq!(saved_in.as_deref(), Some("/Shows/Haas 2024"));

        // Files without it (version 7, or written without a folder) read as before.
        let plain = show_file_to_json(&show, None).unwrap();
        assert!(!plain.contains("savedIn"));
        assert_eq!(show_file_from_json(&plain).unwrap(), (show.clone(), None));
        // A savedIn that isn't text is ignored rather than refusing the show.
        let odd = text.replace("\"/Shows/Haas 2024\"", "5");
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
        assert_eq!(saved["schemaVersion"], 9);
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
