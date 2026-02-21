//! Type-erased, heterogeneous component storage.
//!
//! # Design
//!
//! Each component type `T` is stored as a `Vec<T>` behind a
//! `Box<dyn ComponentVec>` in a `HashMap<TypeId, …>`.  Indexing is always by
//! `AgentId` (i.e., `vec[agent.index()]`), so component arrays are kept the
//! same length as `AgentStore::count` at all times.
//!
//! # Usage
//!
//! ```rust
//! use dt_agent::ComponentMap;
//!
//! #[derive(Default)]
//! struct Health(f32);
//!
//! let mut map = ComponentMap::new();
//! // Register before agents are added (count = 0 here, filled during build).
//! map.register::<Health>(0);
//! assert!(map.contains::<Health>());
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;

// ── Trait object ──────────────────────────────────────────────────────────────

/// Type-erased interface for a per-agent `Vec<T>`.
///
/// The trait is sealed (only implementable inside this crate) via the private
/// `Sealed` supertrait, preventing external implementations that could break
/// length invariants.
pub trait ComponentVec: Send + Sync + 'static + sealed::Sealed {
    /// Append `T::default()` for a newly created agent.
    fn push_default(&mut self);

    /// Current element count (should always equal `AgentStore::count`).
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[doc(hidden)]
    fn as_any(&self) -> &dyn Any;

    #[doc(hidden)]
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

mod sealed {
    pub trait Sealed {}
}

// ── Concrete wrapper ──────────────────────────────────────────────────────────

/// A `Vec<T>` wrapped so it can be stored as `Box<dyn ComponentVec>`.
///
/// This type is `pub` to allow checkpoint crates to downcast to it, but
/// should not generally be constructed directly — use [`ComponentMap::register`].
pub struct TypedComponentVec<T: Default + Send + Sync + 'static>(pub Vec<T>);

impl<T: Default + Send + Sync + 'static> sealed::Sealed for TypedComponentVec<T> {}

impl<T: Default + Send + Sync + 'static> ComponentVec for TypedComponentVec<T> {
    fn push_default(&mut self) {
        self.0.push(T::default());
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ── ComponentMap ──────────────────────────────────────────────────────────────

/// Registry of application-defined component arrays, one `Vec<T>` per type.
///
/// Component arrays are always the same length as `AgentStore::count`.  When
/// a new agent is appended, [`ComponentMap::push_defaults`] pushes
/// `T::default()` for every registered type in a single pass.
///
/// # Thread safety
///
/// `ComponentMap` is `Send + Sync` because `ComponentVec: Send + Sync`.
/// During the parallel intent phase `&ComponentMap` is freely shared.
/// Mutation only occurs in the single-threaded apply phase via `&mut AgentStore`.
#[derive(Default)]
pub struct ComponentMap {
    map: HashMap<TypeId, Box<dyn ComponentVec>>,
}

impl ComponentMap {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Register component type `T`, pre-filling `current_count` default values.
    ///
    /// Calling this twice for the same `T` is a no-op — existing data is not
    /// disturbed.  This makes it safe to call from multiple setup paths.
    pub fn register<T: Default + Send + Sync + 'static>(&mut self, current_count: usize) {
        let key = TypeId::of::<T>();
        if self.map.contains_key(&key) {
            return;
        }
        let mut vec = TypedComponentVec::<T>(Vec::with_capacity(current_count));
        for _ in 0..current_count {
            vec.push_default();
        }
        self.map.insert(key, Box::new(vec));
    }

    /// Append `T::default()` for every registered component type.
    ///
    /// Called once per new agent by [`AgentStoreBuilder::build`] and by
    /// `AgentStore::push_agent`.
    pub(crate) fn push_defaults(&mut self) {
        for vec in self.map.values_mut() {
            vec.push_default();
        }
    }

    // ── Read access ───────────────────────────────────────────────────────

    /// Shared slice of component `T` for all agents (indexed by `AgentId`).
    ///
    /// Returns `None` if `T` was never registered.  In the hot path, prefer
    /// storing the slice reference outside the tick loop rather than calling
    /// this every tick.
    pub fn get<T: Default + Send + Sync + 'static>(&self) -> Option<&[T]> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|v| v.as_any().downcast_ref::<TypedComponentVec<T>>())
            .map(|v| v.0.as_slice())
    }

    /// Mutable reference to the component `Vec<T>`.
    ///
    /// Returns `None` if `T` was never registered.
    pub fn get_mut<T: Default + Send + Sync + 'static>(&mut self) -> Option<&mut Vec<T>> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.as_any_mut().downcast_mut::<TypedComponentVec<T>>())
            .map(|v| &mut v.0)
    }

    // ── Metadata ──────────────────────────────────────────────────────────

    /// Number of distinct component types currently registered.
    pub fn type_count(&self) -> usize {
        self.map.len()
    }

    /// `true` if component `T` has been registered.
    pub fn contains<T: Default + Send + Sync + 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }
}
