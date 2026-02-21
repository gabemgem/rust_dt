//! Strongly typed, zero-cost identifier wrappers.
//!
//! All IDs are `Copy + Ord + Hash` so they can be used as map keys and sorted
//! collection elements without ceremony.  The inner integer is `pub` to allow
//! direct indexing into SoA `Vec`s via `id.0 as usize`, but callers should
//! prefer the `.index()` helpers for clarity.

use std::fmt;

/// Generate a typed ID wrapper around a primitive integer.
macro_rules! typed_id {
    ($(#[$attr:meta])* $vis:vis struct $name:ident($inner:ty);) => {
        $(#[$attr])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        $vis struct $name(pub $inner);

        impl $name {
            /// Sentinel meaning "no valid ID" — equivalent to `u32::MAX`.
            pub const INVALID: $name = $name(<$inner>::MAX);

            /// Cast to `usize` for direct use as a `Vec` index.
            #[inline(always)]
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl Default for $name {
            /// Returns the `INVALID` sentinel so uninitialized IDs are visibly invalid.
            #[inline(always)]
            fn default() -> Self {
                Self::INVALID
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl From<$name> for usize {
            #[inline(always)]
            fn from(id: $name) -> usize {
                id.0 as usize
            }
        }

        impl TryFrom<usize> for $name {
            type Error = std::num::TryFromIntError;
            fn try_from(n: usize) -> Result<$name, Self::Error> {
                <$inner>::try_from(n).map($name)
            }
        }
    };
}

typed_id! {
    /// Index of an agent in SoA storage.  Max ~4.3 billion agents.
    pub struct AgentId(u32);
}

typed_id! {
    /// Index of a road-network node.
    pub struct NodeId(u32);
}

typed_id! {
    /// Index of a directed road-network edge.
    pub struct EdgeId(u32);
}

typed_id! {
    /// Index of an activity type in the application's activity registry.
    /// Using `u16` keeps schedule arrays compact (max 65,535 activity types).
    pub struct ActivityId(u16);
}
