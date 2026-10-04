//! Structural checks: broken references, out-of-range values, duplicate ids.
//! Wiring checks (capacity, universes) live in `pf-mapping`.

use crate::{Issue, IssueCode, Show, ValidationReport};
use std::collections::HashSet;
use std::hash::Hash;

/// Checks a show for structural problems.
pub fn validate_show(show: &Show) -> ValidationReport {
    let mut report = ValidationReport::default();

    check_frame_rate(show, &mut report);
    check_duplicates(
        "prop",
        show.props.iter().map(|p| (p.id, p.name.as_str())),
        &mut report,
    );
    check_duplicates(
        "group",
        show.groups.iter().map(|g| (g.id, g.name.as_str())),
        &mut report,
    );
    check_duplicates(
        "controller",
        show.controllers.iter().map(|c| (c.id, c.name.as_str())),
        &mut report,
    );
    check_props(show, &mut report);
    check_groups(show, &mut report);
    check_wiring(show, &mut report);

    report
}

fn check_frame_rate(show: &Show, report: &mut ValidationReport) {
    let rate = show.settings.frame_rate;
    if !(20..=100).contains(&rate) {
        report.push(
            Issue::error(
                IssueCode::InvalidFrameRate,
                format!("The show frame rate is {rate} fps, but it must be between 20 and 100."),
            )
            .with_fix("Set the frame rate to 40 fps."),
        );
    }
}

fn check_duplicates<'a, Id: Eq + Hash>(
    kind: &str,
    items: impl Iterator<Item = (Id, &'a str)>,
    report: &mut ValidationReport,
) {
    let mut seen = HashSet::new();
    for (id, name) in items {
        if !seen.insert(id) {
            report.push(Issue::error(
                IssueCode::DuplicateId,
                format!("The {kind} '{name}' has the same id as another {kind}."),
            ));
        }
    }
}

fn check_props(show: &Show, report: &mut ValidationReport) {
    for prop in &show.props {
        let nodes = prop.node_count();
        if nodes == 0 {
            report.push(
                Issue::warning(
                    IssueCode::EmptyProp,
                    format!("The prop '{}' has no pixels.", prop.name),
                )
                .with_fix("Give the prop at least one pixel, or delete it."),
            );
        }
        for region in &prop.regions {
            if region.ranges().iter().any(|r| !r.fits_within(nodes)) {
                report.push(
                    Issue::error(
                        IssueCode::RegionOutOfBounds,
                        format!(
                            "The region '{}' on prop '{}' refers to pixels outside the prop's {nodes} pixels.",
                            region.name, prop.name
                        ),
                    )
                    .with_fix("Edit the region so it only uses the prop's pixels."),
                );
            }
        }
    }
}

fn check_groups(show: &Show, report: &mut ValidationReport) {
    for group in &show.groups {
        let missing = group
            .members
            .iter()
            .filter(|id| show.prop(**id).is_none())
            .count();
        if missing > 0 {
            report.push(
                Issue::error(
                    IssueCode::UnknownPropReference,
                    format!(
                        "The group '{}' includes {missing} prop(s) that no longer exist.",
                        group.name
                    ),
                )
                .with_fix("Remove the missing props from the group."),
            );
        }
    }
}

fn check_wiring(show: &Show, report: &mut ValidationReport) {
    for controller in &show.controllers {
        for port in &controller.ports {
            let where_ = format!("port {} on '{}'", port.number, controller.name);
            if port.brightness > 100 {
                report.push(Issue::error(
                    IssueCode::InvalidBrightness,
                    format!(
                        "The brightness of {where_} is {}%, but it must be 0–100%.",
                        port.brightness
                    ),
                ));
            }
            for slot in &port.slots {
                let Some(prop) = show.prop(slot.prop) else {
                    report.push(
                        Issue::error(
                            IssueCode::UnknownPropReference,
                            format!("A slot on {where_} refers to a prop that no longer exists."),
                        )
                        .with_fix("Remove the slot from the port."),
                    );
                    continue;
                };
                let nodes = prop.node_count();
                let range = slot.node_range(nodes);
                if !range.fits_within(nodes) {
                    report.push(
                        Issue::error(
                            IssueCode::SegmentOutOfBounds,
                            format!(
                                "The slot for '{}' on {where_} uses pixels {}–{}, but the prop only has {nodes} pixels.",
                                prop.name,
                                range.start + 1,
                                range.end
                            ),
                        )
                        .with_fix("Edit the slot's pixel range to fit the prop."),
                    );
                }
                if slot.brightness.is_some_and(|b| b > 100) {
                    report.push(Issue::error(
                        IssueCode::InvalidBrightness,
                        format!(
                            "The brightness override for '{}' on {where_} must be 0–100%.",
                            prop.name
                        ),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Controller, Generator, Group, NodeRange, Port, PortSlot, Prop, PropId, Protocol, Region, RegionKind,
        ShapeSource,
    };

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        )
    }

    fn show_with_slot(slot: PortSlot, prop: Prop) -> Show {
        let mut show = Show::new("Test");
        let mut port = Port::new(1);
        port.slots.push(slot);
        let mut controller = Controller::new("Main", "10.0.0.2", Protocol::Ddp);
        controller.ports.push(port);
        show.props.push(prop);
        show.controllers.push(controller);
        show
    }

    #[test]
    fn valid_show_has_no_issues() {
        let prop = line("A", 10);
        let show = show_with_slot(PortSlot::new(prop.id), prop);
        assert_eq!(validate_show(&show).issues, vec![]);
    }

    #[test]
    fn each_structural_problem_is_reported() {
        type Mutate = fn(&mut Show);
        let cases: [(IssueCode, Mutate); 7] = [
            (IssueCode::InvalidFrameRate, |s| s.settings.frame_rate = 5),
            (IssueCode::DuplicateId, |s| {
                let dup = s.props[0].clone();
                s.props.push(dup);
            }),
            (IssueCode::EmptyProp, |s| s.props.push(line("Empty", 0))),
            (IssueCode::RegionOutOfBounds, |s| {
                s.props[0].regions.push(Region {
                    name: "Too far".into(),
                    kind: RegionKind::Nodes {
                        ranges: vec![NodeRange::new(5, 11)],
                    },
                })
            }),
            (IssueCode::UnknownPropReference, |s| {
                let mut group = Group::new("Ghosts");
                group.members.push(PropId::new());
                s.groups.push(group);
            }),
            (IssueCode::SegmentOutOfBounds, |s| {
                s.controllers[0].ports[0].slots[0].segment = Some(NodeRange::new(0, 11))
            }),
            (IssueCode::InvalidBrightness, |s| {
                s.controllers[0].ports[0].brightness = 150
            }),
        ];
        for (code, mutate) in cases {
            let prop = line("A", 10);
            let mut show = show_with_slot(PortSlot::new(prop.id), prop);
            mutate(&mut show);
            let report = validate_show(&show);
            assert!(
                report.has_code(code),
                "expected {code:?}, got {:?}",
                report.issues
            );
        }
    }

    #[test]
    fn slot_pointing_at_missing_prop_is_reported() {
        let prop = line("A", 10);
        let show = show_with_slot(PortSlot::new(PropId::new()), prop);
        let report = validate_show(&show);
        assert!(report.has_code(IssueCode::UnknownPropReference));
        assert!(report.issues[0].message.contains("port 1 on 'Main'"));
    }
}
