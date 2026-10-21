//! Dump colony oracle vectors — mirrors rimworld-game/crates/sim/tests/determinism.rs.

use shared::{Command, TickCommands};
use engine_ecs;
use sim::colony::{run_colony, ColonySim};

fn setup(sim: &mut ColonySim) {
    sim.spawn_pawn(0, 5, 5);
    sim.spawn_pawn(0, 20, 5);
    sim.add_site(0, 8, 5);
    sim.add_site(1, 30, 5);
    sim.add_job(1, 12, 9);
    sim.add_job(0, 14, 20);
}

fn commands(tick: u64) -> TickCommands {
    TickCommands {
        tick,
        cmds: vec![
            (
                0,
                Command::Move { pawn: 0, x: 10, y: 6 },
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

fn main() {
    debug_hook();
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "../oracle_data".into());
    let out = std::path::Path::new(&out_dir);
    std::fs::create_dir_all(out).unwrap();

    let mut dumps = Vec::new();
    let h1 = run_colony(1234, 100, &(0..100).map(commands).collect::<Vec<_>>(), setup);
    dumps.push(("identical".to_string(), 1234u64, 100u64, h1));
    let b: Vec<_> = (0..30).map(commands).collect();
    let mut a: Vec<_> = b.clone();
    for batch in &mut a {
        batch.cmds.sort_by(|(pa, ca), (pb, cb)| pb.cmp(pa).then_with(|| cb.sort_key().cmp(&ca.sort_key())));
    }
    let h2a = run_colony(7, 30, &a, setup);
    let h2b = run_colony(7, 30, &b, setup);
    assert_eq!(h2a, h2b);
    dumps.push(("reversed".to_string(), 7u64, 30u64, h2b));
    let h3 = run_colony(42, 60, &[], setup);
    dumps.push(("idle60".to_string(), 42u64, 60u64, h3));
    let h4 = run_colony(1, 200, &[], setup);
    dumps.push(("empty200".to_string(), 1u64, 200u64, h4));

    // Per-tick trace of the 1234/100 identical stream (run_colony semantics).
    let mut sim = ColonySim::new(1234);
    setup(&mut sim);
    let stream: Vec<_> = (0..100).map(commands).collect();
    let mut per_tick = Vec::new();
    for i in 0..100u64 {
        if let Some(b) = stream.iter().find(|b| b.tick == i) {
            let _ = sim.step(&b.cmds);
        }
        per_tick.push(sim.hash());
    }
    let json = format!(
        "{{\"configs\":[{}],\"identical_trace1234100\":[{}]}}",
        dumps.iter().map(|(n, s, t, h)| format!("{{\"name\":\"{}\",\"seed\":{},\"ticks\":{},\"hash\":{}}}", n, s, t, h)).collect::<Vec<_>>().join(","),
        per_tick.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );
    std::fs::write(out.join("rim_vectors.json"), &json).unwrap();
    println!("{}", json);
}
// appended debug: print state after 1 identical step
pub fn debug_tick1() {
    use sim::colony::{Job, Mode, Needs, Pawn, Pos, Site};
    let mut sim = ColonySim::new(1234);
    setup(&mut sim);
    let cmds = commands(1).cmds;
    let _ = sim.step(&cmds);
    eprintln!("DEBUG tick1_hash={:x}", sim.hash());
    for (e, p) in sim.world.storages.iter::<Pawn>() {
        let n = sim.world.storages.get::<Needs>(e).unwrap();
        let pos = sim.world.storages.get::<Pos>(e).unwrap();
        let m = match &p.mode {
            Mode::Idle => "Idle".to_string(),
            Mode::Go { path, waypoint } => format!("Go wp={} path=[{}]", waypoint, path.iter().map(|t| format!("({},{})", t.x, t.y)).collect::<Vec<_>>().join(",")),
            Mode::Eat { site, timer } => format!("Eat ({},{}) t={}", site.x, site.y, timer),
            Mode::Sleep { site } => format!("Sleep ({},{})", site.x, site.y),
            Mode::Work { job, site } => format!("Work j={} ({},{})", job, site.x, site.y),
        };
        eprintln!("DEBUG pawn e={} owner={} spd={} mode={} n=({},{},{}) pos=({},{})", e.0, p.owner, p.speed.inner(), m, n.hunger, n.energy, n.mood, pos.x.to_int_trunc(), pos.y.to_int_trunc());
    }
    for (e, s) in sim.world.storages.iter::<Site>() {
        eprintln!("DEBUG site e={} kind={} tile=({},{}) occ={:?}", e.0, s.kind, s.tile.x, s.tile.y, s.occupied_by);
    }
    for (e, j) in sim.world.storages.iter::<Job>() {
        eprintln!("DEBUG job e={} kind={} site=({},{}) asgn={:?} done={}", e.0, j.kind, j.site.x, j.site.y, j.assigned, j.done);
    }
}

fn debug_empty() {
    use sim::colony::{Job, Mode, Needs, Pawn, Pos, Site};
    for (seed, ticks) in [(1u64,4u64),(42u64,4u64),(1234u64,4u64)] {
        let mut sim = ColonySim::new(seed);
        setup(&mut sim);
        for t in 0..ticks {
            let _ = sim.step(&[]);
            let h = sim.hash();
            eprintln!("DEBUG empty seed={} t={} hash={:x}", seed, t, h);
            for (e, p) in sim.world.storages.iter::<Pawn>() {
                let n = sim.world.storages.get::<Needs>(e).unwrap();
                let pos = sim.world.storages.get::<Pos>(e).unwrap();
                let m = match &p.mode {
                    Mode::Idle => "Idle".to_string(),
                    Mode::Go { path, waypoint } => format!("Go wp={} path=[{}]", waypoint, path.iter().map(|t| format!("({},{})", t.x, t.y)).collect::<Vec<_>>().join(",")),
                    Mode::Work { job, site } => format!("Work j={} ({},{})", job, site.x, site.y),
                    _ => "?".to_string(),
                };
                eprintln!("  DEBUG pawn {} mode={} spd={} needs=({},{},{}) pos=({},{})", e.0, m, p.speed.inner(), n.hunger, n.energy, n.mood, pos.x.to_int_trunc(), pos.y.to_int_trunc());
            }
        }
    }
}

fn debug_hook() { debug_tick1(); debug_empty(); }
