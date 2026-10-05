//! Building a PixelFlow show from an xLights layout and controller list.
//!
//! Props keep xLights' exact pixel positions (as measured shapes, in channel order) and color
//! order, and they're wired onto their controllers in channel order, with null pixels filling
//! any gaps, so PixelFlow's channel layout matches xLights' and rendered sequences land on the
//! right pixels. Controllers carry their sequence channels.

use crate::channels::{ChannelRequest, resolve};
use crate::geometry::Geometry;
use crate::layout::XLayout;
use crate::model::XmlModel;
use crate::networks::XController;
use pf_model::{
    ColorOrder, Controller, Group, MAX_NULL_PIXELS, Port, PortSlot, Prop, PropId, Protocol, Provenance,
    SacnConfig, SequenceChannels, ShapeSource, Show, UniverseSize, Vec3,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// xLights layout units per PixelFlow unit (a 1280-unit-wide xLights preview is about 12.8 units).
const LAYOUT_SCALE: f32 = 0.01;

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

/// Color order from an xLights `StringType` (xLights renders channel data in this order).
fn color_order(string_type: &str) -> ColorOrder {
    let order = string_type.split_whitespace().next().unwrap_or("");
    match order {
        "RBG" => ColorOrder::Rbg,
        "GRB" => ColorOrder::Grb,
        "GBR" => ColorOrder::Gbr,
        "BRG" => ColorOrder::Brg,
        "BGR" => ColorOrder::Bgr,
        "GRBW" => ColorOrder::Grbw,
        o if o.len() == 4 && o.contains('W') => ColorOrder::Rgbw,
        _ => ColorOrder::Rgb,
    }
}

fn controller_for(x: &XController, notes: &mut Vec<String>) -> Option<Controller> {
    if !x.active {
        notes.push(format!(
            "{} is inactive in xLights, so it wasn't imported.",
            x.name
        ));
        return None;
    }
    let protocol = match x.protocol.as_str() {
        "DDP" => Protocol::Ddp,
        "E131" => {
            let size = x.outputs.first().map_or(510, |o| o.channels);
            let universe_size = if size == 512 {
                UniverseSize::Channels512
            } else {
                if size != 510 {
                    notes.push(format!(
                        "{} uses {size} channels per universe; PixelFlow uses 510 or 512, so it's set to 510.",
                        x.name
                    ));
                }
                UniverseSize::Channels510
            };
            Protocol::Sacn(SacnConfig {
                start_universe: x.outputs.first().and_then(|o| u16::try_from(o.universe).ok()),
                universe_size,
                multicast: x.ip.is_empty() || x.ip.eq_ignore_ascii_case("MULTICAST"),
                ..SacnConfig::default()
            })
        }
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
            return None;
        }
    };
    let mut controller = Controller::new(x.name.clone(), x.ip.clone(), protocol);
    controller.sequence_channels = (x.channels() > 0).then_some(SequenceChannels {
        start: x.start(),
        count: x.channels(),
        raw_ddp_offsets: x.keep_channel_numbers && x.protocol == "DDP",
    });
    Some(controller)
}

/// One model ready to wire: its prop, where its channels start, and how many it uses.
struct Placed {
    prop: PropId,
    name: String,
    start: u32,
    channels: u32,
    channels_per_pixel: u32,
    port: Option<u16>,
}

/// Builds the show. `geometry` computes a model's nodes (injected so wiring can be tested alone).
pub fn build_show(
    name: &str,
    controllers: &[XController],
    layout: &XLayout,
    geometry: impl Fn(&XmlModel) -> Geometry,
) -> XlightsImport {
    let mut notes = Vec::new();
    let mut show = Show::new(name);

    // Props, in file order.
    let mut geometries = Vec::with_capacity(layout.models.len());
    let mut prop_ids: HashMap<&str, PropId> = HashMap::new();
    let mut seen = HashSet::new();
    for model in &layout.models {
        if !seen.insert(model.name.as_str()) {
            notes.push(format!(
                "There are two props named \"{}\"; only the first was imported.",
                model.name
            ));
            geometries.push(None);
            continue;
        }
        let g = geometry(model);
        if g.nodes.is_empty() {
            geometries.push(None);
            continue;
        }
        if let Some(how) = &g.approximate {
            notes.push(format!("{}: {how}", model.name));
        }
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
        let mut prop = Prop::new(
            model.name.clone(),
            ShapeSource::Measured {
                points,
                provenance: Provenance::Import,
            },
        );
        prop.color_order = color_order(model.text("StringType", "RGB Nodes"));
        prop_ids.insert(model.name.as_str(), prop.id);
        show.props.push(prop);
        geometries.push(Some(g));
    }

    // Start channels.
    let requests: Vec<ChannelRequest> = layout
        .models
        .iter()
        .zip(&geometries)
        .map(|(m, g)| ChannelRequest {
            name: &m.name,
            start: m.text("StartChannel", "1"),
            channels: g.as_ref().map_or(0, |g| g.channels),
        })
        .collect();
    let starts = resolve(&requests, controllers);

    // Controllers.
    let imported: Vec<(usize, Controller)> = controllers
        .iter()
        .enumerate()
        .filter_map(|(i, x)| controller_for(x, &mut notes).map(|c| (i, c)))
        .collect();

    // Group placed models by the controller whose channel range holds their start.
    let mut by_controller: HashMap<usize, Vec<Placed>> = HashMap::new();
    let mut unwired = Vec::new();
    for ((model, g), start) in layout.models.iter().zip(&geometries).zip(&starts) {
        let (Some(g), Some(&prop)) = (g, prop_ids.get(model.name.as_str())) else {
            continue;
        };
        let start = match start {
            Ok(s) => *s,
            Err(why) => {
                notes.push(format!("{} isn't wired because {why}.", model.name));
                continue;
            }
        };
        match imported.iter().position(|(i, _)| controllers[*i].contains(start)) {
            Some(slot) => by_controller.entry(slot).or_default().push(Placed {
                prop,
                name: model.name.clone(),
                start,
                channels: g.channels,
                channels_per_pixel: u32::from(g.channels_per_node),
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
            unwired.join(", ")
        ));
    }

    // Wire each controller's models in channel order.
    let mut wired = 0;
    let mut show_controllers = Vec::new();
    for (slot, (i, mut controller)) in imported.into_iter().enumerate() {
        let x = &controllers[i];
        let mut placed = by_controller.remove(&slot).unwrap_or_default();
        placed.sort_by_key(|p| p.start);
        let end = u64::from(x.start()) + u64::from(x.channels());
        let mut cursor = x.start();
        let mut ports: Vec<Port> = Vec::new();
        for p in placed {
            if !matches!(p.channels_per_pixel, 3 | 4) {
                notes.push(format!(
                    "{} uses {} channel(s) per pixel; PixelFlow wires only RGB and RGBW props, so it isn't wired.",
                    p.name, p.channels_per_pixel
                ));
                continue;
            }
            if p.start < cursor {
                notes.push(format!(
                    "{} overlaps the prop before it on {}, so it isn't wired.",
                    p.name, x.name
                ));
                continue;
            }
            if u64::from(p.start) + u64::from(p.channels) > end {
                notes.push(format!(
                    "{} runs past the end of {}'s channels, so it isn't wired.",
                    p.name, x.name
                ));
                continue;
            }
            let gap = p.start - cursor;
            let nulls = gap / p.channels_per_pixel;
            if nulls > MAX_NULL_PIXELS {
                notes.push(format!(
                    "{} starts {gap} channels after the prop before it on {}; that gap is too large to fill, so it isn't wired.",
                    p.name, x.name
                ));
                continue;
            }
            if !gap.is_multiple_of(p.channels_per_pixel) {
                notes.push(format!(
                    "{} doesn't start on a pixel boundary on {}, so its colors may be off by a channel.",
                    p.name, x.name
                ));
            }
            let mut wire = PortSlot::new(p.prop);
            wire.null_pixels = nulls;
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
                Some(port) if current == Some(number) || used_earlier => port.slots.push(wire),
                _ => {
                    let mut port = Port::new(number);
                    port.slots.push(wire);
                    ports.push(port);
                }
            }
            cursor = p.start + p.channels;
            wired += 1;
        }
        controller.ports = ports;
        show_controllers.push(controller);
    }
    show.controllers = show_controllers;

    // Groups: members by name; nested groups are flattened, submodels skipped.
    let group_members: HashMap<&str, &Vec<String>> = layout
        .groups
        .iter()
        .map(|g| (g.name.as_str(), &g.members))
        .collect();
    for xgroup in &layout.groups {
        let mut members = Vec::new();
        let mut stack: Vec<&str> = xgroup.members.iter().rev().map(String::as_str).collect();
        let mut visited = HashSet::new();
        while let Some(name) = stack.pop() {
            if let Some(&id) = prop_ids.get(name) {
                if !members.contains(&id) {
                    members.push(id);
                }
            } else if let Some(nested) = group_members.get(name)
                && visited.insert(name)
            {
                stack.extend(nested.iter().rev().map(String::as_str));
            }
        }
        if members.is_empty() {
            continue;
        }
        let mut group = Group::new(xgroup.name.clone());
        group.members = members;
        show.groups.push(group);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::XNode;
    use crate::layout::XGroup;
    use crate::networks::XOutput;

    /// A stand-in geometry: `NodesPerString` RGB nodes in a row (or what `StringType` implies).
    fn fake_geometry(model: &XmlModel) -> Geometry {
        let nodes = model.int("NodesPerString", None, 0).max(0) as u32;
        let cpn: u8 = if model.text("StringType", "").starts_with("Single") {
            1
        } else {
            3
        };
        Geometry {
            nodes: (0..nodes)
                .map(|i| XNode {
                    channel: i * u32::from(cpn),
                    points: vec![[i as f32 * 10.0, 100.0]],
                })
                .collect(),
            channels_per_node: cpn,
            channels: nodes * u32::from(cpn),
            approximate: None,
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
        if let Some(port) = port {
            m.connection.insert("Port".into(), port.to_string());
        }
        m
    }

    fn falcon() -> XController {
        XController {
            name: "Falcon".into(),
            ip: "192.0.2.20".into(),
            protocol: "DDP".into(),
            kind: "Ethernet".into(),
            active: true,
            keep_channel_numbers: false,
            outputs: vec![XOutput {
                universe: 1,
                start: 1,
                channels: 6147,
            }],
        }
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
        // The channel map puts each prop exactly where xLights does.
        let map = pf_mapping_check(&result.show);
        let expected: Vec<(String, u32)> = [("Tree", 0), ("Arch", 300), ("Star", 480)]
            .iter()
            .map(|(n, c)| (n.to_string(), *c))
            .collect();
        assert_eq!(map, expected);
        assert_eq!(result.show.groups[0].members.len(), 2);
    }

    /// Each wired prop's controller channel, computed the way PixelFlow's mapping does: slots in
    /// port order, null pixels first.
    fn pf_mapping_check(show: &Show) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        for c in &show.controllers {
            let mut channel = 0;
            for port in &c.ports {
                for slot in &port.slots {
                    let prop = show.props.iter().find(|p| p.id == slot.prop).unwrap();
                    channel += slot.null_pixels * 3;
                    out.push((prop.name.clone(), channel));
                    channel += prop.node_count() * 3;
                }
            }
        }
        out
    }

    #[test]
    fn problems_are_reported_not_guessed() {
        let mut inactive = falcon();
        inactive.name = "Old".into();
        inactive.active = false;
        let mut art_net = falcon();
        art_net.name = "ArtNet box".into();
        art_net.protocol = "ArtNet".into();
        let mut white = model("Flood", "!Falcon:100", 2, None);
        white
            .attrs
            .insert("StringType".into(), "Single Color White".into());
        let layout = XLayout {
            models: vec![
                model("Tree", "!Falcon:1", 100, None),
                model("Tree", "!Falcon:400", 5, None),
                model("Overlap", "!Falcon:10", 5, None),
                model("Lost", "!Ghost:1", 5, None),
                model("Far", "9000", 5, None),
                white,
            ],
            groups: vec![],
        };
        let result = build_show("t", &[falcon(), inactive, art_net], &layout, fake_geometry);
        let notes = result.notes.join("\n");
        for expected in [
            "two props named \"Tree\"",
            "Old is inactive",
            "ArtNet box uses ArtNet",
            "Overlap overlaps",
            "Lost isn't wired because its controller \"Ghost\"",
            "Not wired because their channels aren't on any imported controller: Far.",
            "Flood uses 1 channel(s) per pixel",
        ] {
            assert!(notes.contains(expected), "missing {expected:?} in:\n{notes}");
        }
        assert_eq!(result.summary.wired, 1);
    }

    #[test]
    fn color_orders_follow_the_string_type() {
        assert_eq!(color_order("GRB Nodes"), ColorOrder::Grb);
        assert_eq!(color_order("RGBW Nodes"), ColorOrder::Rgbw);
        assert_eq!(color_order("GRBW Nodes"), ColorOrder::Grbw);
        assert_eq!(color_order("3 Channel RGB"), ColorOrder::Rgb);
    }
}
