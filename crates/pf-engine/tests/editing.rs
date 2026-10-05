//! Edits, undo/redo, and file handling through the engine.

use pf_engine::{Edit, Engine, EngineError};
use pf_model::{Generator, Prop, ShapeSource};

fn line(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    )
}

fn engine() -> (Engine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (Engine::new(dir.path().join("data")), dir)
}

#[test]
fn a_new_engine_has_a_clean_untitled_show() {
    let (engine, _dir) = engine();
    let snap = engine.snapshot();
    assert_eq!(snap.show.name, "Untitled Show");
    assert!(!snap.dirty && !snap.can_undo && !snap.can_redo);
    assert_eq!(snap.path, None);
    assert_eq!(snap.summary.props, 0);
}

#[test]
fn edits_are_undoable_and_redoable() {
    let (mut engine, _dir) = engine();
    let snap = engine
        .apply(vec![Edit::AddProp {
            prop: line("Arch", 50),
        }])
        .unwrap();
    assert!(snap.dirty && snap.can_undo);
    assert_eq!(snap.summary.pixels, 50);

    let snap = engine.undo();
    assert_eq!(snap.summary.props, 0);
    assert!(snap.can_redo);
    let snap = engine.redo();
    assert_eq!(snap.show.props[0].name, "Arch");
    assert!(snap.revision > 0);
}

#[test]
fn a_batch_is_one_undo_step_and_is_all_or_nothing() {
    let (mut engine, _dir) = engine();
    let a = line("A", 5);
    engine
        .apply(vec![
            Edit::AddProp { prop: a.clone() },
            Edit::AddProp { prop: line("B", 5) },
            Edit::RenameShow { name: "House".into() },
        ])
        .unwrap();
    assert_eq!(engine.snapshot().summary.props, 2);
    engine.undo();
    let snap = engine.snapshot();
    assert_eq!(snap.summary.props, 0);
    assert_eq!(snap.show.name, "Untitled Show");

    // The second edit fails, so the first must not apply either.
    let err = engine
        .apply(vec![Edit::AddProp { prop: a.clone() }, Edit::AddProp { prop: a }])
        .unwrap_err();
    assert!(matches!(err, EngineError::DuplicateId { kind: "prop" }));
    assert_eq!(engine.snapshot().summary.props, 0);
}

#[test]
fn edits_that_exceed_limits_are_rejected() {
    let (mut engine, _dir) = engine();
    let err = engine
        .apply(vec![Edit::AddProp {
            prop: line("Huge", 2_000_000),
        }])
        .unwrap_err();
    assert!(matches!(err, EngineError::TooLarge(_)));
    assert!(err.to_string().contains("at most 1000000"), "{err}");
    assert!(!engine.snapshot().can_undo);
}

#[test]
fn snapshots_report_issues_errors_first() {
    let (mut engine, _dir) = engine();
    let snap = engine
        .apply(vec![
            Edit::AddProp {
                prop: line("Unwired", 5),
            },
            Edit::SetFrameRate { fps: 5 },
        ])
        .unwrap();
    assert_eq!(snap.issues.len(), 2);
    assert_eq!(snap.issues[0].severity, pf_model::Severity::Error);
}

#[test]
fn save_open_and_dirty_tracking() {
    let (mut engine, dir) = engine();
    assert!(matches!(engine.save(), Err(EngineError::NoPath)));
    engine
        .apply(vec![Edit::RenameShow { name: "Saved".into() }])
        .unwrap();
    let path = dir.path().join("house.pixelflow.json");
    let snap = engine.save_as(&path).unwrap();
    assert!(!snap.dirty);
    assert_eq!(snap.path.as_deref(), Some(path.to_str().unwrap()));
    assert!(snap.can_undo, "saving keeps undo history");

    engine
        .apply(vec![Edit::RenameShow {
            name: "Changed".into(),
        }])
        .unwrap();
    assert!(engine.snapshot().dirty);
    engine.save().unwrap();
    assert!(!engine.snapshot().dirty);

    let (mut other, _d) = self::engine();
    let snap = other.open(&path).unwrap();
    assert_eq!(snap.show.name, "Changed");
    assert!(!snap.dirty && !snap.can_undo);

    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "nope").unwrap();
    assert!(other.open(&bad).is_err());
    assert_eq!(
        other.snapshot().show.name,
        "Changed",
        "failed open keeps the current show"
    );
}

#[test]
fn autosave_writes_history_only_when_changed_and_restore_is_undoable() {
    let (mut engine, _dir) = engine();
    assert!(engine.autosave().unwrap().is_none());
    engine
        .apply(vec![Edit::RenameShow { name: "First".into() }])
        .unwrap();
    let entry = engine.autosave().unwrap().expect("history written");
    assert!(
        engine.autosave().unwrap().is_none(),
        "unchanged since last autosave"
    );

    engine
        .apply(vec![Edit::RenameShow {
            name: "Second".into(),
        }])
        .unwrap();
    engine.autosave().unwrap();
    assert_eq!(engine.history().len(), 2);

    let snap = engine.restore(&entry.id).unwrap();
    assert_eq!(snap.show.name, "First");
    assert_eq!(engine.undo().show.name, "Second");
    assert!(matches!(
        engine.restore("../etc/passwd"),
        Err(EngineError::UnknownHistoryEntry)
    ));
}

#[test]
fn new_show_clears_history() {
    let (mut engine, _dir) = engine();
    engine.apply(vec![Edit::AddProp { prop: line("A", 1) }]).unwrap();
    let snap = engine.new_show("Fresh");
    assert_eq!(snap.show.name, "Fresh");
    assert!(!snap.can_undo && !snap.dirty);
}

#[test]
fn an_edit_that_changes_nothing_is_not_an_undo_step() {
    let (mut engine, dir) = engine();
    let arch = line("Arch", 5);
    engine.apply(vec![Edit::AddProp { prop: arch.clone() }]).unwrap();
    let path = dir.path().join("a.pixelflow.json");
    engine.save_as(&path).unwrap();
    engine.open(&path).unwrap(); // clean state with no undo history
    let before = engine.snapshot().revision;
    let snap = engine.apply(vec![Edit::UpdateProp { prop: arch }]).unwrap();
    assert!(!snap.can_undo, "no undo step");
    assert!(!snap.dirty, "still clean");
    assert_eq!(snap.revision, before, "revision unchanged");
}

#[test]
fn save_as_makes_the_next_autosave_write_into_the_new_history() {
    let (mut engine, dir) = engine();
    engine.apply(vec![Edit::AddProp { prop: line("A", 3) }]).unwrap();
    assert!(engine.autosave().unwrap().is_some());
    assert!(engine.autosave().unwrap().is_none(), "nothing changed");
    engine.save_as(&dir.path().join("house.pixelflow.json")).unwrap();
    assert!(engine.history().is_empty());
    assert!(engine.autosave().unwrap().is_some(), "new file gets a first copy");
    assert_eq!(engine.history().len(), 1);
}

#[test]
fn an_adopted_show_is_new_and_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    let mut show = pf_model::Show::new("Imported");
    show.props.push(pf_model::Prop::new(
        "Roof",
        pf_model::ShapeSource::Generator(pf_model::Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    ));
    let snapshot = engine.adopt_show(pf_engine::CheckedShow::new(show).unwrap());
    assert_eq!(snapshot.show.name, "Imported");
    assert_eq!(snapshot.summary.props, 1);
    assert!(snapshot.dirty && snapshot.path.is_none() && !snapshot.can_undo);
}

#[test]
fn a_show_a_file_couldnt_hold_is_refused_with_a_plain_message() {
    let mut show = pf_model::Show::new("Imported");
    let prop = pf_model::Prop::new(
        "Roof",
        pf_model::ShapeSource::Generator(pf_model::Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    );
    let mut slot = pf_model::PortSlot::new(prop.id);
    slot.null_pixels = pf_model::MAX_NULL_PIXELS + 1;
    show.props.push(prop);
    let mut port = pf_model::Port::new(1);
    port.slots.push(slot);
    let mut controller = pf_model::Controller::new("C", "192.0.2.1", pf_model::Protocol::Ddp);
    controller.ports.push(port);
    show.controllers.push(controller);
    let err = pf_engine::CheckedShow::new(show).unwrap_err();
    assert!(matches!(err, pf_engine::EngineError::InvalidShow(_)), "{err:?}");
    assert!(err.to_string().contains("null pixels"), "{err}");
}
