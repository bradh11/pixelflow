//! Exporting sequences to `.fseq` and reading them back.

use pf_model::{
    ColorOrder, Controller, Generator, Port, PortSlot, Prop, Protocol, SequenceChannels, ShapeSource, Show,
};
use pf_render::export::{ExportError, export_fseq, export_fseq_file};
use pf_render::{AudioSource, Renderer};
use pf_sequence::*;
use std::io::Cursor;

fn strip(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    )
}

/// Prop A (3 px, RGB) and prop B (2 px, GRB, wired in reverse at half brightness) on one DDP
/// controller whose sequence channels start at 7.
fn show() -> Show {
    let a = strip("A", 3);
    let mut b = strip("B", 2);
    b.color_order = ColorOrder::Grb;
    let mut controller = Controller::new("Falcon", "192.0.2.10", Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(a.id));
    let mut slot = PortSlot::new(b.id);
    slot.reverse = true;
    slot.brightness = Some(50);
    port.slots.push(slot);
    controller.ports.push(port);
    controller.sequence_channels = Some(SequenceChannels {
        start: 7,
        count: 15,
        raw_ddp_offsets: false,
    });
    let mut show = Show::new("t");
    show.props = vec![a, b];
    show.controllers.push(controller);
    show
}

/// A: red for the first 100 ms, then off. B: a horizontal ramp red→blue.
fn sequence(show: &Show) -> Sequence {
    let mut seq = Sequence::new("Song", 200);
    seq.frame_ms = 50;
    seq.audio = Some("/Users/me/Music/Carol of the Bells.mp3".into());
    let mut a = Row::new(Target::Prop(show.props[0].id));
    a.layers[0]
        .effects
        .push(Effect::new(EffectKind::On, 0, 100).with_palette([Rgb::RED]));
    let mut b = Row::new(Target::Prop(show.props[1].id));
    b.layers[0].effects.push(
        Effect::new(EffectKind::On, 0, 200)
            .with_palette([Rgb::RED, Rgb::BLUE])
            .with_params(EffectParams::On(OnParams {
                gradient: Gradient::Horizontal,
                ..OnParams::default()
            })),
    );
    seq.rows = vec![a, b];
    seq
}

#[test]
fn exports_controller_channels_into_their_sequence_block() {
    let show = show();
    let seq = sequence(&show);
    let (map, _) = pf_mapping::map_show(&show);
    let mut calls = Vec::new();
    let (out, summary) = export_fseq(
        &show,
        &map,
        &seq,
        &AudioSource::none(),
        Cursor::new(Vec::new()),
        7,
        |done, total| {
            calls.push((done, total));
            true
        },
    )
    .unwrap();
    assert_eq!(calls, vec![(1, 4), (2, 4), (3, 4), (4, 4)]);
    assert_eq!((summary.frames, summary.frame_ms, summary.channels), (4, 50, 21));
    assert_eq!(summary.media.as_deref(), Some("Carol of the Bells.mp3"));
    assert!(summary.notes.is_empty(), "{:?}", summary.notes);

    let mut file = pf_fseq::Sequence::from_reader(Cursor::new(out.into_inner())).unwrap();
    let header = file.header().clone();
    assert_eq!((header.channels, header.frames, header.step_ms), (21, 4, 50));
    assert_eq!(header.media.as_deref(), Some("Carol of the Bells.mp3"));
    assert!(header.producer.unwrap().starts_with("PixelFlow "));

    let half = pf_output::build_lut(50, 1.0)[255];
    let mut frame = vec![0u8; 21];
    file.read_frame(0, &mut frame).unwrap();
    // Channels 1–6 belong to no controller; A starts at channel 7 (index 6).
    assert_eq!(&frame[..6], &[0; 6]);
    assert_eq!(&frame[6..15], &[255, 0, 0, 255, 0, 0, 255, 0, 0]);
    // B is reversed: its blue (right) pixel goes out first, in G R B order, at half brightness.
    assert_eq!(&frame[15..21], &[0, 0, half, 0, half, 0]);
    file.read_frame(2, &mut frame).unwrap();
    assert_eq!(&frame[6..15], &[0; 9], "A's effect ended at 100 ms");
    assert_eq!(&frame[15..21], &[0, 0, half, 0, half, 0]);
}

#[test]
fn every_frame_matches_the_renderer_when_brightness_is_full() {
    let mut show = show();
    show.controllers[0].ports[0].slots[1].brightness = None;
    show.controllers[0].ports[0].slots[1].reverse = false;
    show.props[1].color_order = ColorOrder::Rgb;
    let mut seq = sequence(&show);
    seq.rows[0].layers[0].effects = vec![Effect::new(EffectKind::Twinkle, 0, 200)];
    let (map, _) = pf_mapping::map_show(&show);
    let (out, _) = export_fseq(
        &show,
        &map,
        &seq,
        &AudioSource::none(),
        Cursor::new(Vec::new()),
        0,
        |_, _| true,
    )
    .unwrap();
    let mut file = pf_fseq::Sequence::from_reader(Cursor::new(out.into_inner())).unwrap();
    let mut renderer = Renderer::new(&show, &map);
    let mut expected = vec![0u8; renderer.frame_len()];
    let mut frame = vec![0u8; 21];
    for i in 0..4 {
        renderer.render_frame(&seq, u64::from(i), &mut expected);
        file.read_frame(i, &mut frame).unwrap();
        assert_eq!(&frame[6..21], &expected[..], "frame {i}");
    }
}

#[test]
fn exports_with_music_match_the_preview() {
    let mut show = show();
    show.controllers[0].ports[0].slots[1].brightness = None;
    show.controllers[0].ports[0].slots[1].reverse = false;
    show.props[1].color_order = ColorOrder::Rgb;
    let mut seq = sequence(&show);
    seq.duration_ms = 2000;
    // A quiet half second, a loud one, and quiet again.
    let samples: Vec<f32> = (0..2 * 44_100)
        .map(|i| {
            let loud = (22_050..66_150).contains(&i);
            (if loud { 0.9 } else { 0.1 }) * (std::f32::consts::TAU * 330.0 * i as f32 / 44_100.0).sin()
        })
        .collect();
    let track = std::sync::Arc::new(pf_analysis::audio_track(samples, 44_100, seq.frame_ms));
    let audio = AudioSource::ready(track);
    let mut on = Effect::new(EffectKind::On, 0, 2000).with_palette([Rgb::WHITE]);
    on.curves
        .insert("startLevel".into(), Curve::music(0.0, 1.0, 0.0, false));
    on.curves
        .insert("endLevel".into(), Curve::music(0.0, 1.0, 0.0, false));
    let mut vu = Effect::new(EffectKind::VuMeter, 0, 2000).with_palette([Rgb::RED, Rgb::BLUE]);
    vu.params = EffectParams::VuMeter(VuMeterParams {
        meter: VuMeterType::LevelPulse,
        sensitivity: 50,
        ..VuMeterParams::default()
    });
    seq.rows[0].layers[0].effects = vec![on];
    seq.rows[1].layers[0].effects = vec![vu];
    let (map, _) = pf_mapping::map_show(&show);
    let (out, _) = export_fseq(&show, &map, &seq, &audio, Cursor::new(Vec::new()), 0, |_, _| true).unwrap();
    let mut file = pf_fseq::Sequence::from_reader(Cursor::new(out.into_inner())).unwrap();
    let mut renderer = Renderer::new(&show, &map);
    renderer.set_audio(audio);
    let mut expected = vec![0u8; renderer.frame_len()];
    let mut frame = vec![0u8; 21];
    let mut levels = Vec::new();
    for i in 0..40 {
        // Scrubbed out of order in the preview.
        let i = (i * 7) % 40;
        renderer.render_frame(&seq, u64::from(i), &mut expected);
        file.read_frame(i, &mut frame).unwrap();
        assert_eq!(&frame[6..21], &expected[..], "frame {i}");
        levels.push((i, frame[6]));
    }
    levels.sort();
    // The music shows: dim, bright, dim.
    assert!(
        levels[5].1 < 40 && levels[20].1 > 200 && levels[35].1 < 40,
        "{levels:?}"
    );
}

#[test]
fn file_exports_are_atomic_and_errors_are_plain() {
    let show = show();
    let seq = sequence(&show);
    let (map, _) = pf_mapping::map_show(&show);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Song.fseq");
    let summary = export_fseq_file(&show, &map, &seq, &AudioSource::none(), &path, |_, _| true).unwrap();
    assert_eq!(summary.frames, 4);
    let file = pf_fseq::Sequence::open(&path).unwrap();
    assert_eq!(file.header().frames, 4);
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["Song.fseq"], "no temporary files left behind");

    let missing = dir.path().join("no/such/folder/Song.fseq");
    let err = export_fseq_file(&show, &map, &seq, &AudioSource::none(), &missing, |_, _| true).unwrap_err();
    assert!(matches!(err, ExportError::Write { .. }));
    assert!(err.to_string().starts_with("Could not save"), "{err}");

    let empty = Show::new("empty");
    let (empty_map, _) = pf_mapping::map_show(&empty);
    let err =
        export_fseq_file(&empty, &empty_map, &seq, &AudioSource::none(), &path, |_, _| true).unwrap_err();
    assert!(
        err.to_string().contains("Wire your props to a controller"),
        "{err}"
    );

    let mut short = seq.clone();
    short.duration_ms = 0;
    let err = export_fseq_file(&show, &map, &short, &AudioSource::none(), &path, |_, _| true).unwrap_err();
    assert!(matches!(err, ExportError::Empty));
    assert_eq!(
        pf_fseq::Sequence::open(&path).unwrap().header().frames,
        4,
        "the old file is untouched"
    );
}

#[test]
fn an_export_can_be_cancelled_and_leaves_nothing_behind() {
    let show = show();
    let seq = sequence(&show);
    let (map, _) = pf_mapping::map_show(&show);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Song.fseq");
    let mut seen = Vec::new();
    let err = export_fseq_file(&show, &map, &seq, &AudioSource::none(), &path, |done, _| {
        seen.push(done);
        done < 2
    })
    .unwrap_err();
    assert!(matches!(err, ExportError::Cancelled));
    assert_eq!(err.to_string(), "The export was cancelled.");
    assert_eq!(seen, vec![1, 2], "no frames are rendered after cancelling");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "no file, no temporary file"
    );
}

#[test]
fn exports_to_the_same_file_at_once_each_use_their_own_temporary_file() {
    let show = show();
    let seq = sequence(&show);
    let (map, _) = pf_mapping::map_show(&show);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Song.fseq");
    let results: Vec<_> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| export_fseq_file(&show, &map, &seq, &AudioSource::none(), &path, |_, _| true))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(pf_fseq::Sequence::open(&path).unwrap().header().frames, 4);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no temporary files left"
    );
}
