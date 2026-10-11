//! Every value of the engine's enums that the window mirrors as a union of string literals,
//! and every variant of its tagged unions: the window's side checks its unions have exactly
//! these members, so one added on either side alone fails a test.

use super::*;

/// Every variant of an enum without fields, as the window receives it. The match has no
/// wildcard: a variant added to the enum doesn't compile until it's listed here.
macro_rules! every {
    ($ty:ty: $($variant:ident),+ $(,)?) => {{
        #[allow(dead_code)]
        fn listed(value: $ty) {
            match value {
                $(<$ty>::$variant => {}),+
            }
        }
        Value::Array(vec![$(wire(&<$ty>::$variant)),+])
    }};
}

#[test]
fn union_members() {
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("unions", shell.stable());
    let mut all = |union: &str, values: Value| fixtures.all_of(union, None, &values);

    // The show.
    all(
        "types.ColorOrder",
        every!(pf_model::ColorOrder: Rgb, Rbg, Grb, Gbr, Brg, Bgr, Rgbw, Grbw),
    );
    all(
        "types.Controller['adapter']",
        every!(pf_model::AdapterKind: Fpp, Falcon, Wled, Generic),
    );
    all(
        "types.MatrixWiring['start']",
        every!(pf_model::Corner: BottomLeft, BottomRight, TopLeft, TopRight),
    );
    all(
        "types.MatrixWiring['orientation']",
        every!(pf_model::Orientation: Horizontal, Vertical),
    );
    all(
        "types.StrandStyle",
        every!(pf_model::StrandStyle: ZigZag, NoZigZag, AlternatePixel),
    );
    all(
        "types.CubeStart",
        every!(pf_model::CubeStart: FrontBottomLeft, FrontBottomRight, FrontTopLeft, FrontTopRight,
            BackBottomLeft, BackBottomRight, BackTopLeft, BackTopRight),
    );
    all(
        "types.CubeStyle",
        every!(pf_model::CubeStyle: VerticalFrontBack, VerticalLeftRight, HorizontalFrontBack, HorizontalLeftRight,
            StackedFrontBack, StackedLeftRight),
    );
    all(
        "types.StarStart",
        every!(pf_model::StarStart: Top, Bottom, LeftLeg, RightLeg),
    );
    all(
        "NonNullable<Extract<types.Generator, { type: 'tree' }>['style']>",
        every!(pf_model::TreeStyle: Round, Flat, Ribbon),
    );
    all(
        "Extract<types.ShapeSource, { source: 'measured' }>['provenance']",
        every!(pf_model::Provenance: CameraMap, Import, Manual),
    );
    all(
        "types.LineLayout",
        every!(pf_model::LineLayout: Horizontal, Vertical),
    );
    all(
        "types.BufferStyle",
        every!(pf_model::BufferStyle: Default, KeepXy, StackedStrands),
    );
    all(
        "types.Phoneme",
        every!(pf_model::Phoneme: Ai, E, Etc, Fv, L, Mbp, O, Rest, U, Wq),
    );
    all(
        "types.GroupLayout",
        every!(pf_model::GroupLayout: MinimalGrid, Grid, HorizontalPerModel, VerticalPerModel, HorizontalStack,
            VerticalStack, HorizontalStackScaled, VerticalStackScaled, SingleLine, OverlayCentered, OverlayScaled,
            SingleLineModelAsPixel, DefaultModelAsPixel, PerModelDefault),
    );
    all(
        "types.Issue['severity']",
        every!(pf_model::Severity: Warning, Error),
    );
    all(
        "types.RecentShow['status']",
        every!(recent::Presence: Here, Missing, Unknown),
    );

    // Sequences.
    all(
        "sequence.EffectKind",
        every!(pf_sequence::EffectKind: On, Off, ColorWash, Fade, Chase, Bars, Wave, Twinkle, Shimmer, Strobe, Spiral,
            Fire, Meteors, Ripple, Shape, Fan, Morph, Circles, Pinwheel, Snowflakes, Plasma, Butterfly, Garlands,
            Lines, Life, Tendril, Text, Faces, VuMeter, Impact, Wipe, Lightning, Pulse, Sing, ColorShift, Dancer,
            Picture),
    );
    all(
        "sequence.Blend",
        every!(pf_sequence::Blend: Normal, Add, Subtract, Max, Min, Multiply, Average, Over, Behind, Mask, Reveal,
            RevealBrightness, CutOut, Clip, ClipBrightness, Shadow, ShadowBelow, Highlight, HighlightAdd,
            BottomHalf, LeftHalf),
    );
    all(
        "sequence.CurveShape",
        every!(pf_sequence::CurveShape: Ramp, Sine, Square, Saw, Custom, Music, InvertedMusic, MusicTrigger,
            TimingToggle, TimingFade, TimingFadeSpan),
    );
    all(
        "sequence.RenderStyle",
        every!(pf_model::RenderStyle: Default, PerPreview, SingleLine, AsPixel, HorizontalPerModel, VerticalPerModel,
            HorizontalStack, VerticalStack, HorizontalStackScaled, VerticalStackScaled, OverlayCentered,
            OverlayScaled, SingleLineModelAsPixel, DefaultModelAsPixel, PerModelDefault, PerModelPerPreview,
            PerModelSingleLine),
    );
    all(
        "sequence.BufferTransform",
        every!(pf_model::BufferTransform: None, RotateCw90, RotateCcw90, Rotate180, FlipVertical, FlipHorizontal,
            RotateCw90FlipHorizontal, RotateCcw90FlipHorizontal),
    );
    all(
        "sequence.TimingKind",
        every!(pf_sequence::TimingKind: Beats, Bars, Sections, Lyrics, Words, Phonemes, Custom),
    );
    all(
        "sequence.RowBlending",
        every!(pf_sequence::RowBlending: Cover, Blend, Replace),
    );
    all(
        "sequence.SequenceIssue['severity']",
        every!(pf_model::Severity: Warning, Error),
    );

    // Music.
    all(
        "types.AudioInfo['foundBy']",
        every!(pf_audio::FoundBy: Header, Frames, Decoding),
    );
    all(
        "types.AudioTask",
        every!(progress::AudioTask: Probe, Waveform, AudioTrack, Beats, Separate, Align, Vocals),
    );
    all(
        "video.VideoStage",
        every!(pf_video::Stage: Rendering, Sound, Writing),
    );
    all(
        "sequence.DrumHit['drum']",
        every!(pf_analysis::Drum: Kick, Snare, Hat, Crash),
    );
    all(
        "sequence.AnalysisEvent['kind']",
        every!(pf_analysis::EventKind: Hit, Drop, Break, Build),
    );
    all(
        "sequence.MomentKind",
        every!(pf_analysis::MomentKind: Impact, Stop, Restart, Breakdown, Build, Fill, Peak, Hold, KeyChange, Shout,
            Drop, Crash, SectionChange),
    );
    all(
        "sequence.MomentSuggest",
        every!(pf_analysis::Suggest: Hit, Blackout, Burst, Minimal, Ramp, Chase, Full, Sustain, ColorShift, WordPop,
            Flash, Change),
    );
    all(
        "sequence.AnalysisSection['level']",
        every!(pf_analysis::Level: Low, Medium, High),
    );

    // Controllers.
    all(
        "types.DeviceKind",
        every!(pf_devices::DeviceKind: Fpp, Falcon, Wled),
    );
    all(
        "types.FoundBy",
        every!(pf_devices::FoundBy: Ping, WebSweep, Mdns, FppPeer, Manual),
    );
    all(
        "types.MatchReason",
        every!(pf_devices::MatchReason: SamePort, SameName, SameNameOtherSize),
    );
    all(
        "types.ChangeKind",
        every!(pf_devices::setup::ChangeKind: Pixels, ColorOrder, Start, StringAdded, StringRemoved, Receives,
            StartUniverse, UniverseSize, Setting),
    );
    all(
        "types.SendStatus",
        every!(pf_devices::adapter::SendStatus: Sent, Mismatch, Failed, Refused),
    );
    all(
        "types.PlayerState",
        every!(pf_devices::fpp_player::PlayerState: Idle, Playing, Paused, Stopping, Other),
    );
    all(
        "types.ScheduleKind",
        every!(pf_devices::fpp_info::ScheduleKind: Playlist, Sequence, Command),
    );

    // xLights imports.
    all(
        "types.VendorItemKind",
        every!(pf_xlights::vendor::ItemKind: Model, Group, Submodel, Strand),
    );
    all(
        "types.VendorPropType",
        every!(pf_xlights::vendor::PropType: Tree, Arch, Matrix, Canes, Line, Window, Star, Circle, Wreath, Spinner,
            Sphere, Cube, Icicles, Snowflake, Flood, Other),
    );
    all(
        "types.VendorMatchReason",
        every!(pf_xlights::vendor::MatchReason: Saved, Exact, Alias, Name, Type, Size, None),
    );

    // The assistant.
    all(
        "assistant.DiffSection",
        every!(pf_ai::Section: Show, Prop, Group, Controller, Playlist, Sequence, Row, Effect, TimingTrack),
    );
    all(
        "assistant.DiffAction",
        every!(pf_ai::Action: Added, Removed, Changed),
    );
    all(
        "assistant.KeyLocation",
        every!(pf_ai::KeyLocation: Keychain, Session),
    );
    all(
        "assistant.ProviderId",
        every!(pf_ai::ProviderId: Anthropic, Openai),
    );

    // Tagged unions: one value of each variant.
    use pf_engine::FileRole;
    let id = pf_model::SequenceId::new();
    let roles = vec![
        FileRole::Sequence { id },
        FileRole::Music { id },
        FileRole::Photo,
        FileRole::HouseModel,
        FileRole::SequenceDocMusic,
        FileRole::Picture,
    ];
    assert_every_variant(
        "file role",
        6,
        roles.iter().map(|r| match r {
            FileRole::Sequence { .. } => 0,
            FileRole::Music { .. } => 1,
            FileRole::Photo => 2,
            FileRole::HouseModel => 3,
            FileRole::SequenceDocMusic => 4,
            FileRole::Picture => 5,
        }),
    );
    fixtures.all_of("types.FileRole", Some("kind"), &wire(&roles));

    use pf_engine::TargetSpec;
    let targets: Vec<TargetSpec> = serde_json::from_value(json!([
        { "type": "show" },
        { "type": "prop", "id": prop_id(1) },
        { "type": "group", "id": "22222222-0000-4000-8000-000000000001" },
        { "type": "controller", "id": "33333333-0000-4000-8000-000000000001" },
        { "type": "port", "controller": "33333333-0000-4000-8000-000000000001", "port": 2 },
    ]))
    .unwrap();
    assert_every_variant(
        "test pattern target",
        5,
        targets.iter().map(|t| match t {
            TargetSpec::Show => 0,
            TargetSpec::Prop { .. } => 1,
            TargetSpec::Group { .. } => 2,
            TargetSpec::Controller { .. } => 3,
            TargetSpec::Port { .. } => 4,
        }),
    );
    fixtures.all_of("types.TargetSpec", Some("type"), &wire(&targets));

    use menu::MenuAction;
    let actions = vec![
        MenuAction::NewShow,
        MenuAction::OpenShow,
        MenuAction::OpenRecent {
            path: "/fixture/House.pixelflow.json".into(),
        },
        MenuAction::ClearRecent,
        MenuAction::CloseShow,
        MenuAction::Save,
        MenuAction::SaveAs,
        MenuAction::Undo,
        MenuAction::Redo,
    ];
    assert_every_variant(
        "menu action",
        9,
        actions.iter().map(|a| match a {
            MenuAction::NewShow => 0,
            MenuAction::OpenShow => 1,
            MenuAction::OpenRecent { .. } => 2,
            MenuAction::ClearRecent => 3,
            MenuAction::CloseShow => 4,
            MenuAction::Save => 5,
            MenuAction::SaveAs => 6,
            MenuAction::Undo => 7,
            MenuAction::Redo => 8,
        }),
    );
    fixtures.all_of("types.MenuAction", Some("action"), &wire(&actions));

    // The shapes, submodels, protocols, and channel addressing of the show of every shape
    // (which has one of each: see `show_replies`).
    let shapes: Vec<pf_model::Prop> = serde_json::from_value(Value::Array(every_shape())).unwrap();
    assert_every_variant("shape", SHAPES, shapes.iter().map(|p| shape_index(&p.shape)));
    let full = shell.ok("apply_edits", json!({ "edits": full_show_edits() }));
    let props = full["show"]["props"].as_array().unwrap();
    let generated: Vec<Value> = props
        .iter()
        .map(|p| p["shape"].clone())
        .filter(|s| s["source"] == "generator")
        .collect();
    fixtures.all_of(
        "Extract<types.ShapeSource, { source: 'generator' }>",
        Some("type"),
        &Value::Array(generated),
    );
    let sources: Vec<Value> = props.iter().map(|p| p["shape"].clone()).collect();
    fixtures.all_of(
        "types.ShapeSource",
        Some("source"),
        &Value::Array(sources[sources.len() - 2..].to_vec()),
    );
    let kinds = |region: &pf_model::Region| match region.kind {
        pf_model::RegionKind::Nodes { .. } => 0,
        pf_model::RegionKind::SubBuffer { .. } => 1,
        pf_model::RegionKind::Face(_) => 2,
    };
    assert_every_variant(
        "submodel",
        3,
        shapes.iter().flat_map(|p| p.regions.iter().map(kinds)),
    );
    let regions: Vec<Value> = props
        .iter()
        .flat_map(|p| p["regions"].as_array().unwrap().clone())
        .collect();
    fixtures.all_of("types.Region", Some("kind"), &Value::Array(regions));
    let controllers: Vec<pf_model::Controller> =
        serde_json::from_value(full["show"]["controllers"].clone()).unwrap();
    assert_every_variant(
        "protocol",
        2,
        controllers.iter().map(|c| match c.protocol {
            pf_model::Protocol::Sacn(_) => 0,
            pf_model::Protocol::Ddp => 1,
        }),
    );
    let protocols: Vec<Value> = full["show"]["controllers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["protocol"].clone())
        .collect();
    fixtures.all_of("types.Protocol", Some("type"), &Value::Array(protocols));
    let addressing: Vec<Value> = full["channelMap"]["controllers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["addressing"].clone())
        .collect();
    fixtures.all_of(
        "types.ControllerOutput['addressing']",
        Some("type"),
        &Value::Array(addressing),
    );

    // Every effect's settings at their defaults, and the three things a row can be on.
    let params: Vec<Value> = pf_sequence::EffectKind::ALL
        .iter()
        .map(|kind| {
            wire(&serde_json::from_value::<pf_sequence::EffectParams>(json!({ "kind": kind })).unwrap())
        })
        .collect();
    fixtures.all_of("sequence.EffectParams", Some("kind"), &Value::Array(params));
    use pf_sequence::Target;
    let on = vec![
        Target::Prop(pf_model::PropId::new()),
        Target::Group(pf_model::GroupId::new()),
        Target::Region {
            prop: pf_model::PropId::new(),
            region: pf_model::RegionId::new(),
        },
    ];
    assert_every_variant(
        "row target",
        3,
        on.iter().map(|t| match t {
            Target::Prop(_) => 0,
            Target::Group(_) => 1,
            Target::Region { .. } => 2,
        }),
    );
    fixtures.all_of_by_keys("sequence.SequenceTarget", &wire(&on));
    fixtures.finish();
}
