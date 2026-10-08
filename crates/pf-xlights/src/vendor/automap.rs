//! Suggesting where each vendor item's effects go in the user's show.
//!
//! In order: a mapping saved last time, the same name, an xLights alias, a similar name ("Mega
//! Tree" and "MegaTree 1"), the same kind of prop (a tree for a tree, a group of lines for a
//! group of lines, the whole house for the whole house), then a similar number of lights. Every
//! suggestion has a confidence; only those of at least [`AUTO_MAP_CONFIDENCE`] are applied.
//! Suggestions from similar names, kinds, and sizes give each of the show's props to one item
//! only (the most confident, then the one with the most effects), so five vendor trees don't
//! all pile onto one tree.

use super::{ItemKind, MatchReason, PropType, ShowTarget, Suggestion, VendorItem};
use super::{Mapping, VendorLayout};
use pf_model::{Generator, GroupMember, ShapeSource, Show};
use std::collections::{HashMap, HashSet};

/// The least confidence a suggestion is applied with; less is shown as unmapped.
pub const AUTO_MAP_CONFIDENCE: f32 = 0.5;

/// A group holding at least this share of the props (and at least [`WHOLE_MIN_MEMBERS`]) is the
/// whole house.
const WHOLE_SHARE: f32 = 0.6;
const WHOLE_MIN_MEMBERS: usize = 4;

/// Words in group names that say nothing about what's in them.
const GROUP_WORDS: [&str; 4] = ["group", "groups", "grp", "grps"];

/// A prop type from xLights' `DisplayAs`, `None` when it doesn't say (custom models, DMX, ...).
pub(crate) fn type_of_display_as(display_as: &str, pixels: u32) -> Option<PropType> {
    let d = display_as.trim().to_ascii_lowercase();
    Some(match d.as_str() {
        _ if d.starts_with("tree") => PropType::Tree,
        "arches" => PropType::Arch,
        "candy canes" => PropType::Canes,
        "window frame" => PropType::Window,
        "single line" | "poly line" | "multipoint" if pixels > 0 && pixels <= 3 => PropType::Flood,
        "single line" | "poly line" | "multipoint" => PropType::Line,
        _ if d.contains("matrix") => PropType::Matrix,
        "star" => PropType::Star,
        "circle" => PropType::Circle,
        "wreath" => PropType::Wreath,
        "spinner" => PropType::Spinner,
        "sphere" => PropType::Sphere,
        "cube" => PropType::Cube,
        "icicles" => PropType::Icicles,
        _ => return None,
    })
}

/// A prop type from words in a name, `None` when nothing in it says.
pub(crate) fn type_of_name(name: &str) -> Option<PropType> {
    let words = words(name);
    let has = |w: &str| words.iter().any(|x| x == w || x.starts_with(w) || x.ends_with(w));
    Some(if has("tree") {
        PropType::Tree
    } else if has("arch") {
        PropType::Arch
    } else if has("cane") {
        PropType::Canes
    } else if has("window") {
        PropType::Window
    } else if has("matrix") || words.iter().any(|w| w == "p10" || w == "p5" || w == "panel") {
        PropType::Matrix
    } else if has("flood") {
        PropType::Flood
    } else if has("icicle") {
        PropType::Icicles
    } else if has("star") {
        PropType::Star
    } else if has("wreath") {
        PropType::Wreath
    } else if has("spinner") {
        PropType::Spinner
    } else if has("flake") {
        PropType::Snowflake
    } else if [
        "outline", "roof", "eave", "gutter", "ridge", "line", "fascia", "peak",
    ]
    .iter()
    .any(|w| has(w))
    {
        PropType::Line
    } else {
        return None;
    })
}

/// A prop's type from its shape, else its name.
fn type_of_shape(shape: &ShapeSource, name: &str, pixels: u32) -> PropType {
    let from_shape = match shape {
        ShapeSource::Generator(g) => match g {
            Generator::Line { .. } | Generator::PolyLine { .. } if pixels <= 3 => Some(PropType::Flood),
            Generator::Line { .. } | Generator::PolyLine { .. } => Some(PropType::Line),
            Generator::Arch { .. } => Some(PropType::Arch),
            Generator::Circle { .. } => Some(PropType::Circle),
            Generator::Matrix { .. } => Some(PropType::Matrix),
            Generator::Tree { .. } => Some(PropType::Tree),
            Generator::Star { .. } => Some(PropType::Star),
            Generator::CandyCanes { .. } => Some(PropType::Canes),
            Generator::WindowFrame { .. } => Some(PropType::Window),
            Generator::Icicles { .. } => Some(PropType::Icicles),
            Generator::Wreath { .. } => Some(PropType::Wreath),
            Generator::Spinner { .. } => Some(PropType::Spinner),
            Generator::Sphere { .. } => Some(PropType::Sphere),
            Generator::Cube { .. } => Some(PropType::Cube),
            _ => None,
        },
        ShapeSource::Measured { .. } => None,
    };
    from_shape
        .or_else(|| type_of_name(name))
        .unwrap_or(PropType::Other)
}

/// The words of a name, lowercased, letters and numbers apart ("MegaTree12" is "megatree",
/// "12"), plurals made singular.
fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut digits = false;
    let flush = |word: &mut String, out: &mut Vec<String>| {
        if !word.is_empty() {
            let w = std::mem::take(word);
            let w = match w.strip_suffix('s') {
                Some(stem) if stem.len() >= 3 && !w.ends_with("ss") => stem.to_string(),
                _ => w,
            };
            out.push(w);
        }
    };
    for c in name.chars() {
        if c.is_alphanumeric() {
            let d = c.is_ascii_digit();
            if !word.is_empty() && d != digits {
                flush(&mut word, &mut out);
            }
            digits = d;
            word.extend(c.to_lowercase());
        } else {
            flush(&mut word, &mut out);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// What the name matching looks at.
#[derive(Debug, Clone, Default)]
struct Name {
    /// The words, without numbers or group words, run together ("megatree").
    letters: String,
    words: Vec<String>,
    numbers: Vec<String>,
}

impl Name {
    fn of(name: &str) -> Self {
        let all = words(name);
        let numbers: Vec<String> = all
            .iter()
            .filter(|w| w.chars().all(|c| c.is_ascii_digit()))
            .cloned()
            .collect();
        let words: Vec<String> = all
            .into_iter()
            .filter(|w| !w.chars().all(|c| c.is_ascii_digit()) && !GROUP_WORDS.contains(&w.as_str()))
            .collect();
        Self {
            letters: words.concat(),
            words,
            numbers,
        }
    }
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

/// How alike two names are, from 0 to 1. The same words (ignoring spaces, case, punctuation,
/// plurals, and "group") with the same or no numbers are 0.9; different numbers, 0.7.
fn name_similarity(a: &Name, b: &Name) -> f32 {
    if a.letters.is_empty() || b.letters.is_empty() {
        return 0.0;
    }
    if a.letters == b.letters {
        return if a.numbers == b.numbers {
            1.0
        } else if a.numbers.is_empty() || b.numbers.is_empty() {
            0.9
        } else {
            0.7
        };
    }
    let (x, y) = (bigrams(&a.letters), bigrams(&b.letters));
    let dice = if x.is_empty() || y.is_empty() {
        0.0
    } else {
        let mut pool = y.clone();
        let mut shared = 0;
        for g in &x {
            if let Some(at) = pool.iter().position(|h| h == g) {
                pool.swap_remove(at);
                shared += 1;
            }
        }
        2.0 * shared as f32 / (x.len() + y.len()) as f32
    };
    let wa: HashSet<&String> = a.words.iter().collect();
    let wb: HashSet<&String> = b.words.iter().collect();
    let jaccard = wa.intersection(&wb).count() as f32 / wa.union(&wb).count().max(1) as f32;
    (dice * 0.85).max(jaccard * 0.8)
}

fn pixel_similarity(a: u32, b: u32) -> f32 {
    if a == 0 || b == 0 {
        return 0.0;
    }
    a.min(b) as f32 / a.max(b) as f32
}

/// "Whole", "All", "Everything", "Entire", or "Full House" in a group's name.
fn whole_by_name(name: &str) -> bool {
    const GENERAL: [&str; 13] = [
        "all",
        "house",
        "prop",
        "model",
        "effect",
        "element",
        "light",
        "display",
        "show",
        "yard",
        "home",
        "the",
        "everything",
    ];
    let w = words(name);
    let has = |x: &str| w.iter().any(|y| y == x);
    // "All" only when the rest says nothing more ("All Effects", not "Matrix All").
    let only_general = w.iter().all(|x| {
        GENERAL.contains(&x.as_str())
            || GROUP_WORDS.contains(&x.as_str())
            || x.chars().all(|c| c.is_ascii_digit())
    });
    has("whole")
        || has("everything")
        || has("entire")
        || (has("full") && has("house"))
        || (has("all") && only_general)
}

/// One side of a comparison: a vendor item or one of the show's targets.
#[derive(Debug, Clone)]
struct Thing {
    parsed: Name,
    group: bool,
    ptype: PropType,
    pixels: u32,
    whole: bool,
}

impl Thing {
    /// The type compared: the whole house is its own type.
    fn kind(&self) -> Option<PropType> {
        if self.whole {
            return Some(PropType::Other);
        }
        (self.ptype != PropType::Other).then_some(self.ptype)
    }
}

/// The show's props, groups, and submodels, as the mapping can name them.
pub(crate) fn show_targets(show: &Show) -> Vec<ShowTarget> {
    let mut out = Vec::new();
    let pixels_of: HashMap<_, u32> = show.props.iter().map(|p| (p.id, p.node_count())).collect();
    let type_of: HashMap<_, PropType> = show
        .props
        .iter()
        .map(|p| (p.id, type_of_shape(&p.shape, &p.name, p.node_count())))
        .collect();
    for group in &show.groups {
        let props: HashSet<_> = group.members.iter().map(GroupMember::prop).collect();
        let mut counts: HashMap<PropType, usize> = HashMap::new();
        for id in &props {
            if let Some(t) = type_of.get(id) {
                *counts.entry(*t).or_default() += 1;
            }
        }
        out.push(ShowTarget {
            name: group.name.clone(),
            label: group.name.clone(),
            parent: None,
            kind: ItemKind::Group,
            ptype: dominant(&counts),
            pixels: props.iter().filter_map(|id| pixels_of.get(id)).sum(),
            whole: whole_by_name(&group.name)
                || (props.len() >= WHOLE_MIN_MEMBERS
                    && props.len() as f32 >= WHOLE_SHARE * show.props.len() as f32),
        });
    }
    for prop in &show.props {
        let ptype = type_of[&prop.id];
        out.push(ShowTarget {
            name: prop.name.clone(),
            label: prop.name.clone(),
            parent: None,
            kind: ItemKind::Model,
            ptype,
            pixels: prop.node_count(),
            whole: false,
        });
        for region in prop.regions.iter().filter(|r| r.is_submodel()) {
            out.push(ShowTarget {
                name: format!("{}/{}", prop.name, region.name),
                label: region.name.clone(),
                parent: Some(prop.name.clone()),
                kind: ItemKind::Submodel,
                ptype,
                pixels: 0,
                whole: false,
            });
        }
    }
    out
}

/// The most common type (ties: the first in [`PropType`] order), `Other` when there's none.
fn dominant(counts: &HashMap<PropType, usize>) -> PropType {
    counts
        .iter()
        .filter(|(t, _)| **t != PropType::Other)
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map_or(PropType::Other, |(t, _)| *t)
}

/// Type, size, and whether it's the whole house, for each vendor item named in the sequence,
/// from the vendor's layout (or, without one, from the names).
pub(crate) fn describe_items(items: &mut [VendorItem], layout: Option<&VendorLayout>) {
    for item in items.iter_mut() {
        let fallback = type_of_name(&item.label);
        match (layout, item.kind) {
            (Some(layout), ItemKind::Model | ItemKind::Group) => {
                if let Some(model) = layout.models.get(&item.name) {
                    item.kind = ItemKind::Model;
                    item.display_as = Some(model.display_as.clone());
                    item.pixels = model.pixels;
                    item.ptype = type_of_display_as(&model.display_as, model.pixels)
                        .or(fallback)
                        .unwrap_or(PropType::Other);
                    item.aliases = model.aliases.clone();
                } else if let Some(group) = layout.groups.get(&item.name) {
                    item.kind = ItemKind::Group;
                    item.display_as = Some("ModelGroup".to_string());
                    item.pixels = group.pixels;
                    item.ptype = group.ptype;
                    item.whole = group.whole || whole_by_name(&item.name);
                    item.aliases = group.aliases.clone();
                } else {
                    item.ptype = fallback.unwrap_or(PropType::Other);
                }
            }
            _ => {
                item.ptype = fallback
                    .or_else(|| {
                        let parent = item.parent.as_ref()?;
                        layout?
                            .models
                            .get(parent)
                            .map(|m| type_of_display_as(&m.display_as, m.pixels).unwrap_or(PropType::Other))
                    })
                    .unwrap_or(PropType::Other);
                if let (Some(layout), Some(parent)) = (layout, &item.parent)
                    && let Some(model) = layout.models.get(parent)
                {
                    item.aliases = model
                        .submodel_aliases
                        .get(&item.label)
                        .cloned()
                        .unwrap_or_default();
                }
            }
        }
    }
}

/// An alias as compared: xLights keeps renamed models' old names as "oldname:<name>".
fn alias_name(alias: &str) -> &str {
    alias.strip_prefix("oldname:").unwrap_or(alias).trim()
}

/// The best suggestion for each item (in `items` order) and the mapping applied from them.
/// `saved` is a mapping saved for this vendor before.
pub(crate) fn suggest(
    items: &[VendorItem],
    targets: &[ShowTarget],
    saved: Option<&Mapping>,
) -> (Vec<Suggestion>, Mapping) {
    let by_name: HashMap<&str, &ShowTarget> = targets.iter().map(|t| (t.name.as_str(), t)).collect();
    let by_lower: HashMap<String, &ShowTarget> =
        targets.iter().rev().map(|t| (t.name.to_lowercase(), t)).collect();
    let mut best: Vec<Option<Suggestion>> = vec![None; items.len()];
    let mut claimed: HashSet<&str> = HashSet::new();

    // Saved, exact, and alias matches.
    for (i, item) in items.iter().enumerate() {
        if let Some(wanted) = saved.and_then(|s| s.targets(&item.name)) {
            let kept: Vec<String> = wanted
                .iter()
                .filter(|t| by_name.contains_key(t.as_str()))
                .cloned()
                .collect();
            if wanted.is_empty() || !kept.is_empty() {
                best[i] = Some(Suggestion::new(item, kept, 1.0, MatchReason::Saved));
                continue;
            }
        }
        let exact = by_name
            .get(item.name.as_str())
            .map(|t| (t, 1.0))
            .or_else(|| by_lower.get(&item.name.trim().to_lowercase()).map(|t| (t, 0.95)));
        if let Some((target, confidence)) = exact {
            best[i] = Some(Suggestion::new(
                item,
                vec![target.name.clone()],
                confidence,
                MatchReason::Exact,
            ));
            continue;
        }
        let alias = item
            .aliases
            .iter()
            .find_map(|a| by_lower.get(&alias_name(a).to_lowercase()));
        if let Some(target) = alias {
            best[i] = Some(Suggestion::new(
                item,
                vec![target.name.clone()],
                0.9,
                MatchReason::Alias,
            ));
        }
    }
    for s in best.iter().flatten() {
        claimed.extend(
            s.targets
                .iter()
                .filter_map(|t| by_name.get(t.as_str()).map(|t| t.name.as_str())),
        );
    }

    // Similar names, types, and sizes, for the models and groups not matched yet.
    let target_things: Vec<(&ShowTarget, Thing)> = targets
        .iter()
        .filter(|t| t.kind != ItemKind::Submodel)
        .map(|t| {
            (
                t,
                Thing {
                    parsed: Name::of(&t.name),
                    group: t.kind == ItemKind::Group,
                    ptype: t.ptype,
                    pixels: t.pixels,
                    whole: t.whole,
                },
            )
        })
        .collect();
    // How many targets have each type (props and groups apart): a type with one is a strong hint.
    let mut of_type: HashMap<(bool, Option<PropType>), usize> = HashMap::new();
    for (_, t) in &target_things {
        *of_type.entry((t.group, t.kind())).or_default() += 1;
    }
    let mut candidates: Vec<(f32, usize, usize, MatchReason)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if best[i].is_some() || !matches!(item.kind, ItemKind::Model | ItemKind::Group) {
            continue;
        }
        let v = Thing {
            parsed: Name::of(&item.name),
            group: item.kind == ItemKind::Group,
            ptype: item.ptype,
            pixels: item.pixels,
            whole: item.whole,
        };
        for (j, (_, t)) in target_things.iter().enumerate() {
            if let Some((confidence, reason)) = score(&v, t, of_type.get(&(t.group, t.kind())) == Some(&1)) {
                candidates.push((confidence, i, j, reason));
            }
        }
    }
    // Most confident first; within a few hundredths, the item with more effects (it matters more
    // that the vendor's busiest tree lands on the tree than a little one does).
    let bucket = |c: f32| (c * 20.0).floor() as i32;
    candidates.sort_by(|a, b| {
        bucket(b.0)
            .cmp(&bucket(a.0))
            .then(items[b.1].effects.cmp(&items[a.1].effects))
            .then(b.0.total_cmp(&a.0))
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut hints: Vec<Option<(f32, usize, MatchReason)>> = vec![None; items.len()];
    for &(confidence, i, j, reason) in &candidates {
        if best[i].is_some() {
            continue;
        }
        let target = target_things[j].0;
        if confidence >= AUTO_MAP_CONFIDENCE && !claimed.contains(target.name.as_str()) {
            claimed.insert(&target.name);
            best[i] = Some(Suggestion::new(
                &items[i],
                vec![target.name.clone()],
                confidence,
                reason,
            ));
        } else if hints[i].is_none() {
            // The best idea for an item left unmapped, shown as a hint (below the bar, or its
            // target went to a closer match).
            hints[i] = Some((confidence.min(AUTO_MAP_CONFIDENCE - 0.01), j, reason));
        }
    }

    // Submodels: the submodel of the same (or a similar) name on the prop their model went to.
    for (i, item) in items.iter().enumerate() {
        if best[i].is_some() || item.kind != ItemKind::Submodel {
            continue;
        }
        let Some(parent) = &item.parent else { continue };
        let Some(parent_at) = items.iter().position(|p| &p.name == parent) else {
            continue;
        };
        let Some(prop) = best[parent_at].as_ref().and_then(|s| s.targets.first()).cloned() else {
            continue;
        };
        let wanted = Name::of(&item.label);
        let found = targets
            .iter()
            .filter(|t| t.kind == ItemKind::Submodel && t.parent.as_deref() == Some(prop.as_str()))
            .map(|t| (name_similarity(&wanted, &Name::of(&t.label)), t))
            .filter(|(s, _)| *s >= 0.8)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((similarity, target)) = found {
            let confidence = 0.6 + 0.3 * (similarity - 0.8) / 0.2;
            best[i] = Some(Suggestion::new(
                item,
                vec![target.name.clone()],
                confidence,
                MatchReason::Name,
            ));
        }
    }

    let mut mapping = Mapping::default();
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        match (&best[i], hints[i]) {
            (Some(s), _) => {
                if s.reason == MatchReason::Saved || s.confidence >= AUTO_MAP_CONFIDENCE {
                    mapping.items.insert(item.name.clone(), s.targets.clone());
                }
                out.push(s.clone());
            }
            (None, Some((confidence, j, reason))) => out.push(Suggestion::new(
                item,
                vec![target_things[j].0.name.clone()],
                confidence,
                reason,
            )),
            (None, None) => out.push(Suggestion::new(item, Vec::new(), 0.0, MatchReason::None)),
        }
    }
    (out, mapping)
}

/// How confident a match of vendor item `v` to target `t` is, and why; `None` when they have
/// nothing in common. `only_of_type`: `t` is the show's only target of its type.
fn score(v: &Thing, t: &Thing, only_of_type: bool) -> Option<(f32, MatchReason)> {
    let names = name_similarity(&v.parsed, &t.parsed);
    let (vk, tk) = (v.kind(), t.kind());
    let known = vk.is_some() && tk.is_some();
    let same_type = known && vk == tk;
    let same_side = v.group == t.group;
    let pixels = pixel_similarity(v.pixels, t.pixels);
    if names >= 0.8 {
        let mut confidence = (0.6 + 0.3 * (names - 0.8) / 0.2).min(0.88);
        if known && !same_type {
            confidence -= 0.2;
        }
        if !same_side {
            confidence -= 0.1;
        }
        return Some((confidence, MatchReason::Name));
    }
    if same_type {
        let confidence = 0.42
            + 0.15 * names
            + 0.2 * pixels
            + if same_side { 0.05 } else { -0.12 }
            + if only_of_type { 0.1 } else { 0.0 };
        return Some((confidence.min(0.79), MatchReason::Type));
    }
    // Nothing else to go on: only a near-identical number of lights is applied.
    if !known && same_side && pixels >= 0.85 {
        return Some((0.2 + 0.32 * pixels, MatchReason::Size));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_compare_by_their_words() {
        let sim = |a: &str, b: &str| name_similarity(&Name::of(a), &Name::of(b));
        assert_eq!(sim("Mega Tree", "MegaTree"), 1.0);
        assert_eq!(sim("Mega Tree", "MegaTree 1"), 0.9);
        assert_eq!(sim("Group - Windows", "Window"), 1.0);
        assert_eq!(sim("Candy Canes", "Candy Canes 1-12"), 0.9);
        assert_eq!(sim("Arch 1", "Arch 2"), 0.7);
        assert!(sim("Roof Line", "Roofline Left") < 0.8);
        assert!(sim("Snowflake", "Tree") < 0.3);
        assert_eq!(words("MegaTree12 (Left)"), vec!["megatree", "12", "left"]);
    }

    #[test]
    fn types_come_from_display_as_shapes_and_names() {
        assert_eq!(type_of_display_as("Tree 360", 800), Some(PropType::Tree));
        assert_eq!(type_of_display_as("Poly Line", 120), Some(PropType::Line));
        assert_eq!(type_of_display_as("Single Line", 1), Some(PropType::Flood));
        assert_eq!(type_of_display_as("Vert Matrix", 600), Some(PropType::Matrix));
        assert_eq!(type_of_display_as("Custom", 600), None);
        assert_eq!(type_of_name("GE Flake A 1"), Some(PropType::Snowflake));
        assert_eq!(type_of_name("Roofline Left"), Some(PropType::Line));
        assert_eq!(type_of_name("Pixel Pole"), None);
    }

    #[test]
    fn whole_house_groups_are_named_for_everything() {
        for name in [
            "Whole House",
            "All Effects",
            "Group - All",
            "Everything",
            "Full house without trees",
            "ALL PROPS GRP",
        ] {
            assert!(whole_by_name(name), "{name}");
        }
        for name in ["Matrix All", "Group - Lights", "House Outline", "Windows"] {
            assert!(!whole_by_name(name), "{name}");
        }
    }
}
