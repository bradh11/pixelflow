//! Turning a device's configuration into a controller and starter props.

use crate::config::{Destination, DeviceConfig, DeviceInput};
use crate::device::{Device, DeviceKind};
use pf_model::{
    AdapterKind, ColorOrder, Controller, Generator, Port, PortSlot, Prop, Protocol, SacnConfig,
    SequenceChannels, ShapeSource, Show, UniverseSize, Vec3,
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

/// A controller added from an FPP's output list: no ports yet, but it knows its sequence channels.
/// Importing the real device at its address fills it in rather than adding another copy.
pub fn is_placeholder(controller: &Controller) -> bool {
    controller.ports.is_empty() && controller.sequence_channels.is_some()
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
            let universe_size = UniverseSize::new(u32::from(*channels_per_universe)).unwrap_or_else(|| {
                notes.push(format!(
                    "The controller uses {channels_per_universe} channels per universe, but a universe carries 1 to 512, so PixelFlow uses 510."
                ));
                UniverseSize::CHANNELS_510
            });
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
        // The port's pixel limit, when the device said what it is.
        port.max_pixels = port_config.max_pixels;
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
            // The controller reorders colors for its own strings, so PixelFlow sends plain RGB
            // (RGBW for 4-channel strings) rather than reordering a second time.
            prop.color_order = if string.color_order.channels_per_pixel() == 4 {
                ColorOrder::Rgbw
            } else {
                ColorOrder::Rgb
            };
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
            slot.controller_color_order = Some(string.color_order);
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
    let at_address = || show.controllers.iter().filter(|c| c.address == device.address);
    let already_in_show = at_address().any(|c| !is_placeholder(c));
    if !already_in_show && let Some(placeholder) = at_address().next() {
        notes.push(format!(
            "Fills in {}, added from your FPP's output list.",
            placeholder.name
        ));
    }
    ImportPlan {
        already_in_show,
        can_import: !props.is_empty(),
        controller,
        props,
        notes,
    }
}

/// Plans adding a controller that an FPP sends to, from the FPP's output list. Works even when
/// the controller isn't answering; its strings aren't known, so it has no ports yet. Importing
/// the controller itself later fills them in.
pub fn plan_destination_import(destination: &Destination, show: &Show) -> ImportPlan {
    let mut controller_names: HashSet<String> = show.controllers.iter().map(|c| c.name.clone()).collect();
    let base = if destination.description.trim().is_empty() {
        destination.address.as_str()
    } else {
        destination.description.trim()
    };
    let name = unique(base, &mut controller_names);
    let mut notes = Vec::new();
    let universe_size = match destination.universe_size {
        None => UniverseSize::CHANNELS_510,
        Some(size) => UniverseSize::new(u32::from(size)).unwrap_or_else(|| {
            notes.push(format!(
                "The FPP sends {size} channels per universe, but a universe carries 1 to 512, so PixelFlow uses 510."
            ));
            UniverseSize::CHANNELS_510
        }),
    };
    let sacn = |multicast| {
        Protocol::Sacn(SacnConfig {
            start_universe: destination.start_universe,
            universe_size,
            multicast,
            ..SacnConfig::default()
        })
    };
    let protocol = match destination.protocol.as_str() {
        "DDP" => Some(Protocol::Ddp),
        "sACN unicast" => Some(sacn(false)),
        "sACN multicast" => Some(sacn(true)),
        _ => None,
    };
    let already_in_show = show.controllers.iter().any(|c| c.address == destination.address);
    let Some(protocol) = protocol else {
        return ImportPlan {
            controller: Controller::new(name, destination.address.clone(), Protocol::Ddp),
            props: Vec::new(),
            notes: vec![format!("PixelFlow can't send {} yet.", destination.protocol)],
            already_in_show,
            can_import: false,
        };
    };
    if destination.uneven_universes && matches!(protocol, Protocol::Sacn(_)) {
        notes.push(
            "The FPP lists several universe ranges for this controller that aren't one continuous run \
             of the same size, so check its universes before running a show."
                .to_string(),
        );
    }
    notes.push(format!(
        "PixelFlow adds {name} from the FPP's output list. Its strings aren't known yet: import the \
         controller itself once it's online to add them."
    ));
    let mut controller = Controller::new(name, destination.address.clone(), protocol);
    controller.sequence_channels = (destination.channels > 0).then_some(SequenceChannels {
        start: destination.start_channel.max(1),
        count: destination.channels,
        raw_ddp_offsets: destination.ddp_raw && protocol == Protocol::Ddp,
    });
    ImportPlan {
        controller,
        props: Vec::new(),
        notes,
        already_in_show,
        can_import: true,
    }
}

/// An output target an FPP sends to that setting up the show leaves out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSkip {
    pub name: String,
    pub address: String,
    pub reason: String,
}

/// What "Set up my show from this FPP" would add: the FPP's own outputs (when it has pixel ports
/// and isn't in the show yet), and a controller for each output target it sends to, with the
/// sequence channels the FPP sends there. Applied as one undo step.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FppSetupPlan {
    pub own: Option<ImportPlan>,
    /// One per output target not in the show yet, in the FPP's order.
    pub controllers: Vec<Controller>,
    pub skipped: Vec<SetupSkip>,
    /// Plain-language notes about anything that wasn't planned exactly.
    pub notes: Vec<String>,
}

impl FppSetupPlan {
    /// The controller addresses it adds, in order (to check it's still the plan that was shown).
    pub fn addresses(&self) -> Vec<String> {
        self.own
            .iter()
            .map(|p| p.controller.address.clone())
            .chain(self.controllers.iter().map(|c| c.address.clone()))
            .collect()
    }
}

/// Plans setting up `show` from an FPP (`device`, read as `config`); nothing is changed yet.
pub fn plan_fpp_setup(device: &Device, config: &DeviceConfig, show: &Show) -> FppSetupPlan {
    let mut working = show.clone();
    let own = if config.ports.is_empty() || show.controllers.iter().any(|c| c.address == device.address) {
        None
    } else {
        let plan = plan_import(device, config, &working);
        plan.can_import.then(|| {
            working.controllers.push(plan.controller.clone());
            working.props.extend(plan.props.iter().cloned());
            plan
        })
    };
    let mut controllers = Vec::new();
    let mut skipped = Vec::new();
    let mut notes = Vec::new();
    for destination in &config.destinations {
        let name = if destination.description.trim().is_empty() {
            destination.address.clone()
        } else {
            destination.description.trim().to_string()
        };
        let skip = |reason: String| SetupSkip {
            name: name.clone(),
            address: destination.address.clone(),
            reason,
        };
        if let Some(existing) = show.controllers.iter().find(|c| c.address == destination.address) {
            skipped.push(skip(format!("Already in your show as {}.", existing.name)));
            continue;
        }
        if working
            .controllers
            .iter()
            .any(|c| c.address == destination.address)
        {
            skipped.push(skip(format!(
                "The FPP lists {} more than once; it's added once.",
                destination.address
            )));
            continue;
        }
        let plan = plan_destination_import(destination, &working);
        if !plan.can_import {
            skipped.push(skip(plan.notes.join(" ")));
            continue;
        }
        // The last note says the strings aren't known yet, which the page says once for all.
        notes.extend(plan.notes.iter().take(plan.notes.len() - 1).cloned());
        working.controllers.push(plan.controller.clone());
        controllers.push(plan.controller);
    }
    FppSetupPlan {
        own,
        controllers,
        skipped,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Destination, PortConfig, StringConfig};
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
                    max_pixels: None,
                },
                PortConfig {
                    number: 3,
                    strings: vec![string(None, 100), string(None, 20)],
                    max_pixels: None,
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
        // The controller reorders colors for its strings, so PixelFlow sends plain RGB.
        assert_eq!(plan.props[1].color_order, ColorOrder::Rgb);
        let slot = &c.ports[0].slots[0];
        assert_eq!(slot.prop, plan.props[0].id);
        assert_eq!(
            (slot.null_pixels, slot.reverse, slot.brightness, slot.gamma),
            (0, false, None, None)
        );
        assert_eq!(slot.smart_receiver, Some(1));
        // The controller's own color order is remembered on the slot, for "Send setup" later.
        assert_eq!(slot.controller_color_order, Some(ColorOrder::Grb));
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
    fn ports_take_the_pixel_limit_the_device_reported() {
        let mut config = config();
        config.ports[0].max_pixels = Some(704);
        let plan = plan_import(&device(), &config, &Show::new("t"));
        let limits: Vec<_> = plan.controller.ports.iter().map(|p| p.max_pixels).collect();
        assert_eq!(limits, vec![Some(704), None], "no limit is guessed");
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

    fn destination(protocol: &str) -> Destination {
        Destination {
            address: "192.0.2.20".into(),
            description: "Falcon_F16V5_B9F5".into(),
            protocol: protocol.into(),
            channels: 6147,
            start_channel: 1,
            start_universe: Some(7),
            universe_size: None,
            ddp_raw: false,
            uneven_universes: false,
        }
    }

    #[test]
    fn an_fpp_destination_becomes_a_controller_without_ports() {
        let plan = plan_destination_import(&destination("DDP"), &Show::new("t"));
        assert!(plan.can_import && !plan.already_in_show);
        assert_eq!(plan.controller.name, "Falcon_F16V5_B9F5");
        assert_eq!(plan.controller.address, "192.0.2.20");
        assert_eq!(plan.controller.protocol, Protocol::Ddp);
        assert!(plan.controller.ports.is_empty() && plan.props.is_empty());
        assert_eq!(
            plan.controller.sequence_channels,
            Some(SequenceChannels {
                start: 1,
                count: 6147,
                raw_ddp_offsets: false,
            })
        );
        assert!(plan.notes[0].contains("once it's online"), "{:?}", plan.notes);
    }

    #[test]
    fn sacn_destinations_keep_their_universe_and_unsupported_ones_cant_be_added() {
        let plan = plan_destination_import(&destination("sACN multicast"), &Show::new("t"));
        let Protocol::Sacn(sacn) = &plan.controller.protocol else {
            panic!("expected sACN, got {:?}", plan.controller.protocol);
        };
        assert_eq!((sacn.start_universe, sacn.multicast), (Some(7), true));

        let art_net = plan_destination_import(&destination("Art-Net"), &Show::new("t"));
        assert!(!art_net.can_import);
        assert_eq!(art_net.notes, vec!["PixelFlow can't send Art-Net yet."]);
    }

    #[test]
    fn a_destination_already_in_the_show_is_flagged_and_unnamed_ones_use_the_address() {
        let mut show = Show::new("t");
        show.controllers
            .push(Controller::new("Falcon_F16V5_B9F5", "192.0.2.20", Protocol::Ddp));
        let plan = plan_destination_import(&destination("DDP"), &show);
        assert!(plan.already_in_show);
        assert_eq!(plan.controller.name, "Falcon_F16V5_B9F5 2");
        let unnamed = Destination {
            description: String::new(),
            ..destination("DDP")
        };
        assert_eq!(
            plan_destination_import(&unnamed, &Show::new("t")).controller.name,
            "192.0.2.20"
        );
    }

    #[test]
    fn sacn_universe_sizes_come_from_the_fpp() {
        let universe_size = |size: Option<u16>| {
            let destination = Destination {
                universe_size: size,
                ..destination("sACN unicast")
            };
            let plan = plan_destination_import(&destination, &Show::new("t"));
            let Protocol::Sacn(sacn) = plan.controller.protocol else {
                panic!("expected sACN");
            };
            (sacn.universe_size, plan.notes)
        };
        assert_eq!(universe_size(Some(512)).0, UniverseSize::CHANNELS_512);
        assert_eq!(universe_size(Some(510)).0, UniverseSize::CHANNELS_510);
        assert_eq!(universe_size(None).0, UniverseSize::CHANNELS_510);
        for size in [1, 15, 170, 384] {
            let (got, notes) = universe_size(Some(size));
            assert_eq!(got.channels(), size);
            assert!(!notes.iter().any(|n| n.contains("per universe")), "{notes:?}");
        }
        for size in [0, 600] {
            let (got, notes) = universe_size(Some(size));
            assert_eq!(got, UniverseSize::CHANNELS_510);
            assert!(
                notes[0].contains(&format!(
                    "The FPP sends {size} channels per universe, but a universe carries 1 to 512"
                )),
                "{notes:?}"
            );
        }
    }

    #[test]
    fn controller_universe_sizes_come_from_the_device() {
        let sized = |channels_per_universe: u16| {
            let config = DeviceConfig {
                input: DeviceInput::Sacn {
                    start_universe: 7,
                    channels_per_universe,
                    universe_count: 4,
                },
                ..config()
            };
            let plan = plan_import(&device(), &config, &Show::new("t"));
            let Protocol::Sacn(sacn) = plan.controller.protocol else {
                panic!("expected sACN");
            };
            (sacn.universe_size.channels(), plan.notes)
        };
        let (size, notes) = sized(384);
        assert_eq!(size, 384);
        assert!(!notes.iter().any(|n| n.contains("per universe")), "{notes:?}");
        let (size, notes) = sized(1000);
        assert_eq!(size, 510);
        assert!(
            notes.iter().any(|n| n
                .contains("The controller uses 1000 channels per universe, but a universe carries 1 to 512")),
            "{notes:?}"
        );
    }

    #[test]
    fn uneven_merged_universes_get_a_note() {
        let destination = Destination {
            uneven_universes: true,
            ..destination("sACN unicast")
        };
        let plan = plan_destination_import(&destination, &Show::new("t"));
        assert!(plan.can_import);
        assert!(
            plan.notes.iter().any(|n| n.contains("continuous run")),
            "{:?}",
            plan.notes
        );
    }

    #[test]
    fn raw_ddp_destinations_remember_their_offset_mode() {
        let raw = Destination {
            start_channel: 6148,
            ddp_raw: true,
            ..destination("DDP")
        };
        let plan = plan_destination_import(&raw, &Show::new("t"));
        assert_eq!(
            plan.controller.sequence_channels,
            Some(SequenceChannels {
                start: 6148,
                count: 6147,
                raw_ddp_offsets: true,
            })
        );
    }

    #[test]
    fn importing_over_a_placeholder_says_it_fills_it_in() {
        let mut show = Show::new("t");
        let mut placeholder = Controller::new("Falcon_F16V5_B9F5", "192.0.2.20", Protocol::Ddp);
        placeholder.sequence_channels = Some(SequenceChannels {
            start: 1,
            count: 6147,
            raw_ddp_offsets: false,
        });
        show.controllers.push(placeholder);
        let plan = plan_import(&device(), &config(), &show);
        assert!(!plan.already_in_show);
        assert_eq!(
            plan.notes.last().unwrap(),
            "Fills in Falcon_F16V5_B9F5, added from your FPP's output list."
        );

        // A port-less controller that didn't come from an FPP is a real duplicate.
        show.controllers[0].sequence_channels = None;
        let plan = plan_import(&device(), &config(), &show);
        assert!(plan.already_in_show);
        assert!(plan.notes.iter().all(|n| !n.starts_with("Fills in")));
    }

    fn player() -> (Device, DeviceConfig) {
        let fpp = Device {
            address: "192.0.2.10".into(),
            kind: DeviceKind::Fpp,
            name: "FPP".into(),
            model: "Pi 3 Model B+".into(),
            firmware: "FPP 9.5.3".into(),
            mode: Some("player".into()),
            found_by: vec![],
        };
        let garage = Destination {
            address: "192.0.2.21".into(),
            description: "".into(),
            start_channel: 6148,
            channels: 900,
            ..destination("sACN unicast")
        };
        let config = DeviceConfig {
            input: DeviceInput::Ddp,
            ports: vec![],
            destinations: vec![
                destination("DDP"),
                garage,
                destination("sACN unicast"),
                Destination {
                    address: "192.0.2.30".into(),
                    ..destination("Art-Net")
                },
            ],
            notes: vec![],
        };
        (fpp, config)
    }

    #[test]
    fn setting_up_from_an_fpp_adds_each_output_target_with_its_channels() {
        let (fpp, config) = player();
        let mut show = Show::new("t");
        show.controllers
            .push(Controller::new("Falcon_F16V5_B9F5", "192.0.2.99", Protocol::Ddp));
        let plan = plan_fpp_setup(&fpp, &config, &show);
        assert!(plan.own.is_none());
        let added: Vec<_> = plan
            .controllers
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.address.as_str(),
                    c.sequence_channels.map(|s| (s.start, s.count)),
                )
            })
            .collect();
        assert_eq!(
            added,
            vec![
                // Named apart from the show's controller of that name.
                ("Falcon_F16V5_B9F5 2", "192.0.2.20", Some((1, 6147))),
                ("192.0.2.21", "192.0.2.21", Some((6148, 900))),
            ]
        );
        assert_eq!(plan.addresses(), vec!["192.0.2.20", "192.0.2.21"]);
        let skipped: Vec<_> = plan
            .skipped
            .iter()
            .map(|s| (s.address.as_str(), s.reason.as_str()))
            .collect();
        assert_eq!(
            skipped,
            vec![
                (
                    "192.0.2.20",
                    "The FPP lists 192.0.2.20 more than once; it's added once."
                ),
                ("192.0.2.30", "PixelFlow can't send Art-Net yet."),
            ]
        );
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);

        // What's in the show already is left alone.
        show.controllers.push(plan.controllers[0].clone());
        let again = plan_fpp_setup(&fpp, &config, &show);
        assert_eq!(again.addresses(), vec!["192.0.2.21"]);
        assert_eq!(
            again.skipped[0].reason,
            "Already in your show as Falcon_F16V5_B9F5 2."
        );
    }

    #[test]
    fn an_fpp_with_its_own_outputs_adds_them_too() {
        let (fpp, mut config) = player();
        config.ports = self::config().ports;
        let plan = plan_fpp_setup(&fpp, &config, &Show::new("t"));
        let own = plan.own.as_ref().expect("its own outputs");
        assert_eq!(own.controller.address, "192.0.2.10");
        assert_eq!(own.props.len(), 3);
        assert_eq!(plan.addresses()[0], "192.0.2.10");
    }
}
