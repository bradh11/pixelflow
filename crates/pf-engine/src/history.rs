//! Undo/redo as a bounded stack of show snapshots.

use pf_model::Show;

/// Undo and redo stacks of whole-show snapshots. Snapshotting the whole show makes every
/// edit (including multi-edit batches) undoable as one step, with no per-edit inverse logic.
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<Show>,
    redo: Vec<Show>,
    limit: usize,
}

impl History {
    /// Keeps at most `limit` undo steps (oldest are dropped first).
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Records the show as it was before a change. Clears the redo stack.
    pub fn record(&mut self, before: Show) {
        if self.undo.len() == self.limit {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
    }

    /// Returns the show to restore for undo, remembering `current` for redo.
    pub fn undo(&mut self, current: Show) -> Option<Show> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Returns the show to restore for redo, remembering `current` for undo.
    pub fn redo(&mut self, current: Show) -> Option<Show> {
        let next = self.redo.pop()?;
        self.undo.push(current);
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
        let mut history = History::new(10);
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
        let mut history = History::new(2);
        history.record(show("a"));
        history.record(show("b"));
        history.record(show("c"));
        assert_eq!(history.undo(show("d")).unwrap().name, "c");
        assert_eq!(history.undo(show("c")).unwrap().name, "b");
        assert!(history.undo(show("b")).is_none());

        history.record(show("x"));
        assert!(!history.can_redo());
    }
}
