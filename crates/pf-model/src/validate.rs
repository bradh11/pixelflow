//! Structural checks: broken references, out-of-range values, duplicate ids.
//! Wiring checks (capacity, universes) live in `pf-mapping`.

use crate::{GroupMember, Prop, PropId};
use crate::{Issue, IssueCode, Show, ValidationReport, limits};
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

/// The problems that stop a show file from opening: sizes past PixelFlow's limits, shapes it
/// can't build, numbers that aren't finite. They're among [`validate_show`]'s errors too, but an
/// edit or import that causes one must be refused outright, since the show couldn't be saved
/// and opened again.
pub fn limit_issues(show: &Show) -> Vec<Issue> {
    limits::check_limits(show)
        .into_iter()
        .map(|p| Issue::error(p.code, p.message).with_fix(p.fix))
        .collect()
}

fn check_limits(show: &Show, report: &mut ValidationReport) {
    report.issues.extend(limit_issues(show));
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
            report.push(
                Issue::error(
                    IssueCode::DuplicateId,
                    format!("The {kind} '{name}' has the same id as another {kind}."),
                )
                .with_fix(format!(
                    "Delete one of the two {kind}s, then add it again if you still need it; a new {kind} gets its own id."
                )),
            );
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
        check_regions(prop, report);
    }
}

/// A prop's submodels and faces: named, unique by name and id, and inside the prop.
fn check_regions(prop: &Prop, report: &mut ValidationReport) {
    let nodes = prop.node_count();
    let mut names = HashSet::new();
    let mut ids = HashSet::new();
    for region in &prop.regions {
        let what = region.kind_word();
        if !ids.insert(region.id) {
            report.push(
                Issue::error(
                    IssueCode::DuplicateId,
                    format!(
                        "The {what} '{}' on '{}' has the same id as another one on the prop.",
                        region.name, prop.name
                    ),
                )
                .with_fix(format!(
                    "Delete the {what} in the prop's Submodels & faces, then add it again."
                )),
            );
        }
        if let Some(problem) = region.problem() {
            let problem = if region.name.trim().is_empty() {
                format!("{} (on '{}')", problem.trim_end_matches('.'), prop.name) + "."
            } else {
                problem
            };
            report.push(
                Issue::error(IssueCode::InvalidRegion, problem)
                    .with_fix(format!("Edit the {what} in the prop's Submodels & faces.")),
            );
        } else if !names.insert(region.name.trim().to_lowercase()) {
            report.push(
                Issue::error(
                    IssueCode::DuplicateRegionName,
                    format!(
                        "'{}' has two submodels or faces named '{}'.",
                        prop.name, region.name
                    ),
                )
                .with_fix("Rename one of them; names must be different on each prop."),
            );
        }
        if let Some(&(_, high)) = region.node_bounds().iter().max_by_key(|(_, high)| *high)
            && high >= nodes
        {
            report.push(
                Issue::error(
                    IssueCode::RegionOutOfBounds,
                    format!(
                        "The {what} '{}' on '{}' uses pixel {}, but the prop only has {nodes} pixels.",
                        region.name,
                        prop.name,
                        u64::from(high) + 1
                    ),
                )
                .with_fix(format!("Edit the {what} so it only uses the prop's pixels.")),
            );
        }
    }
}

fn check_groups(show: &Show, props: &HashMap<PropId, &Prop>, report: &mut ValidationReport) {
    for group in &show.groups {
        let lost = group
            .members
            .iter()
            .filter_map(|m| match m {
                GroupMember::Region(r) => Some(r),
                GroupMember::Prop(_) => None,
            })
            .filter(|m| props.get(&m.prop).and_then(|p| p.region(m.region)).is_none())
            .count();
        if lost > 0 {
            report.push(
                Issue::error(
                    IssueCode::UnknownPropReference,
                    format!(
                        "The group '{}' includes {}.",
                        group.name,
                        if lost == 1 {
                            "1 submodel that no longer exists".to_string()
                        } else {
                            format!("{lost} submodels that no longer exist")
                        }
                    ),
                )
                .with_fix("Remove the missing submodels from the group."),
            );
        }
        let missing = group
            .members
            .iter()
            .filter(|m| matches!(m, GroupMember::Prop(id) if !props.contains_key(id)))
            .count();
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

/// Why PixelFlow can't send to a (trimmed, non-empty) controller address, if it can't: output
/// goes over IPv4 only, and no address or hostname has a space in it.
fn address_problem(address: &str) -> Option<&'static str> {
    if address.parse::<std::net::Ipv6Addr>().is_ok()
        || address.parse::<std::net::SocketAddrV6>().is_ok()
        || address.starts_with('[')
    {
        Some("which is IPv6, but PixelFlow sends to controllers over IPv4 only")
    } else if address.contains(char::is_whitespace) {
        Some("which has a space in it")
    } else {
        None
    }
}

fn check_controllers(show: &Show, props: &HashMap<PropId, &Prop>, report: &mut ValidationReport) {
    for controller in &show.controllers {
        if controller.address.trim().is_empty() {
            report.push(
                Issue::warning(
                    IssueCode::MissingAddress,
                    format!(
                        "The controller '{}' has no IP address, so nothing is sent to it.",
                        controller.name
                    ),
                )
                .with_fix("Enter the controller's IP address on the Wiring screen."),
            );
        } else if let Some(problem) = address_problem(controller.address.trim()) {
            report.push(
                Issue::warning(
                    IssueCode::InvalidAddress,
                    format!(
                        "The controller '{}' has the address '{}', {problem}, so nothing is sent to it.",
                        controller.name,
                        controller.address.trim()
                    ),
                )
                .with_fix(
                    "Enter the controller's IPv4 address (like 192.168.1.50) or its hostname on the Wiring screen.",
                ),
            );
        }
        let mut numbers = HashSet::new();
        for port in &controller.ports {
            let where_ = format!("port {} on '{}'", port.number, controller.name);
            if port.number == 0 {
                report.push(
                    Issue::error(
                        IssueCode::InvalidPortNumber,
                        format!(
                            "'{}' has a port 0, but ports are numbered from 1, as printed on the controller.",
                            controller.name
                        ),
                    )
                    .with_fix("Renumber the port to match the controller."),
                );
            } else if !numbers.insert(port.number) {
                report.push(
                    Issue::error(
                        IssueCode::DuplicatePort,
                        format!(
                            "'{}' has port {} twice, so both would send to the same output.",
                            controller.name, port.number
                        ),
                    )
                    .with_fix("Renumber one of the ports, or move its props onto the other."),
                );
            }
            if port.brightness > 100 {
                report.push(
                    Issue::error(
                        IssueCode::InvalidBrightness,
                        format!(
                            "The brightness of {where_} is {}%, but it must be 0–100%.",
                            port.brightness
                        ),
                    )
                    .with_fix("Set the port's brightness between 0 and 100%."),
                );
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
                    report.push(
                        Issue::error(
                            IssueCode::InvalidBrightness,
                            format!(
                                "The brightness override for '{}' on {where_} must be 0–100%.",
                                prop.name
                            ),
                        )
                        .with_fix("Set the brightness override between 0 and 100%, or clear it."),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Controller, FaceDefinition, Generator, Group, NodeRange, NodeRun, Port, PortSlot, Prop, PropId,
        Protocol, Region, RegionId, RegionKind, RegionRef, ShapeSource,
    };

    fn line(name: &str, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        )
    }

    /// A poly line through `points` points along X, `nodes` pixels on each stretch.
    fn poly(name: &str, points: usize, nodes: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::PolyLine {
                vertices: (0..points)
                    .map(|i| crate::Vec3::new(i as f32, 0.0, 0.0))
                    .collect(),
                segments: vec![crate::PolySegment::straight(nodes); points.saturating_sub(1)],
                spread_nodes: None,
            }),
        )
    }

    fn icicles(name: &str, drops: Vec<u32>) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Icicles {
                strings: 1,
                lights_per_string: 10,
                drops,
                width: 2.0,
                drop_height: 0.5,
                alternate_nodes: false,
            }),
        )
    }

    fn canes(name: &str, canes: u32, nodes_per_cane: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::CandyCanes {
                canes,
                nodes_per_cane,
                width: 3.0,
                height: 1.0,
                cane_height: 1.0,
                reverse: false,
                sticks: false,
                alternate_nodes: false,
                skew_deg: 0.0,
                start_right: false,
            }),
        )
    }

    fn spinner(name: &str, arms: u32, nodes_per_arm: u32, hollow: u32, arc: f32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Spinner {
                arms,
                nodes_per_arm,
                hollow,
                start_angle: 0.0,
                arc,
                zig_zag: false,
                alternate: false,
                from_center: false,
                clockwise: false,
                radius: 1.0,
            }),
        )
    }

    fn frame(name: &str, top: u32, sides: u32, bottom: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::WindowFrame {
                top,
                sides,
                bottom,
                width: 2.0,
                height: 1.0,
                start: crate::Corner::BottomLeft,
                counter_clockwise: false,
            }),
        )
    }

    #[test]
    fn window_frames_wreaths_and_spinners_within_the_limits_are_valid() {
        let wreath = Prop::new(
            "Door",
            ShapeSource::Generator(Generator::Wreath {
                nodes: 50,
                radius: 1.0,
                start_at_bottom: false,
                counter_clockwise: false,
            }),
        );
        for prop in [
            frame("Window", 10, 8, 10),
            wreath,
            spinner("Fan", crate::MAX_SPINNER_ARMS, 2, 100, 360.0),
            spinner("Half", 6, 10, 0, 1.0),
        ] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    fn sphere(name: &str, columns: u32, rows: u32, latitudes: (f32, f32), degrees: f32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Sphere {
                columns,
                rows,
                radius: 1.0,
                start_latitude: latitudes.0,
                end_latitude: latitudes.1,
                degrees,
                start: crate::Corner::BottomLeft,
                strand_style: crate::StrandStyle::ZigZag,
            }),
        )
    }

    fn cube(name: &str, width: u32, height: u32, depth: u32) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Cube {
                width,
                height,
                depth,
                spacing: 0.1,
                start: crate::CubeStart::FrontBottomLeft,
                style: crate::CubeStyle::VerticalFrontBack,
                strand_style: crate::StrandStyle::ZigZag,
                strand_per_layer: false,
            }),
        )
    }

    #[test]
    fn spheres_and_cubes_within_the_limits_are_valid() {
        for prop in [
            sphere("Globe", 16, 25, (-86.0, 86.0), 360.0),
            sphere("Dome", 10, 10, (-90.0, 90.0), 1.0),
            cube("Box", 10, 10, 10),
        ] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    fn arch(name: &str, count: u32, nodes: u32, list: Vec<u32>) -> Prop {
        let mut shape = Generator::arch(nodes, 2.0, 1.0);
        if let Generator::Arch { arches, layers, .. } = &mut shape {
            (*arches, *layers) = (count, list);
        }
        Prop::new(name, ShapeSource::Generator(shape))
    }

    fn tree(name: &str, strings: u32, strands_per_string: u32, spiral_rotations: f32) -> Prop {
        let mut shape = Generator::tree(strings, 10, 2.0, 1.0, 0.2, crate::TreeStyle::Round);
        if let Generator::Tree {
            strands_per_string: fold,
            spiral_rotations: spiral,
            ..
        } = &mut shape
        {
            (*fold, *spiral) = (strands_per_string, spiral_rotations);
        }
        Prop::new(name, ShapeSource::Generator(shape))
    }

    #[test]
    fn spiral_and_folded_trees_within_the_limits_are_valid() {
        for prop in [tree("Mega", 16, 3, 2.5), tree("Swirl", 8, 8, -100.0)] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    #[test]
    fn rows_of_arches_and_layered_arches_within_the_limits_are_valid() {
        for prop in [
            arch("Row", 8, 25, vec![]),
            arch("Layered", 1, 60, vec![10, 20, 30]),
        ] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    #[test]
    fn layered_circles_and_stars_within_the_limits_are_valid() {
        let mut circle = Generator::circle(60, 1.0);
        if let Generator::Circle { layers, .. } = &mut circle {
            *layers = vec![10, 20, 30];
        }
        let mut star = Generator::star(5, 60, 1.0, 0.4);
        if let Generator::Star { layers, .. } = &mut star {
            *layers = vec![20, 40];
        }
        for prop in [
            Prop::new("Target", ShapeSource::Generator(circle)),
            Prop::new("Topper", ShapeSource::Generator(star)),
        ] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    #[test]
    fn icicles_and_candy_canes_within_the_limits_are_valid() {
        for prop in [icicles("Eaves", vec![3, 0, 5]), canes("Walk", 3, 18)] {
            let show = show_with_slot(PortSlot::new(prop.id), prop);
            assert_eq!(validate_show(&show).issues, vec![]);
        }
    }

    #[test]
    fn a_poly_line_with_matching_stretches_is_valid() {
        let prop = poly("Roof", 4, 10);
        assert_eq!(prop.node_count(), 30);
        let show = show_with_slot(PortSlot::new(prop.id), prop);
        assert_eq!(validate_show(&show).issues, vec![]);
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
        let cases: [(IssueCode, Mutate); 71] = [
            (IssueCode::InvalidFrameRate, |s| s.settings.frame_rate = 5),
            (IssueCode::DuplicateId, |s| {
                let dup = s.props[0].clone();
                s.props.push(dup);
            }),
            (IssueCode::EmptyProp, |s| s.props.push(line("Empty", 0))),
            (IssueCode::RegionOutOfBounds, |s| {
                s.props[0]
                    .regions
                    .push(Region::nodes("Too far", vec![vec![Some(NodeRun::new(5, 10))]]))
            }),
            (IssueCode::RegionOutOfBounds, |s| {
                let mut face = FaceDefinition::default();
                face.outline.push(NodeRange::new(8, 11));
                s.props[0].regions.push(Region::face("Face", face))
            }),
            (IssueCode::DuplicateRegionName, |s| {
                s.props[0].regions.push(Region::nodes("Left", vec![]));
                s.props[0].regions.push(Region::nodes("left ", vec![]));
            }),
            (IssueCode::InvalidRegion, |s| {
                s.props[0].regions.push(Region {
                    id: RegionId::new(),
                    name: "Window".into(),
                    kind: RegionKind::SubBuffer {
                        x1: 0.0,
                        y1: 0.0,
                        x2: 150.0,
                        y2: 100.0,
                    },
                })
            }),
            (IssueCode::UnknownPropReference, |s| {
                let mut group = Group::new("Lost");
                group.members.push(
                    RegionRef {
                        prop: s.props[0].id,
                        region: RegionId::new(),
                    }
                    .into(),
                );
                s.groups.push(group);
            }),
            (IssueCode::UnknownPropReference, |s| {
                let mut group = Group::new("Ghosts");
                group.members.push(PropId::new().into());
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
            (IssueCode::InvalidShape, |s| {
                s.props.push(poly("Too bendy", crate::MAX_POLY_VERTICES + 1, 0))
            }),
            (IssueCode::InvalidShape, |s| s.props.push(poly("Dot", 1, 0))),
            (IssueCode::InvalidShape, |s| {
                let mut p = poly("Odd", 3, 10);
                if let ShapeSource::Generator(Generator::PolyLine { segments, .. }) = &mut p.shape {
                    segments.pop();
                }
                s.props.push(p)
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(icicles("Long pattern", vec![1; crate::MAX_ICICLE_DROPS + 1]))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(icicles("Long drop", vec![3, crate::MAX_ICICLE_DROP_LIGHTS + 1]))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(icicles("Dry", vec![0, 0]))
            }),
            (IssueCode::InvalidShape, |s| s.props.push(icicles("Bare", vec![]))),
            (IssueCode::InvalidShape, |s| {
                s.props.push(canes("Forest", crate::MAX_PROP_NODES + 1, 0))
            }),
            (IssueCode::LimitExceeded, |s| {
                // The real count, not the 32-bit one that stops at its largest value.
                s.props.push(canes("Huge", 70_000, 70_000))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(spinner("Windmill", crate::MAX_SPINNER_ARMS + 1, 1, 20, 360.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(spinner("Hole", 4, 5, 101, 360.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(spinner("No sweep", 4, 5, 20, 0.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(spinner("Past round", 4, 5, 20, 361.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(spinner("Big fan", 1_000, u32::MAX, 20, 360.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                // Both sides count, beyond what a 32-bit count can hold.
                s.props.push(frame("Huge", u32::MAX, u32::MAX, 1))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props
                    .push(sphere("Big globe", 70_000, 70_000, (-86.0, 86.0), 360.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(sphere("Past the pole", 4, 4, (-95.0, 86.0), 360.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(sphere("No sweep", 4, 4, (-86.0, 86.0), 0.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(sphere("Past round", 4, 4, (-86.0, 86.0), 400.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(cube("Big box", 2_000, 2_000, 2_000))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(sphere("Wide", u32::MAX, 0, (-86.0, 86.0), 360.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(cube("Flat", u32::MAX, u32::MAX, 0))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut p = poly("Bent", 2, 5);
                if let ShapeSource::Generator(Generator::PolyLine { segments, .. }) = &mut p.shape {
                    segments[0].curve = Some([crate::Vec3::new(f32::NAN, 0.0, 0.0), crate::Vec3::ZERO]);
                }
                s.props.push(p)
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(cube("Thin", 1, 1, crate::MAX_PROP_NODES + 1))
            }),
            (IssueCode::LimitExceeded, |s| {
                // The real count of a row of arches, beyond what a 32-bit count can hold.
                s.props.push(arch("Tunnel", 70_000, 70_000, vec![]))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(arch("Endless", crate::MAX_PROP_NODES + 1, 0, vec![]))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(arch("Deep", 1, 10, vec![1; crate::MAX_SHAPE_LAYERS + 1]))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props
                    .push(arch("Wide layer", 1, 10, vec![crate::MAX_PROP_NODES + 1]))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut p = arch("Flat", 1, 10, vec![]);
                if let ShapeSource::Generator(Generator::Arch { arc, .. }) = &mut p.shape {
                    *arc = 0.0;
                }
                s.props.push(p)
            }),
            (IssueCode::InvalidShape, |s| {
                let mut p = arch("Ring", 1, 10, vec![]);
                if let ShapeSource::Generator(Generator::Arch { arc, .. }) = &mut p.shape {
                    *arc = 270.0;
                }
                s.props.push(p)
            }),
            (IssueCode::InvalidShape, |s| {
                let mut shape = Generator::circle(10, 1.0);
                if let Generator::Circle { inner_percent, .. } = &mut shape {
                    *inner_percent = 120;
                }
                s.props.push(Prop::new("Halo", ShapeSource::Generator(shape)))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut shape = Generator::circle(10, 1.0);
                if let Generator::Circle { layers, .. } = &mut shape {
                    *layers = vec![1; crate::MAX_SHAPE_LAYERS + 1];
                }
                s.props.push(Prop::new("Target", ShapeSource::Generator(shape)))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut shape = Generator::star(5, 10, 1.0, 0.4);
                if let Generator::Star { layers, .. } = &mut shape {
                    *layers = vec![crate::MAX_PROP_NODES + 1];
                }
                s.props.push(Prop::new("Big star", ShapeSource::Generator(shape)))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut p = arch("Apart", 2, 10, vec![]);
                if let ShapeSource::Generator(Generator::Arch { gap, .. }) = &mut p.shape {
                    *gap = -0.5;
                }
                s.props.push(p)
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(tree("Corkscrew", 4, 0, 1e30))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(tree("Twister", 4, 0, -101.0))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(tree("Folded", 4, 5, 0.0))
            }),
            (IssueCode::InvalidShape, |s| {
                let mut p = arch("Hollow", 1, 10, vec![5, 5]);
                if let ShapeSource::Generator(Generator::Arch { hollow, .. }) = &mut p.shape {
                    *hollow = 101;
                }
                s.props.push(p)
            }),
            (IssueCode::DuplicateId, |s| {
                let group = Group::new("Twins");
                s.groups.push(group.clone());
                s.groups.push(group);
            }),
            (IssueCode::DuplicateId, |s| {
                let dup = s.controllers[0].clone();
                s.controllers.push(Controller { ports: vec![], ..dup });
            }),
            (IssueCode::DuplicateId, |s| {
                let sequence = crate::SequenceEntry::new("Song", "song.pfseq.json");
                s.sequences.push(sequence.clone());
                s.sequences.push(sequence);
            }),
            (IssueCode::InvalidBrightness, |s| {
                s.controllers[0].ports[0].slots[0].brightness = Some(101)
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(Prop::new(
                    "Grid",
                    ShapeSource::Generator(Generator::CustomGrid {
                        columns: 2,
                        rows: 2,
                        cells: vec![1, 2, 3],
                    }),
                ))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(Prop::new(
                    "Line",
                    ShapeSource::Generator(Generator::Line {
                        nodes: 5,
                        length: f32::NAN,
                    }),
                ))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(Prop::new(
                    "Tree",
                    ShapeSource::Generator(Generator::tree(
                        4,
                        5,
                        f32::INFINITY,
                        1.0,
                        0.0,
                        crate::TreeStyle::Round,
                    )),
                ))
            }),
            (IssueCode::InvalidShape, |s| {
                s.props.push(Prop::new(
                    "Measured",
                    ShapeSource::Measured {
                        points: vec![crate::Vec3::new(0.0, f32::NAN, 0.0)],
                        provenance: crate::Provenance::Import,
                    },
                ))
            }),
            (IssueCode::InvalidTransform, |s| {
                s.props[0].transform.position.x = f32::NAN
            }),
            (IssueCode::InvalidTransform, |s| {
                s.props[0].transform.rotation_deg.z = f32::INFINITY
            }),
            (IssueCode::InvalidTransform, |s| {
                s.props[0].transform.scale.y = f32::NEG_INFINITY
            }),
            (IssueCode::InvalidPortNumber, |s| {
                s.controllers[0].ports[0].number = 0
            }),
            (IssueCode::DuplicatePort, |s| {
                s.controllers[0].ports.push(Port::new(1))
            }),
            (IssueCode::MissingAddress, |s| {
                s.controllers[0].address = "  ".into()
            }),
            (IssueCode::InvalidAddress, |s| {
                s.controllers[0].address = "fe80::1".into()
            }),
            (IssueCode::InvalidAddress, |s| {
                s.controllers[0].address = "[::1]:4048".into()
            }),
            (IssueCode::InvalidAddress, |s| {
                s.controllers[0].address = "10.0.0 .5".into()
            }),
        ];
        let mut wrong = Vec::new();
        for (i, (code, mutate)) in cases.into_iter().enumerate() {
            let prop = line("A", 10);
            let mut show = show_with_slot(PortSlot::new(prop.id), prop);
            mutate(&mut show);
            let report = validate_show(&show);
            // The expected problem and no other error: one mistake isn't reported as several.
            // (A shape that can't be built may also warn that the prop has no pixels.)
            let only = report.has_code(code)
                && report
                    .issues
                    .iter()
                    .all(|i| i.code == code || i.severity == crate::Severity::Warning);
            if !only {
                wrong.push(format!("case {i}: expected {code:?}, got {:?}", report.issues));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn addresses_with_spaces_around_them_are_fine_but_ipv6_is_explained() {
        let prop = line("A", 10);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        for fine in [" 10.0.0.5 ", "10.0.0.5:4048\t", "wled-porch.local"] {
            show.controllers[0].address = fine.into();
            assert!(validate_show(&show).issues.is_empty(), "{fine:?}");
        }
        show.controllers[0].address = "fe80::1".into();
        let report = validate_show(&show);
        let issue = &report.issues[0];
        assert_eq!(issue.severity, crate::Severity::Warning);
        assert!(issue.message.contains("IPv6"), "{issue:?}");
        assert!(issue.fix.as_deref().unwrap().contains("IPv4"), "{issue:?}");
    }

    #[test]
    fn every_error_says_how_to_fix_it() {
        let prop = line("A", 10);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        show.props.push(show.props[0].clone());
        show.controllers[0].ports[0].brightness = 101;
        show.controllers[0].ports[0].slots[0].brightness = Some(101);
        show.controllers[0].ports[0].slots[0].null_pixels = crate::MAX_NULL_PIXELS + 1;
        let report = validate_show(&show);
        for code in [
            IssueCode::DuplicateId,
            IssueCode::InvalidBrightness,
            IssueCode::LimitExceeded,
        ] {
            assert!(report.has_code(code), "{code:?}: {:?}", report.issues);
        }
        for issue in &report.issues {
            assert!(issue.fix.is_some(), "{issue:?}");
        }
        let nulls = report
            .issues
            .iter()
            .find(|i| i.code == IssueCode::LimitExceeded)
            .unwrap();
        assert!(nulls.fix.as_deref().unwrap().contains("null pixels"), "{nulls:?}");
    }

    #[test]
    fn problems_that_stop_a_file_opening_are_the_limit_issues() {
        let prop = line("A", 10);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        assert_eq!(limit_issues(&show), vec![]);

        // Port and address problems are ordinary errors: the file still opens.
        show.controllers[0].ports.push(Port::new(0));
        show.controllers[0].address = String::new();
        assert_eq!(limit_issues(&show), vec![]);

        show.props[0].transform.scale.x = f32::NAN;
        show.props.push(Prop::new(
            "Grid",
            ShapeSource::Generator(Generator::CustomGrid {
                columns: 1,
                rows: 1,
                cells: vec![],
            }),
        ));
        let codes: Vec<IssueCode> = limit_issues(&show).iter().map(|i| i.code).collect();
        assert_eq!(codes, vec![IssueCode::InvalidTransform, IssueCode::InvalidShape]);
        assert!(crate::check_show(&show).is_err());
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
    fn region_problems_name_the_prop_and_the_pixel() {
        let mut prop = line("Arch", 10);
        prop.regions.push(Region::nodes(
            "Right",
            vec![vec![Some(NodeRun::new(4, 9))], vec![Some(NodeRun::new(12, 11))]],
        ));
        prop.regions.push(Region::nodes("", vec![]));
        let show = show_with_slot(PortSlot::new(prop.id), prop);
        let messages: Vec<String> = validate_show(&show)
            .issues
            .into_iter()
            .map(|i| i.message)
            .collect();
        assert_eq!(
            messages,
            vec![
                "The submodel 'Right' on 'Arch' uses pixel 13, but the prop only has 10 pixels.".to_string(),
                "A submodel has no name (on 'Arch').".to_string(),
            ]
        );
    }

    #[test]
    fn valid_submodels_and_group_members_have_no_issues() {
        let mut prop = line("Arch", 10);
        let left = Region::nodes("Left", vec![vec![Some(NodeRun::new(0, 4))]]);
        let member = RegionRef {
            prop: prop.id,
            region: left.id,
        };
        prop.regions.push(left);
        let mut show = show_with_slot(PortSlot::new(prop.id), prop);
        let mut group = Group::new("Halves");
        group.members.push(member.into());
        show.groups.push(group);
        assert_eq!(validate_show(&show).issues, vec![]);
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
            group
                .members
                .extend((0..count).map(|_| GroupMember::Prop(PropId::new())));
            show.groups.push(group);
            let report = validate_show(&show);
            assert!(report.issues[0].message.contains(expected), "{:?}", report.issues);
        }
    }
}
