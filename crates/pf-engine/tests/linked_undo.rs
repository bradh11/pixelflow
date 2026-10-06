//! A change to the show and the open sequence made together (an assistant proposal) is one
//! undo step: undoing it from either side takes both halves back, and redo brings both again.

use pf_engine::{Edit, Engine, SequenceEdit};
use pf_model::{Generator, Prop, ShapeSource};
use pf_sequence::{Row, Target};

fn line(name: &str) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line {
            nodes: 10,
            length: 1.0,
        }),
    )
}

/// An engine with one prop and an open, empty sequence.
fn engine() -> (Engine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    engine.apply(vec![Edit::AddProp { prop: line("Roof") }]).unwrap();
    engine.new_sequence_doc("Song", 10_000, None).unwrap();
    (engine, dir)
}

/// Adds a prop and a row lighting it, together.
fn add_tree_with_row(engine: &mut Engine) -> (Prop, Row) {
    let tree = line("Tree");
    let row = Row::new(Target::Prop(tree.id));
    engine
        .apply_with_sequence(
            vec![Edit::AddProp { prop: tree.clone() }],
            vec![SequenceEdit::AddRow {
                row: row.clone(),
                index: None,
            }],
        )
        .unwrap();
    (tree, row)
}

fn rows(engine: &Engine) -> usize {
    engine.sequence_document().unwrap().rows.len()
}

#[test]
fn one_undo_takes_back_both_halves_from_either_side() {
    let (mut engine, _dir) = engine();
    let show_before = engine.show().clone();
    let (tree, _) = add_tree_with_row(&mut engine);
    assert_eq!(engine.show().props.len(), 2);
    assert_eq!(rows(&engine), 1);

    // From the show's undo (Layout)...
    let undone = engine.undo();
    assert_eq!(engine.show(), &show_before);
    assert_eq!(rows(&engine), 0, "the row went too");
    assert_eq!(
        undone.sequence_revision,
        engine.sequence_doc().map(|s| s.revision)
    );
    // ...and redo brings both back.
    engine.redo();
    assert!(engine.show().props.iter().any(|p| p.id == tree.id));
    assert_eq!(rows(&engine), 1);

    // From the sequence's undo (Sequence screen).
    let result = engine.undo_sequence().unwrap();
    assert_eq!(rows(&engine), 0);
    assert_eq!(engine.show(), &show_before, "the prop went too");
    assert_eq!(result.show_revision, engine.revision());
    engine.redo_sequence().unwrap();
    assert_eq!((engine.show().props.len(), rows(&engine)), (2, 1));
}

#[test]
fn a_refused_sequence_half_changes_nothing_and_leaves_no_redo() {
    let (mut engine, _dir) = engine();
    let show_before = engine.show().clone();
    let can_undo = engine.snapshot().can_undo;
    let missing = pf_sequence::RowId::new();
    let err = engine
        .apply_with_sequence(
            vec![Edit::AddProp { prop: line("Tree") }],
            vec![SequenceEdit::RemoveRow { id: missing }],
        )
        .unwrap_err();
    assert!(err.to_string().contains("isn't in the sequence anymore"), "{err}");
    assert_eq!(engine.show(), &show_before);
    let snapshot = engine.snapshot();
    assert_eq!((snapshot.can_undo, snapshot.can_redo), (can_undo, false));
    assert!(!engine.sequence_doc().unwrap().can_undo);
}

#[test]
fn later_edits_undo_on_their_own_first() {
    let (mut engine, _dir) = engine();
    let show_before = engine.show().clone();
    add_tree_with_row(&mut engine);
    engine
        .apply(vec![Edit::RenameShow { name: "Later".into() }])
        .unwrap();
    engine.undo();
    assert_eq!(engine.show().name, show_before.name);
    assert_eq!(
        (engine.show().props.len(), rows(&engine)),
        (2, 1),
        "only the rename went"
    );
    engine.undo();
    assert_eq!((engine.show(), rows(&engine)), (&show_before, 0));
}

#[test]
fn a_half_buried_under_later_sequence_edits_is_left_alone() {
    let (mut engine, _dir) = engine();
    add_tree_with_row(&mut engine);
    let roof = engine.show().props[0].id;
    let later = Row::new(Target::Prop(roof));
    engine
        .edit_sequence(vec![SequenceEdit::AddRow {
            row: later,
            index: None,
        }])
        .unwrap();
    // The show half is on top of its stack, but the sequence's top is the user's later row:
    // undoing the show doesn't reach under it.
    engine.undo();
    assert_eq!((engine.show().props.len(), rows(&engine)), (1, 2));
    // The later row undoes, then the sequence half on its own (its partner already went).
    engine.undo_sequence().unwrap();
    engine.undo_sequence().unwrap();
    assert_eq!(rows(&engine), 0);
    // Both halves are next to redo again, so they come back together.
    engine.redo();
    assert_eq!((engine.show().props.len(), rows(&engine)), (2, 1));
}

#[test]
fn a_new_show_or_sequence_ends_the_pairing() {
    let (mut engine, _dir) = engine();
    add_tree_with_row(&mut engine);
    engine.new_sequence_doc("Other", 5_000, None).unwrap();
    engine.undo();
    assert_eq!(engine.show().props.len(), 1);
    assert_eq!(rows(&engine), 0, "the other sequence is untouched");
}
