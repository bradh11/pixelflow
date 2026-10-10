//! Bringing a purchased (vendor) xLights sequence, made for the vendor's props, onto the user's
//! own props, as xLights' Import Effects does with its model mapping.
//!
//! [`inspect`] reads a package (see [`Package`]): the sequences in it, each vendor model, group,
//! submodel, and strand that has effects, the user's props they could go to, and a suggested
//! mapping (see `automap`). [`import`] then builds the sequence with the mapping the user
//! settled on: effects are re-targeted to the user's props and groups and render on their
//! buffers; timing tracks and lyrics come across as with any import.

mod automap;
mod package;
mod xmap;

pub use automap::AUTO_MAP_CONFIDENCE;
pub use package::{Limits, MUSIC_EXTENSIONS, Package, PackageKind};
pub use xmap::{MAX_XMAP_BYTES, XmapRead, read_xmap, write_xmap};

use crate::XlightsError;
use crate::layout::parse_layout;
use crate::sequence::{
    ElementKind, SequenceImport, XsqFile, build_sequence_mapped, find_music, name_from_path, unxml_safe,
};
use pf_model::Show;
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where each sequence item's effects go: the item's name ("Model", "Model/Submodel", "Model/
/// Strand 1") to the names of the show's props, groups, and submodels ("Prop/Submodel"). An
/// item mapped to several targets is copied to each; one mapped to nothing is skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub items: BTreeMap<String, Vec<String>>,
}

impl Mapping {
    /// Where `item` goes, when the mapping says (an empty list: nowhere).
    pub fn targets(&self, item: &str) -> Option<&[String]> {
        self.items.get(item).map(Vec::as_slice)
    }

    /// Adds `target` to `item`'s targets (once).
    pub fn add(&mut self, item: &str, target: &str) {
        let targets = self.items.entry(item.to_string()).or_default();
        if !targets.iter().any(|t| t == target) {
            targets.push(target.to_string());
        }
    }
}

/// What a vendor item or a target is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    /// A model (in the vendor's sequence) or a prop (in the show).
    Model,
    Group,
    Submodel,
    Strand,
}

/// The kind of prop, for matching like with like and for the icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PropType {
    Tree,
    Arch,
    Matrix,
    Canes,
    Line,
    Window,
    Star,
    Circle,
    Wreath,
    Spinner,
    Sphere,
    Cube,
    Icicles,
    Snowflake,
    Flood,
    Other,
}

/// Why a suggestion was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchReason {
    /// Mapped this way last time.
    Saved,
    /// The same name.
    Exact,
    /// One of the vendor model's xLights aliases is the prop's name.
    Alias,
    /// A similar name.
    Name,
    /// The same kind of prop.
    Type,
    /// A similar number of lights.
    Size,
    /// Nothing alike.
    None,
}

/// A vendor model, group, submodel, or strand with effects in the sequence (or whose
/// submodels or strands have some).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorItem {
    /// As the mapping names it: "Model", "Model/Submodel", "Model/Strand 1".
    pub name: String,
    /// As shown: the model's, submodel's, or strand's own name.
    pub label: String,
    /// The model of a submodel or strand.
    pub parent: Option<String>,
    pub kind: ItemKind,
    #[serde(rename = "type")]
    pub ptype: PropType,
    /// The vendor layout's `DisplayAs`, when there's a layout.
    pub display_as: Option<String>,
    /// Effects on the item itself (not its submodels or strands).
    pub effects: usize,
    /// Lights, when the vendor's layout says (0 when unknown).
    pub pixels: u32,
    #[serde(skip)]
    pub aliases: Vec<String>,
    #[serde(skip)]
    pub whole: bool,
}

/// A prop, group, or submodel in the user's show that effects can go to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowTarget {
    /// As the mapping names it: "Prop", "Group", "Prop/Submodel".
    pub name: String,
    pub label: String,
    pub parent: Option<String>,
    pub kind: ItemKind,
    #[serde(rename = "type")]
    pub ptype: PropType,
    /// Lights (a group's props' together; 0 for a submodel).
    pub pixels: u32,
    #[serde(skip)]
    pub whole: bool,
}

/// The best idea for one vendor item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub item: String,
    /// Empty when nothing fits.
    pub targets: Vec<String>,
    /// 0 to 1; applied from [`AUTO_MAP_CONFIDENCE`].
    pub confidence: f32,
    pub reason: MatchReason,
}

impl Suggestion {
    fn new(item: &VendorItem, targets: Vec<String>, confidence: f32, reason: MatchReason) -> Self {
        Self {
            item: item.name.clone(),
            targets,
            confidence: (confidence * 100.0).round() / 100.0,
            reason,
        }
    }
}

/// What's in a package, and a suggested mapping onto the show.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    /// The sequences in the package (paths inside it), best first.
    pub sequences: Vec<String>,
    /// The one inspected.
    pub sequence: String,
    /// Its song title (or file name).
    pub song: String,
    /// Whether the vendor's layout came with it (types and sizes are guessed from names
    /// without it).
    pub has_layout: bool,
    pub items: Vec<VendorItem>,
    pub targets: Vec<ShowTarget>,
    /// One per item, in `items` order.
    pub suggestions: Vec<Suggestion>,
    /// The suggestions applied (those confident enough, and saved ones).
    pub mapping: Mapping,
    /// What the mapping is saved under: the vendor's layout (so their other songs pre-fill
    /// too), or this sequence when there's no layout.
    pub key: String,
    /// Every item with effects has a prop of the same name (the user's own sequence): there's
    /// nothing to map.
    pub all_exact: bool,
    /// The music file in a zip package, copied next to the show on import.
    pub music: Option<String>,
}

/// Names to their aliases.
type Aliases = HashMap<String, Vec<String>>;

/// One model of the vendor's layout, as matching uses it.
#[derive(Debug, Clone, Default)]
pub(crate) struct VendorModel {
    pub display_as: String,
    pub pixels: u32,
    pub aliases: Vec<String>,
    /// Submodel name to its aliases.
    pub submodel_aliases: Aliases,
}

/// One group of the vendor's layout.
#[derive(Debug, Clone)]
pub(crate) struct VendorGroup {
    pub pixels: u32,
    pub ptype: PropType,
    pub whole: bool,
    pub aliases: Vec<String>,
}

/// The vendor's layout, as matching uses it.
#[derive(Debug, Clone, Default)]
pub(crate) struct VendorLayout {
    pub models: HashMap<String, VendorModel>,
    pub groups: HashMap<String, VendorGroup>,
    pub key: String,
}

/// FNV-1a: a short, stable name for a list of names.
fn fingerprint<'a>(prefix: &str, names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<String> = names.map(|n| n.trim().to_lowercase()).collect();
    names.sort();
    names.dedup();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in names.join("\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{prefix}:{hash:016x}")
}

/// The `<Aliases><alias name="…"/></Aliases>` of a model, submodel, or group.
fn aliases_of(node: Node<'_, '_>) -> Vec<String> {
    node.children()
        .filter(|c| c.is_element() && c.tag_name().name() == "Aliases")
        .flat_map(|a| a.children())
        .chain(node.children())
        .filter(|c| c.is_element() && c.tag_name().name() == "alias")
        .filter_map(|a| a.attribute("name"))
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

/// Reads the vendor's layout for matching.
pub(crate) fn vendor_layout(xml: &str) -> Result<VendorLayout, XlightsError> {
    let layout = parse_layout(xml)?;
    // Aliases, which the layout import doesn't need.
    let mut model_aliases: HashMap<String, (Vec<String>, Aliases)> = HashMap::new();
    let mut group_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let readable = crate::xml::without_bare_doctype(xml);
    if let Ok(doc) = crate::xml::parse(&readable) {
        for section in doc.root_element().children().filter(Node::is_element) {
            for node in section.children().filter(Node::is_element) {
                let name = node.attribute("name").unwrap_or("").trim().to_string();
                match (section.tag_name().name(), node.tag_name().name()) {
                    ("modelGroups", "modelGroup") => {
                        group_aliases.insert(name, aliases_of(node));
                    }
                    ("models", _) => {
                        let subs = node
                            .children()
                            .filter(|c| c.is_element() && c.tag_name().name() == "subModel")
                            .map(|s| {
                                (
                                    s.attribute("name").unwrap_or("").trim().to_string(),
                                    aliases_of(s),
                                )
                            })
                            .collect();
                        model_aliases.insert(name, (aliases_of(node), subs));
                    }
                    _ => {}
                }
            }
        }
    }
    let mut out = VendorLayout {
        key: fingerprint(
            "layout",
            layout
                .models
                .iter()
                .map(|m| m.name.as_str())
                .chain(layout.groups.iter().map(|g| g.name.as_str())),
        ),
        ..VendorLayout::default()
    };
    let mut types: HashMap<&str, PropType> = HashMap::new();
    for model in &layout.models {
        // Lights, not nodes: a "dumb" string of 50 lights is one node, but it's a line, not a
        // flood.
        let lights: usize = crate::geometry(model)
            .nodes
            .iter()
            .map(|n| n.points.len().max(1))
            .sum();
        let pixels = u32::try_from(lights).unwrap_or(u32::MAX);
        let (aliases, submodel_aliases) = model_aliases.remove(&model.name).unwrap_or_default();
        types.insert(
            &model.name,
            automap::type_of_display_as(&model.display_as, pixels)
                .or_else(|| automap::type_of_name(&model.name))
                .unwrap_or(PropType::Other),
        );
        out.models.insert(
            model.name.clone(),
            VendorModel {
                display_as: model.display_as.clone(),
                pixels,
                aliases,
                submodel_aliases,
            },
        );
    }
    let groups: HashMap<&str, &crate::XGroup> = layout.groups.iter().map(|g| (g.name.as_str(), g)).collect();
    let lit = out.models.values().filter(|m| m.pixels > 0).count();
    for group in &layout.groups {
        // Its models, through nested groups (a submodel counts as its model).
        let mut models: HashSet<&str> = HashSet::new();
        let mut queue: Vec<&str> = vec![&group.name];
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(name) = queue.pop() {
            if !seen.insert(name) || seen.len() > 10_000 {
                continue;
            }
            match groups.get(name) {
                Some(g) => queue.extend(g.members.iter().map(String::as_str)),
                None => {
                    let model = name.split('/').next().unwrap_or(name);
                    if out.models.contains_key(model) {
                        models.insert(model);
                    }
                }
            }
        }
        let mut counts: HashMap<PropType, usize> = HashMap::new();
        for m in &models {
            *counts.entry(types[m]).or_default() += 1;
        }
        let ptype = counts
            .iter()
            .filter(|(t, _)| **t != PropType::Other)
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .map_or(PropType::Other, |(t, _)| *t);
        out.groups.insert(
            group.name.clone(),
            VendorGroup {
                pixels: models.iter().map(|m| out.models[*m].pixels).sum(),
                ptype,
                whole: models.len() >= 4 && models.len() as f32 >= 0.6 * lit as f32,
                aliases: group_aliases.remove(&group.name).unwrap_or_default(),
            },
        );
    }
    Ok(out)
}

/// Every model, submodel, and strand with effects in the sequence, in its order (a model is
/// listed when its submodels or strands have effects, even with none of its own).
pub fn items_of(file: &XsqFile) -> Vec<VendorItem> {
    let timing: HashSet<&str> = file
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Timing)
        .map(|e| e.name.as_str())
        .collect();
    let item =
        |name: String, label: String, parent: Option<String>, kind: ItemKind, effects: usize| VendorItem {
            name,
            label,
            parent,
            kind,
            ptype: PropType::Other,
            display_as: None,
            effects,
            pixels: 0,
            aliases: Vec::new(),
            whole: false,
        };
    let mut out: Vec<VendorItem> = Vec::new();
    let mut listed: HashSet<String> = HashSet::new();
    for element in &file.elements {
        if element.kind != ElementKind::Model || timing.contains(element.name.as_str()) {
            continue;
        }
        let own: usize = element.layers.iter().map(|l| l.effects.len()).sum();
        let mut children: Vec<VendorItem> = Vec::new();
        for (layers, kind) in [
            (&element.submodels, ItemKind::Submodel),
            (&element.strands, ItemKind::Strand),
        ] {
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for layer in layers {
                match counts.iter_mut().find(|(n, _)| *n == layer.name) {
                    Some((_, c)) => *c += layer.effects.len(),
                    None => counts.push((&layer.name, layer.effects.len())),
                }
            }
            for (sub, effects) in counts.into_iter().filter(|(_, c)| *c > 0) {
                children.push(item(
                    format!("{}/{sub}", element.name),
                    unxml_safe(sub),
                    Some(element.name.clone()),
                    kind,
                    effects,
                ));
            }
        }
        if (own == 0 && children.is_empty()) || !listed.insert(element.name.clone()) {
            continue;
        }
        out.push(item(
            element.name.clone(),
            unxml_safe(&element.name),
            None,
            ItemKind::Model,
            own,
        ));
        out.extend(children);
    }
    out
}

fn file_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Reads `package`'s sequence `sequence` (or its best one) and suggests a mapping onto `show`.
/// `saved` gives the mapping saved under a key (see [`Inspection::key`]), if any.
pub fn inspect(
    package: &Package,
    sequence: Option<&str>,
    show: &Show,
    saved: impl FnOnce(&str) -> Option<Mapping>,
) -> Result<Inspection, XlightsError> {
    let (name, file) = package.read_sequence(sequence)?;
    // A layout that can't be read only means guessing from names.
    let layout = package
        .read_layout()
        .ok()
        .flatten()
        .and_then(|xml| vendor_layout(&xml).ok());
    let mut items = items_of(&file);
    automap::describe_items(&mut items, layout.as_ref());
    let key = layout.as_ref().map_or_else(
        || fingerprint("sequence", items.iter().map(|i| i.name.as_str())),
        |l| l.key.clone(),
    );
    let targets = automap::show_targets(show);
    let saved = saved(&key);
    let (suggestions, mapping) = automap::suggest(&items, &targets, saved.as_ref());
    let all_exact = items
        .iter()
        .zip(&suggestions)
        .filter(|(i, _)| i.effects > 0)
        .all(|(_, s)| s.reason == MatchReason::Exact && s.confidence >= 1.0);
    let song = unxml_safe(file.head.song.trim());
    let music = match package.kind() {
        PackageKind::Zip => package
            .music_for(&name, Some(&file.head.media_file))
            .map(|m| file_name(m).to_string()),
        _ => None,
    };
    Ok(Inspection {
        sequences: package.sequences().to_vec(),
        song: if song.is_empty() {
            name_from_path(Path::new(file_name(&name)))
        } else {
            song
        },
        sequence: name,
        has_layout: layout.is_some(),
        items,
        targets,
        suggestions,
        mapping,
        key,
        all_exact,
        music,
    })
}

/// Imports `package`'s sequence `sequence` (or its best one) onto `show` with `mapping`. Music
/// on disk (a sequence or folder package) is found with `find_audio`, as for any import; a zip's
/// music is copied into `music_folder` (see [`Package::copy_music`]), or, without one, left out
/// with a note.
pub fn import(
    package: &Package,
    sequence: Option<&str>,
    show: &Show,
    mapping: &Mapping,
    music_folder: Option<&Path>,
    find_audio: impl Fn(&Path, Option<&str>) -> Option<PathBuf>,
) -> Result<SequenceImport, XlightsError> {
    let (name, file) = package.read_sequence(sequence)?;
    let fallback = name_from_path(Path::new(file_name(&name)));
    let mut import = build_sequence_mapped(&file, show, &fallback, mapping);
    // Pictures are only looked for on disk: a zip's stay in it (and are named in a note).
    let on_disk = package.file_path(&name);
    crate::sequence::find_pictures(&mut import, on_disk.as_deref());
    if let Some(path) = on_disk {
        find_music(&mut import, &file, &path, find_audio);
        return Ok(import);
    }
    if matches!(file.head.sequence_type.trim(), "Animation" | "Effect") {
        return Ok(import);
    }
    let media = file.head.media_file.trim();
    match (package.music_for(&name, Some(media)), music_folder) {
        (Some(entry), Some(folder)) => match package.copy_music(entry, folder) {
            Ok(copied) => {
                let text = pf_model::path_to_text(&copied);
                if text.chars().count() <= pf_sequence::MAX_TEXT_LEN {
                    import.sequence.audio = Some(text);
                }
            }
            Err(e) => import.notes.insert(0, format!("{e} Choose the music in the sequence's settings.")),
        },
        (Some(entry), None) => import.notes.insert(
            0,
            format!(
                "The package's music ({}) wasn't saved, because there's no folder for it yet. Save the show, then import again, or choose the music in the sequence's settings.",
                file_name(entry)
            ),
        ),
        (None, _) if !media.is_empty() => import.notes.insert(
            0,
            format!(
                "The music ({}) isn't in the package; choose it in the sequence's settings.",
                file_name(media)
            ),
        ),
        (None, _) => {}
    }
    Ok(import)
}

#[cfg(test)]
mod tests;
