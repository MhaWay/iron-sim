//! Determinism test: two runs of identical command logs -> identical state hashes.
//! If this fails, a desync bug entered sim math or iteration order. Fix the sim, don't tune the test.

use game::{run_sim, Sim};
use shared::{Command, TickCommands};

// NOTE ON UNIT IDS: side-storage markers occupy entity ids 0..2 (Deposits,
// GridSide, VisMirror) and 24 resource nodes + two Town Centers spawn before
// any unit, so the first Serfs are entity ids > 2. The stream below uses ids
// 0/1/2 — any Move/Attack on them resolves to UnknownUnit and is silently
// ignored (commands must reference real spawned units). This keeps the golden
// stream cheap but exercises only map/fog/idle ticks; the economy test below
// captures REAL spawned ids from spawn_unit() instead, per lead guidance.
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

#[test]
fn economy_build_train_harvest_determinism() {
    // Wood-only economy stream: place a Farm (wood-only cost, no gold), order
    // one worker to harvest wood, and run exactly one full cycle: walk (~24t)
    // + pick (150t) + carry back (~24t) + deposit. After 260 idle ticks the
    // deposit must have landed (wood 90 -> 110 after the Farm's 50-wood spend
    // wait no: start 150, Farm costs 50 -> 100, deposit +10 -> 110). Two
    // identical runs must agree on both hash and stock.
    let run = || -> (u64, i64) {
        let mut sim = Sim::new(777);
        let worker = sim.spawn_unit(0, 18, 16);
        let mut farm = None;
        'farm: for fy in 12i16..36 {
            for fx in 12i16..36 {
                if sim.build_at(0, 1, fx, fy).is_ok() {
                    farm = Some((fx, fy));
                    break 'farm;
                }
            }
        }
        assert!(farm.is_some(), "Farm placeable nowhere");
        assert_eq!(sim.players[0].stock.wood, 100, "Farm cost not deducted");
        sim.step(&[(
            0,
            Command::Gather {
                unit: worker,
                resource: 1,
            },
        )])
        .unwrap();
        for _ in 0..260u64 {
            sim.step(&[]).unwrap();
        }
        (sim.hash(), sim.players[0].stock.wood)
    };
    let (h1, w1) = run();
    let (h2, w2) = run();
    assert_eq!(
        h1, h2,
        "economy command stream diverged across identical runs"
    );
    assert_eq!(w1, w2);
    assert_eq!(
        w1, 110,
        "one full harvest deposit cycle did not complete: wood={}",
        w1
    );
}
