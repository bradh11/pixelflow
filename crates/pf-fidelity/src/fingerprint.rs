//! The PixelFlow-only check of the corpus: each sequence imported and rendered by PixelFlow
//! alone, and held to a low-resolution fingerprint of what it drew before. It needs no xLights
//! render, so it catches an unintended change to how a sequence is imported or drawn today.
//!
//! **The fingerprint.** Each prop's pixels are cut into runs of at least [`MIN_SEGMENT`] pixels
//! (at most [`MAX_SEGMENTS`] runs, in the prop's own pixel order), and time into slices of
//! [`SLICE_MS`]. For each run and slice it keeps the mean red, green, and blue, rounded to whole
//! levels of 255. A change that moves any of them by more than [`TOLERANCE`] levels fails the
//! check: a color, a brightness, a speed, a direction, or light on another part of a prop all
//! do that many times over.
//!
//! **Why not the bits.** A frame is the same bytes on any number of threads, but not yet on
//! every system: what is worked out with the system's own sines, cosines, arctangents,
//! hypotenuses, or powers can differ in its last bit from one system or processor to another,
//! and now and then that turns a channel by a level, or a pixel on an edge on or off. Of the
//! corpus's effects, Fan, Circles, Shape, and Butterfly do that (so do Lines, Lightning, Tendril,
//! Wave, VU Meter, the singing faces, and any effect's acceleration); and the layout's own
//! geometry (where a tree's, an arch's, or a star's pixels are) is worked out the same way.
//! Plasma, Pinwheel, Fireworks, and Shader are written to give the same bits everywhere, and
//! the rest of the corpus's effects use no such function. A mean over 16 pixels or more and half
//! a second of frames moves by a small fraction of a level for such a difference. So the
//! fingerprint, not the bits, is the gate on every system, and a digest of every frame is kept
//! beside it for the system it was made on: where it matches, the render is the same to the bit,
//! which the report says, and where it doesn't the fingerprint decides.
//!
//! **Deterministic.** Each effect's id, which its chance is drawn from, is made from its place
//! in the sequence ([`crate::compare::fix_ids`]), the music isn't read (the corpus has none),
//! and nothing else of a render depends on the run.

use crate::FidelityError;
use crate::compare::{Alone, render_alone};
use crate::corpus::Manifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// The file the fingerprints are kept in, in the corpus folder.
pub const FILE: &str = "fingerprint.json";
/// Milliseconds of frames each mean is taken over.
pub const SLICE_MS: u32 = 500;
/// Fewest pixels a run of a prop's pixels has (a prop with fewer is one run).
pub const MIN_SEGMENT: usize = 16;
/// Most runs a prop is cut into.
pub const MAX_SEGMENTS: usize = 16;
/// Levels of 255 a mean may move by and still pass.
pub const TOLERANCE: u8 = 2;
/// The effect ids the check renders with ([`crate::compare::fix_ids`]).
pub const IDS: u64 = 0;

/// Every sequence's fingerprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprints {
    pub about: String,
    pub tool: String,
    pub slice_ms: u32,
    pub tolerance: u8,
    pub sequences: Vec<SequencePrint>,
}

/// One sequence's fingerprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequencePrint {
    pub label: String,
    pub frames: u32,
    pub frame_ms: u32,
    /// A digest of every frame's bytes, by the system it was made on ("macos-aarch64").
    pub digests: BTreeMap<String, String>,
    pub props: Vec<PropPrint>,
}

/// One prop's fingerprint: for each slice of time, the runs' mean red, green, and blue, as hex.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropPrint {
    pub name: String,
    pub pixels: usize,
    pub segments: usize,
    pub slices: Vec<String>,
}

/// The system this runs on, as digests are kept by.
pub fn system() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// FNV-1a, 64 bits: the same on every system.
fn digest(bytes: &[u8], mut hash: u64) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .filter_map(|i| u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// How many runs a prop of `pixels` pixels is cut into.
fn segments(pixels: usize) -> usize {
    (pixels / MIN_SEGMENT).clamp(1, MAX_SEGMENTS)
}

/// The fingerprint of a render.
pub fn of(label: &str, mut alone: Alone) -> Result<SequencePrint, FidelityError> {
    let channels = alone.frames.header().channels as usize;
    let frame_ms = alone.frame_ms.max(1);
    let slices = (u64::from(alone.frame_count) * u64::from(frame_ms)).div_ceil(u64::from(SLICE_MS)) as usize;
    // Sums of red, green, and blue and how many pixel-frames, by prop, slice, and run.
    let mut sums: Vec<Vec<Vec<[u64; 4]>>> = alone
        .props
        .iter()
        .map(|p| vec![vec![[0; 4]; segments(p.pixels.len())]; slices])
        .collect();
    let mut frame = vec![0u8; channels];
    let mut hash = 0xCBF2_9CE4_8422_2325;
    for f in 0..alone.frame_count {
        alone
            .frames
            .read_frame(f, &mut frame)
            .map_err(|e| FidelityError::Render(e.to_string()))?;
        hash = digest(&frame, hash);
        let slice = (u64::from(f) * u64::from(frame_ms) / u64::from(SLICE_MS)) as usize;
        for (prop, sums) in alone.props.iter().zip(&mut sums) {
            let n = prop.pixels.len();
            let k = segments(n);
            for (i, pixel) in prop.pixels.iter().enumerate() {
                let Some(at) = pixel else { continue };
                let cell = &mut sums[slice][i * k / n.max(1)];
                for (sum, &c) in cell.iter_mut().zip(at) {
                    *sum += u64::from(frame.get(c).copied().unwrap_or(0));
                }
                cell[3] += 1;
            }
        }
    }
    let props = alone
        .props
        .iter()
        .zip(sums)
        .map(|(prop, sums)| PropPrint {
            name: prop.name.clone(),
            pixels: prop.pixels.len(),
            segments: segments(prop.pixels.len()),
            slices: sums
                .iter()
                .map(|cells| {
                    let means: Vec<u8> = cells
                        .iter()
                        .flat_map(|&[r, g, b, n]| {
                            [r, g, b].map(|sum| (sum + n / 2).checked_div(n).unwrap_or(0).min(255) as u8)
                        })
                        .collect();
                    hex(&means)
                })
                .collect(),
        })
        .collect();
    Ok(SequencePrint {
        label: label.to_string(),
        frames: alone.frame_count,
        frame_ms,
        digests: BTreeMap::from([(system(), format!("{hash:016x}"))]),
        props,
    })
}

/// Fingerprints every sequence of the manifest, as PixelFlow renders it now. `done` hears of
/// each as it is finished.
pub fn take(manifest: &Manifest, mut done: impl FnMut(&str)) -> Result<Fingerprints, FidelityError> {
    let mut sequences = Vec::new();
    for entry in &manifest.sequences {
        let label = crate::corpus::label_of(entry);
        let alone = render_alone(&entry.show, &entry.xsq, IDS)?;
        sequences.push(of(&label, alone)?);
        done(&label);
    }
    Ok(Fingerprints {
        about: "What PixelFlow alone draws for each sequence of the corpus: for each prop, each run of its \
                pixels, and each half second, the mean red, green, and blue. `pf-fidelity fingerprint` holds \
                renders to it; `pf-fidelity fingerprint --accept` writes it. See docs/fidelity.md."
            .to_string(),
        tool: crate::TOOL.to_string(),
        slice_ms: SLICE_MS,
        tolerance: TOLERANCE,
        sequences,
    })
}

/// Means of one prop that moved by more than the tolerance.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub sequence: String,
    pub prop: String,
    /// How many of its means moved, and of how many.
    pub cells: usize,
    pub of: usize,
    /// The most any moved, in levels.
    pub most: u8,
    /// The first and last half second it moved in, in milliseconds.
    pub from_ms: u64,
    pub to_ms: u64,
}

/// What holding a render to the fingerprint found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Checked {
    /// What can't be held at all: a sequence, a prop, or a length that isn't the same.
    pub broken: Vec<String>,
    pub moved: Vec<Moved>,
    /// Per sequence, what its digest says.
    pub notes: Vec<String>,
}

impl Checked {
    pub fn passed(&self) -> bool {
        self.broken.is_empty() && self.moved.is_empty()
    }
}

/// Holds `now` to the fingerprint `kept`.
pub fn check(kept: &Fingerprints, now: &Fingerprints) -> Checked {
    let mut checked = Checked::default();
    let system = system();
    for seq in &now.sequences {
        let Some(old) = kept.sequences.iter().find(|s| s.label == seq.label) else {
            checked.broken.push(format!(
                "{}: not in the fingerprint (a sequence added to the corpus)",
                seq.label
            ));
            continue;
        };
        if (old.frames, old.frame_ms) != (seq.frames, seq.frame_ms) {
            checked.broken.push(format!(
                "{}: {} frames of {} ms, where the fingerprint has {} of {} ms",
                seq.label, seq.frames, seq.frame_ms, old.frames, old.frame_ms
            ));
            continue;
        }
        match (old.digests.get(&system), seq.digests.get(&system)) {
            (Some(a), Some(b)) if a == b => checked
                .notes
                .push(format!("{}: the same to the bit as when the fingerprint was made ({system})", seq.label)),
            (Some(_), _) => checked.notes.push(format!(
                "{}: not the same bits as when the fingerprint was made on this system ({system}); held to the fingerprint",
                seq.label
            )),
            (None, _) => checked.notes.push(format!(
                "{}: the fingerprint was made on {}; held to the fingerprint, not the bits",
                seq.label,
                old.digests.keys().cloned().collect::<Vec<_>>().join(", ")
            )),
        }
        for prop in &seq.props {
            let Some(was) = old.props.iter().find(|p| p.name == prop.name) else {
                checked.broken.push(format!(
                    "{} / {}: a prop the fingerprint doesn't have",
                    seq.label, prop.name
                ));
                continue;
            };
            if (was.pixels, was.segments, was.slices.len()) != (prop.pixels, prop.segments, prop.slices.len())
            {
                checked.broken.push(format!(
                    "{} / {}: {} pixels in {} slices, where the fingerprint has {} in {}",
                    seq.label,
                    prop.name,
                    prop.pixels,
                    prop.slices.len(),
                    was.pixels,
                    was.slices.len()
                ));
                continue;
            }
            let mut moved = Moved {
                sequence: seq.label.clone(),
                prop: prop.name.clone(),
                cells: 0,
                of: 0,
                most: 0,
                from_ms: 0,
                to_ms: 0,
            };
            for (slice, (a, b)) in was.slices.iter().zip(&prop.slices).enumerate() {
                let (a, b) = (unhex(a), unhex(b));
                moved.of += a.len().max(b.len());
                let moves = a.len() != b.len() || a.iter().zip(&b).any(|(&x, &y)| x.abs_diff(y) > TOLERANCE);
                if !moves {
                    continue;
                }
                let at = slice as u64 * u64::from(SLICE_MS);
                if moved.cells == 0 {
                    moved.from_ms = at;
                }
                moved.to_ms = at + u64::from(SLICE_MS);
                for (&x, &y) in a.iter().zip(&b) {
                    if x.abs_diff(y) > TOLERANCE {
                        moved.cells += 1;
                        moved.most = moved.most.max(x.abs_diff(y));
                    }
                }
                if a.len() != b.len() {
                    moved.cells += a.len().abs_diff(b.len());
                    moved.most = u8::MAX;
                }
            }
            if moved.cells > 0 {
                checked.moved.push(moved);
            }
        }
        for was in &old.props {
            if !seq.props.iter().any(|p| p.name == was.name) {
                checked.broken.push(format!(
                    "{} / {}: a prop the render no longer has",
                    seq.label, was.name
                ));
            }
        }
    }
    for old in &kept.sequences {
        if !now.sequences.iter().any(|s| s.label == old.label) {
            checked.broken.push(format!(
                "{}: in the fingerprint, and no longer in the corpus",
                old.label
            ));
        }
    }
    checked
}

/// What to keep when a render is accepted: the render now, with the digests other systems made
/// kept where nothing this system shows has changed (the same means everywhere).
pub fn accepted(kept: Option<&Fingerprints>, mut now: Fingerprints) -> Fingerprints {
    let Some(kept) = kept else { return now };
    for seq in &mut now.sequences {
        let Some(old) = kept.sequences.iter().find(|s| s.label == seq.label) else {
            continue;
        };
        if old.props == seq.props && (old.frames, old.frame_ms) == (seq.frames, seq.frame_ms) {
            for (system, digest) in &old.digests {
                seq.digests
                    .entry(system.clone())
                    .or_insert_with(|| digest.clone());
            }
        }
    }
    now
}

/// A report of a check, in Markdown.
pub fn markdown(checked: &Checked) -> String {
    let mut out = String::from("## PixelFlow's own render of the corpus\n\n");
    if checked.passed() {
        let _ = writeln!(
            out,
            "Every sequence draws what its fingerprint has, within {TOLERANCE} levels of 255 in every mean.\n"
        );
    }
    if !checked.broken.is_empty() {
        out.push_str("What can't be held to the fingerprint:\n\n");
        for line in &checked.broken {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }
    if !checked.moved.is_empty() {
        let _ = writeln!(
            out,
            "What moved by more than {TOLERANCE} levels (means of a run of a prop's pixels over half a second):\n"
        );
        out.push_str("| Sequence | Prop | Means moved | Most | When |\n|---|---|---|---|---|\n");
        for m in &checked.moved {
            let _ = writeln!(
                out,
                "| {} | {} | {} of {} | {} | {:.1}–{:.1} s |",
                m.sequence,
                m.prop,
                m.cells,
                m.of,
                if m.most == u8::MAX {
                    "a run's count changed".to_string()
                } else {
                    format!("{} levels", m.most)
                },
                m.from_ms as f64 / 1000.0,
                m.to_ms as f64 / 1000.0
            );
        }
        out.push('\n');
    }
    for note in &checked.notes {
        let _ = writeln!(out, "- {note}");
    }
    out
}

/// Reads the fingerprint file in `corpus`, if there is one.
pub fn read(corpus: &Path) -> Result<Option<Fingerprints>, FidelityError> {
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

/// Writes `prints` to the fingerprint file in `corpus`.
pub fn write(corpus: &Path, prints: &Fingerprints) -> Result<(), FidelityError> {
    let path = corpus.join(FILE);
    let text = serde_json::to_string_pretty(prints).unwrap_or_default() + "\n";
    std::fs::write(&path, text).map_err(|source| FidelityError::Write { path, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(slices: &[&str]) -> Fingerprints {
        Fingerprints {
            about: String::new(),
            tool: String::new(),
            slice_ms: SLICE_MS,
            tolerance: TOLERANCE,
            sequences: vec![SequencePrint {
                label: "A".into(),
                frames: 20,
                frame_ms: 50,
                digests: BTreeMap::from([("elsewhere".to_string(), "0".to_string())]),
                props: vec![PropPrint {
                    name: "Matrix".into(),
                    pixels: 32,
                    segments: 2,
                    slices: slices.iter().map(|s| s.to_string()).collect(),
                }],
            }],
        }
    }

    #[test]
    fn a_mean_within_the_tolerance_passes_and_one_past_it_fails() {
        let kept = print(&["0a0a0a141414", "000000ffffff"]);
        assert!(check(&kept, &print(&["0c080a141414", "000000fdffff"])).passed());
        let moved = check(&kept, &print(&["0a0a0a141414", "000000fcffff"]));
        assert!(!moved.passed());
        assert_eq!(moved.moved[0].cells, 1);
        assert_eq!(moved.moved[0].most, 3);
        assert_eq!((moved.moved[0].from_ms, moved.moved[0].to_ms), (500, 1000));
    }

    #[test]
    fn a_prop_or_a_length_that_isnt_the_same_cant_be_held() {
        let kept = print(&["0a0a0a141414", "000000ffffff"]);
        let mut shorter = print(&["0a0a0a141414"]);
        shorter.sequences[0].frames = 10;
        assert!(!check(&kept, &shorter).broken.is_empty());
        let mut renamed = print(&["0a0a0a141414", "000000ffffff"]);
        renamed.sequences[0].props[0].name = "Tree".into();
        assert_eq!(check(&kept, &renamed).broken.len(), 2);
    }

    #[test]
    fn accepting_keeps_other_systems_digests_only_when_nothing_moved() {
        let kept = print(&["0a0a0a141414", "000000ffffff"]);
        let same = accepted(Some(&kept), print(&["0a0a0a141414", "000000ffffff"]));
        assert!(same.sequences[0].digests.contains_key("elsewhere"));
        let mut changed = print(&["0b0a0a141414", "000000ffffff"]);
        changed.sequences[0].digests.clear();
        let changed = accepted(Some(&kept), changed);
        assert!(changed.sequences[0].digests.is_empty());
    }

    #[test]
    fn props_are_cut_into_runs_of_sixteen_pixels_or_more() {
        assert_eq!(segments(5), 1);
        assert_eq!(segments(31), 1);
        assert_eq!(segments(80), 5);
        assert_eq!(segments(512), MAX_SEGMENTS);
    }
}
