//! Importing hand-written xLights sequences onto the sample show.

use pf_model::{Generator, Prop, ShapeSource, Show};
use pf_sequence::{
    Axis, BarsParams, Blend, ChaseParams, ColorWashParams, Direction, Effect, EffectParams, FaceColorSource,
    FaceEyes, FacesParams, FireParams, Gradient, Mark, MeteorDirection, MeteorsParams, OnParams, Rgb,
    RippleParams, Row, ShapeObject, ShimmerParams, SpiralParams, StrobeParams, Target, TimingKind,
    TwinkleParams, WaveParams,
};
use pf_xlights::sequence::{SequenceImport, build_sequence, parse_xsq};
use pf_xlights::{import_folder, import_sequence_file};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/sequences")
        .join(name)
}

/// The sample show (props Roofline, Arches, Mega Tree, Window Matrix, Porch Star, Candy Canes;
/// groups Outline and Everything).
fn show() -> Show {
    import_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-show"))
        .unwrap()
        .show
}

fn import(name: &str) -> SequenceImport {
    import_sequence_file(&fixture(name), &show(), |_, _| None).unwrap()
}

fn has_note(import: &SequenceImport, text: &str) -> bool {
    import.notes.iter().any(|n| n.contains(text))
}

#[track_caller]
fn assert_note(import: &SequenceImport, text: &str) {
    assert!(
        has_note(import, text),
        "no note containing {text:?} in {:#?}",
        import.notes
    );
}

/// The imported sequence opens like a saved file would (limits, schema).
#[track_caller]
fn assert_opens(import: &SequenceImport) {
    pf_sequence::check_sequence(&import.sequence).unwrap();
}

#[test]
fn reads_the_head_and_timing_tracks() {
    let i = import("timing.xsq");
    assert_opens(&i);
    let seq = &i.sequence;
    assert_eq!(seq.name, "Rock & Roll Christmas", "double-escaped song title");
    assert_eq!((seq.duration_ms, seq.frame_ms), (10_000, 25));
    assert_eq!(
        i.media_file.as_deref(),
        Some(r"C:\Users\someone\Music\rock-and-roll.mp3")
    );
    let names: Vec<(&str, TimingKind)> = seq
        .timing_tracks
        .iter()
        .map(|t| (t.name.as_str(), t.kind))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Lyrics", TimingKind::Lyrics),
            ("Lyrics (words)", TimingKind::Words),
            ("Lyrics (phonemes)", TimingKind::Phonemes),
            ("Half Seconds", TimingKind::Custom),
            ("Song Parts", TimingKind::Custom),
        ]
    );
    let lyrics = &seq.timing_tracks[0].marks;
    // 3010 ms rounds to the nearest 25 ms frame, as xLights does.
    assert_eq!(
        lyrics,
        &vec![
            Mark::new(1000, 2000, "Rock & roll"),
            Mark::new(2000, 3000, "all night")
        ]
    );
    assert_eq!(seq.timing_tracks[1].marks[1].label, "&");
    assert_eq!(seq.timing_tracks[2].marks.len(), 5);
    let fixed = &seq.timing_tracks[3].marks;
    assert_eq!(fixed.len(), 20);
    assert_eq!(fixed[0], Mark::new(0, 500, ""));
    assert_eq!(fixed[19], Mark::new(9500, 10_000, ""));
    // Zero-length marks are dropped, marks past the end are cut or dropped.
    let parts = &seq.timing_tracks[4].marks;
    assert_eq!(
        parts,
        &vec![Mark::new(0, 4000, "Intro"), Mark::new(4000, 10_000, "Verse \n1")]
    );
    assert_eq!(i.summary.timing_tracks, 5);
    assert_eq!(i.summary.marks, 2 + 3 + 5 + 20 + 2);
    assert_eq!(i.summary.lyric_marks, 10);
    assert_eq!(
        (i.summary.skipped, i.summary.marks_skipped),
        (0, 1),
        "marks are counted apart"
    );
    assert_note(
        &i,
        "1 timing mark ran past the end of the sequence and was cut off there.",
    );
    assert_note(&i, "1 timing mark started after the end of the sequence");
    assert_note(&i, "1 timing mark had no length");
}

#[test]
fn missing_music_is_reported_and_found_music_is_used() {
    let i = import("timing.xsq");
    assert_eq!(i.sequence.audio, None);
    assert_note(
        &i,
        "Couldn't find the music (rock-and-roll.mp3) near the sequence",
    );

    let found = import_sequence_file(&fixture("timing.xsq"), &show(), |path, media| {
        assert!(path.ends_with("timing.xsq"));
        assert_eq!(media, Some(r"C:\Users\someone\Music\rock-and-roll.mp3"));
        Some(PathBuf::from("/music/rock-and-roll.mp3"))
    })
    .unwrap();
    assert_eq!(found.sequence.audio.as_deref(), Some("/music/rock-and-roll.mp3"));
    assert!(!has_note(&found, "Couldn't find the music"));
}

#[test]
fn old_files_count_time_in_seconds() {
    let i = import("seconds.xsq");
    assert_opens(&i);
    let seq = &i.sequence;
    assert_eq!(seq.name, "seconds", "no song title: named after the file");
    assert_eq!((seq.duration_ms, seq.frame_ms), (8500, 50));
    let effects = &seq.rows[0].layers[0].effects;
    // 2.27 s rounds to the nearest 50 ms frame.
    let spans: Vec<(u64, u64)> = effects.iter().map(|e| (e.start_ms, e.end_ms)).collect();
    assert_eq!(spans, vec![(1500, 2250), (2300, 8500)]);
    assert_eq!(seq.timing_tracks[0].marks, vec![Mark::new(0, 500, "1")]);
    assert_eq!(i.sequence.audio, None);
    assert!(!has_note(&i, "music"), "animations have no music: {:#?}", i.notes);
}

#[test]
fn compressed_data_blocks_are_read() {
    let show = show();
    let i = import_sequence_file(&fixture("compressed.xsq"), &show, |_, _| None).unwrap();
    assert_opens(&i);
    let tree = show.props.iter().find(|p| p.name == "Mega Tree").unwrap();
    assert_eq!(i.sequence.rows.len(), 1);
    assert_eq!(i.sequence.rows[0].target, Target::Prop(tree.id));
    let wash = &i.sequence.rows[0].layers[0].effects[0];
    assert_eq!((wash.start_ms, wash.end_ms), (0, 2000));
    assert_eq!(wash.kind(), pf_sequence::EffectKind::ColorWash);
    assert_eq!(i.sequence.timing_tracks[0].marks.len(), 2);
    assert!(!has_note(&i, "couldn't be read"), "{:#?}", i.notes);
}

#[test]
fn hostile_input_is_refused_or_bounded_without_panicking() {
    let show = show();
    // Not XML, wrong root, unclosed.
    for xml in ["", "garbage", "<xrgb/>", "<xsequence>"] {
        assert!(parse_xsq(xml).is_err(), "{xml:?}");
    }
    // Deep nesting.
    let deep = format!(
        "<xsequence>{}{}</xsequence>",
        "<a>".repeat(20_000),
        "</a>".repeat(20_000)
    );
    let _ = parse_xsq(&deep).map(|f| build_sequence(&f, &show, "x"));
    // Absurd times, numbers, references, and frame times.
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><sequenceTiming>-5 ms</sequenceTiming><sequenceDuration>1e300</sequenceDuration></head>
      <ElementEffects>
        <Element type="model" name="Roofline"><EffectLayer>
          <Effect ref="99999999999999999999" name="Bars" palette="-1" startTime="NaN" endTime="inf"/>
          <Effect ref="1.5" name="On" palette="7" startTime="0" endTime="18446744073709551616000"/>
          <Effect name="Wave" startTime="-100" endTime="1000">E_SLIDER_Number_Waves=1e308,E_TEXTCTRL_Wave_Speed=-1e308</Effect>
        </EffectLayer></Element>
        <Element type="timing" name="T" fixed="0.0001"><EffectLayer/></Element>
        <Element type="timing" name="U" fixed="1e30"><EffectLayer/></Element>
      </ElementEffects>
    </xsequence>"#;
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show, "x");
    pf_sequence::check_sequence(&i.sequence).unwrap();
    assert_eq!(i.sequence.duration_ms, pf_sequence::MAX_DURATION_MS);
    assert_note(&i, "longer than 4 hours");
    assert_note(&i, "doesn't say how far apart its frames are");
    let marks: usize = i.sequence.timing_tracks.iter().map(|t| t.marks.len()).sum();
    assert!(marks <= pf_sequence::MAX_MARKS);
}

#[test]
fn a_fixed_track_never_exceeds_the_mark_limit() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><sequenceTiming>10 ms</sequenceTiming><sequenceDuration>14400</sequenceDuration></head>
      <ElementEffects><Element type="timing" name="Every frame" fixed="10"><EffectLayer/></Element></ElementEffects>
    </xsequence>"#;
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show(), "x");
    assert_eq!(i.sequence.timing_tracks[0].marks.len(), pf_sequence::MAX_MARKS);
    assert_note(&i, "more timing marks than PixelFlow's limit");
    pf_sequence::check_sequence(&i.sequence).unwrap();
}

/// The sample show plus a prop whose name needs escaping.
fn show_with_santa() -> Show {
    let mut show = show();
    show.props.push(Prop::new(
        "Santa's Sleigh & Reindeer",
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    ));
    show
}

fn row<'a>(i: &'a SequenceImport, show: &Show, name: &str) -> &'a Row {
    let target = show
        .props
        .iter()
        .find(|p| p.name == name)
        .map(|p| Target::Prop(p.id))
        .or_else(|| {
            show.groups
                .iter()
                .find(|g| g.name == name)
                .map(|g| Target::Group(g.id))
        })
        .unwrap();
    i.sequence
        .rows
        .iter()
        .find(|r| r.target == target)
        .unwrap_or_else(|| panic!("no row for {name}"))
}

fn params(effects: &[Effect]) -> Vec<EffectParams> {
    effects.iter().map(|e| e.params.clone()).collect()
}

const RED: Rgb = Rgb::RED;
const WHITE: Rgb = Rgb::WHITE;

#[test]
fn effects_translate_with_their_settings_palettes_blends_and_fades() {
    let show = show_with_santa();
    let i = import_sequence_file(&fixture("effects.xsq"), &show, |_, _| None).unwrap();
    assert_opens(&i);

    // xLights' first layer is on top, so it becomes PixelFlow's last.
    let roof = row(&i, &show, "Roofline");
    assert_eq!(roof.layers.len(), 2);
    assert_eq!(roof.layers[0].effects[0].kind(), pf_sequence::EffectKind::Off);
    let on = &roof.layers[1].effects[0];
    assert_eq!(
        on.params,
        EffectParams::On(OnParams {
            gradient: Gradient::None,
            start_level: 1.0,
            end_level: 0.0
        })
    );
    // The disabled green is left out of the palette.
    assert_eq!(on.palette.colors, vec![RED, Rgb::BLUE, WHITE]);
    assert_eq!((on.fade_in_ms, on.fade_out_ms), (500, 0));
    let wash = &roof.layers[1].effects[1];
    assert_eq!(
        wash.params,
        EffectParams::ColorWash(ColorWashParams {
            cycles: 1.0,
            gradient: Gradient::None
        })
    );
    assert_eq!(wash.palette.colors, vec![Rgb::new(128, 64, 0)], "50% brightness");

    let bars = &row(&i, &show, "Arches").layers[0].effects[0];
    assert_eq!(
        bars.params,
        EffectParams::Bars(BarsParams {
            count: 6,
            speed: 2.0,
            axis: Axis::Horizontal,
            direction: Direction::Reverse
        })
    );
    assert_eq!(bars.blend, Blend::Add);

    // Submodel layers become rows on the submodels, right after their model's row.
    let arches = show.props.iter().find(|p| p.name == "Arches").unwrap();
    let on_submodel = |name: &str| {
        let region = arches.regions.iter().find(|r| r.name == name).unwrap();
        let target = Target::Region {
            prop: arches.id,
            region: region.id,
        };
        let at = i.sequence.rows.iter().position(|r| r.target == target).unwrap();
        (at, &i.sequence.rows[at])
    };
    let model_at = i
        .sequence
        .rows
        .iter()
        .position(|r| r.target == Target::Prop(arches.id))
        .unwrap();
    let (at, arch_1) = on_submodel("Arch 1");
    assert_eq!(at, model_at + 1);
    assert_eq!(arch_1.layers.len(), 1);
    assert_eq!(arch_1.layers[0].effects[0].kind(), pf_sequence::EffectKind::On);
    let (at, tops) = on_submodel("Tops");
    assert_eq!(at, model_at + 2);
    // xLights' layer 1 is below its (empty) layer 0.
    assert_eq!(tops.layers.len(), 2);
    assert_eq!(
        tops.layers[0].effects[0].kind(),
        pf_sequence::EffectKind::ColorWash
    );
    assert!(tops.layers[1].effects.is_empty());

    let chase = &row(&i, &show, "Candy Canes").layers[0].effects[0];
    assert_eq!(
        chase.params,
        EffectParams::Chase(ChaseParams {
            speed: 3.0,
            width: 0.4,
            bands: 2,
            direction: Direction::Reverse,
            bounce: true,
            ..ChaseParams::default()
        })
    );

    let matrix = &row(&i, &show, "Window Matrix").layers[0].effects;
    let p = params(matrix);
    assert_eq!(
        p[..8],
        [
            EffectParams::Wave(WaveParams {
                cycles: 2.0,
                speed: 1.0,
                height: 0.8,
                thickness: 0.1,
                direction: Direction::Forward
            }),
            EffectParams::Twinkle(TwinkleParams {
                density: 0.2,
                rate: 1.0
            }),
            EffectParams::Shimmer(ShimmerParams {
                rate: 5.0,
                duty: 0.25
            }),
            EffectParams::Strobe(StrobeParams {
                rate: 10.0,
                density: 0.1
            }),
            EffectParams::Spiral(SpiralParams {
                count: 6,
                speed: 1.0,
                thickness: 0.4,
                twist: 1.5,
                direction: Direction::Reverse
            }),
            EffectParams::Fire(FireParams {
                height: 0.7,
                ..FireParams::default()
            }),
            EffectParams::Meteors(MeteorsParams {
                count: 8,
                speed: 2.0,
                length: 0.4,
                direction: MeteorDirection::Up
            }),
            EffectParams::Ripple(RippleParams {
                speed: 1.0,
                spacing: 0.25,
                ..RippleParams::default()
            }),
        ]
    );
    // 'expand' bars with a value curve, Average blending, and a Wipe transition.
    let expand = &matrix[8];
    assert_eq!(expand.blend, Blend::Average);
    assert_eq!(expand.fade_out_ms, 1000);
    // Its cycles curve leaves P1 and P2 unset: flat at 0, so the bars stand still.
    assert!(matches!(
        expand.params,
        EffectParams::Bars(BarsParams {
            axis: Axis::Vertical,
            speed: 0.0,
            ..
        })
    ));
    assert!(expand.curves.is_empty());
    assert!(matches!(matrix[9].params, EffectParams::Chase(_)), "marquee");

    let tree = &row(&i, &show, "Mega Tree").layers[0].effects;
    assert_eq!(tree.len(), 4, "Adjust and Random are left out");
    assert_eq!(
        tree[0].params,
        EffectParams::default_for(pf_sequence::EffectKind::Butterfly)
    );
    for faces in &tree[1..3] {
        assert_eq!(faces.params, EffectParams::Faces(FacesParams::default()));
    }
    // Text with no words: nothing to show, as in xLights.
    assert_eq!(
        tree[3].params,
        EffectParams::Text(pf_sequence::TextParams {
            text: String::new(),
            size: 10,
            ..Default::default()
        })
    );

    row(&i, &show, "Outline");
    row(&i, &show, "Santa's Sleigh & Reindeer");

    let s = i.summary;
    assert_eq!(s.rows, 9);
    assert_eq!(s.effects, 23);
    assert_eq!((s.exact, s.approximate, s.placeholders), (17, 6, 0));
    assert_eq!(
        s.skipped,
        1 + 3 + 2,
        "Adjust, effects on a missing submodel and a node, Garage Door"
    );

    assert_note(
        &i,
        "Text (1 effect) approximated: 'default' font shown in PixelFlow's pixel font.",
    );
    assert_note(
        &i,
        "weren't imported (a stand-in would light the prop wrongly): Adjust (1).",
    );
    assert_note(&i, "Bars (2 effects) approximated: bars have gaps between them, ");
    assert!(
        !has_note(&i, "layer blending"),
        "every layer method in the sample translates: {:#?}",
        i.notes
    );
    assert_note(&i, "'Wipe' transition shown as a fade (1)");
    assert!(!has_note(&i, "kept at one value"), "{:#?}", i.notes);
    assert!(!has_note(&i, "Butterfly"), "{:#?}", i.notes);
    assert_note(
        &i,
        "Meteors (1 effect) approximated: meteor count and speed approximated.",
    );
    assert_note(
        &i,
        "These xLights models aren't in the show, so their effects weren't imported: Garage Door (2 effects).",
    );
    assert_note(
        &i,
        "1 model in the sequence isn't in the show; it had no effects, so nothing was lost.",
    );
    assert_note(
        &i,
        "These submodels aren't in the show, so their effects weren't imported: Mega Tree/Star (2 effects).",
    );
    assert_note(
        &i,
        "PixelFlow doesn't import effects on strands or single nodes yet; these weren't imported: Mega Tree (1 effect).",
    );
    assert_note(&i, "1 effect is xLights' Random effect");
    assert!(!has_note(&i, "Wave"), "{:#?}", i.notes);
}

/// xLights' "Default" face is the model's first face in name order (its faces are a sorted
/// map), and a face renamed on import ("Singer (face)", beside a submodel called "Singer") is
/// still the one a Faces effect names.
#[test]
fn faces_effects_find_the_default_and_renamed_faces() {
    let mut show = show();
    let matrix = show.props.iter_mut().find(|p| p.name == "Window Matrix").unwrap();
    let singer = matrix.regions.iter_mut().find(|r| r.name == "Singer").unwrap();
    singer.name = "Singer (face)".into();
    let mut alto = singer.clone();
    alto.id = pf_model::RegionId::new();
    alto.name = "Alto".into();
    matrix.regions.push(alto);
    let i = import_sequence_file(&fixture("faces.xsq"), &show, |_, _| None).unwrap();
    let faces: Vec<String> = row(&i, &show, "Window Matrix").layers[0]
        .effects
        .iter()
        .map(|e| match &e.params {
            EffectParams::Faces(p) => p.face.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(faces, ["Singer (face)", "Alto", "Pictures", "Singer (face)"]);
}

#[test]
fn faces_effects_sing_the_lyric_tracks_phonemes() {
    let show = show();
    let i = import_sequence_file(&fixture("faces.xsq"), &show, |_, _| None).unwrap();
    assert_opens(&i);
    let phonemes = i
        .sequence
        .timing_tracks
        .iter()
        .find(|t| t.kind == TimingKind::Phonemes)
        .unwrap()
        .id;
    let effects = &row(&i, &show, "Window Matrix").layers[0].effects;
    assert_eq!(
        params(effects),
        vec![
            EffectParams::Faces(FacesParams {
                face: "Singer".into(),
                timing_track: Some(phonemes),
                eyes: FaceEyes::Open,
                colors: FaceColorSource::Face,
                outline: true,
            }),
            // "Default" is the model's first face by name, as xLights picks it.
            EffectParams::Faces(FacesParams {
                face: "Singer".into(),
                ..FacesParams::default()
            }),
            EffectParams::Faces(FacesParams {
                face: "Pictures".into(),
                timing_track: Some(phonemes),
                ..FacesParams::default()
            }),
            // A beat track has no lyrics; xLights keeps the mouth at rest, and so does PixelFlow.
            EffectParams::Faces(FacesParams {
                face: "Singer".into(),
                ..FacesParams::default()
            }),
        ]
    );
    assert_eq!((i.summary.exact, i.summary.approximate), (2, 2));
    assert_note(
        &i,
        "its timing track isn't in the sequence, so the mouth stays at rest",
    );
    assert_note(
        &i,
        "its timing track has no lyrics, so the mouth stays at rest, as in xLights",
    );
    assert_note(&i, "blinks at PixelFlow's usual pace");
    let issues = pf_sequence::validate_sequence(&i.sequence, &show);
    assert!(
        issues.iter().any(|p| p
            .message
            .ends_with("uses the face 'Pictures', but 'Window Matrix' has no face by that name.")),
        "{issues:#?}"
    );

    // At 1.2 s the face sings "AI": the AI mouth (pixels 1-4) in its red, the open eyes (61-62,
    // 79-80) green and the outline (21-40) yellow, as the face's own colors say.
    let (map, _) = pf_mapping::map_show(&show);
    let matrix = show.props.iter().find(|p| p.name == "Window Matrix").unwrap();
    let at = map
        .props
        .iter()
        .find(|p| p.prop == matrix.id)
        .unwrap()
        .frame_offset;
    let mut renderer = pf_render::Renderer::new(&show, &map);
    let mut frame = vec![0; renderer.frame_len()];
    renderer.render(&i.sequence, 1_200, &mut frame);
    let pixel_in = |frame: &[u8], n: usize| [frame[at + 3 * n], frame[at + 3 * n + 1], frame[at + 3 * n + 2]];
    let pixel = |n: usize| pixel_in(&frame, n);
    assert_eq!(pixel(0), [255, 0, 0]);
    assert_eq!(pixel(3), [255, 0, 0]);
    assert_eq!(pixel(4), [0, 0, 0], "the O mouth is dark");
    assert_eq!(pixel(60), [0, 255, 0]);
    assert_eq!(pixel(25), [255, 255, 0]);
    renderer.render(&i.sequence, 1_700, &mut frame);
    assert_eq!(
        (pixel_in(&frame, 0), pixel_in(&frame, 5)),
        ([0, 0, 0], [255, 255, 255]),
        "O, which has no color of its own"
    );
}

#[test]
fn elements_xlights_would_not_read_as_models_are_counted_with_a_note() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head>
      <ElementEffects>
        <Element type="timing" name="Roofline"><EffectLayer><Effect label="a" startTime="0" endTime="500"/></EffectLayer></Element>
        <Element type="model" name="Roofline"><EffectLayer><Effect name="On" startTime="0" endTime="500"/><Effect name="On" startTime="500" endTime="900"/></EffectLayer></Element>
        <Element type="view" name="Arches"><EffectLayer><Effect name="On" startTime="0" endTime="500"/></EffectLayer></Element>
      </ElementEffects>
    </xsequence>"#;
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show(), "x");
    assert!(i.sequence.rows.is_empty());
    assert_eq!(i.summary.skipped, 3);
    assert_note(
        &i,
        "These models have the same name as a timing track, so xLights reads them as that track and their effects weren't imported: Roofline (2 effects).",
    );
    assert_note(
        &i,
        "1 element in the sequence is neither a model nor a timing track, so it was left out (1 effect).",
    );
}

#[test]
fn only_nested_labelled_layers_are_lyrics_and_other_layers_keep_their_number() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head>
      <ElementEffects>
        <Element type="timing" name="Cues">
          <EffectLayer><Effect label="" startTime="0" endTime="1000"/></EffectLayer>
          <EffectLayer><Effect label="x" startTime="2000" endTime="3000"/></EffectLayer>
          <EffectLayer/>
          <EffectLayer><Effect label="y" startTime="0" endTime="500"/></EffectLayer>
        </Element>
        <Element type="timing" name="Song">
          <EffectLayer><Effect label="la la" startTime="0" endTime="1000"/></EffectLayer>
          <EffectLayer><Effect label="la" startTime="0" endTime="500"/><Effect label="la" startTime="500" endTime="1000"/></EffectLayer>
          <EffectLayer><Effect label="L" startTime="0" endTime="250"/></EffectLayer>
          <EffectLayer><Effect label="extra" startTime="0" endTime="250"/></EffectLayer>
        </Element>
      </ElementEffects>
    </xsequence>"#;
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show(), "x");
    let tracks: Vec<(&str, TimingKind)> = i
        .sequence
        .timing_tracks
        .iter()
        .map(|t| (t.name.as_str(), t.kind))
        .collect();
    assert_eq!(
        tracks,
        vec![
            ("Cues", TimingKind::Custom),
            ("Cues layer 2", TimingKind::Custom),
            ("Cues layer 4", TimingKind::Custom),
            ("Song", TimingKind::Lyrics),
            ("Song (words)", TimingKind::Words),
            ("Song (phonemes)", TimingKind::Phonemes),
            ("Song layer 4", TimingKind::Custom),
        ]
    );
    assert_eq!(i.summary.lyric_marks, 4);
}

#[test]
fn many_elements_import_quickly() {
    use std::fmt::Write;
    let mut xml = String::from(
        r#"<xsequence FixedPointTiming="1"><head><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head><ElementEffects>"#,
    );
    for n in 0..800 {
        write!(
            xml,
            r#"<Element type="timing" name="T{n}"><EffectLayer/></Element>"#
        )
        .unwrap();
    }
    for n in 0..40_000 {
        write!(
            xml,
            r#"<Element type="model" name="Missing {n}"><EffectLayer><Effect name="On" startTime="0" endTime="500"/></EffectLayer></Element>"#
        )
        .unwrap();
    }
    xml.push_str("</ElementEffects></xsequence>");
    let file = parse_xsq(&xml).unwrap();
    let started = std::time::Instant::now();
    let i = build_sequence(&file, &show(), "x");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(i.summary.skipped, 40_000);
    assert_note(&i, "Missing 0 (1 effect), Missing 1 (1 effect)");
    assert_note(&i, "and 39980 more");
}

/// Shapes firing on a timing track get that track (one with a single layer, as xLights
/// requires), fans from before xLights 2025.04 keep their radii in pixels, and sizes in pixels on
/// a group are noted.
#[test]
fn shapes_fire_on_their_timing_track_and_old_fans_measure_in_pixels() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><version>2024.19</version><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head>
      <ElementEffects>
        <Element type="timing" name="Beats">
          <EffectLayer><Effect label="" startTime="0" endTime="500"/><Effect label="" startTime="500" endTime="1000"/></EffectLayer>
        </Element>
        <Element type="timing" name="Song">
          <EffectLayer><Effect label="la la" startTime="0" endTime="1000"/></EffectLayer>
          <EffectLayer><Effect label="la" startTime="0" endTime="500"/></EffectLayer>
        </Element>
        <Element type="model" name="Window Matrix"><EffectLayer>
          <Effect name="Shape" startTime="0" endTime="2000">E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Beats,E_CHOICE_Shape_ObjectToDraw=Heart</Effect>
          <Effect name="Shape" startTime="2000" endTime="4000">E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Song</Effect>
          <Effect name="Shape" startTime="4000" endTime="6000">E_CHECKBOX_Shape_FireTiming=1,E_CHOICE_Shape_FireTimingTrack=Gone</Effect>
          <Effect name="Fan" startTime="6000" endTime="8000">E_SLIDER_Fan_End_Radius=12</Effect>
        </EffectLayer></Element>
        <Element type="model" name="Outline"><EffectLayer>
          <Effect name="Circles" startTime="0" endTime="2000">E_SLIDER_Circles_Size=4</Effect>
          <Effect name="Circles" startTime="2000" endTime="4000">E_CHECKBOX_Circles_Radial=1</Effect>
        </EffectLayer></Element>
      </ElementEffects>
    </xsequence>"#;
    let show = show();
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show, "x");
    assert_opens(&i);
    let beats = i
        .sequence
        .timing_tracks
        .iter()
        .find(|t| t.name == "Beats")
        .unwrap()
        .id;
    let effects = &row(&i, &show, "Window Matrix").layers[0].effects;
    let EffectParams::Shape(heart) = &effects[0].params else {
        panic!("{:?}", effects[0].params)
    };
    assert_eq!(
        (heart.shape, heart.timing_track),
        (ShapeObject::Heart, Some(beats))
    );
    for shape in &effects[1..3] {
        let EffectParams::Shape(p) = &shape.params else {
            panic!("{:?}", shape.params)
        };
        assert_eq!(p.timing_track, None);
    }
    assert_note(
        &i,
        "its timing track has more than one layer, so shapes are shown as a steady stream (1)",
    );
    assert_note(
        &i,
        "its timing track isn't in the sequence, so shapes are shown as a steady stream (1)",
    );
    let EffectParams::Fan(fan) = &effects[3].params else {
        panic!("{:?}", effects[3].params)
    };
    assert_eq!((fan.scale, fan.end_radius), (false, 12.0));
    assert!(!has_note(&i, "Fan"), "{:#?}", i.notes);
    // Groups draw on xLights' grid, so sizes in pixels on a group match.
    assert!(!has_note(&i, "Circles"), "{:#?}", i.notes);
}

/// VU Meters bring their timing tracks, and so do curves that follow a timing track.
#[test]
fn vu_meters_and_curves_find_their_timing_tracks() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><version>2024.19</version><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head>
      <ElementEffects>
        <Element type="timing" name="Beats">
          <EffectLayer><Effect label="kick" startTime="0" endTime="500"/><Effect label="snare" startTime="500" endTime="1000"/></EffectLayer>
        </Element>
        <Element type="model" name="Window Matrix"><EffectLayer>
          <Effect name="VU Meter" startTime="0" endTime="2000">E_CHOICE_VUMeter_Type=Timing Event Bar,E_CHOICE_VUMeter_TimingTrack=Beats,E_SLIDER_VUMeter_Bars=4,E_TEXTCTRL_Filter=snare</Effect>
          <Effect name="VU Meter" startTime="2000" endTime="4000">E_CHOICE_VUMeter_Type=Timing Event Pulse,E_CHOICE_VUMeter_TimingTrack=Gone</Effect>
          <Effect name="VU Meter" startTime="4000" endTime="6000">E_CHOICE_VUMeter_Type=Spectrogram Peak,E_SLIDER_VUMeter_Bars=16,E_CHECKBOX_VUMeter_LogarithmicX=1,E_SLIDER_VUMeter_StartNote=24</Effect>
          <Effect name="Twinkle" startTime="6000" endTime="8000">E_VALUECURVE_Twinkle_Count=Active=TRUE|Type=Timing Track Toggle|Min=2.00|Max=100.00|P1=10.00|P2=50.00|TT=Beats|RV=TRUE|</Effect>
        </EffectLayer></Element>
      </ElementEffects>
    </xsequence>"#;
    let show = show();
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show, "x");
    assert_opens(&i);
    let beats = i.sequence.timing_tracks[0].id;
    let effects = &row(&i, &show, "Window Matrix").layers[0].effects;
    let vu = |n: usize| match &effects[n].params {
        EffectParams::VuMeter(p) => p.clone(),
        other => panic!("{other:?}"),
    };
    let bar = vu(0);
    assert_eq!(
        (bar.meter, bar.timing_track, bar.bars, bar.filter.as_str()),
        (pf_sequence::VuMeterType::TimingEventBar, Some(beats), 4, "snare")
    );
    assert_eq!(vu(1).timing_track, None);
    assert_note(
        &i,
        "VU Meter (1 effect) approximated: its timing track isn't in the sequence",
    );
    let spectrogram = vu(2);
    assert_eq!(
        (
            spectrogram.meter,
            spectrogram.bars,
            spectrogram.log_x,
            spectrogram.start_note
        ),
        (pf_sequence::VuMeterType::SpectrogramPeak, 16, true, 24)
    );
    assert_eq!(effects[3].curves["density"].timing_track, Some(beats));
    assert_eq!(i.summary.placeholders, 0);
    assert_eq!(i.summary.exact, 3, "{:#?}", i.notes);
}

/// Lightning comes in as PixelFlow's Lightning, and a Single Strand chase on a timing track steps
/// on its marks.
#[test]
fn lightning_and_chases_on_a_timing_track() {
    let xml = r#"<xsequence FixedPointTiming="1">
      <head><version>2024.19</version><sequenceTiming>25 ms</sequenceTiming><sequenceDuration>10</sequenceDuration></head>
      <ElementEffects>
        <Element type="timing" name="Beats">
          <EffectLayer><Effect label="" startTime="0" endTime="500"/><Effect label="" startTime="500" endTime="1000"/></EffectLayer>
        </Element>
        <Element type="model" name="Window Matrix"><EffectLayer>
          <Effect name="Lightning" startTime="0" endTime="2000">E_SLIDER_Number_Bolts=12,E_CHECKBOX_ForkedLightning=1,E_SLIDER_Lightning_WIDTH=3,E_CHOICE_Lightning_Direction=Down</Effect>
          <Effect name="Single Strand" startTime="2000" endTime="4000">E_CHOICE_SingleStrand_TimingTrack=Beats,E_SLIDER_Number_Chases=2</Effect>
          <Effect name="Single Strand" startTime="4000" endTime="6000">E_CHOICE_SingleStrand_TimingTrack=Gone</Effect>
        </EffectLayer></Element>
      </ElementEffects>
    </xsequence>"#;
    let show = show();
    let i = build_sequence(&parse_xsq(xml).unwrap(), &show, "x");
    assert_opens(&i);
    let beats = i.sequence.timing_tracks[0].id;
    let effects = &row(&i, &show, "Window Matrix").layers[0].effects;
    let EffectParams::Lightning(bolt) = &effects[0].params else {
        panic!("{:?}", effects[0].params)
    };
    assert_eq!((bolt.segments, bolt.thickness, bolt.branches), (12, 3, 0.5));
    assert!(!bolt.flash_only);
    let chase = |n: usize| match &effects[n].params {
        EffectParams::Chase(p) => *p,
        other => panic!("{other:?}"),
    };
    assert_eq!((chase(1).timing_track, chase(1).bands), (Some(beats), 2));
    assert_eq!(chase(2).timing_track, None);
    assert_note(
        &i,
        "its timing track isn't in the sequence, so it chases at its speed",
    );
    assert_eq!(i.summary.placeholders, 0);
}

/// An xLights sequence with a Pictures effect per `file`, written into `folder`.
fn pictures_xsq(folder: &Path, files: &[&str]) -> PathBuf {
    let effects: String = files
        .iter()
        .map(|f| {
            format!(
                "    <Effect>E_CHOICE_Scaling=Scale Keep Aspect Ratio,E_TEXTCTRL_Pictures_Filename={f}</Effect>\n"
            )
        })
        .collect();
    let placed: String = (0..files.len())
        .map(|i| {
            format!(
                "        <Effect ref=\"{i}\" name=\"Pictures\" palette=\"0\" startTime=\"{}\" endTime=\"{}\"/>\n",
                i * 1000,
                (i + 1) * 1000
            )
        })
        .collect();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xsequence BaseChannel="0" ChanCtrlBasic="0" ChanCtrlColor="0" FixedPointTiming="1" ModelBlending="true">
  <head>
    <version>2025.01</version>
    <song>Pictures</song>
    <sequenceTiming>25 ms</sequenceTiming>
    <sequenceType>Animation</sequenceType>
    <sequenceDuration>10.000</sequenceDuration>
  </head>
  <ColorPalettes>
    <ColorPalette>C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1</ColorPalette>
  </ColorPalettes>
  <EffectDB>
{effects}  </EffectDB>
  <ElementEffects>
    <Element type="model" name="Window Matrix">
      <EffectLayer>
{placed}      </EffectLayer>
    </Element>
  </ElementEffects>
</xsequence>
"#
    );
    std::fs::create_dir_all(folder).unwrap();
    let path = folder.join("pics.xsq");
    std::fs::write(&path, xml).unwrap();
    path
}

#[test]
fn pictures_are_found_where_the_sequence_says_or_by_name_in_the_show_folder() {
    use pf_sequence::{PictureFit, PictureParams, PictureTiming};
    let dir = tempfile::tempdir().unwrap();
    let xlights = dir.path().join("xlights");
    let touch = |path: PathBuf| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"GIF89a").unwrap();
        path
    };
    // The show folder (the one with the layout), with its pictures in it and below it.
    touch(xlights.join("xlights_rgbeffects.xml"));
    let santa = touch(xlights.join("Images/santa dancing.gif"));
    let star = touch(xlights.join("star.png"));
    let elsewhere = touch(dir.path().join("Art/elf.gif"));
    let elsewhere_text = pf_model::path_to_text(&elsewhere);
    // The sequence is two folders down, as xLights users often keep them.
    let xsq = pictures_xsq(
        &xlights.join("Sequences/2025"),
        &[
            // From another computer: found under the show folder by the end of its path.
            r"C:\Users\someone\Documents\xlights\Images\santa dancing.gif",
            // Somewhere that isn't there: found in the show folder by its name.
            "/Volumes/Old Disk/shows/star.png",
            // Where it says: used as it is.
            &elsewhere_text,
            // Nowhere: kept as written, and named in a note (once, however often it's used).
            r"D:\Art\gone.gif",
            r"D:\Art\gone.gif",
            "",
        ],
    );
    let i = import_sequence_file(&xsq, &show(), |_, _| None).unwrap();
    assert_opens(&i);
    let pictures: Vec<&PictureParams> = i.sequence.rows[0].layers[0]
        .effects
        .iter()
        .map(|e| match &e.params {
            EffectParams::Picture(p) => p,
            other => panic!("{other:?}"),
        })
        .collect();
    let files: Vec<&str> = pictures.iter().map(|p| p.file.as_str()).collect();
    assert_eq!(
        files,
        [
            pf_model::path_to_text(&santa).as_str(),
            pf_model::path_to_text(&star).as_str(),
            elsewhere_text.as_str(),
            r"D:\Art\gone.gif",
            r"D:\Art\gone.gif",
            "",
        ]
    );
    assert!(
        pictures
            .iter()
            .all(|p| p.fit == PictureFit::Fit && p.timing == PictureTiming::Stretch)
    );
    assert_note(
        &i,
        "Couldn't find a picture near the sequence (gone.gif); choose it in the Picture effects' settings.",
    );
    // They're real Picture effects now, not dim stand-ins, and none is approximate.
    assert_eq!(
        (i.summary.placeholders, i.summary.approximate, i.summary.exact),
        (0, 0, 6),
        "{:#?}",
        i.notes
    );
    assert!(!has_note(&i, "no matching effect"));

    // Without the layout to say where the show folder ends, the folders just above are tried.
    std::fs::remove_file(xlights.join("xlights_rgbeffects.xml")).unwrap();
    let i = import_sequence_file(&xsq, &show(), |_, _| None).unwrap();
    let EffectParams::Picture(p) = &i.sequence.rows[0].layers[0].effects[0].params else {
        unreachable!()
    };
    assert_eq!(p.file, pf_model::path_to_text(&santa));
}
