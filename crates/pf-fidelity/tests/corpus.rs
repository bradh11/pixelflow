//! The corpus kept in the repository (`crates/pf-fidelity/corpus`), and the two checks CI holds
//! it to: PixelFlow's own render against its fingerprint, and the looks next to xLights'
//! references against the baseline.

use pf_fidelity::compare::render_fseq;
use pf_fidelity::corpus::Manifest;
use pf_fidelity::madeup::CORPUS_DIR;
use pf_fidelity::{fingerprint, regress};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn corpus() -> PathBuf {
    PathBuf::from(CORPUS_DIR)
}

/// What PixelFlow draws for the corpus is what it drew when the fingerprint was made. A change
/// meant to change it is accepted with `cargo run --release -p pf-fidelity -- fingerprint
/// --accept` (docs/fidelity.md).
#[test]
fn pixelflow_draws_what_the_corpus_fingerprint_has() {
    let manifest = Manifest::read(&corpus().join("manifest.json")).unwrap();
    let kept = fingerprint::read(&corpus())
        .unwrap()
        .expect("the corpus has a fingerprint.json");
    let now = fingerprint::take(&manifest, |_| {}).unwrap();
    let checked = fingerprint::check(&kept, &now);
    assert!(
        checked.passed(),
        "{}\nIf the change is meant, run `cargo run --release -p pf-fidelity -- fingerprint --accept`.",
        fingerprint::markdown(&checked)
    );
}

/// Until xLights' references are rendered, the baseline holds nothing and the check passes,
/// listing every sequence as having no reference yet.
#[test]
fn the_baseline_holds_nothing_until_there_are_references() {
    let manifest = Manifest::read(&corpus().join("manifest.json")).unwrap();
    let (with, without) = regress::references(&manifest);
    let baseline = regress::read(&corpus())
        .unwrap()
        .expect("the corpus has a baseline.json");
    if !with.is_empty() {
        // The references have arrived: the baseline must cover them (CI's check says the rest).
        assert!(
            !baseline.sequences.is_empty(),
            "references without a baseline: run regress --accept"
        );
        return;
    }
    assert!(baseline.sequences.is_empty());
    let checked = regress::check(Some(&baseline), &[], &without);
    assert!(checked.passed());
    assert_eq!(checked.unrendered.len(), manifest.sequences.len());
    assert!(regress::markdown(&checked, baseline.margin).contains("No xLights reference yet"));
}

/// A copy of the fidelity tests' small show and its two sequences, as a corpus with a manifest
/// and no references yet.
fn small_corpus(dir: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/show");
    std::fs::create_dir_all(dir.join("show")).unwrap();
    for file in [
        "xlights_rgbeffects.xml",
        "xlights_networks.xml",
        "known.xsq",
        "chance.xsq",
    ] {
        std::fs::copy(fixtures.join(file), dir.join("show").join(file)).unwrap();
    }
    std::fs::write(
        dir.join("manifest.json"),
        r#"{ "sequences": [
  { "label": "Known", "show": "show", "xsq": "show/known.xsq", "fseq": "show/known.fseq" },
  { "label": "Chance", "show": "show", "xsq": "show/chance.xsq", "fseq": "show/chance.fseq" }
] }"#,
    )
    .unwrap();
}

/// Renders a stand-in reference for `name` from `xsq_text` (the sequence as xLights had it).
fn stand_in(dir: &Path, name: &str, xsq_text: &str) {
    let other = dir.join(format!("show/{name}-as-rendered.xsq"));
    std::fs::write(&other, xsq_text).unwrap();
    let bytes = render_fseq(&dir.join("show"), &other, 7).unwrap();
    std::fs::write(dir.join(format!("show/{name}.fseq")), bytes).unwrap();
    std::fs::remove_file(other).unwrap();
}

#[test]
fn a_baseline_holds_its_own_scoring_exactly_and_fails_an_import_that_looks_worse() {
    let dir = TempDir::new().unwrap();
    small_corpus(dir.path());
    let manifest = Manifest::read(&dir.path().join("manifest.json")).unwrap();
    // No references: nothing to measure, nothing held, every sequence listed as unrendered.
    let (with, without) = regress::references(&manifest);
    assert!(with.is_empty());
    assert_eq!(without, ["Known", "Chance"]);

    let known = std::fs::read_to_string(dir.path().join("show/known.xsq")).unwrap();
    let chance = std::fs::read_to_string(dir.path().join("show/chance.xsq")).unwrap();
    stand_in(dir.path(), "known", &known);
    stand_in(dir.path(), "chance", &chance);
    let (with, without) = regress::references(&manifest);
    assert_eq!((with.len(), without.len()), (2, 0));

    let baseline = regress::measure(&with, 3, |_, _| {}).unwrap();
    assert_eq!(baseline.sequences.len(), 2);
    assert!(baseline.sequences.iter().all(|s| s.looks.len() == 3));
    regress::write(dir.path(), &baseline).unwrap();
    assert_eq!(regress::read(dir.path()).unwrap().as_ref(), Some(&baseline));

    // Scored again from the baseline's first ids: the same scorecard, so nothing moves at all.
    let score = |entries: &[&pf_fidelity::corpus::Entry]| -> Vec<_> {
        entries
            .iter()
            .map(|e| regress::score(e, regress::IDS).unwrap())
            .collect()
    };
    let checked = regress::check(Some(&baseline), &score(&with), &without);
    assert!(
        checked.passed(),
        "{}",
        regress::markdown(&checked, baseline.margin)
    );
    assert!(checked.moved.is_empty());
    for (seq, card) in baseline.sequences.iter().zip(score(&with)) {
        for (b, e) in seq.effects.iter().zip(&card.effects) {
            let now = e
                .look
                .as_ref()
                .map(|j| j.score)
                .or(e.says_little.as_ref().map(|l| l.score));
            assert_eq!(
                b.looks[0],
                now.map(|s| (s * 100.0).round() / 100.0),
                "{} #{}",
                seq.label,
                b.index
            );
        }
    }

    // xLights drew the first second's red as cyan: the import now looks worse there.
    stand_in(dir.path(), "known", &known.replacen("#FF0000", "#00FFFF", 1));
    let checked = regress::check(Some(&baseline), &score(&with), &without);
    assert!(!checked.passed());
    let worse: Vec<_> = checked
        .moved
        .iter()
        .filter(|m| m.how == regress::Move::Worse)
        .collect();
    assert!(
        worse
            .iter()
            .any(|m| m.sequence == "Known" && m.kind == "On" && m.start_ms == 0),
        "{}",
        regress::markdown(&checked, baseline.margin)
    );
    assert!(regress::markdown(&checked, baseline.margin).contains("Worse than the baseline"));

    // A reference the baseline expects that has gone missing fails too.
    std::fs::remove_file(dir.path().join("show/chance.fseq")).unwrap();
    let (with, without) = regress::references(&manifest);
    stand_in(dir.path(), "known", &known);
    let checked = regress::check(Some(&baseline), &score(&with), &without);
    assert!(checked.broken.iter().any(|b| b.contains("Chance")));
}

#[test]
fn a_sequence_with_a_reference_and_no_baseline_cant_be_held() {
    let dir = TempDir::new().unwrap();
    small_corpus(dir.path());
    let known = std::fs::read_to_string(dir.path().join("show/known.xsq")).unwrap();
    stand_in(dir.path(), "known", &known);
    let manifest = Manifest::read(&dir.path().join("manifest.json")).unwrap();
    let (with, without) = regress::references(&manifest);
    let cards: Vec<_> = with
        .iter()
        .map(|e| regress::score(e, regress::IDS).unwrap())
        .collect();
    let checked = regress::check(Some(&regress::empty()), &cards, &without);
    assert!(!checked.passed());
    assert!(checked.broken[0].contains("regress --accept"));
    assert_eq!(checked.unrendered, ["Chance"]);
}

/// The corpus's moving heads and servos, imported and rendered by PixelFlow alone, are sent what
/// the sequence's DMX effects set at their channels (where xLights puts them), with the colors
/// drawn on the heads in their color channels.
#[test]
fn the_corpus_heads_and_servos_get_their_channels() {
    let show = corpus().join("show");
    let bytes = render_fseq(&show, &show.join("heads.xsq"), regress::IDS).unwrap();
    let mut fseq = pf_fseq::Sequence::from_reader(std::io::Cursor::new(bytes)).unwrap();
    let channels = fseq.header().channels as usize;
    assert_eq!(channels, 3_297);
    let mut at = |n: u32| {
        let mut frame = vec![0u8; channels];
        fseq.read_frame(n, &mut frame).unwrap();
        frame
    };
    // Spot Left at 3274, Spot Right at 3286, Arm at 3294 (0-based: 3273, 3285, 3293).
    let (left, right, arm) = (3273, 3285, 3293);
    let first = at(0);
    let late = at(195);
    assert!(first[left] < 10 && late[left] > 240, "the pan sweeps up");
    assert_eq!(
        &first[left + 1..left + 4],
        &[0, 96, 0],
        "pan fine, tilt, tilt fine as set"
    );
    assert_eq!(first[left + 5], 255, "the dimmer, from the red On under it");
    assert_eq!(&first[left + 6..left + 10], &[255, 0, 0, 0], "red");
    assert_eq!(first[left + 10], 255, "the shutter held open");
    assert!(first[right] > 245 && late[right] < 10, "the pan sweeps down");
    assert_eq!(first[right + 1], 128);
    assert!(
        [0, 16, 32, 48].contains(&first[right + 4]),
        "the wheel at one of its colors"
    );
    assert!(first[arm] < 10 && late[arm] > 240, "the first servo turns");
    assert_eq!(&first[arm + 1..arm + 4], &[0, 64, 0]);
}
