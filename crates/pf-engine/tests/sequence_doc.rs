//! Authoring a sequence document: edits with undo, files, live playback through the output plan
//! (captured in memory, with a silent clock), previews, and export.

use pf_engine::{ClockFactory, Edit, Engine, EngineError, SequenceEdit};
use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource};
use pf_output::{Recorded, RecordingTransport, Transport};
use pf_sequence::{Effect, EffectKind, Rgb, Row, Target};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ADDRESS: &str = "127.0.0.1:4048";

/// An engine with a 10-pixel strip wired to one DDP controller on loopback.
fn engine() -> (Engine, Recorded, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (transport, recorded) = RecordingTransport::new();
    let mut engine =
        Engine::new(dir.path()).with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn Transport>));
    let prop = Prop::new(
        "Strip",
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    );
    let mut controller = Controller::new("Bench", ADDRESS, Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(prop.id));
    controller.ports.push(port);
    engine
        .apply(vec![Edit::AddProp { prop }, Edit::AddController { controller }])
        .unwrap();
    (engine, recorded, dir)
}

/// Opens a new sequence with one row on the strip; returns the row id.
fn new_doc(engine: &mut Engine, duration_ms: u64) -> pf_sequence::RowId {
    engine.new_sequence_doc("Song", duration_ms).unwrap();
    let row = Row::new(Target::Prop(engine.show().props[0].id));
    let id = row.id;
    engine
        .edit_sequence(vec![SequenceEdit::AddRow { row, index: None }])
        .unwrap();
    id
}

fn on(color: Rgb, start: u64, end: u64) -> Effect {
    Effect::new(EffectKind::On, start, end).with_palette([color])
}

fn packets(recorded: &Recorded) -> Vec<Vec<u8>> {
    let dest: SocketAddr = ADDRESS.parse().unwrap();
    recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, to)| *to == dest)
        .map(|(p, _)| p.clone())
        .collect()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn solid(color: [u8; 3]) -> Vec<u8> {
    color.repeat(10)
}

#[test]
fn edits_undo_redo_and_files() {
    let (mut engine, _recorded, dir) = engine();
    assert!(matches!(
        engine.edit_sequence(vec![]),
        Err(EngineError::NoSequence)
    ));
    let row = new_doc(&mut engine, 10_000);
    let effect = on(Rgb::RED, 0, 1000);
    let id = effect.id;
    let snap = engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect,
        }])
        .unwrap();
    assert!(snap.dirty && snap.can_undo && !snap.can_redo && snap.changed);
    assert_eq!(snap.changes.effects.len(), 1, "the reply lists the new effect");
    assert_eq!(snap.changes.effects[0].effect.id, id);
    assert!(snap.changes.rows.is_empty());
    assert_eq!(engine.sequence_doc().unwrap().sequence.effect_count(), 1);
    assert!(snap.issues.is_empty(), "{:?}", snap.issues);

    let snap = engine.undo_sequence().unwrap();
    assert!(
        snap.changed && snap.can_undo && snap.can_redo,
        "the added row is still undoable"
    );
    assert_eq!(
        snap.changes.rows[0].layers[0].effects.len(),
        0,
        "undo sends the restored row"
    );
    assert_eq!(engine.sequence_doc().unwrap().sequence.effect_count(), 0);
    let snap = engine.redo_sequence().unwrap();
    assert_eq!(
        engine.sequence_doc().unwrap().sequence.effect(id).unwrap().kind(),
        EffectKind::On
    );
    let revision = snap.revision;
    let nothing = engine.redo_sequence().unwrap();
    assert!(!nothing.changed && nothing.revision == revision);

    assert!(matches!(
        engine.save_sequence_doc(),
        Err(EngineError::SequenceNoPath)
    ));
    let path = dir.path().join("song.pfseq.json");
    let saved = engine.save_sequence_doc_as(&path).unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.path.as_deref(), Some(path.to_str().unwrap()));

    engine.close_sequence_doc();
    assert!(engine.sequence_doc().is_none());
    let opened = engine.open_sequence_doc(&path).unwrap();
    assert_eq!(opened.sequence, saved.sequence);
    assert!(!opened.dirty && !opened.can_undo);
    assert!(
        opened.revision > revision,
        "revisions keep growing across documents"
    );

    // Issues are checked against the current show.
    let prop = engine.show().props[0].id;
    engine.apply(vec![Edit::RemoveProp { id: prop }]).unwrap();
    let issues = engine.sequence_doc().unwrap().issues;
    assert!(
        issues[0].message.contains("isn't in the show anymore"),
        "{issues:?}"
    );
}

#[test]
fn scrubbing_renders_any_moment_without_playing() {
    let (mut engine, _recorded, _dir) = engine();
    assert!(engine.sequence_doc_frame(0).is_none(), "no sequence open");
    let row = new_doc(&mut engine, 2000);
    engine
        .edit_sequence(vec![
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: on(Rgb::RED, 0, 1000),
            },
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: on(Rgb::BLUE, 1000, 2000),
            },
        ])
        .unwrap();
    assert_eq!(engine.sequence_doc_frame(500).unwrap(), solid([255, 0, 0]));
    assert_eq!(engine.sequence_doc_frame(1500).unwrap(), solid([0, 0, 255]));
    assert!(engine.playback_status().is_none());
}

#[test]
fn plays_through_the_output_plan_and_shows_edits_live() {
    let (mut engine, recorded, _dir) = engine();
    let row = new_doc(&mut engine, 20_000);
    let effect = on(Rgb::RED, 0, 20_000);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: effect.clone(),
        }])
        .unwrap();
    let status = engine.play_sequence_doc(0).unwrap();
    assert!(status.authored);
    assert_eq!(
        (status.state, status.duration_ms, status.frame_ms),
        ("playing", 20_000, 25)
    );
    assert!(status.notes.is_empty(), "{:?}", status.notes);
    assert_eq!(
        engine.live_frame().unwrap(),
        solid([255, 0, 0]),
        "the first frame is ready at once"
    );
    wait_until(|| {
        packets(&recorded)
            .iter()
            .any(|p| p[10..40] == solid([255, 0, 0])[..])
    });
    assert!(
        engine.sequence_frame().is_none(),
        "no raw sequence frame for a document"
    );

    // Edits reach the playing sequence without restarting it, even while paused.
    engine.set_playback_paused(true).unwrap();
    let generation = engine.playback_generation();
    let blue = Effect {
        palette: pf_sequence::Palette::new([Rgb::BLUE]),
        ..effect
    };
    engine
        .edit_sequence(vec![SequenceEdit::UpdateEffect { effect: blue }])
        .unwrap();
    wait_until(|| engine.live_frame().unwrap() == solid([0, 0, 255]));
    wait_until(|| packets(&recorded).last().unwrap()[10..40] == solid([0, 0, 255])[..]);
    assert_eq!(engine.playback_generation(), generation);
    assert_eq!(engine.playback_status().unwrap().state, "paused");

    engine.undo_sequence().unwrap();
    wait_until(|| engine.live_frame().unwrap() == solid([255, 0, 0]));

    // Moving the prop redraws without restarting; a new address restarts at the same spot.
    engine.seek_playback(5000).unwrap();
    let mut prop = engine.show().props[0].clone();
    prop.transform.position.x = 4.0;
    engine.apply(vec![Edit::UpdateProp { prop }]).unwrap();
    assert_eq!(engine.playback_generation(), generation);
    let mut controller = engine.show().controllers[0].clone();
    controller.address = "127.0.0.2:4048".into();
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    assert_eq!(engine.playback_generation(), generation + 1);
    let status = engine.playback_status().unwrap();
    assert_eq!(
        (status.state, status.position_ms, status.authored),
        ("paused", 5000, true)
    );

    engine.stop_playback();
    assert!(engine.playback_status().is_none());
}

#[test]
fn ends_dark_and_seeking_plays_again() {
    let (mut engine, recorded, _dir) = engine();
    let row = new_doc(&mut engine, 150);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::GREEN, 0, 150),
        }])
        .unwrap();
    engine.play_sequence_doc(0).unwrap();
    wait_until(|| engine.playback_status().unwrap().state == "ended");
    assert_eq!(engine.playback_status().unwrap().position_ms, 150);
    wait_until(|| packets(&recorded).last().unwrap()[10..].iter().all(|&b| b == 0));
    assert!(engine.live_frame().unwrap().iter().all(|&b| b == 0));
    engine.seek_playback(0).unwrap();
    assert_eq!(engine.playback_status().unwrap().state, "playing");
}

#[test]
fn a_show_with_errors_plays_only_in_the_preview() {
    let (mut engine, recorded, _dir) = engine();
    let row = new_doc(&mut engine, 10_000);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::WHITE, 0, 10_000),
        }])
        .unwrap();
    engine.apply(vec![Edit::SetFrameRate { fps: 5 }]).unwrap();
    let status = engine.play_sequence_doc(0).unwrap();
    assert!(
        status.notes[0].contains("only the preview plays"),
        "{:?}",
        status.notes
    );
    assert_eq!(engine.live_frame().unwrap(), solid([255, 255, 255]));
    std::thread::sleep(Duration::from_millis(60));
    assert!(packets(&recorded).is_empty(), "nothing sent");

    // Fixing the show restarts playback with output.
    engine.apply(vec![Edit::SetFrameRate { fps: 40 }]).unwrap();
    assert!(engine.playback_status().unwrap().notes.is_empty());
    wait_until(|| !packets(&recorded).is_empty());
}

#[test]
fn the_music_is_found_next_to_the_document_and_playback_follows_its_clock() {
    let (engine, _recorded, dir) = engine();
    let opened: Arc<Mutex<Vec<String>>> = Default::default();
    let log = opened.clone();
    let clocks: ClockFactory = Arc::new(move |music: Option<&Path>| {
        log.lock()
            .unwrap()
            .push(music.map_or("none".into(), |m| m.display().to_string()));
        Ok(Box::new(pf_audio::SilentClock::new()) as Box<dyn pf_audio::AudioClock>)
    });
    let mut engine = engine.with_clocks(clocks);
    new_doc(&mut engine, 10_000);
    let info = |audio: &str| SequenceEdit::UpdateInfo {
        name: "Song".into(),
        audio: Some(audio.into()),
        duration_ms: 10_000,
        frame_ms: 25,
    };
    engine.edit_sequence(vec![info("song.mp3")]).unwrap();
    let path = dir.path().join("shows").join("song.pfseq.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    engine.save_sequence_doc_as(&path).unwrap();
    let status = engine.play_sequence_doc(1000).unwrap();
    let expected = dir.path().join("shows").join("song.mp3");
    assert_eq!(status.music.as_deref(), Some(expected.as_path()));
    assert_eq!(
        opened.lock().unwrap().as_slice(),
        [expected.display().to_string()]
    );

    // New music restarts playback with it (opened on the player thread, without waiting).
    engine.edit_sequence(vec![info("/music/other.mp3")]).unwrap();
    wait_until(|| opened.lock().unwrap().last().unwrap() == "/music/other.mp3");
    assert!(engine.playback_status().unwrap().position_ms >= 1000);

    // A new address or frame time sends through a new output without reopening the music.
    let generation = engine.playback_generation();
    let mut controller = engine.show().controllers[0].clone();
    controller.address = "127.0.0.2:4048".into();
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    engine
        .edit_sequence(vec![SequenceEdit::UpdateInfo {
            name: "Song".into(),
            audio: Some("/music/other.mp3".into()),
            duration_ms: 10_000,
            frame_ms: 50,
        }])
        .unwrap();
    assert_eq!(engine.playback_generation(), generation + 2);
    wait_until(|| engine.playback_status().unwrap().frame_ms == 50);
    let status = engine.playback_status().unwrap();
    assert_eq!(status.state, "playing");
    assert!(status.position_ms >= 1000, "{status:?}");
    assert_eq!(opened.lock().unwrap().len(), 2, "{:?}", opened.lock().unwrap());

    // Opening another sequence stops the playing one.
    engine.new_sequence_doc("Next", 1000).unwrap();
    assert!(engine.playback_status().is_none());
}

#[test]
fn exports_the_open_sequence() {
    let (mut engine, _recorded, dir) = engine();
    let path = dir.path().join("song.fseq");
    assert!(matches!(
        engine.export_sequence_doc(&path),
        Err(EngineError::NoSequence)
    ));
    let row = new_doc(&mut engine, 1000);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::RED, 0, 500),
        }])
        .unwrap();
    let job = engine.sequence_export().unwrap();
    assert_eq!(job.layout().channels, 30);
    let summary = engine.export_sequence_doc(&path).unwrap();
    assert_eq!((summary.frames, summary.channels), (40, 30));
    let mut file = pf_fseq::Sequence::open(&path).unwrap();
    let mut frame = vec![0u8; 30];
    file.read_frame(0, &mut frame).unwrap();
    assert_eq!(frame, solid([255, 0, 0]));
    file.read_frame(30, &mut frame).unwrap();
    assert_eq!(frame, vec![0; 30]);
    assert!(summary.notes.is_empty(), "{:?}", summary.notes);

    // A show with errors still exports, saying so.
    engine.apply(vec![Edit::SetFrameRate { fps: 5 }]).unwrap();
    let summary = engine.export_sequence_doc(&path).unwrap();
    assert!(
        summary.notes[0].starts_with("The show has errors, so some props may be missing or wrong"),
        "{:?}",
        summary.notes
    );
    // A cancelled export says so and writes nothing.
    let cancelled = dir.path().join("cancelled.fseq");
    let err = engine
        .sequence_export()
        .unwrap()
        .run(&cancelled, |done, _| done < 3)
        .unwrap_err();
    assert_eq!(err.to_string(), "The export was cancelled.");
    assert!(!cancelled.exists());

    let empty = pf_model::Show::new("empty");
    engine.new_show("empty");
    assert_eq!(engine.show().name, empty.name);
    let err = engine.export_sequence_doc(&path).unwrap_err();
    assert!(err.to_string().contains("Wire your props"), "{err}");
}

#[test]
fn an_exported_file_plays_back_looking_like_the_document() {
    let (mut engine, _recorded, dir) = engine();
    let mut controller = engine.show().controllers[0].clone();
    controller.sequence_channels = Some(pf_model::SequenceChannels {
        start: 1,
        count: 30,
        raw_ddp_offsets: false,
    });
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    let row = new_doc(&mut engine, 2000);
    let rainbow = Effect::new(EffectKind::Chase, 0, 2000).with_palette([Rgb::RED, Rgb::GREEN, Rgb::BLUE]);
    let twinkle = Effect::new(EffectKind::Twinkle, 0, 2000);
    engine
        .edit_sequence(vec![
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: rainbow,
            },
            SequenceEdit::AddEffect {
                row,
                layer: 1,
                effect: twinkle,
            },
        ])
        .unwrap();
    let path = dir.path().join("song.fseq");
    engine.export_sequence_doc(&path).unwrap();
    engine.start_playback(&path, 0).unwrap();
    engine.set_playback_paused(true).unwrap();
    for ms in [0, 475, 1250, 1975] {
        engine.seek_playback(ms).unwrap();
        let expected = engine.sequence_doc_frame(ms).unwrap();
        wait_until(|| engine.live_frame().unwrap() == expected);
    }
}

#[test]
fn detected_timing_tracks_replace_earlier_ones_in_one_undo_step() {
    use pf_sequence::{Mark, TimingKind, TimingTrack};
    let (mut engine, _recorded, dir) = engine();
    assert!(matches!(
        engine.replace_timing_tracks(vec![]),
        Err(EngineError::NoSequence)
    ));
    new_doc(&mut engine, 10_000);
    assert_eq!(engine.sequence_music(), None);
    let lyrics = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![Mark::new(0, 900, "Ding")]);
    let beats = |n: u64| {
        TimingTrack::new(
            "Beats",
            TimingKind::Beats,
            (0..n).map(|i| Mark::new(i * 500, i * 500 + 500, "")).collect(),
        )
    };
    engine
        .replace_timing_tracks(vec![lyrics.clone(), beats(4)])
        .unwrap();
    let snap = engine.replace_timing_tracks(vec![beats(8)]).unwrap();
    assert_eq!(snap.changes.timing_tracks.len(), 1);
    assert_eq!(snap.changes.removed_timing_tracks.len(), 1);
    let doc = engine.sequence_doc().unwrap().sequence;
    let tracks: Vec<(&str, usize)> = doc
        .timing_tracks
        .iter()
        .map(|t| (t.name.as_str(), t.marks.len()))
        .collect();
    assert_eq!(tracks, vec![("Lyrics", 1), ("Beats", 8)]);
    engine.undo_sequence().unwrap();
    assert_eq!(
        engine.sequence_doc().unwrap().sequence.timing_tracks[1]
            .marks
            .len(),
        4,
        "one undo step"
    );

    engine
        .edit_sequence(vec![SequenceEdit::UpdateInfo {
            name: "Song".into(),
            audio: Some("song.mp3".into()),
            duration_ms: 10_000,
            frame_ms: 25,
        }])
        .unwrap();
    // Unsaved, relative music has no folder to be in (it isn't looked for where the app runs).
    assert_eq!(engine.sequence_music(), None);
    let path = dir.path().join("song.pfseq.json");
    engine.save_sequence_doc_as(&path).unwrap();
    assert_eq!(engine.sequence_music(), Some(dir.path().join("song.mp3")));
    // Saving somewhere else keeps the same music file.
    let elsewhere = dir.path().join("copies").join("song.pfseq.json");
    std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    let snap = engine.save_sequence_doc_as(&elsewhere).unwrap();
    assert_eq!(
        snap.sequence.audio,
        Some(dir.path().join("song.mp3").display().to_string())
    );
    assert_eq!(engine.sequence_music(), Some(dir.path().join("song.mp3")));
    assert!(!snap.dirty);
}

#[test]
fn a_layout_change_right_after_a_move_draws_with_the_newest_layout() {
    let (mut engine, _recorded, _dir) = engine();
    let row = new_doc(&mut engine, 20_000);
    // A second strip, not in the show yet, already has a green row.
    let second = Prop::new(
        "Second",
        ShapeSource::Generator(Generator::Line {
            nodes: 5,
            length: 1.0,
        }),
    );
    let second_row = Row::new(Target::Prop(second.id));
    let second_row_id = second_row.id;
    engine
        .edit_sequence(vec![
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: on(Rgb::RED, 0, 20_000),
            },
            SequenceEdit::AddRow {
                row: second_row,
                index: None,
            },
            SequenceEdit::AddEffect {
                row: second_row_id,
                layer: 0,
                effect: on(Rgb::GREEN, 0, 20_000),
            },
        ])
        .unwrap();
    for _ in 0..20 {
        engine.play_sequence_doc(1000).unwrap();
        engine.set_playback_paused(true).unwrap();
        // A move (new renderer queued for the player) and, at once, new wiring (a rebuild): the
        // queued renderer is older than the rebuild's and must not replace it.
        let mut prop = engine.show().props[0].clone();
        prop.transform.position.x += 1.0;
        engine.apply(vec![Edit::UpdateProp { prop }]).unwrap();
        let mut controller = engine.show().controllers[0].clone();
        controller.ports[0].slots.push(PortSlot::new(second.id));
        engine
            .apply(vec![
                Edit::AddProp { prop: second.clone() },
                Edit::UpdateController { controller },
            ])
            .unwrap();
        let mut expected = solid([255, 0, 0]);
        expected.extend([0, 255, 0].repeat(5));
        wait_until(|| engine.live_frame().unwrap() == expected);
        // Let any stale renderer arrive, then check again.
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(engine.live_frame().unwrap(), expected);
        engine.stop_playback();
        engine.undo();
        engine.undo();
    }
}
