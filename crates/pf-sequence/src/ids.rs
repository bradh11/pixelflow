//! Strongly typed identifiers for sequence entities.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Identifies a [`crate::Row`].
    RowId
);
define_id!(
    /// Identifies an [`crate::Effect`]. It also seeds the effect's randomness (twinkles, fire),
    /// so an effect looks the same every time it plays.
    EffectId
);
define_id!(
    /// Identifies a [`crate::TimingTrack`].
    TimingTrackId
);

impl EffectId {
    /// A 64-bit seed derived from the id, for deterministic randomness.
    pub fn seed(self) -> u64 {
        let v = self.0.as_u128();
        (v as u64) ^ ((v >> 64) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_plain_uuid_strings_and_seed_deterministically() {
        let id = EffectId(Uuid::from_u128(0x0000_0000_0000_0001_0000_0000_0000_0003));
        assert_eq!(id.seed(), 2);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000-0000-0001-0000-000000000003\"");
        assert_eq!(serde_json::from_str::<EffectId>(&json).unwrap(), id);
        assert_ne!(RowId::new(), RowId::new());
    }
}
