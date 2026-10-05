//! Importing hand-written xLights sequences onto the sample show.

use pf_model::{Generator, Prop, ShapeSource, Show};
use pf_sequence::{
    Axis, BarsParams, Blend, ChaseParams, ColorWashParams, Direction, Effect, EffectParams, FireParams,
    Gradient, Mark, MeteorDirection, MeteorsParams, OnParams, Rgb, RippleParams, Row, ShimmerParams,
    SpiralParams, StrobeParams, Target, TimingKind, TwinkleParams, WaveParams,
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
            bounce: true
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
    assert_eq!(expand.blend, Blend::Normal);
    assert_eq!(expand.fade_out_ms, 1000);
    assert!(matches!(
        expand.params,
        EffectParams::Bars(BarsParams {
            axis: Axis::Vertical,
            ..
        })
    ));
    assert!(matches!(matrix[9].params, EffectParams::Chase(_)), "marquee");

    let tree = &row(&i, &show, "Mega Tree").layers[0].effects;
    assert_eq!(tree.len(), 4, "Adjust and Random are left out");
    assert_eq!(tree[0].kind(), pf_sequence::EffectKind::ColorWash, "Butterfly");
    for placeholder in &tree[1..] {
        assert_eq!(
            placeholder.params,
            EffectParams::On(OnParams {
                gradient: Gradient::None,
                start_level: 0.25,
                end_level: 0.25
            })
        );
        assert_eq!(placeholder.palette.colors, vec![RED]);
    }

    row(&i, &show, "Outline");
    row(&i, &show, "Santa's Sleigh & Reindeer");

    let s = i.summary;
    assert_eq!(s.rows, 9);
    assert_eq!(s.effects, 23);
    assert_eq!((s.exact, s.approximate, s.placeholders), (14, 6, 3));
    assert_eq!(
        s.skipped,
        1 + 3 + 2,
        "Adjust, effects on a missing submodel and a node, Garage Door"
    );

    assert_note(
        &i,
        "PixelFlow has no matching effect yet for these xLights effects, so they are shown as a dim fill in each one's first color: Faces (2), Text (1).",
    );
    assert_note(
        &i,
        "weren't imported (a stand-in would light the prop wrongly): Adjust (1).",
    );
    assert_note(&i, "Bars (2 effects) approximated: bars have gaps between them, ");
    assert_note(&i, "'Average' layer blending shown as Normal (1)");
    assert_note(&i, "'Wipe' transition shown as a fade (1)");
    assert_note(&i, "settings that change over the effect kept at one value (1)");
    assert_note(&i, "Butterfly (1 effect) approximated: shown as a color wash.");
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
