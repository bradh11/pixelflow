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

/// A Text effect in a font this computer hasn't got is drawn in the pixel font here, so it isn't
/// held to a baseline made where the font is: the check lists it and passes over it.
#[test]
fn text_in_a_font_this_computer_lacks_is_listed_and_not_held() {
    let dir = TempDir::new().unwrap();
    small_corpus(dir.path());
    let path = dir.path().join("show/known.xsq");
    let twinkle = "    <Effect>E_SLIDER_Twinkle_Count=20,E_SLIDER_Twinkle_Steps=40</Effect>\n";
    let blue = "        <Effect ref=\"0\" name=\"On\" palette=\"2\" startTime=\"0\" endTime=\"2000\"/>\n";
    let known = std::fs::read_to_string(&path)
        .unwrap()
        .replace(
            twinkle,
            &format!(
                "{twinkle}    <Effect>E_CHOICE_Text_Font=Use OS Fonts,E_FONTPICKER_Text_Font='PixelFlow Made Up Narrow' 8,E_TEXTCTRL_Text=HI</Effect>\n"
            ),
        )
        .replace(
            blue,
            &format!("{blue}        <Effect ref=\"3\" name=\"Text\" palette=\"4\" startTime=\"2000\" endTime=\"4000\"/>\n"),
        );
    assert!(known.contains("name=\"Text\""));
    std::fs::write(&path, &known).unwrap();
    stand_in(dir.path(), "known", &known);
    let manifest = Manifest::read(&dir.path().join("manifest.json")).unwrap();
    let (with, without) = regress::references(&manifest);
    assert_eq!((with.len(), without.len()), (1, 1));
    let baseline = regress::measure(&with, 1, |_, _| {}).unwrap();
    let cards: Vec<_> = with
        .iter()
        .map(|e| regress::score(e, regress::IDS).unwrap())
        .collect();
    let checked = regress::check(Some(&baseline), &cards, &without);
    assert!(
        checked.passed(),
        "{}",
        regress::markdown(&checked, baseline.margin)
    );
    assert_eq!(checked.not_here.len(), 1, "{:?}", checked.not_here);
    let line = &checked.not_here[0];
    assert!(
        line.starts_with("Known #")
            && line.contains("Text on Matrix")
            && line.contains("isn't on this computer"),
        "{line}"
    );
    assert_eq!(checked.held, cards[0].effects.len() - 1);
    assert!(regress::markdown(&checked, baseline.margin).contains("Not held on this computer"));
}

/// A copy of the corpus's show and its moving heads and servos sequence, as a corpus of one with
/// a stand-in reference: PixelFlow's own render, so every fixture matches.
fn heads_corpus(dir: &Path) {
    let show = corpus().join("show");
    std::fs::create_dir_all(dir.join("show")).unwrap();
    for file in ["xlights_rgbeffects.xml", "xlights_networks.xml", "heads.xsq"] {
        std::fs::copy(show.join(file), dir.join("show").join(file)).unwrap();
    }
    std::fs::write(
        dir.join("manifest.json"),
        r#"{ "sequences": [
  { "label": "Heads", "show": "show", "xsq": "show/heads.xsq", "fseq": "show/heads.fseq" }
] }"#,
    )
    .unwrap();
    let bytes = render_fseq(&dir.join("show"), &dir.join("show/heads.xsq"), 7).unwrap();
    std::fs::write(dir.join("show/heads.fseq"), bytes).unwrap();
}

/// The reference's channel (counting from 0) of the first servo of the corpus's Arm, which a
/// scorecard found its channels at.
fn arm_servo(dir: &Path, card: &pf_fidelity::Scorecard) -> usize {
    let imported = pf_fidelity::layout::comparison_show(&dir.join("show")).unwrap();
    let arm = imported.show.props.iter().find(|p| p.name == "Arm").unwrap();
    let servo = arm
        .dmx
        .as_ref()
        .unwrap()
        .channels
        .iter()
        .position(|c| c.role == pf_model::DmxRole::Servo);
    let (first, _) = card
        .fixtures
        .iter()
        .find(|f| f.name == "Arm")
        .unwrap()
        .channels
        .unwrap();
    first as usize - 1 + servo.unwrap()
}

/// Rewrites the reference with `channel` turned by `by` levels (back the other way near the top)
/// in every frame: the servo 45° from where xLights had it.
fn turn(dir: &Path, channel: usize, by: u8) {
    let path = dir.join("show/heads.fseq");
    let mut fseq = pf_fseq::Sequence::open(&path).unwrap();
    let header = fseq.header().clone();
    let mut frames = Vec::new();
    for n in 0..header.frames {
        let mut frame = vec![0u8; header.channels as usize];
        fseq.read_frame(n, &mut frame).unwrap();
        let v = frame[channel];
        frame[channel] = if v < 255 - by { v + by } else { v - by };
        frames.push(frame);
    }
    drop(fseq);
    let options = pf_fseq::WriteOptions::new(header.channels, header.frames, header.step_ms);
    let mut writer = pf_fseq::FseqWriter::new(std::fs::File::create(&path).unwrap(), options).unwrap();
    for frame in &frames {
        writer.write_frame(frame).unwrap();
    }
    writer.finish().unwrap();
}

/// The corpus's heads and servos have channels, not pixels: each is held to xLights' channels,
/// and found the same next to PixelFlow's own render, and a servo turned elsewhere is found and
/// said so.
#[test]
fn the_corpus_heads_and_servos_are_held_to_the_reference_channel_by_channel() {
    let dir = TempDir::new().unwrap();
    heads_corpus(dir.path());
    let manifest = Manifest::read(&dir.path().join("manifest.json")).unwrap();
    let card = regress::score(&manifest.sequences[0], regress::IDS).unwrap();
    let names: Vec<&str> = card.fixtures.iter().map(|f| f.name.as_str()).collect();
    for name in ["Spot Left", "Spot Right", "Arm"] {
        assert!(names.contains(&name), "{names:?}");
    }
    for fixture in &card.fixtures {
        assert_eq!(
            (fixture.verdict, fixture.score),
            (Some(pf_fidelity::scorecard::Verdict::Matches), Some(100.0)),
            "{fixture:?}"
        );
        assert_eq!(fixture.frames, card.timing.frames_compared, "{}", fixture.name);
    }
    assert_eq!(card.dmx.judged, card.fixtures.len());
    assert_eq!(card.dmx.verdict, Some(pf_fidelity::scorecard::Verdict::Matches));
    // The heads are lit, and held to where they point and the light they show.
    let left = card.fixtures.iter().find(|f| f.name == "Spot Left").unwrap();
    let parts: Vec<&str> = left.parts.iter().map(|p| p.name.as_str()).collect();
    assert!(parts.contains(&"Pan") && parts.contains(&"Light"), "{parts:?}");
    // A fixture compared by its channels isn't listed as unmeasured.
    assert!(
        !card
            .coverage
            .unmeasured_props
            .iter()
            .any(|p| names.contains(&p.as_str()))
    );
    let report = pf_fidelity::report::sequence_markdown(&card);
    assert!(report.contains("## DMX fixtures") && report.contains("**The fixtures come across the same.**"));

    turn(dir.path(), arm_servo(dir.path(), &card), 64);
    let card = regress::score(&manifest.sequences[0], regress::IDS).unwrap();
    let arm = card.fixtures.iter().find(|f| f.name == "Arm").unwrap();
    assert_eq!(arm.verdict, Some(pf_fidelity::scorecard::Verdict::Different));
    let differs = arm.differs.as_deref().unwrap();
    assert!(
        differs.starts_with("servo 1 off by up to 45° in 100% of frames"),
        "{differs}"
    );
    assert_eq!(card.dmx.verdict, Some(pf_fidelity::scorecard::Verdict::Different));
    assert_eq!(
        card.dmx.differs.as_deref(),
        Some(format!("Arm: {differs}").as_str())
    );
    assert!(
        card.fixtures
            .iter()
            .filter(|f| f.name != "Arm")
            .all(|f| f.verdict == Some(pf_fidelity::scorecard::Verdict::Matches))
    );
}

/// The baseline holds each fixture's score and verdict, and a fixture that looks worse fails
/// the check. A baseline made before fixtures were compared still reads, and holds none of them.
#[test]
fn a_baseline_holds_the_fixtures_and_fails_one_that_looks_worse() {
    let dir = TempDir::new().unwrap();
    heads_corpus(dir.path());
    let manifest = Manifest::read(&dir.path().join("manifest.json")).unwrap();
    let (with, without) = regress::references(&manifest);
    let baseline = regress::measure(&with, 2, |_, _| {}).unwrap();
    let kept = baseline.sequences[0].fixtures.as_ref().unwrap();
    assert!(kept.iter().all(|f| f.scores == [Some(100.0); 2]), "{kept:?}");
    let score = || -> Vec<_> {
        with.iter()
            .map(|e| regress::score(e, regress::IDS).unwrap())
            .collect()
    };
    let cards = score();
    let checked = regress::check(Some(&baseline), &cards, &without);
    assert!(
        checked.passed(),
        "{}",
        regress::markdown(&checked, baseline.margin)
    );
    assert_eq!(checked.fixtures_held, cards[0].fixtures.len());
    assert!(checked.fixtures_moved.is_empty());
    assert!(regress::markdown(&checked, baseline.margin).contains("DMX fixtures held to it too"));

    // A baseline written before fixtures were compared has no `fixtures`: it reads, and holds
    // none of them, saying so.
    let mut old = serde_json::to_value(&baseline).unwrap();
    old["sequences"][0].as_object_mut().unwrap().remove("fixtures");
    let old: regress::Baseline = serde_json::from_value(old).unwrap();
    assert_eq!(old.sequences[0].fixtures, None);
    let checked = regress::check(Some(&old), &cards, &without);
    assert!(checked.passed());
    assert_eq!(
        (checked.fixtures_held, checked.fixtures_unheld.as_slice()),
        (0, ["Heads".to_string()].as_slice())
    );
    assert!(regress::markdown(&checked, old.margin).contains("made before fixtures were compared"));

    // xLights turned the Arm's first servo elsewhere: the fixture looks worse, and fails.
    turn(dir.path(), arm_servo(dir.path(), &cards[0]), 64);
    let checked = regress::check(Some(&baseline), &score(), &without);
    assert!(!checked.passed());
    let worse: Vec<_> = checked
        .fixtures_moved
        .iter()
        .filter(|m| m.how == regress::Move::Worse)
        .collect();
    assert_eq!(worse.len(), 1, "{worse:?}");
    assert_eq!(
        (worse[0].sequence.as_str(), worse[0].fixture.as_str()),
        ("Heads", "Arm")
    );
    assert!(
        worse[0]
            .what
            .contains("different where the baseline is at worst matches"),
        "{}",
        worse[0].what
    );
    assert!(
        worse[0].what.contains("servo 1 off by up to 45°"),
        "{}",
        worse[0].what
    );
    let report = regress::markdown(&checked, baseline.margin);
    assert!(
        report.contains("DMX fixtures worse than the baseline"),
        "{report}"
    );
}

/// The corpus's moving heads and servos, imported and rendered by PixelFlow alone, are sent what
/// the sequence's DMX effects set at their channels (where xLights puts them), with the colors
/// drawn on the heads in their color channels; where nothing sets them, the heads rest at Home.
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
    // Its sweep runs the whole 20 seconds: halfway at 9.75 s, all the way down at the end.
    let last = at(399);
    assert!(
        first[right] > 245 && (110..=145).contains(&late[right]) && last[right] < 10,
        "the pan sweeps down: {} {} {}",
        first[right],
        late[right],
        last[right]
    );
    assert_eq!(first[right + 1], 128);
    assert!(
        [0, 16, 32, 48].contains(&first[right + 4]),
        "the wheel at one of its colors"
    );
    assert!(first[arm] < 10 && late[arm] > 240, "the first servo turns");
    assert_eq!(&first[arm + 1..arm + 4], &[0, 64, 0]);

    // Where nothing moves them, the heads rest at Home: each motor's Home is the middle of its
    // travel (Spot Left's 16-bit pan and tilt, Spot Right's 8-bit ones, its tilt reversed).
    let imported = pf_fidelity::layout::comparison_show(&show).unwrap();
    let held = |name: &str, n: usize| -> Vec<u8> {
        let prop = imported.show.props.iter().find(|p| p.name == name).unwrap();
        let dmx = prop.dmx.as_ref().unwrap();
        dmx.channels[..n].iter().map(|c| c.value).collect()
    };
    assert_eq!(
        held("Spot Left", 4),
        [128, 0, 128, 0],
        "pan, pan fine, tilt, tilt fine"
    );
    assert_eq!(held("Spot Right", 2), [128, 128], "pan, tilt");

    // From 20 s, xLights' Moving Head and Servo effects, as PixelFlow's own. Where each head
    // points, in degrees from Home, read back through its own motors.
    let fixture = |name: &str| {
        let prop = imported.show.props.iter().find(|p| p.name == name).unwrap();
        prop.dmx.clone().unwrap()
    };
    let degrees = |frame: &[u8], name: &str, at: usize, role: pf_model::DmxRole| {
        let dmx = fixture(name);
        let axis = dmx.axes().into_iter().find(|a| a.role == role).unwrap();
        axis.degrees(&frame[at..at + dmx.footprint()])
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1.5;
    use pf_model::DmxRole::{Pan, Servo, Tilt};
    // 22.5 s: both at pan 45 and tilt 20, Spot Left in red.
    let still = at(450);
    for (name, place) in [("Spot Left", left), ("Spot Right", right)] {
        let (pan, tilt) = (
            degrees(&still, name, place, Pan),
            degrees(&still, name, place, Tilt),
        );
        assert!(near(pan, 45.0) && near(tilt, 20.0), "{name}: {pan} {tilt}");
    }
    assert_eq!(&still[left + 5..left + 10], &[255, 255, 0, 0, 0], "dimmer, red");
    // 27.5 s: fanned 30 apart about the middle, at tilt 30 (Spot Left first in the group).
    let fanned = at(550);
    assert!(near(degrees(&fanned, "Spot Left", left, Pan), -15.0));
    assert!(near(degrees(&fanned, "Spot Right", right, Pan), 15.0));
    assert!(near(degrees(&fanned, "Spot Left", left, Tilt), 25.0));
    assert!(near(degrees(&fanned, "Spot Right", right, Tilt), 35.0));
    // 25 s: the first servo half way from 10% to 90% of its travel, at Home.
    assert!(near(degrees(&at(500), "Arm", arm, Servo), 0.0));
}
