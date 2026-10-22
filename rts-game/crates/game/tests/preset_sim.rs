//! Preset map + full sim determinism: run_preset twice, hash equal; install_map matches.

use game::mapgen::{generate_map_preset, run_preset, PRESETS};
use shared::{Command, TickCommands};

fn batches(ticks: u64) -> Vec<TickCommands> {
    (0..ticks)
        .map(|t| TickCommands {
            tick: t,
            cmds: if t % 20 == 5 {
                vec![(0, Command::Move { unit: 3, x: 20, y: 20 })]
            } else {
                Vec::new()
            },
        })
        .collect()
}

#[test]
fn run_preset_twidentical_hash() {
    let bs = batches(120);
    for p in PRESETS {
        let h1 = run_preset(7, 120, &bs, *p);
        let h2 = run_preset(7, 120, &bs, *p);
        assert_eq!(h1, h2, "run_preset mismatch {}", p.name);
    }
}

#[test]
fn install_map_hash_matches_run_preset() {
    let bs = batches(120);
    for p in PRESETS {
        let h = run_preset(11, 120, &bs, *p);
        let mut sim = game::Sim::new(11);
        sim.install_map(generate_map_preset(11, *p));
        let mut bi = 0usize;
        while (sim.tick()) < 120 {
            if bi < bs.len() && bs[bi].tick == sim.tick() {
                let _ = sim.step(&bs[bi].cmds);
                bi += 1;
            } else {
                let _ = sim.step(&[]);
            }
        }
        assert_eq!(h, sim.hash(), "install vs run hash {}", p.name);
    }
}

#[test]
fn preset_nodes_survive_steps() {
    for p in PRESETS {
        let mut sim = game::Sim::new(3);
        sim.install_map(generate_map_preset(3, *p));
        let n0 = sim.world.storages.iter::<game::economy::NodePos>().count();
        let _ = sim.step(&[]);
        let _ = sim.step(&[]);
        let n1 = sim.world.storages.iter::<game::economy::NodePos>().count();
        assert_eq!(n0, n1, "nodes vanished after step");
    }
}
