//! Turning a device's configuration into a controller and starter props.

use crate::config::{DeviceConfig, DeviceInput};
use crate::device::{Device, DeviceKind};
use pf_model::{
    AdapterKind, Controller, Generator, Port, PortSlot, Prop, Protocol, SacnConfig, ShapeSource, Show,
    UniverseSize, Vec3,
};
use serde::Serialize;
use std::collections::HashSet;

/// What importing a device would add to the show.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub controller: Controller,
    /// One starter prop per configured string, in wiring order.
    pub props: Vec<Prop>,
    /// Plain-language notes about anything that wasn't imported exactly.
    pub notes: Vec<String>,
    /// A controller with this address is already in the show.
    pub already_in_show: bool,
    /// False when there is nothing to import (no pixel ports).
    pub can_import: bool,
}

fn unique(base: &str, taken: &mut HashSet<String>) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base} {n}");
        n += 1;
    }
    taken.insert(name.clone());
    name
}

/// Plans an import of `device` with `config` into `show` (nothing is changed yet).
pub fn plan_import(device: &Device, config: &DeviceConfig, show: &Show) -> ImportPlan {
    let mut notes = config.notes.clone();
    let mut controller_names: HashSet<String> = show.controllers.iter().map(|c| c.name.clone()).collect();
    let mut prop_names: HashSet<String> = show.props.iter().map(|p| p.name.clone()).collect();

    let protocol = match &config.input {
        DeviceInput::Ddp | DeviceInput::Unsupported { .. } => Protocol::Ddp,
        DeviceInput::Sacn {
            start_universe,
            channels_per_universe,
            ..
        } => {
            let universe_size = if *channels_per_universe == 512 {
                UniverseSize::Channels512
            } else {
                if *channels_per_universe != 510 {
                    notes.push(format!(
                        "The controller uses {channels_per_universe} channels per universe; PixelFlow uses 510."
                    ));
                }
                UniverseSize::Channels510
            };
            Protocol::Sacn(SacnConfig {
                start_universe: Some(*start_universe),
                universe_size,
                ..SacnConfig::default()
            })
        }
    };
    let adapter = match device.kind {
        DeviceKind::Fpp => AdapterKind::Fpp,
        DeviceKind::Falcon => AdapterKind::Falcon,
        DeviceKind::Wled => AdapterKind::Wled,
    };
    let controller_name = unique(&device.name, &mut controller_names);
    let mut controller = Controller::new(controller_name.clone(), device.address.clone(), protocol);
    controller.adapter = adapter;

    let mut props = Vec::new();
    let mut skipped_nulls = Vec::new();
    let mut controller_applied = Vec::new();
    for port_config in &config.ports {
        let mut port = Port::new(port_config.number);
        let several = port_config.strings.len() > 1;
        for (i, string) in port_config.strings.iter().enumerate() {
            let fallback = if several {
                format!("{controller_name} Port {} String {}", port_config.number, i + 1)
            } else {
                format!("{controller_name} Port {}", port_config.number)
            };
            let name = unique(string.name.as_deref().unwrap_or(&fallback), &mut prop_names);
            let mut prop = Prop::new(
                name,
                ShapeSource::Generator(Generator::Line {
                    nodes: string.pixels,
                    length: (string.pixels as f32 * 0.05).max(1.0),
                }),
            );
            prop.color_order = string.color_order;
            prop.transform.position = Vec3::new(0.0, -(props.len() as f32) * 0.5, 0.0);
            let mut slot = PortSlot::new(prop.id);
            // The controller skips its own null pixels; sending dark pixels too would shift every later one.
            if string.null_pixels > 0 {
                skipped_nulls.push(format!(
                    "Port {} \"{}\": {}",
                    port_config.number, prop.name, string.null_pixels
                ));
            }
            // The controller applies reverse, brightness, and gamma itself; the slot keeps its
            // defaults so PixelFlow doesn't apply them a second time.
            let mut applied = Vec::new();
            if string.reverse {
                applied.push("reversed".to_string());
            }
            if string.brightness != 100 {
                applied.push(format!("{}% brightness", string.brightness));
            }
            if (string.gamma - 1.0).abs() > f32::EPSILON {
                applied.push(format!("gamma {}", string.gamma));
            }
            if !applied.is_empty() {
                controller_applied.push(format!(
                    "Port {} \"{}\": {}",
                    port_config.number,
                    prop.name,
                    applied.join(", ")
                ));
            }
            slot.smart_receiver = string.smart_receiver;
            port.slots.push(slot);
            props.push(prop);
        }
        controller.ports.push(port);
    }
    if !skipped_nulls.is_empty() {
        notes.push(format!(
            "The controller skips its own null pixels ({}), so PixelFlow won't send data for them.",
            skipped_nulls.join(", ")
        ));
    }
    if !controller_applied.is_empty() {
        notes.push(format!(
            "The controller applies its own settings ({}), so PixelFlow sends unadjusted data.",
            controller_applied.join("; ")
        ));
    }
    ImportPlan {
        already_in_show: show.controllers.iter().any(|c| c.address == device.address),
        can_import: !props.is_empty(),
        controller,
        props,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PortConfig, StringConfig};
    use pf_model::ColorOrder;

    fn device() -> Device {
        Device {
            address: "192.0.2.20".into(),
            kind: DeviceKind::Falcon,
            name: "Garage Falcon".into(),
            model: "F16v5".into(),
            firmware: "1.0".into(),
            mode: None,
            found_by: vec![],
        }
    }

    fn string(name: Option<&str>, pixels: u32) -> StringConfig {
        StringConfig {
            name: name.map(String::from),
            pixels,
            color_order: ColorOrder::Grb,
            null_pixels: 1,
            reverse: true,
            brightness: 50,
            gamma: 2.2,
            smart_receiver: Some(1),
        }
    }

    fn config() -> DeviceConfig {
        DeviceConfig {
            input: DeviceInput::Sacn {
                start_universe: 7,
                channels_per_universe: 510,
                universe_count: 4,
            },
            ports: vec![
                PortConfig {
                    number: 1,
                    strings: vec![string(Some("Arch"), 50)],
                },
                PortConfig {
                    number: 3,
                    strings: vec![string(None, 100), string(None, 20)],
                },
            ],
            destinations: vec![],
            notes: vec!["note".into()],
        }
    }

    #[test]
    fn builds_a_controller_with_ports_slots_and_props() {
        let plan = plan_import(&device(), &config(), &Show::new("t"));
        assert!(plan.can_import && !plan.already_in_show);
        let c = &plan.controller;
        assert_eq!(c.name, "Garage Falcon");
        assert_eq!(c.address, "192.0.2.20");
        assert_eq!(c.adapter, AdapterKind::Falcon);
        assert!(matches!(
            c.protocol,
            Protocol::Sacn(SacnConfig {
                start_universe: Some(7),
                ..
            })
        ));
        assert_eq!(c.ports.iter().map(|p| p.number).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(c.ports[1].slots.len(), 2);
        let names: Vec<_> = plan.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Arch",
                "Garage Falcon Port 3 String 1",
                "Garage Falcon Port 3 String 2"
            ]
        );
        assert_eq!(plan.props[1].node_count(), 100);
        assert_eq!(plan.props[1].color_order, ColorOrder::Grb);
        let slot = &c.ports[0].slots[0];
        assert_eq!(slot.prop, plan.props[0].id);
        assert_eq!(
            (slot.null_pixels, slot.reverse, slot.brightness, slot.gamma),
            (0, false, None, None)
        );
        assert_eq!(slot.smart_receiver, Some(1));
        assert_eq!(plan.notes.len(), 3);
        assert_eq!(plan.notes[0], "note");
        assert_eq!(
            plan.notes[1],
            "The controller skips its own null pixels (Port 1 \"Arch\": 1, Port 3 \"Garage Falcon Port 3 String 1\": 1, Port 3 \"Garage Falcon Port 3 String 2\": 1), so PixelFlow won't send data for them."
        );
        assert_eq!(
            plan.notes[2],
            "The controller applies its own settings (Port 1 \"Arch\": reversed, 50% brightness, gamma 2.2; Port 3 \"Garage Falcon Port 3 String 1\": reversed, 50% brightness, gamma 2.2; Port 3 \"Garage Falcon Port 3 String 2\": reversed, 50% brightness, gamma 2.2), so PixelFlow sends unadjusted data."
        );
    }

    #[test]
    fn names_never_clash_with_the_show_and_existing_addresses_are_flagged() {
        let mut show = Show::new("t");
        show.controllers
            .push(Controller::new("Garage Falcon", "192.0.2.20", Protocol::Ddp));
        show.props.push(Prop::new(
            "Arch",
            ShapeSource::Generator(Generator::Line {
                nodes: 1,
                length: 1.0,
            }),
        ));
        let plan = plan_import(&device(), &config(), &show);
        assert_eq!(plan.controller.name, "Garage Falcon 2");
        assert_eq!(plan.props[0].name, "Arch 2");
        assert!(plan.already_in_show);
    }

    #[test]
    fn nothing_to_import_without_ports() {
        let mut config = config();
        config.ports.clear();
        config.input = DeviceInput::Ddp;
        let plan = plan_import(&device(), &config, &Show::new("t"));
        assert!(!plan.can_import);
        assert_eq!(plan.controller.protocol, Protocol::Ddp);
    }
}
