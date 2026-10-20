//! Iron-sim RTS — deterministic lockstep simulation.
//!
//! Hard rules (see AGENTS.md + docs/engine-api.md): single-threaded, fixed tick rate 30 Hz,
//! Q16.16 fixed-point only (engine-core::Fixed1616), NO f32/f64, NO default-hash HashMap in sim
//! code, NO global RNG, iteration order = ECS entity index / Vec order only.
//!
//! SIM_VERSION: bump deliberately whenever sim math changes; golden hash depends on it.

pub mod sim;

pub use sim::{run_sim, state_hash, Sim, SimError};

/// Bump deliberately on intentional sim-math changes.
pub const SIM_VERSION: u32 = 1;
/// Fixed tick rate. The host (server/client) owns the clock; the sim only advances ticks.
pub const TICK_HZ: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke_spawn_tick() {
        let mut sim = Sim::new(7);
        let id = sim.spawn_unit(0, 5, 5);
        sim.step(&[(
            0,
            shared::Command::Move {
                unit: id,
                x: 10,
                y: 5,
            },
        )])
        .unwrap();
        assert_eq!(sim.world.tick, 1);
        assert!(sim.alive_units() > 0);
    }
}
