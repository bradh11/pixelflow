//! The assistant's draft: a private copy of the show (and open sequence) that its edit tools
//! change, a proposal made from it (summary, diff, preview), and applying that proposal to the
//! engine as one undo step. Until the user presses Apply, nothing the assistant does reaches
//! the open show, the controllers, or any file.

use crate::align::{Anchors, lock};
use crate::diff::{Diff, diff};
use crate::summary::{SectionSummary, Timeline, section_summaries, timeline};
use pf_analysis::Analysis;
use pf_engine::{
    Edit, Engine, EngineError, SequenceEdit, SequenceEditResult, ShowSnapshot, edited_sequence, edited_show,
};
use pf_model::{PropId, Show};
use pf_sequence::{EffectId, Sequence, TimingTrackId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

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
            let mut end = screen.len().min(40);
            while !screen.is_char_boundary(end) {
                end -= 1;
            }
            screen.truncate(end);
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
    /// Which show this is (see [`Engine::show_generation`]).
    pub show_generation: u64,
    pub sequence: Option<OpenDoc>,
    /// The open sequence's song (resolved next to the sequence file), for analysis. Only ever
    /// read, never written.
    pub music: Option<PathBuf>,
    pub context: UiContext,
}

impl Workspace {
    pub fn from_engine(engine: &Engine, context: UiContext) -> Self {
        Self {
            show: engine.show().clone(),
            revision: engine.revision(),
            show_generation: engine.show_generation(),
            sequence: engine
                .sequence_doc_id()
                .zip(engine.sequence_document())
                .map(|(id, doc)| OpenDoc { id, doc: doc.clone() }),
            music: engine.sequence_music(),
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
    /// Effect edges and timing marks locked to the music so far (see [`Draft::lock_to_music`]).
    locked_edges: usize,
    /// Timing tracks `place_effects` cut effects at.
    placed_on: Vec<TimingTrackId>,
    /// Cues staged so far, by kind (see [`crate::cues`]).
    cues: BTreeMap<String, usize>,
}

impl Draft {
    pub fn new(base: Workspace) -> Self {
        Self {
            show: base.show.clone(),
            sequence: base.sequence.as_ref().map(|s| s.doc.clone()),
            base,
            show_edits: Vec::new(),
            sequence_edits: Vec::new(),
            locked_edges: 0,
            placed_on: Vec::new(),
            cues: BTreeMap::new(),
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

    /// Tries several sequence edits at once (all or nothing), as one step of the draft.
    pub fn edit_sequence_batch(&mut self, edits: Vec<SequenceEdit>) -> Result<(), EngineError> {
        let doc = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        self.sequence = Some(edited_sequence(doc, &edits)?);
        self.sequence_edits.extend(edits);
        Ok(())
    }

    /// Notes that effects were cut at the timing track `id` (see [`Draft::lock_to_music`]).
    pub fn placed_on(&mut self, id: TimingTrackId) {
        if !self.placed_on.contains(&id) {
            self.placed_on.push(id);
        }
    }

    /// Counts cues staged on the draft, by kind (for the proposal's summary).
    pub fn staged(&mut self, counts: &BTreeMap<String, usize>) {
        for (cue, n) in counts {
            *self.cues.entry(cue.clone()).or_default() += n;
        }
    }

    /// Moves the effect edges and timing marks the draft added or moved onto the song's
    /// sections, moments, accents, sung words, bars, and beats (see [`crate::align`]), as one
    /// more step of the draft. The user's own Sections, Moments, and Accents tracks win over the
    /// analysis's; edges on the syllables of a syllables track effects were cut at stay on them.
    /// Answers how many edges moved.
    pub fn lock_to_music(&mut self, analysis: Option<&Analysis>) -> usize {
        let base = self.base.sequence.as_ref().map(|s| &s.doc);
        let Some(doc) = self.sequence.as_ref().filter(|doc| Some(*doc) != base) else {
            return 0;
        };
        let Some(anchors) = Anchors::for_song(analysis, base) else {
            return 0;
        };
        let syllables: Vec<u64> = doc
            .timing_tracks
            .iter()
            .filter(|t| self.placed_on.contains(&t.id) && crate::lyrics::tracks::is_syllables(t))
            .flat_map(|t| t.marks.iter().flat_map(|m| [m.start_ms, m.end_ms]))
            .collect();
        let anchors = anchors.with_syllables(syllables);
        // The song's own tracks, as analysis makes them, are the music already.
        let as_made: Vec<_> = analysis
            .map(|a| {
                let mut tracks = a.timing_tracks();
                tracks.push(a.sections_track());
                tracks.push(a.accents_track());
                tracks.push(a.drums_track());
                tracks.push(crate::song::moments_track(a, base.unwrap_or(doc)));
                tracks.into_iter().map(|t| t.marks).collect()
            })
            .unwrap_or_default();
        let locked = lock(base, doc, &anchors, &as_made);
        if locked.edits.is_empty() || self.edit_sequence_batch(locked.edits.clone()).is_err() {
            return 0;
        }
        self.locked_edges += locked.edges();
        locked.edges()
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
            locked_edges: if sequence_changed { self.locked_edges } else { 0 },
            cues: sequence_changed
                .then(|| crate::cues::summary(&self.cues))
                .flatten(),
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
            show_generation: self.base.show_generation,
            sequence_doc: self.base.sequence.as_ref().map(|s| s.id),
            base_show: self.base.show.clone(),
            base_sequence: self.base.sequence.as_ref().map(|s| s.doc.clone()),
            draft_show: self.show.clone(),
            draft_sequence: self.sequence.clone(),
        })
    }

    /// Whether this draft was made for the show and sequence open in `workspace` (another
    /// show, or another sequence document, means it no longer applies).
    pub fn is_for(&self, workspace: &Workspace) -> bool {
        self.is_for_ids(
            workspace.show_generation,
            workspace.sequence.as_ref().map(|s| s.id),
        )
    }

    /// Like [`Draft::is_for`], from the show's generation and the sequence document's id.
    pub fn is_for_ids(&self, show_generation: u64, sequence_doc: Option<u64>) -> bool {
        self.base.show_generation == show_generation
            && self.base.sequence.as_ref().map(|s| s.id) == sequence_doc
    }
}

/// A finished draft for the user to review: a summary, what changes, and the edits that make it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub diff: Diff,
    /// Effect edges and timing marks the draft locked to the music.
    pub locked_edges: usize,
    /// The cues staged ("Staged 14 cues: 6 hits, 3 word pops, …"), if any.
    pub cues: Option<String>,
    pub show_edits: Vec<Edit>,
    pub sequence_edits: Vec<SequenceEdit>,
    /// The show revision the draft started from.
    pub base_revision: u64,
    /// The show the draft was made for (see [`Engine::show_generation`]).
    pub show_generation: u64,
    /// The sequence document the sequence edits are for.
    pub sequence_doc: Option<u64>,
    /// The show and sequence as the draft started from them: every item the draft changes
    /// must still be this way when it's applied, or the user's own later edit would be undone.
    pub base_show: Show,
    pub base_sequence: Option<Sequence>,
    /// The show as it would be (for the preview).
    pub draft_show: Show,
    pub draft_sequence: Option<Sequence>,
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
    /// For a sequence proposal: what it does in each section of the song.
    pub sections: Vec<SectionSummary>,
    /// For a sequence proposal: the draft sequence drawn small.
    pub timeline: Option<Timeline>,
    /// For a sequence proposal: effect edges and timing marks locked to the music.
    pub locked_edges: usize,
    /// For a sequence proposal: the cues staged, if any.
    pub cues: Option<String>,
}

impl Proposal {
    pub fn view(&self) -> ProposalView {
        let changes_sequence = !self.sequence_edits.is_empty();
        let sequences = self
            .base_sequence
            .as_ref()
            .zip(self.draft_sequence.as_ref())
            .filter(|_| changes_sequence);
        ProposalView {
            id: self.id.clone(),
            summary: self.summary.clone(),
            changed_props: self.diff.touched_props(),
            diff: self.diff.clone(),
            changes_show: !self.show_edits.is_empty(),
            changes_sequence,
            sections: sequences
                .map(|(before, after)| section_summaries(before, after))
                .unwrap_or_default(),
            timeline: sequences.map(|(_, after)| timeline(after, &self.draft_show)),
            locked_edges: if changes_sequence { self.locked_edges } else { 0 },
            cues: self.cues.clone().filter(|_| changes_sequence),
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
    if engine.show_generation() != proposal.show_generation {
        return Err(
            "A different show is open now, so this suggestion no longer applies. Ask again.".to_string(),
        );
    }
    let changes_sequence = !proposal.sequence_edits.is_empty();
    if changes_sequence && engine.sequence_doc_id() != proposal.sequence_doc {
        return Err(
            "The sequence this suggestion changes isn't open anymore. Open it again and ask again."
                .to_string(),
        );
    }
    // Every item the draft changes must be as it was when the draft began: the draft replaces
    // whole items, so applying over the user's later edit to one would quietly undo that edit.
    let mut changed = show_conflicts(&proposal.base_show, &proposal.draft_show, engine.show());
    if let (Some(base), Some(draft), Some(current)) = (
        proposal.base_sequence.as_ref(),
        proposal.draft_sequence.as_ref(),
        engine.sequence_document(),
    ) && changes_sequence
    {
        changed.extend(sequence_conflicts(base, draft, current, engine.show()));
    }
    if !changed.is_empty() {
        return Err(format!(
            "The show changed since this was suggested — ask again. Changed meanwhile: {}.",
            changed.join(", ")
        ));
    }
    let text = |e: EngineError| e.to_string();
    match (proposal.show_edits.is_empty(), changes_sequence) {
        (false, true) => {
            // One undo step for both halves (and nothing at all if either is refused).
            let (snapshot, result) = engine
                .apply_with_sequence(proposal.show_edits.clone(), proposal.sequence_edits.clone())
                .map_err(text)?;
            Ok(Applied {
                snapshot: Some(snapshot),
                sequence: Some(result),
            })
        }
        (false, false) => {
            let before = engine.revision();
            let snapshot = engine.apply(proposal.show_edits.clone()).map_err(text)?;
            Ok(Applied {
                snapshot: (snapshot.revision != before).then_some(snapshot),
                sequence: None,
            })
        }
        (true, true) => Ok(Applied {
            snapshot: None,
            sequence: Some(
                engine
                    .edit_sequence(proposal.sequence_edits.clone())
                    .map_err(text)?,
            ),
        }),
        (true, false) => Ok(Applied {
            snapshot: None,
            sequence: None,
        }),
    }
}

/// Items matched by id that the draft changed (added, changed, or removed) and that are no
/// longer as the draft found them; named as they are now (or were).
fn keyed_conflicts<T: PartialEq>(
    base: &[T],
    draft: &[T],
    current: &[T],
    id: impl Fn(&T) -> String,
    name: impl Fn(&T) -> String,
    out: &mut Vec<String>,
) {
    fn by_id<'a, T>(items: &'a [T], id: &impl Fn(&T) -> String) -> HashMap<String, &'a T> {
        items.iter().map(|item| (id(item), item)).collect()
    }
    let (base, draft, current) = (by_id(base, &id), by_id(draft, &id), by_id(current, &id));
    let mut ids: Vec<&String> = base.keys().chain(draft.keys()).collect();
    ids.sort();
    ids.dedup();
    for key in ids {
        let was = base.get(key);
        if was != draft.get(key)
            && current.get(key) != was
            && let Some(item) = current.get(key).or(was).or(draft.get(key))
        {
            out.push(name(item));
        }
    }
}

fn show_conflicts(base: &Show, draft: &Show, current: &Show) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = |label: &str, touched: bool, kept: bool| {
        if touched && !kept {
            out.push(label.to_string());
        }
    };
    field(
        "the show's name",
        base.name != draft.name,
        current.name == base.name,
    );
    field(
        "the show's settings",
        base.settings != draft.settings,
        current.settings == base.settings,
    );
    field(
        "the background photo",
        base.background != draft.background,
        current.background == base.background,
    );
    field(
        "the house model",
        base.house_model != draft.house_model,
        current.house_model == base.house_model,
    );
    let order = |show: &Show| show.sequences.iter().map(|s| s.id).collect::<Vec<_>>();
    field(
        "the playlist",
        order(base) != order(draft),
        order(current) == order(base),
    );
    keyed_conflicts(
        &base.props,
        &draft.props,
        &current.props,
        |p| p.id.to_string(),
        |p| p.name.clone(),
        &mut out,
    );
    keyed_conflicts(
        &base.groups,
        &draft.groups,
        &current.groups,
        |g| g.id.to_string(),
        |g| g.name.clone(),
        &mut out,
    );
    keyed_conflicts(
        &base.controllers,
        &draft.controllers,
        &current.controllers,
        |c| c.id.to_string(),
        |c| c.name.clone(),
        &mut out,
    );
    keyed_conflicts(
        &base.sequences,
        &draft.sequences,
        &current.sequences,
        |s| s.id.to_string(),
        |s| s.name.clone(),
        &mut out,
    );
    out
}

/// A sequence's effects with where they sit, by id.
fn placed(doc: &Sequence) -> Vec<(pf_sequence::RowId, usize, &pf_sequence::Effect)> {
    doc.rows
        .iter()
        .flat_map(|row| {
            row.layers
                .iter()
                .enumerate()
                .flat_map(move |(layer, l)| l.effects.iter().map(move |e| (row.id, layer, e)))
        })
        .collect()
}

fn sequence_conflicts(base: &Sequence, draft: &Sequence, current: &Sequence, show: &Show) -> Vec<String> {
    let mut out = Vec::new();
    let info = |doc: &Sequence| (doc.name.clone(), doc.audio.clone(), doc.duration_ms, doc.frame_ms);
    if info(base) != info(draft) && info(current) != info(base) {
        out.push("the sequence's name, music, or length".to_string());
    }
    // Rows as shells (what they light, how many layers); their effects are compared one by one.
    let shells = |doc: &Sequence| {
        doc.rows
            .iter()
            .map(|r| (r.id, r.target, r.layers.len()))
            .collect::<Vec<_>>()
    };
    keyed_conflicts(
        &shells(base),
        &shells(draft),
        &shells(current),
        |r| r.0.to_string(),
        |r| crate::diff::target_name(show, r.1),
        &mut out,
    );
    let order = |doc: &Sequence| doc.rows.iter().map(|r| r.id).collect::<Vec<_>>();
    if order(base) != order(draft) && order(current) != order(base) {
        out.push("the row order".to_string());
    }
    let effect_name = |(row, _, effect): &(pf_sequence::RowId, usize, &pf_sequence::Effect)| {
        let target = [current, base, draft]
            .iter()
            .find_map(|doc| doc.rows.iter().find(|r| r.id == *row))
            .map(|r| crate::diff::effect_name(effect, r, show));
        target.unwrap_or_else(|| "an effect".to_string())
    };
    keyed_conflicts(
        &placed(base),
        &placed(draft),
        &placed(current),
        |e| e.2.id.to_string(),
        effect_name,
        &mut out,
    );
    keyed_conflicts(
        &base.timing_tracks,
        &draft.timing_tracks,
        &current.timing_tracks,
        |t| t.id.to_string(),
        |t| t.name.clone(),
        &mut out,
    );
    let tracks = |doc: &Sequence| doc.timing_tracks.iter().map(|t| t.id).collect::<Vec<_>>();
    if tracks(base) != tracks(draft) && tracks(current) != tracks(base) {
        out.push("the timing track order".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_non_ascii_screen_name_is_shortened_without_a_panic() {
        let context = UiContext {
            screen: Some("€".repeat(30)),
            ..UiContext::default()
        }
        .bounded();
        assert_eq!(context.screen.unwrap(), "€".repeat(13));
    }
}
