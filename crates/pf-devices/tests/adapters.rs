//! Adapters, discovery, and import planning against recorded and documented device responses.

use pf_devices::testing::{
    FALCON, FPP, FPP_HAT, WLED, assert_no_secret_endpoints, falcon_query, falcon_synthetic, fpp_only, network,
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
    assert_eq!(falcon.firmware, "F16V5 Bld 32");
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
    assert_eq!(
        (
            config.destinations[0].start_channel,
            config.destinations[0].start_universe
        ),
        (1, None)
    );
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert!(!plan.can_import);
    assert!(plan.notes[0].contains("passes its sequence on to the controllers it sends to"));
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
        plan.controller.ports[0].slots[0].null_pixels, 0,
        "the controller skips its own nulls"
    );
    assert!(plan.notes.contains(
        &"The controller skips its own null pixels (Port 1 \"Roof Line\": 1), so PixelFlow won't send data for them."
            .to_string()
    ));
    let gutter = &plan.controller.ports[0].slots[1];
    assert_eq!(
        (gutter.reverse, gutter.brightness, gutter.gamma),
        (false, None, None),
        "the controller applies these itself"
    );
    assert!(plan.notes.contains(
        &"The controller applies its own settings (Port 1 \"Gutter\": reversed, 50% brightness, gamma 2.2), so PixelFlow sends unadjusted data."
            .to_string()
    ));
    assert_eq!(
        plan.props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["Roof Line", "Gutter"]
    );
    assert_no_secret_endpoints(&http);
}

#[test]
fn a_real_f16v5_is_recognized_and_imported_as_its_web_ui_shows_it() {
    // Recorded read-only from an F16V5 on firmware Bld 32 (scrubbed). Its home page is a 404, so
    // it's recognized from /status.xml; its settings come in one ST page marked final.
    let http = network();
    let device = identify(&http, FALCON, None).unwrap();
    assert_eq!(
        (device.kind, device.model.as_str(), device.firmware.as_str()),
        (DeviceKind::Falcon, "F16v5", "F16V5 Bld 32"),
        "BR (165 on this firmware) is not a port count"
    );
    let config = read_config(&http, &device).unwrap();
    assert_eq!(config.input, DeviceInput::Ddp, "O 2 is DDP");
    let strings: Vec<_> = config
        .ports
        .iter()
        .flat_map(|p| p.strings.iter().map(move |s| (p.number, s)))
        .collect();
    assert_eq!(
        strings
            .iter()
            .map(|(port, s)| (*port, s.name.as_deref().unwrap_or(""), s.pixels))
            .collect::<Vec<_>>(),
        vec![
            (1, "Pillar Right", 600),
            (2, "Pillar Left", 600),
            (3, "Door Frame Front", 340),
            (4, "Roof Door", 238),
            (6, "Door Arch", 271),
        ],
        "p is 0-based; empty ports 5 and 7–32 are skipped"
    );
    for (_, s) in &strings {
        assert_eq!(
            (
                s.color_order,
                s.brightness,
                s.null_pixels,
                s.reverse,
                s.smart_receiver
            ),
            (ColorOrder::Rgb, 40, 0, false, None)
        );
        assert!((s.gamma - 1.0).abs() < 1e-6);
    }
    // Board mode 4 (16 local ports + smart receiver chains, 32 in all): 1,024 pixels a port, as the
    // board's own status (k0, k1) says.
    assert!(config.ports.iter().all(|p| p.max_pixels == Some(1024)));
    // sc is 0-based and contiguous from 0 (the FPP feeding this board sends 6,147 DDP channels
    // from channel 1), so there's nothing to warn about.
    assert!(config.notes.is_empty(), "{:?}", config.notes);
    let total: u32 = strings.iter().map(|(_, s)| s.pixels * 3).sum();
    assert_eq!(total, 6147);
    let plan = plan_import(&device, &config, &Show::new("t"));
    assert_eq!(plan.controller.protocol, Protocol::Ddp);
    assert_eq!(plan.props.len(), 5);
    // Only reads: the home page, /status.xml, and JSON API queries.
    for request in http.requests() {
        assert!(
            request.starts_with("GET ") || request.contains(r#"{"T":"Q","#),
            "{request}"
        );
    }
    assert!(
        !http.requests().iter().any(|r| r.contains(r#""B":1"#)),
        "ST page 0 was final"
    );
    assert_no_secret_endpoints(&http);
}

#[test]
fn falcon_settings_split_over_two_pages_are_merged() {
    // Older V4 firmware (xLights' recording): the mode is only in ST page 0, the board mode in page 1.
    let st0 = include_str!("../fixtures/falcon/synthetic/st0.json").replace(r#""O":2"#, r#""O":0"#);
    let http = falcon_synthetic().with_post(FALCON, "/api", &falcon_query("ST", 0), &st0);
    let config = falcon_config(&http).unwrap();
    assert!(
        matches!(config.input, DeviceInput::Sacn { .. }),
        "{:?}",
        config.input
    );
    let st1 = include_str!("../fixtures/falcon/synthetic/st1.json");
    assert!(
        st1.contains(r#""A":0,"B":0"#) && !st1.contains(r#""O""#),
        "fixture changed"
    );
    let http = falcon_synthetic().with_post(
        FALCON,
        "/api",
        &falcon_query("ST", 1),
        &st1.replace(r#""A":0,"B":0"#, r#""A":0,"B":10"#),
    );
    let config = falcon_config(&http).unwrap();
    assert!(config.ports.iter().all(|p| p.max_pixels == Some(704)));
}

#[test]
fn falcon_strings_that_do_not_start_on_the_first_channel_are_flagged() {
    const LATE: &str = "strings start at its channel";
    let sp0 = include_str!("../fixtures/falcon/sp0.json");
    let st0 = include_str!("../fixtures/falcon/st0.json");
    let shifted = sp0
        .replace(r#""sc":0,"n":600"#, r#""sc":30,"n":600"#)
        .replace(r#""sc":1800,"#, r#""sc":1830,"#)
        .replace(r#""sc":3600,"#, r#""sc":3630,"#)
        .replace(r#""sc":4620,"#, r#""sc":4650,"#)
        .replace(r#""sc":5334,"#, r#""sc":5364,"#);
    for (st, sp, note) in [
        (st0.to_string(), shifted.clone(), Some(31)),
        // The controller's own first channel (ps) is where PixelFlow's channel 1 lands.
        (st0.replace(r#""ps":0"#, r#""ps":30"#), shifted.clone(), None),
        (st0.to_string(), sp0.to_string(), None),
    ] {
        let http = network()
            .with_post(FALCON, "/api", &falcon_query("ST", 0), &st)
            .with_post(FALCON, "/api", &falcon_query("SP", 0), &sp);
        let notes = falcon_config(&http).unwrap().notes;
        let found: Vec<_> = notes.iter().filter(|n| n.contains(LATE)).collect();
        match note {
            Some(channel) => {
                assert_eq!(found.len(), 1, "{notes:?}");
                assert!(found[0].contains(&format!("channel {channel},")), "{notes:?}");
            }
            None => assert!(found.is_empty(), "{notes:?}"),
        }
        assert!(!notes.iter().any(|n| n.contains("continuous block")), "{notes:?}");
    }
    // Universe addressing: start channels are per universe, so they aren't compared.
    let http = network().with_post(
        FALCON,
        "/api",
        &falcon_query("ST", 0),
        &st0.replace(r#""A":0,"B":4"#, r#""A":1,"B":4"#),
    );
    let notes = falcon_config(&http).unwrap().notes;
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("by universe"), "{notes:?}");
}

#[test]
fn falcon_strings_ports_and_ddp_mode() {
    let http = falcon_synthetic();
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
    let st0 = include_str!("../fixtures/falcon/synthetic/st0.json").replace(r#""O":2"#, r#""O":0"#);
    let http = falcon_synthetic().with_post(FALCON, "/api", &falcon_query("ST", 0), &st0);
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
fn falcon_e131_inputs_that_are_not_one_even_run_are_flagged() {
    const UNEVEN: &str = "input universes aren't one continuous run";
    let st0 = include_str!("../fixtures/falcon/synthetic/st0.json").replace(r#""O":2"#, r#""O":0"#);
    let in0 = include_str!("../fixtures/falcon/synthetic/in.json");
    let second = r#"{"p":"e","u":11,"c":510,"uc":2}"#;
    assert!(in0.contains(second), "fixture changed");
    for (entry, uneven) in [
        (second, false),
        (r#"{"p":"e","u":12,"c":510,"uc":2}"#, true),
        (r#"{"p":"e","u":11,"c":512,"uc":2}"#, true),
    ] {
        let http = falcon_synthetic()
            .with_post(FALCON, "/api", &falcon_query("ST", 0), &st0)
            .with_post(
                FALCON,
                "/api",
                &falcon_query("IN", 0),
                &in0.replace(second, entry),
            );
        let config = falcon_config(&http).unwrap();
        assert_eq!(
            config.notes.iter().filter(|n| n.contains(UNEVEN)).count(),
            usize::from(uneven),
            "{entry}: {:?}",
            config.notes
        );
    }
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

#[test]
fn discovery_never_probes_peers_that_are_not_plain_addresses() {
    let sync = r#"{"systems":[
        {"address":"fpp.local","hostname":"a","local":0},
        {"address":"10.0.0.5:80","hostname":"b","local":0},
        {"address":"239.255.0.1","hostname":"c","local":0}]}"#;
    let outputs = r#"{"channelOutputs":[{"enabled":1,"universes":[
        {"address":"127.0.0.1","active":1,"description":"d","type":4,"channelCount":3},
        {"address":"255.255.255.255","active":1,"description":"e","type":4,"channelCount":3}]}]}"#;
    let http = fpp_only()
        .with_get(FPP, "/api/fppd/multiSyncSystems", sync)
        .with_get(FPP, "/api/channel/output/universeOutputs", outputs);
    let options = DiscoverOptions {
        ping: false,
        mdns: false,
        sweep: false,
        extra_hosts: vec![FPP.to_string()],
        ..DiscoverOptions::default()
    };
    let found = discover(&http, &http, &options);
    assert_eq!(found.devices.len(), 1);
    assert!(found.silent.is_empty());
    for request in http.requests() {
        for bad in [
            "fpp.local",
            "10.0.0.5",
            "239.255.0.1",
            "127.0.0.1",
            "255.255.255.255",
        ] {
            assert!(!request.contains(bad), "{request}");
        }
    }
}

#[test]
fn password_protected_controllers_are_reported_not_dropped() {
    // An FPP whose API asks for a password, and a typed address whose whole UI does.
    let http = fpp_only()
        .with_get_status(FPP, "/api/system/info", 401)
        .with_get_status("192.0.2.50", "/", 401);
    let options = DiscoverOptions {
        ping: false,
        mdns: false,
        sweep: false,
        extra_hosts: vec![FPP.to_string(), "192.0.2.50".to_string()],
        ..DiscoverOptions::default()
    };
    let found = discover(&http, &http, &options);
    assert!(found.devices.is_empty());
    assert_eq!(found.locked, vec![FPP.to_string(), "192.0.2.50".to_string()]);
    assert!(found.silent.is_empty());

    // A controller the FPP lists, behind a password.
    let http = network().with_get_status(FALCON, "/", 401);
    let options = DiscoverOptions {
        extra_hosts: vec![FPP.to_string()],
        ..options
    };
    let found = discover(&http, &http, &options);
    assert_eq!(found.devices.len(), 1);
    assert_eq!(found.locked, vec![FALCON.to_string()]);
    assert!(found.silent.is_empty(), "it answered, so it isn't silent");
    assert_no_secret_endpoints(&http);
}

#[test]
fn a_peer_that_answers_but_is_not_a_controller_is_not_silent() {
    let http = fpp_only().with_get(FALCON, "/", "<html>some other web page</html>");
    let options = DiscoverOptions {
        ping: false,
        mdns: false,
        sweep: false,
        extra_hosts: vec![FPP.to_string()],
        ..DiscoverOptions::default()
    };
    let found = discover(&http, &http, &options);
    assert_eq!(found.devices.len(), 1);
    assert!(found.silent.is_empty());
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
    let sp0 = include_str!("../fixtures/falcon/synthetic/sp0.json");
    assert!(sp0.contains(from), "fixture changed: {from}");
    falcon_synthetic().with_post(FALCON, "/api", &falcon_query("SP", 0), &sp0.replace(from, to))
}

#[test]
fn fpp_with_a_channel_gap_warns_once() {
    let http = hat_with(r#""startChannel": 450"#, r#""startChannel": 460"#);
    let config = hat_config(&http).unwrap();
    assert_eq!(config.notes.iter().filter(|n| n.contains(LAYOUT)).count(), 1);
    assert_no_secret_endpoints(&http);
}

#[test]
fn nulls_do_not_count_toward_the_channel_layout() {
    // Roof Line has a null pixel; the next string still starts exactly 150 pixels * 3 channels later.
    let config = hat_config(&network().with_get(
        FPP_HAT,
        HAT_STRINGS,
        include_str!("../fixtures/fpp-hat/api_channel_output_co-pixelStrings.json"),
    ))
    .unwrap();
    assert!(config.ports[0].strings[0].null_pixels > 0);
    assert!(!config.notes.iter().any(|n| n.contains(LAYOUT)));
}

#[test]
fn falcon_empty_strings_on_invalid_ports_raise_no_note() {
    let http = falcon_with_sp0(
        r#""p":0,"s":1,"r":0,"v":1,"u":0,"sc":300,"n":50"#,
        r#""p":-1,"s":1,"r":0,"v":1,"u":0,"sc":300,"n":0"#,
    );
    let config = falcon_config(&http).unwrap();
    assert!(
        config.notes.iter().all(|n| !n.contains("port number")),
        "{:?}",
        config.notes
    );
    assert_no_secret_endpoints(&http);
}

#[test]
fn strings_on_port_numbers_that_cannot_exist_are_skipped_with_a_note() {
    let skipped = "A string on port number 65536 was skipped; PixelFlow can't use that port number.";
    let http = hat_with(r#""portNumber": 0"#, r#""portNumber": 65535"#);
    let config = hat_config(&http).unwrap();
    assert!(config.ports.is_empty(), "{:?}", config.ports);
    assert!(config.notes.contains(&skipped.to_string()), "{:?}", config.notes);

    let http = falcon_with_sp0(
        r#""p":0,"s":1,"r":0,"v":1,"u":0,"sc":300,"n":50"#,
        r#""p":65535,"s":1,"r":0,"v":1,"u":0,"sc":300,"n":50"#,
    );
    let config = falcon_config(&http).unwrap();
    let strings = |c: &pf_devices::DeviceConfig| c.ports.iter().map(|p| p.strings.len()).sum::<usize>();
    assert_eq!(
        strings(&config) + 1,
        strings(&falcon_config(&falcon_synthetic()).unwrap())
    );
    assert!(config.notes.contains(&skipped.to_string()), "{:?}", config.notes);
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
    assert_no_secret_endpoints(&http);
}

#[test]
fn huge_null_counts_are_clamped_with_a_note() {
    let http = hat_with(r#""nullNodes": 1"#, r#""nullNodes": 99999"#);
    let config = hat_config(&http).unwrap();
    assert_eq!(config.ports[0].strings[0].null_pixels, pf_model::MAX_NULL_PIXELS);
    assert!(config.notes.iter().any(|n| n.contains("null pixels")));
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
}

#[test]
fn wled_bus_type_range_edges() {
    let bus = |start: i64, ty: i64| {
        format!(r#"{{"start":{start},"len":10,"pin":[2],"order":1,"rev":false,"skip":0,"type":{ty}}}"#)
    };
    // The last one-wire type and the first two-wire type are pixels; the types just past them aren't.
    let cfg = format!(
        r#"{{"hw":{{"led":{{"ins":[{},{},{},{}]}}}},"if":{{"live":{{"en":true}}}}}}"#,
        bus(0, 39),
        bus(10, 48),
        bus(20, 40),
        bus(30, 47)
    );
    let http = network().with_get(WLED, "/json/cfg", &cfg);
    let config = read_config(&http, &identify(&http, WLED, None).unwrap()).unwrap();
    let kept: Vec<_> = config.ports.iter().map(|p| p.number).collect();
    assert_eq!(kept, vec![1, 2]);
    assert!(
        config
            .notes
            .contains(&"Output 3 isn't a pixel output (type 40); it was skipped.".to_string())
    );
    assert!(
        config
            .notes
            .contains(&"Output 4 isn't a pixel output (type 47); it was skipped.".to_string())
    );
}

const WLED_GAP: &str = "outputs don't follow one another";

fn wled_with_starts(first: i64, second: i64) -> pf_devices::DeviceConfig {
    let cfg = include_str!("../fixtures/wled/cfg.json");
    let (a, b) = (r#""start":0,"len":60"#, r#""start":60,"len":60"#);
    assert!(cfg.contains(a) && cfg.contains(b), "fixture changed");
    let cfg = cfg
        .replace(a, &format!(r#""start":{first},"len":60"#))
        .replace(b, &format!(r#""start":{second},"len":60"#));
    let http = network().with_get(WLED, "/json/cfg", &cfg);
    read_config(&http, &identify(&http, WLED, None).unwrap()).unwrap()
}

#[test]
fn wled_outputs_must_run_on_from_led_zero() {
    // WLED's `len` excludes the skipped LEDs (`skip` sacrificial LEDs come on top; wled00/
    // bus_manager.cpp creates `count + skip` LEDs and offsets every pixel by `skip`), so output 2
    // starts right after output 1's 60 LEDs even though output 1 also skips one.
    let config = wled_with_starts(0, 60);
    assert_eq!(
        (
            config.ports[0].strings[0].pixels,
            config.ports[0].strings[0].null_pixels
        ),
        (60, 1)
    );
    assert!(
        config.notes.iter().all(|n| !n.contains(WLED_GAP)),
        "{:?}",
        config.notes
    );
    for (first, second) in [(0, 70), (0, 61), (10, 70), (60, 0)] {
        let config = wled_with_starts(first, second);
        assert_eq!(
            config.notes.iter().filter(|n| n.contains(WLED_GAP)).count(),
            1,
            "{first} {second}"
        );
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
    assert_no_secret_endpoints(&http);
}

#[test]
fn falcon_paging_dedupes_and_flags_a_runaway_list() {
    // A firmware that ignores the batch number and never sets the final flag.
    let sp0 = include_str!("../fixtures/falcon/synthetic/sp0.json");
    let mut http = falcon_synthetic();
    for batch in 0..70 {
        http = http.with_post(FALCON, "/api", &falcon_query("SP", batch), sp0);
    }
    let config = falcon_config(&http).unwrap();
    assert_eq!(config.ports.len(), 1);
    assert_eq!(config.ports[0].strings.len(), 2);
    assert!(config.notes.iter().any(|n| n.contains("may be incomplete")));
    assert_no_secret_endpoints(&http);
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
    assert_no_secret_endpoints(&http);
}

#[test]
fn fpp_peers_keep_the_description() {
    let sync = r#"{"systems":[{"address":"192.0.2.20","hostname":"","local":0}]}"#;
    let http = fpp_only().with_get(FPP, "/api/fppd/multiSyncSystems", sync);
    let peers = pf_devices::fpp::peers(&http, FPP);
    assert_eq!(peers, vec![(FALCON.to_string(), "Falcon_F16V5_B9F5".to_string())]);
    assert_no_secret_endpoints(&http);
}

fn hat_notes(info: &str, strings_from: &str, strings_to: &str) -> Vec<String> {
    let doc = include_str!("../fixtures/fpp-hat/api_channel_output_co-pixelStrings.json");
    let universes = include_str!("../fixtures/fpp-hat/api_channel_output_universeOutputs.json");
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get(FPP_HAT, HAT_STRINGS, &doc.replace(strings_from, strings_to))
        .with_get(FPP_HAT, "/api/channel/output/universeOutputs", universes);
    hat_config(&http).unwrap().notes
}

#[test]
fn fpp_5_and_later_warn_that_a_running_playlist_overrides_live_output() {
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json");
    let notes = hat_notes(info, "\"x\"", "\"x\"");
    let note = "If a playlist or sequence is running on this FPP, it overrides PixelFlow's live output; stop it while using PixelFlow.";
    assert!(notes.contains(&note.to_string()), "{notes:?}");
    assert!(!notes.iter().any(|n| n.contains("bridge") || n.contains("mode")));
}

#[test]
fn old_fpp_keeps_the_bridge_wording_and_never_prints_an_empty_mode() {
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json")
        .replace(r#""majorVersion": 9"#, r#""majorVersion": 4"#);
    let notes = hat_notes(&info, "\"x\"", "\"x\"");
    assert!(notes.contains(
        &"FPP is in player mode. Switch it to bridge mode to show PixelFlow's live output.".to_string()
    ));
    let info = info.replace(r#""Mode": "player", "#, "");
    let notes = hat_notes(&info, "\"x\"", "\"x\"");
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("This FPP isn't in bridge mode")),
        "{notes:?}"
    );
    assert!(!notes.iter().any(|n| n.contains("is in  mode")));
}

#[test]
fn fpp_strings_not_starting_at_channel_one_are_flagged() {
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json");
    assert!(
        !hat_notes(info, "\"x\"", "\"x\"")
            .iter()
            .any(|n| n.contains("start at channel"))
    );
    let notes = hat_notes(
        info,
        r#""startChannel": 0, "pixelCount": 150"#,
        r#""startChannel": 99, "pixelCount": 150"#,
    );
    assert!(
        notes.contains(
            &"This FPP's strings start at channel 100, but PixelFlow sends from channel 1. Set the first string to start at channel 1 on the FPP, or the strings will stay dark."
                .to_string()
        ),
        "{notes:?}"
    );
}

#[test]
fn fpp_destinations_merge_by_address_and_protocol_and_count_universes() {
    let universes = r#"{"channelOutputs":[{"enabled":1,"universes":[
        {"active":1,"address":"192.0.2.20","channelCount":510,"universeCount":4,"type":4,"description":""},
        {"active":1,"address":"192.0.2.20","channelCount":510,"type":4,"description":"Falcon"},
        {"active":1,"address":"192.0.2.20","channelCount":510,"type":1,"description":"Falcon"}]}]}"#;
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json");
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get_status(FPP_HAT, HAT_STRINGS, 404)
        .with_get(FPP_HAT, "/api/channel/output/universeOutputs", universes);
    let d = hat_config(&http).unwrap().destinations;
    assert_eq!(d.len(), 2, "{d:?}");
    assert_eq!((d[0].protocol.as_str(), d[0].channels), ("DDP", 2550));
    assert_eq!(d[0].description, "Falcon");
    assert_eq!((d[1].protocol.as_str(), d[1].channels), ("sACN unicast", 510));
    // Type 4 is DDP with raw channel numbers; sACN entries record their universe size.
    assert!(d[0].ddp_raw && !d[1].ddp_raw);
    assert_eq!((d[0].universe_size, d[1].universe_size), (None, Some(510)));
}

fn sacn_destinations(universes: &str) -> Vec<pf_devices::Destination> {
    let info = include_str!("../fixtures/fpp-hat/api_system_info.json");
    let http = FakeHttp::new()
        .with_get(FPP_HAT, "/api/system/info", info)
        .with_get_status(FPP_HAT, HAT_STRINGS, 404)
        .with_get(FPP_HAT, "/api/channel/output/universeOutputs", universes);
    hat_config(&http).unwrap().destinations
}

#[test]
fn merged_sacn_ranges_are_flagged_unless_they_run_back_to_back() {
    let entry = |id: u32, size: u32, count: u32| {
        format!(
            r#"{{"active":1,"address":"192.0.2.30","id":{id},"channelCount":{size},"universeCount":{count},"type":1,"description":"Arches"}}"#
        )
    };
    let run = |entries: &[String]| {
        let json = format!(
            r#"{{"channelOutputs":[{{"enabled":1,"universes":[{}]}}]}}"#,
            entries.join(",")
        );
        sacn_destinations(&json)
    };
    let even = run(&[entry(1, 510, 4), entry(5, 510, 2)]);
    assert_eq!(
        (even.len(), even[0].channels, even[0].uneven_universes),
        (1, 3060, false)
    );
    assert!(
        run(&[entry(1, 510, 4), entry(9, 510, 2)])[0].uneven_universes,
        "gap"
    );
    assert!(
        run(&[entry(1, 510, 4), entry(5, 512, 2)])[0].uneven_universes,
        "sizes differ"
    );
}

#[test]
fn falcon_without_a_product_code_is_treated_as_older_and_never_queried() {
    let status = include_str!("../fixtures/falcon/status.xml").replace("<p>130</p>", "");
    let http = network().with_get(FALCON, "/status.xml", &status);
    // Recognized from an older Falcon's home page; status.xml alone isn't enough without the code.
    assert!(identify(&http, FALCON, None).is_err());
    let device = identify(&http, FALCON, Some(DeviceKind::Falcon)).unwrap();
    let before = http.requests().len();
    let err = read_config(&http, &device).unwrap_err().to_string();
    assert!(err.contains("older Falcon controller"), "{err}");
    assert!(http.requests()[before..].iter().all(|r| !r.starts_with("POST")));
}

#[test]
fn falcon_ports_carry_the_boards_pixel_limit_for_its_board_mode() {
    // Board mode 0 (16 local ports): xLights' 3,072 channels a port, 1,024 RGB pixels.
    let config = falcon_config(&falcon_synthetic()).unwrap();
    assert!(
        config.ports.iter().all(|p| p.max_pixels == Some(1024)),
        "{:?}",
        config.ports
    );

    // Board mode 10 (4 + 4 + 4 smart receiver chains, 48 ports): 2,112 channels, 704 pixels.
    let st0 = include_str!("../fixtures/falcon/st0.json");
    assert!(st0.contains(r#""A":0,"B":4"#), "fixture changed");
    let http = network().with_post(
        FALCON,
        "/api",
        &falcon_query("ST", 0),
        &st0.replace(r#""A":0,"B":4"#, r#""A":0,"B":10"#),
    );
    let config = falcon_config(&http).unwrap();
    assert!(
        config.ports.iter().all(|p| p.max_pixels == Some(704)),
        "{:?}",
        config.ports
    );
    let plan = plan_import(&identify(&http, FALCON, None).unwrap(), &config, &Show::new("t"));
    assert!(plan.controller.ports.iter().all(|p| p.max_pixels == Some(704)));
}
