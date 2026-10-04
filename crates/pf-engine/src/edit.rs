//! Edits: the only way the show changes.

use crate::error::EngineError;
use pf_model::{Controller, ControllerId, Group, GroupId, Prop, PropId, Show};
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
    /// Replaces the prop with the same id.
    UpdateProp {
        prop: Prop,
    },
    /// Removes the prop, its port slots, and its group memberships.
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
                    group.members.retain(|m| m != id);
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
        group.members.push(a.id);
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
}
