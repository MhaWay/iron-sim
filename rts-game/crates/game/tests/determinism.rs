//! Determinism test: two runs of identical command logs -> identical state hashes.
//! If this fails, a desync bug entered sim math or iteration order. Fix the sim, don't tune the test.

use game::{run_sim, Sim};
use shared::{Command, TickCommands};

fn build_command_stream() -> Vec<TickCommands> {
    let mut batches = Vec::new();
    let mut x = 1i16;
    let mut dir = 1i16;
    for t in 0..120u64 {
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        if t % 13 == 3 {
            b.cmds.push((0, Command::Move { unit: 0, x, y: 5 }));
        }
        if t % 13 == 7 {
            b.cmds.push((
                1,
                Command::Move {
                    unit: 1,
                    x: 30 - x,
                    y: 5,
                },
            ));
        }
        if t % 29 == 9 {
            b.cmds.push((2, Command::Attack { unit: 2, target: 0 }));
        }
        x += dir;
        if !(1..=20).contains(&x) {
            dir = -dir;
            x += dir;
        }
        b.sort();
        batches.push(b);
    }
    batches
}

#[test]
fn identical_command_logs_identical_hashes() {
    let stream = build_command_stream();
    let h1 = run_sim(12345, 120, &stream);
    let h2 = run_sim(12345, 120, &stream);
    assert_eq!(h1, h2, "two runs of identical command logs diverged");
    assert_ne!(h1, 0, "degenerate hash");
}

#[test]
fn per_seed_determinism() {
    let stream = build_command_stream();
    assert_eq!(run_sim(1, 120, &stream), run_sim(1, 120, &stream));
}

#[test]
fn canonical_order_makes_input_order_irrelevant() {
    // Two units; command stream lists them in different raw order but canonical sorting
    // normalizes it, so both runs must end identical.
    let mut s1 = Sim::new(9);
    let mut s2 = Sim::new(9);
    s1.spawn_unit(0, 5, 5);
    s1.spawn_unit(1, 30, 5);
    s2.spawn_unit(0, 5, 5);
    s2.spawn_unit(1, 30, 5);
    let unordered = vec![
        (
            1u32,
            Command::Move {
                unit: 1,
                x: 10,
                y: 5,
            },
        ),
        (
            0u32,
            Command::Move {
                unit: 0,
                x: 12,
                y: 5,
            },
        ),
        (0u32, Command::Stop { unit: 1 }),
    ];
    let mut sorted = unordered.clone();
    sorted.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
    s1.step(&sorted).unwrap();
    // s2 gets the same batch applied in reverse raw order; step() sorts internally, so same result.
    let mut reverse = unordered.clone();
    reverse.reverse();
    s2.step(&reverse).unwrap();
    assert_eq!(s1.hash(), s2.hash());
}

#[test]
fn golden_hash_check() {
    // Golden hash: if this fails, you INTENTIONALLY changed sim math — bump SIM_VERSION in
    // crates/game/src/lib.rs and refresh this constant with a git-blame rationale.
    // Never "fix" CI by nudging this number.
    let stream = build_command_stream();
    let h = run_sim(12345, 120, &stream);
    let golden: u64 = 8358503953490564257; // 0x73FF_5F0F_7980_D4A1 (SIM_VERSION=2, Phase-1 economy: Build/Train/Gather/Map/Vis/Harvest/TrainQueue systems). Refresh deliberately on intentional sim-math change — bump SIM_VERSION first.
    println!("current={:016x} golden={:016x}", h, golden);
    assert_eq!(h, golden, "golden hash mismatch — if sim math changed intentionally, bump SIM_VERSION and update this line");
}
