//! What a camera-mapping capture covers: which prop node each step of the sequence lights.

use crate::output::TargetSpec;
use pf_camera_map::{Owner, PropInput};
use pf_mapping::ChannelMap;
use pf_model::{PropId, Show};
use pf_patterns::{Target, resolve_target};
use serde::Serialize;

/// A prop a capture lights (some or all of its nodes).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraMapProp {
    pub prop: PropId,
    pub name: String,
    pub nodes: u32,
    /// How many of its nodes the capture lights (fewer than `nodes` when only part of it is on
    /// the target).
    pub covered: u32,
}

/// The pixels a camera-mapping sequence lights, in sequence order.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraMapTarget {
    pub props: Vec<CameraMapProp>,
    /// Which prop (index into `props`) and node each sequence index lights.
    pub owners: Vec<Owner>,
    /// Each prop as it is in the layout now, for lining the capture up.
    pub inputs: Vec<PropInput>,
}

impl CameraMapTarget {
    pub fn pixels(&self) -> u32 {
        u32::try_from(self.owners.len()).unwrap_or(u32::MAX)
    }
}

/// The pixels `target` lights, in the order the camera-mapping pattern numbers them.
pub(crate) fn camera_map_target(show: &Show, map: &ChannelMap, target: &TargetSpec) -> CameraMapTarget {
    let mut layouts: Vec<_> = map.props.iter().collect();
    layouts.sort_by_key(|l| l.frame_offset);
    let mut props: Vec<CameraMapProp> = Vec::new();
    let mut owners = Vec::new();
    for range in resolve_target(show, map, &Target::from(target)) {
        let cpp = usize::from(range.channels_per_pixel.max(1));
        for k in 0..range.pixels {
            let node = if range.reverse { range.pixels - 1 - k } else { k };
            let offset = range.frame_offset + node as usize * cpp;
            // The prop whose bytes hold this pixel.
            let at = layouts.partition_point(|l| l.frame_offset <= offset);
            let Some(layout) = at.checked_sub(1).map(|i| layouts[i]) else {
                continue;
            };
            let cpp = usize::from(layout.channels_per_pixel.max(1));
            let prop_node = ((offset - layout.frame_offset) / cpp) as u32;
            if prop_node >= layout.nodes {
                continue;
            }
            let index = match props.iter().position(|p| p.prop == layout.prop) {
                Some(i) => i,
                None => {
                    let name = show
                        .prop(layout.prop)
                        .map_or_else(String::new, |p| p.name.clone());
                    props.push(CameraMapProp {
                        prop: layout.prop,
                        name,
                        nodes: layout.nodes,
                        covered: 0,
                    });
                    props.len() - 1
                }
            };
            props[index].covered += 1;
            owners.push(Owner {
                prop: index,
                node: prop_node,
            });
        }
    }
    let inputs = props
        .iter()
        .map(|p| {
            let prop = show.prop(p.prop);
            PropInput {
                nodes: p.nodes,
                expected: prop
                    .map(|prop| {
                        pf_geometry::world_positions(prop)
                            .into_iter()
                            .take(p.nodes as usize)
                            .map(|v| [f64::from(v.x), f64::from(v.y)])
                            .collect()
                    })
                    .unwrap_or_default(),
                color_order: prop
                    .and_then(|prop| serde_json::to_value(prop.color_order).ok())
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "RGB".into()),
            }
        })
        .collect();
    CameraMapTarget {
        props,
        owners,
        inputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{ColorOrder, Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource};

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 3.0 }),
        )
    }

    #[test]
    fn numbers_pixels_in_wiring_order_across_props() {
        let mut show = Show::new("t");
        let a = line("A", 3);
        let mut b = line("B", 2);
        b.color_order = ColorOrder::Grb;
        let mut controller = Controller::new("C", "10.0.0.1", Protocol::Ddp);
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(a.id));
        let mut back = PortSlot::new(b.id);
        back.reverse = true;
        port.slots.push(back);
        controller.ports.push(port);
        let (a_id, b_id, c_id) = (a.id, b.id, controller.id);
        show.props.extend([a, b]);
        show.controllers.push(controller);
        let (map, _) = pf_mapping::map_show(&show);

        let target = camera_map_target(
            &show,
            &map,
            &TargetSpec::Port {
                controller: c_id,
                port: 1,
            },
        );
        assert_eq!(target.pixels(), 5);
        assert_eq!(
            target.props.iter().map(|p| p.prop).collect::<Vec<_>>(),
            [a_id, b_id]
        );
        let order: Vec<(usize, u32)> = target.owners.iter().map(|o| (o.prop, o.node)).collect();
        assert_eq!(order, [(0, 0), (0, 1), (0, 2), (1, 1), (1, 0)]);
        assert_eq!(target.inputs[1].color_order, "GRB");
        assert_eq!(target.inputs[0].expected.len(), 3);
        assert_eq!(target.props[1].covered, 2);

        let one = camera_map_target(&show, &map, &TargetSpec::Prop { id: b_id });
        let order: Vec<(usize, u32)> = one.owners.iter().map(|o| (o.prop, o.node)).collect();
        assert_eq!(order, [(0, 0), (0, 1)]);
    }
}
