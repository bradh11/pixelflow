//! Adapters, discovery, and import planning against recorded and documented device responses.

use pf_devices::testing::{
    FALCON, FPP, FPP_HAT, WLED, assert_no_secret_endpoints, falcon_query, fpp_only, network,
};
use pf_devices::{
    DeviceInput, DeviceKind, DiscoverOptions, FoundBy, discover, identify, plan_import, read_config,
};
use pf_model::{AdapterKind, ColorOrder, Protocol, SacnConfig, Show};

#[test]
fn identifies_each_kind_from_its_home_page() {
    let http = network();
    let fpp = identify(&http, FPP, None).unwrap();
    assert_eq!(
        (fpp.kind, fpp.name.as_str(), fpp.model.as_str()),
        (DeviceKind::Fpp, "FPP", "Pi 3 Model B+")
    );
    assert_eq!(fpp.firmware, "FPP 9.3-4-g97360ca2");
    assert_eq!(fpp.mode.as_deref(), Some("player"));
    let falcon = identify(&http, FALCON, None).unwrap();
    assert_eq!(
        (falcon.kind, falcon.name.as_str(), falcon.model.as_str()),
        (DeviceKind::Falcon, "Falcon_F16V5_B9F5", "F16v5")
    );
    assert_eq!(falcon.firmware, "F16V5 v2.00");
    let wled = identify(&http, WLED, None).unwrap();
    assert_eq!(
        (wled.kind, wled.name.as_str(), wled.firmware.as_str()),
        (DeviceKind::Wled, "Porch WLED", "WLED 0.15.0")
    );
    assert!(identify(&http, "192.0.2.99", None).is_err());
    assert_no_secret_endpoints(&http);
}

#[test]
fn fpp_player_reports_its_destinations_and_nothing_to_import() {
    let http = network();
    let device = identify(&http, FPP, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert!(config.ports.is_empty());
    assert_eq!(config.destinations.len(), 1);
    assert_eq!(config.destinations[0].address, FALCON);
    assert_eq!(config.destinations[0].protocol, "DDP");
    assert_eq!(config.destinations[0].channels, 6147);
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert!(!plan.can_import);
    assert!(plan.notes[0].contains("Import those instead"));
    assert_no_secret_endpoints(&http);
}

#[test]
fn fpp_with_a_pixel_hat_imports_its_strings() {
    let http = network();
    let device = pf_devices::fpp::probe(&http, FPP_HAT).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert_eq!(config.ports.len(), 1, "empty strings are skipped");
    let strings = &config.ports[0].strings;
    assert_eq!(strings.len(), 2);
    assert_eq!((strings[0].pixels, strings[0].null_pixels), (150, 1));
    assert_eq!(
        (strings[1].color_order, strings[1].reverse, strings[1].brightness),
        (ColorOrder::Grb, true, 50)
    );
    assert!((strings[1].gamma - 2.2).abs() < 1e-6);
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert!(plan.can_import);
    assert_eq!(plan.controller.adapter, AdapterKind::Fpp);
    assert_eq!(
        plan.props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["Roof Line", "Gutter"]
    );
}

#[test]
fn falcon_strings_ports_and_ddp_mode() {
    let http = network();
    let device = identify(&http, FALCON, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert_eq!(config.input, DeviceInput::Ddp);
    assert_eq!(
        config.ports.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 3],
        "empty port 4 skipped"
    );
    let port1 = &config.ports[0].strings;
    assert_eq!((port1[0].pixels, port1[0].color_order), (100, ColorOrder::Grb));
    assert_eq!(port1[0].name.as_deref(), Some("Mega Tree 1"));
    assert_eq!(
        (port1[1].reverse, port1[1].null_pixels, port1[1].brightness),
        (true, 2, 50)
    );
    assert!((port1[1].gamma - 2.3).abs() < 1e-6);
    assert_eq!(config.ports[1].strings[0].smart_receiver, Some(1));
    assert!(config.notes.iter().any(|n| n.contains("white-first")));
    assert!(config.notes.iter().any(|n| n.contains("grouping")));
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert_eq!(plan.controller.adapter, AdapterKind::Falcon);
    assert_eq!(plan.controller.protocol, Protocol::Ddp);
    assert_eq!(plan.props.len(), 3);
}

#[test]
fn falcon_in_e131_mode_reads_its_universes() {
    let st1 = include_str!("../fixtures/falcon/st1.json").replace(r#""O":2"#, r#""O":0"#);
    let http = network().with_post(FALCON, "/api", &falcon_query("ST", 1), &st1);
    let device = identify(&http, FALCON, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert_eq!(
        config.input,
        DeviceInput::Sacn {
            start_universe: 7,
            channels_per_universe: 510,
            universe_count: 6
        }
    );
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert!(matches!(
        plan.controller.protocol,
        Protocol::Sacn(SacnConfig {
            start_universe: Some(7),
            ..
        })
    ));
}

#[test]
fn wled_outputs_including_rgbw() {
    let http = network();
    let device = identify(&http, WLED, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert_eq!(config.ports.len(), 2);
    assert_eq!(
        (
            config.ports[0].strings[0].color_order,
            config.ports[0].strings[0].null_pixels
        ),
        (ColorOrder::Grb, 1)
    );
    assert_eq!(
        (
            config.ports[1].strings[0].color_order,
            config.ports[1].strings[0].reverse
        ),
        (ColorOrder::Rgbw, true)
    );
    assert!(config.notes.is_empty());
}

#[test]
fn discovery_expands_fpp_peers_and_reports_silent_ones() {
    let http = network();
    let options = DiscoverOptions {
        ping: false,
        mdns: false,
        sweep: false,
        extra_hosts: vec![FPP.to_string(), WLED.to_string()],
        ..DiscoverOptions::default()
    };
    let found = discover(&http, &http, &options);
    let addresses: Vec<_> = found
        .devices
        .iter()
        .map(|d| (d.address.as_str(), d.kind))
        .collect();
    assert_eq!(
        addresses,
        vec![
            (FPP, DeviceKind::Fpp),
            (FALCON, DeviceKind::Falcon),
            (WLED, DeviceKind::Wled)
        ]
    );
    assert_eq!(found.devices[1].found_by, vec![FoundBy::FppPeer]);
    assert_eq!(found.devices[0].found_by, vec![FoundBy::Manual]);
    assert!(found.silent.is_empty());

    // The same FPP, but its Falcon doesn't answer.
    let quiet = fpp_only();
    let found = discover(&quiet, &quiet, &options);
    assert_eq!(found.devices.len(), 1);
    assert_eq!(found.silent.len(), 1);
    assert_eq!(found.silent[0].address, FALCON);
    assert_eq!(found.silent[0].description, "Falcon_F16V5_B9F5");
    assert_eq!(found.silent[0].listed_by, "FPP");
    assert_no_secret_endpoints(&http);
}
