//! The brief and the plan of a staged build, read from the model's calls and checked against
//! the show, the song, the effects, and the tools before anything is built: a part of the song,
//! a prop, an effect, or a tool that isn't there is sent back, named, for the plan to be fixed;
//! so is a plan that leaves a part of the song to nobody (without saying why it stays as it is),
//! or keeps bigger choruses to a corner of the house.
//!
//! The plan is terse: each field has a length it is cut to ([`MAX_TEXT`] and the like), so what
//! the steps read stays small whatever the design wrote.
//!
//! The plan's steps are put in waves: a step waits for an earlier one it names (`after`), and
//! for any earlier one that lights the same props at the same time, so steps that run together
//! never draw over each other. Waves run one after another; the steps of a wave at the same time.

use crate::draft::Draft;
use crate::knowledge::{library, reference};
use crate::tools::Toolbox;
use pf_analysis::Analysis;
use pf_model::PropId;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Steps a plan may have.
pub const MAX_PLAN_STEPS: usize = 8;
/// What a step's `do` is cut to (characters).
pub const MAX_DO: usize = 600;
/// What the plan's other words are cut to: a part's look and why it is kept, a role, the
/// summary, the vocals.
pub const MAX_TEXT: usize = 200;
/// Colors in a part's palette, at most.
pub const MAX_PALETTE: usize = 3;
/// Effects and guide topics a step is given, at most.
pub const MAX_STEP_EFFECTS: usize = 6;
pub const MAX_STEP_TOPICS: usize = 3;
/// Moments a plan stages by hand, at most.
pub const MAX_MOMENTS: usize = 16;
/// How a part moves.
pub const MOTIONS: [&str; 4] = ["still", "slow", "medium", "fast"];
/// What a plan's choruses light, at least, when the brief asks for bigger choruses: three props
/// in four.
const BIG_SHARE: (usize, usize) = (3, 4);

/// `text` cut to `most` characters (with an ellipsis when it was longer).
fn cut(text: String, most: usize) -> String {
    if text.chars().count() <= most {
        return text;
    }
    let mut out: String = text.chars().take(most.saturating_sub(1)).collect();
    out.push('…');
    out
}
/// What every part of the song is called when a plan means all of it.
pub const WHOLE_SONG: &str = "Whole song";
/// What a step's `props` says to mean every prop.
pub const ALL_PROPS: &str = "all";

/// Tools a step can never have: reviewing, proposing, and asking belong to the stages around
/// the build, and a build changes the sequence, not the layout.
const NOT_FOR_STEPS: &[&str] = &[
    "plan_build",
    "review_draft",
    "reset_draft",
    "propose_changes",
    "ask_for_song",
    "use_tools",
];

/// Whether a step may be given the tool `name`.
pub fn step_tool(name: &str) -> bool {
    !NOT_FOR_STEPS.contains(&name) && !name.starts_with("show_")
}

/// Whether the tool `name` changes the draft (rather than reading it).
pub fn drafts(name: &str) -> bool {
    matches!(
        name,
        "place_effects" | "repeat_effects" | "change_effects" | "stage_cue" | "add_song_timing"
    ) || name.starts_with("sequence_")
}

/// A part of the song a plan can name.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub label: String,
    pub from_ms: u64,
    pub to_ms: u64,
}

/// What a brief and a plan are checked against: the song's parts, the show's props and groups
/// (each as the props it lights), and the tools a step can have.
#[derive(Debug, Clone)]
pub struct World {
    pub parts: Vec<Part>,
    pub duration_ms: u64,
    /// A bar of the song (ms): two seconds when the song has no steady pulse.
    pub bar_ms: u64,
    /// Props and groups by lowercase name: the props each lights.
    targets: BTreeMap<String, (String, BTreeSet<PropId>)>,
    all_props: BTreeSet<PropId>,
    /// Each prop's name.
    prop_names: BTreeMap<PropId, String>,
    tools: Vec<String>,
}

impl World {
    /// The world of `draft`'s show and sequence, with `song` (its analysis, when there is one).
    pub fn of(draft: &Draft, song: Option<&Analysis>, toolbox: &Toolbox) -> Self {
        let show = draft.show();
        let duration_ms = draft.sequence().map_or(0, |s| s.duration_ms);
        let user = draft.base().sequence.as_ref().map(|s| &s.doc);
        let mut parts: Vec<Part> = song
            .map(|analysis| crate::song::describe(analysis, user))
            .and_then(|described| described["sections"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| {
                Some(Part {
                    label: s["label"].as_str()?.to_string(),
                    from_ms: s["startMs"].as_u64()?,
                    to_ms: s["endMs"].as_u64()?,
                })
            })
            .collect();
        parts.retain(|p| p.to_ms > p.from_ms);
        let mut targets = BTreeMap::new();
        for prop in &show.props {
            targets.insert(
                prop.name.to_lowercase(),
                (prop.name.clone(), BTreeSet::from([prop.id])),
            );
        }
        for group in &show.groups {
            let props: BTreeSet<PropId> = group.members.iter().map(|m| m.prop()).collect();
            targets
                .entry(group.name.to_lowercase())
                .or_insert((group.name.clone(), props));
        }
        let bar_ms = song
            .and_then(|a| a.tempo_bpm)
            .filter(|bpm| *bpm > 20.0)
            .map_or(2_000, |bpm| (4.0 * 60_000.0 / f64::from(bpm)).round() as u64);
        Self {
            parts,
            duration_ms,
            bar_ms,
            targets,
            all_props: show.props.iter().map(|p| p.id).collect(),
            prop_names: show.props.iter().map(|p| (p.id, p.name.clone())).collect(),
            tools: toolbox
                .tools()
                .iter()
                .map(|t| t.spec.name.clone())
                .filter(|name| step_tool(name))
                .collect(),
        }
    }

    /// The spans a part's name covers: a label ("Chorus 2"), a label's root ("Chorus": every
    /// chorus), one of a repeated part by its count ("Chorus 2": the second, when every chorus
    /// is labelled "Chorus"), or the whole song.
    pub fn spans(&self, name: &str) -> Option<Vec<(u64, u64)>> {
        let wanted = name.trim();
        if wanted.eq_ignore_ascii_case(WHOLE_SONG) {
            return Some(vec![(0, self.duration_ms.max(1))]);
        }
        let exact: Vec<(u64, u64)> = self
            .parts
            .iter()
            .filter(|p| p.label.eq_ignore_ascii_case(wanted))
            .map(|p| (p.from_ms, p.to_ms))
            .collect();
        if !exact.is_empty() {
            return Some(exact);
        }
        let root = |key: &str| -> Vec<(u64, u64)> {
            self.parts
                .iter()
                .filter(|p| crate::song::label_root(&p.label).eq_ignore_ascii_case(key))
                .map(|p| (p.from_ms, p.to_ms))
                .collect()
        };
        let base = crate::song::label_root(wanted);
        if base.len() < wanted.len()
            && let Ok(n) = wanted[base.len()..].trim().parse::<usize>()
        {
            return n
                .checked_sub(1)
                .and_then(|i| root(base).get(i).copied())
                .map(|span| vec![span]);
        }
        let all = root(wanted);
        (!all.is_empty()).then_some(all)
    }

    /// The props a name lights: a prop's, a group's members, or every prop for "all".
    pub fn props(&self, name: &str) -> Option<BTreeSet<PropId>> {
        let wanted = name.trim();
        if wanted.eq_ignore_ascii_case(ALL_PROPS) {
            return Some(self.all_props.clone());
        }
        let key = wanted.strip_prefix("group ").unwrap_or(wanted).to_lowercase();
        self.targets.get(&key).map(|(_, props)| props.clone())
    }

    /// Every part of the song by a name of its own: its label, numbered when the label repeats
    /// ("Chorus 3", as [`World::spans`] reads it), with its times.
    pub fn occurrences(&self) -> Vec<(String, (u64, u64))> {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let repeats = |label: &str| {
            let root = crate::song::label_root(label);
            root == label.trim()
                && self
                    .parts
                    .iter()
                    .filter(|p| crate::song::label_root(&p.label).eq_ignore_ascii_case(root))
                    .count()
                    > 1
        };
        self.parts
            .iter()
            .map(|p| {
                let name = if repeats(&p.label) {
                    let n = seen.entry(p.label.to_lowercase()).or_default();
                    *n += 1;
                    format!("{} {n}", p.label.trim())
                } else {
                    p.label.clone()
                };
                (name, (p.from_ms, p.to_ms))
            })
            .collect()
    }

    /// Every prop of the show.
    pub fn all_props(&self) -> &BTreeSet<PropId> {
        &self.all_props
    }

    /// A prop's name.
    pub fn prop_name(&self, id: PropId) -> &str {
        self.prop_names.get(&id).map_or("a prop", String::as_str)
    }

    /// The names the song's parts go by, for a message.
    fn part_names(&self) -> String {
        let mut names: Vec<String> = self.parts.iter().map(|p| format!("\"{}\"", p.label)).collect();
        names.push(format!("\"{WHOLE_SONG}\""));
        names.join(", ")
    }

    fn target_names(&self) -> String {
        let names: Vec<String> = self
            .targets
            .values()
            .take(40)
            .map(|(name, _)| format!("\"{name}\""))
            .collect();
        format!("{}, or \"{ALL_PROPS}\"", names.join(", "))
    }
}

fn text(value: &Value, field: &str) -> String {
    value[field].as_str().unwrap_or_default().trim().to_string()
}

fn names(value: &Value, field: &str) -> Vec<String> {
    match &value[field] {
        Value::String(one) if !one.trim().is_empty() => vec![one.trim().to_string()],
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// What the person asked for, as the first stage read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Brief {
    /// What is wanted, in a sentence or two.
    pub ask: String,
    /// The parts of the song the job is about (none: all of it).
    pub sections: Vec<String>,
    /// What must stay as it is.
    pub keep: String,
    /// What the person said about the look.
    pub style: String,
}

impl Brief {
    /// The brief in `input` (`plan_build`'s), checked against `world`.
    pub fn read(input: &Value, world: &World) -> Result<Self, String> {
        let brief = Self {
            ask: text(input, "ask"),
            sections: names(input, "sections"),
            keep: text(input, "keep"),
            style: text(input, "style"),
        };
        let mut problems = Vec::new();
        if brief.ask.is_empty() {
            problems.push("ask: say what the person wants, in a sentence or two.".to_string());
        }
        for section in &brief.sections {
            if world.spans(section).is_none() {
                problems.push(format!(
                    "sections: there's no part called \"{section}\". Parts: {}.",
                    world.part_names()
                ));
            }
        }
        if problems.is_empty() {
            Ok(brief)
        } else {
            Err(format!(
                "Nothing was planned yet. Fix the brief: {}",
                problems.join(" ")
            ))
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "ask": self.ask, "sections": self.sections, "keep": self.keep, "style": self.style })
    }

    /// The times the job is about: the parts it names, or the whole song.
    pub fn scope(&self, world: &World) -> Vec<(u64, u64)> {
        let named: Vec<(u64, u64)> = self
            .sections
            .iter()
            .filter_map(|s| world.spans(s))
            .flatten()
            .collect();
        if named.is_empty() {
            vec![(0, world.duration_ms.max(1))]
        } else {
            named
        }
    }

    /// Whether the person asked for bigger choruses.
    pub fn wants_bigger_choruses(&self) -> bool {
        let said = format!("{} {}", self.ask, self.style).to_lowercase();
        said.contains("chorus")
            && [
                "bigger",
                "biggest",
                "larger",
                "fuller",
                "big chorus",
                "more of the house",
            ]
            .iter()
            .any(|w| said.contains(w))
    }
}

/// Whether `span` lies mostly inside `spans` (half of it or more).
pub fn inside(spans: &[(u64, u64)], span: (u64, u64)) -> bool {
    let length = span.1.saturating_sub(span.0).max(1);
    let covered: u64 = spans
        .iter()
        .map(|(a, b)| span.1.min(*b).saturating_sub(span.0.max(*a)))
        .sum();
    covered * 2 >= length
}

/// One part of the song as the plan has it.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionPlan {
    pub label: String,
    pub palette: Vec<String>,
    /// The props and groups that play (the rest rest).
    pub props: Vec<String>,
    /// How bright it plays, 0–1, when the plan says.
    pub level: Option<f64>,
    /// How it moves: one of [`MOTIONS`], or nothing said.
    pub motion: String,
    /// The look, in a few words.
    pub look: String,
    /// Why it is left as it is (nothing: the plan builds it).
    pub keep: String,
    /// The times its label covers.
    pub spans: Vec<(u64, u64)>,
    /// The props it has playing.
    pub lights: BTreeSet<PropId>,
}

impl SectionPlan {
    /// Whether the plan builds this part and has something playing there.
    pub fn plays(&self) -> bool {
        self.keep.is_empty() && !self.lights.is_empty()
    }

    /// How much of the song its label names: a part by itself, every part of a label, or the
    /// whole song (lower: more particular).
    fn breadth(&self) -> u8 {
        if self.label.eq_ignore_ascii_case(WHOLE_SONG) {
            2
        } else {
            u8::from(self.spans.len() > 1)
        }
    }
}

/// A moment the plan stages by hand: a cue at a time, on some props (none: every prop).
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub at: u64,
    pub until: Option<u64>,
    pub cue: String,
    pub props: Vec<String>,
    pub lights: BTreeSet<PropId>,
}

impl Moment {
    /// The time it takes: to `until`, or a bar.
    pub fn window(&self, bar_ms: u64) -> (u64, u64) {
        (self.at, self.until.unwrap_or(self.at + bar_ms).max(self.at + 1))
    }
}

/// One step of the build: built in a conversation of its own, with only its tools.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub sections: Vec<String>,
    pub props: Vec<String>,
    pub tools: Vec<String>,
    pub effects: Vec<String>,
    pub topics: Vec<String>,
    /// What to build.
    pub todo: String,
    pub after: Vec<String>,
    /// The times its sections cover.
    pub spans: Vec<(u64, u64)>,
    /// The props it lights.
    pub lights: BTreeSet<PropId>,
}

/// Whether the tool `name` puts new effects in (rather than changing or removing those there).
pub fn places(name: &str) -> bool {
    matches!(
        name,
        "place_effects" | "repeat_effects" | "stage_cue" | "sequence_add_effect"
    )
}

impl Step {
    /// Whether this step and `other` light some of the same props at the same time.
    fn overlaps(&self, other: &Step) -> bool {
        let in_time = self
            .spans
            .iter()
            .any(|a| other.spans.iter().any(|b| a.0 < b.1 && b.0 < a.1));
        in_time && !self.lights.is_disjoint(&other.lights)
    }

    /// Its parts of the song, in a few words ("Verse 1, Verse 2").
    pub fn label(&self) -> String {
        if self.sections.is_empty() {
            self.title.clone()
        } else {
            self.sections.join(", ")
        }
    }

    /// Whether it works over the whole song (the moments, the singing) rather than parts of it.
    pub fn whole_song(&self) -> bool {
        self.sections.is_empty() || self.sections.iter().any(|s| s.eq_ignore_ascii_case(WHOLE_SONG))
    }

    /// Whether it can put a look in (with effects of its own, not cues alone).
    pub fn lays_looks(&self) -> bool {
        self.tools.iter().any(|t| {
            matches!(
                t.as_str(),
                "place_effects" | "repeat_effects" | "sequence_add_effect"
            )
        })
    }

    /// Whether it builds the part at `span`: a step of that part (by its label, the label's root,
    /// or its number) that changes the sequence, or one over the whole song that lays looks.
    fn builds(&self, span: (u64, u64)) -> bool {
        if self.whole_song() {
            self.lays_looks()
        } else {
            self.tools.iter().any(|t| drafts(t)) && inside(&self.spans, span)
        }
    }
}

/// The design: what the show is, part by part, and the steps that build it.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub summary: String,
    pub sections: Vec<SectionPlan>,
    pub roles: Vec<(String, String)>,
    /// The moments it stages by hand.
    pub moments: Vec<Moment>,
    /// What it says of the moments besides (or instead, in a plan written as prose).
    pub moments_note: String,
    pub vocals: String,
    pub steps: Vec<Step>,
}

fn is_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The first of `fields` in `value` that has words, joined with the rest that have.
fn words(value: &Value, fields: &[&str]) -> String {
    fields
        .iter()
        .map(|f| text(value, f))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

impl Plan {
    /// The plan in `input` (`submit_plan`'s), checked against `world` for a job over the whole
    /// song: see [`Plan::read_for`].
    pub fn read(input: &Value, world: &World) -> Result<Self, Vec<String>> {
        Self::read_for(input, world, None)
    }

    /// The plan in `input` (`submit_plan`'s), checked against `world` and `brief`: every part,
    /// prop, effect, tool, guide, and step it names must be there; every part of the song the
    /// job is about must be described (or kept as it is, saying why) and, where something plays,
    /// built by a step; and when the brief asks for bigger choruses, they light most of the
    /// house. Every problem is listed at once. Words past their length are cut, not refused.
    pub fn read_for(input: &Value, world: &World, brief: Option<&Brief>) -> Result<Self, Vec<String>> {
        let mut problems = Vec::new();
        let summary = cut(text(input, "summary"), MAX_TEXT);
        let mut sections = Vec::new();
        for (i, s) in input["sections"].as_array().into_iter().flatten().enumerate() {
            let label = text(s, "label");
            let spans = world.spans(&label);
            if spans.is_none() {
                problems.push(format!(
                    "sections[{i}]: there's no part called \"{label}\". Parts: {}.",
                    world.part_names()
                ));
            }
            let mut palette = names(s, "palette");
            if let Some(bad) = palette.iter().find(|c| !is_color(c)) {
                problems.push(format!(
                    "sections[{i}] ({label}): \"{bad}\" isn't a color: write \"#rrggbb\"."
                ));
            }
            palette.truncate(MAX_PALETTE);
            let props = names(s, "props");
            let mut lights = BTreeSet::new();
            for name in &props {
                match world.props(name) {
                    Some(found) => lights.extend(found),
                    None => problems.push(format!(
                        "sections[{i}] ({label}): there's no prop or group \"{name}\"."
                    )),
                }
            }
            let motion = text(s, "motion").to_lowercase();
            sections.push(SectionPlan {
                label,
                palette,
                props,
                level: s["level"].as_f64().map(|l| l.clamp(0.0, 1.0)),
                motion: if MOTIONS.contains(&motion.as_str()) {
                    motion
                } else {
                    String::new()
                },
                // A plan in the longer form gives the base, the movement, and the accents.
                look: cut(words(s, &["look", "base", "movement", "accents"]), MAX_TEXT),
                keep: cut(text(s, "keep"), MAX_TEXT),
                spans: spans.unwrap_or_default(),
                lights,
            });
        }
        let mut roles = Vec::new();
        for r in input["roles"].as_array().into_iter().flatten() {
            let prop = text(r, "prop");
            if world.props(&prop).is_none() {
                problems.push(format!("roles: there's no prop or group \"{prop}\"."));
            }
            roles.push((prop, cut(text(r, "role"), MAX_TEXT / 2)));
        }
        let (moments, moments_note) = read_moments(&input["moments"], world, &mut problems);
        let mut steps: Vec<Step> = Vec::new();
        let given = input["steps"].as_array().cloned().unwrap_or_default();
        if given.is_empty() || given.len() > MAX_PLAN_STEPS {
            problems.push(format!("steps: give 1 to {MAX_PLAN_STEPS} steps."));
        }
        let topics = library::topic_keys();
        for (i, s) in given.iter().take(MAX_PLAN_STEPS).enumerate() {
            let id = text(s, "id");
            let at = format!("steps[{i}] ({id})");
            if id.is_empty() || steps.iter().any(|earlier| earlier.id == id) {
                problems.push(format!("steps[{i}]: give each step an id of its own."));
            }
            let section_names = names(s, "sections");
            let mut spans = Vec::new();
            for name in &section_names {
                match world.spans(name) {
                    Some(found) => spans.extend(found),
                    None => problems.push(format!(
                        "{at}: there's no part called \"{name}\". Parts: {}.",
                        world.part_names()
                    )),
                }
            }
            if section_names.is_empty() {
                spans.push((0, world.duration_ms.max(1)));
            }
            let props = names(s, "props");
            let mut lights = BTreeSet::new();
            for name in &props {
                match world.props(name) {
                    Some(found) => lights.extend(found),
                    None => problems.push(format!(
                        "{at}: there's no prop or group \"{name}\". Props and groups: {}.",
                        world.target_names()
                    )),
                }
            }
            if props.is_empty() {
                problems.push(format!(
                    "{at}: name the props or groups it lights (or \"{ALL_PROPS}\")."
                ));
            }
            let tools = names(s, "tools");
            for tool in &tools {
                if !world.tools.contains(tool) {
                    problems.push(format!(
                        "{at}: a step can't use \"{tool}\". Tools for steps: {}.",
                        world.tools.join(", ")
                    ));
                }
            }
            if !tools.iter().any(|t| drafts(t)) {
                problems.push(format!(
                    "{at}: name a tool that changes the sequence (place_effects, change_effects, repeat_effects, stage_cue, ...)."
                ));
            }
            let mut effects = names(s, "effects");
            for effect in &effects {
                if reference::kind_named(effect).is_none() {
                    problems.push(format!(
                        "{at}: there's no effect \"{effect}\". Effects: {}.",
                        reference::kind_keys().join(", ")
                    ));
                }
            }
            effects.truncate(MAX_STEP_EFFECTS);
            let mut step_topics = names(s, "topics");
            for topic in &step_topics {
                if !topics.contains(&topic.as_str()) {
                    problems.push(format!(
                        "{at}: there's no guide topic \"{topic}\". Topics: {}.",
                        topics.join(", ")
                    ));
                }
            }
            step_topics.truncate(MAX_STEP_TOPICS);
            let after = names(s, "after");
            for other in &after {
                if !steps.iter().any(|earlier| earlier.id == *other) {
                    problems.push(format!(
                        "{at}: after names \"{other}\", which isn't an earlier step."
                    ));
                }
            }
            let todo = cut(text(s, "do"), MAX_DO);
            if todo.is_empty() {
                problems.push(format!("{at}: say in \"do\" what the step builds."));
            }
            let title = match text(s, "title") {
                t if t.is_empty() => id.clone(),
                t => cut(t, 80),
            };
            steps.push(Step {
                id,
                title,
                sections: section_names,
                props,
                tools,
                effects,
                topics: step_topics,
                todo,
                after,
                spans,
                lights,
            });
        }
        let plan = Self {
            summary,
            sections,
            roles,
            moments,
            moments_note,
            vocals: cut(text(input, "vocals"), MAX_TEXT),
            steps,
        };
        if problems.is_empty() {
            problems.extend(plan.gaps(world, brief));
        }
        if problems.is_empty() {
            Ok(plan)
        } else {
            Err(problems)
        }
    }

    /// What the plan leaves out of the job: a part of the song nobody describes, a part with
    /// something playing that no step builds, and choruses kept small when the brief asks for
    /// bigger ones.
    fn gaps(&self, world: &World, brief: Option<&Brief>) -> Vec<String> {
        let scope = brief.map_or_else(|| vec![(0, world.duration_ms.max(1))], |b| b.scope(world));
        let bigger = brief.is_some_and(Brief::wants_bigger_choruses);
        let mut problems = Vec::new();
        let mut small_choruses = BTreeSet::new();
        for (name, span) in world.occurrences() {
            if !inside(&scope, span) {
                continue;
            }
            let at = format!("\"{name}\" ({}–{} ms)", span.0, span.1);
            let Some(section) = self.section_at(span) else {
                problems.push(format!(
                    "sections: nothing says what {at} is: describe it (its props, its look), or say in keep why it stays as it is."
                ));
                continue;
            };
            if !section.plays() {
                continue;
            }
            if !self.steps.iter().any(|s| s.builds(span)) {
                problems.push(format!(
                    "steps: {at} has {} playing in the plan, but no step builds it: give it to a step (by its label, or its label's root).",
                    section.props.join(", ")
                ));
            }
            let all = world.all_props().len();
            if bigger
                && crate::song::label_root(&name).eq_ignore_ascii_case("chorus")
                && section.lights.len() * BIG_SHARE.1 < all * BIG_SHARE.0
            {
                small_choruses.insert(section.label.clone());
            }
        }
        let all = world.all_props().len();
        for label in small_choruses {
            problems.push(format!(
                "sections ({label}): the brief asks for bigger choruses: give them most of the house, at least {} of the {all} props.",
                (all * BIG_SHARE.0).div_ceil(BIG_SHARE.1)
            ));
        }
        problems
    }

    /// What the plan says of the part at `span`: the most particular section that covers it
    /// (a part by its own name before every part of its label, before the whole song).
    pub fn section_at(&self, span: (u64, u64)) -> Option<&SectionPlan> {
        self.sections
            .iter()
            .filter(|s| s.spans.iter().any(|&own| inside(&[own], span)))
            .min_by_key(|s| s.breadth())
    }

    /// The steps in waves, by index: a step comes after every step it names and every earlier
    /// step it overlaps (the same props at the same time). In each wave the steps keep the
    /// plan's order.
    pub fn waves(&self) -> Vec<Vec<usize>> {
        let mut wave_of: Vec<usize> = Vec::with_capacity(self.steps.len());
        for (i, step) in self.steps.iter().enumerate() {
            let wave = (0..i)
                .filter(|&j| step.after.contains(&self.steps[j].id) || step.overlaps(&self.steps[j]))
                .map(|j| wave_of[j] + 1)
                .max()
                .unwrap_or(0);
            wave_of.push(wave);
        }
        let mut waves: Vec<Vec<usize>> = vec![Vec::new(); wave_of.iter().max().map_or(0, |w| w + 1)];
        for (i, wave) in wave_of.into_iter().enumerate() {
            waves[wave].push(i);
        }
        waves
    }

    /// The plan in brief, for each step and the review: what keeps the steps one show.
    pub fn outline(&self) -> Value {
        let mut outline = json!({
            "summary": self.summary,
            "sections": self.sections.iter().map(|s| {
                let mut section = json!({ "label": s.label, "palette": s.palette, "props": s.props });
                if let Some(level) = s.level {
                    section["level"] = json!(level);
                }
                for (key, value) in [("motion", &s.motion), ("look", &s.look), ("keep", &s.keep)] {
                    if !value.is_empty() {
                        section[key] = json!(value);
                    }
                }
                section
            }).collect::<Vec<_>>(),
            "roles": self.roles.iter().map(|(prop, role)| json!([prop, role])).collect::<Vec<_>>(),
            "steps": self.steps.iter().map(|s| json!([s.id, s.title, s.label()])).collect::<Vec<_>>(),
        });
        if !self.moments.is_empty() {
            outline["moments"] = json!(
                self.moments
                    .iter()
                    .map(|m| json!([m.at, m.until, m.cue, m.props]))
                    .collect::<Vec<_>>()
            );
        }
        for (key, value) in [("momentsNote", &self.moments_note), ("vocals", &self.vocals)] {
            if !value.is_empty() {
                outline[key] = json!(value);
            }
        }
        outline
    }

    /// What the person is told the show is: the plan's summary, or when it gave none, each part
    /// of the song in a few words, from the plan itself.
    pub fn reply(&self) -> String {
        if !self.summary.is_empty() {
            return self.summary.clone();
        }
        let parts: Vec<String> = self
            .sections
            .iter()
            .filter(|s| s.keep.is_empty())
            .map(|s| {
                let mut words = vec![s.label.clone()];
                if s.props.is_empty() {
                    words.push("dark".into());
                } else {
                    words.push(s.props.join(", "));
                }
                if !s.motion.is_empty() {
                    words.push(s.motion.clone());
                }
                if !s.look.is_empty() {
                    words.push(s.look.clone());
                }
                words.join(": ")
            })
            .collect();
        if parts.is_empty() {
            "The show is planned and built.".to_string()
        } else {
            format!("Planned and built, part by part. {}.", parts.join(". "))
        }
    }
}

/// The plan's moments: a list of cues (each checked), or words alone (a plan in the longer
/// form), or both (`note` beside the list).
fn read_moments(value: &Value, world: &World, problems: &mut Vec<String>) -> (Vec<Moment>, String) {
    match value {
        Value::String(note) => (Vec::new(), cut(note.trim().to_string(), MAX_TEXT)),
        Value::Array(items) => {
            let mut moments = Vec::new();
            for (i, m) in items.iter().take(MAX_MOMENTS).enumerate() {
                let cue = text(m, "cue");
                if !crate::cues::CUES.contains(&cue.as_str()) {
                    problems.push(format!(
                        "moments[{i}]: there's no cue \"{cue}\". Cues: {}.",
                        crate::cues::CUES.join(", ")
                    ));
                }
                let Some(at) = m["at"].as_u64() else {
                    problems.push(format!("moments[{i}]: give at, in ms."));
                    continue;
                };
                let props = names(m, "props");
                let mut lights = BTreeSet::new();
                for name in &props {
                    match world.props(name) {
                        Some(found) => lights.extend(found),
                        None => problems.push(format!("moments[{i}]: there's no prop or group \"{name}\".")),
                    }
                }
                moments.push(Moment {
                    at,
                    until: m["until"].as_u64().filter(|u| *u > at),
                    cue,
                    props,
                    lights,
                });
            }
            (moments, String::new())
        }
        Value::Object(_) => {
            let (moments, _) = read_moments(&value["cues"], world, problems);
            (moments, cut(text(value, "note"), MAX_TEXT))
        }
        _ => (Vec::new(), String::new()),
    }
}

/// How many requests the longest-waiting worker makes when `loads` (each step's requests, in
/// plan order) are shared among `workers`, each step going to the first worker free: what a
/// person waits through for one wave.
pub fn longest_worker(loads: &[usize], workers: usize) -> usize {
    let mut busy = vec![0usize; workers.max(1)];
    for load in loads {
        if let Some(least) = busy.iter_mut().min() {
            *least += load;
        }
    }
    busy.into_iter().max().unwrap_or(0)
}
