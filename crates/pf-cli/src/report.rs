//! Human-readable text output.

use pf_mapping::{Addressing, ChannelAddress, ChannelMap};
use pf_model::{Severity, Show, ValidationReport};
use std::fmt::Write;

/// One-paragraph overview of a show.
pub fn summary(show: &Show, map: &ChannelMap) -> String {
    let pixels: u64 = show.props.iter().map(|p| u64::from(p.node_count())).sum();
    let (props, controllers, universes) = (show.props.len(), show.controllers.len(), map.universe_count());
    format!(
        "{}\n  {props} {} · {} {} · {controllers} {} · {universes} {}\n\n",
        show.name,
        plural(props, "prop"),
        thousands(pixels),
        plural(usize::try_from(pixels).unwrap_or(usize::MAX), "pixel"),
        plural(controllers, "controller"),
        plural(universes, "universe"),
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

/// How a vendor sequence's items map onto the show: what's mapped (most effects first) and
/// what isn't, with the share of effects that will come in.
pub fn mapping(inspection: &pf_xlights::vendor::Inspection, mapping: &pf_xlights::vendor::Mapping) -> String {
    let mut out = String::new();
    if inspection.sequences.len() > 1 {
        let _ = writeln!(
            out,
            "Sequences in the package: {} (importing {})",
            inspection.sequences.join(", "),
            inspection.sequence
        );
    }
    if !inspection.has_layout {
        let _ = writeln!(
            out,
            "No vendor layout in the package: types are guessed from names."
        );
    }
    let mapped_to = |name: &str| mapping.targets(name).filter(|t| !t.is_empty());
    let total: usize = inspection.items.iter().map(|i| i.effects).sum();
    let mapped: Vec<_> = inspection
        .items
        .iter()
        .filter(|i| mapped_to(&i.name).is_some())
        .collect();
    let mapped_effects: usize = mapped.iter().map(|i| i.effects).sum();
    let _ = writeln!(
        out,
        "Mapped {} of {} items ({}% of {} effects)",
        mapped.len(),
        inspection.items.len(),
        (mapped_effects * 100).checked_div(total).unwrap_or(100),
        thousands(total as u64)
    );
    let mut items: Vec<_> = inspection.items.iter().zip(&inspection.suggestions).collect();
    items.sort_by_key(|(i, _)| std::cmp::Reverse(i.effects));
    let describe = |i: &pf_xlights::vendor::VendorItem| {
        format!(
            "{} ({}{}, {} {})",
            match &i.parent {
                Some(parent) => format!("{}/{}", pf_xlights::sequence::unxml_safe(parent), i.label),
                None => i.label.clone(),
            },
            format!("{:?}", i.kind).to_lowercase(),
            match i.ptype {
                pf_xlights::vendor::PropType::Other => String::new(),
                t => format!(" {}", format!("{t:?}").to_lowercase()),
            },
            i.effects,
            if i.effects == 1 { "effect" } else { "effects" }
        )
    };
    for (item, suggestion) in &items {
        if let Some(targets) = mapped_to(&item.name) {
            let why = if suggestion.targets.as_slice() == targets {
                format!("  [{:?} {:.2}]", suggestion.reason, suggestion.confidence).to_lowercase()
            } else {
                String::new()
            };
            let _ = writeln!(out, "  {} -> {}{why}", describe(item), targets.join(", "));
        }
    }
    let unmapped: Vec<_> = items
        .iter()
        .filter(|(i, _)| mapped_to(&i.name).is_none() && i.effects > 0)
        .collect();
    if !unmapped.is_empty() {
        let _ = writeln!(out, "Not mapped:");
        for (item, suggestion) in unmapped {
            let hint = match suggestion.targets.first() {
                Some(t) => format!("  (maybe {t}? {:.2})", suggestion.confidence),
                None => String::new(),
            };
            let _ = writeln!(out, "  {}{hint}", describe(item));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, ShapeSource};

    #[test]
    fn summary_uses_singular_for_one() {
        let mut show = Show::new("Tiny");
        let prop = Prop::new(
            "A",
            ShapeSource::Generator(Generator::Line {
                nodes: 1,
                length: 1.0,
            }),
        );
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(prop.id));
        let mut controller = Controller::new("C", "10.0.0.2", Protocol::Ddp);
        controller.ports.push(port);
        show.props.push(prop);
        show.controllers.push(controller);
        let (map, _) = pf_mapping::map_show(&show);
        let text = summary(&show, &map);
        assert!(
            text.contains("1 prop · 1 pixel · 1 controller · 0 universes"),
            "{text}"
        );
    }

    #[test]
    fn thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_150), "1,150");
        assert_eq!(thousands(200_000), "200,000");
    }
}
