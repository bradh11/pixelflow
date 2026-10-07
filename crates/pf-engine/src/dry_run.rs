//! Edits applied to copies, away from the engine: what a draft (the AI assistant's, say) needs to
//! try changes exactly as the engine would, without changing the open show or sequence.

use crate::edit::Edit;
use crate::error::EngineError;
use crate::sequence_doc::{OpenSequence, SequenceEdit};
use crate::snapshot::PreviewProp;
use pf_model::Show;
use pf_sequence::Sequence;

/// The show after `edits`, checked exactly as [`crate::Engine::apply`] checks a batch: every edit
/// must apply and the result must still open as a show file: within PixelFlow's size limits,
/// with shapes it can build and numbers that are finite. `show` is not changed.
pub fn edited_show(show: &Show, edits: &[Edit]) -> Result<Show, EngineError> {
    let mut next = show.clone();
    for edit in edits {
        edit.apply(&mut next)?;
    }
    if let Some(issue) = pf_model::limit_issues(&next).into_iter().next() {
        return Err(EngineError::TooLarge(issue.message));
    }
    Ok(next)
}

/// The sequence after `edits`, checked exactly as [`crate::Engine::edit_sequence`] checks a
/// batch (ranges, size limits, unique ids). `doc` is not changed.
pub fn edited_sequence(doc: &Sequence, edits: &[SequenceEdit]) -> Result<Sequence, EngineError> {
    let mut open = OpenSequence::new(doc.clone(), None, 0);
    open.apply(edits, None)?;
    Ok(open.doc)
}

/// Every prop's pixel positions in the front view (x right, y up) for any show, with where its
/// colors would sit in a live frame: what [`crate::Engine::preview_props`] gives for the open one.
pub fn preview_props_of(show: &Show) -> Vec<PreviewProp> {
    let (map, _) = pf_mapping::map_show(show);
    show.props
        .iter()
        .filter_map(|prop| {
            let layout = map.prop_layout(prop.id)?;
            let points = pf_geometry::world_positions(prop)
                .into_iter()
                .take(layout.nodes as usize)
                .flat_map(|p| [p.x, p.y])
                .collect();
            Some(PreviewProp {
                prop: prop.id,
                frame_offset: layout.frame_offset,
                channels_per_pixel: layout.channels_per_pixel,
                points,
            })
        })
        .collect()
}

/// Renders frames of any sequence on any show, laid out like [`preview_props_of`] says: what a
/// draft's preview plays without touching the open show or sequence.
pub struct DraftRenderer {
    renderer: pf_render::Renderer,
}

impl DraftRenderer {
    pub fn new(show: &Show) -> Self {
        let (map, _) = pf_mapping::map_show(show);
        Self {
            renderer: pf_render::Renderer::new(show, &map),
        }
    }

    /// The frame (show frame bytes) at `position_ms`.
    pub fn frame(&mut self, doc: &Sequence, position_ms: u64) -> Vec<u8> {
        let mut frame = vec![0u8; self.renderer.frame_len()];
        self.renderer.render(doc, position_ms, &mut frame);
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use pf_model::{Generator, Prop, ShapeSource};
    use pf_sequence::{Row, Target};

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        )
    }

    #[test]
    fn an_edited_copy_leaves_the_original_alone() {
        let show = Show::new("t");
        let a = line("A", 5);
        let next = edited_show(&show, &[Edit::AddProp { prop: a.clone() }]).unwrap();
        assert!(show.props.is_empty());
        assert_eq!(next.props, vec![a]);
    }

    #[test]
    fn a_failing_edit_fails_the_whole_batch_like_the_engine() {
        let show = Show::new("t");
        let a = line("A", 5);
        let edits = [Edit::AddProp { prop: a.clone() }, Edit::AddProp { prop: a }];
        let err = edited_show(&show, &edits).unwrap_err();
        assert_eq!(err.to_string(), "A prop with that id already exists.");
    }

    #[test]
    fn size_limits_are_checked_like_the_engine() {
        let show = Show::new("t");
        let huge = line("Huge", 5_000_000);
        let mut engine = Engine::new(std::env::temp_dir());
        let ours = edited_show(&show, &[Edit::AddProp { prop: huge.clone() }]).unwrap_err();
        let theirs = engine.apply(vec![Edit::AddProp { prop: huge }]).unwrap_err();
        assert_eq!(ours.to_string(), theirs.to_string());
    }

    #[test]
    fn sequence_edits_are_checked_on_a_copy() {
        let doc = Sequence::new("Song", 10_000);
        let row = Row::new(Target::Prop(pf_model::PropId::new()));
        let next = edited_sequence(
            &doc,
            &[SequenceEdit::AddRow {
                row: row.clone(),
                index: None,
            }],
        )
        .unwrap();
        assert!(doc.rows.is_empty());
        assert_eq!(next.rows.len(), 1);
        let again = edited_sequence(&next, &[SequenceEdit::AddRow { row, index: None }]);
        assert!(again.is_err(), "repeated ids are refused like in the engine");
    }

    #[test]
    fn the_show_generation_changes_only_when_another_show_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        let first = engine.show_generation();
        engine.apply(vec![Edit::AddProp { prop: line("A", 4) }]).unwrap();
        engine.undo();
        engine.redo();
        assert_eq!(
            engine.show_generation(),
            first,
            "edits, undo, and redo keep the show"
        );
        let saved = dir.path().join("a.json");
        engine.save_as(&saved).unwrap();
        assert_eq!(engine.show_generation(), first, "saving keeps the show");
        engine.new_show("B");
        let second = engine.show_generation();
        assert_ne!(second, first);
        engine.open(&saved).unwrap();
        assert_ne!(engine.show_generation(), second);
        let third = engine.show_generation();
        engine.adopt_show(crate::CheckedShow::new(Show::new("C")).unwrap());
        assert_ne!(engine.show_generation(), third);

        // The ways that read the disk without holding the engine replace the show too.
        let fourth = engine.show_generation();
        let loaded = crate::persist::read_show(&saved).unwrap();
        let snapshot = engine.open_read(&saved, loaded);
        assert_ne!(engine.show_generation(), fourth, "opened off the lock");
        assert!(snapshot.files_checked);
        assert_eq!(snapshot.sequence_revision, None, "no sequence is open");
        engine.apply(vec![Edit::AddProp { prop: line("D", 4) }]).unwrap();
        let entry = engine.autosave().unwrap().expect("a version to restore");
        let fifth = engine.show_generation();
        let restored = engine.history_file(&entry.id).unwrap().read().unwrap();
        engine.restore_read(restored);
        assert_ne!(engine.show_generation(), fifth, "restored off the lock");
    }

    #[test]
    fn a_draft_renders_like_the_engine_renders_the_open_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        let prop = line("A", 4);
        engine.apply(vec![Edit::AddProp { prop: prop.clone() }]).unwrap();
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0].effects.push(
            pf_sequence::Effect::new(pf_sequence::EffectKind::On, 0, 1000)
                .with_palette(vec![pf_model::Rgb::new(255, 0, 0)]),
        );
        engine
            .new_sequence_doc_with_rows("Song", 2000, None, vec![row])
            .unwrap();
        let doc = engine.sequence_document().unwrap().clone();
        let mut draft = DraftRenderer::new(engine.show());
        let ours = draft.frame(&doc, 500);
        assert_eq!(Some(ours.clone()), engine.sequence_doc_frame(500));
        assert!(ours.contains(&255));
        assert!(draft.frame(&doc, 1500).iter().all(|&b| b == 0));
    }

    #[test]
    fn preview_positions_match_the_engine_for_the_open_show() {
        let mut engine = Engine::new(std::env::temp_dir());
        engine
            .apply(vec![
                Edit::AddProp { prop: line("A", 4) },
                Edit::AddProp { prop: line("B", 3) },
            ])
            .unwrap();
        assert_eq!(preview_props_of(engine.show()), engine.preview_props());
    }
}
