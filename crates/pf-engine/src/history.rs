//! Undo/redo as a bounded stack of show snapshots.

use pf_model::{Generator, ShapeSource, Show};

/// A rough in-memory size of a show, used to bound undo memory (not an exact measure).
pub fn estimated_bytes(show: &Show) -> usize {
    let props: usize = show
        .props
        .iter()
        .map(|prop| {
            let shape = match &prop.shape {
                ShapeSource::Measured { points, .. } => 12 * points.len(),
                ShapeSource::Generator(Generator::CustomGrid { cells, .. }) => 4 * cells.len(),
                ShapeSource::Generator(_) => 0,
            };
            let regions: u64 = prop
                .regions
                .iter()
                .map(|r| r.entry_count(prop.node_count()))
                .sum();
            256 + shape + 64 * prop.regions.len() + 8 * regions as usize
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
        .map(|g| 16 * g.members.len() + 32 * g.submodels.len())
        .sum();
    props + controllers + groups
}

/// Undo and redo stacks of whole-show snapshots. Snapshotting the whole show makes every
/// edit (including multi-edit batches) undoable as one step, with no per-edit inverse logic.
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<(Show, usize)>,
    redo: Vec<(Show, usize)>,
    undo_bytes: usize,
    limit: usize,
    byte_budget: usize,
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
        }
    }

    /// Records the show as it was before a change. Clears the redo stack.
    pub fn record(&mut self, before: Show) {
        let bytes = estimated_bytes(&before);
        self.push_undo(before, bytes);
        self.redo.clear();
    }

    fn push_undo(&mut self, show: Show, bytes: usize) {
        self.undo.push((show, bytes));
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
        let (previous, bytes) = self.undo.pop()?;
        self.undo_bytes -= bytes;
        let current_bytes = estimated_bytes(&current);
        self.redo.push((current, current_bytes));
        Some(previous)
    }

    /// Returns the show to restore for redo, remembering `current` for undo.
    pub fn redo(&mut self, current: Show) -> Option<Show> {
        let (next, _) = self.redo.pop()?;
        let current_bytes = estimated_bytes(&current);
        self.push_undo(current, current_bytes);
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
