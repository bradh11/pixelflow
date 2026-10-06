//! Undo/redo as a bounded stack of show snapshots.

use pf_model::{Generator, GroupMember, RegionKind, ShapeSource, Show};

/// A rough in-memory size of a show, used to bound undo memory (not an exact measure).
pub fn estimated_bytes(show: &Show) -> usize {
    let props: usize = show
        .props
        .iter()
        .map(|prop| {
            let shape = match &prop.shape {
                ShapeSource::Measured { points, .. } => 12 * points.len(),
                ShapeSource::Generator(Generator::CustomGrid { cells, .. }) => 4 * cells.len(),
                ShapeSource::Generator(Generator::PolyLine {
                    vertices, segments, ..
                }) => 12 * vertices.len() + 32 * segments.len(),
                ShapeSource::Generator(Generator::Icicles { drops, .. }) => 4 * drops.len(),
                ShapeSource::Generator(_) => 0,
            };
            // What a region holds in memory: its runs and gaps (a run of any length is one
            // entry), not the pixels they cover.
            let regions: usize = prop
                .regions
                .iter()
                .map(|r| match &r.kind {
                    RegionKind::Nodes { lines, .. } => lines.iter().map(|l| 24 + 12 * l.len()).sum(),
                    RegionKind::SubBuffer { .. } => 0,
                    RegionKind::Face(face) => 8 * face.ranges().count(),
                })
                .sum();
            256 + shape + 64 * prop.regions.len() + regions
        })
        .sum();
    let controllers: usize = show
        .controllers
        .iter()
        .map(|c| 128 + c.ports.iter().map(|p| 32 * p.slots.len()).sum::<usize>())
        .sum();
    let groups: usize = show
        .groups
        .iter()
        .map(|g| {
            g.members
                .iter()
                .map(|m| match m {
                    GroupMember::Prop(_) => 16,
                    GroupMember::Region(_) => 32,
                })
                .sum::<usize>()
        })
        .sum();
    props + controllers + groups
}

/// Undo and redo stacks of whole-show snapshots. Snapshotting the whole show makes every
/// edit (including multi-edit batches) undoable as one step, with no per-edit inverse logic.
///
/// Each step has a serial number that stays with it as it moves between the stacks, so a step
/// can be recognized (to undo it together with a sequence step made at the same time).
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<(Show, usize, u64)>,
    redo: Vec<(Show, usize, u64)>,
    undo_bytes: usize,
    limit: usize,
    byte_budget: usize,
    next_serial: u64,
}

impl History {
    /// Keeps at most `limit` undo steps and about `byte_budget` bytes of them (oldest are
    /// dropped first; the newest step is always kept).
    pub fn new(limit: usize, byte_budget: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            limit: limit.max(1),
            byte_budget,
            next_serial: 1,
        }
    }

    /// Records the show as it was before a change. Clears the redo stack.
    pub fn record(&mut self, before: Show) {
        let bytes = estimated_bytes(&before);
        let serial = self.next_serial;
        self.next_serial += 1;
        self.push_undo(before, bytes, serial);
        self.redo.clear();
    }

    /// The serial of the step undo would take back next.
    pub fn next_undo(&self) -> Option<u64> {
        self.undo.last().map(|step| step.2)
    }

    /// The serial of the step redo would bring back next.
    pub fn next_redo(&self) -> Option<u64> {
        self.redo.last().map(|step| step.2)
    }

    fn push_undo(&mut self, show: Show, bytes: usize, serial: u64) {
        self.undo.push((show, bytes, serial));
        self.undo_bytes += bytes;
        let mut drop_count = 0;
        let mut kept_bytes = self.undo_bytes;
        while self.undo.len() - drop_count > 1
            && (self.undo.len() - drop_count > self.limit || kept_bytes > self.byte_budget)
        {
            kept_bytes -= self.undo[drop_count].1;
            drop_count += 1;
        }
        self.undo.drain(..drop_count);
        self.undo_bytes = kept_bytes;
    }

    /// Returns the show to restore for undo, remembering `current` for redo.
    pub fn undo(&mut self, current: Show) -> Option<Show> {
        let (previous, bytes, serial) = self.undo.pop()?;
        self.undo_bytes -= bytes;
        let current_bytes = estimated_bytes(&current);
        self.redo.push((current, current_bytes, serial));
        Some(previous)
    }

    /// Returns the show to restore for redo, remembering `current` for undo.
    pub fn redo(&mut self, current: Show) -> Option<Show> {
        let (next, _, serial) = self.redo.pop()?;
        let current_bytes = estimated_bytes(&current);
        self.push_undo(current, current_bytes, serial);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(name: &str) -> Show {
        Show::new(name)
    }

    #[test]
    fn undo_and_redo_walk_the_stacks() {
        let mut history = History::new(10, usize::MAX);
        history.record(show("v1"));
        history.record(show("v2"));
        let back = history.undo(show("v3")).unwrap();
        assert_eq!(back.name, "v2");
        assert!(history.can_redo());
        let forward = history.redo(back).unwrap();
        assert_eq!(forward.name, "v3");
        assert!(!history.can_redo());
    }

    #[test]
    fn recording_clears_redo_and_the_limit_drops_oldest() {
        let mut history = History::new(2, usize::MAX);
        history.record(show("a"));
        history.record(show("b"));
        history.record(show("c"));
        assert_eq!(history.undo(show("d")).unwrap().name, "c");
        assert_eq!(history.undo(show("c")).unwrap().name, "b");
        assert!(history.undo(show("b")).is_none());

        history.record(show("x"));
        assert!(!history.can_redo());
    }

    fn big(name: &str, points: usize) -> Show {
        let mut show = Show::new(name);
        show.props.push(pf_model::Prop::new(
            "Measured",
            ShapeSource::Measured {
                points: vec![pf_model::Vec3::new(0.0, 0.0, 0.0); points],
                provenance: pf_model::Provenance::Manual,
            },
        ));
        show
    }

    #[test]
    fn a_long_submodel_run_costs_one_entry_not_one_per_pixel() {
        let line = |len: u32| {
            let mut show = show("s");
            let mut prop = pf_model::Prop::new(
                "Line",
                ShapeSource::Generator(Generator::Line {
                    nodes: 100_000,
                    length: 1.0,
                }),
            );
            prop.regions.push(pf_model::Region::nodes(
                "All",
                vec![vec![Some(pf_model::NodeRun::new(0, len - 1))]],
            ));
            show.props.push(prop);
            estimated_bytes(&show)
        };
        assert_eq!(line(1), line(100_000));
    }

    #[test]
    fn the_byte_budget_drops_oldest_entries() {
        // Each show is about 1.2 MB; a 2.5 MB budget holds two.
        let mut history = History::new(100, 2_500_000);
        for name in ["a", "b", "c", "d"] {
            history.record(big(name, 100_000));
        }
        assert_eq!(history.undo(show("now")).unwrap().name, "d");
        assert_eq!(history.undo(show("d")).unwrap().name, "c");
        assert!(history.undo(show("c")).is_none());
    }

    #[test]
    fn the_newest_entry_is_kept_even_over_budget() {
        let mut history = History::new(100, 10);
        history.record(big("a", 1000));
        history.record(big("b", 1000));
        assert_eq!(history.undo(show("now")).unwrap().name, "b");
        assert!(history.undo(show("b")).is_none());
    }
}
