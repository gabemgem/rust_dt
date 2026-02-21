//! Fluent builder for constructing `AgentStore` + `AgentRngs` in one step.
//!
//! # Usage
//!
//! ```rust
//! use dt_agent::AgentStoreBuilder;
//!
//! #[derive(Default)]
//! struct HealthState { infected: bool }
//!
//! let (mut store, mut rngs) = AgentStoreBuilder::new(10_000, /*seed=*/ 42)
//!     .register_component::<HealthState>()
//!     .build();
//!
//! assert_eq!(store.count, 10_000);
//! assert_eq!(rngs.len(),  10_000);
//!
//! // Fill in actual values from CSV / shapefiles after building.
//! // (All arrays start at sentinel / Default values.)
//! ```

use crate::{AgentRngs, AgentStore, ComponentMap};

/// Fluent builder for [`AgentStore`] + [`AgentRngs`].
///
/// All arrays are pre-allocated at construction time so later field writes
/// (from CSV loaders, etc.) are simple indexed assignments, not pushes.
pub struct AgentStoreBuilder {
    count: usize,
    seed: u64,
    components: ComponentMap,
}

impl AgentStoreBuilder {
    /// Create a builder for `count` agents using `seed` as the global RNG seed.
    ///
    /// `count` is typically the number of rows in the population CSV.
    pub fn new(count: usize, seed: u64) -> Self {
        Self {
            count,
            seed,
            components: ComponentMap::new(),
        }
    }

    /// Register an application-defined component type `T`.
    ///
    /// Every agent will start with `T::default()`.  Must be called before
    /// [`build`](Self::build) — components cannot be added after the store
    /// is constructed.
    ///
    /// Calling this twice for the same `T` is harmless (second call is a
    /// no-op).
    pub fn register_component<T: Default + Send + Sync + 'static>(mut self) -> Self {
        // Register with count=0; build() fills defaults in one batch pass.
        self.components.register::<T>(0);
        self
    }

    /// Construct `AgentStore` and `AgentRngs`.
    ///
    /// All SoA arrays are allocated and filled with sentinel / `Default`
    /// values.  Applications write actual initial state (from CSV, etc.)
    /// directly to the `pub` fields of the returned `AgentStore`.
    pub fn build(mut self) -> (AgentStore, AgentRngs) {
        // Push T::default() once per agent for every registered component.
        for _ in 0..self.count {
            self.components.push_defaults();
        }

        let store = AgentStore::new(self.count, self.components);
        let rngs = AgentRngs::new(self.count, self.seed);

        (store, rngs)
    }
}
