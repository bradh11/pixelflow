//! Structural checks on the assistant's tools: every engine edit (show and sequence) yields a
//! valid tool definition whose schema accepts real edits and turns back into the same edit,
//! names are unique and provider-safe, and no tool reaches outside the draft.

use pf_ai::Toolbox;
use pf_ai::tools::{ToolKind, sequence_edit, sequence_tool_name, show_edit, show_tool_name};
use pf_engine::{Edit, SequenceEdit};
use pf_model::{
    Background, Controller, Generator, Group, GroupMember, HouseModel, NodeRun, Port, PortSlot, Prop,
    Protocol, Region, RegionRef, SacnConfig, SequenceEntry, ShapeSource, Vec3,
};
use pf_sequence::{Effect, EffectKind, EffectParams, Mark, Palette, Row, Target, TimingKind, TimingTrack};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn rich_prop() -> Prop {
    let mut prop = Prop::new(
        "Mega Tree",
        ShapeSource::Generator(Generator::Tree {
            strings: 16,
            nodes_per_string: 50,
            height: 3.0,
            base_radius: 1.0,
            top_radius: 0.1,
            serpentine: true,
        }),
    );
    prop.transform.position = Vec3::new(1.5, 0.0, -2.0);
    prop.regions
        .push(Region::nodes("Top", vec![vec![Some(NodeRun::new(1, 10)), None]]));
    prop.tags.push("yard".into());
    prop
}

fn measured_prop() -> Prop {
    Prop::new(
        "Custom",
        ShapeSource::Measured {
            points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)],
            provenance: pf_model::Provenance::Import,
        },
    )
}

fn controller(prop: &Prop) -> Controller {
    let mut c = Controller::new("Falcon", "10.0.0.5", Protocol::Sacn(SacnConfig::default()));
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    c.ports.push(port);
    c
}

fn group(prop: &Prop) -> Group {
    let mut g = Group::new("Yard");
    g.members.push(GroupMember::Prop(prop.id));
    g.members.push(GroupMember::Region(RegionRef {
        prop: prop.id,
        region: prop.regions[0].id,
    }));
    g
}

/// At least one real edit of every show edit type.
fn show_samples() -> Vec<Edit> {
    let prop = rich_prop();
    let sequence = SequenceEntry::new("Wizards", "/shows/wizards.fseq");
    vec![
        Edit::RenameShow {
            name: "Christmas".into(),
        },
        Edit::SetFrameRate { fps: 20 },
        Edit::AddProp { prop: prop.clone() },
        Edit::AddProp {
            prop: measured_prop(),
        },
        Edit::AddProp {
            prop: Prop::new(
                "Grid",
                ShapeSource::Generator(Generator::CustomGrid {
                    columns: 2,
                    rows: 1,
                    cells: vec![1, 2],
                }),
            ),
        },
        Edit::UpdateProp { prop: prop.clone() },
        Edit::RemoveProp { id: prop.id },
        Edit::AddGroup { group: group(&prop) },
        Edit::UpdateGroup { group: group(&prop) },
        Edit::RemoveGroup {
            id: pf_model::GroupId::new(),
        },
        Edit::AddController {
            controller: controller(&prop),
        },
        Edit::AddController {
            controller: Controller::new("Bench", "127.0.0.1", Protocol::Ddp),
        },
        Edit::UpdateController {
            controller: controller(&prop),
        },
        Edit::RemoveController {
            id: pf_model::ControllerId::new(),
        },
        Edit::AddSequence {
            sequence: sequence.clone(),
        },
        Edit::UpdateSequence {
            sequence: sequence.clone(),
        },
        Edit::RemoveSequence { id: sequence.id },
        Edit::MoveSequence {
            id: sequence.id,
            index: 2,
        },
        Edit::SetBackground {
            background: Some(Background::new("/photos/house.jpg", -10.0, 8.0, 20.0)),
        },
        Edit::SetBackground { background: None },
        Edit::SetHouseModel {
            house_model: Some(HouseModel::new("/models/house.glb")),
        },
        Edit::SetHouseModel { house_model: None },
    ]
}

/// At least one real edit of every sequence edit type, with every effect kind.
fn sequence_samples() -> Vec<SequenceEdit> {
    let prop = rich_prop();
    let row = Row::new(Target::Region {
        prop: prop.id,
        region: prop.regions[0].id,
    });
    let mut effect = Effect::new(EffectKind::Twinkle, 1000, 2000);
    effect.palette = Palette::new(vec![pf_model::Rgb::RED, pf_model::Rgb::new(0, 128, 255)]);
    let track = TimingTrack::new("Beats", TimingKind::Beats, vec![Mark::new(0, 500, "one")]);
    let words = TimingTrack::new("Words", TimingKind::Words, vec![]);
    let mut out = vec![
        SequenceEdit::UpdateInfo {
            name: "Song".into(),
            audio: Some("song.mp3".into()),
            duration_ms: 60_000,
            frame_ms: 25,
        },
        SequenceEdit::AddRow {
            row: row.clone(),
            index: Some(0),
        },
        SequenceEdit::AddRow {
            row: Row::new(Target::Group(pf_model::GroupId::new())),
            index: None,
        },
        SequenceEdit::RemoveRow { id: row.id },
        SequenceEdit::MoveRow { id: row.id, index: 1 },
        SequenceEdit::AddLayer {
            row: row.id,
            index: None,
        },
        SequenceEdit::RemoveLayer {
            row: row.id,
            layer: 0,
        },
        SequenceEdit::AddEffect {
            row: row.id,
            layer: 0,
            effect: effect.clone(),
        },
        SequenceEdit::UpdateEffect {
            effect: effect.clone(),
        },
        SequenceEdit::SetEffectTiming {
            id: effect.id,
            start_ms: 0,
            end_ms: 10,
        },
        SequenceEdit::MoveEffect {
            id: effect.id,
            row: row.id,
            layer: 1,
            start_ms: 5,
            end_ms: 50,
        },
        SequenceEdit::RemoveEffect { id: effect.id },
        SequenceEdit::AddTimingTrack { track: track.clone() },
        SequenceEdit::UpdateTimingTrack { track: track.clone() },
        SequenceEdit::RemoveTimingTrack { id: track.id },
        SequenceEdit::RenameTimingTrack {
            id: track.id,
            name: "Downbeats".into(),
        },
        SequenceEdit::MoveTimingTrack {
            id: track.id,
            index: 0,
        },
        SequenceEdit::AddMarks {
            track: track.id,
            marks: vec![Mark::new(500, 900, "")],
        },
        SequenceEdit::SetMark {
            track: track.id,
            index: 0,
            mark: Mark::new(0, 400, "uno"),
        },
        SequenceEdit::RemoveMarks {
            track: track.id,
            indices: vec![0],
        },
        SequenceEdit::SplitMark {
            track: track.id,
            index: 0,
            at_ms: 200,
        },
        SequenceEdit::MergeMarks {
            track: track.id,
            index: 0,
        },
        SequenceEdit::GenerateMarks {
            track: track.id,
            every_ms: 500,
            from_ms: 0,
            to_ms: 5000,
        },
        SequenceEdit::CopyMarks {
            from: track.id,
            to: words.id,
            every: 4,
        },
        SequenceEdit::SpreadLyrics {
            track: track.id,
            lines: vec!["Deck the halls".into()],
            from_ms: 0,
            to_ms: 3000,
        },
        SequenceEdit::LabelMarks {
            track: track.id,
            indices: vec![0],
            labels: vec!["Fa".into()],
        },
        SequenceEdit::BreakIntoWords {
            track: track.id,
            indices: vec![0],
            words: words.id,
        },
    ];
    for kind in EffectKind::ALL {
        out.push(SequenceEdit::SetEffectParams {
            id: effect.id,
            params: EffectParams::default_for(kind),
        });
    }
    out
}

fn tag(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value).unwrap()["type"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The edit as a tool input: its JSON without the tag.
fn input(value: &impl serde::Serialize) -> Value {
    let mut v = serde_json::to_value(value).unwrap();
    v.as_object_mut().unwrap().remove("type");
    v
}

fn assert_valid_against(schema: &Value, instance: &Value, what: &str) {
    let validator =
        jsonschema::draft202012::new(schema).unwrap_or_else(|e| panic!("{what}: bad schema: {e}"));
    let errors: Vec<String> = validator.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(
        errors.is_empty(),
        "{what}: {instance} doesn't fit its tool schema: {errors:?}"
    );
}

#[test]
fn every_show_edit_yields_a_valid_tool_that_round_trips() {
    let toolbox = Toolbox::new();
    let tool_tags: BTreeSet<String> = toolbox
        .tools()
        .iter()
        .filter_map(|t| match &t.kind {
            ToolKind::ShowEdit { tag } => Some(tag.clone()),
            _ => None,
        })
        .collect();
    let samples = show_samples();
    let sample_tags: BTreeSet<String> = samples.iter().map(tag).collect();
    // A new Edit variant appears in the schema, so it gets a tool; this fails until it also has
    // a sample here (and so is proven to work).
    assert_eq!(tool_tags, sample_tags, "every show edit has a tool and a sample");
    assert_eq!(tool_tags.len(), 17);
    for edit in samples {
        let tag = tag(&edit);
        let tool = toolbox.find(&show_tool_name(&tag)).expect("tool exists");
        assert_valid_against(&tool.spec.input_schema, &input(&edit), &tool.spec.name);
        assert_eq!(show_edit(&tag, &input(&edit)).unwrap(), edit, "{tag} round-trips");
    }
}

#[test]
fn every_sequence_edit_yields_a_valid_tool_that_round_trips() {
    let toolbox = Toolbox::new();
    let tool_tags: BTreeSet<String> = toolbox
        .tools()
        .iter()
        .filter_map(|t| match &t.kind {
            ToolKind::SequenceEdit { tag } => Some(tag.clone()),
            _ => None,
        })
        .collect();
    let samples = sequence_samples();
    let sample_tags: BTreeSet<String> = samples.iter().map(tag).collect();
    assert_eq!(
        tool_tags, sample_tags,
        "every sequence edit has a tool and a sample"
    );
    for edit in samples {
        let tag = tag(&edit);
        let tool = toolbox.find(&sequence_tool_name(&tag)).expect("tool exists");
        assert_valid_against(&tool.spec.input_schema, &input(&edit), &tool.spec.name);
        assert_eq!(
            sequence_edit(&tag, &input(&edit)).unwrap(),
            edit,
            "{tag} round-trips"
        );
    }
}

#[test]
fn every_tool_definition_is_well_formed_for_both_providers() {
    let toolbox = Toolbox::new();
    let mut names = BTreeSet::new();
    for tool in toolbox.tools() {
        let spec = &tool.spec;
        assert!(names.insert(spec.name.clone()), "{} is listed twice", spec.name);
        // Anthropic and OpenAI both take ^[a-zA-Z0-9_-]{1,64}$.
        assert!(
            !spec.name.is_empty()
                && spec.name.len() <= 64
                && spec
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "{}",
            spec.name
        );
        assert!(spec.description.len() > 20, "{} needs a description", spec.name);
        assert!(
            !spec.description.contains("[`"),
            "{}: rustdoc links in a description",
            spec.name
        );
        let schema = &spec.input_schema;
        assert_eq!(schema["type"], "object", "{}: input is an object", spec.name);
        assert!(schema.get("$schema").is_none(), "{}", spec.name);
        jsonschema::draft202012::meta::validate(schema).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        // Every $ref points at a definition carried with the tool.
        let text = schema.to_string();
        for part in text.split("\"$ref\":\"#/$defs/").skip(1) {
            let name = part.split('"').next().unwrap();
            assert!(
                schema["$defs"].get(name).is_some(),
                "{}: missing definition {name}",
                spec.name
            );
        }
        // Required fields are real fields.
        if let Some(required) = schema["required"].as_array() {
            for field in required {
                assert!(
                    schema["properties"].get(field.as_str().unwrap()).is_some(),
                    "{}: {field}",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn no_tool_reaches_outside_the_draft() {
    // Words that would mean files, output, playback, or devices.
    const OUTSIDE: &[&str] = &[
        "save", "export", "write", "file", "files", "output", "device", "devices", "push", "send", "upload",
        "start", "play", "fpp", "network", "render", "discover", "import",
    ];
    for tool in Toolbox::new().tools() {
        for word in tool.spec.name.split('_') {
            assert!(
                !OUTSIDE.contains(&word),
                "{} reaches outside the draft",
                tool.spec.name
            );
        }
    }
}

#[test]
fn tool_inputs_that_dont_fit_are_explained() {
    let err = show_edit("setFrameRate", &json!({ "fps": "fast" })).unwrap_err();
    assert!(err.starts_with("That input doesn't fit this edit"), "{err}");
    assert!(show_edit("renameShow", &json!(["x"])).is_err());
}
