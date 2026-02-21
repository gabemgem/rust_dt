//! Transportation mode enum shared across all mobility-related crates.
//!
//! All variants are always compiled in (no per-variant feature flags).
//! Feature flags in `dt-mobility` control which movement implementations
//! are available; applications that declare unsupported modes will receive
//! a runtime error from the mobility engine.

/// The means by which an agent is currently travelling (or not).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum TransportMode {
    /// Agent is stationary at a node (default state).
    #[default]
    None,
    /// Private vehicle.
    Car,
    /// On foot.
    Walk,
    /// Bicycle.
    Bike,
    /// Scheduled public transit (bus, rail, ferry…).
    Transit,
}

impl TransportMode {
    /// `true` for any mode that causes the agent to be in motion.
    #[inline]
    pub fn is_moving(self) -> bool {
        !matches!(self, TransportMode::None)
    }

    /// Human-readable label, useful for CSV/Parquet column values.
    pub fn as_str(self) -> &'static str {
        match self {
            TransportMode::None    => "none",
            TransportMode::Car     => "car",
            TransportMode::Walk    => "walk",
            TransportMode::Bike    => "bike",
            TransportMode::Transit => "transit",
        }
    }
}

impl std::fmt::Display for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
