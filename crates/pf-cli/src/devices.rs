//! `pixelflow discover` and `pixelflow device`: read-only device discovery and inspection.

use anyhow::{Context, Result};
use pf_devices::{
    Device, DeviceConfig, DeviceInput, DiscoverOptions, Discovery, HttpClient, ImportPlan, plan_import,
};
use pf_model::Show;
use std::fmt::Write;
use std::process::ExitCode;
use std::time::Duration;

#[derive(clap::Args)]
pub struct DiscoverArgs {
    /// Also check this address (repeatable).
    #[arg(long = "host")]
    pub hosts: Vec<String>,
    /// Skip the HTTP sweep of the local subnet.
    #[arg(long)]
    pub no_sweep: bool,
    /// Print JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args)]
pub struct DeviceArgs {
    /// The controller's IP address or hostname.
    pub address: String,
    /// Print JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

fn client() -> HttpClient {
    HttpClient::new(Duration::from_millis(1500))
}

pub fn discover(args: &DiscoverArgs) -> Result<ExitCode> {
    let options = DiscoverOptions {
        sweep: !args.no_sweep,
        extra_hosts: args.hosts.clone(),
        ..DiscoverOptions::default()
    };
    eprintln!("Looking for controllers…");
    let sweep = HttpClient::with_connect_timeout(Duration::from_millis(400), Duration::from_millis(1500));
    let found = pf_devices::discover(&client(), &sweep, &options);
    if args.json {
        outln!("{}", serde_json::to_string_pretty(&found)?)?;
    } else {
        out!("{}", discovery_table(&found))?;
    }
    Ok(ExitCode::SUCCESS)
}

pub fn device(args: &DeviceArgs) -> Result<ExitCode> {
    let http = client();
    let device = pf_devices::identify(&http, &args.address, None)
        .with_context(|| format!("could not identify {}", args.address))?;
    let config = pf_devices::read_config(&http, &device)
        .with_context(|| format!("could not read the configuration of {}", args.address))?;
    let plan = plan_import(&device, &config, &Show::new("Preview"));
    if args.json {
        outln!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "device": device, "config": config, "import": plan })
            )?
        )?;
    } else {
        out!("{}", describe(&device, &config, &plan))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// A table of discovered devices, plus controllers that were listed but silent, and addresses
/// that asked for a password.
pub fn discovery_table(found: &Discovery) -> String {
    let mut out = if found.devices.is_empty() {
        "No controllers found. Try --host <address> if you know where one is.\n".to_string()
    } else {
        format!("Found {} controller(s):\n", found.devices.len())
    };
    for d in &found.devices {
        let _ = writeln!(
            out,
            "  {:<16} {:<7} {:<24} {:<18} {}",
            d.address,
            format!("{:?}", d.kind).to_uppercase(),
            d.name,
            d.model,
            d.firmware
        );
    }
    for s in &found.silent {
        let _ = writeln!(
            out,
            "  {:<16} not responding — {} lists it as \"{}\". Is it powered on and connected?",
            s.address, s.listed_by, s.description
        );
    }
    for address in &found.locked {
        let _ = writeln!(
            out,
            "  {address:<16} asks for a password, so PixelFlow can't read it. Turn off its UI/API password and scan again."
        );
    }
    out
}

/// A plain-text description of a device, its configuration, and the import preview.
pub fn describe(device: &Device, config: &DeviceConfig, plan: &ImportPlan) -> String {
    let mut out = format!(
        "{} — {} at {}\n  Firmware: {}\n",
        device.name, device.model, device.address, device.firmware
    );
    if let Some(mode) = &device.mode {
        let _ = writeln!(out, "  Mode: {mode}");
    }
    let input = match &config.input {
        DeviceInput::Ddp => "DDP".to_string(),
        DeviceInput::Sacn {
            start_universe,
            channels_per_universe,
            universe_count,
        } => {
            format!("sACN, universes {start_universe}+ ({universe_count} × {channels_per_universe} channels)")
        }
        DeviceInput::Unsupported { description } => format!("{description} (not supported yet)"),
    };
    let _ = writeln!(out, "  Receives: {input}");
    for port in &config.ports {
        for s in &port.strings {
            let _ = writeln!(
                out,
                "  Port {:>2}: {:>5} px  {:<4} nulls {:<3} {}{}",
                port.number,
                s.pixels,
                format!("{:?}", s.color_order).to_uppercase(),
                s.null_pixels,
                if s.reverse { "reversed " } else { "" },
                s.name.as_deref().unwrap_or("")
            );
        }
    }
    for d in &config.destinations {
        let _ = writeln!(
            out,
            "  Sends {} channels by {} to {} ({})",
            d.channels, d.protocol, d.address, d.description
        );
    }
    for note in &plan.notes {
        let _ = writeln!(out, "  Note: {note}");
    }
    if plan.can_import {
        let _ = writeln!(
            out,
            "Importing would add controller \"{}\" with {} port(s) and {} prop(s).",
            plan.controller.name,
            plan.controller.ports.len(),
            plan.props.len()
        );
    } else {
        let _ = writeln!(out, "Nothing to import from this device.");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_devices::{DeviceKind, FoundBy, PortConfig, SilentPeer, StringConfig};
    use pf_model::ColorOrder;

    fn falcon() -> Device {
        Device {
            address: "192.0.2.20".into(),
            kind: DeviceKind::Falcon,
            name: "Garage".into(),
            model: "F16v5".into(),
            firmware: "F16V5 v2.00".into(),
            mode: None,
            found_by: vec![FoundBy::WebSweep],
        }
    }

    #[test]
    fn discovery_table_lists_devices_and_silent_controllers() {
        let found = Discovery {
            devices: vec![falcon()],
            silent: vec![SilentPeer {
                address: "192.0.2.21".into(),
                description: "Falcon_B".into(),
                listed_by: "FPP".into(),
            }],
            locked: vec!["192.0.2.30".into()],
        };
        let text = discovery_table(&found);
        assert!(text.starts_with("Found 1 controller(s):\n"), "{text}");
        assert!(text.contains("192.0.2.20       FALCON  Garage"), "{text}");
        assert!(
            text.contains("192.0.2.21       not responding — FPP lists it as \"Falcon_B\""),
            "{text}"
        );
        assert!(
            text.contains("192.0.2.30       asks for a password, so PixelFlow can't read it."),
            "{text}"
        );
        assert!(discovery_table(&Discovery::default()).starts_with("No controllers found."));
    }

    #[test]
    fn describe_shows_input_strings_and_the_import_preview() {
        let config = DeviceConfig {
            input: DeviceInput::Sacn {
                start_universe: 7,
                channels_per_universe: 510,
                universe_count: 2,
            },
            ports: vec![PortConfig {
                number: 1,
                strings: vec![StringConfig {
                    name: Some("Arch".into()),
                    pixels: 50,
                    color_order: ColorOrder::Grb,
                    null_pixels: 1,
                    reverse: true,
                    brightness: 100,
                    gamma: 1.0,
                    smart_receiver: None,
                }],
                max_pixels: None,
            }],
            destinations: vec![],
            notes: vec![],
        };
        let plan = plan_import(&falcon(), &config, &Show::new("t"));
        let text = describe(&falcon(), &config, &plan);
        assert!(
            text.contains("Receives: sACN, universes 7+ (2 × 510 channels)"),
            "{text}"
        );
        assert!(
            text.contains("Port  1:    50 px  GRB  nulls 1   reversed Arch"),
            "{text}"
        );
        assert!(
            text.contains("Importing would add controller \"Garage\" with 1 port(s) and 1 prop(s)."),
            "{text}"
        );
    }
}
