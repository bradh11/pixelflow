//! `pixelflow test-pattern`: drive real controllers with a test pattern.

use crate::report;
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use pf_model::Show;
use pf_output::{
    ControllerState, OutputPlan, OutputSettings, OutputStats, Transport, UdpTransport, build_plan,
    start_output,
};
use pf_patterns::{Pattern, Preset, Rgbw, Target, TargetRange, render, resolve_target};
use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(clap::Args)]
pub struct Args {
    /// Path to a .pixelflow.json show file.
    pub show: PathBuf,
    /// Pattern to send.
    #[arg(long, value_enum, default_value_t = PatternKind::Chase)]
    pub pattern: PatternKind,
    /// Color as rrggbb or rrggbbww hex (used by solid, chase, ramp, alternate, walk).
    #[arg(long, default_value = "ffffff")]
    pub color: String,
    /// What to light: show, prop:NAME, group:NAME, controller:NAME, or port:CONTROLLER:NUMBER.
    #[arg(long, default_value = "show")]
    pub target: String,
    /// How long to run, in seconds.
    #[arg(long, default_value_t = 10.0)]
    pub seconds: f32,
    /// Local IP address to send from (picks the network interface). Default: any.
    #[arg(long)]
    pub bind: Option<IpAddr>,
    /// sACN synchronization universe (receivers then show frames in lockstep).
    #[arg(long)]
    pub sync_universe: Option<u16>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PatternKind {
    Solid,
    Cycle,
    Chase,
    Ramp,
    Alternate,
    Identify,
    Walk,
}

/// Builds the pattern from the command-line choices.
pub fn pattern(kind: PatternKind, color: Rgbw) -> Pattern {
    let preset = match kind {
        PatternKind::Solid => Preset::Solid,
        PatternKind::Cycle => Preset::Cycle,
        PatternKind::Chase => Preset::Chase,
        PatternKind::Ramp => Preset::Ramp,
        PatternKind::Alternate => Preset::Alternate,
        PatternKind::Identify => Preset::Identify,
        PatternKind::Walk => Preset::Walk,
    };
    Pattern::preset(preset, color)
}

/// Parses `show`, `prop:NAME`, `group:NAME`, `controller:NAME`, or `port:CONTROLLER:NUMBER`.
pub fn parse_target(show: &Show, text: &str) -> Result<Target> {
    if text == "show" {
        return Ok(Target::Show);
    }
    let Some((kind, rest)) = text.split_once(':') else {
        bail!(
            "unknown target '{text}'; use show, prop:NAME, group:NAME, controller:NAME, or port:CONTROLLER:NUMBER"
        );
    };
    let controller = |name: &str| {
        show.controllers
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.id)
            .with_context(|| format!("no controller named '{name}'"))
    };
    match kind {
        "prop" => show
            .props
            .iter()
            .find(|p| p.name == rest)
            .map(|p| Target::Prop(p.id))
            .with_context(|| format!("no prop named '{rest}'")),
        "group" => show
            .groups
            .iter()
            .find(|g| g.name == rest)
            .map(|g| Target::Group(g.id))
            .with_context(|| format!("no group named '{rest}'")),
        "controller" => Ok(Target::Controller(controller(rest)?)),
        "port" => {
            let (name, number) = rest
                .rsplit_once(':')
                .with_context(|| format!("port target must be port:CONTROLLER:NUMBER, got '{text}'"))?;
            let port: u16 = number
                .parse()
                .with_context(|| format!("'{number}' is not a port number"))?;
            Ok(Target::Port {
                controller: controller(name)?,
                port,
            })
        }
        other => bail!("unknown target kind '{other}'; use show, prop, group, controller, or port"),
    }
}

pub fn run(show: &Show, args: &Args) -> Result<ExitCode> {
    let mut issues = pf_model::validate_show(show);
    let (map, wiring) = pf_mapping::map_show(show);
    issues.extend(wiring);
    if issues.has_errors() {
        out!("{}", report::issues(&issues))?;
        outln!("\nFix these problems before sending output.")?;
        return Ok(ExitCode::FAILURE);
    }
    let run_for = Duration::try_from_secs_f32(args.seconds)
        .ok()
        .filter(|d| *d <= Duration::from_secs(86_400))
        .context("--seconds must be a number of seconds between 0 and 86400")?;
    let color = Rgbw::from_hex(&args.color)
        .with_context(|| format!("'{}' is not a color; use rrggbb or rrggbbww hex", args.color))?;
    let pattern = pattern(args.pattern, color);
    check_sync_universe(args.sync_universe)?;
    let targets = resolve_target(show, &map, &parse_target(show, &args.target)?);
    if targets.is_empty() {
        bail!(
            "'{}' has no pixels to light; wire its props to a controller first",
            args.target
        );
    }

    let local = SocketAddr::new(args.bind.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), 0);
    let transport =
        UdpTransport::bind(local).with_context(|| format!("could not open a UDP socket on {local}"))?;
    let settings = OutputSettings {
        sync_universe: args.sync_universe,
        ..OutputSettings::default()
    };
    // Ctrl-C (or a termination signal) ends the run early, still blacking out the controllers.
    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&interrupted);
    if let Err(e) = ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed)) {
        outln!("warning: Ctrl-C will not black out the controllers ({e})")?;
    }
    let (stats, elapsed) = drive(
        build_plan(show, &map),
        settings,
        Box::new(transport),
        &pattern,
        &targets,
        run_for,
        &interrupted,
    );
    out!("{}", summary(&stats, elapsed))?;
    Ok(ExitCode::SUCCESS)
}

/// The sACN sync universe must be a real universe, 1–63999.
fn check_sync_universe(universe: Option<u16>) -> Result<()> {
    match universe {
        Some(u) if !(1..=63_999).contains(&u) => {
            bail!("--sync-universe must be between 1 and 63999, not {u}")
        }
        _ => Ok(()),
    }
}

/// Paints `pattern` and sends it until `run_for` has passed or `interrupted` is set, then stops
/// output, which blacks out the controllers. Returns the final stats and how long it ran.
fn drive(
    plan: OutputPlan,
    settings: OutputSettings,
    transport: Box<dyn Transport>,
    pattern: &Pattern,
    targets: &[TargetRange],
    run_for: Duration,
    interrupted: &AtomicBool,
) -> (OutputStats, Duration) {
    let frame_period = Duration::from_secs_f64(1.0 / f64::from(plan.frame_rate.max(1)));
    let (mut writer, reader) = pf_frame::frame_buffers(plan.frame_len);
    // Publish frame 0 before output starts, so the first packets are never the empty buffer.
    let started = Instant::now();
    render(pattern, 0.0, targets, writer.frame_mut());
    writer.publish();
    let handle = start_output(plan, settings, reader, transport);

    while started.elapsed() < run_for && !interrupted.load(Ordering::Relaxed) {
        render(
            pattern,
            started.elapsed().as_secs_f32(),
            targets,
            writer.frame_mut(),
        );
        writer.publish();
        std::thread::sleep(frame_period);
    }
    let stats = handle.stop();
    (stats, started.elapsed())
}

/// End-of-run report: frame rate and per-controller packet counts.
pub fn summary(stats: &OutputStats, elapsed: Duration) -> String {
    let mut out = format!(
        "Sent {} frames in {:.1} s ({:.1} fps, {} late)\n",
        stats.frames,
        elapsed.as_secs_f32(),
        stats.achieved_fps,
        stats.late_frames
    );
    for c in &stats.controllers {
        let state = match c.state {
            ControllerState::Ok => "ok",
            ControllerState::Degraded => "degraded",
            ControllerState::Unresolved => "unresolved",
        };
        let _ = write!(
            out,
            "  {:<20} {:<10} {} packets, {} errors",
            c.name, state, c.packets_sent, c.send_errors
        );
        if let Some(error) = &c.last_error {
            let _ = write!(out, " (last error: {error})");
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::ControllerId;
    use pf_output::ControllerStats;

    fn demo() -> Show {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/shows/demo.pixelflow.json"
        ))
        .unwrap();
        pf_model::show_from_json(&text).unwrap()
    }

    #[test]
    fn parses_every_target_kind_and_reports_unknown_names() {
        let show = demo();
        assert_eq!(parse_target(&show, "show").unwrap(), Target::Show);
        assert_eq!(
            parse_target(&show, "prop:Mega Tree").unwrap(),
            Target::Prop(show.props[1].id)
        );
        assert_eq!(
            parse_target(&show, "group:Yard").unwrap(),
            Target::Group(show.groups[0].id)
        );
        assert_eq!(
            parse_target(&show, "controller:Porch WLED").unwrap(),
            Target::Controller(show.controllers[1].id)
        );
        assert_eq!(
            parse_target(&show, "port:Main FPP:2").unwrap(),
            Target::Port {
                controller: show.controllers[0].id,
                port: 2
            }
        );
        let err = parse_target(&show, "prop:Nope").unwrap_err().to_string();
        assert_eq!(err, "no prop named 'Nope'");
        assert!(parse_target(&show, "port:Main FPP:x").is_err());
        assert!(parse_target(&show, "bogus").is_err());
    }

    #[test]
    fn summary_lists_controllers_with_state_and_errors() {
        let stats = OutputStats {
            frames: 400,
            late_frames: 1,
            achieved_fps: 40.0,
            controllers: vec![ControllerStats {
                controller: ControllerId::new(),
                name: "Porch WLED".into(),
                state: ControllerState::Degraded,
                packets_sent: 10,
                send_errors: 3,
                last_error: Some("host unreachable".into()),
            }],
            failure: None,
        };
        let text = summary(&stats, Duration::from_secs(10));
        assert_eq!(
            text,
            "Sent 400 frames in 10.0 s (40.0 fps, 1 late)\n  Porch WLED           degraded   10 packets, 3 errors (last error: host unreachable)\n"
        );
    }
}
