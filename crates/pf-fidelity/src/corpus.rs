//! Running a list of sequences: a manifest names each one's show folder, `.xsq`, and the `.fseq`
//! xLights rendered from it.
//!
//! ```json
//! { "sequences": [
//!   { "label": "sequence A", "show": "show", "xsq": "show/a.xsq", "fseq": "show/a.fseq" }
//! ] }
//! ```
//!
//! Paths are relative to the manifest's folder (or absolute). `label` is what the reports call
//! the sequence (its file's name otherwise); `frameOffset` compares against xLights' frames that
//! many later.

use crate::compare::{Options, Request, compare};
use crate::report::{corpus_markdown, sequence_markdown};
use crate::scorecard::{CorpusCard, Scorecard};
use crate::{FidelityError, TOOL};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The environment variable naming the manifest when the command line doesn't.
pub const MANIFEST_VARIABLE: &str = "PF_FIDELITY_CORPUS";

/// One sequence to compare.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    #[serde(default)]
    pub label: Option<String>,
    /// The xLights show folder the reference was rendered on.
    pub show: PathBuf,
    pub xsq: PathBuf,
    pub fseq: PathBuf,
    #[serde(default)]
    pub frame_offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub sequences: Vec<Entry>,
}

impl Manifest {
    /// Reads the manifest at `path`, its paths made to start from the manifest's folder.
    pub fn read(path: &Path) -> Result<Self, FidelityError> {
        let text = fs::read_to_string(path).map_err(|source| FidelityError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut manifest: Manifest = serde_json::from_str(&text).map_err(|e| FidelityError::Manifest {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
        let folder = path.parent().unwrap_or(Path::new(""));
        for entry in &mut manifest.sequences {
            for file in [&mut entry.show, &mut entry.xsq, &mut entry.fseq] {
                *file = folder.join(&*file);
            }
        }
        Ok(manifest)
    }
}

/// What a corpus run measured.
#[derive(Debug, Clone)]
pub struct CorpusRun {
    pub card: CorpusCard,
    pub scorecards: Vec<Scorecard>,
}

/// Compares every sequence in the manifest. One that can't be compared (a file missing, say) is
/// noted in the corpus scorecard and the rest still run. `done` hears of each as it finishes.
pub fn run(
    manifest: &Manifest,
    options: &Options,
    mut done: impl FnMut(&str, Result<&Scorecard, &FidelityError>),
) -> CorpusRun {
    let mut card = CorpusCard::new(TOOL);
    let mut scorecards = Vec::new();
    for entry in &manifest.sequences {
        let label = label_of(entry);
        let request = Request {
            show: &entry.show,
            xsq: &entry.xsq,
            fseq: &entry.fseq,
        };
        let options = Options {
            label: Some(label.clone()),
            frame_offset: entry.frame_offset,
            ..options.clone()
        };
        match compare(request, options) {
            Ok(scorecard) => {
                card.add(&scorecard);
                done(&label, Ok(&scorecard));
                scorecards.push(scorecard);
            }
            Err(e) => {
                card.fail(&label, &e.to_string());
                done(&label, Err(&e));
            }
        }
    }
    CorpusRun { card, scorecards }
}

/// What the reports call a sequence: its label, or its file's name.
pub fn label_of(entry: &Entry) -> String {
    entry.label.clone().unwrap_or_else(|| {
        entry
            .xsq
            .file_stem()
            .map_or_else(|| "sequence".to_string(), |s| s.to_string_lossy().into_owned())
    })
}

/// A label as a file name: lowercase letters and digits, dashes for everything else.
pub fn file_name(label: &str) -> String {
    let mut name = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "sequence".to_string()
    } else {
        name.to_string()
    }
}

fn write(path: &Path, contents: &str) -> Result<(), FidelityError> {
    fs::write(path, contents).map_err(|source| FidelityError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn json<T: serde::Serialize>(value: &T) -> String {
    // Scorecards hold only numbers, text, and lists: they always serialize.
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Writes one sequence's scorecard to `dir` as `<name>.json` and `<name>.md`.
pub fn write_scorecard(dir: &Path, name: &str, card: &Scorecard) -> Result<Vec<PathBuf>, FidelityError> {
    fs::create_dir_all(dir).map_err(|source| FidelityError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let (data, summary) = (dir.join(format!("{name}.json")), dir.join(format!("{name}.md")));
    write(&data, &json(card))?;
    write(&summary, &sequence_markdown(card))?;
    Ok(vec![data, summary])
}

/// Writes a corpus run to `dir`: each sequence's scorecard (numbered in the manifest's order),
/// and `corpus.json` and `corpus.md` over all of them.
pub fn write_run(dir: &Path, run: &CorpusRun) -> Result<Vec<PathBuf>, FidelityError> {
    let mut written = Vec::new();
    for (i, card) in run.scorecards.iter().enumerate() {
        let name = format!("{:02}-{}", i + 1, file_name(&card.sequence.label));
        written.extend(write_scorecard(dir, &name, card)?);
    }
    fs::create_dir_all(dir).map_err(|source| FidelityError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let (data, summary) = (dir.join("corpus.json"), dir.join("corpus.md"));
    write(&data, &json(&run.card))?;
    write(&summary, &corpus_markdown(&run.card))?;
    written.extend([data, summary]);
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_make_plain_file_names() {
        assert_eq!(file_name("Local corpus: sequence A"), "local-corpus-sequence-a");
        assert_eq!(file_name("  (2024) "), "2024");
        assert_eq!(file_name("***"), "sequence");
    }
}
