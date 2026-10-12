//! Holding the corpus's imports to the looks they earned before: CI's check that an import
//! hasn't got materially worse next to xLights' own frames.
//!
//! **The baseline** (`baseline.json` in the corpus folder) keeps, for each sequence with an
//! xLights reference and each of its effects, the look score it got and the verdict it earned
//! in each of [`RUNS`] scorings. Each scoring gives every effect its chance from another set of
//! ids ([`crate::compare::fix_ids`]), and holds the effects that draw on chance against
//! PixelFlow's own renders from other seeds as every scorecard does: so the spread of those
//! scores is how far chance moves each effect's look, measured.
//!
//! **The check** scores each sequence once, from the first set of ids, which the baseline's
//! first scoring was made from too: on the same build it is the same scorecard, to the digit,
//! every run. An effect fails when
//!
//! - its look is more than [`MARGIN`] points under the lowest the baseline measured, or
//! - it earns a worse verdict than the worst the baseline saw, even with [`MARGIN`] points
//!   added to its look (so that a look on a verdict's line, a hair either side of it on another
//!   system, never fails), or
//! - it was measured in the baseline and isn't now.
//!
//! An effect judged in every scoring of the baseline whose look says too little now is listed,
//! and doesn't fail by itself: the look it got is held to the baseline all the same. An effect
//! that does better than the baseline by the margin is listed as improved, for the baseline to
//! be raised on purpose. A sequence whose effects aren't the ones the baseline
//! lists (an effect more or less, another kind, another place) can't be held to it at all, and
//! fails until the baseline is made again.

use crate::compare::{Options, Request, compare};
use crate::corpus::{Entry, Manifest, label_of};
use crate::scorecard::{EffectScore, Scorecard};
use crate::{FidelityError, look};
use pf_xlights::sequence::Verdict;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;

/// The file the baseline is kept in, in the corpus folder.
pub const FILE: &str = "baseline.json";
/// Points a look may fall under the lowest the baseline measured before the check fails.
pub const MARGIN: f64 = 3.0;
/// Scorings a baseline is made from, each from another set of ids.
pub const RUNS: u32 = 5;
/// The set of ids the check scores from: the baseline's first.
pub const IDS: u64 = 0;

/// The looks the corpus's imports earned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub about: String,
    pub tool: String,
    pub margin: f64,
    pub runs: u32,
    pub sequences: Vec<SequenceBase>,
}

/// One sequence's effects, and its look in each scoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceBase {
    pub label: String,
    pub looks: Vec<Option<f64>>,
    pub effects: Vec<EffectBase>,
}

/// One effect: which it is, and in each scoring the look it got (`None` where it wasn't
/// measured) and the verdict it was judged to earn (`None` where it wasn't judged).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectBase {
    pub index: usize,
    pub kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub looks: Vec<Option<f64>>,
    pub verdicts: Vec<Option<Verdict>>,
}

impl EffectBase {
    fn scores(&self) -> impl Iterator<Item = f64> + '_ {
        self.looks.iter().flatten().copied()
    }

    /// The lowest and highest look of the scorings.
    pub fn range(&self) -> Option<(f64, f64)> {
        let low = self
            .scores()
            .fold(None, |m: Option<f64>, s| Some(m.map_or(s, |m| m.min(s))))?;
        let high = self.scores().fold(low, f64::max);
        Some((low, high))
    }

    /// The worst and best verdict it was judged to earn.
    pub fn verdicts(&self) -> Option<(Verdict, Verdict)> {
        let worst = self.verdicts.iter().flatten().max().copied()?;
        let best = self.verdicts.iter().flatten().min().copied()?;
        Some((worst, best))
    }

    fn judged_always(&self) -> bool {
        !self.verdicts.is_empty() && self.verdicts.iter().all(Option::is_some)
    }

    fn is(&self, effect: &EffectScore) -> bool {
        (
            self.index,
            self.kind.as_str(),
            self.target.as_str(),
            self.start_ms,
            self.end_ms,
        ) == (
            effect.index,
            effect.kind.as_str(),
            effect.target.as_str(),
            effect.start_ms,
            effect.end_ms,
        )
    }
}

/// The look an effect got in a scorecard, whether or not it says enough to judge it by, and
/// the verdict it was judged to earn.
fn seen(effect: &EffectScore) -> (Option<f64>, Option<Verdict>) {
    match (&effect.look, &effect.says_little) {
        (Some(judged), _) => (Some(judged.score), Some(judged.verdict)),
        (None, Some(little)) => (Some(little.score), None),
        (None, None) => (None, None),
    }
}

/// The sequences of the manifest that have an xLights reference, and the labels of those that
/// don't yet.
pub fn references(manifest: &Manifest) -> (Vec<&Entry>, Vec<String>) {
    let (with, without): (Vec<&Entry>, Vec<&Entry>) =
        manifest.sequences.iter().partition(|e| e.fseq.is_file());
    (with, without.into_iter().map(label_of).collect())
}

/// Scores one sequence of the corpus with its effects' ids from `set`.
pub fn score(entry: &Entry, set: u64) -> Result<Scorecard, FidelityError> {
    compare(
        Request {
            show: &entry.show,
            xsq: &entry.xsq,
            fseq: &entry.fseq,
        },
        Options {
            label: Some(label_of(entry)),
            frame_offset: entry.frame_offset,
            ids: Some(set),
            ..Options::default()
        },
    )
}

/// Makes the baseline: every sequence with a reference, scored `runs` times from the sets of
/// ids 0, 1, …. `done` hears of each scoring as it is finished.
pub fn measure(
    entries: &[&Entry],
    runs: u32,
    mut done: impl FnMut(&str, u32),
) -> Result<Baseline, FidelityError> {
    let runs = runs.max(1);
    let mut sequences = Vec::new();
    for entry in entries {
        let mut base: Option<SequenceBase> = None;
        for set in 0..u64::from(runs) {
            let card = score(entry, set)?;
            let base = base.get_or_insert_with(|| SequenceBase {
                label: card.sequence.label.clone(),
                looks: Vec::new(),
                effects: card
                    .effects
                    .iter()
                    .map(|e| EffectBase {
                        index: e.index,
                        kind: e.kind.clone(),
                        target: e.target.clone(),
                        start_ms: e.start_ms,
                        end_ms: e.end_ms,
                        looks: Vec::new(),
                        verdicts: Vec::new(),
                    })
                    .collect(),
            });
            base.looks.push(card.look.score);
            // The effects are the same in every scoring: only their ids differ.
            for (b, e) in base.effects.iter_mut().zip(&card.effects) {
                let (look, verdict) = seen(e);
                b.looks.push(look.map(|s| (s * 100.0).round() / 100.0));
                b.verdicts.push(verdict);
            }
            done(&card.sequence.label, set as u32 + 1);
        }
        sequences.extend(base);
    }
    Ok(Baseline {
        about: "How each effect of the corpus looks next to xLights' own render, in each of several \
                scorings from other seeds. `pf-fidelity regress` holds the import to it; \
                `pf-fidelity regress --accept` makes it again. See docs/fidelity.md."
            .to_string(),
        tool: crate::TOOL.to_string(),
        margin: MARGIN,
        runs,
        sequences,
    })
}

/// How far each effect's look spread over the baseline's scorings: the widest, and the effect.
pub fn widest_spread(baseline: &Baseline) -> Option<(f64, String)> {
    baseline
        .sequences
        .iter()
        .flat_map(|s| s.effects.iter().map(move |e| (s, e)))
        .filter_map(|(s, e)| {
            let (low, high) = e.range()?;
            Some((
                high - low,
                format!("{} #{} {} on {}", s.label, e.index, e.kind, e.target),
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
}

/// One effect that moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub sequence: String,
    pub index: usize,
    pub kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The baseline: its looks' range and its worst verdict.
    pub was: String,
    pub now: String,
    pub what: String,
    pub how: Move,
}

/// Which way an effect moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// It fails the check.
    Worse,
    /// For the baseline to be raised on purpose.
    Better,
    /// Judged in every scoring of the baseline, and its look says too little now. Not a failure
    /// by itself: whether a look says enough is chance's to decide, and can tip on a hair, and
    /// the look it got is held to the baseline all the same.
    Unjudged,
}

/// A sequence's label, its look now, and the lowest and highest the baseline measured.
pub type SequenceLook = (String, Option<f64>, Option<(f64, f64)>);

/// What holding the corpus to the baseline found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Checked {
    /// What can't be held to the baseline at all.
    pub broken: Vec<String>,
    pub moved: Vec<Moved>,
    /// Per sequence, its look now and in the baseline.
    pub looks: Vec<SequenceLook>,
    /// Sequences of the corpus that have no reference yet and aren't in the baseline.
    pub unrendered: Vec<String>,
    /// How many effects were held to the baseline.
    pub held: usize,
}

impl Checked {
    pub fn passed(&self) -> bool {
        self.broken.is_empty() && !self.moved.iter().any(|m| m.how == Move::Worse)
    }
}

fn verdict_word(v: Option<Verdict>) -> &'static str {
    match v {
        Some(Verdict::Matches) => "matches",
        Some(Verdict::Close) => "close",
        Some(Verdict::Different) => "different",
        Some(Verdict::NoEquivalent) => "no equivalent",
        None => "not judged",
    }
}

/// Holds the scorecards of the sequences with references to the baseline. `unrendered` are the
/// sequences that have no reference.
pub fn check(baseline: Option<&Baseline>, cards: &[Scorecard], unrendered: &[String]) -> Checked {
    let empty = Vec::new();
    let kept = baseline.map_or(&empty, |b| &b.sequences);
    let margin = baseline.map_or(MARGIN, |b| b.margin);
    let mut checked = Checked::default();
    for label in unrendered {
        if kept.iter().any(|s| &s.label == label) {
            checked.broken.push(format!(
                "{label}: in the baseline, and its xLights reference is missing"
            ));
        } else {
            checked.unrendered.push(label.clone());
        }
    }
    for card in cards {
        let label = &card.sequence.label;
        let Some(base) = kept.iter().find(|s| &s.label == label) else {
            checked.broken.push(format!(
                "{label}: has an xLights reference and no baseline: make it with `pf-fidelity regress --accept`"
            ));
            continue;
        };
        let looks: Vec<f64> = base.looks.iter().flatten().copied().collect();
        let range = looks.iter().copied().fold(None, |r: Option<(f64, f64)>, s| {
            Some(r.map_or((s, s), |(a, b)| (a.min(s), b.max(s))))
        });
        checked.looks.push((label.clone(), card.look.score, range));
        if base.effects.len() != card.effects.len()
            || !base.effects.iter().zip(&card.effects).all(|(b, e)| b.is(e))
        {
            checked.broken.push(format!(
                "{label}: its effects aren't the ones the baseline lists ({} now, {} then): an import that \
                 brings effects in otherwise is a change to accept on purpose",
                card.effects.len(),
                base.effects.len()
            ));
            continue;
        }
        for (b, e) in base.effects.iter().zip(&card.effects) {
            checked.held += 1;
            let (look, verdict) = seen(e);
            let range = b.range();
            let verdicts = b.verdicts();
            let was = match (range, verdicts) {
                (Some((low, high)), Some((worst, _))) => {
                    format!("{low:.1}–{high:.1}, at worst {}", verdict_word(Some(worst)))
                }
                (Some((low, high)), None) => format!("{low:.1}–{high:.1}, never judged"),
                (None, _) => "not measured".to_string(),
            };
            let now = match look {
                Some(score) => format!("{score:.1}, {}", verdict_word(verdict)),
                None => "not measured".to_string(),
            };
            let mut worse = Vec::new();
            let mut better = Vec::new();
            let mut unjudged = Vec::new();
            match (look, range) {
                (None, Some(_)) => worse.push("no longer measured".to_string()),
                (Some(score), Some((low, high))) => {
                    if score < low - margin {
                        worse.push(format!(
                            "look {:.1} points under the lowest measured",
                            low - score
                        ));
                    } else if score > high + margin {
                        better.push(format!(
                            "look {:.1} points over the highest measured",
                            score - high
                        ));
                    }
                }
                _ => {}
            }
            if b.judged_always() && verdict.is_none() && look.is_some() {
                unjudged.push("no longer judged: its look says too little now".to_string());
            }
            if let (Some(v), Some(score), Some((worst, best))) = (verdict, look, verdicts) {
                // The verdict the look would earn with the margin added: a look on a line, a
                // hair either side of it, never fails.
                let slack = if v == Verdict::NoEquivalent {
                    v
                } else {
                    look::verdict(score + margin)
                };
                if slack > worst {
                    worse.push(format!(
                        "{} where the baseline is at worst {}",
                        verdict_word(Some(v)),
                        verdict_word(Some(worst))
                    ));
                } else if v < best && score > b.range().map_or(f64::MAX, |r| r.1) {
                    better.push(format!(
                        "{} where the baseline is at best {}",
                        verdict_word(Some(v)),
                        verdict_word(Some(best))
                    ));
                }
            }
            let (what, how) = if !worse.is_empty() {
                (worse.join("; "), Move::Worse)
            } else if !unjudged.is_empty() {
                (unjudged.join("; "), Move::Unjudged)
            } else if !better.is_empty() {
                (better.join("; "), Move::Better)
            } else {
                continue;
            };
            checked.moved.push(Moved {
                sequence: label.clone(),
                index: e.index,
                kind: e.kind.clone(),
                target: e.target.clone(),
                start_ms: e.start_ms,
                end_ms: e.end_ms,
                was,
                now,
                what,
                how,
            });
        }
    }
    checked
}

/// A report of a check, in Markdown.
pub fn markdown(checked: &Checked, margin: f64) -> String {
    let mut out = String::from("## The corpus next to xLights' renders\n\n");
    if !checked.looks.is_empty() {
        out.push_str("| Sequence | Look now | Look in the baseline |\n|---|---|---|\n");
        for (label, now, range) in &checked.looks {
            let now = now.map_or("–".to_string(), |s| format!("{s:.1}"));
            let was = range.map_or("–".to_string(), |(a, b)| format!("{a:.1}–{b:.1}"));
            let _ = writeln!(out, "| {label} | {now} | {was} |");
        }
        out.push('\n');
    }
    if checked.passed() {
        let _ = writeln!(
            out,
            "{} effects held to the baseline: none looks worse by more than {margin} points under the lowest it \
             was measured at, or earns a worse verdict.\n",
            checked.held
        );
    }
    if !checked.broken.is_empty() {
        out.push_str("What can't be held to the baseline:\n\n");
        for line in &checked.broken {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }
    for (how, title) in [
        (Move::Worse, "Worse than the baseline (these fail the check)"),
        (
            Move::Unjudged,
            "No longer judged (not a failure by itself: the look it got is still held to the baseline)",
        ),
        (
            Move::Better,
            "Better than the baseline (accept it on purpose to raise the bar)",
        ),
    ] {
        let rows: Vec<&Moved> = checked.moved.iter().filter(|m| m.how == how).collect();
        if rows.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{title}:\n");
        out.push_str("| Sequence | # | Effect | On | When | Baseline | Now | What moved |\n|---|---|---|---|---|---|---|---|\n");
        for m in rows {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {:.1}–{:.1} s | {} | {} | {} |",
                m.sequence,
                m.index,
                m.kind,
                m.target,
                m.start_ms as f64 / 1000.0,
                m.end_ms as f64 / 1000.0,
                m.was,
                m.now,
                m.what
            );
        }
        out.push('\n');
    }
    if !checked.unrendered.is_empty() {
        let _ = writeln!(
            out,
            "No xLights reference yet, so not held to anything: {}.\n",
            checked.unrendered.join(", ")
        );
    }
    out
}

/// Reads the baseline in `corpus`, if there is one.
pub fn read(corpus: &Path) -> Result<Option<Baseline>, FidelityError> {
    let path = corpus.join(FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|source| FidelityError::Read {
        path: path.clone(),
        source,
    })?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| FidelityError::Manifest {
            path,
            reason: e.to_string(),
        })
}

/// Writes `baseline` to the baseline file in `corpus`.
pub fn write(corpus: &Path, baseline: &Baseline) -> Result<(), FidelityError> {
    let path = corpus.join(FILE);
    let text = serde_json::to_string_pretty(baseline).unwrap_or_default() + "\n";
    std::fs::write(&path, text).map_err(|source| FidelityError::Write { path, source })
}

/// The baseline before any sequence has a reference: it holds nothing, and says so.
pub fn empty() -> Baseline {
    Baseline {
        about: "No sequence of the corpus has an xLights reference yet, so there is nothing to hold \
                imports to. Render them (crates/pf-fidelity/corpus/README.md), then make this file with \
                `pf-fidelity regress --accept`. See docs/fidelity.md."
            .to_string(),
        tool: crate::TOOL.to_string(),
        margin: MARGIN,
        runs: RUNS,
        sequences: Vec::new(),
    }
}
