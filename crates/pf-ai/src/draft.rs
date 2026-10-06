//! The assistant's draft: a private copy of the show (and open sequence) that its edit tools
//! change, a proposal made from it (summary, diff, preview), and applying that proposal to the
//! engine as one undo step. Until the user presses Apply, nothing the assistant does reaches
//! the open show, the controllers, or any file.

use crate::diff::{Diff, diff};
use pf_engine::{
    Edit, Engine, EngineError, SequenceEdit, SequenceEditResult, ShowSnapshot, edited_sequence, edited_show,
};
use pf_model::{PropId, Show};
use pf_sequence::{EffectId, Sequence};
use serde::{Deserialize, Serialize};

/// What the user is looking at, sent with each message so "these props" means something.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiContext {
    /// The screen ("layout", "sequence", ...).
    pub screen: Option<String>,
    pub selected_props: Vec<PropId>,
    pub selected_effects: Vec<EffectId>,
    /// Where the sequence editor's playhead is.
    pub playhead_ms: Option<u64>,
}

impl UiContext {
    /// Keeps the context small (a huge selection is summarized by the tools instead).
    pub fn bounded(mut self) -> Self {
        const MAX: usize = 200;
        self.selected_props.truncate(MAX);
        self.selected_effects.truncate(MAX);
        if let Some(screen) = &mut self.screen {
            screen.truncate(40);
        }
        self
    }
}

/// The open sequence, and which document it is (see [`Engine::sequence_doc_id`]).
#[derive(Debug, Clone, PartialEq)]
pub struct OpenDoc {
    pub id: u64,
    pub doc: Sequence,
}

/// A copy of what's open in the engine, taken under its lock and then let go of, so no network
/// call ever holds the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub show: Show,
    pub revision: u64,
    pub sequence: Option<OpenDoc>,
    pub context: UiContext,
}

impl Workspace {
    pub fn from_engine(engine: &Engine, context: UiContext) -> Self {
        Self {
            show: engine.show().clone(),
            revision: engine.revision(),
            sequence: engine
                .sequence_doc_id()
                .zip(engine.sequence_document())
                .map(|(id, doc)| OpenDoc { id, doc: doc.clone() }),
            context: context.bounded(),
        }
    }
}

/// The draft: the workspace it started from, and the edits made to a copy of it so far.
#[derive(Debug, Clone)]
pub struct Draft {
    base: Workspace,
    show: Show,
    show_edits: Vec<Edit>,
    sequence: Option<Sequence>,
    sequence_edits: Vec<SequenceEdit>,
}

impl Draft {
    pub fn new(base: Workspace) -> Self {
        Self {
            show: base.show.clone(),
            sequence: base.sequence.as_ref().map(|s| s.doc.clone()),
            base,
            show_edits: Vec::new(),
            sequence_edits: Vec::new(),
        }
    }

    pub fn base(&self) -> &Workspace {
        &self.base
    }

    /// The show as the draft has it now.
    pub fn show(&self) -> &Show {
        &self.show
    }

    /// The open sequence as the draft has it now.
    pub fn sequence(&self) -> Option<&Sequence> {
        self.sequence.as_ref()
    }

    pub fn has_edits(&self) -> bool {
        !self.show_edits.is_empty() || !self.sequence_edits.is_empty()
    }

    /// Tries a show edit on the draft, checked as the engine would; on error nothing changes.
    pub fn edit_show(&mut self, edit: Edit) -> Result<(), EngineError> {
        self.show = edited_show(&self.show, std::slice::from_ref(&edit))?;
        self.show_edits.push(edit);
        Ok(())
    }

    /// Tries a sequence edit on the draft's copy of the open sequence.
    pub fn edit_sequence(&mut self, edit: SequenceEdit) -> Result<(), EngineError> {
        let doc = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        self.sequence = Some(edited_sequence(doc, std::slice::from_ref(&edit))?);
        self.sequence_edits.push(edit);
        Ok(())
    }

    /// Starts over from the workspace.
    pub fn reset(&mut self) {
        *self = Draft::new(self.base.clone());
    }

    /// What the draft changes, compared with where it started.
    pub fn diff(&self) -> Diff {
        let sequences = self
            .base
            .sequence
            .as_ref()
            .map(|s| &s.doc)
            .zip(self.sequence.as_ref());
        diff(&self.base.show, &self.show, sequences)
    }

    /// A proposal for the user to review, or `None` when the draft changes nothing.
    pub fn propose(&self, summary: &str) -> Option<Proposal> {
        let diff = self.diff();
        if diff.is_empty() {
            return None;
        }
        let show_changed = self.show != self.base.show;
        let sequence_changed = self.sequence.as_ref() != self.base.sequence.as_ref().map(|s| &s.doc);
        Some(Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            summary: summary.trim().chars().take(2000).collect(),
            diff,
            show_edits: if show_changed {
                self.show_edits.clone()
            } else {
                Vec::new()
            },
            sequence_edits: if sequence_changed {
                self.sequence_edits.clone()
            } else {
                Vec::new()
            },
            base_revision: self.base.revision,
            sequence_doc: self.base.sequence.as_ref().map(|s| s.id),
            draft_show: self.show.clone(),
        })
    }
}

/// A finished draft for the user to review: a summary, what changes, and the edits that make it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub diff: Diff,
    pub show_edits: Vec<Edit>,
    pub sequence_edits: Vec<SequenceEdit>,
    /// The show revision the draft started from.
    pub base_revision: u64,
    /// The sequence document the sequence edits are for.
    pub sequence_doc: Option<u64>,
    /// The show as it would be (for the preview).
    pub draft_show: Show,
}

/// What the window sees of a proposal (no edits, which stay in Rust until applied).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalView {
    pub id: String,
    pub summary: String,
    pub diff: Diff,
    /// Props added or changed, to highlight in the preview.
    pub changed_props: Vec<String>,
    pub changes_show: bool,
    pub changes_sequence: bool,
}

impl Proposal {
    pub fn view(&self) -> ProposalView {
        ProposalView {
            id: self.id.clone(),
            summary: self.summary.clone(),
            changed_props: self.diff.touched_props(),
            diff: self.diff.clone(),
            changes_show: !self.show_edits.is_empty(),
            changes_sequence: !self.sequence_edits.is_empty(),
        }
    }
}

/// What applying a proposal did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// The show after (when the proposal changed it): one undo step.
    pub snapshot: Option<ShowSnapshot>,
    /// The sequence edit result (when it changed the open sequence): one sequence undo step.
    pub sequence: Option<SequenceEditResult>,
}

/// Applies a proposal: its show edits as one undo step, and its sequence edits as one step of
/// the sequence's own undo. If the show changed since the draft began, the edits are tried on
/// the show as it is now and refused (changing nothing) when they no longer fit. Only ever
/// called from the user's Apply.
pub fn apply_proposal(engine: &mut Engine, proposal: &Proposal) -> Result<Applied, String> {
    let changed_since = engine.revision() != proposal.base_revision;
    let stale = |e: EngineError| {
        if changed_since {
            format!(
                "The show changed after the assistant made this draft, and the draft no longer fits ({e}). Ask the assistant again."
            )
        } else {
            e.to_string()
        }
    };
    if !proposal.sequence_edits.is_empty() && engine.sequence_doc_id() != proposal.sequence_doc {
        return Err(
            "The sequence this draft changes isn't open anymore. Open it again and ask the assistant again."
                .to_string(),
        );
    }
    // Check the sequence half first, so a refusal there changes nothing at all.
    if !proposal.sequence_edits.is_empty() {
        let doc = engine
            .sequence_document()
            .ok_or_else(|| EngineError::NoSequence.to_string())?;
        edited_sequence(doc, &proposal.sequence_edits).map_err(stale)?;
    }
    let mut snapshot = None;
    if !proposal.show_edits.is_empty() {
        let before = engine.revision();
        let next = engine.apply(proposal.show_edits.clone()).map_err(stale)?;
        if next.revision != before {
            snapshot = Some(next);
        }
    }
    let mut sequence = None;
    if !proposal.sequence_edits.is_empty() {
        match engine.edit_sequence(proposal.sequence_edits.clone()) {
            Ok(result) => sequence = Some(result),
            Err(e) => {
                // All or nothing: take back the show half.
                if snapshot.is_some() {
                    engine.undo();
                }
                return Err(stale(e));
            }
        }
    }
    Ok(Applied { snapshot, sequence })
}
