//! Human-readable text output.

use pf_mapping::{Addressing, ChannelAddress, ChannelMap};
use pf_model::{Severity, Show, ValidationReport};
use std::fmt::Write;

/// One-paragraph overview of a show.
pub fn summary(show: &Show, map: &ChannelMap) -> String {
    let pixels: u64 = show.props.iter().map(|p| u64::from(p.node_count())).sum();
    format!(
        "{}\n  {} props · {} pixels · {} controllers · {} universes\n\n",
        show.name,
        show.props.len(),
        thousands(pixels),
        show.controllers.len(),
        map.universe_count()
    )
}

/// Issues, errors first, with suggested fixes.
pub fn issues(report: &ValidationReport) -> String {
    if report.issues.is_empty() {
        return "No problems found.\n".to_string();
    }
    let errors = report.count(Severity::Error);
    let warnings = report.count(Severity::Warning);
    let mut out = format!(
        "{} {}, {} {}\n\n",
        errors,
        plural(errors, "error"),
        warnings,
        plural(warnings, "warning")
    );
    let mut sorted: Vec<_> = report.issues.iter().collect();
    sorted.sort_by_key(|i| std::cmp::Reverse(i.severity));
    for issue in sorted {
        let label = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let _ = writeln!(out, "  {label:<8} {}", issue.message);
        if let Some(fix) = &issue.fix {
            let _ = writeln!(out, "           Fix: {fix}");
        }
    }
    out
}

/// Controller → port → prop listing with channel and universe ranges.
pub fn channel_map(show: &Show, map: &ChannelMap) -> String {
    let mut out = String::new();
    for (controller, output) in show.controllers.iter().zip(&map.controllers) {
        let protocol = match &output.addressing {
            Addressing::Ddp => "DDP".to_string(),
            Addressing::Sacn { universes, multicast } => {
                let mode = if *multicast { "multicast" } else { "unicast" };
                match (universes.first(), universes.last()) {
                    (Some(first), Some(last)) => {
                        format!("sACN universes {}–{} ({mode})", first.universe, last.universe)
                    }
                    _ => format!("sACN, no universes ({mode})"),
                }
            }
        };
        let _ = writeln!(out, "{}  {}  {}", controller.name, controller.address, protocol);
        let mut current_port = None;
        for span in &output.spans {
            if current_port != Some(span.port) {
                let _ = writeln!(out, "  Port {}", span.port);
                current_port = Some(span.port);
            }
            let name = show.prop(span.prop).map_or("?", |p| p.name.as_str());
            let first = span.controller_channel;
            let last = first + span.byte_len() - 1;
            let _ = writeln!(
                out,
                "    {:<20} {:>6} px   ch {:<15} {} → {}",
                name,
                span.pixels,
                format!("{}–{}", first + 1, last + 1),
                address(output.addressing.address_of(first)),
                address(output.addressing.address_of(last)),
            );
        }
    }
    out
}

fn address(address: Option<ChannelAddress>) -> String {
    match address {
        Some(ChannelAddress::Sacn { universe, channel }) => format!("U{universe}:{channel}"),
        Some(ChannelAddress::Ddp { offset }) => format!("@{offset}"),
        None => "?".to_string(),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_150), "1,150");
        assert_eq!(thousands(200_000), "200,000");
    }
}
