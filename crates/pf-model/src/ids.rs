//! Strongly typed identifiers for show entities.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Creates a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

define_id!(
    /// Identifies a [`crate::Prop`].
    PropId
);
define_id!(
    /// Identifies a [`crate::SequenceEntry`].
    SequenceId
);
define_id!(
    /// Identifies a [`crate::Group`].
    GroupId
);
define_id!(
    /// Identifies a [`crate::Controller`].
    ControllerId
);
define_id!(
    /// Identifies a [`crate::Region`] (a submodel or face) on its prop.
    RegionId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_unique() {
        assert_ne!(PropId::new(), PropId::new());
    }

    #[test]
    fn ids_serialize_as_plain_uuid_strings() {
        let id = PropId(Uuid::nil());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000-0000-0000-0000-000000000000\"");
        let back: PropId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
