//! The scorecard: what was measured, per prop, per effect, and per kind of effect, with the
//! ranking of what to fix first and the audit of the importer's own labels.

use crate::layout::SetAside;
use crate::metrics::{Limits, Scores, Stats};
use pf_xlights::SequenceImportSummary;
use serde::Serialize;
use std::collections::BTreeMap;

/// The scorecard format's version.
pub const SCHEMA: u32 = 1;

/// How the importer labelled an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Label {
    Exact,
    Approximate,
    Placeholder,
}

/// How an effect can fairly be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Class {
    /// The same settings give the same pixels: compared pixel by pixel.
    Deterministic,
    /// Random placement: compared by what it shows over a window.
    Stochastic,
    /// It follows the music and the music wasn't read: set apart, not scored.
    NoMusic,
}

/// Whether effects that follow the music had it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Music {
    /// The sequence has none.
    None,
    Read,
    /// The sequence names music that wasn't found, or it was left unread.
    Missing,
}

/// Where a prop's channels are in PixelFlow's export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// On a controller PixelFlow imports.
    Controller,
    /// On a stand-in for channels PixelFlow has no controller for (see [`crate::layout`]).
    StandIn,
    /// Nowhere both sides have: not measured.
    None,
}

/// Which samples an effect is judged on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Basis {
    /// The pixels and frames it was the only effect on: the clean signal.
    Solo,
    /// Everything it was lit on, other effects mixed in: all there is for an effect that is
    /// (nearly) always layered.
    Layered,
}

/// Where the label audit draws its lines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLimits {
    /// An "exact" effect differs when its error is above this (0–255)...
    pub exact_mae: f64,
    /// ...or fewer than this share of its pixels are within the tolerance.
    pub exact_within: f64,
    /// An "exact" stochastic effect differs when its distribution distance is above this.
    pub exact_distribution: f64,
    /// An "approximate" effect matches when its error is at most this...
    pub match_mae: f64,
    /// ...and at least this share of its pixels are within the tolerance.
    pub match_within: f64,
    /// An "approximate" stochastic effect matches at or under this distribution distance.
    pub match_distribution: f64,
    /// Effects measured on less than this (pixels times seconds) aren't judged.
    pub min_pixel_seconds: f64,
    /// An effect is judged alone when at least this share of what it was lit on had no other
    /// effect (and that is enough to judge); otherwise on everything, layers mixed in.
    pub solo_share: f64,
    /// What a sparkling pixel that differs is taken to cost in error (0–255): an effect with
    /// sparkles is allowed this times the share of pixels its sparkles can change, and that
    /// share off its pixels within tolerance.
    pub sparkle_error: f64,
}

impl Default for AuditLimits {
    fn default() -> Self {
        Self {
            exact_mae: 8.0,
            exact_within: 0.9,
            exact_distribution: 0.15,
            match_mae: 4.0,
            match_within: 0.97,
            match_distribution: 0.05,
            min_pixel_seconds: 0.5,
            solo_share: 0.02,
            sparkle_error: 128.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub label: String,
    pub name: String,
    pub duration_ms: u64,
    pub frame_ms: u32,
    /// The importer's own counts.
    pub import: SequenceImportSummary,
    pub import_notes: Vec<String>,
    pub music: Music,
}

/// A stretch of the reference taken as not rendered (see `Limits::dark_run_ms`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub prop: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceInfo {
    pub channels: u32,
    pub frames: u32,
    pub step_ms: u32,
    /// The program that wrote it.
    pub producer: Option<String>,
    /// It holds only some channel ranges.
    pub sparse: bool,
    /// Where it is dark for so long, with PixelFlow lit, that it looks unrendered: left out of
    /// every score.
    pub left_out: Vec<Gap>,
    /// Those stretches as a share of everything that could otherwise be measured.
    pub left_out_share: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub tool: String,
    pub limits: Limits,
    pub audit: AuditLimits,
    /// Controller-side settings left out of the comparison (xLights' file is rendered before
    /// them).
    pub set_aside: SetAside,
    pub stand_in_controllers: usize,
}

/// The error with xLights' frames moved by `offset` frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OffsetError {
    pub offset: i64,
    pub mae: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub frame_ms: u32,
    pub reference_step_ms: u32,
    pub frames: u32,
    pub frames_compared: u32,
    /// Frames the reference has nothing for (it's shorter).
    pub frames_missing: u32,
    /// The offset the scores were measured at.
    pub frame_offset: i64,
    /// The whole-show error at nearby offsets, on a sample of frames. The lowest should be at
    /// `frameOffset`; anything else is a timing difference to look into.
    pub offsets: Vec<OffsetError>,
    pub best_offset: i64,
    /// For effects starting on dark pixels: how many frames later xLights first lit them than
    /// PixelFlow did, and how many effects had that difference.
    pub onset_deltas: BTreeMap<i64, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub props: usize,
    pub props_measured: usize,
    pub pixels: usize,
    pub pixels_measured: usize,
    pub pixels_on_controllers: usize,
    pub pixels_on_stand_ins: usize,
    pub unmeasured_props: Vec<String>,
    pub effects: usize,
    /// Effects with at least one measured pixel.
    pub effects_measured: usize,
    /// Effects measured somewhere with no other effect on the same pixels.
    pub effects_measured_alone: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropScore {
    pub name: String,
    pub pixels: usize,
    pub measured_pixels: usize,
    /// First and last channel compared, counting from 1.
    pub channels: Option<(u32, u32)>,
    pub on: Source,
    /// Over the frames either side lit the prop.
    pub scores: Option<Scores>,
    /// The error with xLights' pixels taken in the opposite order: far below `scores.mae` means
    /// the prop is wired the other way round.
    pub reversed_mae: Option<f64>,
}

/// Frames from an effect's start to the first one each side lit its pixels on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Onset {
    pub pixelflow: u32,
    pub xlights: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectScore {
    /// Its place in this list.
    pub index: usize,
    /// xLights' name for the effect.
    pub kind: String,
    pub pixelflow_kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The row's layer, counting from the bottom.
    pub layer: usize,
    /// The importer's label, and what it said differs.
    pub label: Label,
    pub notes: Vec<String>,
    pub class: Class,
    pub class_reason: Option<String>,
    /// The share of its lit pixels that random sparkles may make differ (0: it has none). The
    /// label audit allows a deterministic effect that much.
    pub sparkles: f64,
    pub pixels: usize,
    pub measured_pixels: usize,
    /// Where it was the only effect on the pixel.
    pub solo: Option<Scores>,
    /// Everywhere it was lit, whatever else was.
    pub layered: Option<Scores>,
    /// Its share of what it was lit on, in pixel-seconds: a pixel under three effects counts a
    /// third for each, so effects stacked on the same pixels don't each claim all of them.
    pub share: f64,
    pub onset: Option<Onset>,
    #[serde(skip)]
    pub(crate) solo_stats: Stats,
    #[serde(skip)]
    pub(crate) layered_stats: Stats,
}

impl EffectScore {
    /// The samples it's judged on, when it was measured at all.
    pub fn basis(&self, limits: &AuditLimits) -> Option<(Basis, &Scores)> {
        let layered = self.layered.as_ref()?;
        match &self.solo {
            Some(solo)
                if solo.pixel_seconds >= limits.solo_share * layered.pixel_seconds
                    && solo.pixel_seconds >= limits.min_pixel_seconds =>
            {
                Some((Basis::Solo, solo))
            }
            _ => Some((Basis::Layered, layered)),
        }
    }

    /// Its error: per channel (0–255) for a deterministic effect, the distribution distance
    /// (0–1) for a stochastic one. `None` when it wasn't measured or isn't scored.
    pub fn error(&self, limits: &AuditLimits) -> Option<f64> {
        let (_, scores) = self.basis(limits)?;
        match self.class {
            Class::Deterministic => Some(scores.mae),
            Class::Stochastic => Some(scores.distribution.distance),
            Class::NoMusic => None,
        }
    }
}

/// Effects by label.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Labels {
    pub exact: usize,
    pub approximate: usize,
    pub placeholder: usize,
}

impl Labels {
    fn add(&mut self, label: Label) {
        match label {
            Label::Exact => self.exact += 1,
            Label::Approximate => self.approximate += 1,
            Label::Placeholder => self.placeholder += 1,
        }
    }

    fn merge(&mut self, other: Labels) {
        self.exact += other.exact;
        self.approximate += other.approximate;
        self.placeholder += other.placeholder;
    }
}

/// One kind of xLights effect, over every instance compared the same way.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindScore {
    pub kind: String,
    pub class: Class,
    pub instances: usize,
    pub measured: usize,
    pub labels: Labels,
    /// Its instances' shares of what they were lit on, in pixel-seconds (see
    /// [`EffectScore::share`]).
    pub pixel_seconds: f64,
    /// The share of what its instances were lit on that no other effect was on.
    pub solo_share: f64,
    /// Its instances' errors weighted by what each was lit on: per channel (0–255) when
    /// deterministic, the distribution distance (0–1) when stochastic.
    pub error: Option<f64>,
    /// Error times pixel-seconds: how much there is to gain by fixing it.
    pub impact: f64,
    pub solo: Option<Scores>,
    pub layered: Option<Scores>,
    #[serde(skip)]
    solo_stats: Stats,
    #[serde(skip)]
    layered_stats: Stats,
    /// The shares of all instances, of those that have an error, and their errors times
    /// their shares.
    #[serde(skip)]
    sums: (f64, f64, f64),
}

/// A kind's place in a ranking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub kind: String,
    pub impact: f64,
    /// Its share of the ranking's whole impact.
    pub share: f64,
    pub error: f64,
    pub pixel_seconds: f64,
    pub instances: usize,
    /// The share of what it was lit on that no other effect was on: low means its error is
    /// mostly measured with other effects mixed in, and may be theirs.
    pub solo_share: f64,
}

/// An effect whose label and measurement disagree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// The sequence it's in (in a corpus scorecard).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    /// Its place in that sequence's `effects`.
    pub index: usize,
    pub kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub basis: Basis,
    pub mae: f64,
    pub within_tolerance: f64,
    pub lit_iou: f64,
    pub distribution: f64,
    pub notes: Vec<String>,
}

/// How a kind's "exact" instances measured.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactKind {
    pub kind: String,
    pub class: Class,
    /// Instances the importer called exact.
    pub exact: usize,
    /// Those measured well enough to judge.
    pub judged: usize,
    /// Those whose measurement says otherwise.
    pub differ: usize,
    /// Of those, the ones judged with nothing else on their pixels.
    pub differ_alone: usize,
}

/// Where the importer's labels and the measurements disagree.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelAudit {
    /// Labelled exact, measured pixel by pixel with nothing else on its pixels, and not: the
    /// list the labels must answer for.
    pub exact_but_differs: Vec<AuditEntry>,
    /// Labelled exact and different, but only ever measured under or over other effects: the
    /// difference may be theirs.
    pub exact_but_differs_layered: Vec<AuditEntry>,
    /// Labelled exact and random by nature, but showing something measurably different.
    pub exact_but_distribution_differs: Vec<AuditEntry>,
    /// Labelled approximate, yet matching.
    pub approximate_but_matches: Vec<AuditEntry>,
    pub exact_kinds: Vec<ExactKind>,
    /// Labelled exact but not judged: not measured, measured on too little, or set apart.
    pub exact_unjudged: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scorecard {
    pub schema: u32,
    pub sequence: SequenceInfo,
    pub reference: ReferenceInfo,
    pub method: Method,
    pub timing: Timing,
    pub coverage: Coverage,
    /// Every measured pixel, over the frames either side lit its prop.
    pub overall: Option<Scores>,
    /// Pixels lit (by either side) where no imported effect was: effects that weren't imported,
    /// or frames one side holds an effect longer.
    pub unattributed: Option<Scores>,
    pub props: Vec<PropScore>,
    pub kinds: Vec<KindScore>,
    /// Deterministic kinds, the most to gain first.
    pub ranking: Vec<Ranked>,
    /// Stochastic kinds, ranked by their distributions.
    pub stochastic_ranking: Vec<Ranked>,
    pub label_audit: LabelAudit,
    pub effects: Vec<EffectScore>,
    pub notes: Vec<String>,
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// The effects grouped into kinds (and how each kind is compared), biggest impact first.
fn kinds(effects: &[EffectScore], audit: &AuditLimits, frame_ms: u32, limits: &Limits) -> Vec<KindScore> {
    let mut by_kind: BTreeMap<(String, Class), KindScore> = BTreeMap::new();
    for effect in effects {
        let key = (effect.kind.clone(), effect.class);
        let kind = by_kind.entry(key).or_insert_with(|| KindScore {
            kind: effect.kind.clone(),
            class: effect.class,
            instances: 0,
            measured: 0,
            labels: Labels::default(),
            pixel_seconds: 0.0,
            solo_share: 0.0,
            error: None,
            impact: 0.0,
            solo: None,
            layered: None,
            solo_stats: Stats::new(frame_ms, limits),
            layered_stats: Stats::new(frame_ms, limits),
            sums: (0.0, 0.0, 0.0),
        });
        kind.instances += 1;
        kind.labels.add(effect.label);
        kind.solo_stats.merge(&effect.solo_stats);
        kind.layered_stats.merge(&effect.layered_stats);
        kind.measured += usize::from(effect.layered.is_some());
        kind.sums.0 += effect.share;
        if let Some(error) = effect.error(audit) {
            kind.sums.1 += effect.share;
            kind.sums.2 += error * effect.share;
        }
    }
    let mut kinds: Vec<KindScore> = by_kind
        .into_values()
        .map(|mut kind| {
            kind.finish();
            kind
        })
        .collect();
    sort_kinds(&mut kinds);
    kinds
}

/// Biggest impact first.
fn sort_kinds(kinds: &mut [KindScore]) {
    kinds.sort_by(|a, b| {
        b.impact
            .total_cmp(&a.impact)
            .then(a.kind.cmp(&b.kind))
            .then(a.class.cmp(&b.class))
    });
}

impl KindScore {
    /// Works out the scores from the sums.
    fn finish(&mut self) {
        self.solo = self.solo_stats.scores();
        self.layered = self.layered_stats.scores();
        self.solo_share = match (&self.solo, &self.layered) {
            (Some(solo), Some(layered)) if layered.pixel_seconds > 0.0 => {
                round(solo.pixel_seconds / layered.pixel_seconds)
            }
            _ => 0.0,
        };
        let (all, scored, errors) = self.sums;
        self.pixel_seconds = round(all);
        self.impact = round(errors);
        self.error = (scored > 0.0).then(|| round(errors / scored));
    }

    /// Adds the same kind from another sequence.
    fn merge(&mut self, other: &KindScore) {
        self.instances += other.instances;
        self.measured += other.measured;
        self.labels.merge(other.labels);
        self.solo_stats.merge(&other.solo_stats);
        self.layered_stats.merge(&other.layered_stats);
        self.sums.0 += other.sums.0;
        self.sums.1 += other.sums.1;
        self.sums.2 += other.sums.2;
        self.finish();
    }
}

/// The kinds of one class by impact, biggest first.
fn ranking(kinds: &[KindScore], class: Class) -> Vec<Ranked> {
    let ranked: Vec<&KindScore> = kinds
        .iter()
        .filter(|k| k.class == class && k.error.is_some())
        .collect();
    let total: f64 = ranked.iter().map(|k| k.impact).sum();
    ranked
        .into_iter()
        .map(|k| Ranked {
            kind: k.kind.clone(),
            impact: k.impact,
            share: if total > 0.0 { round(k.impact / total) } else { 0.0 },
            error: k.error.unwrap_or(0.0),
            pixel_seconds: k.pixel_seconds,
            instances: k.instances,
            solo_share: k.solo_share,
        })
        .collect()
}

/// Holds each effect's label against its measurement.
fn audit(effects: &[EffectScore], limits: &AuditLimits) -> LabelAudit {
    let mut audit = LabelAudit::default();
    let mut exact: BTreeMap<(String, Class), ExactKind> = BTreeMap::new();
    for effect in effects {
        let judged = effect
            .basis(limits)
            .filter(|(_, s)| s.pixel_seconds >= limits.min_pixel_seconds && effect.class != Class::NoMusic);
        if effect.label == Label::Exact {
            let kind = exact
                .entry((effect.kind.clone(), effect.class))
                .or_insert_with(|| ExactKind {
                    kind: effect.kind.clone(),
                    class: effect.class,
                    exact: 0,
                    judged: 0,
                    differ: 0,
                    differ_alone: 0,
                });
            kind.exact += 1;
            kind.judged += usize::from(judged.is_some());
            audit.exact_unjudged += usize::from(judged.is_none());
        }
        let Some((basis, scores)) = judged else {
            continue;
        };
        let entry = || AuditEntry {
            sequence: None,
            index: effect.index,
            kind: effect.kind.clone(),
            target: effect.target.clone(),
            start_ms: effect.start_ms,
            end_ms: effect.end_ms,
            basis,
            mae: scores.mae,
            within_tolerance: scores.within_tolerance,
            lit_iou: scores.lit_iou,
            distribution: scores.distribution.distance,
            notes: effect.notes.clone(),
        };
        let stochastic = effect.class == Class::Stochastic;
        // What its sparkles alone may cost.
        let (spared, allowed) = (effect.sparkles, effect.sparkles * limits.sparkle_error);
        let differs = if stochastic {
            scores.distribution.distance > limits.exact_distribution
        } else {
            scores.mae > limits.exact_mae + allowed || scores.within_tolerance < limits.exact_within - spared
        };
        let matches = if stochastic {
            scores.distribution.distance <= limits.match_distribution
        } else {
            scores.mae <= limits.match_mae + allowed
                && scores.within_tolerance >= limits.match_within - spared
        };
        match effect.label {
            Label::Exact if differs => {
                if let Some(kind) = exact.get_mut(&(effect.kind.clone(), effect.class)) {
                    kind.differ += 1;
                    kind.differ_alone += usize::from(basis == Basis::Solo);
                }
                if stochastic {
                    audit.exact_but_distribution_differs.push(entry());
                } else if basis == Basis::Solo {
                    audit.exact_but_differs.push(entry());
                } else {
                    audit.exact_but_differs_layered.push(entry());
                }
            }
            Label::Approximate if matches => audit.approximate_but_matches.push(entry()),
            _ => {}
        }
    }
    audit.exact_but_differs.sort_by(|a, b| b.mae.total_cmp(&a.mae));
    audit
        .exact_but_differs_layered
        .sort_by(|a, b| b.mae.total_cmp(&a.mae));
    audit
        .exact_but_distribution_differs
        .sort_by(|a, b| b.distribution.total_cmp(&a.distribution));
    audit.exact_kinds = exact.into_values().collect();
    audit
        .exact_kinds
        .sort_by(|a, b| b.differ.cmp(&a.differ).then(a.kind.cmp(&b.kind)));
    audit
}

impl Scorecard {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        sequence: SequenceInfo,
        reference: ReferenceInfo,
        method: Method,
        timing: Timing,
        coverage: Coverage,
        overall: Option<Scores>,
        unattributed: Option<Scores>,
        props: Vec<PropScore>,
        effects: Vec<EffectScore>,
        notes: Vec<String>,
    ) -> Self {
        let kinds = kinds(&effects, &method.audit, sequence.frame_ms, &method.limits);
        Self {
            schema: SCHEMA,
            ranking: ranking(&kinds, Class::Deterministic),
            stochastic_ranking: ranking(&kinds, Class::Stochastic),
            label_audit: audit(&effects, &method.audit),
            sequence,
            reference,
            method,
            timing,
            coverage,
            overall,
            unattributed,
            props,
            kinds,
            effects,
            notes,
        }
    }
}

/// One sequence's line in a corpus scorecard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    pub label: String,
    pub duration_ms: u64,
    pub import: SequenceImportSummary,
    pub pixels: usize,
    pub pixels_measured: usize,
    pub effects: usize,
    pub effects_measured: usize,
    pub frames_compared: u32,
    pub frames_missing: u32,
    pub best_offset: i64,
    pub overall: Option<Scores>,
    pub exact_judged: usize,
    pub exact_differ: usize,
    pub approximate_match: usize,
}

/// A sequence that couldn't be compared.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub label: String,
    pub error: String,
}

/// Every sequence of a corpus together.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusCard {
    pub schema: u32,
    pub tool: String,
    pub sequences: Vec<Headline>,
    pub failures: Vec<Failure>,
    pub kinds: Vec<KindScore>,
    pub ranking: Vec<Ranked>,
    pub stochastic_ranking: Vec<Ranked>,
    pub label_audit: LabelAudit,
}

impl CorpusCard {
    pub fn new(tool: &str) -> Self {
        Self {
            schema: SCHEMA,
            tool: tool.to_string(),
            sequences: Vec::new(),
            failures: Vec::new(),
            kinds: Vec::new(),
            ranking: Vec::new(),
            stochastic_ranking: Vec::new(),
            label_audit: LabelAudit::default(),
        }
    }

    /// Adds a sequence's scorecard.
    pub fn add(&mut self, card: &Scorecard) {
        let label = &card.sequence.label;
        let audit = &card.label_audit;
        self.sequences.push(Headline {
            label: label.clone(),
            duration_ms: card.sequence.duration_ms,
            import: card.sequence.import,
            pixels: card.coverage.pixels,
            pixels_measured: card.coverage.pixels_measured,
            effects: card.coverage.effects,
            effects_measured: card.coverage.effects_measured,
            frames_compared: card.timing.frames_compared,
            frames_missing: card.timing.frames_missing,
            best_offset: card.timing.best_offset,
            overall: card.overall,
            exact_judged: audit.exact_kinds.iter().map(|k| k.judged).sum(),
            exact_differ: audit.exact_kinds.iter().map(|k| k.differ).sum(),
            approximate_match: audit.approximate_but_matches.len(),
        });
        for kind in &card.kinds {
            match self
                .kinds
                .iter_mut()
                .find(|k| k.kind == kind.kind && k.class == kind.class)
            {
                Some(known) => known.merge(kind),
                None => self.kinds.push(kind.clone()),
            }
        }
        sort_kinds(&mut self.kinds);
        self.ranking = ranking(&self.kinds, Class::Deterministic);
        self.stochastic_ranking = ranking(&self.kinds, Class::Stochastic);

        let named = |entries: &[AuditEntry]| -> Vec<AuditEntry> {
            entries
                .iter()
                .cloned()
                .map(|mut e| {
                    e.sequence = Some(label.clone());
                    e
                })
                .collect()
        };
        let all = &mut self.label_audit;
        all.exact_but_differs.extend(named(&audit.exact_but_differs));
        all.exact_but_differs_layered
            .extend(named(&audit.exact_but_differs_layered));
        all.exact_but_distribution_differs
            .extend(named(&audit.exact_but_distribution_differs));
        all.approximate_but_matches
            .extend(named(&audit.approximate_but_matches));
        all.exact_unjudged += audit.exact_unjudged;
        for kind in &audit.exact_kinds {
            match all
                .exact_kinds
                .iter_mut()
                .find(|k| k.kind == kind.kind && k.class == kind.class)
            {
                Some(known) => {
                    known.exact += kind.exact;
                    known.judged += kind.judged;
                    known.differ += kind.differ;
                    known.differ_alone += kind.differ_alone;
                }
                None => all.exact_kinds.push(kind.clone()),
            }
        }
        all.exact_but_differs.sort_by(|a, b| b.mae.total_cmp(&a.mae));
        all.exact_but_differs_layered
            .sort_by(|a, b| b.mae.total_cmp(&a.mae));
        all.exact_but_distribution_differs
            .sort_by(|a, b| b.distribution.total_cmp(&a.distribution));
        all.exact_kinds
            .sort_by(|a, b| b.differ.cmp(&a.differ).then(a.kind.cmp(&b.kind)));
    }

    /// Notes a sequence that couldn't be compared.
    pub fn fail(&mut self, label: &str, error: &str) {
        self.failures.push(Failure {
            label: label.to_string(),
            error: error.to_string(),
        });
    }
}
