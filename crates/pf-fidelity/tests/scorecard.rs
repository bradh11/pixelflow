//! Scoring a made-up show against references written here by hand: every frame is known, so
//! every number in the scorecard can be checked.
//!
//! The show (`fixtures/show`) has Strip on channels 1–30 and Matrix on 31–60 of a controller,
//! Porch (GRB) on 61–90 with no controller, and Far on 91–105. Its sequence (`known.xsq`, 50 ms
//! frames): Strip red for a second, then green, then twinkling; Matrix blue and Porch orange for
//! two seconds; and everything in the group white for the last second.

use pf_fidelity::compare::{OTHER_SEEDS, Options, Request, compare};
use pf_fidelity::corpus::{self, Manifest};
use pf_fidelity::look::{Chance, Component, LITTLE};
use pf_fidelity::metrics::Limits;
use pf_fidelity::report::{corpus_markdown, headline, sequence_markdown};
use pf_fidelity::scorecard::{Basis, Class, EffectScore, Label, Scorecard, Source, Verdict};
use pf_fidelity::{FidelityError, labels, strips};
use pf_fseq::{FseqWriter, WriteOptions};
use std::fs::File;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Channels the whole layout uses.
const LAYOUT_CHANNELS: usize = 105;

fn show() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/show")
}

fn xsq() -> PathBuf {
    show().join("known.xsq")
}

/// A reference as xLights might have rendered the sequence.
#[derive(Debug, Clone, Copy)]
struct Reference {
    channels: usize,
    frames: u32,
    step_ms: u32,
    /// How bright xLights drew Matrix's blue (PixelFlow draws 255).
    matrix_blue: u8,
    /// Far red for the first two seconds (for [`STAND_IN`], whose only effect is on Far).
    far_red: bool,
    /// Frames late everything is.
    late: i64,
    /// Saved while xLights was still rendering: the group's white over the last second was never
    /// drawn, and Strip's own twinkles only as far as three and a half seconds.
    cut_short: bool,
}

impl Default for Reference {
    fn default() -> Self {
        Self {
            channels: LAYOUT_CHANNELS,
            frames: 80,
            step_ms: 50,
            matrix_blue: 255,
            far_red: false,
            late: 0,
            cut_short: false,
        }
    }
}

impl Reference {
    /// Every channel of the layout at `t_ms`.
    fn at(&self, t_ms: i64) -> Vec<u8> {
        let mut frame = vec![0u8; LAYOUT_CHANNELS];
        let mut fill = |channels: std::ops::Range<usize>, color: [u8; 3]| {
            for pixel in frame[channels].chunks_mut(3) {
                pixel.copy_from_slice(&color);
            }
        };
        match t_ms {
            0..2000 => {
                fill(0..30, if t_ms < 1000 { [255, 0, 0] } else { [0, 255, 0] });
                fill(30..60, [0, 0, self.matrix_blue]);
                // Orange (255, 128, 0) as a GRB prop's channels carry it.
                fill(60..90, [128, 255, 0]);
            }
            3000..4000 if !self.cut_short => fill(0..90, [255; 3]),
            // Twinkles: a quarter of Strip's pixels white, moving along.
            2000..3500 => {
                for pixel in 0..10 {
                    if (t_ms / 50 + pixel * 3) % 4 == 0 {
                        fill(pixel as usize * 3..pixel as usize * 3 + 3, [255; 3]);
                    }
                }
            }
            _ => {}
        }
        if self.far_red && (0..2000).contains(&t_ms) {
            fill(90..105, [255, 0, 0]);
        }
        frame
    }

    /// Writes the reference into `dir`.
    fn write(&self, dir: &TempDir) -> PathBuf {
        let path = dir.path().join("known.fseq");
        let options = WriteOptions::new(self.channels as u32, self.frames, self.step_ms);
        let mut writer = FseqWriter::new(File::create(&path).unwrap(), options).unwrap();
        for frame in 0..i64::from(self.frames) {
            let all = self.at((frame - self.late) * i64::from(self.step_ms));
            writer.write_frame(&all[..self.channels]).unwrap();
        }
        writer.finish().unwrap();
        path
    }
}

fn score_with(reference: Reference, options: Options) -> Scorecard {
    let dir = TempDir::new().unwrap();
    let fseq = reference.write(&dir);
    let request = Request {
        show: &show(),
        xsq: &xsq(),
        fseq: &fseq,
    };
    compare(request, options).unwrap()
}

/// Every effect held moment by moment against the reference alone. The sequence's twinkles
/// are ten pixels for two seconds: next to PixelFlow's other seeds what they earn is a matter of
/// the seeds an import happens to draw (see
/// `a_few_twinkling_pixels_are_found_different_or_say_too_little`), and these tests are of
/// other things.
fn by_moment() -> Options {
    Options {
        by_moment: true,
        ..Options::default()
    }
}

fn score(reference: Reference) -> Scorecard {
    score_with(reference, by_moment())
}

/// The effect of `kind` on `target` starting at `start_ms`.
fn effect<'a>(card: &'a Scorecard, kind: &str, target: &str, start_ms: u64) -> &'a EffectScore {
    card.effects
        .iter()
        .find(|e| e.kind == kind && e.target == target && e.start_ms == start_ms)
        .unwrap_or_else(|| panic!("no {kind} on {target} at {start_ms}"))
}

#[test]
fn a_reference_that_matches_scores_perfectly_and_coverage_says_what_was_measured() {
    let card = score(Reference::default());
    let coverage = &card.coverage;
    assert_eq!((coverage.props, coverage.props_measured), (4, 4));
    assert_eq!((coverage.pixels, coverage.pixels_measured), (35, 35));
    // Strip and Matrix are on the controller; Porch and Far, on none, have stand-ins.
    assert_eq!(
        (coverage.pixels_on_controllers, coverage.pixels_on_stand_ins),
        (20, 15)
    );
    assert!(coverage.unmeasured_props.is_empty());
    assert_eq!((coverage.effects, coverage.effects_measured), (6, 6));
    let on: Vec<_> = card
        .props
        .iter()
        .map(|p| (p.name.as_str(), p.on, p.channels))
        .collect();
    assert_eq!(
        on,
        [
            ("Strip", Source::Controller, Some((1, 30))),
            ("Matrix", Source::Controller, Some((31, 60))),
            ("Porch", Source::StandIn, Some((61, 90))),
            ("Far", Source::StandIn, Some((91, 105))),
        ]
    );
    // Strip is at 40% brightness and gamma 2.2 on its controller: xLights' file holds what it
    // rendered before those, so they're set aside and Strip still matches.
    let aside = card.method.set_aside;
    assert_eq!((aside.brightness, aside.gamma, aside.reverse), (1, 1, 0));

    // Matrix and Porch never twinkle: every frame of theirs matches. Far is never lit.
    for prop in &card.props[1..3] {
        let scores = prop.scores.unwrap();
        assert_eq!(
            (
                scores.mae,
                scores.within_tolerance,
                scores.lit_iou,
                scores.pixel_seconds
            ),
            (0.0, 1.0, 1.0, 30.0),
            "{}",
            prop.name
        );
        assert_eq!(scores.brightness_correlation, None, "{}: steady", prop.name);
    }
    assert!(card.props[3].scores.is_none());
    assert!(card.diagnostics.overall.is_some());
    for (kind, target, start, seconds) in [
        ("On", "Strip", 0, 10.0),
        ("On", "Strip", 1000, 10.0),
        ("On", "Matrix", 0, 20.0),
        // A GRB prop: its channels are read back in its own order.
        ("On", "Porch", 0, 20.0),
    ] {
        let solo = effect(&card, kind, target, start).solo.unwrap();
        assert_eq!(
            (
                solo.pixel_seconds,
                solo.mae,
                solo.rmse,
                solo.within_tolerance,
                solo.lit_iou
            ),
            (seconds, 0.0, 0.0, 1.0, 1.0),
            "{kind} on {target} at {start}"
        );
    }
    assert_eq!(
        (
            card.timing.frames,
            card.timing.frames_compared,
            card.timing.frames_missing
        ),
        (80, 80, 0)
    );
    assert_eq!(card.timing.best_offset, 0);
    assert!(card.diagnostics.label_audit.exact_but_differs.is_empty());
    assert!(card.reference.left_out.is_empty());
    assert!(card.notes.is_empty(), "{:#?}", card.notes);
}

#[test]
fn a_twinkle_wont_match_pixel_for_pixel_and_isnt_asked_to() {
    let card = score(Reference::default());
    let twinkle = effect(&card, "Twinkle", "Strip", 2000);
    assert_eq!(twinkle.class, Class::Stochastic);
    assert!(twinkle.class_reason.as_deref().unwrap().contains("random"));
    // Ranked with the stochastic effects, by its distribution; never among the deterministic.
    assert!(card.diagnostics.ranking.iter().all(|r| r.kind != "Twinkle"));
    assert_eq!(card.diagnostics.stochastic_ranking[0].kind, "Twinkle");
    let solo = twinkle.solo.unwrap();
    assert!((0.0..=1.0).contains(&solo.distribution.distance), "{solo:?}");
    assert!(
        card.diagnostics
            .label_audit
            .exact_but_differs
            .iter()
            .all(|e| e.kind != "Twinkle")
    );
}

#[test]
fn error_is_attributed_to_the_effects_on_the_pixel_alone_and_layered() {
    let card = score(Reference::default());
    // The group's white is alone on Matrix and Porch (20 pixels for a second) and over the
    // twinkles on Strip (10 more): half of each of those pixels counts as its share.
    let white = effect(&card, "On", "All", 3000);
    assert_eq!((white.pixels, white.measured_pixels, white.layer), (30, 30, 0));
    assert_eq!(white.solo.unwrap().pixel_seconds, 20.0);
    assert_eq!(white.layered.unwrap().pixel_seconds, 30.0);
    assert_eq!(white.share, 25.0);
    assert_eq!(white.basis(&card.method.audit).unwrap().0, Basis::Solo);
    // The twinkles are alone for their first second, under the white for their second.
    let twinkle = effect(&card, "Twinkle", "Strip", 2000);
    assert_eq!(twinkle.solo.unwrap().pixel_seconds, 10.0);
    assert_eq!(twinkle.layered.unwrap().pixel_seconds, 20.0);
    assert_eq!(twinkle.share, 15.0);
    // Under the white PixelFlow and xLights both show white: no error from the layered part.
    assert_eq!(white.layered.unwrap().mae, 0.0);
    // Nothing was lit where no effect was.
    assert!(card.diagnostics.unattributed.is_none());

    let on = card.kinds.iter().find(|k| k.kind == "On").unwrap();
    assert_eq!(
        (on.instances, on.measured, on.class),
        (5, 5, Class::Deterministic)
    );
    assert_eq!((on.labels.exact, on.labels.approximate), (4, 1));
    assert_eq!(on.pixel_seconds, 10.0 + 10.0 + 20.0 + 20.0 + 25.0);
}

#[test]
fn the_label_audit_names_exact_effects_that_differ_and_approximate_ones_that_match() {
    // xLights drew Matrix's blue at 200 where PixelFlow draws 255.
    let card = score(Reference {
        matrix_blue: 200,
        ..Reference::default()
    });
    let blue = effect(&card, "On", "Matrix", 0);
    assert_eq!(blue.label, Label::Exact);
    let solo = blue.solo.unwrap();
    // One channel of three is 55 off, on every pixel.
    assert_eq!((solo.mae, solo.rmse), (18.3333, 31.7543));
    assert_eq!((solo.within_tolerance, solo.lit_iou), (0.0, 1.0));
    assert_eq!(solo.hue_difference, Some(0.0));
    assert_eq!((solo.level.pixelflow, solo.level.xlights), (255.0, 200.0));

    let audit = &card.diagnostics.label_audit;
    let named: Vec<(&str, &str, u64, Basis)> = audit
        .exact_but_differs
        .iter()
        .map(|e| (e.kind.as_str(), e.target.as_str(), e.start_ms, e.basis))
        .collect();
    assert_eq!(named, [("On", "Matrix", 0, Basis::Solo)]);
    assert_eq!(audit.exact_but_differs[0].index, blue.index);
    assert!(audit.exact_but_differs_layered.is_empty());
    let on = audit.exact_kinds.iter().find(|k| k.kind == "On").unwrap();
    assert_eq!((on.exact, on.judged, on.differ, on.differ_alone), (4, 4, 1, 1));

    // The green On has a layer mix amount PixelFlow doesn't apply, so the importer notes a
    // difference; the reference shows it as PixelFlow draws it, so it matches.
    let green = effect(&card, "On", "Strip", 1000);
    assert_eq!(green.label, Label::Approximate);
    assert!(
        green.notes.iter().any(|n| n.contains("part-way")),
        "{:?}",
        green.notes
    );
    let matching: Vec<usize> = audit.approximate_but_matches.iter().map(|e| e.index).collect();
    assert!(matching.contains(&green.index), "{matching:?}");

    // What to fix first: the only deterministic kind with any error, by error times
    // pixel-seconds (18.33 on Matrix's 20).
    let first = &card.diagnostics.ranking[0];
    assert_eq!((first.kind.as_str(), first.share), ("On", 1.0));
    assert!((first.impact - 18.3333 * 20.0).abs() < 0.01, "{first:?}");
    assert!((first.error - 18.3333 * 20.0 / 85.0).abs() < 0.001, "{first:?}");

    let summary = sequence_markdown(&card);
    assert!(summary.contains("#### Carried over exactly, not the same pixels"));
    assert!(
        summary.contains("| On | Matrix | 0:00.0–0:02.0 | 18.3 | 0% | 1.00 |"),
        "{summary}"
    );
}

#[test]
fn frames_are_lined_up_by_time_and_a_late_reference_shows_in_the_timing_check() {
    // The same picture at 25 ms a frame: every PixelFlow frame has its moment's frame.
    let card = score(Reference {
        frames: 160,
        step_ms: 25,
        ..Reference::default()
    });
    assert_eq!((card.timing.frame_ms, card.timing.reference_step_ms), (50, 25));
    assert_eq!((card.timing.frames_compared, card.timing.frames_missing), (80, 0));
    assert_eq!(effect(&card, "On", "Strip", 1000).solo.unwrap().mae, 0.0);
    assert!(
        card.notes.iter().any(|n| n.contains("25 ms")),
        "{:#?}",
        card.notes
    );

    // Everything one frame late: each effect is wrong on its first frame, and the timing check
    // finds the offset that isn't.
    let late = Reference {
        late: 1,
        ..Reference::default()
    };
    let card = score(late);
    assert_eq!(card.timing.best_offset, 1);
    let red = effect(&card, "On", "Strip", 0);
    assert!(red.solo.unwrap().mae > 0.0);
    assert_eq!(
        red.onset.map(|o| (o.pixelflow, o.xlights)),
        Some((0, 1)),
        "xLights lights it a frame later"
    );
    assert_eq!(
        card.timing.onset_deltas.get(&1),
        Some(&3),
        "{:?}",
        card.timing.onset_deltas
    );
    // Measured at that offset it matches again.
    let card = score_with(
        late,
        Options {
            frame_offset: 1,
            ..Options::default()
        },
    );
    assert_eq!(effect(&card, "On", "Strip", 0).solo.unwrap().mae, 0.0);
    assert_eq!((card.timing.frame_offset, card.timing.best_offset), (1, 1));
}

#[test]
fn a_short_or_narrow_reference_is_measured_as_far_as_it_goes() {
    // Two seconds of four: the rest has nothing to compare with.
    let card = score(Reference {
        frames: 40,
        ..Reference::default()
    });
    assert_eq!(
        (card.timing.frames_compared, card.timing.frames_missing),
        (40, 40)
    );
    assert!(
        card.notes.iter().any(|n| n.contains("40 of 80 frames")),
        "{:#?}",
        card.notes
    );
    assert!(effect(&card, "On", "All", 3000).layered.is_none());
    assert_eq!(effect(&card, "On", "Matrix", 0).solo.unwrap().pixel_seconds, 20.0);
    assert_eq!((card.coverage.effects, card.coverage.effects_measured), (6, 4));
    // An effect with nothing measured isn't judged: the group's On is labelled exact.
    assert!(card.diagnostics.label_audit.exact_unjudged >= 1);
    let on = card
        .diagnostics
        .label_audit
        .exact_kinds
        .iter()
        .find(|k| k.kind == "On")
        .unwrap();
    assert_eq!((on.exact, on.judged, on.differ), (4, 3, 0));

    // Only the controller's 60 channels: Porch has none in the reference either.
    let card = score(Reference {
        channels: 60,
        ..Reference::default()
    });
    assert_eq!(card.coverage.unmeasured_props, ["Porch", "Far"]);
    assert_eq!(card.coverage.pixels_measured, 20);
    assert!(effect(&card, "On", "Porch", 0).layered.is_none());
    assert_eq!(effect(&card, "On", "All", 3000).measured_pixels, 20);
    assert!(
        card.notes
            .iter()
            .any(|n| n.contains("60 channels") && n.contains("105")),
        "{:#?}",
        card.notes
    );
    assert_eq!(effect(&card, "On", "Matrix", 0).solo.unwrap().mae, 0.0);
}

#[test]
fn a_reference_that_isnt_there_or_isnt_a_sequence_is_an_error_not_a_panic() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("nothing.fseq");
    let request = |fseq: &Path| {
        compare(
            Request {
                show: &show(),
                xsq: &xsq(),
                fseq,
            },
            Options::default(),
        )
    };
    let err = request(&missing).unwrap_err();
    assert!(matches!(err, FidelityError::Reference { .. }), "{err}");
    assert!(err.to_string().contains("nothing.fseq"), "{err}");

    let junk = dir.path().join("junk.fseq");
    std::fs::write(&junk, b"not a sequence").unwrap();
    assert!(matches!(
        request(&junk).unwrap_err(),
        FidelityError::Reference { .. }
    ));

    let fseq = Reference::default().write(&dir);
    let err = compare(
        Request {
            show: dir.path(),
            xsq: &xsq(),
            fseq: &fseq,
        },
        Options::default(),
    )
    .unwrap_err();
    assert!(matches!(err, FidelityError::NotAShowFolder(_)), "{err}");
}

/// An uncompressed version 2 file holding only `range` (first channel, count) of the reference.
fn sparse(reference: Reference, range: (usize, usize), dir: &TempDir) -> PathBuf {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PSEQ");
    bytes.extend_from_slice(&40u16.to_le_bytes()); // where the frames start
    bytes.extend_from_slice(&[0, 2]); // version 2.0
    bytes.extend_from_slice(&40u16.to_le_bytes()); // no variable headers
    bytes.extend_from_slice(&(range.1 as u32).to_le_bytes()); // channels stored per frame
    bytes.extend_from_slice(&reference.frames.to_le_bytes());
    bytes.push(reference.step_ms as u8);
    bytes.extend_from_slice(&[0, 0, 0, 1, 0]); // flags, uncompressed, no blocks, one range
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&(range.0 as u32).to_le_bytes()[..3]);
    bytes.extend_from_slice(&(range.1 as u32).to_le_bytes()[..3]);
    bytes.extend_from_slice(&[0; 2]);
    for frame in 0..i64::from(reference.frames) {
        let all = reference.at(frame * i64::from(reference.step_ms));
        bytes.extend_from_slice(&all[range.0..range.0 + range.1]);
    }
    let path = dir.path().join("sparse.fseq");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_sparse_reference_is_measured_only_where_it_holds_channels() {
    let dir = TempDir::new().unwrap();
    // Matrix's channels only (31–60).
    let fseq = sparse(Reference::default(), (30, 30), &dir);
    let card = compare(
        Request {
            show: &show(),
            xsq: &xsq(),
            fseq: &fseq,
        },
        Options::default(),
    )
    .unwrap();
    assert!(card.reference.sparse);
    assert_eq!(card.coverage.unmeasured_props, ["Strip", "Porch", "Far"]);
    assert_eq!(card.coverage.pixels_measured, 10);
    assert_eq!(effect(&card, "On", "Matrix", 0).solo.unwrap().mae, 0.0);
    assert!(effect(&card, "On", "Strip", 0).layered.is_none());
}

#[test]
fn a_prop_the_reference_leaves_dark_for_long_is_left_out_as_unrendered() {
    // xLights saved the file before it had rendered Matrix's blue: black for two seconds.
    let unrendered = Reference {
        matrix_blue: 0,
        ..Reference::default()
    };
    let with_run = |dark_run_ms| {
        score_with(
            unrendered,
            Options {
                limits: Limits {
                    dark_run_ms,
                    ..Limits::default()
                },
                ..Options::default()
            },
        )
    };
    // Scored as it stands, the blue On is the worst effect there is, and its label is blamed.
    let card = with_run(0);
    assert_eq!(effect(&card, "On", "Matrix", 0).solo.unwrap().mae, 85.0);
    assert_eq!(card.diagnostics.label_audit.exact_but_differs.len(), 1);
    assert!(card.reference.left_out.is_empty());
    // Two seconds isn't long enough to call unrendered by default...
    assert!(score(unrendered).reference.left_out.is_empty());
    // ...but with the line at one second it is: Matrix is left out for those two seconds.
    let card = with_run(1000);
    let gaps: Vec<(&str, u64, u64)> = card
        .reference
        .left_out
        .iter()
        .map(|g| (g.prop.as_str(), g.start_ms, g.end_ms))
        .collect();
    // (Strip is dark too between twinkles, but never for a second.)
    assert_eq!(gaps, [("Matrix", 0, 3000)]);
    assert!(effect(&card, "On", "Matrix", 0).layered.is_none());
    assert!(card.diagnostics.label_audit.exact_but_differs.is_empty());
    assert!(
        card.notes.iter().any(|n| n.contains("unrendered")),
        "{:#?}",
        card.notes
    );
    assert!(card.reference.left_out_share > 0.0);
    // Matrix still counts where the reference has it: under the group's white.
    assert_eq!(
        effect(&card, "On", "All", 3000).layered.unwrap().pixel_seconds,
        30.0
    );
}

#[test]
fn props_a_render_was_cut_short_on_are_left_out_from_where_the_first_went_dark() {
    // xLights saved the file while it was still rendering. The group's white, the last thing in
    // the sequence, never got drawn, and each prop's own row only so far: Matrix and Porch are
    // dark from where their rows end, and Strip twinkles on, as its own row has it, for half a
    // second into the white before it goes dark as well.
    let unfinished = Reference {
        cut_short: true,
        ..Reference::default()
    };
    let with_run = |dark_run_ms| {
        score_with(
            unfinished,
            Options {
                limits: Limits {
                    dark_run_ms,
                    ..Limits::default()
                },
                ..Options::default()
            },
        )
    };
    // Scored as it stands, every prop is held against a white the file was never given.
    let card = with_run(0);
    assert!(card.reference.left_out.is_empty());
    let white = effect(&card, "On", "All", 3000);
    assert_eq!(white.layered.unwrap().pixel_seconds, 30.0);
    assert_ne!(white.look.as_ref().unwrap().verdict, Verdict::Matches);
    // All three are dark to the end of the file while PixelFlow lights them, Matrix and Porch
    // from the third second. The render wasn't finished on any of them from there, so Strip's
    // half second of twinkles under the white, lit though it is, is left out with the rest.
    let card = with_run(400);
    let gaps: Vec<(&str, u64, u64)> = card
        .reference
        .left_out
        .iter()
        .map(|g| (g.prop.as_str(), g.start_ms, g.end_ms))
        .collect();
    assert_eq!(
        gaps,
        [
            ("Strip", 3000, 4000),
            ("Matrix", 2000, 4000),
            ("Porch", 2000, 4000)
        ]
    );
    assert!(effect(&card, "On", "All", 3000).layered.is_none());
    // Strip's twinkles are judged on the second they had to themselves.
    let twinkle = effect(&card, "Twinkle", "Strip", 2000);
    assert_eq!(twinkle.layered.unwrap().pixel_seconds, 10.0);
    assert!(
        card.notes.iter().any(|n| n.contains("cut short")),
        "{:#?}",
        card.notes
    );
    // A prop that is dark for a while and lit again wasn't cut short: where the reference left
    // Matrix's blue unrendered, the group's white is there at the end, and nothing more is left
    // out than the dark stretch itself.
    let card = score_with(
        Reference {
            matrix_blue: 0,
            ..Reference::default()
        },
        Options {
            limits: Limits {
                dark_run_ms: 1000,
                ..Limits::default()
            },
            ..Options::default()
        },
    );
    assert_eq!(card.reference.left_out.len(), 1);
    assert!(!card.notes.iter().any(|n| n.contains("cut short")));
}

#[test]
fn a_manifest_runs_every_sequence_and_keeps_going_past_one_it_cant_compare() {
    let dir = TempDir::new().unwrap();
    let good = Reference {
        matrix_blue: 200,
        ..Reference::default()
    }
    .write(&dir);
    let manifest = dir.path().join("corpus.json");
    let text = serde_json::json!({
        "sequences": [
            { "label": "sequence A", "show": show(), "xsq": xsq(), "fseq": "known.fseq" },
            { "label": "sequence B", "show": show(), "xsq": xsq(), "fseq": "gone.fseq" },
            { "show": show(), "xsq": xsq(), "fseq": good, "frameOffset": 0 },
        ]
    });
    std::fs::write(&manifest, text.to_string()).unwrap();
    let manifest = Manifest::read(&manifest).unwrap();
    // A path in the manifest starts from the manifest's folder.
    assert_eq!(manifest.sequences[0].fseq, good);

    let mut heard = Vec::new();
    let run = corpus::run(&manifest, &by_moment(), |label, result| {
        heard.push((label.to_string(), result.is_ok()));
    });
    assert_eq!(
        heard,
        [
            ("sequence A".to_string(), true),
            ("sequence B".to_string(), false),
            ("known".to_string(), true)
        ]
    );
    let card = &run.card;
    assert_eq!(card.sequences.len(), 2);
    // Four Ons and the twinkles are labelled exact; the blue On differs, and so do the made-up
    // twinkles' distribution from PixelFlow's.
    assert_eq!(
        (card.sequences[0].exact_judged, card.sequences[0].exact_differ),
        (5, 2)
    );
    assert_eq!(card.failures.len(), 1);
    assert!(
        card.failures[0].error.contains("gone.fseq"),
        "{:?}",
        card.failures
    );
    // Both sequences' effects, kind by kind, and both sequences' audits, each entry named.
    let on = card.kinds.iter().find(|k| k.kind == "On").unwrap();
    assert_eq!((on.instances, on.labels.exact), (10, 8));
    assert_eq!(card.diagnostics.ranking[0].kind, "On");
    assert!((card.diagnostics.ranking[0].impact - 2.0 * 18.3333 * 20.0).abs() < 0.01);
    let named: Vec<Option<&str>> = card
        .diagnostics
        .label_audit
        .exact_but_differs
        .iter()
        .map(|e| e.sequence.as_deref())
        .collect();
    assert_eq!(named, [Some("sequence A"), Some("known")]);

    let out = dir.path().join("reports");
    let written = corpus::write_run(&out, &run).unwrap();
    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "01-sequence-a.json",
            "01-sequence-a.md",
            "02-known.json",
            "02-known.md",
            "corpus.json",
            "corpus.md"
        ]
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("corpus.json")).unwrap()).unwrap();
    assert_eq!(json["sequences"][0]["label"], "sequence A");
    // Both sequences' looks together, and each one's own.
    assert_eq!((card.look.judged, card.look.of), (12, 12));
    assert_eq!(card.look.effects.matches, 10);
    assert_eq!(card.sequences[1].look.effects.matches, 5);
    assert_eq!(json["look"]["time"]["matches"], 0.7778);
    assert_eq!(json["improve"][0]["kind"], "Twinkle");
    // The importer's table held against it: plain Ons match, as the table says; the made-up
    // twinkles don't, twice over, which is enough to say the table is wrong about them.
    let checked =
        labels::check(&labels::read(&std::fs::read_to_string(out.join("corpus.json")).unwrap()).unwrap());
    let said = |kind: &str, noted: bool| {
        let row = checked
            .iter()
            .find(|c| c.row.kind == kind && c.row.notes.is_empty() != noted)
            .unwrap();
        (row.table, row.earned, row.disagrees())
    };
    assert_eq!(
        said("On", false),
        (Verdict::Matches, Some(Verdict::Matches), false)
    );
    assert_eq!(said("On", true), (Verdict::Close, Some(Verdict::Matches), true));
    let (table, earned, disagrees) = said("Twinkle", false);
    assert!(table == Verdict::Matches && earned != Some(Verdict::Matches) && disagrees);
    assert_eq!(
        json["diagnostics"]["labelAudit"]["exactButDiffers"][0]["kind"],
        "On"
    );
    let summary = corpus_markdown(card);
    assert!(
        summary.contains("| sequence A |") && summary.contains("- sequence B: "),
        "{summary}"
    );

    // A manifest that isn't one says so.
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "{ \"sequences\": [ { \"show\": 1 } ] }").unwrap();
    assert!(matches!(
        Manifest::read(&bad).unwrap_err(),
        FidelityError::Manifest { .. }
    ));
}

#[test]
fn a_scorecard_is_json_with_every_effect_and_its_importers_label() {
    let card = score(Reference::default());
    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(json["schema"], 4);
    assert_eq!(json["sequence"]["import"]["effects"], 6);
    assert_eq!(json["coverage"]["pixelsMeasured"], 35);
    let effects = json["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 6);
    let green = &effects[1];
    assert_eq!(
        (&green["kind"], &green["target"], &green["label"], &green["class"]),
        (
            &"On".into(),
            &"Strip".into(),
            &"approximate".into(),
            &"deterministic".into()
        )
    );
    assert_eq!((&green["startMs"], &green["endMs"]), (&1000.into(), &2000.into()));
    assert!(green["notes"][0].as_str().unwrap().contains("part-way"));
    // How the importer expected it to look, and how it was judged to.
    assert_eq!(
        (&green["expected"], &green["look"]["verdict"]),
        (&"close".into(), &"matches".into())
    );
    assert_eq!(green["look"]["basis"], "solo");
    assert_eq!(green["lookSolo"]["components"]["color"], 1.0);
    assert_eq!(json["look"]["effects"]["matches"], 5);
    assert_eq!(
        (&json["method"]["matches"], &json["method"]["close"]),
        (&85.0.into(), &55.0.into())
    );
    assert_eq!(green["solo"]["mae"], 0.0);
    assert_eq!(green["solo"]["pixelSeconds"], 10.0);
    assert!(json["method"]["limits"]["tolerance"].is_number());
}

#[test]
fn frames_can_be_drawn_side_by_side() {
    let dir = TempDir::new().unwrap();
    let fseq = Reference {
        matrix_blue: 200,
        ..Reference::default()
    }
    .write(&dir);
    let request = Request {
        show: &show(),
        xsq: &xsq(),
        fseq: &fseq,
    };
    let mut prepared = pf_fidelity::Prepared::open(request, Options::default()).unwrap();
    let matrix = strips::find_target(prepared.show(), "Matrix").unwrap();
    assert!(strips::find_target(prepared.show(), "All").is_some());
    assert!(strips::find_target(prepared.show(), "Nothing").is_none());
    // The blue On is effect 3 (after Strip's three): two frames of it, a pixel per light.
    let frames = strips::effect_frames(&prepared, 3, 2).unwrap();
    assert_eq!(frames, [0, 39]);
    let image = strips::draw(&mut prepared, matrix, &frames, 1).unwrap();
    // Two frames of a 2×5 matrix, xLights' above PixelFlow's, with gaps between.
    assert_eq!(
        (image.width(), image.height()),
        (4 + 2 * (2 + 4), 4 + 2 * (5 + 4))
    );
    assert_eq!(image.get_pixel(4, 4).0, [0, 0, 200], "xLights' blue");
    assert_eq!(image.get_pixel(4, 13).0, [0, 0, 255], "PixelFlow's");
    let levels = strips::levels(&mut prepared, matrix, &frames).unwrap();
    assert_eq!(
        (levels[0].pixelflow, levels[0].xlights),
        ([0.0, 0.0, 255.0], [0.0, 0.0, 200.0])
    );
    assert_eq!(levels[0].error, 55.0 / 3.0);
}

#[test]
fn the_headline_is_how_much_of_the_sequence_looks_the_same() {
    let card = score(Reference::default());
    let look = &card.look;
    assert_eq!((look.judged, look.of, look.not_judged), (6, 6, 0.0));
    // The five Ons are as xLights drew them: seven of the nine seconds of effect.
    for on in card.effects.iter().filter(|e| e.kind == "On") {
        let judged = on.look.as_ref().unwrap();
        assert_eq!(
            (judged.verdict, judged.basis, judged.differs.as_deref()),
            (Verdict::Matches, Basis::Solo, None),
            "On on {} at {}",
            on.target,
            on.start_ms
        );
        assert!(judged.score > 99.0, "{judged:?}");
    }
    assert_eq!((look.effects.matches, look.effects.no_equivalent), (5, 0));
    assert_eq!(look.time.matches, 0.7778);
    assert!(
        headline(look).starts_with("78% comes across the same · "),
        "{}",
        headline(look)
    );
    // The made-up twinkles slide along the strip where PixelFlow's come and go at random: that
    // isn't the same look, and the scorecard says what a viewer would notice.
    let twinkle = effect(&card, "Twinkle", "Strip", 2000).look.as_ref().unwrap();
    assert_ne!(twinkle.verdict, Verdict::Matches, "{twinkle:?}");
    assert!(twinkle.score < 70.0 && twinkle.differs.is_some(), "{twinkle:?}");
    assert!(look.score.is_some_and(|s| s > 70.0 && s < 95.0), "{look:?}");

    // What to improve first: the twinkles are all there is to gain.
    let kinds: Vec<&str> = card.improve.iter().map(|i| i.kind.as_str()).collect();
    assert_eq!(kinds, ["Twinkle", "On"]);
    assert!(card.improve[0].share > 0.99 && card.improve[0].judged_alone == 1);
    assert_eq!(card.improve[1].verdicts.matches, 5);

    // The picture as a whole, prop by prop: Matrix and Porch never twinkle, and Far is never lit.
    for prop in &card.props[1..3] {
        let seen = prop.look.unwrap();
        assert_eq!(seen.verdict, Verdict::Matches, "{}", prop.name);
        assert!(seen.score > 99.0, "{}: {seen:?}", prop.name);
    }
    assert!(card.props[3].look.is_none());
    assert!(look.as_seen.is_some_and(|s| s > 50.0 && s < 100.0), "{look:?}");

    // The summary leads with the look and ends with the pixels.
    let summary = sequence_markdown(&card);
    let at = |heading: &str| summary.find(heading).unwrap_or_else(|| panic!("no {heading}"));
    assert!(at("## How it looks") < at("## What to improve first"));
    assert!(at("## What to improve first") < at("## Diagnostics: pixel for pixel"));
    assert!(summary.contains("**78% comes across the same · "), "{summary}");
}

#[test]
fn a_dimmer_render_is_judged_by_how_it_looks_not_by_its_pixels() {
    // xLights drew Matrix's blue at 200 where PixelFlow draws 255: a fifth dimmer. Pixel for
    // pixel that is a difference; nobody watching would see one.
    let card = score(Reference {
        matrix_blue: 200,
        ..Reference::default()
    });
    let blue = effect(&card, "On", "Matrix", 0);
    assert_eq!(blue.look.as_ref().unwrap().verdict, Verdict::Matches);
    assert_eq!(card.diagnostics.label_audit.exact_but_differs.len(), 1);
    assert!(card.labels.optimistic.iter().all(|m| m.kind != "On"));

    // At 100 it is well under half as bright, which shows: close, and the brightness is why.
    let card = score(Reference {
        matrix_blue: 100,
        ..Reference::default()
    });
    let blue = effect(&card, "On", "Matrix", 0);
    let judged = blue.look.as_ref().unwrap();
    assert_eq!(
        (judged.verdict, judged.weakest, judged.basis),
        (Verdict::Close, Component::Brightness, Basis::Solo)
    );
    assert_eq!(judged.differs.as_deref(), Some("brighter (2.5× as bright)"));
    let seen = blue.look_solo.unwrap().seen;
    assert_eq!((seen.level.pixelflow, seen.level.xlights), (1.0, 0.3922));
    // Nothing else about it differs.
    let parts = blue.look_solo.unwrap().components;
    assert_eq!(
        (parts.coverage, parts.color, parts.pattern, parts.motion),
        (1.0, 1.0, 1.0, 1.0)
    );
    assert_eq!(card.look.effects.close, 1);

    let on = card.improve.iter().find(|i| i.kind == "On").unwrap();
    assert_eq!(
        (on.weakest, on.differs.as_deref()),
        (Component::Brightness, Some("brighter (2.5× as bright)"))
    );
    assert_eq!((on.verdicts.matches, on.verdicts.close), (4, 1));

    // The importer expected it to match: it is listed, judged alone.
    let labels = &card.labels;
    let optimistic: Vec<(&str, &str, Verdict, Verdict)> = labels
        .optimistic
        .iter()
        .filter(|m| m.kind == "On")
        .map(|m| (m.kind.as_str(), m.target.as_str(), m.expected, m.measured))
        .collect();
    assert_eq!(optimistic, [("On", "Matrix", Verdict::Matches, Verdict::Close)]);
    assert!(labels.optimistic_layered.is_empty());
    // And the green On, which it expected to look close for its layer mix amount, matches.
    let pessimistic: Vec<(&str, Verdict, Verdict)> = labels
        .pessimistic
        .iter()
        .map(|m| (m.target.as_str(), m.expected, m.measured))
        .collect();
    assert_eq!(pessimistic, [("Strip", Verdict::Close, Verdict::Matches)]);
    let plain = labels
        .rows
        .iter()
        .find(|r| r.kind == "On" && r.notes.is_empty())
        .unwrap();
    assert_eq!(
        (
            plain.expected,
            plain.effects,
            plain.measured.matches,
            plain.measured.close
        ),
        (Verdict::Matches, 4, 3, 1)
    );
    assert_eq!((plain.judged(), plain.earned()), (4, Some(Verdict::Matches)));
    assert_eq!((labels.judged, labels.agree), (6, 3));

    let summary = sequence_markdown(&card);
    assert!(
        summary.contains("| On | Matrix | 0:00.0–0:02.0 | Matches | Close | 71 | brightness | brighter (2.5× as bright) | – |"),
        "{summary}"
    );
}

/// Two seconds on the tests' show with one effect PixelFlow has no effect for: a Shader on Far,
/// in red.
const STAND_IN: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<xsequence BaseChannel="0" ChanCtrlBasic="0" ChanCtrlColor="0" FixedPointTiming="1" ModelBlending="true">
  <head>
    <version>2024.19</version>
    <song>Stand-in</song>
    <sequenceTiming>50 ms</sequenceTiming>
    <sequenceType>Animation</sequenceType>
    <sequenceDuration>2.000</sequenceDuration>
  </head>
  <ColorPalettes>
    <ColorPalette>C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1</ColorPalette>
  </ColorPalettes>
  <EffectDB>
    <Effect></Effect>
  </EffectDB>
  <DisplayElements>
    <Element collapsed="0" type="model" name="Far" visible="1"/>
  </DisplayElements>
  <ElementEffects>
    <Element type="model" name="Far">
      <EffectLayer>
        <Effect ref="0" name="Shader" palette="0" startTime="0" endTime="2000"/>
      </EffectLayer>
    </Element>
  </ElementEffects>
</xsequence>
"##;

#[test]
fn an_effect_pixelflow_has_no_effect_for_has_no_equivalent_whatever_it_scores() {
    // A Shader on Far, which PixelFlow shows as a dim fill in its first color; xLights drew red.
    let dir = TempDir::new().unwrap();
    let fseq = Reference {
        far_red: true,
        ..Reference::default()
    }
    .write(&dir);
    let sequence = dir.path().join("stand-in.xsq");
    std::fs::write(&sequence, STAND_IN).unwrap();
    let request = Request {
        show: &show(),
        xsq: &sequence,
        fseq: &fseq,
    };
    let card = compare(request, Options::default()).unwrap();
    let shader = effect(&card, "Shader", "Far", 0);
    assert_eq!(
        (shader.label, shader.expected),
        (Label::Placeholder, Verdict::NoEquivalent)
    );
    let judged = shader.look.as_ref().unwrap();
    assert_eq!(judged.verdict, Verdict::NoEquivalent);
    // It is the right color on the right pixels, a quarter as bright.
    assert_eq!(judged.weakest, Component::Brightness);
    assert!(judged.score > 50.0, "{judged:?}");
    assert_eq!(card.look.effects.no_equivalent, 1);
    assert_eq!((card.look.time.no_equivalent, card.look.time.matches), (1.0, 0.0));
    assert_eq!(
        headline(&card.look),
        "0% comes across the same · 0% looks close · 0% looks different · 100% has no PixelFlow effect yet"
    );
    assert_eq!((card.labels.judged, card.labels.agree), (1, 1));
}

/// The show's sequence of chance (`chance.xsq`) held against PixelFlow's own render of it from
/// another import, and so from other seeds: the fireworks go off at other moments in the two.
fn score_against_itself(options: Options) -> Scorecard {
    let dir = TempDir::new().unwrap();
    let sequence = show().join("chance.xsq");
    let comparison = pf_fidelity::layout::comparison_show(&show()).unwrap();
    let import = pf_xlights::import_sequence_file(&sequence, &comparison.show, |_, _| None).unwrap();
    let fseq = dir.path().join("chance.fseq");
    pf_render::export::export_fseq_file(
        &comparison.show,
        &comparison.map,
        &import.sequence,
        &pf_render::AudioSource::none(),
        &pf_render::Pictures::new(|file| std::fs::read(file).map_err(|e| e.to_string())),
        &fseq,
        |_, _| true,
    )
    .unwrap();
    let request = Request {
        show: &show(),
        xsq: &sequence,
        fseq: &fseq,
    };
    compare(request, options).unwrap()
}

#[test]
fn an_effect_that_draws_on_chance_is_held_against_pixelflows_other_seeds() {
    let card = score_against_itself(Options::default());
    assert_eq!(card.method.other_seeds, OTHER_SEEDS as usize);
    assert!(!card.method.by_moment);

    // The fireworks' moments are chance: judged over the whole effect, next to the other seeds,
    // with what they score moment by moment beside.
    let fireworks = effect(&card, "Fireworks", "All", 0);
    assert_eq!(fireworks.chance, Chance::Moments);
    assert!(
        fireworks
            .chance_reason
            .as_deref()
            .is_some_and(|why| why.contains("chance moments"))
    );
    let (_, look) = fireworks.look_measured(&card.method.audit).unwrap();
    assert_eq!(look.chance, Chance::Moments);
    assert!(look.itself.is_some(), "{look:?}");
    // Four seconds of bursts say enough or too little, as the seeds have it: the effect is
    // judged on that look, or it is not judged and the look is kept beside it.
    match (&fireworks.look, &fireworks.says_little) {
        (Some(judged), None) => {
            assert_eq!(judged.chance, Chance::Moments);
            assert!(judged.by_moment.is_some());
        }
        (None, Some(said)) => {
            assert_eq!((said.chance, said.score), (Chance::Moments, look.score));
            assert!(said.by_moment.is_some() && !said.why.is_empty(), "{said:?}");
            assert!(fireworks.look_basis(&card.method.audit).is_none());
        }
        both => panic!("judged, or not for a look that says too little: {both:?}"),
    }
    let (_, by_moment) = fireworks.look_basis_by_moment(&card.method.audit).unwrap();
    assert_eq!((by_moment.chance, by_moment.itself), (Chance::Nothing, None));
    assert_eq!(by_moment.pixel_seconds, look.pixel_seconds);

    // Strip's red draws on nothing, but it is only ever seen under the fireworks: the picture
    // it is judged on changes from seed to seed, and is held against the other seeds too.
    let strip = effect(&card, "On", "Strip", 0);
    assert_eq!(
        (strip.chance, strip.chance_reason.as_ref()),
        (Chance::Nothing, None)
    );
    let (basis, look) = strip.look_measured(&card.method.audit).unwrap();
    assert_eq!((basis, look.chance), (Basis::Layered, Chance::Drawn));
    assert!(strip.look_layered_by_moment.is_some());
    assert!(strip.look_solo.is_none() && strip.look_solo_by_moment.is_none());
    assert_ne!(strip.look.is_some(), strip.says_little.is_some());

    // Far's blue has its pixels to itself, and every seed draws it the same: judged as ever.
    let far = effect(&card, "On", "Far", 0);
    let judged = far.look.as_ref().unwrap();
    assert_eq!(
        (
            judged.basis,
            judged.chance,
            judged.by_moment,
            judged.verdict,
            judged.score
        ),
        (Basis::Solo, Chance::Nothing, None, Verdict::Matches, 100.0)
    );
    assert!(far.look_solo_by_moment.is_none() && far.look_layered_by_moment.is_none());

    // Both ways of judging are in the scorecard, over the same effects: moment by moment
    // every one is judged, and next to other seeds all but those whose looks say too little.
    let little = card.effects.iter().filter(|e| e.says_little.is_some()).count();
    assert_eq!(
        (card.look.judged + little, card.look.of),
        (card.look_by_moment.judged, card.look_by_moment.of)
    );
    assert_eq!(
        (
            card.look.says_little.effects,
            card.look_by_moment.says_little.effects
        ),
        (little, 0)
    );
    assert_eq!(card.look.says_little.share > 0.0, little > 0);
    assert!(card.look.not_judged >= card.look.says_little.share);
    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(json["effects"][fireworks.index]["chance"], "moments");
    let held = if fireworks.look.is_some() {
        "look"
    } else {
        "saysLittle"
    };
    assert_eq!(json["effects"][fireworks.index][held]["chance"], "moments");
    assert!(json["effects"][fireworks.index][held]["byMoment"]["score"].is_number());
    assert_eq!(json["look"]["saysLittle"]["effects"], little);
    assert!(json["lookByMoment"]["score"].is_number());
    assert_eq!(json["effects"][far.index]["look"]["chance"], "nothing");
    assert!(json["effects"][far.index]["look"].get("byMoment").is_none());
    let markdown = sequence_markdown(&card);
    assert!(
        markdown.contains("## Effects held against other seeds"),
        "{markdown}"
    );
    assert!(
        markdown.contains("## Not judged: looks that say little"),
        "{markdown}"
    );
    // What isn't judged for a look that says too little is said in the summary and listed.
    assert_eq!(
        markdown.contains("has a look that says too little to judge it by"),
        little > 0,
        "{markdown}"
    );
    for effect in card.effects.iter().filter(|e| e.says_little.is_some()) {
        let row = format!("| {} | {} | {} |", effect.index, effect.kind, effect.target);
        assert!(markdown.contains(&row), "{row}: {markdown}");
    }
}

#[test]
fn a_few_twinkling_pixels_are_found_different_or_say_too_little() {
    // The made-up twinkles slide along ten pixels for two seconds, where PixelFlow's come and go
    // at random, and at other moments from each seed. Held against its other seeds they are
    // found different on the parts its own renders agree on, or, where those are too few, the
    // look says too little and the effect is not judged. It never matches.
    let card = score_with(Reference::default(), Options::default());
    assert_eq!(card.method.other_seeds, OTHER_SEEDS as usize);
    let twinkle = effect(&card, "Twinkle", "Strip", 2000);
    assert_eq!(twinkle.chance, Chance::Drawn);
    let look = &card.look;
    match (&twinkle.look, &twinkle.says_little) {
        (Some(judged), None) => {
            assert_ne!(judged.verdict, Verdict::Matches, "{judged:?}");
            assert_eq!(
                (look.judged, look.says_little.effects, look.not_judged),
                (6, 0, 0.0)
            );
            assert_eq!(look.time.matches, 0.7778);
        }
        (None, Some(said)) => {
            assert!(said.itself < LITTLE && said.why.contains("in common"), "{said:?}");
            // Two of the nine seconds of effect are in no share: the Ons are all that is judged.
            assert_eq!((look.judged, look.of), (5, 6));
            assert_eq!((look.says_little.effects, look.says_little.seconds), (1, 2.0));
            assert_eq!((look.says_little.share, look.not_judged), (0.2222, 0.2222));
            assert_eq!((look.time.matches, look.effects.matches), (1.0, 5));
            let labels = card.labels.rows.iter().find(|r| r.kind == "Twinkle").unwrap();
            assert_eq!((labels.judged(), labels.says_little), (0, 1));
            let kind = card.kinds.iter().find(|k| k.kind == "Twinkle").unwrap();
            assert!(kind.look.is_none() && kind.says_little == 1);
            let summary = sequence_markdown(&card);
            assert!(
                summary.contains(
                    "22% of the effect-time (1 effect, 2 s) was measured and has a look that says too little"
                ),
                "{summary}"
            );
            assert!(
                summary.contains("| Twinkle | Strip | 0:02.0–0:04.0 |"),
                "{summary}"
            );
        }
        both => panic!("judged, or not for a look that says too little: {both:?}"),
    }
    // Moment by moment it is judged either way, as it was before.
    assert_eq!(card.look_by_moment.judged, 6);
}

#[test]
fn every_effect_can_be_judged_moment_by_moment_as_before() {
    let options = Options {
        by_moment: true,
        ..Options::default()
    };
    let card = score_against_itself(options);
    assert!(card.method.by_moment);
    assert_eq!(card.method.other_seeds, 0);
    for effect in &card.effects {
        let judged = effect.look.as_ref().unwrap();
        assert_eq!(
            (judged.chance, judged.by_moment),
            (Chance::Nothing, None),
            "{effect:?}"
        );
        assert!(effect.look_solo_by_moment.is_none() && effect.look_layered_by_moment.is_none());
    }
    // What chance decides in each is still said.
    assert_eq!(effect(&card, "Fireworks", "All", 0).chance, Chance::Moments);
    // The one summary is the other.
    assert_eq!(card.look.score, card.look_by_moment.score);
    assert_eq!(card.look.time, card.look_by_moment.time);
    assert!(sequence_markdown(&card).contains("`--by-moment`"));
}
