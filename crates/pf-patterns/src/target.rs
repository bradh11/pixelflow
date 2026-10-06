//! Which pixels a test pattern lights, in wiring order.

use pf_mapping::ChannelMap;
use pf_model::{ControllerId, GroupId, GroupMember, PropId, RegionKind, Show};

/// What a test pattern lights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Every prop in the show.
    Show,
    Prop(PropId),
    Group(GroupId),
    /// Everything wired to a controller, in wiring order.
    Controller(ControllerId),
    /// Everything wired to one port, in wiring order.
    Port {
        controller: ControllerId,
        port: u16,
    },
}

/// A run of pixels in the frame buffer that a pattern paints, in pattern order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetRange {
    /// Frame-buffer byte offset of the lowest-numbered node in the range.
    pub frame_offset: usize,
    pub pixels: u32,
    pub channels_per_pixel: u8,
    /// When true, pattern order runs from the last node to the first (matches wiring).
    pub reverse: bool,
}

/// Frame ranges for a target. Unknown ids resolve to no ranges.
pub fn resolve_target(show: &Show, map: &ChannelMap, target: &Target) -> Vec<TargetRange> {
    let whole_prop = |id: PropId| {
        map.prop_layout(id).map(|layout| TargetRange {
            frame_offset: layout.frame_offset,
            pixels: layout.nodes,
            channels_per_pixel: layout.channels_per_pixel,
            reverse: false,
        })
    };
    match target {
        Target::Show => map.props.iter().filter_map(|p| whole_prop(p.prop)).collect(),
        Target::Prop(id) => whole_prop(*id).into_iter().collect(),
        Target::Group(id) => show
            .groups
            .iter()
            .find(|g| g.id == *id)
            .map(|g| {
                g.members
                    .iter()
                    .flat_map(|m| match m {
                        GroupMember::Prop(id) => whole_prop(*id).into_iter().collect(),
                        GroupMember::Region(r) => region_ranges(show, map, r.prop, r.region),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::Controller(id) => spans(map, *id, None),
        Target::Port { controller, port } => spans(map, *controller, Some(*port)),
    }
}

/// A submodel's pixels as runs in its own order (a backwards run plays backwards). A rectangle
/// submodel needs pixel positions, which test patterns don't have, so it lights its whole prop.
fn region_ranges(
    show: &Show,
    map: &ChannelMap,
    prop: PropId,
    region: pf_model::RegionId,
) -> Vec<TargetRange> {
    let (Some(layout), Some(region)) = (
        map.prop_layout(prop),
        show.prop(prop).and_then(|p| p.region(region)),
    ) else {
        return Vec::new();
    };
    let range = |first: u32, pixels: u32, reverse: bool| TargetRange {
        frame_offset: layout.frame_offset + first as usize * usize::from(layout.channels_per_pixel),
        pixels,
        channels_per_pixel: layout.channels_per_pixel,
        reverse,
    };
    if matches!(region.kind, RegionKind::SubBuffer { .. }) {
        return vec![range(0, layout.nodes, false)];
    }
    let mut out = Vec::new();
    // The current run: its first node, its length, and whether it runs backwards.
    let mut run: Option<(u32, u32, bool)> = None;
    for n in region.node_list(layout.nodes) {
        run = match run {
            Some((start, len, back)) if !back && n == start + len => Some((start, len + 1, false)),
            Some((start, len, back)) if (back || len == 1) && start.checked_sub(len) == Some(n) => {
                Some((start, len + 1, true))
            }
            Some((start, len, back)) => {
                out.push(if back {
                    range(start + 1 - len, len, true)
                } else {
                    range(start, len, false)
                });
                Some((n, 1, false))
            }
            None => Some((n, 1, false)),
        };
    }
    if let Some((start, len, back)) = run {
        out.push(if back {
            range(start + 1 - len, len, true)
        } else {
            range(start, len, false)
        });
    }
    out
}

fn spans(map: &ChannelMap, controller: ControllerId, port: Option<u16>) -> Vec<TargetRange> {
    map.controllers
        .iter()
        .filter(|c| c.controller == controller)
        .flat_map(|c| &c.spans)
        .filter(|s| port.is_none_or(|p| s.port == p))
        .map(|s| TargetRange {
            frame_offset: s.frame_offset,
            pixels: s.pixels,
            channels_per_pixel: s.channels_per_pixel,
            reverse: s.reverse,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Generator, Group, Port, PortSlot, Prop, Protocol, ShapeSource};

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        )
    }

    fn show() -> (Show, Prop, Prop, Controller) {
        let mut show = Show::new("t");
        let a = line("A", 4);
        let b = line("B", 2);
        let mut controller = Controller::new("C", "10.0.0.1", Protocol::Ddp);
        let mut p1 = Port::new(1);
        p1.slots.push(PortSlot::new(a.id));
        let mut p2 = Port::new(2);
        let mut reversed = PortSlot::new(b.id);
        reversed.reverse = true;
        p2.slots.push(reversed);
        controller.ports = vec![p1, p2];
        let mut group = Group::new("G");
        group.members = vec![b.id.into(), a.id.into()];
        show.props = vec![a.clone(), b.clone()];
        show.groups.push(group);
        show.controllers.push(controller.clone());
        (show, a, b, controller)
    }

    #[test]
    fn groups_light_submodel_members_in_member_order() {
        let (mut show, a, b, _) = show();
        // A's pixels 1-2 forwards, then 4..3 backwards (A has 4 pixels: 0..3).
        let part = pf_model::Region::nodes(
            "Part",
            vec![vec![
                Some(pf_model::NodeRun::new(1, 2)),
                Some(pf_model::NodeRun::new(3, 3)),
                Some(pf_model::NodeRun::new(0, 0)),
            ]],
        );
        let member = pf_model::RegionRef {
            prop: a.id,
            region: part.id,
        };
        show.props[0].regions.push(part);
        show.groups[0].members = vec![member.into(), b.id.into()];
        let (map, _) = pf_mapping::map_show(&show);
        let range = |frame_offset, pixels, reverse| TargetRange {
            frame_offset,
            pixels,
            channels_per_pixel: 3,
            reverse,
        };
        assert_eq!(
            resolve_target(&show, &map, &Target::Group(show.groups[0].id)),
            vec![range(3, 3, false), range(0, 1, false), range(12, 2, false)]
        );
        // Backwards runs play backwards.
        let RegionKind::Nodes { lines, .. } = &mut show.props[0].regions[0].kind else {
            unreachable!()
        };
        *lines = vec![vec![Some(pf_model::NodeRun::new(3, 1))]];
        let (map, _) = pf_mapping::map_show(&show);
        assert_eq!(
            resolve_target(&show, &map, &Target::Group(show.groups[0].id)),
            vec![range(3, 3, true), range(12, 2, false)]
        );
    }

    #[test]
    fn resolves_each_target_kind_in_order() {
        let (show, _, b, controller) = show();
        let (map, _) = pf_mapping::map_show(&show);
        let range = |frame_offset, pixels, reverse| TargetRange {
            frame_offset,
            pixels,
            channels_per_pixel: 3,
            reverse,
        };

        assert_eq!(
            resolve_target(&show, &map, &Target::Show),
            vec![range(0, 4, false), range(12, 2, false)]
        );
        assert_eq!(
            resolve_target(&show, &map, &Target::Prop(b.id)),
            vec![range(12, 2, false)]
        );
        let group = show.groups[0].id;
        assert_eq!(
            resolve_target(&show, &map, &Target::Group(group)),
            vec![range(12, 2, false), range(0, 4, false)]
        );
        assert_eq!(
            resolve_target(&show, &map, &Target::Controller(controller.id)),
            vec![range(0, 4, false), range(12, 2, true)]
        );
        assert_eq!(
            resolve_target(
                &show,
                &map,
                &Target::Port {
                    controller: controller.id,
                    port: 2
                }
            ),
            vec![range(12, 2, true)]
        );
        assert!(resolve_target(&show, &map, &Target::Prop(PropId::new())).is_empty());
    }
}
