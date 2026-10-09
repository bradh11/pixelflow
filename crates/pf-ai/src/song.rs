//! The open sequence's song: analyzed (tempo, beats, bars, energy, sections, accents) the first
//! time the assistant asks, kept for the chat, and turned into timing tracks in the draft. The
//! analysis runs on the chat's thread, never holding the engine, and stops when the user presses
//! Stop. The song file is only ever read.

use crate::draft::Draft;
use crate::provider::Cancel;
use pf_analysis::{Analysis, BarEnergy};
use pf_engine::SequenceEdit;
use pf_sequence::{Sequence, TimingKind, TimingTrack};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Analyzes a song file, giving up when cancelled.
pub type Analyzer = dyn Fn(&Path, &Cancel) -> Result<Analysis, String> + Send + Sync;

/// The real analysis ([`pf_analysis`]).
pub fn default_analyzer() -> Arc<Analyzer> {
    Arc::new(|path: &Path, cancel: &Cancel| {
        pf_analysis::analyze_file_cancellable(path, &|| cancel.is_cancelled()).map_err(|e| e.to_string())
    })
}

/// The song as one tool call sees it.
pub struct Song<'a> {
    /// The open sequence's music file, if it has one.
    pub music: Option<&'a Path>,
    /// The last analysis, by file.
    pub cache: &'a mut Option<(PathBuf, Arc<Analysis>)>,
    pub analyzer: &'a Analyzer,
    pub cancel: &'a Cancel,
}

/// Bar times (and bar energies) listed by `analyze_song`, at most.
const MAX_LISTED_BARS: usize = 400;
/// Accents listed by `analyze_song`, at most (the strongest).
const MAX_LISTED_ACCENTS: usize = 60;
/// Lyric lines listed by `analyze_song`, at most, and the characters of each.
const MAX_LISTED_LINES: usize = 40;
const MAX_LINE_CHARS: usize = 40;
/// Sung stretches listed by `analyze_song`, at most.
const MAX_LISTED_VOCALS: usize = 30;

impl Song<'_> {
    /// The song's analysis, run now if this song hasn't been analyzed yet.
    pub fn analysis(&mut self, draft: &Draft) -> Result<Arc<Analysis>, String> {
        if draft.sequence().is_none() {
            return Err(
                "No sequence is open. Offer ask_for_song so the user can pick a song for a new one.".into(),
            );
        }
        let Some(path) = self.music else {
            return Err(
                "This sequence has no song. Its music is set on the Sequence screen; or use ask_for_song for a new sequence from a song.".into(),
            );
        };
        if let Some((cached, analysis)) = self.cache.as_ref()
            && cached == path
        {
            return Ok(Arc::clone(analysis));
        }
        let analysis = Arc::new((self.analyzer)(path, self.cancel)?);
        *self.cache = Some((path.to_path_buf(), Arc::clone(&analysis)));
        Ok(analysis)
    }

    /// The song's analysis if it's been run already (never runs it).
    pub fn analyzed(&self) -> Option<Arc<Analysis>> {
        let (path, analysis) = self.cache.as_ref()?;
        (Some(path.as_path()) == self.music).then(|| Arc::clone(analysis))
    }
}

/// A 0–1 value to two places, as it reads in JSON (not 0.699999988).
fn round2(x: f32) -> f64 {
    (f64::from(x) * 100.0).round() / 100.0
}

/// A section label without its count ("Chorus 2" → "Chorus"): what repeats.
fn label_root(label: &str) -> &str {
    label
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end()
}

/// The mean of `level` over the bars that start in `start..end` (`None` without any).
fn bar_mean(analysis: &Analysis, start: u64, end: u64, level: fn(&BarEnergy) -> f32) -> Option<f32> {
    let values: Vec<f32> = analysis
        .bars
        .iter()
        .zip(&analysis.bar_energy)
        .filter(|(b, _)| (start..end).contains(*b))
        .map(|(_, e)| level(e))
        .collect();
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

/// The user's own sections, from their Sections track: labeled as they are, grouped by label
/// (a count after it doesn't matter), with the song's energy in each.
fn user_sections(analysis: &Analysis, track: &TimingTrack) -> Vec<Value> {
    let mut roots: Vec<String> = Vec::new();
    track
        .marks
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let named = !m.label.trim().is_empty();
            let label = if named {
                m.label.trim().to_string()
            } else {
                format!("Section {}", i + 1)
            };
            // An unnamed section is its own group.
            let root = if named {
                label_root(&label).to_string()
            } else {
                label.clone()
            };
            let at = roots.iter().position(|r| *r == root).unwrap_or_else(|| {
                roots.push(root);
                roots.len() - 1
            });
            let group = char::from(b'A' + (at % 26) as u8).to_string();
            let mut section =
                json!({ "label": label, "group": group, "startMs": m.start_ms, "endMs": m.end_ms });
            if let Some(energy) = bar_mean(analysis, m.start_ms, m.end_ms, |e| e.overall) {
                section["energy"] = round2(energy).into();
            }
            section
        })
        .collect()
}

/// The user's own accents, from their Accents track: a kind from each label (a hit when it
/// names none), and how long the long ones last.
fn user_accents(analysis: &Analysis, track: &TimingTrack) -> Vec<Value> {
    let beat = analysis
        .tempo_bpm
        .filter(|t| *t > 0.0)
        .map_or(500, |t| (60_000.0 / t) as u64);
    track
        .marks
        .iter()
        .take(MAX_LISTED_ACCENTS)
        .map(|m| {
            let word = m.label.trim().to_ascii_lowercase();
            let kind = ["hit", "drop", "break", "build"]
                .into_iter()
                .find(|k| word.starts_with(k))
                .unwrap_or("hit");
            let mut accent = json!({ "atMs": m.start_ms, "kind": kind });
            if m.end_ms - m.start_ms > 2 * beat {
                accent["forMs"] = (m.end_ms - m.start_ms).into();
            }
            accent
        })
        .collect()
}

/// The user's lyrics, briefly: the lyrics track and its words, syllables, and phonemes tracks
/// by name, each line's start and words (the first lines, cut short), and the sung stretches
/// from their Vocals track. `None` without lyrics.
fn lyrics_summary(user: &Sequence) -> Option<Value> {
    let lines = user
        .timing_tracks
        .iter()
        .find(|t| t.kind == TimingKind::Lyrics && !t.marks.is_empty())?;
    let words = user
        .timing_tracks
        .iter()
        .find(|t| t.kind == TimingKind::Words && t.name == format!("{} (words)", lines.name))
        .or_else(|| user.timing_tracks.iter().find(|t| t.kind == TimingKind::Words));
    let listed: Vec<Value> = lines
        .marks
        .iter()
        .take(MAX_LISTED_LINES)
        .map(|m| {
            let mut text: String = m.label.trim().chars().take(MAX_LINE_CHARS).collect();
            if m.label.trim().chars().count() > MAX_LINE_CHARS {
                text.push('…');
            }
            json!([m.start_ms, text])
        })
        .collect();
    let mut summary = json!({ "track": lines.name, "lines": listed });
    if let Some(words) = words {
        summary["wordsTrack"] = words.name.clone().into();
    }
    let named = |suffix: &str, kind: TimingKind| {
        let name = format!("{} {suffix}", lines.name);
        user.timing_tracks
            .iter()
            .find(|t| t.kind == kind && t.name == name && !t.marks.is_empty())
    };
    if let Some(syllables) = named("(syllables)", TimingKind::Custom) {
        summary["syllablesTrack"] = syllables.name.clone().into();
    }
    if let Some(phonemes) = named("(phonemes)", TimingKind::Phonemes) {
        summary["phonemesTrack"] = phonemes.name.clone().into();
    }
    if lines.marks.len() > MAX_LISTED_LINES {
        summary["moreLines"] = (lines.marks.len() - MAX_LISTED_LINES).into();
    }
    if let Some(vocals) = user
        .timing_tracks
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(crate::lyrics::tracks::VOCALS_TRACK) && !t.marks.is_empty())
    {
        let sung: Vec<Value> = vocals
            .marks
            .iter()
            .take(MAX_LISTED_VOCALS)
            .map(|m| json!([m.start_ms, m.end_ms]))
            .collect();
        summary["vocalsMs"] = sung.into();
    }
    Some(summary)
}

/// What `analyze_song` answers: tempo, counts, bar times, sections (named, grouped by what
/// repeats, with their energy), the strongest accents, each bar's energy and bass as a digit
/// string (0–9, a digit per bar), and how sure the analysis is. Sections and accents come from
/// the user's own Sections and Accents tracks in `user` when it has them (`sectionsFrom`,
/// `accentsFrom`: "user"): those win over what was detected. With the user's lyrics, a short
/// summary of them too (`lyrics`).
pub fn describe(analysis: &Analysis, user: Option<&Sequence>) -> Value {
    let my_sections = user.and_then(crate::align::user_sections);
    let sections: Vec<Value> = match my_sections {
        Some(track) => user_sections(analysis, track),
        None => analysis
            .sections()
            .iter()
            .map(|s| {
                json!({
                    "label": s.label,
                    "group": s.group,
                    "startMs": s.start_ms,
                    "endMs": s.end_ms,
                    "energy": round2(s.energy),
                    "level": s.level,
                    "confidence": round2(s.confidence),
                })
            })
            .collect(),
    };
    let my_accents = user.and_then(crate::align::user_accents);
    let accents: Vec<Value> = match my_accents {
        Some(track) => user_accents(analysis, track),
        None => {
            let mut accents: Vec<_> = analysis.events.iter().collect();
            accents.sort_by(|a, b| b.strength.total_cmp(&a.strength));
            accents.truncate(MAX_LISTED_ACCENTS);
            accents.sort_by_key(|e| e.time_ms);
            accents
                .iter()
                .map(|e| {
                    let mut accent =
                        json!({ "atMs": e.time_ms, "kind": e.kind, "strength": round2(e.strength) });
                    if let Some(d) = e.duration_ms {
                        accent["forMs"] = d.into();
                    }
                    accent
                })
                .collect()
        }
    };
    let digits = |level: fn(&BarEnergy) -> f32| -> String {
        analysis
            .bar_energy
            .iter()
            .take(MAX_LISTED_BARS)
            .map(|e| char::from(b'0' + (level(e).clamp(0.0, 1.0) * 9.0).round() as u8))
            .collect()
    };
    let from = |user: bool| if user { "user" } else { "analysis" };
    let mut described = json!({
        "durationMs": analysis.duration_ms,
        "tempoBpm": analysis.tempo_bpm.map(|t| (t * 10.0).round() / 10.0),
        "beats": analysis.beats.len(),
        "barsMs": analysis.bars.iter().take(MAX_LISTED_BARS).collect::<Vec<_>>(),
        "sectionsFrom": from(my_sections.is_some()),
        "sections": sections,
        "accentsFrom": from(my_accents.is_some()),
        "accents": accents,
        "barEnergy": digits(|e| e.overall),
        "barBass": digits(|e| e.low),
        "confidence": {
            "tempo": round2(analysis.confidence.tempo),
            "downbeat": round2(analysis.confidence.downbeat),
            "sections": round2(analysis.confidence.sections),
        },
    });
    if let Some(lyrics) = user.and_then(lyrics_summary) {
        described["lyrics"] = lyrics;
    }
    described
}

/// The tracks `add_song_timing` can add, by name.
pub const TRACK_CHOICES: [&str; 7] = [
    "beats",
    "bars",
    "sections",
    "onsets",
    "accents",
    "syllables",
    "phonemes",
];

/// The tracks made from the user's sung words rather than the song's analysis.
const FROM_WORDS: [&str; 2] = ["syllables", "phonemes"];

/// Whether adding `wanted` needs the song analyzed.
pub fn needs_analysis(wanted: &[String]) -> bool {
    wanted.iter().any(|w| !FROM_WORDS.contains(&w.as_str()))
}

/// Adds the song's timing tracks to the draft (one draft step); a track already there by name
/// and kind is reused. Syllables and phonemes are made (again) from the sequence's words track
/// ([`crate::lyrics::tracks::from_words`]), replacing their namesakes; they need no analysis,
/// but use its onsets when there is one. Answers each track's name, id, mark count, and whether
/// it was added.
pub fn add_timing(
    analysis: Option<&Analysis>,
    draft: &mut Draft,
    wanted: &[String],
) -> Result<Value, String> {
    let doc = draft
        .sequence()
        .ok_or("No sequence is open. Offer ask_for_song first.")?;
    let made = match analysis {
        Some(analysis) => Some(
            <[TimingTrack; 3]>::try_from(analysis.timing_tracks())
                .map_err(|_| "The song's analysis came back incomplete.".to_string())?,
        ),
        None => None,
    };
    let mut out = Vec::new();
    let mut edits = Vec::new();
    for (i, name) in wanted.iter().enumerate() {
        if wanted[..i].contains(name) {
            continue;
        }
        if !TRACK_CHOICES.contains(&name.as_str()) {
            return Err(format!(
                "There's no \"{name}\" timing; choose from {}.",
                TRACK_CHOICES.join(", ")
            ));
        }
        if FROM_WORDS.contains(&name.as_str()) {
            let onsets = analysis.map_or(&[][..], |a| &a.onsets[..]);
            let rebuilt = crate::lyrics::tracks::from_words(&doc.timing_tracks, onsets, doc.duration_ms)
                .ok_or("There are no sung words to work from. Lyrics come from Find lyrics, beside Detect beats in the sequencer: offer it to the user.")?;
            let kind = if name == "phonemes" {
                TimingKind::Phonemes
            } else {
                TimingKind::Custom
            };
            for edit in rebuilt {
                if let SequenceEdit::AddTimingTrack { track } | SequenceEdit::UpdateTimingTrack { track } =
                    &edit
                    && track.kind == kind
                {
                    let added = matches!(edit, SequenceEdit::AddTimingTrack { .. });
                    out.push(json!({ "name": track.name, "id": track.id, "marks": track.marks.len(), "added": added }));
                    edits.push(edit);
                }
            }
            continue;
        }
        let (Some(analysis), Some([beats, bars, onsets])) = (analysis, &made) else {
            return Err("The song hasn't been analyzed.".into());
        };
        let track = match name.as_str() {
            "beats" => beats.clone(),
            "bars" => bars.clone(),
            "sections" => analysis.sections_track(),
            "accents" => analysis.accents_track(),
            _ => onsets.clone(),
        };
        let existing = doc
            .timing_tracks
            .iter()
            .find(|t| t.name == track.name && t.kind == track.kind);
        match existing {
            Some(t) => {
                out.push(json!({ "name": t.name, "id": t.id, "marks": t.marks.len(), "added": false }))
            }
            None => {
                out.push(
                    json!({ "name": track.name, "id": track.id, "marks": track.marks.len(), "added": true }),
                );
                edits.push(SequenceEdit::AddTimingTrack { track });
            }
        }
    }
    if !edits.is_empty() {
        draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    }
    Ok(Value::Array(out))
}

/// A timing track named or identified by `key` (its id or its name, any case).
pub fn find_track<'a>(tracks: &'a [TimingTrack], key: &str) -> Option<&'a TimingTrack> {
    tracks
        .iter()
        .find(|t| t.id.to_string() == key)
        .or_else(|| tracks.iter().find(|t| t.name.eq_ignore_ascii_case(key.trim())))
}
