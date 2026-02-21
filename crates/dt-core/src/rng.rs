//! Deterministic per-agent and simulation-level RNG wrappers.
//!
//! # Determinism strategy
//!
//! Each agent gets its own independent `SmallRng` seeded by:
//!
//!   seed = global_seed XOR (agent_id * MIXING_CONSTANT)
//!
//! The mixing constant is the 64-bit fractional part of the golden ratio,
//! which spreads consecutive agent IDs uniformly across the seed space.
//! This means:
//!
//! - Agents never share RNG state (no contention, no ordering dependency).
//! - Adding or removing agents at the end of the list does not disturb the
//!   seeds of existing agents — runs are reproducible even as populations grow.
//! - All RNG calls are local to the owning thread; no synchronisation needed.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::AgentId;

/// 64-bit fractional golden-ratio constant for seed mixing.
const MIXING_CONSTANT: u64 = 0x9e37_79b9_7f4a_7c15;

// ── AgentRng ──────────────────────────────────────────────────────────────────

/// Per-agent deterministic RNG.
///
/// Create one per agent at simulation init; store in a parallel `Vec<AgentRng>`
/// alongside the other SoA arrays.  The type is `!Sync` to prevent accidental
/// sharing across threads — each Rayon worker must hold its own slice.
pub struct AgentRng(SmallRng);

impl AgentRng {
    /// Seed deterministically from the run's global seed and an agent ID.
    pub fn new(global_seed: u64, agent: AgentId) -> Self {
        let seed = global_seed ^ (agent.0 as u64).wrapping_mul(MIXING_CONSTANT);
        AgentRng(SmallRng::seed_from_u64(seed))
    }

    /// Expose the inner `SmallRng` for use with `rand` distribution types
    /// (`rng.inner().sample(...)`, `rng.inner().gen_range(...)`, etc.)
    #[inline]
    pub fn inner(&mut self) -> &mut SmallRng {
        &mut self.0
    }

    /// Sample a uniformly distributed value of any `Standard`-distributed type.
    #[inline]
    pub fn random<T>(&mut self) -> T
    where
        rand::distributions::Standard: rand::distributions::Distribution<T>,
    {
        self.0.r#gen()
    }

    /// Generate a value uniformly in `range`.
    #[inline]
    pub fn gen_range<T, R>(&mut self, range: R) -> T
    where
        T: rand::distributions::uniform::SampleUniform,
        R: rand::distributions::uniform::SampleRange<T>,
    {
        self.0.gen_range(range)
    }

    /// `true` with probability `p` (clamped to [0, 1]).
    #[inline]
    pub fn gen_bool(&mut self, p: f64) -> bool {
        self.0.gen_bool(p.clamp(0.0, 1.0))
    }

    /// Shuffle a mutable slice in-place (Fisher-Yates).
    #[inline]
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        use rand::seq::SliceRandom;
        slice.shuffle(&mut self.0);
    }

    /// Choose a random element from a non-empty slice.
    /// Returns `None` if the slice is empty.
    #[inline]
    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        use rand::seq::SliceRandom;
        slice.choose(&mut self.0)
    }
}

// ── SimRng ────────────────────────────────────────────────────────────────────

/// Simulation-level RNG for global operations (network evolution, exogenous
/// events, etc.).
///
/// Used only in single-threaded or explicitly synchronised contexts.  If you
/// need parallel randomness, give each worker thread its own `SimRng` seeded
/// from this one.
pub struct SimRng(SmallRng);

impl SimRng {
    pub fn new(seed: u64) -> Self {
        SimRng(SmallRng::seed_from_u64(seed))
    }

    /// Derive a child `SimRng` with a different seed offset — useful for
    /// seeding per-thread RNGs deterministically from the root seed.
    pub fn child(&mut self, offset: u64) -> SimRng {
        let child_seed: u64 = self.0.r#gen::<u64>() ^ offset.wrapping_mul(MIXING_CONSTANT);
        SimRng(SmallRng::seed_from_u64(child_seed))
    }

    #[inline]
    pub fn inner(&mut self) -> &mut SmallRng {
        &mut self.0
    }

    #[inline]
    pub fn random<T>(&mut self) -> T
    where
        rand::distributions::Standard: rand::distributions::Distribution<T>,
    {
        self.0.r#gen()
    }

    #[inline]
    pub fn gen_range<T, R>(&mut self, range: R) -> T
    where
        T: rand::distributions::uniform::SampleUniform,
        R: rand::distributions::uniform::SampleRange<T>,
    {
        self.0.gen_range(range)
    }

    #[inline]
    pub fn gen_bool(&mut self, p: f64) -> bool {
        self.0.gen_bool(p.clamp(0.0, 1.0))
    }
}
