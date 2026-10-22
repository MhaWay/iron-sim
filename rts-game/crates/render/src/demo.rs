//! Demo AI: deterministic scripted opponent per side; commands reference real entity ids
//! captured live (spawn order is fixed: 3 Serfs per side after map gen -> ids p*4+3, +1, +2).
//! NO randomness. Commands must be valid for Sim::new state: builds are issued via build_at-like
//! Command::Build which places a SITE (apply_build validates footprint + costs + blocked).

use game::sim::Sim;
use shared::{Command, TickCommands};

/// Build one tick's command batch.
pub fn policy(sim: &mut Sim, tick: u64) -> Vec<(u32, Command)> {
    let mut out = Vec::new();
    for p in 0..2u32 {
        let s0 = p * 4 + 3;
        if tick == 5 {
            out.push((
                p,
                Command::Gather {
                    unit: s0,
                    resource: 1,
                },
            ));
            out.push((
                p,
                Command::Gather {
                    unit: s0 + 1,
                    resource: 0,
                },
            ));
        }
        if tick == 8 {
            // Farm near own TC (coords picked clear via probes: seed-independent near-TC band)
            let (x, y) = if p == 0 { (21, 21) } else { (108, 108) };
            out.push((
                p,
                Command::Build {
                    building: 1,
                    x,
                    y,
                    level: 1,
                },
            ));
        }
        if tick == 40 {
            let (x, y) = if p == 0 { (25, 18) } else { (104, 111) };
            out.push((
                p,
                Command::Build {
                    building: 3,
                    x,
                    y,
                    level: 1,
                },
            ));
        }
        if tick == 70 {
            let (x, y) = if p == 0 { (45, 45) } else { (83, 83) };
            out.push((
                p,
                Command::Build {
                    building: 8,
                    x,
                    y,
                    level: 1,
                },
            ));
        }
        if tick == 120 {
            out.push((p, Command::AgeUp { age: 1 }));
        }
        if tick >= 300 && tick.is_multiple_of(150) {
            let bar = sim.first_building_entity_of(p, 3);
            if bar != 0 {
                out.push((
                    p,
                    Command::Train {
                        building: bar,
                        unit_kind: 1,
                    },
                ));
            }
        }
        if tick == 30 || tick == 500 || tick == 900 {
            let other = (1 - p) * 4 + 3;
            out.push((
                p,
                Command::Attack {
                    unit: s0,
                    target: other,
                },
            ));
        }
        if tick >= 1200 && tick.is_multiple_of(97) {
            let other_tc = sim.first_building_entity_of(1 - p, 0);
            if other_tc != 0 {
                out.push((
                    p,
                    Command::Attack {
                        unit: s0 + 2,
                        target: other_tc,
                    },
                ));
            }
        }
    }
    out
}

pub fn demo_commands(seed: u64, ticks: u64) -> Vec<TickCommands> {
    let mut sim = Sim::new(seed);
    let mut out = Vec::new();
    for t in 0..ticks {
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        b.cmds.extend(policy(&mut sim, t));
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use game::run_sim;
    #[test]
    fn demo_stream_deterministic() {
        let a = demo_commands(7, 1500);
        let h1 = run_sim(7, 1500, &a);
        let h2 = run_sim(7, 1500, &a);
        assert_eq!(h1, h2);
    }
}
