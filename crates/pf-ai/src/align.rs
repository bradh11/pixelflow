//! Locking a drafted sequence to the music before the user sees it: every effect edge and timing
//! mark the assistant drafted moves onto the nearest musical moment within reach, so a sloppy
//! 2,130 ms lands on the chorus at 2,000 ms.
//!
//! **Anchors**, most important first, each reaching a share of a beat (`b`, from the tempo):
//!
//! | anchor | from | reach |
//! |---|---|---|
//! | section boundary | the user's Sections track, else the detected sections | 1 beat |
//! | accent | the user's Accents track, else hits, drops, breaks, builds (and where breaks and builds end) | ½ beat |
//! | word | where each sung word starts, from the user's Lyrics (words) track (else its Lyrics lines) | ½ beat |
//! | syllable | where each sung syllable starts and ends, only from a syllables track the draft placed effects on | 1 ms |
//! | bar (downbeat) | the analysis | ¼ bar (1 beat in 4/4), only when no beat is nearer |
//! | beat | the analysis | ¼ beat |
//!
//! An edge goes to the anchor nearest for its reach (distance ÷ reach, smallest wins; a tie goes
//! to the more important anchor), so an edge already on beat 2 stays there rather than jumping to
//! the bar line a beat away, while one 300 ms off a section start lands on it. A syllable only
//! holds an edge already on it (an effect cut at a syllables track stays with each syllable,
//! not pulled onto the beat), and pulls nothing else.
//!
//! **Rules**: only what the draft added or moved is touched (an effect's edge that is as the user
//! had it stays); an effect never ends up shorter than a frame (or than it was, if shorter), and
//! never outside the sequence; nothing comes to overlap on a layer that didn't overlap before;
//! marks never overlap. An edge whose move would break a rule stays where it is. Running the pass
//! again changes nothing.

use pf_analysis::Analysis;
use pf_engine::SequenceEdit;
use pf_sequence::{EffectId, Mark, Sequence, TimingKind, TimingTrack};
use std::collections::{HashMap, HashSet};

/// What an anchor is, most important first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anchor {
    Section,
    Accent,
    Word,
    Syllable,
    Bar,
    Beat,
}

const ANCHORS: [Anchor; 6] = [
    Anchor::Section,
    Anchor::Accent,
    Anchor::Word,
    Anchor::Syllable,
    Anchor::Bar,
    Anchor::Beat,
];

/// How far a syllable reaches: only an edge already on it.
const SYLLABLE_REACH_MS: f64 = 1.0;

/// The beat without a tempo to go by (120 BPM).
const DEFAULT_BEAT_MS: f64 = 500.0;

/// The musical moments to lock to, and how far each reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchors {
    beat_ms: f64,
    /// Sorted times for each kind of anchor, in [`Anchor`] order.
    times: [Vec<u64>; 6],
}

/// The time in `sorted` nearest `t`.
fn nearest(sorted: &[u64], t: u64) -> Option<u64> {
    let at = sorted.partition_point(|&a| a < t);
    let after = sorted.get(at).copied();
    let before = at.checked_sub(1).map(|i| sorted[i]);
    match (before, after) {
        (Some(b), Some(a)) => Some(if t - b <= a - t { b } else { a }),
        (b, a) => b.or(a),
    }
}

fn sorted(mut times: Vec<u64>) -> Vec<u64> {
    times.sort_unstable();
    times.dedup();
    times
}

/// Whether a user's track is the song's sections, or its accents.
fn is_sections(track: &TimingTrack) -> bool {
    track.kind == TimingKind::Sections && !track.marks.is_empty()
}

fn is_accents(track: &TimingTrack) -> bool {
    track.name.trim().eq_ignore_ascii_case("accents") && !track.marks.is_empty()
}

/// The user's own Sections track in `doc`, if it has one with marks.
pub fn user_sections(doc: &Sequence) -> Option<&TimingTrack> {
    doc.timing_tracks.iter().find(|t| is_sections(t))
}

/// The user's own Accents track in `doc`, if it has one with marks.
pub fn user_accents(doc: &Sequence) -> Option<&TimingTrack> {
    doc.timing_tracks.iter().find(|t| is_accents(t))
}

/// The user's sung words in `doc`: a words track with marks, else a lyrics track's lines.
pub fn user_words(doc: &Sequence) -> Option<&TimingTrack> {
    let with = |kind: TimingKind| {
        doc.timing_tracks
            .iter()
            .find(|t| t.kind == kind && !t.marks.is_empty())
    };
    with(TimingKind::Words).or_else(|| with(TimingKind::Lyrics))
}

impl Anchors {
    /// Anchors from times (each in any order), with a beat of `beat_ms`.
    pub fn new(beat_ms: f64, sections: Vec<u64>, accents: Vec<u64>, bars: Vec<u64>, beats: Vec<u64>) -> Self {
        let beat_ms = if beat_ms.is_finite() && beat_ms > 0.0 {
            beat_ms
        } else {
            DEFAULT_BEAT_MS
        };
        Self {
            beat_ms,
            times: [
                sorted(sections),
                sorted(accents),
                Vec::new(),
                Vec::new(),
                sorted(bars),
                sorted(beats),
            ],
        }
    }

    /// These anchors with where sung words start (in any order).
    pub fn with_words(mut self, words: Vec<u64>) -> Self {
        self.times[Anchor::Word as usize] = sorted(words);
        self
    }

    /// These anchors with where sung syllables start and end (in any order): only for a
    /// syllables track effects were placed on.
    pub fn with_syllables(mut self, syllables: Vec<u64>) -> Self {
        self.times[Anchor::Syllable as usize] = sorted(syllables);
        self
    }

    /// The song's anchors: sections and accents from the user's own tracks in `user` when it has
    /// them (they win over what was detected), sung words from its lyrics, the rest from
    /// `analysis`. `None` with nothing to lock to.
    pub fn for_song(analysis: Option<&Analysis>, user: Option<&Sequence>) -> Option<Self> {
        let beat_ms = analysis
            .and_then(|a| a.tempo_bpm)
            .filter(|t| *t > 0.0)
            .map(|t| 60_000.0 / f64::from(t))
            .unwrap_or(DEFAULT_BEAT_MS);
        // A span (a break, a build) also ends somewhere worth landing on; a moment doesn't.
        let span = (2.0 * beat_ms) as u64;
        let sections = match user.and_then(user_sections) {
            Some(track) => track.marks.iter().flat_map(|m| [m.start_ms, m.end_ms]).collect(),
            None => analysis
                .map(|a| a.sections().iter().flat_map(|s| [s.start_ms, s.end_ms]).collect())
                .unwrap_or_default(),
        };
        let accents = match user.and_then(user_accents) {
            Some(track) => track
                .marks
                .iter()
                .flat_map(|m| {
                    std::iter::once(m.start_ms).chain((m.end_ms - m.start_ms > span).then_some(m.end_ms))
                })
                .collect(),
            None => analysis
                .map(|a| {
                    a.events
                        .iter()
                        .flat_map(|e| std::iter::once(e.time_ms).chain(e.duration_ms.map(|d| e.time_ms + d)))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let (bars, beats) = analysis.map_or_else(Default::default, |a| (a.bars.clone(), a.beats.clone()));
        let words = user
            .and_then(user_words)
            .map(|track| track.marks.iter().map(|m| m.start_ms).collect())
            .unwrap_or_default();
        let anchors = Self::new(beat_ms, sections, accents, bars, beats).with_words(words);
        anchors.times.iter().any(|t| !t.is_empty()).then_some(anchors)
    }

    /// How far (ms) an anchor of this kind reaches.
    pub fn reach(&self, anchor: Anchor) -> f64 {
        let b = self.beat_ms;
        match anchor {
            Anchor::Section => b,
            Anchor::Accent | Anchor::Word => b / 2.0,
            Anchor::Syllable => SYLLABLE_REACH_MS,
            // A quarter of a 4/4 bar.
            Anchor::Bar => b,
            Anchor::Beat => b / 4.0,
        }
    }

    /// The anchors within reach of `t`, best first (nearest for their reach; a tie goes to the
    /// more important), one time each.
    pub fn candidates(&self, t: u64) -> Vec<(u64, Anchor)> {
        let beats = &self.times[Anchor::Beat as usize];
        let nearest_beat = nearest(beats, t).map(|b| b.abs_diff(t));
        let mut found: Vec<(f64, u64, Anchor)> = Vec::new();
        for anchor in ANCHORS {
            let Some(at) = nearest(&self.times[anchor as usize], t) else {
                continue;
            };
            let distance = at.abs_diff(t);
            // A bar line isn't worth jumping a beat for.
            if anchor == Anchor::Bar && nearest_beat.is_some_and(|d| d < distance) {
                continue;
            }
            let reach = self.reach(anchor);
            if distance as f64 <= reach {
                found.push((distance as f64 / reach, at, anchor));
            }
        }
        // Stable: equal scores keep the more important anchor first.
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(u64, Anchor)> = Vec::new();
        for (_, at, anchor) in found {
            if !out.iter().any(|(t, _)| *t == at) {
                out.push((at, anchor));
            }
        }
        out
    }

    /// Where an edge at `t` locks to, and to what (`None`: nothing is within reach).
    pub fn snap(&self, t: u64) -> Option<(u64, Anchor)> {
        self.candidates(t).first().copied()
    }

    /// Where an edge at `t` may go, best first, ending with staying put: only anchors that suit it
    /// better than where it is (an edge on an anchor stays on it).
    fn choices(&self, t: u64, limit: u64) -> Vec<u64> {
        let mut out: Vec<u64> = Vec::new();
        for (at, _) in self.candidates(t) {
            if at == t {
                break;
            }
            if at <= limit {
                out.push(at);
            }
        }
        out.push(t);
        out
    }
}

/// What locking a draft came to: the edits that do it, and how many edges moved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Locked {
    pub edits: Vec<SequenceEdit>,
    /// Effect edges moved onto an anchor.
    pub effect_edges: usize,
    /// Timing mark edges moved onto an anchor.
    pub mark_edges: usize,
}

impl Locked {
    pub fn edges(&self) -> usize {
        self.effect_edges + self.mark_edges
    }
}

fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Spans on one layer (or track) and which of their edges may move.
struct Spans {
    /// As they were.
    was: Vec<(u64, u64)>,
    now: Vec<(u64, u64)>,
    /// Whether each span's start and end may move.
    free: Vec<(bool, bool)>,
    /// Spans that may share time only if they did before (effects on a layer), or never (marks).
    may_keep_overlaps: bool,
    end_ms: u64,
    frame_ms: u64,
}

impl Spans {
    fn fits(&self, i: usize, (start, end): (u64, u64)) -> bool {
        let (was_start, was_end) = self.was[i];
        let shortest = self.frame_ms.min(was_end - was_start).max(1);
        if end <= start || end - start < shortest || end > self.end_ms {
            return false;
        }
        (0..self.now.len()).filter(|&j| j != i).all(|j| {
            !overlaps((start, end), self.now[j])
                || (self.may_keep_overlaps && overlaps(self.was[i], self.was[j]))
        })
    }

    /// Moves free edges onto anchors until nothing more can move. Answers how many edges moved.
    fn lock(&mut self, anchors: &Anchors) -> usize {
        let mut order: Vec<usize> = (0..self.now.len()).collect();
        order.sort_by_key(|&i| self.now[i]);
        // An edge moves at most once (onto an anchor, where it stays), so this ends.
        for _ in 0..=2 * self.now.len() {
            let mut moved = false;
            for &i in &order {
                let (start, end) = self.now[i];
                let (free_start, free_end) = self.free[i];
                let starts = if free_start {
                    anchors.choices(start, self.end_ms)
                } else {
                    vec![start]
                };
                let ends = if free_end {
                    anchors.choices(end, self.end_ms)
                } else {
                    vec![end]
                };
                // The best pair first: by how far down each edge's choices it is.
                let mut tries: Vec<(usize, (u64, u64))> = starts
                    .iter()
                    .enumerate()
                    .flat_map(|(a, &s)| ends.iter().enumerate().map(move |(b, &e)| (a + b, (s, e))))
                    .filter(|&(_, c)| c != (start, end))
                    .collect();
                tries.sort_by_key(|&(rank, _)| rank);
                if let Some(&(_, next)) = tries.iter().find(|&&(_, c)| self.fits(i, c)) {
                    self.now[i] = next;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        self.was
            .iter()
            .zip(&self.now)
            .map(|(a, b)| usize::from(a.0 != b.0) + usize::from(a.1 != b.1))
            .sum()
    }
}

/// Tracks whose marks follow the voice (lyrics, words, syllables, phonemes): never moved to
/// the beat.
fn follows_the_voice(track: &TimingTrack) -> bool {
    matches!(
        track.kind,
        TimingKind::Lyrics | TimingKind::Words | TimingKind::Phonemes
    ) || crate::lyrics::tracks::is_syllables(track)
}

/// Locks what `draft` added or moved, compared with `base` (the user's sequence), to `anchors`.
/// Tracks whose marks are exactly one of `as_made` (the song's own tracks, as analysis makes
/// them) are left as they are: their marks are the music already.
pub fn lock(base: Option<&Sequence>, draft: &Sequence, anchors: &Anchors, as_made: &[Vec<Mark>]) -> Locked {
    let mut locked = Locked::default();
    let frame_ms = u64::from(draft.frame_ms.max(1));
    let before: HashMap<EffectId, (u64, u64)> = base
        .map(|doc| doc.effects().map(|e| (e.id, (e.start_ms, e.end_ms))).collect())
        .unwrap_or_default();
    for row in &draft.rows {
        for layer in &row.layers {
            let was: Vec<(u64, u64)> = layer.effects.iter().map(|e| (e.start_ms, e.end_ms)).collect();
            let free: Vec<(bool, bool)> = layer
                .effects
                .iter()
                .map(|e| match before.get(&e.id) {
                    Some(&(start, end)) => (e.start_ms != start, e.end_ms != end),
                    None => (true, true),
                })
                .collect();
            if !free.iter().any(|&(s, e)| s || e) {
                continue;
            }
            let mut spans = Spans {
                now: was.clone(),
                was,
                free,
                may_keep_overlaps: true,
                end_ms: draft.duration_ms,
                frame_ms,
            };
            locked.effect_edges += spans.lock(anchors);
            for (effect, (&now, &was)) in layer.effects.iter().zip(spans.now.iter().zip(&spans.was)) {
                if now != was {
                    locked.edits.push(SequenceEdit::SetEffectTiming {
                        id: effect.id,
                        start_ms: now.0,
                        end_ms: now.1,
                    });
                }
            }
        }
    }
    for track in &draft.timing_tracks {
        if follows_the_voice(track) || as_made.contains(&track.marks) {
            continue;
        }
        let kept: HashSet<(u64, u64, &str)> = base
            .and_then(|doc| doc.timing_track(track.id))
            .map(|t| {
                t.marks
                    .iter()
                    .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let free: Vec<(bool, bool)> = track
            .marks
            .iter()
            .map(|m| {
                let drafted = !kept.contains(&(m.start_ms, m.end_ms, m.label.as_str()));
                (drafted, drafted)
            })
            .collect();
        if !free.iter().any(|&(s, _)| s) {
            continue;
        }
        let was: Vec<(u64, u64)> = track.marks.iter().map(|m| (m.start_ms, m.end_ms)).collect();
        let mut spans = Spans {
            now: was.clone(),
            was,
            free,
            may_keep_overlaps: false,
            end_ms: draft.duration_ms.max(track.marks.last().map_or(0, |m| m.end_ms)),
            frame_ms: 1,
        };
        let moved = spans.lock(anchors);
        if moved == 0 {
            continue;
        }
        locked.mark_edges += moved;
        let mut next = track.clone();
        for (mark, &(start, end)) in next.marks.iter_mut().zip(&spans.now) {
            mark.start_ms = start;
            mark.end_ms = end;
        }
        next.marks.sort_by_key(|m| (m.start_ms, m.end_ms));
        locked.edits.push(SequenceEdit::UpdateTimingTrack { track: next });
    }
    locked
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{Effect, EffectKind, Row, Target};

    /// 120 BPM (a 500 ms beat) for 32 s: bars every 2 s, sections at 0, 8, 16, 24 s, a hit at
    /// 10.3 s, and a break from 20 s to 22.1 s.
    fn anchors() -> Anchors {
        let beats: Vec<u64> = (0..64).map(|i| i * 500).collect();
        Anchors::new(
            500.0,
            vec![0, 8_000, 16_000, 24_000, 32_000],
            vec![10_300, 20_000, 22_100],
            beats.iter().copied().step_by(4).collect(),
            beats,
        )
    }

    fn doc_with(spans: &[&[(u64, u64)]]) -> Sequence {
        let mut doc = Sequence::new("Song", 32_000);
        let mut row = Row::new(Target::Group(pf_model::GroupId::new()));
        row.layers = spans
            .iter()
            .map(|layer| pf_sequence::Layer {
                effects: layer
                    .iter()
                    .map(|&(s, e)| Effect::new(EffectKind::On, s, e))
                    .collect(),
            })
            .collect();
        doc.rows.push(row);
        doc
    }

    fn applied(doc: &Sequence, locked: &Locked) -> Sequence {
        pf_engine::edited_sequence(doc, &locked.edits).unwrap()
    }

    fn times(doc: &Sequence, layer: usize) -> Vec<(u64, u64)> {
        doc.rows[0].layers[layer]
            .effects
            .iter()
            .map(|e| (e.start_ms, e.end_ms))
            .collect()
    }

    #[test]
    fn edges_go_to_the_most_important_anchor_in_reach() {
        let a = anchors();
        // 300 ms past a section start (beyond a beat's reach): the section.
        assert_eq!(a.snap(8_300), Some((8_000, Anchor::Section)));
        // Near the hit, nearer than the beat at 10.5 s: the hit.
        assert_eq!(a.snap(10_240), Some((10_300, Anchor::Accent)));
        // On beat 2 of a bar: stays (the bar line a beat away doesn't pull it).
        assert_eq!(a.snap(2_500), Some((2_500, Anchor::Beat)));
        // A little off a bar line: the bar.
        assert_eq!(a.snap(4_200), Some((4_000, Anchor::Bar)));
        // A little off a beat: the beat.
        assert_eq!(a.snap(5_080), Some((5_000, Anchor::Beat)));
        // Between beats, nothing in reach.
        assert_eq!(a.snap(5_250), None);
        // Just past the end of a break (nearer for its reach than the bar line before it).
        assert_eq!(a.snap(22_150), Some((22_100, Anchor::Accent)));
        // Choices come best first.
        assert_eq!(
            a.candidates(10_240),
            [(10_300, Anchor::Accent), (10_000, Anchor::Bar)]
        );
    }

    #[test]
    fn reach_follows_the_tempo() {
        let slow = Anchors::new(1_000.0, vec![10_000], vec![], vec![], vec![]);
        let fast = Anchors::new(250.0, vec![10_000], vec![], vec![], vec![]);
        assert_eq!(slow.snap(10_900), Some((10_000, Anchor::Section)));
        assert_eq!(fast.snap(10_300), None);
        assert_eq!(fast.reach(Anchor::Beat), 62.5);
    }

    #[test]
    fn sung_words_are_anchors_when_there_are_lyrics() {
        let mut user = Sequence::new("Song", 32_000);
        user.timing_tracks.push(TimingTrack::new(
            "Lyrics",
            TimingKind::Lyrics,
            vec![Mark::new(6_100, 7_400, "Paper lanterns")],
        ));
        user.timing_tracks.push(TimingTrack::new(
            "Lyrics (words)",
            TimingKind::Words,
            vec![
                Mark::new(6_100, 6_700, "Paper"),
                Mark::new(6_870, 7_400, "lanterns"),
            ],
        ));
        let analysis = Analysis {
            duration_ms: 32_000,
            tempo_bpm: Some(120.0),
            beats: (0..64).map(|i| i * 500).collect(),
            ..Analysis::default()
        };
        let a = Anchors::for_song(Some(&analysis), Some(&user)).unwrap();
        // A word start within half a beat wins over the beat beside it, like an accent.
        assert_eq!(a.snap(6_800), Some((6_870, Anchor::Word)));
        assert_eq!(a.snap(6_180), Some((6_100, Anchor::Word)));
        assert_eq!(a.reach(Anchor::Word), 250.0);
        // Beyond half a beat, the beat.
        assert_eq!(a.snap(7_480), Some((7_500, Anchor::Beat)));
        // Lines alone, without words: their starts.
        user.timing_tracks.pop();
        let a = Anchors::for_song(Some(&analysis), Some(&user)).unwrap();
        assert_eq!(a.snap(6_180), Some((6_100, Anchor::Word)));
        assert_eq!(a.snap(6_800), None);
        // No lyrics: no word anchors.
        let a = Anchors::for_song(Some(&analysis), Some(&Sequence::new("Song", 32_000))).unwrap();
        assert_eq!(a.snap(6_180), None);
    }

    #[test]
    fn sloppy_effects_lock_and_the_pass_is_idempotent() {
        let base = Sequence::new("Song", 32_000);
        let draft = doc_with(&[&[(130, 7_920), (8_210, 16_180), (16_090, 23_700)]]);
        let a = anchors();
        let locked = lock(Some(&base), &draft, &a, &[]);
        let after = applied(&draft, &locked);
        assert_eq!(times(&after, 0), [(0, 8_000), (8_000, 16_000), (16_000, 24_000)]);
        assert_eq!(locked.effect_edges, 6);
        let again = lock(Some(&base), &after, &a, &[]);
        assert_eq!(again, Locked::default());
    }

    #[test]
    fn the_users_own_effects_stay_where_they_are() {
        let base = doc_with(&[&[(130, 7_920)]]);
        let mut draft = base.clone();
        // The assistant moved only the end of the user's effect, and added one.
        draft.rows[0].layers[0].effects[0].end_ms = 7_890;
        draft.rows[0].layers[0]
            .effects
            .push(Effect::new(EffectKind::On, 8_100, 9_000));
        let locked = lock(Some(&base), &draft, &anchors(), &[]);
        let after = applied(&draft, &locked);
        assert_eq!(times(&after, 0), [(130, 8_000), (8_000, 9_000)]);
    }

    #[test]
    fn no_overlap_is_made_and_none_is_ever_zero_length() {
        let base = Sequence::new("Song", 32_000);
        // The first ends just before a section; the second starts on the section already. A
        // tiny effect sits between two anchors it would both snap onto.
        let draft = doc_with(&[&[(6_000, 7_700), (7_800, 9_000), (12_010, 12_030)]]);
        let locked = lock(Some(&base), &draft, &anchors(), &[]);
        let after = applied(&draft, &locked);
        let spans = times(&after, 0);
        // 7,700 can't lock to the section at 8,000 while the next effect still starts at 7,800;
        // once that one has moved there, it can: they touch, never overlap.
        assert_eq!(spans[0], (6_000, 8_000));
        assert_eq!(spans[1], (8_000, 9_000));
        // Snapping both edges to 12,000 would leave nothing: it keeps at least a frame.
        assert!(spans[2].1 - spans[2].0 >= 20, "{spans:?}");
        for w in spans.windows(2) {
            assert!(w[0].1 <= w[1].0, "{spans:?}");
        }
    }

    #[test]
    fn an_overlap_that_was_there_may_stay_but_none_is_added() {
        let base = Sequence::new("Song", 32_000);
        // The first's end would lock past the second's start (which can't move: not in reach).
        let draft = doc_with(&[&[(0, 1_900), (1_950, 3_000)]]);
        let a = Anchors::new(500.0, vec![2_000], vec![], vec![], vec![]);
        let locked = lock(Some(&base), &draft, &a, &[]);
        let after = applied(&draft, &locked);
        assert_eq!(times(&after, 0), [(0, 2_000), (2_000, 3_000)]);
        // When the second can't move (it's the user's, as they had it), the first stays short of it.
        let base = doc_with(&[&[(1_950, 3_000)]]);
        let mut draft = base.clone();
        draft.rows[0].layers[0]
            .effects
            .insert(0, Effect::new(EffectKind::On, 0, 1_900));
        let locked = lock(Some(&base), &draft, &a, &[]);
        assert!(locked.edits.is_empty(), "{locked:?}");
        // Overlapping already: they may lock (and still overlap, or not).
        let draft = doc_with(&[&[(0, 2_300), (1_800, 3_000)]]);
        let locked = lock(Some(&Sequence::new("Song", 32_000)), &draft, &a, &[]);
        assert_eq!(times(&applied(&draft, &locked), 0), [(0, 2_000), (2_000, 3_000)]);
    }

    #[test]
    fn effects_stay_inside_the_sequence() {
        let base = Sequence::new("Song", 32_000);
        let draft = doc_with(&[&[(30_000, 31_800)]]);
        // An anchor in reach, but past the end: the edge stays.
        let a = Anchors::new(500.0, vec![32_300], vec![], vec![], vec![]);
        let locked = lock(Some(&base), &draft, &a, &[]);
        assert!(locked.edits.is_empty(), "{locked:?}");
        let a = Anchors::new(500.0, vec![32_000], vec![], vec![], vec![]);
        let locked = lock(Some(&base), &draft, &a, &[]);
        assert_eq!(times(&applied(&draft, &locked), 0), [(30_000, 32_000)]);
    }

    #[test]
    fn syllables_hold_effects_on_them_and_pull_nothing_else() {
        let base = doc_with(&[&[]]);
        // Cut at syllables: "bus" 10,120–10,270 and "ters" 10,270–10,410.
        let draft = doc_with(&[&[(10_120, 10_270), (10_270, 10_410)], &[(5_078, 6_000)]]);
        // Without them, the hit at 10.3 s and the beats pull the edges about.
        let moved = applied(&draft, &lock(Some(&base), &draft, &anchors(), &[]));
        assert_ne!(times(&moved, 0), times(&draft, 0));
        let a = anchors().with_syllables(vec![10_120, 10_270, 10_410, 5_079]);
        let after = applied(&draft, &lock(Some(&base), &draft, &a, &[]));
        assert_eq!(times(&after, 0), [(10_120, 10_270), (10_270, 10_410)]);
        // A syllable a millisecond away doesn't beat a beat 78 ms away.
        assert_eq!(times(&after, 1), [(5_000, 6_000)]);
        // A drafted syllables track isn't moved to the beat.
        let mut draft = Sequence::new("Song", 32_000);
        let syllables = TimingTrack::new(
            "Lyrics (syllables)",
            TimingKind::Custom,
            vec![
                Mark::new(10_120, 10_270, "bus"),
                Mark::new(10_270, 10_410, "ters"),
            ],
        );
        draft.timing_tracks.push(syllables);
        assert!(lock(Some(&base), &draft, &anchors(), &[]).edits.is_empty());
    }

    #[test]
    fn drafted_marks_lock_but_the_songs_own_tracks_and_lyrics_stay() {
        let base = Sequence::new("Song", 32_000);
        let mut draft = base.clone();
        let mine = TimingTrack::new(
            "Hits",
            TimingKind::Custom,
            vec![Mark::new(7_900, 8_400, "a"), Mark::new(10_250, 10_700, "b")],
        );
        let onsets = vec![Mark::new(130, 470, ""), Mark::new(470, 900, "")];
        let made = TimingTrack::new("Onsets", TimingKind::Custom, onsets.clone());
        let lyrics = TimingTrack::new("Lyrics", TimingKind::Lyrics, vec![Mark::new(130, 2_100, "la")]);
        draft.timing_tracks = vec![mine.clone(), made, lyrics];
        let locked = lock(Some(&base), &draft, &anchors(), std::slice::from_ref(&onsets));
        let after = applied(&draft, &locked);
        let marks: Vec<(u64, u64)> = after.timing_tracks[0]
            .marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms))
            .collect();
        // The first's end can't go to the section it now starts on: the beat after, then.
        assert_eq!(marks, [(8_000, 8_500), (10_300, 10_700)]);
        assert_eq!(after.timing_tracks[1].marks, onsets);
        assert_eq!(after.timing_tracks[2].marks[0].start_ms, 130);
        assert_eq!(locked.mark_edges, 3);
        assert_eq!(
            lock(Some(&base), &after, &anchors(), &[onsets]),
            Locked::default()
        );
    }

    #[test]
    fn the_users_sections_and_accents_win() {
        let mut analysis = Analysis {
            duration_ms: 32_000,
            tempo_bpm: Some(120.0),
            beats: (0..64).map(|i| i * 500).collect(),
            ..Analysis::default()
        };
        analysis.bars = analysis.beats.iter().copied().step_by(4).collect();
        analysis.sections = vec![pf_analysis::Section {
            start_ms: 0,
            end_ms: 32_000,
            energy: 0.5,
            level: pf_analysis::Level::Medium,
            label: "Whole song".into(),
            group: "A".into(),
            confidence: 0.5,
        }];
        let mut user = Sequence::new("Song", 32_000);
        user.timing_tracks.push(TimingTrack::new(
            "Sections",
            TimingKind::Sections,
            vec![Mark::new(0, 9_250, "Intro"), Mark::new(9_250, 32_000, "Chorus")],
        ));
        user.timing_tracks.push(TimingTrack::new(
            "Accents",
            TimingKind::Custom,
            vec![Mark::new(12_120, 12_400, "Hit")],
        ));
        let a = Anchors::for_song(Some(&analysis), Some(&user)).unwrap();
        // The user's section boundary (off the beat grid), not the detected one.
        assert_eq!(a.snap(9_400), Some((9_250, Anchor::Section)));
        assert_eq!(a.snap(12_080), Some((12_120, Anchor::Accent)));
        let detected = Anchors::for_song(Some(&analysis), None).unwrap();
        assert_eq!(detected.snap(9_400), Some((9_500, Anchor::Beat)));
        assert_eq!(detected.snap(12_080), Some((12_000, Anchor::Bar)));
    }

    /// A small, fixed pseudo-random sequence (the same every run).
    struct Jitter(u64);

    impl Jitter {
        /// A whole number in `lo..=hi` ms, early or late.
        fn next(&mut self, lo: u64, hi: u64) -> i64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let size = lo + (self.0 >> 33) % (hi - lo + 1);
            if (self.0 >> 20) & 1 == 0 {
                size as i64
            } else {
                -(size as i64)
            }
        }
    }

    fn off(t: u64, by: i64) -> u64 {
        t.saturating_add_signed(by)
    }

    #[test]
    fn a_sloppy_draft_lands_on_the_music_and_nothing_breaks() {
        // 100 BPM (600 ms beats, 2.4 s bars) for 4 minutes; sections on bar lines; hits off the
        // beat grid, on the "and" after beat 3 of some bars.
        let beats: Vec<u64> = (0..400).map(|i| i * 600).collect();
        let bars: Vec<u64> = beats.iter().copied().step_by(4).collect();
        let section_bars = [0usize, 8, 24, 32, 48, 56, 72, 88, 100];
        let sections: Vec<u64> = section_bars.iter().map(|&b| b as u64 * 2_400).collect();
        let hits: Vec<u64> = (0..100).step_by(5).map(|b| b * 2_400 + 1_500).collect();
        let anchors = Anchors::new(600.0, sections.clone(), hits.clone(), bars.clone(), beats.clone());
        let mut jitter = Jitter(7);
        let mut doc = Sequence::new("Song", 240_000);
        let mut looks = Row::new(Target::Group(pf_model::GroupId::new()));
        let mut pulses = Row::new(Target::Group(pf_model::GroupId::new()));
        let mut flashes = Row::new(Target::Group(pf_model::GroupId::new()));
        let mut intended: Vec<(EffectId, u64, Option<u64>)> = Vec::new();
        let mut before_error = 0u64;
        let mut edges = 0u64;
        let mut note = |id, (s, e): (u64, u64), (want_s, want_e): (u64, Option<u64>), out: &mut Vec<_>| {
            before_error += s.abs_diff(want_s) + want_e.map_or(0, |w| e.abs_diff(w));
            edges += 1 + u64::from(want_e.is_some());
            out.push((id, want_s, want_e));
        };
        // A look per section, each edge 50–400 ms off (some overlapping, some leaving gaps).
        for w in sections.windows(2) {
            let (s, e) = (
                off(w[0], jitter.next(50, 400)),
                off(w[1], jitter.next(50, 400)).min(240_000),
            );
            let effect = Effect::new(EffectKind::ColorWash, s, e);
            note(effect.id, (s, e), (w[0], Some(w[1])), &mut intended);
            looks.layers[0].effects.push(effect);
        }
        // A pulse per bar in the first chorus, 20–140 ms off.
        for b in 24..32u64 {
            let (s, e) = (
                off(b * 2_400, jitter.next(20, 140)),
                off((b + 1) * 2_400, jitter.next(20, 140)),
            );
            let effect = Effect::new(EffectKind::On, s, e);
            note(
                effect.id,
                (s, e),
                (b * 2_400, Some((b + 1) * 2_400)),
                &mut intended,
            );
            pulses.layers[0].effects.push(effect);
        }
        // A flash on each hit, 50–180 ms off; it lasts 300 ms, wherever that ends.
        for &h in &hits {
            let s = off(h, jitter.next(50, 180));
            let effect = Effect::new(EffectKind::Strobe, s, s + 300);
            note(effect.id, (s, s + 300), (h, None), &mut intended);
            flashes.layers[0].effects.push(effect);
        }
        doc.rows = vec![looks, pulses, flashes];
        let base = Sequence::new("Song", 240_000);
        let locked = lock(Some(&base), &doc, &anchors, &[]);
        let after = applied(&doc, &locked);

        let mut after_error = 0u64;
        for (id, want_s, want_e) in &intended {
            let e = after.effect(*id).unwrap();
            after_error += e.start_ms.abs_diff(*want_s) + want_e.map_or(0, |w| e.end_ms.abs_diff(w));
            assert_eq!(e.start_ms, *want_s, "{:?}", (e.kind(), e.start_ms, e.end_ms));
            if let Some(want_e) = want_e {
                assert_eq!(e.end_ms, *want_e, "{:?}", (e.kind(), e.start_ms, e.end_ms));
            }
        }
        for row in &after.rows {
            let mut spans: Vec<(u64, u64)> = row.layers[0]
                .effects
                .iter()
                .map(|e| (e.start_ms, e.end_ms))
                .collect();
            spans.sort();
            assert!(spans.iter().all(|&(s, e)| e > s && e <= 240_000), "{spans:?}");
            assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0), "{spans:?}");
        }
        assert!(before_error / edges >= 50, "{}", before_error / edges);
        assert_eq!(after_error, 0);
        assert_eq!(lock(Some(&base), &after, &anchors, &[]), Locked::default());
    }
}
