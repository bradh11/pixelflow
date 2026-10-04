//! Which pixels a test pattern lights, in wiring order.

use pf_mapping::ChannelMap;
use pf_model::{ControllerId, GroupId, PropId, Show};

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
            .map(|g| g.members.iter().filter_map(|m| whole_prop(*m)).collect())
            .unwrap_or_default(),
        Target::Controller(id) => spans(map, *id, None),
        Target::Port { controller, port } => spans(map, *controller, Some(*port)),
    }
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
        group.members = vec![b.id, a.id];
        show.props = vec![a.clone(), b.clone()];
        show.groups.push(group);
        show.controllers.push(controller.clone());
        (show, a, b, controller)
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
