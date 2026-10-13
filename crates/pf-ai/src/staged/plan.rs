//! The brief and the plan of a staged build, read from the model's calls and checked against
//! the show, the song, the effects, and the tools before anything is built: a part of the song,
//! a prop, an effect, or a tool that isn't there is sent back, named, for the plan to be fixed.
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
pub const MAX_PLAN_STEPS: usize = 10;
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
    /// Props and groups by lowercase name: the props each lights.
    targets: BTreeMap<String, (String, BTreeSet<PropId>)>,
    all_props: BTreeSet<PropId>,
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
        Self {
            parts,
            duration_ms,
            targets,
            all_props: show.props.iter().map(|p| p.id).collect(),
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
}

/// One part of the song as the plan has it.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionPlan {
    pub label: String,
    pub palette: Vec<String>,
    pub props: Vec<String>,
    pub base: String,
    pub movement: String,
    pub accents: String,
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
}

/// The design: what the show is, part by part, and the steps that build it.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub summary: String,
    pub sections: Vec<SectionPlan>,
    pub roles: Vec<(String, String)>,
    pub moments: String,
    pub vocals: String,
    pub steps: Vec<Step>,
}

fn is_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Plan {
    /// The plan in `input` (`submit_plan`'s), checked against `world`: every part, prop, effect,
    /// tool, guide, and step it names must be there. Every problem is listed at once.
    pub fn read(input: &Value, world: &World) -> Result<Self, Vec<String>> {
        let mut problems = Vec::new();
        let summary = text(input, "summary");
        if summary.is_empty() {
            problems.push("summary: say in a sentence or two what the show will be.".to_string());
        }
        let mut sections = Vec::new();
        for (i, s) in input["sections"].as_array().into_iter().flatten().enumerate() {
            let label = text(s, "label");
            if world.spans(&label).is_none() {
                problems.push(format!(
                    "sections[{i}]: there's no part called \"{label}\". Parts: {}.",
                    world.part_names()
                ));
            }
            let palette = names(s, "palette");
            if let Some(bad) = palette.iter().find(|c| !is_color(c)) {
                problems.push(format!(
                    "sections[{i}] ({label}): \"{bad}\" isn't a color: write \"#rrggbb\"."
                ));
            }
            let props = names(s, "props");
            for name in &props {
                if world.props(name).is_none() {
                    problems.push(format!(
                        "sections[{i}] ({label}): there's no prop or group \"{name}\"."
                    ));
                }
            }
            sections.push(SectionPlan {
                label,
                palette,
                props,
                base: text(s, "base"),
                movement: text(s, "movement"),
                accents: text(s, "accents"),
            });
        }
        let mut roles = Vec::new();
        for r in input["roles"].as_array().into_iter().flatten() {
            let prop = text(r, "prop");
            if world.props(&prop).is_none() {
                problems.push(format!("roles: there's no prop or group \"{prop}\"."));
            }
            roles.push((prop, text(r, "role")));
        }
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
            let effects = names(s, "effects");
            for effect in &effects {
                if reference::kind_named(effect).is_none() {
                    problems.push(format!(
                        "{at}: there's no effect \"{effect}\". Effects: {}.",
                        reference::kind_keys().join(", ")
                    ));
                }
            }
            let step_topics = names(s, "topics");
            for topic in &step_topics {
                if !topics.contains(&topic.as_str()) {
                    problems.push(format!(
                        "{at}: there's no guide topic \"{topic}\". Topics: {}.",
                        topics.join(", ")
                    ));
                }
            }
            let after = names(s, "after");
            for other in &after {
                if !steps.iter().any(|earlier| earlier.id == *other) {
                    problems.push(format!(
                        "{at}: after names \"{other}\", which isn't an earlier step."
                    ));
                }
            }
            let todo = text(s, "do");
            if todo.is_empty() {
                problems.push(format!("{at}: say in \"do\" what the step builds."));
            }
            steps.push(Step {
                id,
                title: text(s, "title"),
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
        if problems.is_empty() {
            Ok(Self {
                summary,
                sections,
                roles,
                moments: text(input, "moments"),
                vocals: text(input, "vocals"),
                steps,
            })
        } else {
            Err(problems)
        }
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
        json!({
            "summary": self.summary,
            "sections": self.sections.iter().map(|s| json!({
                "label": s.label,
                "palette": s.palette,
                "props": s.props,
                "base": s.base,
                "movement": s.movement,
                "accents": s.accents,
            })).collect::<Vec<_>>(),
            "roles": self.roles.iter().map(|(prop, role)| json!([prop, role])).collect::<Vec<_>>(),
            "moments": self.moments,
            "vocals": self.vocals,
            "steps": self.steps.iter().map(|s| json!([s.id, s.title, s.label()])).collect::<Vec<_>>(),
        })
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
