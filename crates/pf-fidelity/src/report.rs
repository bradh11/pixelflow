//! The scorecards as markdown, for reading.

use crate::metrics::Scores;
use crate::scorecard::{
    AuditEntry, Basis, Class, CorpusCard, EffectScore, ExactKind, KindScore, LabelAudit, Music, Ranked,
    Scorecard, Source,
};
use std::fmt::Write as _;

/// Most effects listed in a table before it's cut short.
const MOST_ROWS: usize = 40;

fn pct(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

fn num(value: f64) -> String {
    format!("{value:.1}")
}

fn maybe(value: Option<f64>, show: impl Fn(f64) -> String) -> String {
    value.map_or_else(|| "–".to_string(), show)
}

fn two(value: f64) -> String {
    format!("{value:.2}")
}

/// A time as m:ss.s.
fn time(ms: u64) -> String {
    format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

/// A cell's text, safe inside a table.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn class(class: Class) -> &'static str {
    match class {
        Class::Deterministic => "deterministic",
        Class::Stochastic => "stochastic",
        Class::NoMusic => "follows music (not read)",
    }
}

fn headline(out: &mut String, scores: &Scores, tolerance: u8) {
    let _ = writeln!(out, "| Measure | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| Mean error per channel (0–255) | {} |", num(scores.mae));
    let _ = writeln!(out, "| RMS error per channel | {} |", num(scores.rmse));
    let _ = writeln!(
        out,
        "| Pixels within ±{tolerance} on every channel | {} |",
        pct(scores.within_tolerance)
    );
    let _ = writeln!(out, "| Lit agreement (IoU) | {} |", two(scores.lit_iou));
    let _ = writeln!(
        out,
        "| Brightness correlation over time | {} |",
        maybe(scores.brightness_correlation, two)
    );
    let _ = writeln!(
        out,
        "| Hue difference where both are lit | {} |",
        maybe(scores.hue_difference, |h| format!("{h:.0}°"))
    );
    let _ = writeln!(
        out,
        "| Mean brightness, PixelFlow / xLights | {} / {} |",
        num(scores.level.pixelflow),
        num(scores.level.xlights)
    );
    let _ = writeln!(
        out,
        "| Pixels lit, PixelFlow / xLights | {} / {} |",
        pct(scores.lit.pixelflow),
        pct(scores.lit.xlights)
    );
    let _ = writeln!(out, "| Pixel-seconds measured | {:.0} |", scores.pixel_seconds);
}

fn ranking(out: &mut String, ranked: &[Ranked], error: &str) {
    if ranked.is_empty() {
        let _ = writeln!(out, "None measured.");
        return;
    }
    let _ = writeln!(
        out,
        "| # | Effect | Impact | Share | {error} | Pixel-seconds | Instances | Measured alone |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    for (i, r) in ranked.iter().enumerate() {
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} | {} | {} | {:.0} | {} | {} |",
            i + 1,
            cell(&r.kind),
            r.impact,
            pct(r.share),
            if error.starts_with("Distribution") {
                two(r.error)
            } else {
                num(r.error)
            },
            r.pixel_seconds,
            r.instances,
            pct(r.solo_share)
        );
    }
}

fn kinds(out: &mut String, kinds: &[KindScore]) {
    let _ = writeln!(
        out,
        "| Effect | Compared | Instances (exact / approx. / placeholder) | Measured | Alone | Error alone | Within alone | IoU alone | Error layered | Distribution |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|");
    for k in kinds {
        let solo = k.solo.as_ref();
        let _ = writeln!(
            out,
            "| {} | {} | {} ({} / {} / {}) | {} | {} | {} | {} | {} | {} | {} |",
            cell(&k.kind),
            class(k.class),
            k.instances,
            k.labels.exact,
            k.labels.approximate,
            k.labels.placeholder,
            k.measured,
            pct(k.solo_share),
            maybe(solo.map(|s| s.mae), num),
            maybe(solo.map(|s| s.within_tolerance), pct),
            maybe(solo.map(|s| s.lit_iou), two),
            maybe(k.layered.as_ref().map(|s| s.mae), num),
            maybe(
                k.solo
                    .as_ref()
                    .or(k.layered.as_ref())
                    .map(|s| s.distribution.distance),
                two
            ),
        );
    }
}

fn basis(basis: Basis) -> &'static str {
    match basis {
        Basis::Solo => "alone",
        Basis::Layered => "layered",
    }
}

fn audit_table(out: &mut String, entries: &[AuditEntry]) {
    let in_corpus = entries.iter().any(|e| e.sequence.is_some());
    let _ = writeln!(
        out,
        "| {}Effect | On | At | Error | Within | IoU | Distribution | Judged |",
        if in_corpus { "Sequence | " } else { "" }
    );
    let _ = writeln!(
        out,
        "|{}---|---|---|---|---|---|---|---|",
        if in_corpus { "---|" } else { "" }
    );
    for e in entries.iter().take(MOST_ROWS) {
        let sequence = e
            .sequence
            .as_ref()
            .map_or_else(String::new, |s| format!("{} | ", cell(s)));
        let _ = writeln!(
            out,
            "| {sequence}{} | {} | {}–{} | {} | {} | {} | {} | {} |",
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            num(e.mae),
            pct(e.within_tolerance),
            two(e.lit_iou),
            two(e.distribution),
            basis(e.basis)
        );
    }
    if entries.len() > MOST_ROWS {
        let _ = writeln!(
            out,
            "\n…and {} more (the JSON scorecard lists them all).",
            entries.len() - MOST_ROWS
        );
    }
}

fn exact_kinds(out: &mut String, kinds: &[ExactKind]) {
    let _ = writeln!(
        out,
        "| Effect | Compared | Labelled exact | Judged | Differ | Of those, judged alone |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    for k in kinds {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} |",
            cell(&k.kind),
            class(k.class),
            k.exact,
            k.judged,
            k.differ,
            k.differ_alone
        );
    }
}

fn label_audit(out: &mut String, audit: &LabelAudit) {
    let _ = writeln!(out, "### Labelled exact, measurably different\n");
    let _ = writeln!(
        out,
        "Measured pixel by pixel where nothing else was on the pixels: these labels are wrong, or \
         the effect has a bug.\n"
    );
    if audit.exact_but_differs.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.exact_but_differs);
    }
    let _ = writeln!(
        out,
        "\n### Labelled exact, different, but only measured with other effects\n"
    );
    let _ = writeln!(
        out,
        "Never on pixels of their own, so the difference may belong to what they're layered with.\n"
    );
    if audit.exact_but_differs_layered.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.exact_but_differs_layered);
    }
    let _ = writeln!(
        out,
        "\n### Labelled exact, random by nature, showing something different\n"
    );
    if audit.exact_but_distribution_differs.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.exact_but_distribution_differs);
    }
    let _ = writeln!(out, "\n### Labelled approximate, yet matching\n");
    if audit.approximate_but_matches.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.approximate_but_matches);
    }
    let _ = writeln!(out, "\n### \"Exact\" labels by effect\n");
    if audit.exact_kinds.is_empty() {
        let _ = writeln!(out, "No effect was labelled exact.");
    } else {
        exact_kinds(out, &audit.exact_kinds);
        if audit.exact_unjudged > 0 {
            let _ = writeln!(
                out,
                "\n{} labelled exact couldn't be judged (not measured, measured on too little, or \
                 following music that wasn't read).",
                audit.exact_unjudged
            );
        }
    }
}

/// The error an effect is listed by: its own frames when it has them.
fn effect_error(effect: &EffectScore) -> Option<(&Scores, &'static str)> {
    match (&effect.solo, &effect.layered) {
        (Some(solo), _) => Some((solo, "alone")),
        (None, Some(layered)) => Some((layered, "layered")),
        (None, None) => None,
    }
}

/// One sequence's scorecard.
pub fn sequence_markdown(card: &Scorecard) -> String {
    let mut out = String::new();
    let o = &mut out;
    let (seq, reference, coverage, timing) = (&card.sequence, &card.reference, &card.coverage, &card.timing);
    let _ = writeln!(o, "# Fidelity scorecard: {}\n", seq.label);
    let _ = writeln!(
        o,
        "{} long, {} frames {} ms apart, measured against a reference of {} channels and {} frames \
         {} ms apart{}.\n",
        time(seq.duration_ms),
        timing.frames,
        timing.frame_ms,
        reference.channels,
        reference.frames,
        reference.step_ms,
        reference
            .producer
            .as_ref()
            .map_or_else(String::new, |p| format!(" written by {p}"))
    );
    let _ = writeln!(o, "## Headline\n");
    match &card.overall {
        Some(scores) => headline(o, scores, card.method.limits.tolerance),
        None => {
            let _ = writeln!(o, "Nothing could be measured.");
        }
    }

    let _ = writeln!(o, "\n## Coverage\n");
    let _ = writeln!(
        o,
        "- {} of {} props and {} of {} pixels have channels on both sides: {} on controllers \
         PixelFlow imports, {} on stand-ins for channels it has no controller for.",
        coverage.props_measured,
        coverage.props,
        coverage.pixels_measured,
        coverage.pixels,
        coverage.pixels_on_controllers,
        coverage.pixels_on_stand_ins
    );
    if !coverage.unmeasured_props.is_empty() {
        let _ = writeln!(o, "- Not measured: {}.", coverage.unmeasured_props.join(", "));
    }
    let _ = writeln!(
        o,
        "- {} of {} imported effects are on measured pixels; {} of them have pixels to themselves \
         at some point.",
        coverage.effects_measured, coverage.effects, coverage.effects_measured_alone
    );
    let _ = writeln!(
        o,
        "- The importer's count: {} exact, {} approximate, {} placeholders; {} xLights effects not \
         imported.",
        seq.import.exact, seq.import.approximate, seq.import.placeholders, seq.import.skipped
    );
    let _ = writeln!(
        o,
        "- {} of {} frames compared.",
        timing.frames_compared, timing.frames
    );
    let _ = writeln!(
        o,
        "- Music: {}.",
        match seq.music {
            Music::None => "the sequence has none",
            Music::Read => "read, so effects that follow it are scored",
            Music::Missing => "not read, so effects that follow it are set apart",
        }
    );
    let aside = card.method.set_aside;
    if aside.brightness + aside.gamma + aside.reverse > 0 {
        let _ = writeln!(
            o,
            "- Controller-side settings set aside: brightness on {} wired props, gamma on {}, reverse \
             on {}.",
            aside.brightness, aside.gamma, aside.reverse
        );
    }
    for note in &card.notes {
        let _ = writeln!(o, "- {note}");
    }

    let _ = writeln!(o, "\n## Timing\n");
    let offsets: Vec<String> = timing
        .offsets
        .iter()
        .map(|e| format!("{:+}: {}", e.offset, two(e.mae)))
        .collect();
    let _ = writeln!(
        o,
        "Whole-show error with xLights' frames moved by a few frames ({}); lowest at {:+}, measured at {:+}.",
        offsets.join(", "),
        timing.best_offset,
        timing.frame_offset
    );
    if !timing.onset_deltas.is_empty() {
        let deltas: Vec<String> = timing
            .onset_deltas
            .iter()
            .map(|(delta, n)| format!("{delta:+}: {n}"))
            .collect();
        let _ = writeln!(
            o,
            "\nFrames xLights first lights an effect's pixels after PixelFlow does, for effects \
             starting on dark pixels ({}).",
            deltas.join(", ")
        );
    }

    let _ = writeln!(o, "\n## What to fix first\n");
    let _ = writeln!(
        o,
        "Deterministic effects by impact: error (each instance's own frames where it has them) \
         times its share of the pixel-seconds it was lit on.\n"
    );
    ranking(o, &card.ranking, "Error (0–255)");
    let _ = writeln!(o, "\n## Stochastic effects\n");
    let _ = writeln!(
        o,
        "These place things at random, so they are compared by what they show over each {} ms \
         (brightness, share of pixels lit, colors), not pixel by pixel.\n",
        card.method.limits.window_ms
    );
    ranking(o, &card.stochastic_ranking, "Distribution distance (0–1)");

    let _ = writeln!(o, "\n## Every effect kind\n");
    kinds(o, &card.kinds);
    if let Some(scores) = &card.unattributed {
        let _ = writeln!(
            o,
            "\nLit with no imported effect on the pixel: {:.0} pixel-seconds, error {}.",
            scores.pixel_seconds,
            num(scores.mae)
        );
    }

    let _ = writeln!(o, "\n## Label audit\n");
    label_audit(o, &card.label_audit);

    let _ = writeln!(o, "\n## Props\n");
    let _ = writeln!(
        o,
        "| Prop | Pixels | Measured | Channels | On | Error | Within | IoU | Brightness correlation | Error reversed |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|");
    for p in &card.props {
        let s = p.scores.as_ref();
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            cell(&p.name),
            p.pixels,
            p.measured_pixels,
            p.channels
                .map_or_else(|| "–".to_string(), |(a, b)| format!("{a}–{b}")),
            match p.on {
                Source::Controller => "controller",
                Source::StandIn => "stand-in",
                Source::None => "–",
            },
            maybe(s.map(|s| s.mae), num),
            maybe(s.map(|s| s.within_tolerance), pct),
            maybe(s.map(|s| s.lit_iou), two),
            maybe(s.and_then(|s| s.brightness_correlation), two),
            maybe(p.reversed_mae, num),
        );
    }

    let _ = writeln!(o, "\n## Effects, worst first\n");
    let mut worst: Vec<(&EffectScore, &Scores, &str)> = card
        .effects
        .iter()
        .filter_map(|e| effect_error(e).map(|(s, how)| (e, s, how)))
        .collect();
    worst.sort_by(|a, b| {
        (b.1.mae * b.0.share)
            .total_cmp(&(a.1.mae * a.0.share))
            .then(a.0.index.cmp(&b.0.index))
    });
    let _ = writeln!(
        o,
        "| # | Effect | On | At | Label | Compared | Error | Within | IoU | Distribution | Judged | Pixel-seconds |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (e, s, how) in worst.iter().take(MOST_ROWS) {
        let _ = writeln!(
            o,
            "| {} | {} | {} | {}–{} | {} | {} | {} | {} | {} | {} | {} | {:.0} |",
            e.index,
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            format!("{:?}", e.label).to_lowercase(),
            match &e.class_reason {
                Some(why) => format!("{} ({why})", class(e.class)),
                None => class(e.class).to_string(),
            },
            num(s.mae),
            pct(s.within_tolerance),
            two(s.lit_iou),
            two(s.distribution.distance),
            how,
            e.share
        );
    }
    if worst.len() > MOST_ROWS {
        let _ = writeln!(
            o,
            "\n…and {} more (the JSON scorecard lists them all).",
            worst.len() - MOST_ROWS
        );
    }
    out
}

/// A whole corpus's scorecard.
pub fn corpus_markdown(card: &CorpusCard) -> String {
    let mut out = String::new();
    let o = &mut out;
    let _ = writeln!(o, "# Fidelity scorecard: corpus\n");
    let _ = writeln!(
        o,
        "{} sequences compared with what xLights rendered ({}).\n",
        card.sequences.len(),
        card.tool
    );
    let _ = writeln!(o, "## Sequences\n");
    let _ = writeln!(
        o,
        "| Sequence | Length | Effects (exact / approx. / placeholder) | Measured | Pixels measured | Error | Within | IoU | Brightness correlation | Exact differing | Best offset |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|---|");
    for s in &card.sequences {
        let scores = s.overall.as_ref();
        let _ = writeln!(
            o,
            "| {} | {} | {} ({} / {} / {}) | {} | {} of {} | {} | {} | {} | {} | {} of {} | {:+} |",
            cell(&s.label),
            time(s.duration_ms),
            s.effects,
            s.import.exact,
            s.import.approximate,
            s.import.placeholders,
            s.effects_measured,
            s.pixels_measured,
            s.pixels,
            maybe(scores.map(|x| x.mae), num),
            maybe(scores.map(|x| x.within_tolerance), pct),
            maybe(scores.map(|x| x.lit_iou), two),
            maybe(scores.and_then(|x| x.brightness_correlation), two),
            s.exact_differ,
            s.exact_judged,
            s.best_offset
        );
    }
    if !card.failures.is_empty() {
        let _ = writeln!(o, "\nNot compared:\n");
        for f in &card.failures {
            let _ = writeln!(o, "- {}: {}", f.label, f.error);
        }
    }
    let _ = writeln!(o, "\n## What to fix first\n");
    let _ = writeln!(
        o,
        "Deterministic effects by impact: error (each instance's own frames where it has them) \
         times its share of the pixel-seconds it was lit on.\n"
    );
    ranking(o, &card.ranking, "Error (0–255)");
    let _ = writeln!(o, "\n## Stochastic effects\n");
    let _ = writeln!(
        o,
        "Compared by what they show over a window (brightness, share of pixels lit, colors), not \
         pixel by pixel.\n"
    );
    ranking(o, &card.stochastic_ranking, "Distribution distance (0–1)");
    let _ = writeln!(o, "\n## Every effect kind\n");
    kinds(o, &card.kinds);
    let _ = writeln!(o, "\n## Label audit\n");
    label_audit(o, &card.label_audit);
    out
}
