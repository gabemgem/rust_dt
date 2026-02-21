//! Core agent storage: `AgentStore` (SoA data) and `AgentRngs` (per-agent RNG).
//!
//! # Why two structs?
//!
//! The parallel intent phase needs `&mut AgentRngs` (exclusive mutable access to
//! each agent's RNG) and `&AgentStore` (shared read access to world state)
//! simultaneously.  Rust's borrow checker forbids this if both live inside a
//! single struct.  Keeping RNGs in a separate `AgentRngs` struct resolves the
//! conflict cleanly:
//!
//! ```ignore
//! // dt-sim tick loop (simplified):
//! let store: &AgentStore = &sim.store;
//! let intents = sim.rngs.inner
//!     .par_iter_mut()
//!     .enumerate()
//!     .map(|(i, rng)| behavior.replan(AgentId(i as u32), store, rng))
//!     .collect::<Vec<_>>();
//! ```

use dt_core::{AgentId, AgentRng};

#[cfg(feature = "mobility")]
use dt_core::TransportMode;
#[cfg(feature = "schedule")]
use dt_core::{ActivityId, Tick};
#[cfg(feature = "spatial")]
use dt_core::{EdgeId, NodeId};

use crate::component::ComponentMap;

// ── AgentRngs ─────────────────────────────────────────────────────────────────

/// Per-agent deterministic RNG state, separated from [`AgentStore`] to enable
/// simultaneous `&mut AgentRngs` + `&AgentStore` borrows in the parallel phase.
///
/// `AgentRngs` is `Send` (the inner `SmallRng` is `Send`) but intentionally
/// not `Sync` — per-agent RNG state must never be shared between threads.
/// Rayon's `par_iter_mut()` handles the exclusive-per-thread access pattern.
pub struct AgentRngs {
    pub inner: Vec<AgentRng>,
}

impl AgentRngs {
    /// Allocate and seed `count` per-agent RNGs from `global_seed`.
    pub(crate) fn new(count: usize, global_seed: u64) -> Self {
        let inner = (0..count as u32)
            .map(|i| AgentRng::new(global_seed, AgentId(i)))
            .collect();
        Self { inner }
    }

    /// Mutable reference to one agent's RNG.
    #[inline]
    pub fn get_mut(&mut self, agent: AgentId) -> &mut AgentRng {
        &mut self.inner[agent.index()]
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return mutable references to the RNGs for a set of agents.
    ///
    /// Used by dt-sim's parallel intent phase: `agents_to_wake` is zipped with
    /// the returned refs and processed with Rayon.
    ///
    /// # Precondition (enforced by caller)
    ///
    /// `agents` must contain no duplicate `AgentId`s and all indices must be
    /// in-bounds.  Both invariants hold for agents drained from `WakeQueue`
    /// because BTreeMap keys are unique and the simulation never inserts an
    /// `AgentId >= agent_count`.
    pub fn get_many_mut(&mut self, agents: &[AgentId]) -> Vec<&mut AgentRng> {
        let ptr = self.inner.as_mut_ptr();
        // SAFETY: Every `AgentId` in `agents` is unique (caller invariant) and
        // within bounds (simulation invariant).  Each pointer therefore aliases
        // a distinct element of `self.inner`, so no two references overlap.
        agents
            .iter()
            .map(|a| unsafe { &mut *ptr.add(a.index()) })
            .collect()
    }
}

// ── AgentStore ────────────────────────────────────────────────────────────────

/// Structure-of-Arrays storage for all agent state.
///
/// Every `Vec` field has exactly `count` elements; the `AgentId` value is the
/// index into all of them:
///
/// ```ignore
/// let pos = store.node_id[agent.index()];  // O(1), cache-friendly
/// ```
///
/// The feature-gated fields are only compiled when the corresponding Cargo
/// feature is enabled.  Unused features cost zero bytes at runtime.
///
/// Application-defined state lives in [`ComponentMap`] and is accessed via
/// [`AgentStore::component`] / [`AgentStore::component_mut`].
pub struct AgentStore {
    /// Number of agents.  Equals the length of every SoA `Vec`.
    pub count: usize,

    // ── Spatial state ─────────────────────────────────────────────────────
    /// Current road-network node.  `NodeId::INVALID` while the agent is
    /// mid-edge.
    #[cfg(feature = "spatial")]
    pub node_id: Vec<NodeId>,

    /// Edge currently being traversed.  `EdgeId::INVALID` when stationary at
    /// a node.
    #[cfg(feature = "spatial")]
    pub edge_id: Vec<EdgeId>,

    /// Progress along `edge_id` in `[0.0, 1.0)`.  Meaningless when
    /// `edge_id == EdgeId::INVALID`.
    #[cfg(feature = "spatial")]
    pub edge_progress: Vec<f32>,

    // ── Schedule state ────────────────────────────────────────────────────
    /// The tick at which this agent must wake up and call `BehaviorModel::replan`.
    /// The scheduler in `dt-sim` reads this to maintain the wake queue.
    #[cfg(feature = "schedule")]
    pub next_event_tick: Vec<Tick>,

    /// Activity the agent is currently performing.  `ActivityId::INVALID`
    /// means "unassigned / pre-simulation".
    #[cfg(feature = "schedule")]
    pub current_activity: Vec<ActivityId>,

    // ── Mobility state ────────────────────────────────────────────────────
    /// How the agent is currently travelling.  `TransportMode::None` when
    /// stationary.
    #[cfg(feature = "mobility")]
    pub transport_mode: Vec<TransportMode>,

    // ── Application components ────────────────────────────────────────────
    components: ComponentMap,
}

impl AgentStore {
    /// `true` if there are no agents.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterator over all `AgentId`s in ascending index order.
    pub fn agent_ids(&self) -> impl Iterator<Item = AgentId> + '_ {
        (0..self.count as u32).map(AgentId)
    }

    // ── Spatial helpers ───────────────────────────────────────────────────

    /// `true` if the agent is at a road node (not mid-edge).
    #[cfg(feature = "spatial")]
    #[inline]
    pub fn is_at_node(&self, agent: AgentId) -> bool {
        self.edge_id[agent.index()] == EdgeId::INVALID
    }

    /// `true` if the agent is currently traversing an edge.
    #[cfg(feature = "spatial")]
    #[inline]
    pub fn is_moving(&self, agent: AgentId) -> bool {
        self.edge_id[agent.index()] != EdgeId::INVALID
    }

    // ── Component access ──────────────────────────────────────────────────

    /// Read-only slice of application component `T`.
    ///
    /// Returns `None` if `T` was not registered before the store was built.
    /// Index by `agent.index()` to access a specific agent's value.
    pub fn component<T: Default + Send + Sync + 'static>(&self) -> Option<&[T]> {
        self.components.get::<T>()
    }

    /// Mutable reference to the component `Vec<T>`.
    ///
    /// Returns `None` if `T` was not registered.  Only call this during the
    /// apply phase (single-threaded write).
    pub fn component_mut<T: Default + Send + Sync + 'static>(&mut self) -> Option<&mut Vec<T>> {
        self.components.get_mut::<T>()
    }

    /// Reference to the whole `ComponentMap` (e.g. for passing to output writers).
    pub fn components(&self) -> &ComponentMap {
        &self.components
    }

    /// Mutable reference to the `ComponentMap` (e.g. for the apply phase).
    pub fn components_mut(&mut self) -> &mut ComponentMap {
        &mut self.components
    }

    // ── Package-private constructor used by AgentStoreBuilder ─────────────

    pub(crate) fn new(count: usize, components: ComponentMap) -> Self {
        Self {
            count,

            #[cfg(feature = "spatial")]
            node_id: vec![NodeId::INVALID; count],
            #[cfg(feature = "spatial")]
            edge_id: vec![EdgeId::INVALID; count],
            #[cfg(feature = "spatial")]
            edge_progress: vec![0.0_f32; count],

            #[cfg(feature = "schedule")]
            next_event_tick: vec![Tick::ZERO; count],
            #[cfg(feature = "schedule")]
            current_activity: vec![ActivityId::INVALID; count],

            #[cfg(feature = "mobility")]
            transport_mode: vec![TransportMode::None; count],

            components,
        }
    }
}
