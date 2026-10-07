//! Structural checks on the assistant's tools: every engine edit (show and sequence) yields a
//! valid tool definition whose schema accepts real edits and turns back into the same edit,
//! names are unique and provider-safe, and no tool reaches outside the draft.

use pf_ai::Toolbox;
use pf_ai::tools::{
    FILE_OPERATIONS, ONE_TOOL_BUDGET_BYTES, TOOL_BUDGET_BYTES, TOOL_HEADROOM_BYTES, ToolKind, sequence_edit,
    sequence_tool_name, shape_settings, show_edit, show_tool_name,
};
use pf_engine::{Edit, SequenceEdit};
use pf_model::{
    Background, Controller, Corner, CubeStart, CubeStyle, Generator, Group, GroupMember, HouseModel, NodeRun,
    PolySegment, Port, PortSlot, Prop, Protocol, Region, RegionRef, SacnConfig, SequenceEntry, ShapeSource,
    StrandStyle, TreeStyle, Vec3,
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
            style: TreeStyle::Round,
            degrees: 360.0,
            start_angle: 0.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
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

/// One of every prop shape that isn't in `rich_prop` or the other samples, with its options set.
fn shape_samples() -> Vec<Generator> {
    vec![
        Generator::Line {
            nodes: 50,
            length: 3.0,
        },
        Generator::arch(25, 2.0, 1.0),
        Generator::circle(30, 0.5),
        Generator::Matrix {
            columns: 16,
            rows: 8,
            width: 2.0,
            height: 1.0,
            wiring: Default::default(),
        },
        Generator::Tree {
            strings: 8,
            nodes_per_string: 20,
            height: 2.0,
            base_radius: 0.8,
            top_radius: 0.0,
            serpentine: false,
            style: TreeStyle::Ribbon,
            degrees: 180.0,
            start_angle: 45.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
        },
        Generator::star(5, 50, 1.0, 0.4),
        Generator::PolyLine {
            vertices: vec![Vec3::ZERO, Vec3::new(1.0, 0.5, 0.0), Vec3::new(2.0, 0.0, 0.0)],
            segments: vec![
                PolySegment::straight(10),
                PolySegment {
                    nodes: 12,
                    curve: Some([Vec3::new(1.2, 0.8, 0.0), Vec3::new(1.8, 0.4, 0.0)]),
                },
            ],
            spread_nodes: Some(20),
        },
        Generator::CandyCanes {
            canes: 4,
            nodes_per_cane: 18,
            width: 3.0,
            height: 1.0,
            cane_height: 1.2,
            reverse: true,
            sticks: false,
            alternate_nodes: true,
            skew_deg: 5.0,
            start_right: true,
        },
        Generator::Icicles {
            strings: 2,
            lights_per_string: 50,
            drops: vec![3, 4, 5, 4],
            width: 4.0,
            drop_height: 0.6,
            alternate_nodes: false,
        },
        Generator::WindowFrame {
            top: 20,
            sides: 30,
            bottom: 20,
            width: 1.0,
            height: 1.5,
            start: Corner::TopRight,
            counter_clockwise: true,
        },
        Generator::Wreath {
            nodes: 40,
            radius: 0.6,
            start_at_bottom: true,
            counter_clockwise: false,
        },
        Generator::Spinner {
            arms: 6,
            nodes_per_arm: 10,
            hollow: 20,
            start_angle: 30.0,
            arc: 360.0,
            zig_zag: true,
            alternate: false,
            from_center: true,
            clockwise: false,
            radius: 1.0,
        },
        Generator::Sphere {
            columns: 12,
            rows: 10,
            radius: 1.0,
            start_latitude: -80.0,
            end_latitude: 80.0,
            degrees: 360.0,
            start: Corner::BottomRight,
            strand_style: StrandStyle::AlternatePixel,
        },
        Generator::Cube {
            width: 5,
            height: 5,
            depth: 5,
            spacing: 0.2,
            start: CubeStart::BackTopRight,
            style: CubeStyle::StackedLeftRight,
            strand_style: StrandStyle::NoZigZag,
            strand_per_layer: true,
        },
    ]
}

/// The `type` of every prop shape: each must have a sample that fits its tool.
fn shape_type(shape: &Generator) -> &'static str {
    match shape {
        Generator::Line { .. } => "line",
        Generator::Arch { .. } => "arch",
        Generator::Circle { .. } => "circle",
        Generator::Matrix { .. } => "matrix",
        Generator::Tree { .. } => "tree",
        Generator::Star { .. } => "star",
        Generator::PolyLine { .. } => "polyLine",
        Generator::CandyCanes { .. } => "candyCanes",
        Generator::Icicles { .. } => "icicles",
        Generator::WindowFrame { .. } => "windowFrame",
        Generator::Wreath { .. } => "wreath",
        Generator::Spinner { .. } => "spinner",
        Generator::Sphere { .. } => "sphere",
        Generator::Cube { .. } => "cube",
        Generator::CustomGrid { .. } => "customGrid",
    }
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
        Edit::UpdateProp {
            prop: Prop::new(
                "Roofline",
                ShapeSource::Generator(Generator::PolyLine {
                    vertices: vec![Vec3::ZERO, Vec3::new(2.0, 1.0, 0.0)],
                    segments: vec![PolySegment::straight(20)],
                    spread_nodes: None,
                }),
            ),
        },
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
    .into_iter()
    .chain(shape_samples().into_iter().map(|shape| Edit::AddProp {
        prop: Prop::new(shape_type(&shape), ShapeSource::Generator(shape)),
    }))
    .collect()
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

/// The variant's name, by an exhaustive match: a new `Edit` variant doesn't compile here until
/// it's named, and then the count checks fail until it has a sample.
fn show_variant(edit: &Edit) -> &'static str {
    match edit {
        Edit::RenameShow { .. } => "RenameShow",
        Edit::SetFrameRate { .. } => "SetFrameRate",
        Edit::AddProp { .. } => "AddProp",
        Edit::UpdateProp { .. } => "UpdateProp",
        Edit::RemoveProp { .. } => "RemoveProp",
        Edit::AddGroup { .. } => "AddGroup",
        Edit::UpdateGroup { .. } => "UpdateGroup",
        Edit::RemoveGroup { .. } => "RemoveGroup",
        Edit::AddController { .. } => "AddController",
        Edit::UpdateController { .. } => "UpdateController",
        Edit::RemoveController { .. } => "RemoveController",
        Edit::AddSequence { .. } => "AddSequence",
        Edit::UpdateSequence { .. } => "UpdateSequence",
        Edit::RemoveSequence { .. } => "RemoveSequence",
        Edit::MoveSequence { .. } => "MoveSequence",
        Edit::SetBackground { .. } => "SetBackground",
        Edit::SetHouseModel { .. } => "SetHouseModel",
    }
}

/// Like [`show_variant`], for sequence edits.
fn sequence_variant(edit: &SequenceEdit) -> &'static str {
    match edit {
        SequenceEdit::UpdateInfo { .. } => "UpdateInfo",
        SequenceEdit::AddRow { .. } => "AddRow",
        SequenceEdit::RemoveRow { .. } => "RemoveRow",
        SequenceEdit::MoveRow { .. } => "MoveRow",
        SequenceEdit::AddLayer { .. } => "AddLayer",
        SequenceEdit::RemoveLayer { .. } => "RemoveLayer",
        SequenceEdit::AddEffect { .. } => "AddEffect",
        SequenceEdit::UpdateEffect { .. } => "UpdateEffect",
        SequenceEdit::SetEffectTiming { .. } => "SetEffectTiming",
        SequenceEdit::SetEffectParams { .. } => "SetEffectParams",
        SequenceEdit::MoveEffect { .. } => "MoveEffect",
        SequenceEdit::RemoveEffect { .. } => "RemoveEffect",
        SequenceEdit::AddTimingTrack { .. } => "AddTimingTrack",
        SequenceEdit::UpdateTimingTrack { .. } => "UpdateTimingTrack",
        SequenceEdit::RemoveTimingTrack { .. } => "RemoveTimingTrack",
        SequenceEdit::RenameTimingTrack { .. } => "RenameTimingTrack",
        SequenceEdit::MoveTimingTrack { .. } => "MoveTimingTrack",
        SequenceEdit::AddMarks { .. } => "AddMarks",
        SequenceEdit::SetMark { .. } => "SetMark",
        SequenceEdit::RemoveMarks { .. } => "RemoveMarks",
        SequenceEdit::SplitMark { .. } => "SplitMark",
        SequenceEdit::MergeMarks { .. } => "MergeMarks",
        SequenceEdit::GenerateMarks { .. } => "GenerateMarks",
        SequenceEdit::CopyMarks { .. } => "CopyMarks",
        SequenceEdit::SpreadLyrics { .. } => "SpreadLyrics",
        SequenceEdit::LabelMarks { .. } => "LabelMarks",
        SequenceEdit::BreakIntoWords { .. } => "BreakIntoWords",
    }
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
    let variants: BTreeSet<&str> = samples.iter().map(show_variant).collect();
    assert_eq!(variants.len(), 17, "the samples cover every Edit variant");
    for edit in samples {
        let tag = tag(&edit);
        let tool = toolbox.find(&show_tool_name(&tag)).expect("tool exists");
        assert_valid_against(&tool.spec.input_schema, &input(&edit), &tool.spec.name);
        assert_eq!(show_edit(&tag, &input(&edit)).unwrap(), edit, "{tag} round-trips");
    }
}

fn shape_of(edit: &Edit) -> Generator {
    match edit {
        Edit::AddProp { prop } => match &prop.shape {
            ShapeSource::Generator(g) => g.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_prop_shape_fits_the_add_prop_tool_and_round_trips() {
    let toolbox = Toolbox::new();
    let tool = toolbox.find(&show_tool_name("addProp")).expect("tool exists");
    let mut types = BTreeSet::new();
    for shape in shape_samples().into_iter().chain([Generator::CustomGrid {
        columns: 2,
        rows: 1,
        cells: vec![1, 2],
    }]) {
        let kind = shape_type(&shape);
        assert_eq!(tag(&shape), kind, "the shape's JSON type");
        types.insert(kind);
        let edit = Edit::AddProp {
            prop: Prop::new(kind, ShapeSource::Generator(shape)),
        };
        assert_valid_against(&tool.spec.input_schema, &input(&edit), kind);
        // The full settings the lookup gives fit the shape too.
        assert_valid_against(
            &shape_settings(kind).expect("a lookup for every shape"),
            &serde_json::to_value(shape_of(&edit)).unwrap(),
            kind,
        );
        assert_eq!(
            show_edit("addProp", &input(&edit)).unwrap(),
            edit,
            "{kind} round-trips"
        );
    }
    // `shape_type` matches every Generator variant, so a new one fails to compile until it has a
    // sample here too.
    assert_eq!(types.len(), 15, "a sample of every shape");
    // The tool's schema lists every shape kind.
    let listed = tool.spec.input_schema.to_string();
    for kind in &types {
        assert!(
            listed.contains(&format!("\"{kind}\"")),
            "{kind} isn't in the tool schema"
        );
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
    assert_eq!(tool_tags.len(), 27);
    let variants: BTreeSet<&str> = samples.iter().map(sequence_variant).collect();
    assert_eq!(variants.len(), 27, "the samples cover every SequenceEdit variant");
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
        "start", "play", "fpp", "network", "render", "discover", "import", "relink", "locate", "missing",
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
fn no_tool_checks_finds_or_relinks_the_shows_files() {
    // The engine can check whether the show's files are there, search the disk for missing
    // ones, and point the show at files found or located: none of that is the assistant's.
    let names: BTreeSet<String> = Toolbox::new()
        .tools()
        .iter()
        .map(|t| t.spec.name.clone())
        .collect();
    for operation in FILE_OPERATIONS {
        for name in [
            operation.to_string(),
            show_tool_name(operation),
            sequence_tool_name(operation),
        ] {
            assert!(!names.contains(&name), "{name} is a tool");
        }
    }
    for name in &names {
        let edit_name = name
            .strip_prefix("show_")
            .or_else(|| name.strip_prefix("sequence_"))
            .unwrap_or(name);
        assert!(!FILE_OPERATIONS.contains(&edit_name), "{name} works on files");
    }
}

/// A provider, its tools' total size, and each tool's size and name.
type ToolSizes = (&'static str, usize, Vec<(usize, String)>);

/// The tool definitions exactly as each provider's request carries them (bytes of JSON),
/// largest first, per provider.
fn tool_sizes() -> Vec<ToolSizes> {
    let specs = Toolbox::new().specs();
    let request = pf_ai::provider::TurnRequest {
        model: "any",
        system: "",
        tools: &specs,
        messages: &[],
        max_tokens: 1,
    };
    [
        ("Anthropic", pf_ai::anthropic::request_body(&request)),
        ("OpenAI", pf_ai::openai::request_body(&request, true)),
    ]
    .into_iter()
    .map(|(provider, body)| {
        let mut sizes: Vec<(usize, String)> = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t.to_string().len(), t["name"].as_str().unwrap().to_string()))
            .collect();
        sizes.sort_by(|a, b| b.cmp(a));
        let total = body["tools"].to_string().len();
        (provider, total, sizes)
    })
    .collect()
}

#[test]
fn choices_documented_only_in_rust_are_plain_name_lists() {
    let toolbox = Toolbox::new();
    let add = toolbox
        .tools()
        .iter()
        .find(|t| t.spec.name == "show_add_prop")
        .unwrap();
    // Shape settings now come from `shape_settings`; gather its definitions with the tool's own.
    let mut all = add.spec.input_schema["$defs"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for shape in ["star", "matrix", "arch", "circle", "tree"] {
        if let Some(found) = pf_ai::tools::shape_settings(shape).and_then(|s| s["$defs"].as_object().cloned())
        {
            all.extend(found);
        }
    }
    let defs = &serde_json::Value::Object(all);
    assert_eq!(
        defs["StarStart"]["enum"],
        json!(["top", "bottom", "leftLeg", "rightLeg"])
    );
    assert_eq!(defs["StarStart"]["type"], "string");
    assert!(defs["StarStart"].get("oneOf").is_none());
    assert_eq!(defs["Orientation"]["enum"], json!(["horizontal", "vertical"]));
    // Choices whose names are explained to the model keep their explanations.
    assert!(defs["BufferStyle"]["oneOf"][0].get("description").is_some());
}

#[test]
fn tool_definitions_stay_small() {
    for (provider, total, sizes) in tool_sizes() {
        println!(
            "{provider} tools: {} definitions, {total} bytes (~{} tokens)",
            sizes.len(),
            total / 4
        );
        for (size, name) in sizes.iter().take(6) {
            println!("  {size:>6} {name}");
        }
        // Was 114 KB with every large definition repeated in each tool that takes it, then 63.5
        // KB with every prop shape and effect kind spelled out.
        assert!(
            total + TOOL_HEADROOM_BYTES <= TOOL_BUDGET_BYTES,
            "{provider}: tool definitions grew to {total} bytes: less than {TOOL_HEADROOM_BYTES} bytes of headroom is left"
        );
        for (size, name) in &sizes {
            assert!(
                *size <= ONE_TOOL_BUDGET_BYTES,
                "{provider}: {name} is {size} bytes, over the {ONE_TOOL_BUDGET_BYTES}-byte budget for one tool"
            );
        }
    }
    // Each large definition is spelled out in one tool only.
    for (def, owner) in [
        ("Prop", "show_add_prop"),
        ("Controller", "show_add_controller"),
        ("Effect", "sequence_add_effect"),
        ("EffectParams", "sequence_add_effect"),
    ] {
        let carriers: Vec<String> = Toolbox::new()
            .tools()
            .iter()
            .filter(|t| t.spec.input_schema["$defs"].get(def).is_some())
            .map(|t| t.spec.name.clone())
            .collect();
        assert_eq!(carriers, [owner], "{def}");
    }
    let update = Toolbox::new()
        .find("show_update_prop")
        .unwrap()
        .spec
        .input_schema
        .clone();
    assert_eq!(
        update["properties"]["prop"]["description"],
        "A Prop, exactly as `prop` in the show_add_prop tool's input."
    );
}

#[test]
fn tool_inputs_that_dont_fit_are_explained() {
    let err = show_edit("setFrameRate", &json!({ "fps": "fast" })).unwrap_err();
    assert!(err.starts_with("That input doesn't fit this edit"), "{err}");
    assert!(show_edit("renameShow", &json!(["x"])).is_err());
}

#[test]
fn shapes_and_effect_settings_are_compact_and_looked_up_on_demand() {
    let toolbox = Toolbox::new();
    let add_prop = &toolbox.find("show_add_prop").unwrap().spec.input_schema;
    let generator = &add_prop["$defs"]["Generator"];
    assert!(generator.get("oneOf").is_none(), "{generator}");
    let types: Vec<&str> = generator["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    for shape in ["line", "arch", "circle", "matrix", "tree", "star", "cube"] {
        assert!(types.contains(&shape), "{shape} in {types:?}");
    }
    assert!(
        generator["description"]
            .as_str()
            .unwrap()
            .contains("shape_settings")
    );
    // Definitions only the full shapes used are gone.
    assert!(add_prop["$defs"].get("TreeStyle").is_none());

    let tool = toolbox.find("shape_settings").expect("a lookup tool");
    assert_eq!(tool.spec.input_schema["required"], json!(["type"]));
    let tree = shape_settings("tree").unwrap();
    assert_eq!(tree["properties"]["type"]["const"], "tree");
    assert!(tree["properties"].get("strings").is_some(), "{tree}");
    // It carries the definitions it refers to.
    assert!(tree["$defs"].get("TreeStyle").is_some(), "{tree}");
    assert!(shape_settings("blimp").is_none());
    for shape in types {
        let schema = shape_settings(shape).unwrap();
        jsonschema::draft202012::meta::validate(&schema).unwrap_or_else(|e| panic!("{shape}: {e}"));
    }

    let add_effect = &toolbox.find("sequence_add_effect").unwrap().spec.input_schema;
    let params = &add_effect["$defs"]["EffectParams"];
    assert!(params.get("oneOf").is_none(), "{params}");
    assert_eq!(params["required"], json!(["kind"]));
    assert!(
        params["properties"]["kind"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("twinkle"))
    );
    assert!(
        params["description"]
            .as_str()
            .unwrap()
            .contains("list_effect_kinds")
    );
    assert!(add_effect["$defs"].get("ChaseParams").is_none());
    let kinds = toolbox.find("list_effect_kinds").unwrap();
    assert!(kinds.spec.input_schema["properties"].get("kind").is_some());
}

#[test]
fn settings_a_shape_or_effect_doesnt_have_are_refused_by_name() {
    let mut prop = serde_json::to_value(rich_prop()).unwrap();
    prop["shape"]["strands"] = json!(12);
    let err = show_edit("addProp", &json!({ "prop": prop })).unwrap_err();
    assert!(
        err.contains("prop.shape.strands") && err.contains("shape_settings"),
        "{err}"
    );

    let effect = serde_json::to_value(Effect::new(EffectKind::Chase, 0, 1000)).unwrap();
    let mut misspelled = effect.clone();
    misspelled["params"]["sped"] = json!(4);
    let input = json!({ "row": pf_sequence::RowId::new(), "layer": 0, "effect": misspelled });
    let err = sequence_edit("addEffect", &input).unwrap_err();
    assert!(
        err.contains("effect.params.sped") && err.contains("list_effect_kinds"),
        "{err}"
    );

    // Leaving out what defaults is fine, and so is a false flag that isn't written back.
    let mut sparse = effect.clone();
    sparse["params"] = json!({ "kind": "chase", "speed": 2.0 });
    let input = json!({ "row": pf_sequence::RowId::new(), "layer": 0, "effect": sparse });
    assert!(sequence_edit("addEffect", &input).is_ok());
    let mut plain = serde_json::to_value(controller(&rich_prop())).unwrap();
    plain.as_object_mut().unwrap().retain(|_, v| !v.is_null());
    assert!(show_edit("addController", &json!({ "controller": plain })).is_ok());
}
