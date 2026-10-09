//! Authoring a sequence document: edits with undo, files, live playback through the output plan
//! (captured in memory, with a silent clock), previews, and export.

use pf_audio::AudioClock;
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
    engine.new_sequence_doc("Song", duration_ms, None).unwrap();
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
        Ok(Box::new(pf_audio::SilentClock::new()) as Box<dyn AudioClock>)
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
    engine.new_sequence_doc("Next", 1000, None).unwrap();
    assert!(engine.playback_status().is_none());
}

#[test]
fn an_imported_sequence_opens_unsaved_and_oversized_ones_are_refused() {
    let (mut engine, _recorded, dir) = engine();
    let mut doc = pf_sequence::Sequence::new("Imported", 5000);
    let mut row = Row::new(Target::Prop(engine.show().props[0].id));
    row.layers[0].effects.push(on(Rgb::RED, 0, 1000));
    doc.rows.push(row);
    let snapshot = engine.adopt_sequence_doc(doc.clone()).unwrap();
    assert!(snapshot.dirty, "an import has unsaved changes");
    assert_eq!(snapshot.path, None);
    assert!(!snapshot.can_undo);
    assert_eq!(snapshot.sequence, doc);
    let path = dir.path().join("imported.pfseq.json");
    assert!(!engine.save_sequence_doc_as(&path).unwrap().dirty);

    let mut huge = doc;
    huge.duration_ms = pf_sequence::MAX_DURATION_MS + 1;
    assert!(matches!(
        engine.adopt_sequence_doc(huge),
        Err(EngineError::TooLarge(_))
    ));
    assert_eq!(
        engine.sequence_doc().unwrap().path.as_deref(),
        Some(path.to_str().unwrap()),
        "a refused import leaves the open sequence alone"
    );
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

    // The export joins the show's sequences, named after the sequence, with its music.
    let saved = dir.path().join("song.pfseq.json");
    let mut info = engine.sequence_doc().unwrap().sequence;
    info.audio = Some("song.mp3".into());
    engine
        .edit_sequence(vec![SequenceEdit::UpdateInfo {
            name: info.name,
            audio: info.audio,
            duration_ms: info.duration_ms,
            frame_ms: info.frame_ms,
        }])
        .unwrap();
    engine.save_sequence_doc_as(&saved).unwrap();
    let snapshot = engine.add_sequence_doc_to_show(&path).unwrap();
    let entry = &snapshot.show.sequences[0];
    assert_eq!(entry.name, "Song");
    assert_eq!(entry.path, path.display().to_string());
    assert_eq!(
        entry.audio,
        Some(dir.path().join("song.mp3").display().to_string())
    );
    assert!(snapshot.can_undo);
    // Adding the same file again updates its entry rather than listing it twice.
    let id = entry.id;
    engine
        .edit_sequence(vec![SequenceEdit::UpdateInfo {
            name: "Song v2".into(),
            audio: None,
            duration_ms: 1000,
            frame_ms: 25,
        }])
        .unwrap();
    let again = engine.add_sequence_doc_to_show(&path).unwrap();
    assert_eq!(again.show.sequences.len(), 1);
    assert_eq!(again.show.sequences[0].id, id);
    assert_eq!(again.show.sequences[0].name, "Song v2");
    assert_eq!(again.show.sequences[0].audio, None);
    // Another file with the same name gets a number.
    let other = engine
        .add_sequence_doc_to_show(&dir.path().join("other.fseq"))
        .unwrap();
    assert_eq!(other.show.sequences[1].name, "Song v2 (2)");

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
fn an_export_can_name_the_music_as_it_will_be_on_the_fpp() {
    let (mut engine, _recorded, dir) = engine();
    new_doc(&mut engine, 1000);
    let mut info = engine.sequence_doc().unwrap().sequence;
    info.audio = Some("/music/Rock'n Song.mp3".into());
    engine
        .edit_sequence(vec![SequenceEdit::UpdateInfo {
            name: info.name,
            audio: info.audio.clone(),
            duration_ms: info.duration_ms,
            frame_ms: info.frame_ms,
        }])
        .unwrap();
    let path = dir.path().join("send.fseq");
    let summary = engine
        .sequence_export()
        .unwrap()
        .with_music_named(Some("Rockn Song (2).mp3"))
        .run(&path, |_, _| true)
        .unwrap();
    assert_eq!(summary.media.as_deref(), Some("Rockn Song (2).mp3"));
    let file = pf_fseq::Sequence::open(&path).unwrap();
    assert_eq!(file.header().media.as_deref(), Some("Rockn Song (2).mp3"));

    let none = engine
        .sequence_export()
        .unwrap()
        .with_music_named(None)
        .run(&path, |_, _| true)
        .unwrap();
    assert_eq!(none.media, None);
    // The open sequence keeps its own music.
    assert_eq!(engine.sequence_doc().unwrap().sequence.audio, info.audio);
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
fn imported_timing_tracks_are_added_with_their_own_names_in_one_undo_step() {
    use pf_sequence::{Mark, TimingKind, TimingTrack};
    let (mut engine, _recorded, _dir) = engine();
    assert!(engine.sequence_document().is_none());
    assert!(matches!(
        engine.add_timing_tracks(vec![]),
        Err(EngineError::NoSequence)
    ));
    new_doc(&mut engine, 10_000);
    let lyrics = || TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![Mark::new(0, 900, "Ding")]);
    engine.add_timing_tracks(vec![lyrics()]).unwrap();
    let reply = engine.add_timing_tracks(vec![lyrics(), lyrics()]).unwrap();
    assert_eq!(reply.changes.timing_tracks.len(), 2);
    let names: Vec<&str> = engine
        .sequence_document()
        .unwrap()
        .timing_tracks
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, vec!["Lyrics", "Lyrics 2", "Lyrics 3"]);
    engine.undo_sequence().unwrap();
    assert_eq!(engine.sequence_document().unwrap().timing_tracks.len(), 1);
    // A lyrics timing's layers keep their pairing: the number goes on the shared name.
    let layer = |name: &str, kind| TimingTrack::new(name, kind, vec![Mark::new(0, 900, "Ding")]);
    engine
        .add_timing_tracks(vec![
            layer("Lyrics", TimingKind::Lyrics),
            layer("Lyrics (words)", TimingKind::Words),
            layer("Lyrics (phonemes)", TimingKind::Phonemes),
        ])
        .unwrap();
    engine
        .add_timing_tracks(vec![layer("Lyrics (words)", TimingKind::Words)])
        .unwrap();
    let names: Vec<&str> = engine
        .sequence_document()
        .unwrap()
        .timing_tracks
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Lyrics",
            "Lyrics 2",
            "Lyrics 2 (words)",
            "Lyrics 2 (phonemes)",
            "Lyrics 3 (words)"
        ]
    );
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

#[test]
fn sending_to_controllers_can_be_turned_off_while_editing() {
    let (mut engine, recorded, _dir) = engine();
    let row = new_doc(&mut engine, 10_000);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::GREEN, 0, 10_000),
        }])
        .unwrap();
    assert!(engine.sequence_doc_output(), "on by default");
    assert!(engine.set_sequence_doc_output(false).is_none(), "nothing playing");
    let status = engine.play_sequence_doc(0).unwrap();
    assert!(status.notes.is_empty(), "{:?}", status.notes);
    assert_eq!(
        engine.live_frame().unwrap(),
        solid([0, 255, 0]),
        "the preview plays"
    );
    std::thread::sleep(Duration::from_millis(60));
    assert!(packets(&recorded).is_empty(), "nothing sent");

    // Turning it on sends from where it is, without restarting the music.
    let generation = engine.playback_generation();
    let status = engine.set_sequence_doc_output(true).unwrap();
    assert_eq!(status.state, "playing");
    assert_eq!(engine.playback_generation(), generation + 1);
    wait_until(|| {
        packets(&recorded)
            .iter()
            .any(|p| p[10..40] == solid([0, 255, 0])[..])
    });

    // Editing the show keeps it as chosen.
    engine.set_sequence_doc_output(false);
    std::thread::sleep(Duration::from_millis(30));
    let sent = packets(&recorded).len();
    let mut prop = engine.show().props[0].clone();
    prop.transform.position.x = 2.0;
    engine.apply(vec![Edit::UpdateProp { prop }]).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(packets(&recorded).len() <= sent + 2, "stopped sending");
    engine.stop_playback();
}

#[test]
fn a_new_sequence_can_start_with_its_music_and_nothing_to_undo() {
    let (mut engine, _recorded, _dir) = engine();
    let snapshot = engine
        .new_sequence_doc("Song", 60_000, Some("/music/song.mp3"))
        .unwrap();
    assert_eq!(snapshot.sequence.audio.as_deref(), Some("/music/song.mp3"));
    assert!(!snapshot.dirty);
    assert!(!snapshot.can_undo);
    assert_eq!(
        engine.sequence_music().as_deref(),
        Some(Path::new("/music/song.mp3"))
    );
    let silent = engine.new_sequence_doc("Quiet", 1000, None).unwrap();
    assert_eq!(silent.sequence.audio, None);
}

#[test]
fn a_new_sequence_can_start_with_rows_and_still_nothing_to_undo() {
    let (mut engine, _recorded, _dir) = engine();
    let strip = engine.show().props[0].id;
    let rows = vec![
        Row::new(Target::Group(pf_model::GroupId::new())),
        Row::new(Target::Prop(strip)),
    ];
    let snapshot = engine
        .new_sequence_doc_with_rows("Song", 60_000, None, rows.clone())
        .unwrap();
    assert_eq!(snapshot.sequence.rows, rows);
    assert!(!snapshot.dirty);
    assert!(!snapshot.can_undo);

    // Rows that couldn't be opened from a file aren't taken; the open sequence stays.
    let row = Row::new(Target::Prop(strip));
    let err = engine
        .new_sequence_doc_with_rows("Twice", 1000, None, vec![row.clone(), row])
        .unwrap_err();
    assert_eq!(err.to_string(), "A row with that id already exists.");
    assert_eq!(engine.sequence_doc().unwrap().sequence.name, "Song");
}

#[test]
fn unsaved_sequences_are_kept_and_offered_back_after_a_restart() {
    let (mut engine, _recorded, dir) = engine();
    let row = new_doc(&mut engine, 2000);
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::RED, 0, 500),
        }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());
    // Unchanged since: nothing to write.
    assert!(!engine.autosave_sequence().unwrap());
    // A run doesn't offer its own work back.
    assert!(engine.sequence_recoveries().is_empty());
    let kept = engine.sequence_doc().unwrap().sequence;

    // The app quit without saving; the next run finds the work.
    let mut next = Engine::new(dir.path());
    let offered = next.sequence_recoveries();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].name, "Song");
    assert_eq!(offered[0].path, None);
    assert!(offered[0].saved_at_ms > 0);
    let snapshot = next.recover_sequence(&offered[0].id).unwrap();
    assert_eq!(snapshot.sequence, kept);
    assert!(snapshot.dirty, "recovered work is unsaved");
    assert!(!snapshot.can_undo);
    assert_eq!(snapshot.path, None);
    // It's now this run's to keep: a third run sees it under a new name, once.
    assert!(next.sequence_recoveries().is_empty());
    let third = Engine::new(dir.path());
    let again = third.sequence_recoveries();
    assert_eq!(again.len(), 1);
    assert_ne!(again[0].id, offered[0].id);

    // Saving it leaves nothing to recover.
    let saved = dir.path().join("song.pfseq.json");
    next.save_sequence_doc_as(&saved).unwrap();
    assert!(third.sequence_recoveries().is_empty());
    assert!(!next.autosave_sequence().unwrap());
    assert!(third.sequence_recoveries().is_empty());
}

#[test]
fn an_imported_sequence_is_kept_like_any_unsaved_one() {
    let (mut engine, _recorded, dir) = engine();
    // Unsaved work in the open sequence, kept on disk.
    let row = new_doc(&mut engine, 2000);
    engine
        .edit_sequence(vec![SequenceEdit::RemoveRow { id: row }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());

    // An import replaces it (the UI asked first): the old copy goes, and the import is kept
    // in its place, since it has never been saved.
    let mut doc = pf_sequence::Sequence::new("Imported", 5000);
    let mut row = Row::new(Target::Prop(engine.show().props[0].id));
    row.layers[0].effects.push(on(Rgb::RED, 0, 1000));
    doc.rows.push(row);
    engine.adopt_sequence_doc(doc.clone()).unwrap();
    assert!(Engine::new(dir.path()).sequence_recoveries().is_empty());
    assert!(engine.autosave_sequence().unwrap());
    assert!(!engine.autosave_sequence().unwrap(), "unchanged since");

    // The next run offers the import back, unsaved and without a file.
    let mut next = Engine::new(dir.path());
    let offered = next.sequence_recoveries();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].name, "Imported");
    assert_eq!(offered[0].path, None);
    let snapshot = next.recover_sequence(&offered[0].id).unwrap();
    assert_eq!(snapshot.sequence, doc);
    assert!(snapshot.dirty);

    // Saved: nothing left to recover.
    engine
        .save_sequence_doc_as(&dir.path().join("imported.pfseq.json"))
        .unwrap();
    assert!(!engine.autosave_sequence().unwrap());
}

#[test]
fn recovered_work_saves_back_to_its_file_and_can_be_thrown_away() {
    let (mut engine, _recorded, dir) = engine();
    let file = dir.path().join("show.pfseq.json");
    let row = new_doc(&mut engine, 2000);
    engine.save_sequence_doc_as(&file).unwrap();
    // Saved and unchanged: nothing is kept.
    assert!(!engine.autosave_sequence().unwrap());
    assert!(Engine::new(dir.path()).sequence_recoveries().is_empty());
    engine
        .edit_sequence(vec![SequenceEdit::AddEffect {
            row,
            layer: 0,
            effect: on(Rgb::BLUE, 0, 500),
        }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());

    let mut next = Engine::new(dir.path());
    let offered = next.sequence_recoveries();
    assert_eq!(offered[0].path, Some(file.display().to_string()));
    next.recover_sequence(&offered[0].id).unwrap();
    let snapshot = next.save_sequence_doc().unwrap();
    assert!(!snapshot.dirty);
    let on_disk = pf_engine::load_sequence(&file).unwrap();
    assert_eq!(on_disk.rows[0].layers[0].effects.len(), 1);

    // Thrown away: gone for good, and asking for it again is answered plainly.
    engine
        .edit_sequence(vec![SequenceEdit::RemoveRow { id: row }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());
    let mut later = Engine::new(dir.path());
    let offered = later.sequence_recoveries();
    assert_eq!(offered.len(), 1);
    later.discard_sequence_recovery(&offered[0].id);
    assert!(later.sequence_recoveries().is_empty());
    let err = later.recover_sequence(&offered[0].id).unwrap_err();
    assert_eq!(err.to_string(), "That unsaved sequence isn't there anymore.");
    assert!(matches!(
        later.recover_sequence("../../etc"),
        Err(EngineError::UnknownRecovery)
    ));

    // Closing or replacing a sequence drops its kept copy (the UI asks first).
    let row = new_doc(&mut engine, 1000);
    engine
        .edit_sequence(vec![SequenceEdit::RemoveRow { id: row }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());
    assert_eq!(Engine::new(dir.path()).sequence_recoveries().len(), 1);
    engine.close_sequence_doc();
    assert!(Engine::new(dir.path()).sequence_recoveries().is_empty());
}

/// A sequence saved in `dir/Seq` with music in `dir/Seq/Music`.
fn saved_with_music(engine: &mut Engine, dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let song = dir.join("Seq/Music/Carol.mp3");
    std::fs::create_dir_all(song.parent().unwrap()).unwrap();
    std::fs::write(&song, b"x").unwrap();
    let file = dir.join("Seq/Carol.pfseq.json");
    engine
        .new_sequence_doc("Carol", 5_000, Some(&pf_model::path_to_text(&song)))
        .unwrap();
    engine.save_sequence_doc_as(&file).unwrap();
    (file, song)
}

fn saved_audio(file: &Path) -> serde_json::Value {
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    json["audio"].clone()
}

#[test]
fn music_in_the_sequence_folder_is_saved_relative_and_follows_a_move() {
    let (mut engine, _, dir) = engine();
    let (file, song) = saved_with_music(&mut engine, dir.path());
    assert_eq!(saved_audio(&file), "Music/Carol.mp3");
    assert_eq!(engine.sequence_music(), Some(song));
    assert_eq!(engine.sequence_music_missing(), None);

    let moved = dir.path().join("Elsewhere");
    std::fs::rename(dir.path().join("Seq"), &moved).unwrap();
    engine.open_sequence_doc(&moved.join("Carol.pfseq.json")).unwrap();
    assert_eq!(engine.sequence_music(), Some(moved.join("Music/Carol.mp3")));
    assert_eq!(engine.sequence_music_missing(), None);
}

#[test]
fn missing_music_is_found_again_or_located_as_one_undo_step() {
    let (mut engine, _, dir) = engine();
    let (file, song) = saved_with_music(&mut engine, dir.path());
    let now = dir.path().join("Seq/Audio/Carol.mp3");
    std::fs::create_dir_all(now.parent().unwrap()).unwrap();
    std::fs::rename(&song, &now).unwrap();
    engine.open_sequence_doc(&file).unwrap();

    let missing = engine.sequence_music_missing().expect("missing");
    assert_eq!(missing.message, "Carol.mp3 isn't where it was.");
    assert_eq!(missing.owner, "Music for Carol");
    let search = engine.sequence_music_search().unwrap();
    assert_eq!(search.folders(), [dir.path().join("Seq")]);
    let found = search.run();
    assert_eq!(found.found.len(), 1);
    let result = engine.use_found_sequence_music(&found).unwrap().expect("used");
    assert!(result.changed && result.dirty);
    assert_eq!(engine.sequence_music(), Some(now.clone()));
    assert_eq!(engine.sequence_music_missing(), None);
    // Used once: the same find doesn't apply again.
    assert!(engine.use_found_sequence_music(&found).unwrap().is_none());
    engine.undo_sequence().unwrap();
    assert!(engine.sequence_music_missing().is_some());

    let gone = pf_engine::check_chosen_file(&dir.path().join("nope.mp3")).unwrap_err();
    assert_eq!(
        gone.to_string(),
        "nope.mp3 isn't there anymore. Choose another file."
    );
    engine.relink_sequence_music(&now).unwrap();
    assert_eq!(engine.sequence_music(), Some(now));
    // Saved relative to the sequence file.
    engine.save_sequence_doc().unwrap();
    assert_eq!(saved_audio(&file), "Audio/Carol.mp3");
}

#[test]
fn recovered_work_finds_relative_music_next_to_its_original_file() {
    let (mut engine, _, dir) = engine();
    let (file, song) = saved_with_music(&mut engine, dir.path());
    engine.open_sequence_doc(&file).unwrap();
    assert_eq!(
        engine.sequence_document().unwrap().audio.as_deref(),
        Some("Music/Carol.mp3")
    );
    let row = Row::new(Target::Prop(engine.show().props[0].id));
    engine
        .edit_sequence(vec![SequenceEdit::AddRow { row, index: None }])
        .unwrap();
    assert!(engine.autosave_sequence().unwrap());
    let mut next = Engine::new(dir.path());
    let offered = next.sequence_recoveries();
    next.recover_sequence(&offered[0].id).unwrap();
    assert_eq!(next.sequence_music(), Some(song));
}

/// A silent music clock the test can read, which notes every jump with where the music was.
struct SharedClock {
    inner: Arc<Mutex<pf_audio::SilentClock>>,
    /// (music position when asked to jump, where to), in ms.
    jumps: Arc<Mutex<Vec<(u64, u64)>>>,
}

impl AudioClock for SharedClock {
    fn start(&mut self, position: Duration) {
        self.inner.lock().unwrap().start(position);
    }
    fn pause(&mut self) {
        self.inner.lock().unwrap().pause();
    }
    fn resume(&mut self) {
        self.inner.lock().unwrap().resume();
    }
    fn seek(&mut self, position: Duration) {
        let mut clock = self.inner.lock().unwrap();
        let from = clock.position().as_millis() as u64;
        self.jumps
            .lock()
            .unwrap()
            .push((from, position.as_millis() as u64));
        clock.seek(position);
    }
    fn position(&self) -> Duration {
        self.inner.lock().unwrap().position()
    }
    fn set_volume(&mut self, _volume: f32) {}
}

#[test]
fn looping_plays_again_from_the_top_with_the_music_in_step() {
    let (engine, recorded, _dir) = engine();
    let music: Arc<Mutex<pf_audio::SilentClock>> = Default::default();
    let jumps: Arc<Mutex<Vec<(u64, u64)>>> = Default::default();
    let (shared, noted) = (music.clone(), jumps.clone());
    let clocks: ClockFactory = Arc::new(move |_music: Option<&Path>| {
        Ok(Box::new(SharedClock {
            inner: shared.clone(),
            jumps: noted.clone(),
        }) as Box<dyn AudioClock>)
    });
    let mut engine = engine.with_clocks(clocks);
    let row = new_doc(&mut engine, 200);
    engine
        .edit_sequence(vec![
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: on(Rgb::RED, 0, 100),
            },
            SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: on(Rgb::GREEN, 100, 200),
            },
        ])
        .unwrap();
    assert!(!engine.sequence_doc_loop(), "off by default");
    assert!(engine.set_sequence_doc_loop(true).is_none(), "nothing playing");
    assert!(engine.sequence_doc_loop());

    let status = engine.play_sequence_doc(0).unwrap();
    assert!(status.looping);
    wait_until(|| {
        let status = engine.playback_status().unwrap();
        assert_ne!(status.state, "ended", "a looping sequence never ends");
        assert!(status.position_ms <= 200, "{status:?}");
        jumps.lock().unwrap().len() >= 3
    });

    // Each time round, the music goes back to its very top as soon as the lights reach the end
    // (within a frame or two), so nothing builds up between them from one loop to the next.
    for &(from, to) in jumps.lock().unwrap().iter().take(3) {
        assert_eq!(to, 0, "the music starts again from the top");
        assert!(
            // A frame or two late is in step; a loaded machine can schedule the playback thread
            // later still, so allow that without letting real drift (which adds up) through.
            (200..350).contains(&from),
            "jumped back {from} ms in, for a 200 ms sequence"
        );
    }
    let lights = engine.playback_status().unwrap().position_ms;
    let heard = music.lock().unwrap().position().as_millis() as u64;
    assert!(
        // The two positions are read a moment apart, on a thread that may be scheduled late.
        heard.abs_diff(lights) <= 120,
        "lights at {lights} ms and music at {heard} ms after three loops"
    );

    // The controllers got every loop: red, green, then red again.
    let mut colors: Vec<Vec<u8>> = packets(&recorded).iter().map(|p| p[10..40].to_vec()).collect();
    colors.dedup();
    let red_again = colors
        .windows(3)
        .any(|w| w[0] == solid([255, 0, 0]) && w[1] == solid([0, 255, 0]) && w[2] == solid([255, 0, 0]));
    assert!(red_again, "sent across the loop: {colors:?}");

    // Turning it off lets the sequence finish this time round.
    let status = engine.set_sequence_doc_loop(false).unwrap();
    assert!(!status.looping);
    wait_until(|| engine.playback_status().unwrap().state == "ended");

    // Stopping while looping stops.
    engine.set_sequence_doc_loop(true);
    engine.play_sequence_doc(0).unwrap();
    engine.stop_playback();
    assert!(engine.playback_status().is_none());
}

#[test]
fn a_looping_sequence_paused_at_the_end_stays_there_until_played() {
    let (mut engine, _recorded, _dir) = engine();
    new_doc(&mut engine, 200);
    engine.set_sequence_doc_loop(true);
    engine.play_sequence_doc(0).unwrap();
    engine.set_playback_paused(true).unwrap();
    engine.seek_playback(200).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    let status = engine.playback_status().unwrap();
    assert_eq!((status.state, status.position_ms), ("paused", 175));
    // Playing on from there goes round to the top.
    engine.set_playback_paused(false).unwrap();
    wait_until(|| engine.playback_status().unwrap().position_ms < 100);
    assert_eq!(engine.playback_status().unwrap().state, "playing");
    engine.stop_playback();
}

#[test]
fn effects_follow_the_music_in_the_preview_and_the_export_alike() {
    let (mut engine, _recorded, dir) = engine();
    let mut controller = engine.show().controllers[0].clone();
    controller.sequence_channels = Some(pf_model::SequenceChannels {
        start: 1,
        count: 30,
        raw_ddp_offsets: false,
    });
    engine.apply(vec![Edit::UpdateController { controller }]).unwrap();
    // A loud second, then one at a quarter.
    let rate = 22_050;
    let samples: Vec<f32> = (0..2 * rate)
        .map(|i| {
            let amplitude = if i < rate { 1.0 } else { 0.25 };
            amplitude * (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin()
        })
        .collect();
    let song = dir.path().join("song.wav");
    std::fs::write(&song, pf_audio::wav_bytes(&samples, rate as u32)).unwrap();
    engine.set_audio_cache_dir(Some(dir.path().join("cache")));
    engine
        .new_sequence_doc("Song", 2000, Some(song.to_str().unwrap()))
        .unwrap();
    let row = Row::new(Target::Prop(engine.show().props[0].id));
    let row_id = row.id;
    let mut effect = on(Rgb::RED, 0, 2000);
    for key in ["startLevel", "endLevel"] {
        effect
            .curves
            .insert(key.into(), pf_sequence::Curve::music(0.0, 1.0, 0.0, false));
    }
    engine
        .edit_sequence(vec![
            SequenceEdit::AddRow { row, index: None },
            SequenceEdit::AddEffect {
                row: row_id,
                layer: 0,
                effect,
            },
        ])
        .unwrap();
    // Halfway while the music is worked out in the background, then following it.
    let first = engine.sequence_doc_frame(500).unwrap();
    assert!(
        first == solid([128, 0, 0]) || first == solid([255, 0, 0]),
        "{first:?}"
    );
    wait_until(|| engine.sequence_doc_frame(500).unwrap() == solid([255, 0, 0]));
    let quiet = engine.sequence_doc_frame(1500).unwrap();
    assert!((60..=66).contains(&quiet[0]), "{quiet:?}");
    // The export follows it the same way, frame for frame.
    let path = dir.path().join("song.fseq");
    engine.export_sequence_doc(&path).unwrap();
    let mut file = pf_fseq::Sequence::open(&path).unwrap();
    let mut frame = vec![0u8; 30];
    for index in [0u32, 20, 39, 40, 60, 79] {
        file.read_frame(index, &mut frame).unwrap();
        let preview = engine.sequence_doc_frame(u64::from(index) * 25).unwrap();
        assert_eq!(frame, preview, "frame {index}");
    }
    // The track was kept on disk for next time.
    assert_eq!(std::fs::read_dir(dir.path().join("cache")).unwrap().count(), 1);
}
