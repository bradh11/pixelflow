//! "In your show" defaults when importing the real F16V5 (five strings: Pillar Right 600, Pillar
//! Left 600, Door Frame Front 340, Roof Door 238, Door Arch 271 on ports 1, 2, 3, 4 and 6).

use pf_devices::testing::{FALCON, network};
use pf_devices::{Device, DeviceConfig, MatchReason, identify, plan_import, read_config};
use pf_model::{
    ColorOrder, Controller, Generator, Port, PortSlot, Prop, PropId, Protocol, ShapeSource, Show,
};

fn falcon() -> (Device, DeviceConfig) {
    let http = network();
    let device = identify(&http, FALCON, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    (device, config)
}

fn line(name: &str, nodes: u32) -> Prop {
    Prop::new(
        name,
        ShapeSource::Generator(Generator::Line {
            nodes,
            length: nodes as f32 * 0.05,
        }),
    )
}

/// The suggestions as (string key, prop name, reason).
fn suggested(show: &Show, device: &Device, config: &DeviceConfig) -> Vec<(String, String, MatchReason)> {
    plan_import(device, config, show)
        .suggested
        .into_iter()
        .map(|(key, m)| (key, show.prop(m.prop).unwrap().name.clone(), m.reason))
        .collect()
}

#[test]
fn a_controller_already_in_the_show_suggests_what_each_port_carries() {
    let (device, config) = falcon();
    let first = plan_import(&device, &config, &Show::new("t"));
    assert!(first.suggested.is_empty(), "nothing to match in an empty show");
    let mut show = Show::new("t");
    show.props = first.props.clone();
    show.controllers.push(first.controller.clone());
    // Renamed on the Layout screen since: still the prop on that port.
    show.props[0].name = "Right Column".into();
    // A loose prop with port 1's name doesn't beat what port 1 already carries.
    show.props.push(line("Pillar Right", 600));

    let plan = plan_import(&device, &config, &show);
    assert!(plan.already_in_show);
    let names = |ids: Vec<PropId>| -> Vec<String> {
        ids.iter()
            .map(|id| show.prop(*id).unwrap().name.clone())
            .collect()
    };
    assert_eq!(
        names(plan.suggested.values().map(|m| m.prop).collect()),
        [
            "Right Column",
            "Pillar Left",
            "Door Frame Front",
            "Roof Door",
            "Door Arch"
        ]
    );
    assert!(plan.suggested.values().all(|m| m.reason == MatchReason::SamePort));
    assert_eq!(
        plan.suggested.keys().collect::<Vec<_>>(),
        [
            "port1/string1",
            "port2/string1",
            "port3/string1",
            "port4/string1",
            "port6/string1"
        ]
    );
}

#[test]
fn names_match_ignoring_case_and_spaces_and_prefer_the_same_pixel_count() {
    let (device, config) = falcon();
    let mut show = Show::new("t");
    show.props = vec![
        line("pillar  right", 600),
        line("PillarLeft", 550),
        line("Pillar Left", 600),
        // Another channel width would shift every later pixel: never suggested.
        {
            let mut rgbw = line("Door Frame Front", 340);
            rgbw.color_order = ColorOrder::Rgbw;
            rgbw
        },
        line("Roof Door", 200),
        line("Garage Arch", 271),
    ];
    assert_eq!(
        suggested(&show, &device, &config),
        [
            (
                "port1/string1".into(),
                "pillar  right".into(),
                MatchReason::SameName
            ),
            (
                "port2/string1".into(),
                "Pillar Left".into(),
                MatchReason::SameName
            ),
            (
                "port4/string1".into(),
                "Roof Door".into(),
                MatchReason::SameNameOtherSize
            ),
        ]
    );
}

#[test]
fn no_prop_is_suggested_for_two_strings() {
    let (device, config) = falcon();
    let mut show = Show::new("t");
    // The show wires a prop named "Pillar Left" to the controller's port 1, where the device has
    // "Pillar Right"; the device's own "Pillar Left" (port 2) can't have it too.
    let left = line("Pillar Left", 600);
    let mut controller = Controller::new("Falcon", FALCON, Protocol::Ddp);
    let mut port = Port::new(1);
    port.slots.push(PortSlot::new(left.id));
    controller.ports.push(port);
    show.props.push(left);
    show.controllers.push(controller);
    assert_eq!(
        suggested(&show, &device, &config),
        [(
            "port1/string1".into(),
            "Pillar Left".into(),
            MatchReason::SamePort
        )]
    );
}
