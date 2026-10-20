//! Colony simulation — deterministic lockstep (30 Hz), single-threaded.
//!
//! Rules (AGENTS.md + docs/engine-api.md): no f32/f64, no default-hash HashMap, SplitMix64 only,
//! iteration by ECS entity index, fixed tick rate, SIM_VERSION folds into the state hash.

pub mod colony;

pub use colony::{run_colony, state_hash, ColonySim, SimError};

/// Bump deliberately on intentional sim-math changes.
pub const SIM_VERSION: u32 = 1;
/// Fixed tick rate. The host owns the clock; the sim only advances ticks.
pub const TICK_HZ: u64 = 30;
