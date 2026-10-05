//! Structural checks: broken references, out-of-range values, duplicate ids.
//! Wiring checks (capacity, universes) live in `pf-mapping`.

use crate::{Issue, IssueCode, Show, ValidationReport, limits};
use crate::{Prop, PropId};
use std::collections::{HashMap, HashSet};
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
    check_limits(show, &mut report);
    check_props(show, &mut report);
    // First occurrence wins, matching `Show::prop`.
    let mut props: HashMap<PropId, &Prop> = HashMap::with_capacity(show.props.len());
    for prop in &show.props {
        props.entry(prop.id).or_insert(prop);
    }
    check_groups(show, &props, &mut report);
    check_controllers(show, &props, &mut report);
    check_background(show, &mut report);
    check_house_model(show, &mut report);

    report
}

/// A damaged background photo (say, from a hand-edited file) is only a warning: the lights
/// don't depend on it.
fn check_background(show: &Show, report: &mut ValidationReport) {
    if let Some(problem) = show.background.as_ref().and_then(|b| b.problem()) {
        report.push(
            Issue::warning(IssueCode::InvalidBackground, problem)
                .with_fix("Choose the photo again on the Layout screen, or remove it."),
        );
    }
}

/// Like the photo, a damaged house model is only a warning.
fn check_house_model(show: &Show, report: &mut ValidationReport) {
    if let Some(problem) = show.house_model.as_ref().and_then(|m| m.problem()) {
        report.push(
            Issue::warning(IssueCode::InvalidHouseModel, problem)
                .with_fix("Choose the model again in the 3D view, or remove it."),
        );
    }
}

fn check_limits(show: &Show, report: &mut ValidationReport) {
    for problem in limits::check_limits(show) {
        report.push(
            Issue::error(IssueCode::LimitExceeded, problem)
                .with_fix("Reduce the size, or split it into smaller props."),
        );
    }
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

fn check_groups(show: &Show, props: &HashMap<PropId, &Prop>, report: &mut ValidationReport) {
    for group in &show.groups {
        let missing = group.members.iter().filter(|id| !props.contains_key(*id)).count();
        if missing > 0 {
            report.push(
                Issue::error(
                    IssueCode::UnknownPropReference,
                    format!(
                        "The group '{}' includes {}.",
                        group.name,
                        if missing == 1 {
                            "1 prop that no longer exists".to_string()
                        } else {
                            format!("{missing} props that no longer exist")
                        }
                    ),
                )
                .with_fix("Remove the missing props from the group."),
            );
        }
    }
}

fn check_controllers(show: &Show, props: &HashMap<PropId, &Prop>, report: &mut ValidationReport) {
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
            if !port.gamma.is_finite() || port.gamma <= 0.0 {
                report.push(
                    Issue::error(
                        IssueCode::InvalidGamma,
                        format!(
                            "The gamma of {where_} is {}, but it must be a positive number.",
                            port.gamma
                        ),
                    )
                    .with_fix("Set gamma to a positive number such as 1.0 or 2.2."),
                );
            }
            for slot in &port.slots {
                let Some(prop) = props.get(&slot.prop).copied() else {
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
                if slot.gamma.is_some_and(|g| !g.is_finite() || g <= 0.0) {
                    report.push(
                        Issue::error(
                            IssueCode::InvalidGamma,
                            format!(
                                "The gamma override for '{}' on {where_} must be a positive number.",
                                prop.name
                            ),
                        )
                        .with_fix("Set gamma to a positive number such as 1.0 or 2.2."),
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
        let cases: [(IssueCode, Mutate); 12] = [
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
            (IssueCode::InvalidGamma, |s| s.controllers[0].ports[0].gamma = 0.0),
            (IssueCode::InvalidGamma, |s| {
                s.controllers[0].ports[0].gamma = f32::NAN
            }),
            (IssueCode::InvalidGamma, |s| {
                s.controllers[0].ports[0].slots[0].gamma = Some(-2.0)
            }),
            (IssueCode::LimitExceeded, |s| {
                s.controllers[0].ports[0].slots[0].null_pixels = crate::MAX_NULL_PIXELS + 1
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(line("Huge", crate::MAX_PROP_NODES + 1))
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
    fn a_damaged_background_photo_is_a_warning() {
        let prop = line("A", 10);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        show.background = Some(crate::Background::new("/photos/house.jpg", 0.0, 5.0, 20.0));
        assert_eq!(validate_show(&show).issues, vec![]);

        show.background.as_mut().unwrap().width = -3.0;
        let issues = validate_show(&show).issues;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, IssueCode::InvalidBackground);
        assert_eq!(issues[0].severity, crate::Severity::Warning);
        assert!(
            issues[0].message.contains("wider than zero"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn a_damaged_house_model_is_a_warning() {
        let prop = line("A", 10);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        show.house_model = Some(crate::HouseModel::new("/models/house.glb"));
        assert_eq!(validate_show(&show).issues, vec![]);

        show.house_model.as_mut().unwrap().opacity = 2.0;
        let issues = validate_show(&show).issues;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, IssueCode::InvalidHouseModel);
        assert_eq!(issues[0].severity, crate::Severity::Warning);
    }

    #[test]
    fn slot_pointing_at_missing_prop_is_reported() {
        let prop = line("A", 10);
        let show = show_with_slot(PortSlot::new(PropId::new()), prop);
        let report = validate_show(&show);
        assert!(report.has_code(IssueCode::UnknownPropReference));
        assert!(report.issues[0].message.contains("port 1 on 'Main'"));
    }

    #[test]
    fn missing_group_members_use_correct_plural() {
        for (count, expected) in [
            (1, "1 prop that no longer exists"),
            (3, "3 props that no longer exist"),
        ] {
            let prop = line("A", 10);
            let mut show = show_with_slot(PortSlot::new(prop.id), prop);
            let mut group = Group::new("Ghosts");
            group.members.extend((0..count).map(|_| PropId::new()));
            show.groups.push(group);
            let report = validate_show(&show);
            assert!(report.issues[0].message.contains(expected), "{:?}", report.issues);
        }
    }
}
