//! The scorecards as markdown, for reading: how the import looks first, the pixel-for-pixel
//! diagnostics last.

use crate::look::{Chance, Component, Components};
use crate::metrics::Scores;
use crate::scorecard::{
    AuditEntry, Basis, ByVerdict, Class, CorpusCard, EffectScore, ExactKind, Improve, KindScore, LabelAudit,
    LabelCheck, LabelMiss, LookSummary, Music, Ranked, SaysLittle, Scorecard, Source, Verdict,
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

fn whole(value: f64) -> String {
    format!("{value:.0}")
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

fn basis(basis: Basis) -> &'static str {
    match basis {
        Basis::Solo => "alone",
        Basis::Layered => "layered",
    }
}

/// Counts by verdict, as "matches / close / different / no equivalent".
fn counts(by: &ByVerdict<usize>) -> String {
    format!(
        "{} / {} / {} / {}",
        by.matches, by.close, by.different, by.no_equivalent
    )
}

/// The headline sentence: how much of the effect-time got each verdict.
pub fn headline(look: &LookSummary) -> String {
    if look.judged == 0 {
        return "No effect could be judged.".to_string();
    }
    let mut parts = vec![
        format!("{} comes across the same", pct(look.time.matches)),
        format!("{} looks close", pct(look.time.close)),
        format!("{} looks different", pct(look.time.different)),
    ];
    if look.time.no_equivalent > 0.0 {
        parts.push(format!(
            "{} has no PixelFlow effect yet",
            pct(look.time.no_equivalent)
        ));
    }
    parts.join(" · ")
}

/// What couldn't be judged, as a sentence after the summary's first: how much of the
/// effect-time, and how much of that was measured and has a look that says too little.
fn not_judged(look: &LookSummary) -> String {
    let little = &look.says_little;
    if look.not_judged <= 0.0 && little.effects == 0 {
        return String::new();
    }
    if little.effects == 0 {
        return format!(
            " {} of the effect-time couldn't be judged (not measured, or measured on too little).",
            pct(look.not_judged)
        );
    }
    let count = format!(
        "{} effect{}, {} s",
        little.effects,
        if little.effects == 1 { "" } else { "s" },
        whole(little.seconds)
    );
    let said = "was measured and has a look that says too little to judge it by, so it is in none of \
                these shares (see \"Not judged: looks that say little\")";
    let rest = look.not_judged - little.share;
    if rest < 0.005 {
        return format!(" {} of the effect-time ({count}) {said}.", pct(little.share));
    }
    format!(
        " {} of the effect-time couldn't be judged: {} wasn't measured, or was measured on too \
         little, and {} ({count}) {said}.",
        pct(look.not_judged),
        pct(rest),
        pct(little.share)
    )
}

/// Whether judging every effect moment by moment gives another summary.
fn another(look: &LookSummary, by_moment: &LookSummary) -> bool {
    look.score != by_moment.score || look.time != by_moment.time || look.effects != by_moment.effects
}

fn look_summary(out: &mut String, look: &LookSummary, by_moment: &LookSummary) {
    let _ = writeln!(out, "**{}**\n", headline(look));
    if look.judged == 0 {
        return;
    }
    let _ = writeln!(
        out,
        "Look score {} of 100, over {} of {} effects ({} s of effect){}.{}\n",
        maybe(look.score, whole),
        look.judged,
        look.of,
        whole(look.seconds),
        look.as_seen.map_or_else(String::new, |seen| format!(
            "; the picture as a whole, prop by prop, scores {}",
            whole(seen)
        )),
        not_judged(look)
    );
    let _ = writeln!(out, "| Verdict | Effect-time | By pixel-seconds | Effects |");
    let _ = writeln!(out, "|---|---|---|---|");
    for verdict in Verdict::ALL {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            verdict.title(),
            pct(look.time.get(verdict)),
            pct(look.pixels.get(verdict)),
            look.effects.get(verdict)
        );
    }
    if let Some(components) = &look.components {
        let _ = writeln!(out, "\n| {} |", component_names().join(" | "));
        let _ = writeln!(out, "|{}", "---|".repeat(Component::ALL.len()));
        let _ = writeln!(out, "| {} |", component_cells(components).join(" | "));
    }
    if another(look, by_moment) {
        let _ = writeln!(
            out,
            "\nEffects that draw on chance are judged next to PixelFlow's own renders from other \
             seeds. With every effect held moment by moment against xLights' render alone, as all \
             were before: {}; look score {}, over {} of {} effects.",
            headline(by_moment).to_lowercase(),
            maybe(by_moment.score, whole),
            by_moment.judged,
            by_moment.of
        );
        if let Some(components) = &by_moment.components {
            let _ = writeln!(out, "\n| {} |", component_names().join(" | "));
            let _ = writeln!(out, "|{}", "---|".repeat(Component::ALL.len()));
            let _ = writeln!(out, "| {} |", component_cells(components).join(" | "));
        }
    }
}

/// A score beside the one an effect is judged on, when it is another.
fn beside(score: f64, judged: f64) -> String {
    if (score - judged).abs() < 0.05 {
        "–".to_string()
    } else {
        whole(score)
    }
}

/// A look next to other seeds within this of what it scores by moment is taken as unmoved.
const UNMOVED: f64 = 0.5;

/// The effects judged next to PixelFlow's other seeds: how many, and those whose look that
/// changed, the longest first.
fn chance_effects(out: &mut String, card: &Scorecard) {
    let held: Vec<&EffectScore> = card
        .effects
        .iter()
        .filter(|e| e.look.as_ref().is_some_and(|l| l.chance != Chance::Nothing))
        .collect();
    let judged = card.effects.iter().filter(|e| e.look.is_some()).count();
    if held.is_empty() {
        let _ = writeln!(
            out,
            "None of the {judged} effects judged: each is held moment by moment against xLights' \
             render alone."
        );
        return;
    }
    let draws = |chance: Chance| held.iter().filter(|e| e.chance == chance).count();
    let moved = |e: &EffectScore| {
        e.look.as_ref().map_or(0.0, |l| {
            l.by_moment
                .map_or(0.0, |by_moment| (l.score - by_moment.score).abs())
        })
    };
    let mut listed: Vec<&EffectScore> = held.iter().copied().filter(|e| moved(e) > UNMOVED).collect();
    let _ = writeln!(
        out,
        "{} of the {judged} effects judged: {} whose moments are chance, {} that draw on chance \
         with their moments set, and {} that draw on none but share their pixels with one that \
         does. For {} of them the look is within {UNMOVED} of what they score by moment. The \
         rest:\n",
        held.len(),
        draws(Chance::Moments),
        draws(Chance::Drawn),
        draws(Chance::Nothing),
        held.iter().filter(|e| moved(e) <= UNMOVED).count(),
    );
    if listed.is_empty() {
        let _ = writeln!(out, "None.");
        return;
    }
    listed.sort_by(|a, b| {
        (moved(b) * b.seconds())
            .total_cmp(&(moved(a) * a.seconds()))
            .then(a.index.cmp(&b.index))
    });
    let _ = writeln!(
        out,
        "| # | Effect | On | At | Chance decides | Because | Looks | Score | By moment | Judged | Itself |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");
    for e in listed.iter().take(MOST_ROWS) {
        let (Some(judged), Some((_, look))) = (&e.look, e.look_basis(&card.method.audit)) else {
            continue;
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {}–{} | {} | {} | {} | {} | {} | {} | {} |",
            e.index,
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            e.chance.title(),
            e.chance_reason.as_deref().map_or_else(
                || "it shares its pixels with an effect that draws on chance".to_string(),
                cell
            ),
            judged.verdict.title(),
            whole(judged.score),
            scored_by_moment(judged.by_moment),
            basis(judged.basis),
            maybe(look.itself, two),
        );
    }
    if listed.len() > MOST_ROWS {
        let _ = writeln!(
            out,
            "\n…and {} more (the JSON scorecard lists them all).",
            listed.len() - MOST_ROWS
        );
    }
}

/// What an effect scores and earns moment by moment, for a table.
fn scored_by_moment(scored: Option<crate::scorecard::Scored>) -> String {
    scored.map_or_else(
        || "–".to_string(),
        |m| format!("{} ({})", whole(m.score), m.verdict.title().to_lowercase()),
    )
}

/// What the looks that say too little are, before a list of them.
fn little_intro(out: &mut String, little: f64) {
    let _ = writeln!(
        out,
        "A look next to PixelFlow's other seeds can only say what chance leaves to be said. Where \
         both renders light under one pixel in a hundred, there is too little light to tell \
         anything by. Where two of PixelFlow's own renders have less than {} of some part of the \
         look in common (\"itself\"), a match can't be told from chance, and neither can a \
         difference that rests on that part (one found on the other parts stands, and its effect \
         is judged). Such an effect is not judged, and is in none of the shares, scores, or \
         counts above. The look it got all the same is here, with what that would have earned \
         and why it says too little.\n",
        two(little)
    );
}

/// One row of a list of looks that say too little: the cells after the effect's own.
fn little_cells(look: &SaysLittle) -> String {
    format!(
        "{} | {} | {} | {} | {}{} | {}",
        whole(look.score),
        look.verdict.title().to_lowercase(),
        scored_by_moment(look.by_moment),
        basis(look.basis),
        two(look.itself),
        look.least
            .map_or_else(String::new, |part| format!(" ({})", part.name())),
        cell(&look.why)
    )
}

/// A sequence's effects that were measured and not judged, for a look that says too little: the
/// longest first.
fn said_little(out: &mut String, card: &Scorecard) {
    little_intro(out, card.method.little);
    let mut listed: Vec<(&EffectScore, &SaysLittle)> = card
        .effects
        .iter()
        .filter_map(|e| Some((e, e.says_little.as_ref()?)))
        .collect();
    if listed.is_empty() {
        let _ = writeln!(out, "None.");
        return;
    }
    listed.sort_by(|a, b| {
        b.0.seconds()
            .total_cmp(&a.0.seconds())
            .then(a.0.index.cmp(&b.0.index))
    });
    let _ = writeln!(
        out,
        "| # | Effect | On | At | Chance decides | Look it got | Would have earned | By moment | Looked at | Itself | Why it says too little |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");
    for (e, look) in listed.iter().take(MOST_ROWS) {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {}–{} | {} | {} |",
            e.index,
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            e.chance.title(),
            little_cells(look)
        );
    }
    if listed.len() > MOST_ROWS {
        let _ = writeln!(
            out,
            "\n…and {} more (the JSON scorecard lists them all).",
            listed.len() - MOST_ROWS
        );
    }
}

/// The same for a corpus.
fn corpus_said_little(out: &mut String, card: &CorpusCard) {
    little_intro(out, crate::look::LITTLE);
    if card.says_little.is_empty() {
        let _ = writeln!(out, "None.");
        return;
    }
    let _ = writeln!(
        out,
        "| Sequence | # | Effect | On | At | Look it got | Would have earned | By moment | Looked at | Itself | Why it says too little |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");
    for e in card.says_little.iter().take(MOST_ROWS) {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {}–{} | {} |",
            cell(&e.sequence),
            e.index,
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            little_cells(&e.look)
        );
    }
    if card.says_little.len() > MOST_ROWS {
        let _ = writeln!(
            out,
            "\n…and {} more (the JSON scorecard lists them all).",
            card.says_little.len() - MOST_ROWS
        );
    }
}

fn component_names() -> Vec<String> {
    Component::ALL
        .iter()
        .map(|c| {
            let name = c.name();
            name[..1].to_uppercase() + &name[1..]
        })
        .collect()
}

fn component_cells(components: &Components) -> Vec<String> {
    Component::ALL.iter().map(|&c| two(components.get(c))).collect()
}

fn improve(out: &mut String, ranked: &[Improve]) {
    if ranked.is_empty() {
        let _ = writeln!(out, "None judged.");
        return;
    }
    let _ = writeln!(
        out,
        "| # | Effect | Impact | Share | Look | Matches / close / different / no equivalent | Off | What a viewer would notice | Pixel-seconds | Judged (alone) |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|");
    for (i, r) in ranked.iter().enumerate() {
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} | {} | {} | {} | {} {} | {} | {:.0} | {} ({}) |",
            i + 1,
            cell(&r.kind),
            r.impact,
            pct(r.share),
            whole(r.score),
            counts(&r.verdicts),
            r.weakest.name(),
            two(r.component),
            r.differs.as_deref().map_or_else(|| "–".to_string(), cell),
            r.pixel_seconds,
            r.judged,
            r.judged_alone
        );
    }
}

fn kind_looks(out: &mut String, kinds: &[KindScore]) {
    let _ = writeln!(
        out,
        "| Effect | Instances | Judged (alone) | Say little | Look | By moment | {} | Matches / close / different / no equivalent |",
        component_names().join(" | ")
    );
    let _ = writeln!(
        out,
        "|---|---|---|---|---|---|{}---|",
        "---|".repeat(Component::ALL.len())
    );
    let mut kinds: Vec<&KindScore> = kinds.iter().collect();
    kinds.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.class.cmp(&b.class)));
    for k in kinds {
        let name = if k.class == Class::NoMusic {
            format!("{} ({})", cell(&k.kind), class(k.class))
        } else {
            cell(&k.kind)
        };
        match &k.look {
            Some(look) => {
                let _ = writeln!(
                    out,
                    "| {name} | {} | {} ({}) | {} | {} | {} | {} | {} |",
                    k.instances,
                    look.judged,
                    look.judged_alone,
                    k.says_little,
                    whole(look.score),
                    beside(look.by_moment, look.score),
                    component_cells(&look.components).join(" | "),
                    counts(&look.verdicts)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "| {name} | {} | 0 | {} | – | – | {} | – |",
                    k.instances,
                    k.says_little,
                    vec!["–"; Component::ALL.len()].join(" | ")
                );
            }
        }
    }
}

fn misses(out: &mut String, list: &[LabelMiss]) {
    if list.is_empty() {
        let _ = writeln!(out, "None.");
        return;
    }
    let in_corpus = list.iter().any(|m| m.sequence.is_some());
    let _ = writeln!(
        out,
        "| {}Effect | On | At | Expected | Looks | Score | Off | What a viewer would notice | The importer noted |",
        if in_corpus { "Sequence | " } else { "" }
    );
    let _ = writeln!(
        out,
        "|{}---|---|---|---|---|---|---|---|---|",
        if in_corpus { "---|" } else { "" }
    );
    for m in list.iter().take(MOST_ROWS) {
        let sequence = m
            .sequence
            .as_ref()
            .map_or_else(String::new, |s| format!("{} | ", cell(s)));
        let _ = writeln!(
            out,
            "| {sequence}{} | {} | {}–{} | {} | {} | {} | {} | {} | {} |",
            cell(&m.kind),
            cell(&m.target),
            time(m.start_ms),
            time(m.end_ms),
            m.expected.title(),
            m.measured.title(),
            whole(m.score),
            m.weakest.name(),
            m.differs.as_deref().map_or_else(|| "–".to_string(), cell),
            if m.notes.is_empty() {
                "–".to_string()
            } else {
                cell(&m.notes.join("; "))
            }
        );
    }
    if list.len() > MOST_ROWS {
        let _ = writeln!(
            out,
            "\n…and {} more (the JSON scorecard lists them all).",
            list.len() - MOST_ROWS
        );
    }
}

fn labels(out: &mut String, check: &LabelCheck) {
    let _ = writeln!(
        out,
        "{} of {} effects judged look as the importer said they would.\n",
        check.agree, check.judged
    );
    let _ = writeln!(out, "### Expected to look better than they do\n");
    let _ = writeln!(
        out,
        "Judged where nothing else was on their pixels: a label to correct, or an effect to fix.\n"
    );
    misses(out, &check.optimistic);
    let _ = writeln!(
        out,
        "\n### Expected to look better than they do, judged with other effects mixed in\n"
    );
    let _ = writeln!(out, "The difference may belong to what they're layered with.\n");
    misses(out, &check.optimistic_layered);
    let _ = writeln!(out, "\n### Expected to look worse than they do\n");
    misses(out, &check.pessimistic);
    let _ = writeln!(out, "\n### By effect and what the importer noted\n");
    if check.rows.is_empty() {
        let _ = writeln!(out, "No effects.");
        return;
    }
    let _ = writeln!(
        out,
        "Counts are matches / close / different / no equivalent. `pf-fidelity labels` holds these \
         against the importer's table.\n"
    );
    let _ = writeln!(
        out,
        "| Effect | The importer noted | Expected | Effects | Judged | Judged alone | Say little | Look |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    for row in &check.rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            cell(&row.kind),
            if row.notes.is_empty() {
                "–".to_string()
            } else {
                cell(&row.notes.join("; "))
            },
            row.expected.title(),
            row.effects,
            counts(&row.measured),
            counts(&row.measured_alone),
            row.says_little,
            maybe(row.score, whole)
        );
    }
}

fn pixel_headline(out: &mut String, scores: &Scores, tolerance: u8) {
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
        "| Effect | Compared | Carried over exactly | Judged | Differ | Of those, judged alone |"
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
    let _ = writeln!(
        out,
        "What the importer carried over (every setting, some, or none) against the pixels. An \
         effect can be here and still look the same.\n"
    );
    let _ = writeln!(out, "#### Carried over exactly, not the same pixels\n");
    let _ = writeln!(
        out,
        "Measured pixel by pixel where nothing else was on the pixels.\n"
    );
    if audit.exact_but_differs.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.exact_but_differs);
    }
    let _ = writeln!(
        out,
        "\n#### Carried over exactly, not the same pixels, but only measured with other effects\n"
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
        "\n#### Carried over exactly, random by nature, a different distribution\n"
    );
    if audit.exact_but_distribution_differs.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.exact_but_distribution_differs);
    }
    let _ = writeln!(out, "\n#### Carried over with differences, yet the same pixels\n");
    if audit.approximate_but_matches.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        audit_table(out, &audit.approximate_but_matches);
    }
    let _ = writeln!(out, "\n#### Effects carried over exactly, by kind\n");
    if audit.exact_kinds.is_empty() {
        let _ = writeln!(out, "No effect was carried over exactly.");
    } else {
        exact_kinds(out, &audit.exact_kinds);
        if audit.exact_unjudged > 0 {
            let _ = writeln!(
                out,
                "\n{} carried over exactly couldn't be judged (not measured, measured on too little, \
                 or following music that wasn't read).",
                audit.exact_unjudged
            );
        }
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
        "{} long, {} frames {} ms apart, held against xLights' own render: {} channels and {} frames \
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
    let _ = writeln!(o, "## How it looks\n");
    look_summary(o, &card.look, &card.look_by_moment);

    let _ = writeln!(o, "\n## What to improve first\n");
    let _ = writeln!(
        o,
        "Kinds of effect by what there is to gain: how far each instance's look falls short of \
         matching, times the pixel-seconds it was lit on. \"Off\" is the part of the look that agrees \
         least.\n"
    );
    improve(o, &card.improve);

    let _ = writeln!(o, "\n## Every effect kind\n");
    kind_looks(o, &card.kinds);

    let _ = writeln!(o, "\n## Effects held against other seeds\n");
    let _ = writeln!(
        o,
        "{} An effect that draws on chance is judged next to PixelFlow's own renders of it from \
         other seeds, and so is the layered look of one that shares its pixels with such an effect; \
         where its moments are chance, the parts chance moves are read over the whole effect. \"By \
         moment\" is what it scores held moment by moment against xLights' render alone, as every \
         effect was judged before. \"Itself\" is how alike two of PixelFlow's own renders are in \
         the part of the look chance moves most: under {}, the look says too little and the \
         effect is not judged (the next list).\n",
        if card.method.by_moment {
            "Every effect was judged moment by moment in this scorecard (`--by-moment`).".to_string()
        } else {
            format!(
                "PixelFlow rendered the sequence {} more time{} from other seeds.",
                card.method.other_seeds,
                if card.method.other_seeds == 1 { "" } else { "s" }
            )
        },
        two(card.method.little)
    );
    chance_effects(o, card);

    let _ = writeln!(o, "\n## Not judged: looks that say little\n");
    said_little(o, card);

    let _ = writeln!(o, "\n## The importer's labels\n");
    labels(o, &card.labels);

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
        "- The importer expects {} to come across the same, {} to look close, {} to look different, \
         and has no PixelFlow effect for {}; {} xLights effects weren't imported.",
        seq.import.look.matches,
        seq.import.look.close,
        seq.import.look.different,
        seq.import.look.no_equivalent,
        seq.import.skipped
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

    let _ = writeln!(o, "\n## Effects that don't match, the most to gain first\n");
    let mut worst: Vec<&EffectScore> = card.effects.iter().filter(|e| e.look.is_some()).collect();
    let lost = |e: &EffectScore| e.look.as_ref().map_or(0.0, |l| (100.0 - l.score) * e.share);
    worst.retain(|e| e.look.as_ref().is_some_and(|l| l.verdict != Verdict::Matches));
    worst.sort_by(|a, b| lost(b).total_cmp(&lost(a)).then(a.index.cmp(&b.index)));
    let _ = writeln!(
        o,
        "| # | Effect | On | At | Expected | Looks | Score | By moment | Judged | {} | What a viewer would notice | Pixel-seconds |",
        component_names().join(" | ")
    );
    let _ = writeln!(
        o,
        "|---|---|---|---|---|---|---|---|---|{}---|---|",
        "---|".repeat(Component::ALL.len())
    );
    for e in worst.iter().take(MOST_ROWS) {
        let (Some(judged), Some((_, look))) = (&e.look, e.look_basis(&card.method.audit)) else {
            continue;
        };
        let _ = writeln!(
            o,
            "| {} | {} | {} | {}–{} | {} | {} | {} | {} | {} | {} | {} | {:.0} |",
            e.index,
            cell(&e.kind),
            cell(&e.target),
            time(e.start_ms),
            time(e.end_ms),
            e.expected.title(),
            judged.verdict.title(),
            whole(judged.score),
            judged
                .by_moment
                .map_or_else(|| "–".to_string(), |m| whole(m.score)),
            basis(judged.basis),
            component_cells(&look.components).join(" | "),
            judged.differs.as_deref().map_or_else(|| "–".to_string(), cell),
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

    let _ = writeln!(o, "\n## Props\n");
    let _ = writeln!(
        o,
        "Each prop over the frames either side lit it, whatever effects were on it. \"Error \
         reversed\" far below \"Error\" means the prop is wired the other way round.\n"
    );
    let _ = writeln!(
        o,
        "| Prop | Pixels | Measured | Channels | On | Look | Off | Error | Within | IoU | Error reversed |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|---|");
    for p in &card.props {
        let s = p.scores.as_ref();
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
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
            maybe(p.look.as_ref().map(|l| l.score), whole),
            p.look
                .as_ref()
                .filter(|l| l.verdict != Verdict::Matches)
                .map_or_else(|| "–".to_string(), |l| l.weakest.name().to_string()),
            maybe(s.map(|s| s.mae), num),
            maybe(s.map(|s| s.within_tolerance), pct),
            maybe(s.map(|s| s.lit_iou), two),
            maybe(p.reversed_mae, num),
        );
    }

    let _ = writeln!(o, "\n## Diagnostics: pixel for pixel\n");
    let _ = writeln!(
        o,
        "These compare the two renders channel by channel. They find structural bugs (a prop wired \
         backwards, a frame of delay, layers mixed in another order); they don't say how an import \
         looks, and a random effect scores badly here however alike it looks.\n"
    );
    match &card.diagnostics.overall {
        Some(scores) => pixel_headline(o, scores, card.method.limits.tolerance),
        None => {
            let _ = writeln!(o, "Nothing could be measured.");
        }
    }
    let _ = writeln!(o, "\n### Timing\n");
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
    let _ = writeln!(o, "\n### Deterministic effects by pixel error\n");
    let _ = writeln!(
        o,
        "Error (each instance's own frames where it has them) times its share of the pixel-seconds \
         it was lit on.\n"
    );
    ranking(o, &card.diagnostics.ranking, "Error (0–255)");
    let _ = writeln!(o, "\n### Stochastic effects by distribution\n");
    let _ = writeln!(
        o,
        "These place things at random, so pixel for pixel they are compared by what they show over \
         each {} ms (brightness, share of pixels lit, colors).\n",
        card.method.limits.window_ms
    );
    ranking(
        o,
        &card.diagnostics.stochastic_ranking,
        "Distribution distance (0–1)",
    );
    let _ = writeln!(o, "\n### Every effect kind, pixel for pixel\n");
    kinds(o, &card.kinds);
    if let Some(scores) = &card.diagnostics.unattributed {
        let _ = writeln!(
            o,
            "\nLit with no imported effect on the pixel: {:.0} pixel-seconds, error {}.",
            scores.pixel_seconds,
            num(scores.mae)
        );
    }
    let _ = writeln!(o, "\n### What the importer carried over, against the pixels\n");
    label_audit(o, &card.diagnostics.label_audit);
    out
}

/// A whole corpus's scorecard.
pub fn corpus_markdown(card: &CorpusCard) -> String {
    let mut out = String::new();
    let o = &mut out;
    let _ = writeln!(o, "# Fidelity scorecard: corpus\n");
    let _ = writeln!(
        o,
        "{} sequences held against what xLights rendered ({}).\n",
        card.sequences.len(),
        card.tool
    );
    let _ = writeln!(o, "## How they look\n");
    look_summary(o, &card.look, &card.look_by_moment);
    let _ = writeln!(o, "\n## Sequences\n");
    let _ = writeln!(
        o,
        "| Sequence | Length | Matches | Close | Different | No equivalent | Look | By moment | As seen | Effects judged | Not judged | Of that, says little |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|---|---|");
    for s in &card.sequences {
        let look = &s.look;
        let _ = writeln!(
            o,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} of {} | {} | {} |",
            cell(&s.label),
            time(s.duration_ms),
            pct(look.time.matches),
            pct(look.time.close),
            pct(look.time.different),
            pct(look.time.no_equivalent),
            maybe(look.score, whole),
            match (s.look_by_moment.score, look.score) {
                (Some(by_moment), Some(score)) => beside(by_moment, score),
                _ => "–".to_string(),
            },
            maybe(look.as_seen, whole),
            look.judged,
            look.of,
            pct(look.not_judged),
            if look.says_little.effects == 0 {
                "–".to_string()
            } else {
                format!(
                    "{} ({} effect{}, {} s)",
                    pct(look.says_little.share),
                    look.says_little.effects,
                    if look.says_little.effects == 1 { "" } else { "s" },
                    whole(look.says_little.seconds)
                )
            }
        );
    }
    if !card.failures.is_empty() {
        let _ = writeln!(o, "\nNot compared:\n");
        for f in &card.failures {
            let _ = writeln!(o, "- {}: {}", f.label, f.error);
        }
    }
    let _ = writeln!(o, "\n## What to improve first\n");
    let _ = writeln!(
        o,
        "Kinds of effect by what there is to gain: how far each instance's look falls short of \
         matching, times the pixel-seconds it was lit on. \"Off\" is the part of the look that agrees \
         least.\n"
    );
    improve(o, &card.improve);
    let _ = writeln!(o, "\n## Every effect kind\n");
    kind_looks(o, &card.kinds);
    let _ = writeln!(o, "\n## Not judged: looks that say little\n");
    corpus_said_little(o, card);
    let _ = writeln!(o, "\n## The importer's labels\n");
    labels(o, &card.labels);

    let _ = writeln!(o, "\n## Diagnostics: pixel for pixel\n");
    let _ = writeln!(
        o,
        "These compare the renders channel by channel. They find structural bugs; they don't say \
         how an import looks.\n"
    );
    let _ = writeln!(
        o,
        "| Sequence | Effects (exact / approx. / placeholder) | Measured | Pixels measured | Error | Within | IoU | Brightness correlation | Exact differing | Best offset |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|---|");
    for s in &card.sequences {
        let scores = s.overall.as_ref();
        let _ = writeln!(
            o,
            "| {} | {} ({} / {} / {}) | {} | {} of {} | {} | {} | {} | {} | {} of {} | {:+} |",
            cell(&s.label),
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
    let _ = writeln!(o, "\n### Deterministic effects by pixel error\n");
    ranking(o, &card.diagnostics.ranking, "Error (0–255)");
    let _ = writeln!(o, "\n### Stochastic effects by distribution\n");
    ranking(
        o,
        &card.diagnostics.stochastic_ranking,
        "Distribution distance (0–1)",
    );
    let _ = writeln!(o, "\n### Every effect kind, pixel for pixel\n");
    kinds(o, &card.kinds);
    let _ = writeln!(o, "\n### What the importer carried over, against the pixels\n");
    label_audit(o, &card.diagnostics.label_audit);
    out
}
