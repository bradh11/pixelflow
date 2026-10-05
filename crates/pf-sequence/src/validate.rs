//! Checks a sequence against itself and the show. Problems are reported, never fixed silently,
//! and a sequence with problems still opens (the renderer skips what it can't draw).

use crate::{EffectId, RowId, Sequence, Target};
use pf_model::{Severity, Show};
use serde::Serialize;
use std::collections::HashSet;

/// One problem in a sequence, written for people.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceIssue {
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<RowId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectId>,
}

/// `m:ss.mmm`, e.g. `1:02.500`; hours are added when needed (`1:00:00.000`).
pub fn format_ms(ms: u64) -> String {
    let (h, rest) = (ms / 3_600_000, ms % 3_600_000);
    let (m, rest) = (rest / 60_000, rest % 60_000);
    let (s, milli) = (rest / 1000, rest % 1000);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{milli:03}")
    } else {
        format!("{m}:{s:02}.{milli:03}")
    }
}

fn target_name(show: &Show, target: Target) -> Option<String> {
    match target {
        Target::Prop(id) => show.prop(id).map(|p| format!("'{}'", p.name)),
        Target::Group(id) => show
            .groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| format!("group '{}'", g.name)),
        Target::Region { prop, region } => show
            .region(prop, region)
            .map(|(p, r)| format!("'{} / {}'", p.name, r.name)),
    }
}

/// Every problem in the sequence, errors first.
pub fn validate_sequence(seq: &Sequence, show: &Show) -> Vec<SequenceIssue> {
    let mut issues = Vec::new();
    let mut push = |severity, message: String, row: Option<RowId>, effect: Option<EffectId>| {
        issues.push(SequenceIssue {
            severity,
            message,
            row,
            effect,
        });
    };

    let mut row_ids = HashSet::new();
    let mut effect_ids = HashSet::new();
    for (row_number, row) in seq.rows.iter().enumerate() {
        if !row_ids.insert(row.id) {
            push(
                Severity::Error,
                format!("Row {} has the same id as another row.", row_number + 1),
                Some(row.id),
                None,
            );
        }
        let name = match target_name(show, row.target) {
            Some(name) => name,
            None => {
                let what = match row.target {
                    Target::Prop(_) => "a prop",
                    Target::Group(_) => "a group",
                    Target::Region { .. } => "a submodel",
                };
                push(
                    Severity::Warning,
                    format!(
                        "Row {} lights {what} that isn't in the show anymore, so it shows nothing.",
                        row_number + 1
                    ),
                    Some(row.id),
                    None,
                );
                format!("row {}", row_number + 1)
            }
        };
        for (layer_number, layer) in row.layers.iter().enumerate() {
            let describe = |e: &crate::Effect| {
                format!(
                    "The {} effect at {} on {name} (layer {})",
                    e.kind().label(),
                    format_ms(e.start_ms),
                    layer_number + 1
                )
            };
            for effect in &layer.effects {
                if !effect_ids.insert(effect.id) {
                    push(
                        Severity::Error,
                        format!("{} has the same id as another effect.", describe(effect)),
                        Some(row.id),
                        Some(effect.id),
                    );
                }
                if effect.end_ms <= effect.start_ms {
                    push(
                        Severity::Error,
                        format!(
                            "{} ends at {}, before it starts, so it never shows.",
                            describe(effect),
                            format_ms(effect.end_ms)
                        ),
                        Some(row.id),
                        Some(effect.id),
                    );
                } else if effect.start_ms >= seq.duration_ms {
                    push(
                        Severity::Warning,
                        format!(
                            "{} starts after the sequence ends ({}), so it never shows.",
                            describe(effect),
                            format_ms(seq.duration_ms)
                        ),
                        Some(row.id),
                        Some(effect.id),
                    );
                } else if effect.end_ms > seq.duration_ms {
                    push(
                        Severity::Warning,
                        format!(
                            "{} runs past the end of the sequence ({}); the rest is cut off.",
                            describe(effect),
                            format_ms(seq.duration_ms)
                        ),
                        Some(row.id),
                        Some(effect.id),
                    );
                }
            }
            // Overlaps: sort by start, then each effect only needs checking against the
            // furthest-reaching earlier one.
            let mut order: Vec<&crate::Effect> =
                layer.effects.iter().filter(|e| e.end_ms > e.start_ms).collect();
            order.sort_by_key(|e| (e.start_ms, e.end_ms));
            let mut reach: Option<&crate::Effect> = None;
            for effect in order {
                if let Some(earlier) = reach
                    && earlier.end_ms > effect.start_ms
                {
                    push(
                        Severity::Warning,
                        format!(
                            "{} overlaps the {} effect before it; on one layer only one effect should play at a time.",
                            describe(effect),
                            earlier.kind().label()
                        ),
                        Some(row.id),
                        Some(effect.id),
                    );
                }
                if reach.is_none_or(|r| effect.end_ms > r.end_ms) {
                    reach = Some(effect);
                }
            }
        }
    }

    let mut track_ids = HashSet::new();
    for track in &seq.timing_tracks {
        if !track_ids.insert(track.id) {
            push(
                Severity::Error,
                format!("Timing track '{}' has the same id as another track.", track.name),
                None,
                None,
            );
        }
        if let Some(mark) = track.marks.iter().find(|m| m.end_ms < m.start_ms) {
            push(
                Severity::Warning,
                format!(
                    "A mark at {} in timing track '{}' ends before it starts.",
                    format_ms(mark.start_ms),
                    track.name
                ),
                None,
                None,
            );
        }
    }

    issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Effect, EffectKind, Layer, Mark, Row, TimingKind, TimingTrack};
    use pf_model::{Generator, Group, Prop, ShapeSource};

    fn show() -> (Show, Prop) {
        let mut show = Show::new("t");
        let mut prop = Prop::new(
            "Arch",
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 1.0,
            }),
        );
        prop.regions.push(pf_model::Region::nodes(
            "Left",
            vec![vec![Some(pf_model::NodeRun::new(0, 4))]],
        ));
        show.props.push(prop.clone());
        let mut group = Group::new("Yard");
        group.members.push(prop.id);
        show.groups.push(group);
        (show, prop)
    }

    #[test]
    fn formats_times_for_people() {
        assert_eq!(format_ms(62_500), "1:02.500");
        assert_eq!(format_ms(0), "0:00.000");
        assert_eq!(format_ms(3_600_000 + 61_001), "1:01:01.001");
    }

    #[test]
    fn a_clean_sequence_has_no_issues() {
        let (show, prop) = show();
        let mut seq = Sequence::new("s", 10_000);
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0].effects = vec![
            Effect::new(EffectKind::On, 0, 1000),
            Effect::new(EffectKind::Chase, 1000, 2000),
        ];
        seq.rows.push(row);
        seq.rows.push(Row::new(Target::Group(show.groups[0].id)));
        let left = &show.props[0].regions[0];
        let mut sub = Row::new(Target::Region {
            prop: prop.id,
            region: left.id,
        });
        sub.layers[0].effects.push(Effect::new(EffectKind::On, 0, 1000));
        seq.rows.push(sub);
        assert_eq!(validate_sequence(&seq, &show), vec![]);
    }

    #[test]
    fn rows_on_deleted_submodels_are_reported_by_name() {
        let (show, prop) = show();
        let mut seq = Sequence::new("s", 10_000);
        let mut row = Row::new(Target::Region {
            prop: prop.id,
            region: show.props[0].regions[0].id,
        });
        row.layers[0].effects.push(Effect::new(EffectKind::On, 500, 400));
        seq.rows.push(row);
        seq.rows.push(Row::new(Target::Region {
            prop: prop.id,
            region: pf_model::RegionId::new(),
        }));
        let messages: Vec<String> = validate_sequence(&seq, &show)
            .into_iter()
            .map(|i| i.message)
            .collect();
        assert!(
            messages[0].starts_with("The On effect at 0:00.500 on 'Arch / Left' (layer 1)"),
            "{messages:?}"
        );
        assert_eq!(
            messages[1],
            "Row 2 lights a submodel that isn't in the show anymore, so it shows nothing."
        );
    }

    #[test]
    fn reports_timing_overlap_target_and_id_problems() {
        let (show, prop) = show();
        let mut seq = Sequence::new("s", 10_000);
        let mut row = Row::new(Target::Prop(prop.id));
        let backwards = Effect::new(EffectKind::On, 500, 400);
        let late = Effect::new(EffectKind::Fade, 9_500, 10_500);
        let never = Effect::new(EffectKind::Fire, 10_600, 11_000);
        let long = Effect::new(EffectKind::Twinkle, 0, 5000);
        let inside = Effect::new(EffectKind::Strobe, 1000, 2000);
        let mut dup = Effect::new(EffectKind::Off, 6000, 7000);
        dup.id = inside.id;
        row.layers[0].effects = vec![backwards.clone(), late.clone(), never, long, inside.clone()];
        row.layers.push(Layer { effects: vec![dup] });
        seq.rows.push(row);
        let ghost = Row::new(Target::Prop(pf_model::PropId::new()));
        seq.rows.push(ghost.clone());
        seq.timing_tracks.push(TimingTrack::new(
            "Lyrics",
            TimingKind::Lyrics,
            vec![Mark::new(200, 100, "la")],
        ));

        let issues = validate_sequence(&seq, &show);
        let messages: Vec<&str> = issues.iter().map(|i| i.message.as_str()).collect();
        assert_eq!(issues[0].severity, Severity::Error, "errors first: {messages:#?}");
        let find = |text: &str| {
            issues
                .iter()
                .find(|i| i.message.contains(text))
                .unwrap_or_else(|| panic!("no issue containing {text:?} in {messages:#?}"))
        };
        let i = find("The On effect at 0:00.500 on 'Arch' (layer 1) ends at 0:00.400, before it starts");
        assert_eq!((i.severity, i.effect), (Severity::Error, Some(backwards.id)));
        let i = find("runs past the end of the sequence (0:10.000)");
        assert_eq!((i.severity, i.effect), (Severity::Warning, Some(late.id)));
        find("The Fire effect at 0:10.600 on 'Arch' (layer 1) starts after the sequence ends");
        let i = find("The Strobe effect at 0:01.000 on 'Arch' (layer 1) overlaps the Twinkle effect");
        assert_eq!(i.effect, Some(inside.id));
        find("The Off effect at 0:06.000 on 'Arch' (layer 2) has the same id as another effect.");
        let i = find("Row 2 lights a prop that isn't in the show anymore");
        assert_eq!(i.row, Some(ghost.id));
        find("A mark at 0:00.200 in timing track 'Lyrics' ends before it starts.");
        assert_eq!(issues.len(), 7, "{messages:#?}");
    }

    #[test]
    fn touching_effects_do_not_overlap_and_overlaps_are_found_past_a_short_effect() {
        let (show, prop) = show();
        let mut seq = Sequence::new("s", 10_000);
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0].effects = vec![
            Effect::new(EffectKind::On, 0, 1000),
            Effect::new(EffectKind::On, 1000, 2000),
            Effect::new(EffectKind::On, 3000, 9000),
            Effect::new(EffectKind::On, 3500, 4000),
            Effect::new(EffectKind::On, 5000, 6000),
        ];
        seq.rows.push(row);
        let overlaps = validate_sequence(&seq, &show)
            .into_iter()
            .filter(|i| i.message.contains("overlaps"))
            .count();
        assert_eq!(overlaps, 2);
    }
}
