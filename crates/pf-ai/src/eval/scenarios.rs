//! The requests the Assistant is tried on: what is open when each is made, and what a good
//! answer does. Every one is on the made-up stage with its made-up song.

use super::{Check, Measures, Run, check, effects, parts};
use crate::effect_spec::{self, Context};
use crate::stage::{self, SONG_MS, Stage};
use pf_model::{PropId, Rgb};
use pf_sequence::{Effect, EffectKind, Layer, Sequence, Target};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// A request, where it starts from, and how its answer is judged.
pub struct Scenario {
    /// Its name on the command line.
    pub key: &'static str,
    pub title: &'static str,
    /// What the user types.
    pub request: &'static str,
    /// What is open when they type it, in a few words.
    pub opens: &'static str,
    pub(crate) start: fn(&Stage) -> Sequence,
    pub(crate) judge: fn(&Run<'_>, &Measures) -> Vec<Check>,
}

/// Every scenario, in the order they run.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            key: "whole-song",
            title: "A whole song from nothing",
            request: "Make a light show for this song.",
            opens: "a new sequence for the song, nothing on it",
            start: empty,
            judge: whole_song,
        },
        Scenario {
            key: "dancers",
            title: "Dancers for one section",
            request: "Put dancing skeletons on the matrix for the first chorus.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: dancers,
        },
        Scenario {
            key: "sing-along",
            title: "Singing and words",
            request: "Have the singing face sing the words, and put the words of each chorus up on the matrix as they are sung.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: sing_along,
        },
        Scenario {
            key: "build",
            title: "A build into the chorus",
            request: "Build up into the first chorus over the last two bars of the verse before it.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: build,
        },
        Scenario {
            key: "calm-and-big",
            title: "Calmer verses, bigger choruses",
            request: "It all looks the same from start to finish. Make the verses calmer and the choruses bigger.",
            opens: "one look held for the whole song",
            start: bland,
            judge: calm_and_big,
        },
        Scenario {
            key: "halloween",
            title: "Recolor for Halloween",
            request: "Recolor this for Halloween.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: halloween,
        },
        Scenario {
            key: "review-and-fix",
            title: "Review a sequence and fix it",
            request: "Review this sequence and fix what's wrong with it.",
            opens: "a sequence with faults: a shimmer over the whole house, a rainbow, tiny words, a wrong effect on a string, props left out, a dark last chorus",
            start: flawed,
            judge: review_and_fix,
        },
        Scenario {
            key: "scroll-words",
            title: "Scrolling words",
            request: "Scroll LANTERNS HIGH across the matrix through the last chorus.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: scroll_words,
        },
        Scenario {
            key: "question",
            title: "A question, nothing to change",
            request: "What's the difference between the Chase and Bars effects on my arches? Don't change anything.",
            opens: "a simple red, green, and white sequence",
            start: christmas,
            judge: question,
        },
    ]
}

// ---- where a scenario starts ----

/// `spec` (an effect as `place_effects` takes one) on the row of the prop or group called `name`.
fn put(stage: &Stage, doc: &mut Sequence, name: &str, layer: usize, (from, to): (u64, u64), spec: Value) {
    let tracks = doc.timing_tracks.clone();
    let cx = Context {
        tracks: &tracks,
        pictures: None,
    };
    let mut effect = effect_spec::parse(&spec, &cx).unwrap_or_else(|e| panic!("{name} {spec}: {e}"));
    effect.start_ms = from;
    effect.end_ms = to;
    effect.id = stage::steady_id(&format!("{name}/{layer}/{from}"));
    let target = stage
        .named(name)
        .unwrap_or_else(|| panic!("the stage has no {name}"));
    let row = doc
        .rows
        .iter_mut()
        .find(|r| r.target == target)
        .unwrap_or_else(|| panic!("the stage's sequence has no row for {name}"));
    while row.layers.len() <= layer {
        row.layers.push(Layer::default());
    }
    row.layers[layer].effects.push(effect);
}

/// The stage's new sequence, nothing on it.
pub fn empty(stage: &Stage) -> Sequence {
    stage.sequence()
}

/// A plain Christmas sequence: the house green in the verses and red in the choruses, the
/// arches and the tree joining in, warm white in between.
pub fn christmas(stage: &Stage) -> Sequence {
    let mut doc = stage.sequence();
    for section in &stage::song().sections {
        let span = (section.start_ms, section.end_ms);
        if section.label.starts_with("Chorus") {
            let on = json!({ "kind": "on", "colors": ["#ff0000"], "brightness": 0.9 });
            put(stage, &mut doc, "Whole House", 0, span, on);
            let bars = json!({ "kind": "bars", "settings": { "axis": "horizontal", "count": 3 }, "colors": ["#ff0000", "#ffffff"] });
            put(stage, &mut doc, "Arches", 0, span, bars);
            let spiral = json!({ "kind": "spiral", "colors": ["#ff0000", "#00a030"] });
            put(stage, &mut doc, "Mega Tree", 0, span, spiral);
        } else if section.label.starts_with("Verse") {
            let on = json!({ "kind": "on", "colors": ["#00a030"], "brightness": 0.5 });
            put(stage, &mut doc, "Whole House", 0, span, on);
            let chase = json!({ "kind": "chase", "colors": ["#ff0000"] });
            put(stage, &mut doc, "Roofline", 0, span, chase);
            let twinkle = json!({ "kind": "twinkle", "colors": ["#ff0000", "#00a030"] });
            put(stage, &mut doc, "Mega Tree", 0, span, twinkle);
        } else {
            let on = json!({ "kind": "on", "colors": ["#ffe0b0"], "brightness": 0.3 });
            put(stage, &mut doc, "Whole House", 0, span, on);
        }
    }
    doc
}

/// One look from the first second to the last.
pub fn bland(stage: &Stage) -> Sequence {
    let mut doc = stage.sequence();
    let whole = (0, SONG_MS);
    let on = json!({ "kind": "on", "colors": ["#ffe0b0"] });
    put(stage, &mut doc, "Whole House", 0, whole, on);
    let chase = json!({ "kind": "chase", "colors": ["#ffffff"] });
    put(stage, &mut doc, "Arches", 0, whole, chase);
    let twinkle = json!({ "kind": "twinkle", "colors": ["#ffffff"] });
    put(stage, &mut doc, "Mega Tree", 0, whole, twinkle);
    doc
}

/// A sequence with faults a designer would name at a glance.
pub fn flawed(stage: &Stage) -> Sequence {
    let mut doc = stage.sequence();
    for span in [(24_000, 40_000), (56_000, 72_000)] {
        // The whole house flickering twenty times a second for a whole chorus.
        let shimmer = json!({ "kind": "on", "settings": { "shimmer": true }, "colors": ["#ffffff"] });
        put(stage, &mut doc, "Whole House", 0, span, shimmer);
    }
    for span in [(8_000, 24_000), (40_000, 56_000)] {
        // Six colors at once.
        let rainbow = json!({ "kind": "bars", "settings": { "axis": "horizontal", "count": 6 },
            "colors": ["#ff0000", "#ff8800", "#ffff00", "#00ff00", "#0000ff", "#ff00ff"] });
        put(stage, &mut doc, "Roofline", 0, span, rainbow);
        // A picture effect on a single run of pixels.
        let life = json!({ "kind": "life", "colors": ["#00ffff", "#ff00aa"] });
        put(stage, &mut doc, "Arch 1", 0, span, life);
        // Words too small and too many to read.
        let words = json!({ "kind": "text", "settings": { "text": "EVERY WINDOW STARTS TO GLOW TONIGHT", "size": 5 }, "colors": ["#ffffff"] });
        put(stage, &mut doc, "Matrix", 0, span, words);
    }
    doc
}

// ---- what a good answer does ----

/// The tools that change the sequence.
fn drafts(tool: &str) -> bool {
    matches!(
        tool,
        "place_effects" | "repeat_effects" | "change_effects" | "stage_cue"
    ) || tool.starts_with("sequence_")
}

fn proposes(m: &Measures) -> Check {
    check(
        "It proposes its changes for the user to apply",
        m.proposed,
        if m.proposed { "proposed" } else { "no proposal" },
    )
}

fn used(run: &Run<'_>, tool: &str, why: &str) -> Check {
    let times = run.tools.iter().filter(|t| t.name == tool).count();
    check(why, times > 0, format!("{tool} called {times} times"))
}

/// Whether it had the song's analysis before designing: with the request, or by asking.
fn listened(run: &Run<'_>, m: &Measures) -> Check {
    let times = run.tools.iter().filter(|t| t.name == "analyze_song").count();
    check(
        "It listened to the song before designing",
        m.song_up_front || times > 0,
        if m.song_up_front {
            "the analysis came with the request".to_string()
        } else {
            format!("analyze_song called {times} times")
        },
    )
}

/// Whether it read `guide` before it drafted anything.
fn read_first(run: &Run<'_>, guide: &str) -> Check {
    let read = run.tools.iter().position(|t| {
        t.name == "get_guide"
            && ["topics", "effects"].iter().any(|list| {
                t.input[*list]
                    .as_array()
                    .is_some_and(|names| names.iter().any(|n| n == guide))
            })
    });
    let drafted = run.tools.iter().position(|t| drafts(&t.name));
    let passed = match (read, drafted) {
        (Some(read), Some(drafted)) => read < drafted,
        (Some(_), None) => true,
        (None, _) => false,
    };
    check(
        &format!("It read the {guide} guide before changing anything"),
        passed,
        match read {
            Some(at) => format!("read at tool call {}", at + 1),
            None => "never read".to_string(),
        },
    )
}

fn small(m: &Measures, most: usize) -> Check {
    check(
        &format!("A small job done small ({most} model requests or fewer)"),
        m.requests <= most,
        format!("{} requests", m.requests),
    )
}

fn safe(m: &Measures) -> Check {
    let review = m.review.as_ref();
    check(
        "Nothing flashes unsafely",
        review.is_some_and(|r| r.flashes_safe && r.criteria.safety == 100),
        review.map_or("not reviewed".to_string(), |r| {
            format!("safety {}", r.criteria.safety)
        }),
    )
}

/// Whether what was there outside `from..to` is as it was, and nothing new landed far from it.
fn untouched_outside(run: &Run<'_>, from: u64, to: u64) -> Check {
    let inside = |e: &Effect| e.start_ms < to + 2_000 && e.end_ms + 1_000 > from;
    let after: Vec<&Effect> = effects(run.after).map(|(_, _, e)| e).collect();
    let changed = effects(run.before)
        .filter(|(_, _, e)| !(e.start_ms < to && e.end_ms > from))
        .filter(|(_, _, e)| !after.contains(e))
        .count();
    let before: BTreeSet<_> = effects(run.before).map(|(_, _, e)| e.id).collect();
    let strays = after
        .iter()
        .filter(|e| !before.contains(&e.id) && !inside(e))
        .count();
    check(
        "It left the rest of the song as it was",
        changed == 0 && strays == 0,
        format!("{changed} effects elsewhere changed, {strays} new ones elsewhere"),
    )
}

fn on_prop(target: Target, prop: PropId) -> bool {
    matches!(target, Target::Prop(id) | Target::Region { prop: id, .. } if id == prop)
}

/// The effects of `kind` on `prop`'s own rows that play within `from..to`.
fn found(
    doc: &Sequence,
    kind: EffectKind,
    prop: PropId,
    (from, to): (u64, u64),
) -> impl Iterator<Item = &Effect> {
    effects(doc)
        .filter(move |(target, _, e)| {
            on_prop(*target, prop) && e.kind() == kind && e.start_ms < to && e.end_ms > from
        })
        .map(|(_, _, e)| e)
}

/// How much of `from..to` those effects cover, in milliseconds.
fn covered(doc: &Sequence, kind: EffectKind, prop: PropId, (from, to): (u64, u64)) -> u64 {
    let mut spans: Vec<(u64, u64)> = found(doc, kind, prop, (from, to))
        .map(|e| (e.start_ms.max(from), e.end_ms.min(to)))
        .collect();
    spans.sort_unstable();
    let (mut total, mut reached) = (0, from);
    for (start, end) in spans {
        let start = start.max(reached);
        if end > start {
            total += end - start;
            reached = end;
        }
    }
    total
}

/// One of an effect's own settings, as the tools name it.
fn setting(effect: &Effect, key: &str) -> Value {
    serde_json::to_value(&effect.params)
        .ok()
        .map_or(Value::Null, |v| v[key].clone())
}

/// A color's hue in degrees, or `None` for black, white, and grays.
fn hue(color: Rgb) -> Option<f32> {
    let [r, g, b] = [color.r, color.g, color.b].map(f32::from);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max < 40.0 || (max - min) / max < 0.25 {
        return None;
    }
    let span = max - min;
    let degrees = if max == r {
        60.0 * ((g - b) / span).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / span + 2.0)
    } else {
        60.0 * ((r - g) / span + 4.0)
    };
    Some(degrees.rem_euclid(360.0))
}

fn seconds(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / 1000.0)
}

fn whole_song(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let review = m.review.as_ref();
    let stop = run.lights.level(&[(79_250, 79_750)]);
    vec![
        proposes(m),
        read_first(run, "method"),
        listened(run, m),
        used(run, "review_draft", "It reviewed its draft before proposing"),
        check(
            "Most props have a part (9 of 12 or more)",
            m.props_used >= 9,
            format!("{} of {}", m.props_used, m.props),
        ),
        check(
            "Five kinds of effect or more",
            m.effect_kinds.len() >= 5,
            format!("{}", m.effect_kinds.len()),
        ),
        check(
            "The choruses are brighter than the verses, by a quarter or more",
            m.chorus_light >= m.verse_light * 1.25 && m.chorus_light > 0.05,
            format!("verses {:.2}, choruses {:.2}", m.verse_light, m.chorus_light),
        ),
        check(
            "The look changes where the music does (5 of 7 section starts or more)",
            m.section_changes >= 5,
            format!("{} of {}", m.section_changes, m.section_starts),
        ),
        check(
            "The stop before the last chorus goes dark",
            stop <= 0.25 * m.chorus_light && m.chorus_light > 0.05,
            format!("{stop:.2} against {:.2} in the choruses", m.chorus_light),
        ),
        check(
            "The biggest moments are hit (two thirds of them or more)",
            review.is_some_and(|r| r.moments > 0 && r.moments_hit * 3 >= r.moments * 2),
            review.map_or("not reviewed".to_string(), |r| {
                format!("{} of {}", r.moments_hit, r.moments)
            }),
        ),
        safe(m),
        check(
            "The review scores it 70 or more",
            review.is_some_and(|r| r.score >= 70),
            review.map_or("not reviewed".to_string(), |r| r.score.to_string()),
        ),
    ]
}

fn dancers(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let chorus = (24_000, 40_000);
    let dancing = covered(run.after, EffectKind::Dancer, run.stage.matrix, chorus);
    let characters: BTreeSet<String> = found(run.after, EffectKind::Dancer, run.stage.matrix, chorus)
        .map(|e| setting(e, "character").as_str().unwrap_or("?").to_string())
        .collect();
    vec![
        proposes(m),
        check(
            "Dancers on the Matrix for most of the first chorus (12 of its 16 seconds or more)",
            dancing >= 12_000,
            seconds(dancing),
        ),
        check(
            "They are skeletons",
            characters.len() == 1 && characters.contains("skeleton"),
            format!("{characters:?}"),
        ),
        untouched_outside(run, chorus.0, chorus.1),
        small(m, 10),
    ]
}

fn sing_along(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let lines: Vec<(u64, u64)> = run
        .after
        .timing_tracks
        .iter()
        .find(|t| t.name == crate::lyrics::tracks::LYRICS_TRACK)
        .map(|t| t.marks.iter().map(|m| (m.start_ms, m.end_ms)).collect())
        .unwrap_or_default();
    let sung = lines
        .iter()
        .filter(|line| covered(run.after, EffectKind::Faces, run.stage.face, **line) * 2 >= line.1 - line.0)
        .count();
    let choruses = parts(run.song, "Chorus");
    let chorus_lines: Vec<&(u64, u64)> = lines
        .iter()
        .filter(|line| choruses.iter().any(|c| c.0 <= line.0 && line.0 < c.1))
        .collect();
    let shown = chorus_lines
        .iter()
        .filter(|line| covered(run.after, EffectKind::Text, run.stage.matrix, ***line) > 0)
        .count();
    let text = m.review.as_ref().and_then(|r| r.criteria.text);
    // Most lines sung (three in four), and words for every chorus line but one at most.
    let (most_lines, most_chorus) = (
        (lines.len() * 3).div_ceil(4).max(1),
        chorus_lines.len().saturating_sub(1).max(1),
    );
    vec![
        proposes(m),
        check(
            &format!(
                "The Singing Face sings most of the lines ({most_lines} of {} or more)",
                lines.len()
            ),
            sung >= most_lines,
            format!("{sung} of {}", lines.len()),
        ),
        check(
            &format!(
                "Words are on the Matrix while the chorus lines are sung, the last chorus's too ({most_chorus} of {} or more)",
                chorus_lines.len()
            ),
            shown >= most_chorus,
            format!("{shown} of {}", chorus_lines.len()),
        ),
        check(
            "The words can be read (the review finds no fault with the text)",
            shown > 0 && text.is_none_or(|t| t >= 80),
            text.map_or("no text fault".to_string(), |t| format!("text {t}")),
        ),
        safe(m),
    ]
}

fn build(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let (from, to) = (20_000, 24_000);
    let before: BTreeSet<_> = effects(run.before).map(|(_, _, e)| e.id).collect();
    let new = effects(run.after)
        .filter(|(_, _, e)| !before.contains(&e.id) && e.start_ms < to && e.end_ms > from)
        .count();
    let (first, second) = (
        run.lights.level(&[(from, 22_000)]),
        run.lights.level(&[(22_000, to)]),
    );
    let (few, many) = (run.lights.lit(&[(from, 22_000)]), run.lights.lit(&[(22_000, to)]));
    vec![
        proposes(m),
        check(
            "Something new plays in the two bars before the chorus",
            new > 0,
            format!("{new} new effects"),
        ),
        check(
            "It grows: the second bar is brighter or has more props lit than the first",
            second > first * 1.1 || many > few + 0.5,
            format!("brightness {first:.2} then {second:.2}; props lit {few:.1} then {many:.1}"),
        ),
        untouched_outside(run, from, to),
        safe(m),
        small(m, 10),
    ]
}

fn calm_and_big(_: &Run<'_>, m: &Measures) -> Vec<Check> {
    let energy = m.review.as_ref().and_then(|r| r.criteria.energy);
    vec![
        proposes(m),
        check(
            "The choruses are half as bright again as the verses, or more",
            m.chorus_light >= m.verse_light * 1.5 && m.chorus_light > 0.05,
            format!("verses {:.2}, choruses {:.2}", m.verse_light, m.chorus_light),
        ),
        check(
            "Something rests in the verses (9 of 12 props lit or fewer, on average)",
            m.verse_props <= 9.0,
            format!("{:.1}", m.verse_props),
        ),
        check(
            "The choruses use the house (9 of 12 props lit or more, on average)",
            m.chorus_props >= 9.0,
            format!("{:.1}", m.chorus_props),
        ),
        check(
            "The verses and the choruses no longer look alike (the look changes at 4 section starts or more)",
            m.section_changes >= 4,
            format!("{} of {}", m.section_changes, m.section_starts),
        ),
        check(
            "The light follows the music (the review's energy score is 70 or more)",
            energy.is_some_and(|e| e >= 70),
            energy.map_or("not scored".to_string(), |e| e.to_string()),
        ),
        safe(m),
    ]
}

fn halloween(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let hues: Vec<f32> = effects(run.after)
        .flat_map(|(_, _, e)| e.palette.colors.iter().filter_map(|c| hue(*c)))
        .collect();
    let orange_or_purple = hues
        .iter()
        .filter(|h| (10.0..50.0).contains(*h) || (255.0..320.0).contains(*h))
        .count();
    let red = |h: f32| !(10.0..345.0).contains(&h);
    let green = |h: f32| (90.0..160.0).contains(&h);
    let christmas = effects(run.after)
        .filter(|(_, _, e)| {
            let hues: Vec<f32> = e.palette.colors.iter().filter_map(|c| hue(*c)).collect();
            hues.iter().any(|h| red(*h)) && hues.iter().any(|h| green(*h))
        })
        .count();
    let times = |doc: &Sequence| -> BTreeSet<(String, u64, u64)> {
        effects(doc)
            .map(|(_, _, e)| (e.id.to_string(), e.start_ms, e.end_ms))
            .collect()
    };
    vec![
        proposes(m),
        read_first(run, "palettes"),
        check(
            "Orange and purple lead (6 in 10 of the colors used, or more)",
            !hues.is_empty() && orange_or_purple * 10 >= hues.len() * 6,
            format!("{orange_or_purple} of {}", hues.len()),
        ),
        check(
            "No effect is left in Christmas red and green",
            christmas == 0,
            format!("{christmas} effects"),
        ),
        check(
            "The same effects at the same times: recolored, not rebuilt",
            times(run.before) == times(run.after),
            format!(
                "{} effects before, {} after",
                effects(run.before).count(),
                effects(run.after).count()
            ),
        ),
        small(m, 10),
    ]
}

fn review_and_fix(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let (before, after) = (m.review_before, m.review.as_ref().map(|r| r.score));
    vec![
        used(run, "review_draft", "It reviewed the sequence"),
        proposes(m),
        safe(m),
        check(
            "It scores better than it did, by 15 or more",
            matches!((before, after), (Some(b), Some(a)) if a >= b + 15),
            format!("{before:?} then {after:?}"),
        ),
        check(
            "Most props have a part (9 of 12 or more)",
            m.props_used >= 9,
            format!("{} of {}", m.props_used, m.props),
        ),
        check(
            "The last chorus is no longer dark",
            run.lights.level(&[(80_000, 92_000)]) > 0.1,
            format!("{:.2}", run.lights.level(&[(80_000, 92_000)])),
        ),
    ]
}

fn scroll_words(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let chorus = (80_000, 92_000);
    let texts: Vec<&Effect> = found(run.after, EffectKind::Text, run.stage.matrix, chorus).collect();
    let said: Vec<&Effect> = texts
        .iter()
        .copied()
        .filter(|e| {
            setting(e, "text")
                .as_str()
                .is_some_and(|t| t.to_lowercase().contains("lanterns high"))
        })
        .collect();
    let moving = said
        .iter()
        .filter(|e| setting(e, "movement").as_str().is_some_and(|m| m != "none"))
        .count();
    let text = m.review.as_ref().and_then(|r| r.criteria.text);
    vec![
        proposes(m),
        check(
            "The words are on the Matrix in the last chorus",
            !said.is_empty(),
            format!("{} text effects, {} with the words", texts.len(), said.len()),
        ),
        check("They scroll", moving > 0, format!("{moving} moving")),
        check(
            "They can be read (the review finds no fault with the text)",
            !said.is_empty() && text.is_none_or(|t| t >= 80),
            text.map_or("no text fault".to_string(), |t| format!("text {t}")),
        ),
        untouched_outside(run, chorus.0, chorus.1),
        small(m, 10),
    ]
}

fn question(run: &Run<'_>, m: &Measures) -> Vec<Check> {
    let read = |guide: &str| m.guides.iter().any(|g| g == guide);
    vec![
        check(
            "It answers in words",
            run.reply.chars().count() >= 80,
            format!("{} characters", run.reply.chars().count()),
        ),
        check(
            "It changes and proposes nothing",
            !m.proposed && run.after == run.before,
            if m.proposed { "proposed" } else { "no proposal" },
        ),
        check(
            "It read both effects' guides rather than guess",
            read("chase") && read("bars"),
            format!("read: {}", m.guides.join(", ")),
        ),
        small(m, 6),
    ]
}
