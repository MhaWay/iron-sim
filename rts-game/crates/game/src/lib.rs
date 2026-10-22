//! Iron-sim RTS — deterministic lockstep simulation.
//!
//! Hard rules (see AGENTS.md + docs/engine-api.md): single-threaded, fixed tick rate 30 Hz,
//! Q16.16 fixed-point only (engine-core::Fixed1616), NO f32/f64, NO default-hash HashMap in sim
//! code, NO global RNG, iteration order = ECS entity index / Vec order only.
//!
//! SIM_VERSION: bump deliberately whenever sim math changes; golden hash depends on it.

pub mod economy;
pub mod mapgen;
pub mod sim;

pub use sim::{run_sim, Sim, SimError};

/// Bump deliberately on intentional sim-math changes.
/// v3 (2026-10-21): AoE loop — AgeUp costs/timers (AGE_UP_TICKS/AGE_COST), building pop gates
/// + pop caps (TC/Farm supply), RallyPoint/UnitRally components, Wall/Factory building kinds,
///   building-destroyable combat (Wreck status), victory_check (TC kill / MATCH_TICKS score), and
///   PlayerState age/score/winner fields. All hash-visible; golden stream re-baselined in
///   tests/determinism.rs. Command wire format unchanged (tags 1-10).
///
/// v4 (2026-10-22): Tier A long-match mechanics — node regen waves + army upkeep/starvation
///   (see docs/tier-a-longmatch.md).
///
/// v5 (2026-10-22): Tier A.4 siege/ranged pressure — melee damage falloff vs walls by
///   wall level (WALL_MELEE_FALLOFF), siege bonus (SIEGE_BUILDING_MULT vs buildings,
///   SIEGE_UNIT_MULT vs units) + deterministic Gather auto-loop (worker re-targets the nearest
///   live node of the same slot after a deposit; no rng). Command::Build grew a trailing
///   level byte (wall tier 1..=3, 0/ignored for non-walls) — wire tag 3 layout changed and
///   netcode encode/decode updated; golden stream re-baselined. All sim math stays integer /
///   Fixed1616 only.
pub const SIM_VERSION: u32 = 5;
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
