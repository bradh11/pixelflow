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
            report.push(Issue::error(
                IssueCode::DuplicateId,
                format!(
                    "The {what} '{}' on '{}' has the same id as another one on the prop.",
                    region.name, prop.name
                ),
            ));
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
        let cases: [(IssueCode, Mutate); 36] = [
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
            (IssueCode::LimitExceeded, |s| {
                s.props.push(poly("Too bendy", crate::MAX_POLY_VERTICES + 1, 0))
            }),
            (IssueCode::LimitExceeded, |s| s.props.push(poly("Dot", 1, 0))),
            (IssueCode::LimitExceeded, |s| {
                let mut p = poly("Odd", 3, 10);
                if let ShapeSource::Generator(Generator::PolyLine { segments, .. }) = &mut p.shape {
                    segments.pop();
                }
                s.props.push(p)
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props
                    .push(icicles("Long pattern", vec![1; crate::MAX_ICICLE_DROPS + 1]))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props
                    .push(icicles("Long drop", vec![3, crate::MAX_ICICLE_DROP_LIGHTS + 1]))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(icicles("Dry", vec![0, 0]))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(icicles("Bare", vec![]))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(canes("Forest", crate::MAX_PROP_NODES + 1, 0))
            }),
            (IssueCode::LimitExceeded, |s| {
                // The real count, not the 32-bit one that stops at its largest value.
                s.props.push(canes("Huge", 70_000, 70_000))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props
                    .push(spinner("Windmill", crate::MAX_SPINNER_ARMS + 1, 1, 20, 360.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(spinner("Hole", 4, 5, 101, 360.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(spinner("No sweep", 4, 5, 20, 0.0))
            }),
            (IssueCode::LimitExceeded, |s| {
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
            (IssueCode::LimitExceeded, |s| {
                s.props.push(sphere("Past the pole", 4, 4, (-95.0, 86.0), 360.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(sphere("No sweep", 4, 4, (-86.0, 86.0), 0.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(sphere("Past round", 4, 4, (-86.0, 86.0), 400.0))
            }),
            (IssueCode::LimitExceeded, |s| {
                s.props.push(cube("Big box", 2_000, 2_000, 2_000))
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
