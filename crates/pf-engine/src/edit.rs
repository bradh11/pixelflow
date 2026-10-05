//! Edits: the only way the show changes.

use crate::error::EngineError;
use pf_model::{
    Background, Controller, ControllerId, Group, GroupId, GroupMember, HouseModel, Prop, PropId,
    SequenceEntry, SequenceId, Show,
};
use serde::{Deserialize, Serialize};

/// One change to the show. Batches of edits are applied atomically by [`crate::Engine::apply`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Edit {
    RenameShow {
        name: String,
    },
    SetFrameRate {
        fps: u16,
    },
    AddProp {
        prop: Prop,
    },
    /// Replaces the prop with the same id (submodels it no longer has leave their groups).
    UpdateProp {
        prop: Prop,
    },
    /// Removes the prop, its port slots, and its group memberships (its submodels' too).
    RemoveProp {
        id: PropId,
    },
    AddGroup {
        group: Group,
    },
    UpdateGroup {
        group: Group,
    },
    RemoveGroup {
        id: GroupId,
    },
    AddController {
        controller: Controller,
    },
    /// Replaces the controller with the same id (including its ports and wiring).
    UpdateController {
        controller: Controller,
    },
    RemoveController {
        id: ControllerId,
    },
    AddSequence {
        sequence: SequenceEntry,
    },
    /// Replaces the sequence with the same id (name, files, offset).
    UpdateSequence {
        sequence: SequenceEntry,
    },
    RemoveSequence {
        id: SequenceId,
    },
    /// Moves a sequence to `index` in the playlist (clamped to the end).
    MoveSequence {
        id: SequenceId,
        index: usize,
    },
    /// Sets, moves, dims, or (with `None`) removes the photo behind the layout.
    SetBackground {
        background: Option<Background>,
    },
    /// Sets, places, or (with `None`) removes the house model in the 3D view.
    SetHouseModel {
        house_model: Option<HouseModel>,
    },
}

impl Edit {
    /// Applies the edit to `show`, or explains why it cannot be applied.
    pub fn apply(&self, show: &mut Show) -> Result<(), EngineError> {
        match self {
            Edit::RenameShow { name } => show.name = name.clone(),
            Edit::SetFrameRate { fps } => show.settings.frame_rate = *fps,
            Edit::AddProp { prop } => {
                if show.props.iter().any(|p| p.id == prop.id) {
                    return Err(EngineError::DuplicateId { kind: "prop" });
                }
                show.props.push(prop.clone());
            }
            Edit::UpdateProp { prop } => {
                *find(&mut show.props, |p| p.id == prop.id, "prop")? = prop.clone();
                // A deleted submodel leaves the groups it was in.
                for group in &mut show.groups {
                    group.members.retain(|m| match m {
                        GroupMember::Region(r) => r.prop != prop.id || prop.region(r.region).is_some(),
                        GroupMember::Prop(_) => true,
                    });
                }
            }
            Edit::RemoveProp { id } => {
                let before = show.props.len();
                show.props.retain(|p| p.id != *id);
                if show.props.len() == before {
                    return Err(EngineError::NotFound { kind: "prop" });
                }
                for controller in &mut show.controllers {
                    for port in &mut controller.ports {
                        port.slots.retain(|s| s.prop != *id);
                    }
                }
                for group in &mut show.groups {
                    group.members.retain(|m| m.prop() != *id);
                }
            }
            Edit::AddGroup { group } => {
                if show.groups.iter().any(|g| g.id == group.id) {
                    return Err(EngineError::DuplicateId { kind: "group" });
                }
                show.groups.push(group.clone());
            }
            Edit::UpdateGroup { group } => {
                *find(&mut show.groups, |g| g.id == group.id, "group")? = group.clone();
            }
            Edit::RemoveGroup { id } => remove(&mut show.groups, |g| g.id == *id, "group")?,
            Edit::AddController { controller } => {
                if show.controllers.iter().any(|c| c.id == controller.id) {
                    return Err(EngineError::DuplicateId { kind: "controller" });
                }
                show.controllers.push(controller.clone());
            }
            Edit::UpdateController { controller } => {
                *find(&mut show.controllers, |c| c.id == controller.id, "controller")? = controller.clone();
            }
            Edit::RemoveController { id } => remove(&mut show.controllers, |c| c.id == *id, "controller")?,
            Edit::AddSequence { sequence } => {
                if show.sequences.iter().any(|s| s.id == sequence.id) {
                    return Err(EngineError::DuplicateId { kind: "sequence" });
                }
                show.sequences.push(sequence.clone());
            }
            Edit::UpdateSequence { sequence } => {
                *find(&mut show.sequences, |s| s.id == sequence.id, "sequence")? = sequence.clone();
            }
            Edit::RemoveSequence { id } => remove(&mut show.sequences, |s| s.id == *id, "sequence")?,
            Edit::MoveSequence { id, index } => {
                let from = show
                    .sequences
                    .iter()
                    .position(|s| s.id == *id)
                    .ok_or(EngineError::NotFound { kind: "sequence" })?;
                let sequence = show.sequences.remove(from);
                let to = (*index).min(show.sequences.len());
                show.sequences.insert(to, sequence);
            }
            Edit::SetBackground { background } => {
                if let Some(problem) = background.as_ref().and_then(Background::problem) {
                    return Err(EngineError::InvalidEdit(problem));
                }
                show.background = background.clone();
            }
            Edit::SetHouseModel { house_model } => {
                if let Some(problem) = house_model.as_ref().and_then(HouseModel::problem) {
                    return Err(EngineError::InvalidEdit(problem));
                }
                show.house_model = house_model.clone();
            }
        }
        Ok(())
    }
}

fn find<'a, T>(
    items: &'a mut [T],
    matches: impl Fn(&T) -> bool,
    kind: &'static str,
) -> Result<&'a mut T, EngineError> {
    items
        .iter_mut()
        .find(|item| matches(item))
        .ok_or(EngineError::NotFound { kind })
}

fn remove<T>(
    items: &mut Vec<T>,
    matches: impl Fn(&T) -> bool,
    kind: &'static str,
) -> Result<(), EngineError> {
    let before = items.len();
    items.retain(|item| !matches(item));
    if items.len() == before {
        return Err(EngineError::NotFound { kind });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Generator, Port, PortSlot, Protocol, ShapeSource};

    fn line(name: &str) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 1.0,
            }),
        )
    }

    #[test]
    fn edits_serialize_with_a_type_tag_and_camel_case_fields() {
        let edit = Edit::SetFrameRate { fps: 30 };
        let json = serde_json::to_value(&edit).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "setFrameRate", "fps": 30 }));
        assert_eq!(serde_json::from_value::<Edit>(json).unwrap(), edit);
    }

    #[test]
    fn removing_a_prop_also_unwires_it_and_leaves_groups() {
        let mut show = Show::new("t");
        let a = line("A");
        let mut controller = Controller::new("C", "10.0.0.1", Protocol::Ddp);
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(a.id));
        controller.ports.push(port);
        let mut group = Group::new("G");
        group.members.push(a.id.into());
        let left = pf_model::Region::nodes("Left", vec![]);
        group.members.push(
            pf_model::RegionRef {
                prop: a.id,
                region: left.id,
            }
            .into(),
        );
        let mut a = a;
        a.regions.push(left);
        show.props.push(a.clone());
        show.controllers.push(controller);
        show.groups.push(group);

        Edit::RemoveProp { id: a.id }.apply(&mut show).unwrap();
        assert!(show.props.is_empty());
        assert!(show.controllers[0].ports[0].slots.is_empty());
        assert!(show.groups[0].members.is_empty());
    }

    #[test]
    fn missing_and_duplicate_ids_are_rejected() {
        let mut show = Show::new("t");
        let a = line("A");
        assert!(matches!(
            Edit::UpdateProp { prop: a.clone() }.apply(&mut show),
            Err(EngineError::NotFound { kind: "prop" })
        ));
        Edit::AddProp { prop: a.clone() }.apply(&mut show).unwrap();
        let err = Edit::AddProp { prop: a.clone() }.apply(&mut show).unwrap_err();
        assert_eq!(err.to_string(), "A prop with that id already exists.");
        assert!(Edit::RemoveGroup { id: GroupId::new() }.apply(&mut show).is_err());
        assert!(
            Edit::RemoveController {
                id: ControllerId::new()
            }
            .apply(&mut show)
            .is_err()
        );
    }

    #[test]
    fn update_replaces_by_id() {
        let mut show = Show::new("t");
        let mut a = line("A");
        Edit::AddProp { prop: a.clone() }.apply(&mut show).unwrap();
        a.name = "Renamed".into();
        Edit::UpdateProp { prop: a }.apply(&mut show).unwrap();
        assert_eq!(show.props[0].name, "Renamed");
    }

    #[test]
    fn deleting_a_submodel_takes_it_out_of_groups() {
        let mut show = Show::new("t");
        let mut a = line("A");
        let left = pf_model::Region::nodes("Left", vec![]);
        let right = pf_model::Region::nodes("Right", vec![]);
        let id = a.id;
        let member = |r: &pf_model::Region| {
            GroupMember::Region(pf_model::RegionRef {
                prop: id,
                region: r.id,
            })
        };
        let mut group = Group::new("Halves");
        group.members = vec![member(&left), id.into(), member(&right)];
        a.regions = vec![left, right.clone()];
        show.props.push(a.clone());
        show.groups.push(group);
        a.regions.remove(0);
        Edit::UpdateProp { prop: a.clone() }.apply(&mut show).unwrap();
        assert_eq!(show.groups[0].members, vec![id.into(), member(&right)]);
    }

    #[test]
    fn sequences_can_be_added_updated_moved_and_removed() {
        use pf_model::SequenceEntry;
        let mut show = Show::new("t");
        let a = SequenceEntry::new("Medley", "/shows/medley.fseq");
        let b = SequenceEntry::new("Wizards", "/shows/wizards.fseq");
        Edit::AddSequence { sequence: a.clone() }
            .apply(&mut show)
            .unwrap();
        Edit::AddSequence { sequence: b.clone() }
            .apply(&mut show)
            .unwrap();
        assert!(matches!(
            Edit::AddSequence { sequence: a.clone() }.apply(&mut show),
            Err(EngineError::DuplicateId { .. })
        ));
        let mut changed = a.clone();
        changed.offset_ms = -120;
        changed.audio = Some("/shows/medley.mp3".into());
        Edit::UpdateSequence { sequence: changed }
            .apply(&mut show)
            .unwrap();
        assert_eq!(show.sequences[0].offset_ms, -120);
        Edit::MoveSequence { id: a.id, index: 9 }
            .apply(&mut show)
            .unwrap();
        assert_eq!(
            show.sequences.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["Wizards", "Medley"]
        );
        Edit::RemoveSequence { id: b.id }.apply(&mut show).unwrap();
        assert_eq!(show.sequences.len(), 1);
        assert!(Edit::RemoveSequence { id: b.id }.apply(&mut show).is_err());
    }

    #[test]
    fn the_background_photo_can_be_set_and_removed_but_not_made_invalid() {
        let mut show = Show::new("t");
        let photo = Background::new("/photos/house.jpg", -10.0, 8.0, 20.0);
        let edit = Edit::SetBackground {
            background: Some(photo.clone()),
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert_eq!(json["type"], "setBackground");
        assert_eq!(json["background"]["width"], 20.0);
        edit.apply(&mut show).unwrap();
        assert_eq!(show.background, Some(photo.clone()));

        let flat = Background { width: 0.0, ..photo };
        let err = Edit::SetBackground {
            background: Some(flat),
        }
        .apply(&mut show)
        .unwrap_err();
        assert_eq!(err.to_string(), "The background photo must be wider than zero.");
        assert_eq!(show.background.as_ref().unwrap().width, 20.0, "unchanged");

        Edit::SetBackground { background: None }.apply(&mut show).unwrap();
        assert_eq!(show.background, None);
    }

    #[test]
    fn the_house_model_can_be_set_placed_and_removed_but_not_made_invalid() {
        let mut show = Show::new("t");
        let model = HouseModel::new("/models/house.glb");
        let edit = Edit::SetHouseModel {
            house_model: Some(model.clone()),
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert_eq!(json["type"], "setHouseModel");
        assert_eq!(json["houseModel"]["path"], "/models/house.glb");
        edit.apply(&mut show).unwrap();
        assert_eq!(show.house_model, Some(model.clone()));

        let placed = HouseModel {
            scale: 0.5,
            ..model.clone()
        };
        Edit::SetHouseModel {
            house_model: Some(placed.clone()),
        }
        .apply(&mut show)
        .unwrap();
        assert_eq!(show.house_model, Some(placed));

        let err = Edit::SetHouseModel {
            house_model: Some(HouseModel { scale: -1.0, ..model }),
        }
        .apply(&mut show)
        .unwrap_err();
        assert_eq!(err.to_string(), "The house model's scale must be more than zero.");
        assert_eq!(show.house_model.as_ref().unwrap().scale, 0.5, "unchanged");

        Edit::SetHouseModel { house_model: None }
            .apply(&mut show)
            .unwrap();
        assert_eq!(show.house_model, None);
    }
}
