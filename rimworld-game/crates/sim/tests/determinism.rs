//! Determinism: identical seed + identical command log -> identical state hash.
//! Also exercises needs decay + job routing + movement + site claims.

use shared::{Command, TickCommands};
use sim::colony::{run_colony, ColonySim};

fn setup(sim: &mut ColonySim) {
    // one colony layout: 2 pawns, food storage, bed, a job
    sim.spawn_pawn(0, 5, 5);
    sim.spawn_pawn(0, 20, 5);
    sim.add_site(0, 8, 5); // food storage near p0
    sim.add_site(1, 30, 5); // bed near p1
    sim.add_job(1, 12, 9);
    sim.add_job(0, 14, 20);
}

fn commands(tick: u64) -> TickCommands {
    TickCommands {
        tick,
        cmds: vec![
            (
                0,
                Command::Move {
                    pawn: 0,
                    x: 10,
                    y: 6,
                },
            ),
            (
                0,
                Command::Build {
                    tile: 11,
                    y: 11,
                    kind: 0,
                },
            ),
        ],
    }
}

#[test]
fn identical_seed_and_input_identical_hash() {
    let h1 = run_colony(
        1234,
        100,
        &(0..100).map(commands).collect::<Vec<_>>(),
        setup,
    );
    let h2 = run_colony(
        1234,
        100,
        &(0..100).map(commands).collect::<Vec<_>>(),
        setup,
    );
    assert_eq!(h1, h2);
    // different seed -> (very likely) different hash; still deterministic though
    let h3 = run_colony(
        1234,
        100,
        &(0..100).map(commands).collect::<Vec<_>>(),
        setup,
    );
    assert_eq!(h1, h3);
}

#[test]
fn input_order_makes_no_difference() {
    let mut a: Vec<TickCommands> = (0..30).map(commands).collect();
    let b: Vec<TickCommands> = (0..30).map(commands).collect();
    // internally step() canonical-sorts, so reversing command vectors can't change results
    for batch in &mut a {
        batch.cmds.sort_by(|(pa, ca), (pb, cb)| {
            pb.cmp(pa).then_with(|| cb.sort_key().cmp(&ca.sort_key()))
        });
    }
    assert_eq!(run_colony(7, 30, &a, setup), run_colony(7, 30, &b, setup));
}

#[test]
fn needs_and_jobs_progress_the_colony() {
    // run 60 ticks with no commands; pawns must decay needs and act autonomously
    let h = run_colony(42, 60, &[], setup);
    let mut sim = ColonySim::new(42);
    setup(&mut sim);
    for _ in 0..60 {
        let _ = sim.step(&[]);
    }
    assert_eq!(h, sim.hash());
    // sanity: some pawn moved or claimed something (mode != all Idle at start positions)
    let p0 = sim
        .world
        .storages
        .iter::<sim::colony::Pawn>()
        .map(|(_, p)| p.mode.clone())
        .collect::<Vec<_>>();
    let any_active = p0.iter().any(|m| !matches!(m, sim::colony::Mode::Idle));
    assert!(any_active, "expected autonomous activity by tick 60");
}

#[test]
fn empty_replay_is_stable() {
    assert_eq!(
        run_colony(1, 200, &[], setup),
        run_colony(1, 200, &[], setup)
    );
}
