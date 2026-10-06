//! Building a PixelFlow show from an xLights layout and controller list.
//!
//! Props keep xLights' exact pixel positions (as measured shapes, in channel order) and color
//! order, and they're wired onto their controllers in channel order, with null pixels filling
//! any gaps (between props, and inside a prop whose channels have gaps), so PixelFlow's channel
//! layout matches xLights' and rendered sequences land on the right pixels. A prop that can't be
//! wired exactly isn't wired at all, with a note. Controllers carry their sequence channels.

use crate::channels::{ChannelRequest, resolve};
use crate::geometry::{Geometry, XNode};
use crate::layout::XLayout;
use crate::model::XmlModel;
use crate::networks::{XController, XOutput};
use pf_model::{
    ColorOrder, Controller, Group, GroupMember, MAX_NULL_PIXELS, MAX_SHOW_PIXELS, NodeRange, Port, PortSlot,
    Prop, PropId, Protocol, Provenance, RegionRef, SacnConfig, SequenceChannels, ShapeSource, Show,
    UniverseSize, Vec3,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// xLights layout units per PixelFlow unit (a 1280-unit-wide xLights preview is about 12.8 units).
const LAYOUT_SCALE: f32 = 0.01;

/// Highest sACN universe number.
const MAX_UNIVERSE: u32 = 63_999;

/// Most names listed in one note.
const MAX_LISTED: usize = 20;

/// Counts for the import report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub props: usize,
    pub pixels: u64,
    pub controllers: usize,
    /// Props wired onto a controller.
    pub wired: usize,
    pub groups: usize,
}

/// The imported show and a plain-language report of anything not imported exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct XlightsImport {
    pub show: Show,
    pub summary: ImportSummary,
    pub notes: Vec<String>,
}

/// Color order from an xLights `StringType` for a model with `cpn` channels per node (xLights
/// renders channel data in this order), and whether PixelFlow can represent it exactly.
fn color_order(string_type: &str, cpn: u8) -> (ColorOrder, bool) {
    let order = string_type.split_whitespace().next().unwrap_or("");
    match cpn {
        4 => match order {
            "RGBW" => (ColorOrder::Rgbw, true),
            "GRBW" => (ColorOrder::Grbw, true),
            _ if string_type == "4 Channel RGBW" => (ColorOrder::Rgbw, true),
            _ => (ColorOrder::Rgbw, false),
        },
        3 => match order {
            "RGB" => (ColorOrder::Rgb, true),
            "RBG" => (ColorOrder::Rbg, true),
            "GRB" => (ColorOrder::Grb, true),
            "GBR" => (ColorOrder::Gbr, true),
            "BRG" => (ColorOrder::Brg, true),
            "BGR" => (ColorOrder::Bgr, true),
            _ if string_type == "3 Channel RGB" => (ColorOrder::Rgb, true),
            _ => (ColorOrder::Rgb, false),
        },
        // Not wired (see `wire`), so the order doesn't matter.
        _ => (ColorOrder::Rgb, true),
    }
}

/// `names` joined with commas, the list cut short after [`MAX_LISTED`].
pub(crate) fn list(names: &[String]) -> String {
    if names.len() <= MAX_LISTED {
        return names.join(", ");
    }
    format!(
        "{}, and {} more",
        names[..MAX_LISTED].join(", "),
        names.len() - MAX_LISTED
    )
}

/// A PixelFlow controller and the block of xLights channels it covers.
struct Target {
    start: u32,
    channels: u32,
    controller: Controller,
}

impl Target {
    fn contains(&self, channel: u32) -> bool {
        channel >= self.start && u64::from(channel) < u64::from(self.start) + u64::from(self.channels)
    }
}

/// Splits sACN outputs into runs of consecutive universes of one size.
fn universe_runs(outputs: &[XOutput]) -> Vec<&[XOutput]> {
    let mut runs = Vec::new();
    let mut from = 0;
    for i in 1..=outputs.len() {
        let breaks = i == outputs.len() || {
            let (a, b) = (&outputs[i - 1], &outputs[i]);
            a.universe.checked_add(1) != Some(b.universe) || a.channels != b.channels
        };
        if breaks {
            runs.push(&outputs[from..i]);
            from = i;
        }
    }
    runs
}

/// The PixelFlow controllers for one xLights controller: one, or for sACN universes that aren't
/// consecutive or change size, one per run of consecutive universes of one size.
fn targets_for(x: &XController, notes: &mut Vec<String>) -> Vec<Target> {
    if !x.active {
        notes.push(format!(
            "{} is inactive in xLights, so it wasn't imported.",
            x.name
        ));
        return Vec::new();
    }
    match x.protocol.as_str() {
        "DDP" => {
            let mut controller = Controller::new(x.name.clone(), x.ip.clone(), Protocol::Ddp);
            controller.sequence_channels = (x.channels() > 0).then_some(SequenceChannels {
                start: x.start(),
                count: x.channels(),
                raw_ddp_offsets: x.keep_channel_numbers,
            });
            vec![Target {
                start: x.start(),
                channels: x.channels(),
                controller,
            }]
        }
        "E131" => sacn_targets(x, notes),
        other => {
            notes.push(format!(
                "{} uses {}, which PixelFlow can't send yet, so it wasn't imported.",
                x.name,
                if other.is_empty() {
                    "an unknown protocol"
                } else {
                    other
                }
            ));
            Vec::new()
        }
    }
}

fn sacn_targets(x: &XController, notes: &mut Vec<String>) -> Vec<Target> {
    let multicast = x.ip.is_empty() || x.ip.eq_ignore_ascii_case("MULTICAST");
    if x.outputs.is_empty() {
        let protocol = Protocol::Sacn(SacnConfig {
            allow_pixel_straddle: true,
            multicast,
            ..SacnConfig::default()
        });
        return vec![Target {
            start: x.start(),
            channels: 0,
            controller: Controller::new(x.name.clone(), x.ip.clone(), protocol),
        }];
    }
    let runs = universe_runs(&x.outputs);
    let split = runs.len() > 1;
    let mut targets = Vec::new();
    let mut names = Vec::new();
    for run in runs {
        let (first, last) = (run[0].universe, run[run.len() - 1].universe);
        let name = if !split {
            x.name.clone()
        } else if first == last {
            format!("{} (universe {first})", x.name)
        } else {
            format!("{} (universes {first}–{last})", x.name)
        };
        let size = run[0].channels;
        let universe_size = match size {
            510 => UniverseSize::Channels510,
            512 => UniverseSize::Channels512,
            _ => {
                notes.push(format!(
                    "{name} uses {size} channels per universe; PixelFlow sends 510 or 512, so it wasn't imported."
                ));
                continue;
            }
        };
        if first == 0 || last > MAX_UNIVERSE {
            notes.push(format!(
                "{name} uses universe numbers outside 1–{MAX_UNIVERSE}, so it wasn't imported."
            ));
            continue;
        }
        let protocol = Protocol::Sacn(SacnConfig {
            start_universe: u16::try_from(first).ok(),
            universe_size,
            // xLights numbers channels straight through its universes.
            allow_pixel_straddle: true,
            multicast,
        });
        let channels = run.iter().map(|o| o.channels).fold(0u32, u32::saturating_add);
        let mut controller = Controller::new(name.clone(), x.ip.clone(), protocol);
        controller.sequence_channels = (channels > 0).then_some(SequenceChannels {
            start: run[0].start,
            count: channels,
            raw_ddp_offsets: false,
        });
        names.push(name);
        targets.push(Target {
            start: run[0].start,
            channels,
            controller,
        });
    }
    if split {
        notes.push(format!(
            "{}'s universes aren't all consecutive and the same size, so it's imported as {} controllers: {}.",
            x.name,
            names.len(),
            names.join(", ")
        ));
    }
    targets
}

/// Consecutive nodes whose channels follow one another with no gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    /// Index of the run's first node (nodes are in channel order).
    first_node: u32,
    nodes: u32,
    /// Channel offset of the run's first node from the model's start channel.
    offset: u32,
}

/// Splits nodes (sorted by channel) into gap-free runs; an error when nodes share channels.
fn runs(nodes: &[XNode], cpn: u32) -> Result<Vec<Run>, String> {
    let mut runs: Vec<Run> = Vec::new();
    let mut next = 0u64;
    for (i, node) in nodes.iter().enumerate() {
        let channel = u64::from(node.channel);
        match runs.last_mut() {
            Some(run) if channel == next => run.nodes += 1,
            Some(_) if channel < next => {
                return Err("some of its pixels share channels in xLights".to_string());
            }
            _ => runs.push(Run {
                first_node: i as u32,
                nodes: 1,
                offset: node.channel,
            }),
        }
        next = channel + u64::from(cpn);
    }
    Ok(runs)
}

/// What wiring and start-channel resolution need from a model's geometry.
struct ModelInfo {
    /// See [`ChannelRequest::channels`].
    channels: Option<u32>,
    /// See [`ChannelRequest::first`].
    first: u32,
    cpn: u8,
    absolute: bool,
    /// The prop, when one was built.
    prop: Option<PropId>,
    /// The prop's gap-free channel runs, or why it can't be wired exactly.
    runs: Result<Vec<Run>, String>,
}

/// One model ready to wire.
struct Placed {
    prop: PropId,
    name: String,
    start: u32,
    cpn: u32,
    runs: Vec<Run>,
    port: Option<u16>,
}

/// Builds the show. `geometry` computes a model's nodes (injected so wiring can be tested alone).
pub fn build_show(
    name: &str,
    controllers: &[XController],
    layout: &XLayout,
    geometry: impl Fn(&XmlModel) -> Geometry,
) -> XlightsImport {
    build_show_within(name, controllers, layout, geometry, MAX_SHOW_PIXELS)
}

/// [`build_show`] with at most `max_lights` lights across all props.
fn build_show_within(
    name: &str,
    controllers: &[XController],
    layout: &XLayout,
    geometry: impl Fn(&XmlModel) -> Geometry,
    max_lights: u64,
) -> XlightsImport {
    let mut notes = Vec::new();
    let mut show = Show::new(name);

    // Props, in file order.
    let mut infos: Vec<Option<ModelInfo>> = Vec::with_capacity(layout.models.len());
    let mut prop_ids: HashMap<&str, PropId> = HashMap::new();
    let mut seen = HashSet::new();
    let mut lights: u64 = 0;
    let mut over_budget = Vec::new();
    let mut odd_colors = Vec::new();
    let mut region_notes = crate::submodels::RegionNotes::default();
    for model in &layout.models {
        if !seen.insert(model.name.as_str()) {
            notes.push(format!(
                "There are two props named \"{}\"; only the first was imported.",
                model.name
            ));
            infos.push(None);
            continue;
        }
        let g = geometry(model);
        if let Some(how) = &g.approximate {
            notes.push(format!("{}: {how}", model.name));
        }
        let mut info = ModelInfo {
            channels: g.channels_unknown.is_none().then_some(g.channels),
            first: g.nodes.first().map_or(0, |n| n.channel),
            cpn: g.channels_per_node,
            absolute: g.absolute_channels,
            prop: None,
            runs: match &g.channels_unknown {
                Some(why) => Err(why.clone()),
                None => runs(&g.nodes, u32::from(g.channels_per_node)),
            },
        };
        let count = g.nodes.len() as u64;
        if count == 0 {
            infos.push(Some(info));
            continue;
        }
        if !over_budget.is_empty() || lights + count > max_lights {
            // Keep the channel facts (later models may start relative to this one), not the prop.
            over_budget.push(model.name.clone());
            infos.push(Some(info));
            continue;
        }
        lights += count;
        let points: Vec<Vec3> = g
            .nodes
            .iter()
            .map(|node| {
                // A node with several lights (a "dumb" string) is drawn at their center.
                let n = node.points.len().max(1) as f32;
                let (x, y) = node
                    .points
                    .iter()
                    .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
                Vec3::new(x / n * LAYOUT_SCALE, y / n * LAYOUT_SCALE, 0.0)
            })
            .collect();
        // An editable shape when PixelFlow has one that lands every pixel where xLights does.
        let mut prop = match crate::shapes::editable(model, &points) {
            Some((generator, transform)) => {
                let mut prop = Prop::new(model.name.clone(), ShapeSource::Generator(generator));
                prop.transform = transform;
                prop
            }
            None => Prop::new(
                model.name.clone(),
                ShapeSource::Measured {
                    points,
                    provenance: Provenance::Import,
                },
            ),
        };
        let (order, exact) = color_order(model.text("StringType", "RGB Nodes"), g.channels_per_node);
        if !exact {
            odd_colors.push(model.name.clone());
        }
        prop.color_order = order;
        prop.regions = crate::submodels::regions(
            &model.name,
            &model.submodels,
            &model.faces,
            &model.states,
            prop.node_count(),
            &mut region_notes,
        );
        prop_ids.insert(model.name.as_str(), prop.id);
        info.prop = Some(prop.id);
        show.props.push(prop);
        infos.push(Some(info));
    }
    if !over_budget.is_empty() {
        notes.push(format!(
            "The show reached PixelFlow's limit of {max_lights} lights, so these props weren't imported: {}.",
            list(&over_budget)
        ));
    }
    if !odd_colors.is_empty() {
        notes.push(format!(
            "PixelFlow can't represent the color order of {}, so their colors may be swapped in \
             PixelFlow's preview and effects (xLights sequences still play as rendered).",
            list(&odd_colors)
        ));
    }
    region_notes.into_notes(&mut notes);

    // Start channels.
    let requests: Vec<ChannelRequest> = layout
        .models
        .iter()
        .zip(&infos)
        .map(|(m, info)| {
            let absolute = info.as_ref().is_some_and(|i| i.absolute);
            ChannelRequest {
                name: &m.name,
                start: if absolute {
                    "1"
                } else {
                    m.text("StartChannel", "1")
                },
                channels: info.as_ref().map_or(Some(0), |i| i.channels),
                first: info.as_ref().map_or(0, |i| i.first),
            }
        })
        .collect();
    let starts = resolve(&requests, controllers);

    // Controllers.
    let mut targets: Vec<Target> = controllers
        .iter()
        .flat_map(|x| targets_for(x, &mut notes))
        .collect();

    // Group placed models by the controller whose channel range holds their start.
    let mut by_target: HashMap<usize, Vec<Placed>> = HashMap::new();
    let mut unwired = Vec::new();
    for ((model, info), start) in layout.models.iter().zip(&infos).zip(&starts) {
        let Some(info) = info else { continue };
        let Some(prop) = info.prop else { continue };
        let start = match start {
            Ok(s) => *s,
            Err(why) => {
                notes.push(format!("{} isn't wired because {why}.", model.name));
                continue;
            }
        };
        let runs = match &info.runs {
            Ok(runs) => runs.clone(),
            Err(why) => {
                notes.push(format!("{} isn't wired because {why}.", model.name));
                continue;
            }
        };
        if info.absolute && model.text("StartChannel", "1").trim() != "1" {
            notes.push(format!(
                "{}: xLights' \"first strand\" setting makes this tree count its channels from \
                 channel 1 instead of its start channel, so it's placed at channel 1, as xLights \
                 renders it.",
                model.name
            ));
        }
        match targets.iter().position(|t| t.contains(start)) {
            Some(slot) => by_target.entry(slot).or_default().push(Placed {
                prop,
                name: model.name.clone(),
                start,
                cpn: u32::from(info.cpn),
                runs,
                port: model
                    .connection
                    .get("Port")
                    .and_then(|p| p.trim().parse::<u16>().ok())
                    .filter(|p| *p > 0),
            }),
            None => unwired.push(model.name.clone()),
        }
    }
    if !unwired.is_empty() {
        notes.push(format!(
            "Not wired because their channels aren't on any imported controller: {}.",
            list(&unwired)
        ));
    }

    // Wire each controller's models in channel order.
    let mut wired = 0;
    for (slot, target) in targets.iter_mut().enumerate() {
        let mut placed = by_target.remove(&slot).unwrap_or_default();
        placed.sort_by_key(|p| p.start);
        wired += wire(target, placed, &show.props, &mut notes);
    }
    show.controllers = targets.into_iter().map(|t| t.controller).collect();

    // Groups: members by name (`Prop/Submodel` for a submodel); nested groups are flattened.
    let group_members: HashMap<&str, &Vec<String>> = layout
        .groups
        .iter()
        .map(|g| (g.name.as_str(), &g.members))
        .collect();
    let submodel = |name: &str| -> Option<RegionRef> {
        let (prop, region) = name.split_once('/')?;
        let prop = show.prop(*prop_ids.get(prop.trim())?)?;
        let region = prop
            .regions
            .iter()
            .find(|r| r.is_submodel() && r.name == region.trim())?;
        Some(RegionRef {
            prop: prop.id,
            region: region.id,
        })
    };
    let mut lost_submodels = Vec::new();
    let mut groups = Vec::new();
    for xgroup in &layout.groups {
        // One ordered list, whole props and submodels mixed, as xLights lists them.
        let mut members: Vec<GroupMember> = Vec::new();
        let mut stack: Vec<&str> = xgroup.members.iter().rev().map(String::as_str).collect();
        let mut visited = HashSet::new();
        let mut lost = false;
        while let Some(name) = stack.pop() {
            if let Some(&id) = prop_ids.get(name) {
                if !members.contains(&GroupMember::Prop(id)) {
                    members.push(GroupMember::Prop(id));
                }
            } else if let Some(nested) = group_members.get(name) {
                if visited.insert(name) {
                    stack.extend(nested.iter().rev().map(String::as_str));
                }
            } else if let Some(member) = submodel(name) {
                if !members.contains(&GroupMember::Region(member)) {
                    members.push(GroupMember::Region(member));
                }
            } else if name.contains('/') {
                lost = true;
            }
        }
        if lost {
            lost_submodels.push(xgroup.name.clone());
        }
        if members.is_empty() {
            continue;
        }
        let mut group = Group::new(xgroup.name.clone());
        group.members = members;
        groups.push(group);
    }
    show.groups = groups;
    if !lost_submodels.is_empty() {
        notes.push(format!(
            "These groups list submodels that aren't in the show, so those members were left out: {}.",
            list(&lost_submodels)
        ));
    }

    let summary = ImportSummary {
        props: show.props.len(),
        pixels: show.props.iter().map(|p| u64::from(p.node_count())).sum(),
        controllers: show.controllers.len(),
        wired,
        groups: show.groups.len(),
    };
    XlightsImport { show, summary, notes }
}

/// Wires `placed` (in channel order) onto `target`'s ports, each prop exactly at its xLights
/// channels or not at all. Returns how many props were wired.
fn wire(target: &mut Target, placed: Vec<Placed>, props: &[Prop], notes: &mut Vec<String>) -> usize {
    let name = target.controller.name.clone();
    let end = u64::from(target.start) + u64::from(target.channels);
    // Where the last wired prop's channels end: gaps are measured from here.
    let mut cursor = u64::from(target.start);
    let mut ports: Vec<Port> = Vec::new();
    let mut wired = 0;
    for p in placed {
        let order = props.iter().find(|q| q.id == p.prop).map(|q| q.color_order);
        if order.map(|o| u32::from(o.channels_per_pixel())) != Some(p.cpn) {
            notes.push(format!(
                "{} uses {} channel(s) per pixel; PixelFlow wires only RGB and RGBW props, so it isn't wired.",
                p.name, p.cpn
            ));
            continue;
        }
        let start = u64::from(p.start);
        if start < cursor {
            notes.push(format!(
                "{} overlaps the prop before it on {name}, so it isn't wired.",
                p.name
            ));
            continue;
        }
        let last = p
            .runs
            .last()
            .map_or(0, |r| u64::from(r.offset) + u64::from(r.nodes) * u64::from(p.cpn));
        if start + last > end {
            notes.push(format!(
                "{} runs past the end of {name}'s channels, so it isn't wired.",
                p.name
            ));
            continue;
        }
        // One slot per gap-free run, null pixels filling the gap before it.
        let mut slots = Vec::with_capacity(p.runs.len());
        let mut at_end = cursor;
        let mut problem = None;
        for (i, run) in p.runs.iter().enumerate() {
            let at = start + u64::from(run.offset);
            let gap = at - at_end;
            let place = if i == 0 { "before it" } else { "inside it" };
            if gap % u64::from(p.cpn) != 0 {
                problem = Some(format!(
                    "the {gap}-channel gap {place} on {name} isn't a whole number of its pixels"
                ));
                break;
            }
            let nulls = gap / u64::from(p.cpn);
            if nulls > u64::from(MAX_NULL_PIXELS) {
                problem = Some(format!(
                    "the {gap}-channel gap {place} on {name} is too large to fill"
                ));
                break;
            }
            let mut slot = PortSlot::new(p.prop);
            slot.null_pixels = nulls as u32;
            if p.runs.len() > 1 {
                slot.segment = Some(NodeRange::new(run.first_node, run.first_node + run.nodes));
            }
            slots.push(slot);
            at_end = at + u64::from(run.nodes) * u64::from(p.cpn);
        }
        if let Some(why) = problem {
            notes.push(format!("{} isn't wired because {why}.", p.name));
            continue;
        }
        // Ports follow channel order, so a port number xLights uses again later joins the current port.
        let number = p
            .port
            .unwrap_or_else(|| ports.last().map_or(1, |port| port.number));
        let current = ports.last().map(|port| port.number);
        let used_earlier = current != Some(number) && ports.iter().any(|q| q.number == number);
        if used_earlier {
            notes.push(format!(
                "{} is on port {number} in xLights, after props on a later port; it's kept on port {} so \
                 channels stay in xLights' order.",
                p.name,
                current.unwrap_or(number)
            ));
        }
        match ports.last_mut() {
            Some(port) if current == Some(number) || used_earlier => port.slots.extend(slots),
            _ => {
                let mut port = Port::new(number);
                port.slots.extend(slots);
                ports.push(port);
            }
        }
        cursor = at_end;
        wired += 1;
    }
    target.controller.ports = ports;
    wired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{XNode, geometry};
    use crate::layout::XGroup;
    use crate::networks::XOutput;

    /// A stand-in geometry: `NodesPerString` nodes in a row with 3 channels each (1 for single
    /// color), or nodes at the comma-separated channel `Offsets`.
    fn fake_geometry(model: &XmlModel) -> Geometry {
        let cpn: u8 = if model.text("StringType", "").starts_with("Single") {
            1
        } else {
            3
        };
        let offsets: Vec<u32> = match model.attr("Offsets") {
            Some(list) => list.split(',').map(|v| v.trim().parse().unwrap()).collect(),
            None => {
                let nodes = model.int("NodesPerString", None, 0).max(0) as u32;
                (0..nodes).map(|i| i * u32::from(cpn)).collect()
            }
        };
        Geometry {
            channels: offsets.iter().map(|o| o + u32::from(cpn)).max().unwrap_or(0),
            nodes: offsets
                .iter()
                .enumerate()
                .map(|(i, &channel)| XNode {
                    channel,
                    points: vec![[i as f32 * 10.0, 100.0]],
                })
                .collect(),
            channels_per_node: cpn,
            approximate: None,
            channels_unknown: None,
            absolute_channels: false,
        }
    }

    fn model(name: &str, start: &str, nodes: u32, port: Option<u16>) -> XmlModel {
        let mut m = XmlModel {
            name: name.into(),
            display_as: "Single Line".into(),
            ..XmlModel::default()
        };
        m.attrs.insert("StartChannel".into(), start.into());
        m.attrs.insert("NodesPerString".into(), nodes.to_string());
        m.attrs.insert("X2".into(), "100".into());
        if let Some(port) = port {
            m.connection.insert("Port".into(), port.to_string());
        }
        m
    }

    fn with(mut m: XmlModel, attrs: &[(&str, &str)]) -> XmlModel {
        for (k, v) in attrs {
            m.attrs.insert(k.to_string(), v.to_string());
        }
        m
    }

    fn ddp(name: &str, start: u32, channels: u32) -> XController {
        XController {
            name: name.into(),
            ip: "192.0.2.20".into(),
            protocol: "DDP".into(),
            kind: "Ethernet".into(),
            active: true,
            keep_channel_numbers: false,
            outputs: vec![XOutput {
                universe: 1,
                start,
                channels,
            }],
        }
    }

    fn falcon() -> XController {
        ddp("Falcon", 1, 6147)
    }

    fn layout(models: Vec<XmlModel>) -> XLayout {
        XLayout {
            models,
            groups: vec![],
        }
    }

    /// Every node's controller channel as PixelFlow's channel map places it: `(controller index,
    /// channel)` per node, in node order. Panics if a node isn't wired exactly once.
    fn node_channels(show: &Show, name: &str) -> Vec<(usize, usize)> {
        let (map, _) = pf_mapping::map_show(show);
        let prop = show.props.iter().find(|p| p.name == name).unwrap();
        let layout = map.props.iter().find(|l| l.prop == prop.id).unwrap();
        let cpp = usize::from(layout.channels_per_pixel);
        let mut out = vec![None; layout.nodes as usize];
        for (c, output) in map.controllers.iter().enumerate() {
            for span in output.spans.iter().filter(|s| s.prop == prop.id) {
                let first = (span.frame_offset - layout.frame_offset) / cpp;
                for k in 0..span.pixels as usize {
                    assert!(out[first + k].is_none(), "{name} node {} wired twice", first + k);
                    out[first + k] = Some((c, span.controller_channel + k * cpp));
                }
            }
        }
        out.into_iter()
            .enumerate()
            .map(|(i, n)| n.unwrap_or_else(|| panic!("{name} node {i} isn't wired")))
            .collect()
    }

    /// `node_channels` for a prop on controller `c`, as plain channels.
    fn on(show: &Show, name: &str, c: usize) -> Vec<usize> {
        node_channels(show, name)
            .into_iter()
            .map(|(cc, ch)| {
                assert_eq!(cc, c, "{name} is on the wrong controller");
                ch
            })
            .collect()
    }

    fn is_wired(show: &Show, name: &str) -> bool {
        let id = show.props.iter().find(|p| p.name == name).unwrap().id;
        show.controllers
            .iter()
            .flat_map(|c| &c.ports)
            .flat_map(|p| &p.slots)
            .any(|s| s.prop == id)
    }

    fn row(start: usize, nodes: usize, cpn: usize) -> Vec<usize> {
        (0..nodes).map(|i| start + i * cpn).collect()
    }

    #[track_caller]
    fn has_note(result: &XlightsImport, text: &str) {
        let notes = result.notes.join("\n");
        assert!(notes.contains(text), "missing {text:?} in:\n{notes}");
    }

    #[test]
    fn props_are_wired_in_channel_order_with_gaps_filled() {
        let layout = XLayout {
            models: vec![
                model("Arch", "!Falcon:301", 50, Some(2)),
                model("Tree", "!Falcon:1", 100, Some(1)),
                model("Star", ">Arch:31", 20, Some(2)), // 30 channels (10 pixels) after Arch
            ],
            groups: vec![XGroup {
                name: "All".into(),
                members: vec!["Tree".into(), "Arch".into(), "Nope".into()],
            }],
        };
        let result = build_show("Haas", &[falcon()], &layout, fake_geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        assert_eq!(
            result.summary,
            ImportSummary {
                props: 3,
                pixels: 170,
                controllers: 1,
                wired: 3,
                groups: 1
            }
        );
        let c = &result.show.controllers[0];
        assert_eq!(
            c.sequence_channels,
            Some(SequenceChannels {
                start: 1,
                count: 6147,
                raw_ddp_offsets: false
            })
        );
        let ports: Vec<_> = c.ports.iter().map(|p| (p.number, p.slots.len())).collect();
        assert_eq!(ports, vec![(1, 1), (2, 2)]);
        let star = &c.ports[1].slots[1];
        assert_eq!(star.null_pixels, 10, "a 30-channel gap becomes 10 null pixels");
        // The channel map puts each node exactly where xLights does.
        let show = &result.show;
        assert_eq!(on(show, "Tree", 0), row(0, 100, 3));
        assert_eq!(on(show, "Arch", 0), row(300, 50, 3));
        assert_eq!(on(show, "Star", 0), row(480, 20, 3));
        assert_eq!(result.show.groups[0].members.len(), 2);
    }

    #[test]
    fn problems_are_reported_not_guessed() {
        let mut inactive = falcon();
        inactive.name = "Old".into();
        inactive.active = false;
        let mut art_net = falcon();
        art_net.name = "ArtNet box".into();
        art_net.protocol = "ArtNet".into();
        let white = with(
            model("Flood", "!Falcon:100", 2, None),
            &[("StringType", "Single Color White")],
        );
        let layout = layout(vec![
            model("Tree", "!Falcon:1", 100, None),
            model("Tree", "!Falcon:400", 5, None),
            model("Overlap", "!Falcon:10", 5, None),
            model("Lost", "!Ghost:1", 5, None),
            model("Far", "9000", 5, None),
            white,
        ]);
        let result = build_show("t", &[falcon(), inactive, art_net], &layout, fake_geometry);
        for expected in [
            "two props named \"Tree\"",
            "Old is inactive",
            "ArtNet box uses ArtNet",
            "Overlap overlaps",
            "Lost isn't wired because its controller \"Ghost\"",
            "Not wired because their channels aren't on any imported controller: Far.",
            "Flood uses 1 channel(s) per pixel",
        ] {
            has_note(&result, expected);
        }
        assert_eq!(result.summary.wired, 1);
    }

    #[test]
    fn gaps_inside_a_custom_model_keep_every_node_on_its_xlights_channel() {
        // Nodes 1, 2 and 5: channels 0, 3 and 12; the next prop starts at 15.
        let custom = XmlModel {
            name: "Custom".into(),
            display_as: "Custom".into(),
            attrs: [("StartChannel", "!Falcon:1"), ("CustomModel", "1,2,,,5")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        };
        let layout = layout(vec![custom, model("After", ">Custom:1", 4, None)]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        assert_eq!(on(&result.show, "Custom", 0), [0, 3, 12]);
        assert_eq!(on(&result.show, "After", 0), row(15, 4, 3));
        let (_, report) = pf_mapping::map_show(&result.show);
        assert!(report.issues.is_empty(), "{report:?}");
    }

    #[test]
    fn a_custom_model_without_node_one_starts_at_its_first_node() {
        // Nodes 3 and 4 only: xLights puts node 3 six channels after the start channel.
        let custom = XmlModel {
            name: "Custom".into(),
            display_as: "Custom".into(),
            attrs: [("StartChannel", "!Falcon:31"), ("CustomModel", "3,4")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        };
        let layout = layout(vec![
            custom,
            model("At", "@Custom:13", 2, None),
            model("After", ">Custom:1", 2, None),
        ]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        assert_eq!(on(&result.show, "Custom", 0), [36, 39]);
        // `@Custom:13` counts from the custom model's first node (channel 37), not its start.
        assert_eq!(on(&result.show, "After", 0), row(42, 2, 3));
        assert_eq!(on(&result.show, "At", 0), row(48, 2, 3));
    }

    #[test]
    fn advanced_string_starts_keep_their_gap() {
        // Two strings of 10: String2 starts 300 channels after String1.
        let m = with(
            XmlModel {
                name: "M".into(),
                display_as: "Vert Matrix".into(),
                ..XmlModel::default()
            },
            &[
                ("StartChannel", "!Falcon:1"),
                ("NumStrings", "2"),
                ("NodesPerString", "10"),
                ("Advanced", "1"),
                ("String1", "!Falcon:1"),
                ("String2", "!Falcon:301"),
            ],
        );
        let layout = layout(vec![m, model("After", ">M:1", 5, None)]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        let mut got = on(&result.show, "M", 0);
        got.sort_unstable();
        let mut want = row(0, 10, 3);
        want.extend(row(300, 10, 3));
        assert_eq!(got, want);
        assert_eq!(on(&result.show, "After", 0), row(330, 5, 3));
    }

    #[test]
    fn a_strands_per_string_remainder_keeps_its_gap() {
        // 2 strings of 5 nodes in 2 strands: 2 per strand, 4 per string, strings 15 channels apart.
        let m = with(
            XmlModel {
                name: "M".into(),
                display_as: "Vert Matrix".into(),
                ..XmlModel::default()
            },
            &[
                ("StartChannel", "!Falcon:1"),
                ("NumStrings", "2"),
                ("NodesPerString", "5"),
                ("StrandsPerString", "2"),
            ],
        );
        let layout = layout(vec![m, model("After", ">M:1", 2, None)]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        let mut got = on(&result.show, "M", 0);
        got.sort_unstable();
        assert_eq!(got, [0, 3, 6, 9, 15, 18, 21, 24]);
        assert_eq!(on(&result.show, "After", 0), row(27, 2, 3));
    }

    #[test]
    fn rgbw_props_use_four_channels_per_pixel() {
        // A dumb 3-string "4 Channel RGBW" line: one 4-channel node per string; then an RGB line.
        let rgbw = with(
            model("W", "!Falcon:1", 5, None),
            &[("NumStrings", "3"), ("StringType", "4 Channel RGBW")],
        );
        let layout = layout(vec![rgbw, model("Rgb", ">W:1", 3, None)]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        assert!(result.notes.is_empty(), "{:?}", result.notes);
        let w = result.show.props.iter().find(|p| p.name == "W").unwrap();
        assert_eq!(w.color_order, ColorOrder::Rgbw);
        assert_eq!(on(&result.show, "W", 0), [0, 4, 8]);
        assert_eq!(on(&result.show, "Rgb", 0), row(12, 3, 3));
    }

    #[test]
    fn a_gap_that_isnt_whole_pixels_leaves_the_prop_unwired() {
        // A 7-channel DMX fixture (not wired), then A right after it, B right after A, and C one
        // pixel past an even gap from the true end of the last wired channels.
        let dmx = with(
            XmlModel {
                name: "Dmx".into(),
                display_as: "DmxGeneral".into(),
                ..XmlModel::default()
            },
            &[("StartChannel", "!Falcon:1"), ("DmxChannelCount", "7")],
        );
        let layout = layout(vec![
            dmx,
            model("A", "!Falcon:8", 10, None),
            model("B", ">A:1", 10, None),
            model("C", "!Falcon:76", 2, None),
        ]);
        let result = build_show("t", &[falcon()], &layout, geometry);
        has_note(&result, "Dmx uses 1 channel(s) per pixel");
        has_note(
            &result,
            "A isn't wired because the 7-channel gap before it on Falcon isn't a whole number",
        );
        has_note(&result, "B isn't wired because the 37-channel gap before it");
        assert!(!is_wired(&result.show, "A") && !is_wired(&result.show, "B"));
        assert_eq!(on(&result.show, "C", 0), row(75, 2, 3));
    }

    fn sacn(name: &str, outputs: &[(u32, u32)]) -> XController {
        let mut next = 1;
        XController {
            name: name.into(),
            ip: "192.0.2.30".into(),
            protocol: "E131".into(),
            kind: "Ethernet".into(),
            active: true,
            keep_channel_numbers: false,
            outputs: outputs
                .iter()
                .map(|&(universe, channels)| {
                    let o = XOutput {
                        universe,
                        start: next,
                        channels,
                    };
                    next += channels;
                    o
                })
                .collect(),
        }
    }

    #[test]
    fn sacn_universes_that_skip_or_change_size_become_separate_controllers() {
        // Universes 1–2 (512), 5 (510), 6 (512); xLights' channels run straight through.
        let x = sacn("Porch", &[(1, 512), (2, 512), (5, 510), (6, 512)]);
        let layout = layout(vec![
            model("Straddle", "502", 10, None), // channels 502–531, across universes 1 and 2
            model("Five", "#5:7", 4, None),     // universe 5, channel 7 → absolute 1031
            model("Six", "#6:1", 2, None),
        ]);
        let result = build_show("t", &[x], &layout, fake_geometry);
        has_note(&result, "imported as 3 controllers");
        let names: Vec<_> = result.show.controllers.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Porch (universes 1–2)",
                "Porch (universe 5)",
                "Porch (universe 6)"
            ]
        );
        let seq: Vec<_> = result
            .show
            .controllers
            .iter()
            .map(|c| c.sequence_channels.map(|s| (s.start, s.count)))
            .collect();
        assert_eq!(seq, [Some((1, 1024)), Some((1025, 510)), Some((1535, 512))]);
        for c in &result.show.controllers {
            let Protocol::Sacn(cfg) = c.protocol else {
                panic!("sACN")
            };
            assert!(cfg.allow_pixel_straddle);
        }
        let show = &result.show;
        assert_eq!(on(show, "Straddle", 0), row(501, 10, 3));
        assert_eq!(on(show, "Five", 1), row(6, 4, 3));
        assert_eq!(on(show, "Six", 2), row(0, 2, 3));
        // The wire addresses match xLights': Straddle's 4th pixel (510–512) spans universes 1 and 2.
        let (map, report) = pf_mapping::map_show(show);
        assert!(!report.has_errors(), "{report:?}");
        let address = map.controllers[0].addressing.address_of(512);
        assert_eq!(
            address,
            Some(pf_mapping::ChannelAddress::Sacn {
                universe: 2,
                channel: 1
            })
        );
        assert_eq!(
            map.controllers[1].addressing.address_of(6),
            Some(pf_mapping::ChannelAddress::Sacn {
                universe: 5,
                channel: 7
            })
        );
    }

    #[test]
    fn sacn_universe_sizes_pixelflow_cant_send_are_not_imported() {
        let x = sacn("Odd", &[(1, 500)]);
        let result = build_show("t", &[x], &layout(vec![model("A", "1", 2, None)]), fake_geometry);
        has_note(&result, "Odd uses 500 channels per universe");
        has_note(&result, "aren't on any imported controller: A");
        assert!(result.show.controllers.is_empty());
        let far = sacn("Far", &[(u32::MAX, 510)]);
        let result = build_show("t", &[far], &layout(vec![]), fake_geometry);
        has_note(&result, "Far uses universe numbers outside 1–63999");
    }

    #[test]
    fn a_model_too_large_to_import_still_places_the_models_after_it() {
        // 2 × 600000 nodes is over the per-model limit: no prop, but its 3,600,000 channels count.
        let huge = with(model("Huge", "!Big:1", 600_000, None), &[("NumStrings", "2")]);
        let circle = with(
            XmlModel {
                name: "Ring".into(),
                display_as: "Circle".into(),
                ..XmlModel::default()
            },
            &[
                ("StartChannel", "!Next:100"),
                ("NumStrings", "2"),
                ("NodesPerString", "600000"),
            ],
        );
        let layout = layout(vec![
            huge,
            model("After", ">Huge:1", 3, None),
            circle,
            model("Lost", ">Ring:1", 3, None),
        ]);
        let controllers = [ddp("Big", 1, 3_600_000), ddp("Next", 3_600_001, 600)];
        let result = build_show("t", &controllers, &layout, geometry);
        has_note(&result, "Huge: model has 1200000 lights");
        has_note(&result, "Ring: model has 1200000 lights");
        has_note(
            &result,
            "Lost isn't wired because it starts relative to \"Ring\", whose channels aren't known",
        );
        assert_eq!(on(&result.show, "After", 1), row(0, 3, 3));
    }

    #[test]
    fn huge_channel_numbers_dont_overflow() {
        let big = XController {
            outputs: vec![XOutput {
                universe: 1,
                start: 1,
                channels: u32::MAX,
            }],
            ..falcon()
        };
        let layout = layout(vec![
            model("Edge", "4294967290", 10, None),
            model("Over", ">Edge:1", 1, None),
        ]);
        let result = build_show("t", &[big], &layout, fake_geometry);
        has_note(&result, "Edge runs past the end of Falcon's channels");
        has_note(&result, "Over isn't wired because its channels are too high");
    }

    #[test]
    fn the_show_light_budget_stops_building_props() {
        let layout = layout(vec![
            model("A", "1", 100, None),
            model("B", ">A:1", 40, None),
            model("C", ">B:1", 30, None),
            model("D", ">C:1", 5, None),
        ]);
        let result = build_show_within("t", &[falcon()], &layout, fake_geometry, 150);
        let names: Vec<_> = result.show.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        has_note(
            &result,
            "limit of 150 lights, so these props weren't imported: C, D.",
        );
        assert_eq!(on(&result.show, "B", 0), row(300, 40, 3));
    }

    #[test]
    fn submodel_members_and_odd_color_orders_are_reported() {
        let mut bulbs = with(model("Bulbs", "1", 3, None), &[("StringType", "WRGB Nodes")]);
        bulbs.submodels.push(
            [("name", "Left"), ("line0", "1-2")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let layout = XLayout {
            models: vec![bulbs],
            groups: vec![
                XGroup {
                    name: "Faces".into(),
                    members: vec!["Bulbs/Left".into(), "Bulbs/Eyes".into(), "Bulbs/Left".into()],
                },
                XGroup {
                    name: "Outer".into(),
                    members: vec!["Faces".into()],
                },
                XGroup {
                    name: "Ghosts".into(),
                    members: vec!["Nope/Left".into()],
                },
            ],
        };
        let result = build_show("t", &[falcon()], &layout, geometry);
        has_note(
            &result,
            "These groups list submodels that aren't in the show, so those members were left out: Faces, Outer, Ghosts.",
        );
        has_note(&result, "color order of Bulbs, so their colors may be swapped");
        let bulbs = &result.show.props[0];
        let left = RegionRef {
            prop: bulbs.id,
            region: bulbs.regions[0].id,
        };
        let names: Vec<_> = result.show.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["Faces", "Outer"],
            "a group with nothing left isn't imported"
        );
        for group in &result.show.groups {
            assert_eq!(group.members, vec![GroupMember::Region(left)]);
        }
    }

    #[test]
    fn groups_keep_xlights_member_order_with_submodels_mixed_in() {
        let sub = |name: &str, line: &str| -> crate::submodels::Attrs {
            [("name", name), ("line0", line)]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let mut arch = model("Arch", "1", 4, None);
        arch.submodels = vec![sub("Left", "1-2"), sub("Right", "3-4")];
        let tree = model("Tree", "13", 4, None);
        let layout = XLayout {
            models: vec![arch, tree],
            groups: vec![XGroup {
                name: "Across".into(),
                members: vec!["Arch/Left".into(), "Tree".into(), "Arch/Right".into()],
            }],
        };
        let result = build_show("t", &[falcon()], &layout, geometry);
        let show = &result.show;
        let arch = show.props.iter().find(|p| p.name == "Arch").unwrap();
        let tree = show.props.iter().find(|p| p.name == "Tree").unwrap();
        let part = |i: usize| {
            GroupMember::Region(RegionRef {
                prop: arch.id,
                region: arch.regions[i].id,
            })
        };
        assert_eq!(
            show.groups[0].members,
            vec![part(0), GroupMember::Prop(tree.id), part(1)]
        );
    }

    #[test]
    fn a_tree_with_a_first_strand_is_placed_at_channel_one_like_xlights() {
        let tree = with(
            XmlModel {
                name: "Tree".into(),
                display_as: "Tree 360".into(),
                ..XmlModel::default()
            },
            &[
                ("StartChannel", "100"),
                ("parm1", "4"),
                ("parm2", "2"),
                ("exportFirstStrand", "3"),
            ],
        );
        let result = build_show("t", &[falcon()], &layout(vec![tree]), geometry);
        has_note(&result, "placed at channel 1");
        let mut got = on(&result.show, "Tree", 0);
        got.sort_unstable();
        assert_eq!(got, row(0, 8, 3));
    }

    #[test]
    fn color_orders_follow_the_string_type_and_channel_count() {
        assert_eq!(color_order("GRB Nodes", 3), (ColorOrder::Grb, true));
        assert_eq!(color_order("RGBW Nodes", 4), (ColorOrder::Rgbw, true));
        assert_eq!(color_order("GRBW Nodes", 4), (ColorOrder::Grbw, true));
        assert_eq!(color_order("3 Channel RGB", 3), (ColorOrder::Rgb, true));
        assert_eq!(color_order("4 Channel RGBW", 4), (ColorOrder::Rgbw, true));
        assert_eq!(color_order("4 Channel WRGB", 4), (ColorOrder::Rgbw, false));
        assert_eq!(color_order("BGRW Nodes", 4), (ColorOrder::Rgbw, false));
    }
}
