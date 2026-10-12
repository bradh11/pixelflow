//! The scorecard: how an imported sequence looks next to xLights' own render, per effect, per
//! kind of effect, and over the whole sequence; what to improve first; and how the importer's
//! labels held up. The pixel-for-pixel measures are its diagnostics.

use crate::layout::SetAside;
use crate::look::{Chance, Component, Components, Look, MATCHES};
use crate::metrics::{Limits, Scores, Stats};
use pf_xlights::SequenceImportSummary;
pub use pf_xlights::sequence::{ByVerdict, Verdict};
use serde::Serialize;
use std::collections::BTreeMap;

/// The scorecard format's version.
pub const SCHEMA: u32 = 4;

/// What the importer carried over of an effect (its own bookkeeping; what it tells people is
/// the [`Verdict`] it expects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Label {
    Exact,
    Approximate,
    Placeholder,
}

/// How an effect can fairly be compared pixel for pixel (a diagnostic: the look score treats
/// every effect alike).
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

/// Where the lines are drawn when effects are judged.
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
    /// An effect's error is judged alone when at least this share of what it was lit on had no
    /// other effect (and that is enough to judge); otherwise on everything, layers mixed in.
    pub solo_share: f64,
    /// An effect's look is judged alone when at least this share of what it was lit on had no
    /// other effect: a look needs more of the picture than an error does.
    pub alone_share: f64,
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
            alone_share: 0.1,
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
    /// Where it is dark for so long, with PixelFlow lit, that it looks unrendered, and where
    /// a prop that ends that way had been cut short already: left out of every score.
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
    /// The look score at or above which two renders match, and at or above which they're close.
    pub matches: f64,
    pub close: f64,
    /// Every effect was judged moment by moment against xLights' render alone, whatever chance
    /// decides in it (the numbers from before effects that draw on chance were held against
    /// PixelFlow's own renders from other seeds).
    pub by_moment: bool,
    /// How many more times PixelFlow rendered the sequence, each from other seeds, for those
    /// effects to be held against (0 when none draws on chance).
    pub other_seeds: usize,
    /// Where two of PixelFlow's own renders are less alike than this in some part of an
    /// effect's look, the look has too little to go on to say much.
    pub little: f64,
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
    /// How the prop looks over the frames either side lit it, whatever effects were on it.
    pub look: Option<Look>,
    /// The same frames pixel for pixel.
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

/// How an effect was judged to look.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Judged {
    /// What its look score earns; "no equivalent" for a placeholder, whatever it scored.
    pub verdict: Verdict,
    pub basis: Basis,
    /// The look score it was judged on, 0–100.
    pub score: f64,
    pub weakest: Component,
    /// What a viewer would notice first; `None` when the two match.
    pub differs: Option<String>,
    /// What chance decides in it, as it was judged: "nothing" when it was held moment by
    /// moment against xLights' render alone, anything else when it was held against
    /// PixelFlow's own renders from other seeds as well.
    pub chance: Chance,
    /// What it scores and earns held moment by moment against xLights' render alone; `None`
    /// when that is how it was judged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_moment: Option<Scored>,
}

/// A look score and the verdict it earns.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Scored {
    pub score: f64,
    pub verdict: Verdict,
}

/// A look next to PixelFlow's other seeds that says too little to judge its effect by: the
/// effect counts as not judged, and this is what the look came to all the same.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaysLittle {
    /// Why: too little light in either render, or the part of the look PixelFlow's own renders
    /// have too little of in common from one seed to the next ([`Look::says_little`]).
    pub why: String,
    pub basis: Basis,
    /// The look score it got, and the verdict that would have earned.
    pub score: f64,
    pub verdict: Verdict,
    /// What chance decides in it, as it was looked at.
    pub chance: Chance,
    /// How alike two of PixelFlow's own renders are in the part of the look they have least of
    /// in common, 0–1, and that part.
    pub itself: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub least: Option<Component>,
    /// What it scores and earns held moment by moment against xLights' render alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_moment: Option<Scored>,
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
    /// How the importer expects it to look, what it carried over, and what it said differs.
    pub expected: Verdict,
    pub label: Label,
    pub notes: Vec<String>,
    /// How it was judged to look; `None` when it wasn't measured on enough to say, or its
    /// look says too little (`says_little`).
    pub look: Option<Judged>,
    /// The look it got, when that says too little to judge it by. Such an effect is not judged:
    /// it is in no share, score, or count of the effects judged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub says_little: Option<SaysLittle>,
    /// What chance decides in it, from its kind and settings, and why (see
    /// [`crate::chance`]). A look of it can be held against PixelFlow's other seeds without
    /// that ([`Look::chance`]): where an effect that draws on chance shares its pixels.
    pub chance: Chance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chance_reason: Option<String>,
    /// Its look where it was the only effect on the pixel, and everywhere it was lit: the looks
    /// it is judged on.
    pub look_solo: Option<Look>,
    pub look_layered: Option<Look>,
    /// The same held moment by moment against xLights' render alone, when chance decides
    /// something in it and the looks above are next to PixelFlow's other seeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_solo_by_moment: Option<Look>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_layered_by_moment: Option<Look>,
    pub class: Class,
    pub class_reason: Option<String>,
    /// The share of its lit pixels that random sparkles may make differ (0: it has none). The
    /// pixel audit allows a deterministic effect that much.
    pub sparkles: f64,
    pub pixels: usize,
    pub measured_pixels: usize,
    /// Pixel for pixel, where it was the only effect on the pixel.
    pub solo: Option<Scores>,
    /// Pixel for pixel, everywhere it was lit, whatever else was.
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
    /// The samples its error is judged on, when it was measured at all.
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

    /// The look it's judged on: alone where it has enough of the picture to itself, layered
    /// otherwise. `None` when it was measured on too little, follows music that wasn't read, or
    /// its look says too little to judge it by ([`EffectScore::little`]).
    pub fn look_basis(&self, limits: &AuditLimits) -> Option<(Basis, &Look)> {
        self.look_measured(limits)
            .filter(|(_, look)| self.little_of(look).is_none())
    }

    /// The look it would be judged on, whether or not that says enough to judge it by.
    pub fn look_measured(&self, limits: &AuditLimits) -> Option<(Basis, &Look)> {
        self.basis_of(limits, self.look_solo.as_ref(), self.look_layered.as_ref())
    }

    /// Why `look` says too little to judge this effect by, when it does. A placeholder is "no
    /// equivalent" by what it is, whatever its look says or doesn't.
    fn little_of(&self, look: &Look) -> Option<String> {
        if self.label == Label::Placeholder {
            None
        } else {
            look.says_little()
        }
    }

    /// The look it got and why that says too little to judge it by, when that is so.
    pub fn little(&self, limits: &AuditLimits) -> Option<(Basis, &Look, String)> {
        let (basis, look) = self.look_measured(limits)?;
        Some((basis, look, self.little_of(look)?))
    }

    /// The same held moment by moment against xLights' render alone: another look when chance
    /// decides something in the effect, the one it's judged on otherwise.
    pub fn look_basis_by_moment(&self, limits: &AuditLimits) -> Option<(Basis, &Look)> {
        self.basis_of(
            limits,
            self.look_solo_by_moment.as_ref().or(self.look_solo.as_ref()),
            self.look_layered_by_moment
                .as_ref()
                .or(self.look_layered.as_ref()),
        )
    }

    fn basis_of<'a>(
        &self,
        limits: &AuditLimits,
        solo: Option<&'a Look>,
        layered: Option<&'a Look>,
    ) -> Option<(Basis, &'a Look)> {
        let layered = layered?;
        if self.class == Class::NoMusic || layered.pixel_seconds < limits.min_pixel_seconds {
            return None;
        }
        match solo {
            Some(solo)
                if solo.pixel_seconds >= limits.alone_share * layered.pixel_seconds
                    && solo.pixel_seconds >= limits.min_pixel_seconds =>
            {
                Some((Basis::Solo, solo))
            }
            _ => Some((Basis::Layered, layered)),
        }
    }

    /// The verdict a look of this effect's earns: a placeholder has no equivalent whatever it
    /// scores.
    fn verdict_of(&self, look: &Look) -> Verdict {
        if self.label == Label::Placeholder {
            Verdict::NoEquivalent
        } else {
            look.verdict
        }
    }

    /// The verdict it earned and the look it earned it on.
    fn judged(&self, limits: &AuditLimits) -> Option<(Verdict, &Look)> {
        let (_, look) = self.look_basis(limits)?;
        Some((self.verdict_of(look), look))
    }

    /// The same compared moment by moment throughout.
    fn judged_by_moment(&self, limits: &AuditLimits) -> Option<(Verdict, &Look)> {
        let (_, look) = self.look_basis_by_moment(limits)?;
        Some((self.verdict_of(look), look))
    }

    fn judge(&mut self, limits: &AuditLimits) {
        let by_moment = self.look_basis_by_moment(limits).map(|(_, look)| Scored {
            score: look.score,
            verdict: self.verdict_of(look),
        });
        self.look = self.look_basis(limits).map(|(basis, look)| Judged {
            verdict: self.verdict_of(look),
            basis,
            score: look.score,
            weakest: look.weakest,
            differs: look.differs(),
            chance: look.chance,
            by_moment: by_moment.filter(|_| look.chance != Chance::Nothing),
        });
        self.says_little = self.little(limits).map(|(basis, look, why)| SaysLittle {
            why,
            basis,
            score: look.score,
            verdict: look.verdict,
            chance: look.chance,
            itself: look.itself.unwrap_or(0.0),
            least: look.least,
            by_moment,
        });
    }

    /// How long it runs, in seconds.
    pub(crate) fn seconds(&self) -> f64 {
        self.end_ms.saturating_sub(self.start_ms) as f64 / 1000.0
    }
}

/// Effects by what the importer carried over.
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

fn components_of(sums: &[f64; 6], weight: f64) -> Components {
    let at = |c: Component| round(sums[c as usize] / weight);
    Components {
        coverage: at(Component::Coverage),
        brightness: at(Component::Brightness),
        color: at(Component::Color),
        pattern: at(Component::Pattern),
        motion: at(Component::Motion),
        rhythm: at(Component::Rhythm),
    }
}

fn shares(amounts: &ByVerdict<f64>) -> ByVerdict<f64> {
    let total: f64 = Verdict::ALL.iter().map(|&v| amounts.get(v)).sum();
    let mut shares = ByVerdict::default();
    if total > 0.0 {
        for verdict in Verdict::ALL {
            shares.add(verdict, round(amounts.get(verdict) / total));
        }
    }
    shares
}

/// The headline: how much of the sequence comes across looking the same.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookSummary {
    /// The share of effect-time (each judged effect's length, added up) that got each verdict.
    pub time: ByVerdict<f64>,
    /// The same with each effect counting for the pixels it was on as well as its length.
    pub pixels: ByVerdict<f64>,
    /// Effects judged, by verdict.
    pub effects: ByVerdict<usize>,
    /// The look score over the effects judged, 0–100, weighted by effect-time; and its parts.
    pub score: Option<f64>,
    pub components: Option<Components>,
    /// The look score of the picture as a whole: every prop over the frames either side lit
    /// it, whatever effects were on it, weighted by pixel-seconds.
    pub as_seen: Option<f64>,
    /// Effects judged, of how many; the seconds of effect that is; and the share of all
    /// effect-time that couldn't be judged.
    pub judged: usize,
    pub of: usize,
    pub seconds: f64,
    pub not_judged: f64,
    /// Of what couldn't be judged, the effects that were measured and whose look next to
    /// PixelFlow's other seeds says too little to judge them by ([`EffectScore::says_little`]):
    /// how many, the seconds of effect that is, and that as a share of all effect-time.
    pub says_little: SaidLittle,
    #[serde(skip)]
    sums: SummarySums,
}

/// How much was measured and not judged, for a look that says too little.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SaidLittle {
    pub effects: usize,
    pub seconds: f64,
    pub share: f64,
}

#[derive(Debug, Clone, Default)]
struct SummarySums {
    seconds: ByVerdict<f64>,
    pixel_seconds: ByVerdict<f64>,
    score: f64,
    components: [f64; 6],
    all_seconds: f64,
    little_seconds: f64,
    seen: (f64, f64),
}

impl LookSummary {
    /// Adds an effect with the verdict it earned and the look it earned it on, when it was
    /// judged; `little` when it wasn't for a look that says too little.
    fn add(&mut self, effect: &EffectScore, judged: Option<(Verdict, &Look)>, little: bool) {
        self.of += 1;
        self.sums.all_seconds += effect.seconds();
        let Some((verdict, look)) = judged else {
            if little {
                self.says_little.effects += 1;
                self.sums.little_seconds += effect.seconds();
            }
            return;
        };
        self.judged += 1;
        self.effects.add(verdict, 1);
        self.sums.seconds.add(verdict, effect.seconds());
        self.sums.pixel_seconds.add(verdict, effect.share);
        self.sums.score += look.score * effect.seconds();
        for component in Component::ALL {
            self.sums.components[component as usize] += look.components.get(component) * effect.seconds();
        }
    }

    fn add_prop(&mut self, prop: &PropScore) {
        if let Some(look) = &prop.look {
            self.sums.seen.0 += look.score * look.pixel_seconds;
            self.sums.seen.1 += look.pixel_seconds;
        }
    }

    fn merge(&mut self, other: &LookSummary) {
        self.judged += other.judged;
        self.of += other.of;
        self.effects.merge(&other.effects);
        self.sums.seconds.merge(&other.sums.seconds);
        self.sums.pixel_seconds.merge(&other.sums.pixel_seconds);
        self.sums.score += other.sums.score;
        for (sum, more) in self.sums.components.iter_mut().zip(other.sums.components) {
            *sum += more;
        }
        self.sums.all_seconds += other.sums.all_seconds;
        self.says_little.effects += other.says_little.effects;
        self.sums.little_seconds += other.sums.little_seconds;
        self.sums.seen.0 += other.sums.seen.0;
        self.sums.seen.1 += other.sums.seen.1;
        self.finish();
    }

    fn finish(&mut self) {
        let sums = &self.sums;
        let seconds: f64 = Verdict::ALL.iter().map(|&v| sums.seconds.get(v)).sum();
        self.time = shares(&sums.seconds);
        self.pixels = shares(&sums.pixel_seconds);
        self.seconds = round(seconds);
        self.score = (seconds > 0.0).then(|| round(sums.score / seconds));
        self.components = (seconds > 0.0).then(|| components_of(&sums.components, seconds));
        self.as_seen = (sums.seen.1 > 0.0).then(|| round(sums.seen.0 / sums.seen.1));
        self.not_judged = if sums.all_seconds > seconds {
            round(1.0 - seconds / sums.all_seconds)
        } else {
            0.0
        };
        self.says_little.seconds = round(sums.little_seconds);
        self.says_little.share = if sums.all_seconds > 0.0 {
            round(sums.little_seconds / sums.all_seconds)
        } else {
            0.0
        };
    }
}

/// How one kind of xLights effect looks, over the instances judged.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindLook {
    /// The look score, 0–100, weighted by what each instance was lit on; and its parts.
    pub score: f64,
    /// The same with every instance held moment by moment against xLights' render alone.
    pub by_moment: f64,
    pub components: Components,
    /// The part that agrees least, and what a viewer would notice of it (from the instance
    /// that loses most to it).
    pub weakest: Component,
    pub differs: Option<String>,
    pub verdicts: ByVerdict<usize>,
    pub judged: usize,
    /// Those judged where nothing else was on their pixels.
    pub judged_alone: usize,
    /// What there is to gain: how far each instance falls short of matching (see
    /// [`shortfall`]), times its pixel-seconds.
    pub impact: f64,
    pub pixel_seconds: f64,
}

/// How far a look score falls short of matching, as a share of the whole scale: nothing for a
/// look that matches, however long it runs.
fn shortfall(score: f64) -> f64 {
    (MATCHES - score).max(0.0) / 100.0
}

#[derive(Debug, Clone, Default)]
struct KindLookSums {
    weight: f64,
    score: f64,
    by_moment: f64,
    components: [f64; 6],
    impact: f64,
    verdicts: ByVerdict<usize>,
    judged: usize,
    alone: usize,
    /// By component: the instance that doesn't match and loses most to it, and what a viewer
    /// would notice of that part of its look.
    differs: [Option<(f64, String)>; 6],
}

impl KindLookSums {
    fn add(&mut self, effect: &EffectScore, limits: &AuditLimits) {
        let (Some(judged), Some((basis, look))) = (&effect.look, effect.look_basis(limits)) else {
            return;
        };
        let impact = shortfall(look.score) * effect.share;
        let by_moment = effect
            .look_basis_by_moment(limits)
            .map_or(look.score, |(_, l)| l.score);
        self.weight += effect.share;
        self.score += look.score * effect.share;
        self.by_moment += by_moment * effect.share;
        for component in Component::ALL {
            self.components[component as usize] += look.components.get(component) * effect.share;
        }
        self.impact += impact;
        self.verdicts.add(judged.verdict, 1);
        self.judged += 1;
        self.alone += usize::from(basis == Basis::Solo);
        if look.verdict != Verdict::Matches {
            for component in Component::ALL {
                let lost = (1.0 - look.components.get(component)) * effect.share;
                let worst = &mut self.differs[component as usize];
                if lost > 0.0 && worst.as_ref().is_none_or(|(most, _)| lost > *most) {
                    *worst = Some((lost, look.describe(component)));
                }
            }
        }
    }

    fn merge(&mut self, other: &KindLookSums) {
        self.weight += other.weight;
        self.score += other.score;
        self.by_moment += other.by_moment;
        for (sum, more) in self.components.iter_mut().zip(other.components) {
            *sum += more;
        }
        self.impact += other.impact;
        self.verdicts.merge(&other.verdicts);
        self.judged += other.judged;
        self.alone += other.alone;
        for (worst, theirs) in self.differs.iter_mut().zip(&other.differs) {
            if let Some((impact, _)) = theirs
                && worst.as_ref().is_none_or(|(most, _)| impact > most)
            {
                worst.clone_from(theirs);
            }
        }
    }

    fn look(&self) -> Option<KindLook> {
        if self.judged == 0 || self.weight <= 0.0 {
            return None;
        }
        let components = components_of(&self.components, self.weight);
        let weakest = components.weakest();
        let differs = self.differs[weakest as usize].clone();
        Some(KindLook {
            score: round(self.score / self.weight),
            by_moment: round(self.by_moment / self.weight),
            components,
            weakest,
            differs: differs.map(|(_, text)| text),
            verdicts: self.verdicts,
            judged: self.judged,
            judged_alone: self.alone,
            impact: round(self.impact),
            pixel_seconds: round(self.weight),
        })
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
    /// Those measured and not judged, for a look that says too little.
    pub says_little: usize,
    /// How the instances judged look.
    pub look: Option<KindLook>,
    pub labels: Labels,
    /// Its instances' shares of what they were lit on, in pixel-seconds (see
    /// [`EffectScore::share`]).
    pub pixel_seconds: f64,
    /// The share of what its instances were lit on that no other effect was on.
    pub solo_share: f64,
    /// Its instances' errors weighted by what each was lit on: per channel (0–255) when
    /// deterministic, the distribution distance (0–1) when stochastic.
    pub error: Option<f64>,
    /// Error times pixel-seconds.
    pub impact: f64,
    pub solo: Option<Scores>,
    pub layered: Option<Scores>,
    #[serde(skip)]
    look_sums: KindLookSums,
    #[serde(skip)]
    solo_stats: Stats,
    #[serde(skip)]
    layered_stats: Stats,
    /// The shares of all instances, of those that have an error, and their errors times
    /// their shares.
    #[serde(skip)]
    sums: (f64, f64, f64),
}

/// A kind's place in what to improve first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Improve {
    pub kind: String,
    /// How far each instance falls short of matching, times its pixel-seconds.
    pub impact: f64,
    /// Its share of the whole ranking's impact.
    pub share: f64,
    pub score: f64,
    /// The part of its look that agrees least, that part's score (0–1), and what a viewer
    /// would notice.
    pub weakest: Component,
    pub component: f64,
    pub differs: Option<String>,
    pub verdicts: ByVerdict<usize>,
    pub pixel_seconds: f64,
    pub judged: usize,
    /// Those judged where nothing else was on their pixels: few of them, and the difference
    /// may belong to what the kind is layered with.
    pub judged_alone: usize,
}

/// A kind's place in a pixel-for-pixel ranking.
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

/// An effect whose label and pixel-for-pixel measurement disagree.
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

/// Where the importer's bookkeeping and the pixel-for-pixel measurements disagree.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelAudit {
    /// Carried over exactly, measured pixel by pixel with nothing else on its pixels, and not
    /// the same pixels.
    pub exact_but_differs: Vec<AuditEntry>,
    /// The same, but only ever measured under or over other effects: the difference may be
    /// theirs.
    pub exact_but_differs_layered: Vec<AuditEntry>,
    /// Carried over exactly and random by nature, but showing something measurably different.
    pub exact_but_distribution_differs: Vec<AuditEntry>,
    /// Carried over with differences, yet the same pixels.
    pub approximate_but_matches: Vec<AuditEntry>,
    pub exact_kinds: Vec<ExactKind>,
    /// Carried over exactly but not judged: not measured, measured on too little, or set apart.
    pub exact_unjudged: usize,
}

/// The effects of one kind the importer noted the same things about, and how they were judged.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelRow {
    pub kind: String,
    /// What the importer said differs: the condition its label rests on.
    pub notes: Vec<String>,
    pub expected: Verdict,
    pub effects: usize,
    /// Those judged, by the verdict each earned; and the ones judged alone among them.
    pub measured: ByVerdict<usize>,
    pub measured_alone: ByVerdict<usize>,
    /// Those measured and not judged, for a look that says too little.
    pub says_little: usize,
    /// Their look score, weighted by effect-time.
    pub score: Option<f64>,
    #[serde(skip)]
    sums: (f64, f64),
}

impl LabelRow {
    /// Effects judged.
    pub fn judged(&self) -> usize {
        Verdict::ALL.iter().map(|&v| self.measured.get(v)).sum()
    }

    /// What most of its effects earned, by number: those judged alone when there are any (the
    /// clean signal), all of them otherwise. The worse of two verdicts on a tie.
    pub fn earned(&self) -> Option<Verdict> {
        let alone: usize = Verdict::ALL.iter().map(|&v| self.measured_alone.get(v)).sum();
        let counts = if alone > 0 {
            &self.measured_alone
        } else {
            &self.measured
        };
        Verdict::ALL
            .into_iter()
            .filter(|&v| counts.get(v) > 0)
            .max_by_key(|&v| (counts.get(v), v))
    }

    fn merge(&mut self, other: &LabelRow) {
        self.effects += other.effects;
        self.measured.merge(&other.measured);
        self.measured_alone.merge(&other.measured_alone);
        self.says_little += other.says_little;
        self.sums.0 += other.sums.0;
        self.sums.1 += other.sums.1;
        self.score = (self.sums.1 > 0.0).then(|| round(self.sums.0 / self.sums.1));
    }
}

/// An effect that looks other than the importer expected.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelMiss {
    /// The sequence it's in (in a corpus scorecard).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    /// Its place in that sequence's `effects`.
    pub index: usize,
    pub kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub expected: Verdict,
    pub measured: Verdict,
    pub basis: Basis,
    pub score: f64,
    pub weakest: Component,
    pub differs: Option<String>,
    pub notes: Vec<String>,
    /// What it was lit on, in pixel-seconds.
    pub pixel_seconds: f64,
}

/// The importer's labels held against how the effects were judged to look.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelCheck {
    /// Effects judged, and how many earned the verdict the importer expected.
    pub judged: usize,
    pub agree: usize,
    pub rows: Vec<LabelRow>,
    /// Expected to look better than they do, judged with nothing else on their pixels: labels
    /// to correct, or effects to fix. Most pixel-seconds first.
    pub optimistic: Vec<LabelMiss>,
    /// The same, judged with other effects mixed in: the difference may be theirs.
    pub optimistic_layered: Vec<LabelMiss>,
    /// Expected to look worse than they do.
    pub pessimistic: Vec<LabelMiss>,
}

impl LabelCheck {
    fn of(effects: &[EffectScore]) -> Self {
        let mut check = LabelCheck::default();
        let mut rows: BTreeMap<(String, Vec<String>, Verdict), LabelRow> = BTreeMap::new();
        for effect in effects {
            let mut notes = effect.notes.clone();
            notes.sort();
            let row = rows
                .entry((effect.kind.clone(), notes.clone(), effect.expected))
                .or_insert_with(|| LabelRow {
                    kind: effect.kind.clone(),
                    notes,
                    expected: effect.expected,
                    effects: 0,
                    measured: ByVerdict::default(),
                    measured_alone: ByVerdict::default(),
                    says_little: 0,
                    score: None,
                    sums: (0.0, 0.0),
                });
            row.effects += 1;
            row.says_little += usize::from(effect.says_little.is_some());
            let Some(judged) = &effect.look else {
                continue;
            };
            row.measured.add(judged.verdict, 1);
            if judged.basis == Basis::Solo {
                row.measured_alone.add(judged.verdict, 1);
            }
            row.sums.0 += judged.score * effect.seconds();
            row.sums.1 += effect.seconds();
            check.judged += 1;
            if judged.verdict == effect.expected {
                check.agree += 1;
                continue;
            }
            let miss = LabelMiss {
                sequence: None,
                index: effect.index,
                kind: effect.kind.clone(),
                target: effect.target.clone(),
                start_ms: effect.start_ms,
                end_ms: effect.end_ms,
                expected: effect.expected,
                measured: judged.verdict,
                basis: judged.basis,
                score: judged.score,
                weakest: judged.weakest,
                differs: judged.differs.clone(),
                notes: effect.notes.clone(),
                pixel_seconds: effect.share,
            };
            if judged.verdict < effect.expected {
                check.pessimistic.push(miss);
            } else if judged.basis == Basis::Solo {
                check.optimistic.push(miss);
            } else {
                check.optimistic_layered.push(miss);
            }
        }
        check.rows = rows
            .into_values()
            .map(|mut row| {
                row.score = (row.sums.1 > 0.0).then(|| round(row.sums.0 / row.sums.1));
                row
            })
            .collect();
        check.sort();
        check
    }

    fn sort(&mut self) {
        for list in [
            &mut self.optimistic,
            &mut self.optimistic_layered,
            &mut self.pessimistic,
        ] {
            list.sort_by(|a, b| b.pixel_seconds.total_cmp(&a.pixel_seconds));
        }
        self.rows.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then(a.expected.cmp(&b.expected))
                .then(a.notes.cmp(&b.notes))
        });
    }

    fn merge(&mut self, other: &LabelCheck, sequence: &str) {
        self.judged += other.judged;
        self.agree += other.agree;
        for row in &other.rows {
            match self
                .rows
                .iter_mut()
                .find(|r| r.kind == row.kind && r.notes == row.notes && r.expected == row.expected)
            {
                Some(known) => known.merge(row),
                None => self.rows.push(row.clone()),
            }
        }
        let named = |misses: &[LabelMiss]| -> Vec<LabelMiss> {
            misses
                .iter()
                .cloned()
                .map(|mut miss| {
                    miss.sequence = Some(sequence.to_string());
                    miss
                })
                .collect()
        };
        self.optimistic.extend(named(&other.optimistic));
        self.optimistic_layered.extend(named(&other.optimistic_layered));
        self.pessimistic.extend(named(&other.pessimistic));
        self.sort();
    }
}

/// The pixel-for-pixel measures: for finding structural bugs (a prop wired backwards, a frame
/// of delay, a layer mixed wrongly), not for saying how an import looks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// Every measured pixel, over the frames either side lit its prop.
    pub overall: Option<Scores>,
    /// Pixels lit (by either side) where no imported effect was: effects that weren't imported,
    /// or frames one side holds an effect longer.
    pub unattributed: Option<Scores>,
    /// Deterministic kinds by error times pixel-seconds.
    pub ranking: Vec<Ranked>,
    /// Stochastic kinds, ranked by their distributions.
    pub stochastic_ranking: Vec<Ranked>,
    pub label_audit: LabelAudit,
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
    /// How much of the sequence comes across looking the same.
    pub look: LookSummary,
    /// The same with every effect held moment by moment against xLights' render alone: what
    /// `look` was before effects that draw on chance were held against PixelFlow's own renders
    /// from other seeds. The two differ by those effects alone.
    pub look_by_moment: LookSummary,
    /// Kinds of effect by what there is to gain in look, the most first.
    pub improve: Vec<Improve>,
    /// The importer's labels held against the looks measured.
    pub labels: LabelCheck,
    pub kinds: Vec<KindScore>,
    pub effects: Vec<EffectScore>,
    pub props: Vec<PropScore>,
    pub diagnostics: Diagnostics,
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
            says_little: 0,
            look: None,
            labels: Labels::default(),
            pixel_seconds: 0.0,
            solo_share: 0.0,
            error: None,
            impact: 0.0,
            solo: None,
            layered: None,
            look_sums: KindLookSums::default(),
            solo_stats: Stats::new(frame_ms, limits),
            layered_stats: Stats::new(frame_ms, limits),
            sums: (0.0, 0.0, 0.0),
        });
        kind.instances += 1;
        kind.labels.add(effect.label);
        kind.look_sums.add(effect, audit);
        kind.solo_stats.merge(&effect.solo_stats);
        kind.layered_stats.merge(&effect.layered_stats);
        kind.measured += usize::from(effect.layered.is_some());
        kind.says_little += usize::from(effect.says_little.is_some());
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
        self.look = self.look_sums.look();
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
        self.says_little += other.says_little;
        self.labels.merge(other.labels);
        self.look_sums.merge(&other.look_sums);
        self.solo_stats.merge(&other.solo_stats);
        self.layered_stats.merge(&other.layered_stats);
        self.sums.0 += other.sums.0;
        self.sums.1 += other.sums.1;
        self.sums.2 += other.sums.2;
        self.finish();
    }
}

/// What to improve first: the kinds (however they're compared pixel for pixel) by how far their
/// looks fall short of matching, the most first.
fn improve(kinds: &[KindScore]) -> Vec<Improve> {
    let mut by_kind: BTreeMap<&str, KindLookSums> = BTreeMap::new();
    for kind in kinds {
        by_kind.entry(&kind.kind).or_default().merge(&kind.look_sums);
    }
    let looks: Vec<(&str, KindLook)> = by_kind
        .into_iter()
        .filter_map(|(kind, sums)| Some((kind, sums.look()?)))
        .collect();
    let total: f64 = looks.iter().map(|(_, look)| look.impact).sum();
    let mut ranked: Vec<Improve> = looks
        .into_iter()
        .map(|(kind, look)| Improve {
            kind: kind.to_string(),
            impact: look.impact,
            share: if total > 0.0 {
                round(look.impact / total)
            } else {
                0.0
            },
            score: look.score,
            weakest: look.weakest,
            component: look.components.get(look.weakest),
            differs: look.differs,
            verdicts: look.verdicts,
            pixel_seconds: look.pixel_seconds,
            judged: look.judged,
            judged_alone: look.judged_alone,
        })
        .collect();
    ranked.sort_by(|a, b| b.impact.total_cmp(&a.impact).then(a.kind.cmp(&b.kind)));
    ranked
}

/// The kinds of one class by pixel-for-pixel impact, biggest first.
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

/// Holds what the importer carried over of each effect against its pixel-for-pixel measurement.
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
        mut effects: Vec<EffectScore>,
        notes: Vec<String>,
    ) -> Self {
        let (mut look, mut look_by_moment) = (LookSummary::default(), LookSummary::default());
        for effect in &mut effects {
            effect.judge(&method.audit);
            look.add(effect, effect.judged(&method.audit), effect.says_little.is_some());
            look_by_moment.add(effect, effect.judged_by_moment(&method.audit), false);
        }
        for prop in &props {
            look.add_prop(prop);
            look_by_moment.add_prop(prop);
        }
        look.finish();
        look_by_moment.finish();
        let kinds = kinds(&effects, &method.audit, sequence.frame_ms, &method.limits);
        Self {
            schema: SCHEMA,
            look,
            look_by_moment,
            improve: improve(&kinds),
            labels: LabelCheck::of(&effects),
            diagnostics: Diagnostics {
                overall,
                unattributed,
                ranking: ranking(&kinds, Class::Deterministic),
                stochastic_ranking: ranking(&kinds, Class::Stochastic),
                label_audit: audit(&effects, &method.audit),
            },
            sequence,
            reference,
            method,
            timing,
            coverage,
            props,
            kinds,
            effects,
            notes,
        }
    }
}

/// An effect measured and not judged, for a look that says too little, in a corpus scorecard.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LittleEntry {
    /// The sequence it's in, and its place in that sequence's `effects`.
    pub sequence: String,
    pub index: usize,
    pub kind: String,
    pub target: String,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(flatten)]
    pub look: SaysLittle,
}

/// One sequence's line in a corpus scorecard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    pub label: String,
    pub duration_ms: u64,
    pub look: LookSummary,
    pub look_by_moment: LookSummary,
    pub import: SequenceImportSummary,
    pub pixels: usize,
    pub pixels_measured: usize,
    pub effects: usize,
    pub effects_measured: usize,
    pub frames_compared: u32,
    pub frames_missing: u32,
    pub best_offset: i64,
    /// Pixel for pixel: a diagnostic.
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

/// The pixel-for-pixel measures of a whole corpus.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusDiagnostics {
    pub ranking: Vec<Ranked>,
    pub stochastic_ranking: Vec<Ranked>,
    pub label_audit: LabelAudit,
}

/// Every sequence of a corpus together.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusCard {
    pub schema: u32,
    pub tool: String,
    /// How much of all the sequences comes across looking the same.
    pub look: LookSummary,
    /// The same with every effect held moment by moment against xLights' render alone.
    pub look_by_moment: LookSummary,
    pub sequences: Vec<Headline>,
    pub failures: Vec<Failure>,
    /// The effects measured and not judged, for a look that says too little; the longest first.
    pub says_little: Vec<LittleEntry>,
    pub improve: Vec<Improve>,
    pub labels: LabelCheck,
    pub kinds: Vec<KindScore>,
    pub diagnostics: CorpusDiagnostics,
}

impl CorpusCard {
    pub fn new(tool: &str) -> Self {
        Self {
            schema: SCHEMA,
            tool: tool.to_string(),
            look: LookSummary::default(),
            look_by_moment: LookSummary::default(),
            sequences: Vec::new(),
            failures: Vec::new(),
            says_little: Vec::new(),
            improve: Vec::new(),
            labels: LabelCheck::default(),
            kinds: Vec::new(),
            diagnostics: CorpusDiagnostics::default(),
        }
    }

    /// Adds a sequence's scorecard.
    pub fn add(&mut self, card: &Scorecard) {
        let label = &card.sequence.label;
        let audit = &card.diagnostics.label_audit;
        self.look.merge(&card.look);
        self.look_by_moment.merge(&card.look_by_moment);
        self.labels.merge(&card.labels, label);
        for effect in &card.effects {
            if let Some(look) = &effect.says_little {
                self.says_little.push(LittleEntry {
                    sequence: label.clone(),
                    index: effect.index,
                    kind: effect.kind.clone(),
                    target: effect.target.clone(),
                    start_ms: effect.start_ms,
                    end_ms: effect.end_ms,
                    look: look.clone(),
                });
            }
        }
        self.says_little.sort_by(|a, b| {
            (b.end_ms.saturating_sub(b.start_ms))
                .cmp(&a.end_ms.saturating_sub(a.start_ms))
                .then(a.sequence.cmp(&b.sequence))
                .then(a.index.cmp(&b.index))
        });
        self.sequences.push(Headline {
            label: label.clone(),
            duration_ms: card.sequence.duration_ms,
            look: card.look.clone(),
            look_by_moment: card.look_by_moment.clone(),
            import: card.sequence.import,
            pixels: card.coverage.pixels,
            pixels_measured: card.coverage.pixels_measured,
            effects: card.coverage.effects,
            effects_measured: card.coverage.effects_measured,
            frames_compared: card.timing.frames_compared,
            frames_missing: card.timing.frames_missing,
            best_offset: card.timing.best_offset,
            overall: card.diagnostics.overall,
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
        self.improve = improve(&self.kinds);
        self.diagnostics.ranking = ranking(&self.kinds, Class::Deterministic);
        self.diagnostics.stochastic_ranking = ranking(&self.kinds, Class::Stochastic);

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
        let all = &mut self.diagnostics.label_audit;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::{Grid, LookStats, Scratch};
    use std::rc::Rc;

    /// A look of made-up frames with the score it is given. `itself` is how alike PixelFlow's
    /// own renders were in their rhythm, for a look next to its other seeds.
    fn look(score: f64, itself: Option<f64>) -> Look {
        let mut stats = LookStats::new(Rc::new(Grid::new(&[(0.0, 0.0)], 1, 1)), 25, &Limits::default());
        let mut scratch = Scratch::default();
        stats.add(&mut scratch, 0, 0, [[255; 3]; 2]);
        stats.end_frame(&mut scratch, 0);
        let mut look = stats.finish().unwrap();
        look.score = score;
        look.verdict = crate::look::verdict(score);
        look.pixel_seconds = 100.0;
        if let Some(itself) = itself {
            (look.chance, look.itself, look.least) = (Chance::Drawn, Some(itself), Some(Component::Rhythm));
        }
        look
    }

    /// An effect ten seconds long with pixels to itself, the look it got there, and what that
    /// scores moment by moment when the look is next to other seeds.
    fn effect(index: usize, kind: &str, label: Label, held: Look, by_moment: Option<Look>) -> EffectScore {
        let limits = Limits::default();
        let mut effect = EffectScore {
            index,
            kind: kind.to_string(),
            pixelflow_kind: kind.to_string(),
            target: "Matrix".to_string(),
            start_ms: 0,
            end_ms: 10_000,
            layer: 0,
            expected: if label == Label::Placeholder {
                Verdict::NoEquivalent
            } else {
                Verdict::Matches
            },
            label,
            notes: Vec::new(),
            look: None,
            says_little: None,
            chance: held.chance,
            chance_reason: None,
            look_solo: Some(held),
            look_layered: Some(held),
            look_solo_by_moment: by_moment,
            look_layered_by_moment: by_moment,
            class: Class::Deterministic,
            class_reason: None,
            sparkles: 0.0,
            pixels: 10,
            measured_pixels: 10,
            solo: None,
            layered: None,
            share: 100.0,
            onset: None,
            solo_stats: Stats::new(25, &limits),
            layered_stats: Stats::new(25, &limits),
        };
        effect.judge(&AuditLimits::default());
        effect
    }

    #[test]
    fn a_look_that_says_too_little_leaves_its_effect_not_judged() {
        let audit = AuditLimits::default();
        // PixelFlow's own renders have nine tenths of this one's rhythm in common, and a tenth
        // of that one's: the second look scores 100 and says nothing.
        let sure = effect(
            0,
            "On",
            Label::Exact,
            look(100.0, Some(0.9)),
            Some(look(100.0, None)),
        );
        let little = effect(
            1,
            "Fireworks",
            Label::Exact,
            look(100.0, Some(0.1)),
            Some(look(40.0, None)),
        );
        let close = effect(2, "On", Label::Exact, look(70.0, None), None);
        assert!(sure.look.is_some() && sure.says_little.is_none());
        assert!(close.look.is_some() && close.says_little.is_none());
        assert!(little.look.is_none() && little.look_basis(&audit).is_none());
        // The look it got is kept, with what it would have earned and why it says too little.
        let said = little.says_little.as_ref().unwrap();
        assert_eq!(
            (said.basis, said.score, said.verdict, said.chance),
            (Basis::Solo, 100.0, Verdict::Matches, Chance::Drawn)
        );
        assert_eq!((said.itself, said.least), (0.1, Some(Component::Rhythm)));
        assert_eq!(
            said.by_moment,
            Some(Scored {
                score: 40.0,
                verdict: Verdict::Different
            })
        );
        assert_eq!(
            said.why,
            "PixelFlow's own renders have 10 in 100 of their rhythm in common from one seed to the next"
        );

        // It is in no share, score, or count of what was judged, and is counted as what it is.
        let effects = [sure, little, close];
        let (mut summary, mut by_moment) = (LookSummary::default(), LookSummary::default());
        for effect in &effects {
            summary.add(effect, effect.judged(&audit), effect.says_little.is_some());
            by_moment.add(effect, effect.judged_by_moment(&audit), false);
        }
        summary.finish();
        by_moment.finish();
        assert_eq!((summary.judged, summary.of, summary.seconds), (2, 3, 20.0));
        assert_eq!(
            (summary.time.matches, summary.time.close, summary.score),
            (0.5, 0.5, Some(85.0))
        );
        assert_eq!((summary.effects.matches, summary.effects.close), (1, 1));
        assert_eq!(
            summary.says_little,
            SaidLittle {
                effects: 1,
                seconds: 10.0,
                share: 0.3333
            }
        );
        assert_eq!(summary.not_judged, 0.3333);
        // Moment by moment every effect is judged, as all were before.
        assert_eq!((by_moment.judged, by_moment.says_little.effects), (3, 0));
        assert_eq!(
            (
                by_moment.time.matches,
                by_moment.time.close,
                by_moment.time.different
            ),
            (0.3333, 0.3333, 0.3333)
        );
        // A corpus adds them up.
        let mut all = LookSummary::default();
        all.merge(&summary);
        all.merge(&summary);
        assert_eq!(
            (
                all.judged,
                all.of,
                all.says_little.effects,
                all.says_little.seconds
            ),
            (4, 6, 2, 20.0)
        );
        assert_eq!((all.says_little.share, all.not_judged), (0.3333, 0.3333));

        // The importer's label for it is held against nothing, and its kind has no look.
        let check = LabelCheck::of(&effects);
        assert_eq!((check.judged, check.agree), (2, 1));
        let row = check.rows.iter().find(|row| row.kind == "Fireworks").unwrap();
        assert_eq!(
            (
                row.effects,
                row.judged(),
                row.says_little,
                row.earned(),
                row.score
            ),
            (1, 0, 1, None, None)
        );
        let kinds = kinds(&effects, &audit, 25, &Limits::default());
        let fireworks = kinds.iter().find(|kind| kind.kind == "Fireworks").unwrap();
        assert_eq!((fireworks.instances, fireworks.says_little), (1, 1));
        assert!(fireworks.look.is_none());
        let on = kinds.iter().find(|kind| kind.kind == "On").unwrap();
        assert_eq!(
            (
                on.says_little,
                on.look.as_ref().map(|look| (look.judged, look.score))
            ),
            (0, Some((2, 85.0)))
        );
        assert!(improve(&kinds).iter().all(|ranked| ranked.kind != "Fireworks"));
    }

    #[test]
    fn a_placeholder_has_no_equivalent_whatever_its_look_says() {
        let stand_in = effect(
            0,
            "Shader",
            Label::Placeholder,
            look(40.0, Some(0.1)),
            Some(look(40.0, None)),
        );
        let judged = stand_in.look.as_ref().unwrap();
        assert_eq!((judged.verdict, judged.score), (Verdict::NoEquivalent, 40.0));
        assert!(stand_in.says_little.is_none());
    }

    #[test]
    fn a_look_of_next_to_no_light_says_so() {
        let mut dark = look(100.0, Some(0.2));
        dark.seen.lit.pixelflow = 0.0003;
        dark.seen.lit.xlights = 0.0001;
        let fireworks = effect(0, "Fireworks", Label::Exact, dark, Some(look(38.0, None)));
        assert!(fireworks.look.is_none());
        assert_eq!(
            fireworks.says_little.as_ref().map(|said| said.why.as_str()),
            Some("too little light to tell")
        );
    }
}
