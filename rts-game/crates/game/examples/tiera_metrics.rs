//! Tier A metric bar (lead): attrition rate, income balance, fairness delta per mechanic.
//! Idle soak: regen+upkeep tables only. Soak: full demo script (combat + economy).
//! Run: cargo run --release -p game --example tiera_metrics [seeds=20]
use game::mapgen::{generate_map_preset, preset_by_name};
use game::sim::Sim;
use std::time::Instant;

fn units(sim: &Sim, p: u8) -> u32 {
    sim.world
        .storages
        .iter::<game::sim::Unit>()
        .filter(|(_, u)| u.owner == p)
        .count() as u32
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|x| x.parse().ok()).unwrap_or(20);
    let t_total: u64 = 12_000;
    let t_soak: u64 = 100_000;
    println!("=== Tier A metrics: {} seeds/preset ===", seeds);

    // Idle soak: regen + upkeep tables (no commands, no combat).
    println!(
        "--- idle soak {}t (regen/upkeep tables active, attrition only from starvation) ---",
        t_total
    );
    println!("preset attr_p0% attr_p1% inc_bal_abs fairness gen_ms");
    for name in ["arena", "black_forest"] {
        let preset = preset_by_name(name).unwrap();
        let (mut a0s, mut a1s, mut sbs, mut fds, mut gm) = (0f64, 0f64, 0f64, 0f64, 0f64);
        for s in 1..=seeds {
            let t0 = Instant::now();
            let map = generate_map_preset(s, *preset);
            gm += t0.elapsed().as_secs_f64() * 1000.0;
            let mut sim = Sim::new(s);
            sim.install_map(map);
            let (init0, init1) = (units(&sim, 0), units(&sim, 1));
            let w0 = sim.players[0].stock.wood;
            let w1 = sim.players[1].stock.wood;
            for _ in 0..t_total {
                sim.step(&[]).unwrap();
            }
            let (s0, s1) = (units(&sim, 0), units(&sim, 1));
            a0s += 100.0 * (init0 - s0) as f64 / init0.max(1) as f64;
            a1s += 100.0 * (init1 - s1) as f64 / init1.max(1) as f64;
            sbs +=
                ((sim.players[0].stock.wood - w0) - (sim.players[1].stock.wood - w1)).abs() as f64;
            fds += (init0 as i64 - init1 as i64).abs() as f64;
        }
        let n = seeds as f64;
        println!(
            "{:12} {:6.1} {:6.1} {:11.0} {:7.0} {:6.2}",
            name,
            a0s / n,
            a1s / n,
            sbs / n,
            fds / n,
            gm / n
        );
    }

    // Long soak: full demo policy (build/gather/train/attack) replicated inline so the
    // example stays inside the game crate (no render dependency). Same ids as demo (s0 = p*4+3).
    println!(
        "--- soak {}t (demo policy: combat + economy, attrition + winners) ---",
        t_soak
    );
    println!("preset init_p0 init_p1 alive_p0 alive_p1 attr_p0% attr_p1% fairness gen_ms winners p0/p1/draw");
    for name in ["arena", "black_forest"] {
        let preset = preset_by_name(name).unwrap();
        let (mut a0s, mut a1s, mut fd_s, mut gm, mut w0c, mut w1c, mut wdc) =
            (0f64, 0f64, 0f64, 0f64, 0u64, 0u64, 0u64);
        let (mut init0_s, mut init1_s, mut alive0_s, mut alive1_s) = (0f64, 0f64, 0f64, 0f64);
        for s in 1..=seeds {
            let t0 = Instant::now();
            let map = generate_map_preset(s, *preset);
            gm += t0.elapsed().as_secs_f64() * 1000.0;
            let mut sim = Sim::new(s);
            sim.install_map(map);
            let init0 = units(&sim, 0) as f64;
            let init1 = units(&sim, 1) as f64;
            for t in 0..t_soak {
                let mut cmds: Vec<(u32, shared::Command)> = Vec::new();
                for p in 0..2u32 {
                    let s0 = p * 4 + 3;
                    if t == 5 {
                        cmds.push((
                            p,
                            shared::Command::Gather {
                                unit: s0,
                                resource: 1,
                            },
                        ));
                        cmds.push((
                            p,
                            shared::Command::Gather {
                                unit: s0 + 1,
                                resource: 0,
                            },
                        ));
                    }
                    if t == 8 {
                        // Preset-aware build anchors: base-center + 7 offset (same for both sides by rotation symmetry)
                        let (bx, by) = game::mapgen::base_centers(2)[p as usize];
                        // Farm anchored 7 tiles inside each base corner (rot180-symmetric about map center).
                        let (x, y) = (bx + 7, by + 7);
                        cmds.push((
                            p,
                            shared::Command::Build {
                                building: 1,
                                x,
                                y,
                                level: 1,
                            },
                        ));
                    }
                    if t == 40 {
                        let (x, y) = if p == 0 { (25, 18) } else { (102, 109) };
                        cmds.push((
                            p,
                            shared::Command::Build {
                                building: 3,
                                x,
                                y,
                                level: 1,
                            },
                        ));
                    }
                    if t == 70 {
                        // Wall anchors are exact rot180 mirrors (45,45) <-> (82,82); (83,83) kept per legacy
                        let (x, y) = if p == 0 { (45, 45) } else { (83, 83) };
                        cmds.push((
                            p,
                            shared::Command::Build {
                                building: 8,
                                x,
                                y,
                                level: 1,
                            },
                        ));
                    }
                    if t == 120 {
                        cmds.push((p, shared::Command::AgeUp { age: 1 }));
                    }
                    if t >= 300 && t % 150 == 0 {
                        let bar = sim.first_building_entity_of(p, 3);
                        if bar != 0 {
                            cmds.push((
                                p,
                                shared::Command::Train {
                                    building: bar,
                                    unit_kind: 1,
                                },
                            ));
                        }
                    }
                    if t == 30 || t == 500 || t == 900 {
                        let other = (1 - p) * 4 + 3;
                        cmds.push((
                            p,
                            shared::Command::Attack {
                                unit: s0,
                                target: other,
                            },
                        ));
                    }
                    if t >= 1200 && t % 97 == 0 {
                        let otc = sim.first_building_entity_of(1 - p, 0);
                        if otc != 0 {
                            cmds.push((
                                p,
                                shared::Command::Attack {
                                    unit: s0 + 2,
                                    target: otc,
                                },
                            ));
                        }
                    }
                }
                sim.step(&cmds).unwrap();
                if sim.players[0].winner != 0 || sim.players[1].winner != 0 {
                    break;
                }
            }
            let e0 = units(&sim, 0);
            let e1 = units(&sim, 1);
            init0_s += init0;
            init1_s += init1;
            alive0_s += e0 as f64;
            alive1_s += e1 as f64;
            a0s += 100.0 * (init0 - e0 as f64) / init0.max(1.0);
            a1s += 100.0 * (init1 - e1 as f64) / init1.max(1.0);
            fd_s += (init0 - init1).abs();
            let w = sim.players[0].winner;
            if w == 1 {
                w0c += 1;
            } else if w == 2 {
                w1c += 1;
            } else {
                wdc += 1;
            }
        }
        let n = seeds as f64;
        println!(
            "{:12} {:8.1} {:8.1} {:8.1} {:8.1} {:8.1} {:8.1} {:8.0} {:6.2} {} / {} / {}",
            name,
            init0_s / n,
            init1_s / n,
            alive0_s / n,
            alive1_s / n,
            a0s / n,
            a1s / n,
            fd_s / n,
            gm / n,
            w0c,
            w1c,
            wdc
        );
    }
    println!("attr = (init - alive)/init; fairness = avg |init_p0 - init_p1| (expect 0); winners by tick 36000 score cap or elimination (w=1 -> p0, w=2 -> p1)");
}
