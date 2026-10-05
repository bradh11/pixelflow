//! Adapters, discovery, and import planning against recorded and documented device responses.

use pf_devices::testing::{
    FALCON, FPP, FPP_HAT, WLED, assert_no_secret_endpoints, falcon_query, fpp_only, network,
};
use pf_devices::{
    DeviceInput, DeviceKind, DiscoverOptions, FakeHttp, FoundBy, discover, identify, plan_import, read_config,
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
    assert!(!config.notes.iter().any(|n| n.contains("continuous block")));
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert!(plan.can_import);
    assert_eq!(plan.controller.adapter, AdapterKind::Fpp);
    assert_eq!(
        plan.props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["Roof Line", "Gutter"]
    );
    assert_no_secret_endpoints(&http);
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
    assert!(
        !config.notes.iter().any(|n| n.contains("continuous block")),
        "fixture layout is contiguous"
    );
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert_eq!(plan.controller.adapter, AdapterKind::Falcon);
    assert_eq!(plan.controller.protocol, Protocol::Ddp);
    assert_eq!(plan.props.len(), 3);
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
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

const HAT_STRINGS: &str = "/api/channel/output/co-pixelStrings";
const LAYOUT: &str = "continuous block";

fn hat_with(from: &str, to: &str) -> FakeHttp {
    let doc = include_str!("../fixtures/fpp-hat/api_channel_output_co-pixelStrings.json");
    assert!(doc.contains(from), "fixture changed: {from}");
    network().with_get(FPP_HAT, HAT_STRINGS, &doc.replace(from, to))
}

fn hat_config(http: &FakeHttp) -> Result<pf_devices::DeviceConfig, pf_devices::DeviceError> {
    let device = pf_devices::fpp::probe(http, FPP_HAT).unwrap();
    read_config(http, &device)
}

fn falcon_config(http: &FakeHttp) -> Result<pf_devices::DeviceConfig, pf_devices::DeviceError> {
    let device = identify(http, FALCON, None).unwrap();
    read_config(http, &device)
}

fn falcon_with_sp0(from: &str, to: &str) -> FakeHttp {
    let sp0 = include_str!("../fixtures/falcon/sp0.json");
    assert!(sp0.contains(from), "fixture changed: {from}");
    network().with_post(FALCON, "/api", &falcon_query("SP", 0), &sp0.replace(from, to))
}

#[test]
fn fpp_with_a_channel_gap_warns_once() {
    let http = hat_with(r#""startChannel": 450"#, r#""startChannel": 460"#);
    let config = hat_config(&http).unwrap();
    assert_eq!(config.notes.iter().filter(|n| n.contains(LAYOUT)).count(), 1);
    assert_no_secret_endpoints(&http);
}

#[test]
fn falcon_with_a_channel_gap_warns_once() {
    let http = falcon_with_sp0(r#""sc":300"#, r#""sc":330"#);
    let config = falcon_config(&http).unwrap();
    assert_eq!(config.notes.iter().filter(|n| n.contains(LAYOUT)).count(), 1);
    assert_no_secret_endpoints(&http);
}

#[test]
fn huge_pixel_counts_are_skipped_with_a_note() {
    let http = hat_with(r#""pixelCount": 150"#, r#""pixelCount": 5000000"#);
    let config = hat_config(&http).unwrap();
    assert_eq!(config.ports[0].strings.len(), 1);
    assert!(
        config
            .notes
            .iter()
            .any(|n| n.contains("Port 1 reports 5,000,000 pixels"))
    );

    let http = falcon_with_sp0(r#""n":100"#, r#""n":5000000"#);
    let config = falcon_config(&http).unwrap();
    assert_eq!(config.ports[0].strings.len(), 1);
    assert!(
        config
            .notes
            .iter()
            .any(|n| n.contains("Port 1 reports 5,000,000 pixels"))
    );

    let cfg = include_str!("../fixtures/wled/cfg.json")
        .replace(r#""len":60,"pin":[2]"#, r#""len":5000000,"pin":[2]"#);
    let http = network().with_get(WLED, "/json/cfg", &cfg);
    let device = identify(&http, WLED, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    assert_eq!(config.ports.len(), 1);
    assert!(
        config
            .notes
            .iter()
            .any(|n| n.contains("Output 1 reports 5,000,000 pixels"))
    );
}

#[test]
fn huge_null_counts_are_clamped_with_a_note() {
    let http = hat_with(r#""nullNodes": 1"#, r#""nullNodes": 99999"#);
    let config = hat_config(&http).unwrap();
    assert_eq!(config.ports[0].strings[0].null_pixels, pf_model::MAX_NULL_PIXELS);
    assert!(config.notes.iter().any(|n| n.contains("null pixels")));
}

#[test]
fn fpp_notes_grouping_and_zig_zag() {
    let http = hat_with(
        r#""groupCount": 0, "reverse": 0"#,
        r#""groupCount": 2, "reverse": 0"#,
    );
    assert!(
        hat_config(&http)
            .unwrap()
            .notes
            .iter()
            .any(|n| n.contains("grouping"))
    );
    let http = hat_with(
        r#""zigZag": 0, "brightness": 50"#,
        r#""zigZag": 8, "brightness": 50"#,
    );
    assert!(
        hat_config(&http)
            .unwrap()
            .notes
            .iter()
            .any(|n| n.contains("zig-zag"))
    );
}

#[test]
fn fpp_read_errors_other_than_404_propagate() {
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json");
    let universes = include_str!("../fixtures/fpp-hat/api_channel_output_universeOutputs.json");
    // Strings unregistered: the fake reports "unreachable", which must not read as "no strings".
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get(FPP_HAT, "/api/channel/output/universeOutputs", universes);
    assert!(hat_config(&http).is_err());
    // Destinations failing with a server error also propagates.
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get_status(FPP_HAT, HAT_STRINGS, 404)
        .with_get_status(FPP_HAT, "/api/channel/output/universeOutputs", 500);
    assert!(hat_config(&http).is_err());
    // 404s mean "none".
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get_status(FPP_HAT, HAT_STRINGS, 404)
        .with_get_status(FPP_HAT, "/api/channel/output/universeOutputs", 404);
    assert!(hat_config(&http).unwrap().ports.is_empty());
}

#[test]
fn wled_bus_types_are_classified() {
    let bus =
        |ty: i64| format!(r#"{{"start":0,"len":10,"pin":[2],"order":1,"rev":false,"skip":0,"type":{ty}}}"#);
    let types = [22, 29, 18, 21, 28, 34, 0, 41, 66, 80, 50];
    let ins: Vec<_> = types.iter().map(|t| bus(*t)).collect();
    let cfg = format!(
        r#"{{"hw":{{"led":{{"ins":[{}]}}}},"if":{{"live":{{"en":true}}}}}}"#,
        ins.join(",")
    );
    let http = network().with_get(WLED, "/json/cfg", &cfg);
    let device = identify(&http, WLED, None).unwrap();
    let config = read_config(&http, &device).unwrap();
    let kept: Vec<_> = config
        .ports
        .iter()
        .map(|p| (p.number, p.strings[0].color_order))
        .collect();
    assert_eq!(
        kept,
        vec![(1, ColorOrder::Rgb), (2, ColorOrder::Rgbw), (11, ColorOrder::Rgb)]
    );
    let extra = "uses LEDs with extra white channels that PixelFlow doesn't support yet; it was skipped.";
    for n in [3, 4, 5, 6] {
        assert!(config.notes.contains(&format!("Output {n} {extra}")), "{n}");
    }
    for (n, t) in [(7, 0), (8, 41), (9, 66), (10, 80)] {
        assert!(config.notes.contains(&format!(
            "Output {n} isn't a pixel output (type {t}); it was skipped."
        )));
    }
}

#[test]
fn missing_brightness_means_full_and_bad_gamma_means_one() {
    let http = hat_with(r#""brightness": 100, "gamma": "1.0""#, r#""gamma": "nan""#);
    let strings = &hat_config(&http).unwrap().ports[0].strings;
    assert_eq!((strings[0].brightness, strings[0].gamma), (100, 1.0));
    let http = hat_with(r#""gamma": "2.2""#, r#""gamma": "-3""#);
    assert_eq!(hat_config(&http).unwrap().ports[0].strings[1].gamma, 1.0);

    let http = falcon_with_sp0(r#""g":10,"o":2,"b":100,"#, r#""g":0,"o":2,"#);
    let strings = &falcon_config(&http).unwrap().ports[0].strings;
    assert_eq!((strings[0].brightness, strings[0].gamma), (100, 1.0));
}

#[test]
fn falcon_paging_dedupes_and_flags_a_runaway_list() {
    // A firmware that ignores the batch number and never sets the final flag.
    let sp0 = include_str!("../fixtures/falcon/sp0.json");
    let mut http = network();
    for batch in 0..70 {
        http = http.with_post(FALCON, "/api", &falcon_query("SP", batch), sp0);
    }
    let config = falcon_config(&http).unwrap();
    assert_eq!(config.ports.len(), 1);
    assert_eq!(config.ports[0].strings.len(), 2);
    assert!(config.notes.iter().any(|n| n.contains("may be incomplete")));
}

#[test]
fn older_falcons_are_not_read() {
    let status = include_str!("../fixtures/falcon/status.xml").replace("<p>130</p>", "<p>5</p>");
    let http = network().with_get(FALCON, "/status.xml", &status);
    let device = identify(&http, FALCON, None).unwrap();
    let before = http.requests().len();
    let err = read_config(&http, &device).unwrap_err().to_string();
    assert!(
        err.contains("older Falcon controller that PixelFlow can't read yet."),
        "{err}"
    );
    assert!(http.requests()[before..].iter().all(|r| !r.starts_with("POST")));
}

#[test]
fn fpp_peers_keep_the_description() {
    let sync = r#"{"systems":[{"address":"192.0.2.20","hostname":"","local":0}]}"#;
    let http = fpp_only().with_get(FPP, "/api/fppd/multiSyncSystems", sync);
    let peers = pf_devices::fpp::peers(&http, FPP);
    assert_eq!(peers, vec![(FALCON.to_string(), "Falcon_F16V5_B9F5".to_string())]);
}
