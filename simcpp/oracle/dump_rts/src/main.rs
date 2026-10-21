//! Dump RTS oracle vectors: golden-run final hash + economy test hash/wood.
//! Stream construction mirrors rts-game/crates/game/tests/determinism.rs.

use game::sim::Sim;
use shared::Command;

fn zigzag(ticks: u64) -> Vec<i16> {
    let mut v = Vec::new();
    let mut x = 1i16;
    let mut dir = 1i16;
    for _ in 0..ticks {
        v.push(x);
        x += dir;
        if x > 20 { dir = -1; }
        if x < 1 { dir = 1; }
    }
    v
}

fn golden_stream(ticks: u64) -> Vec<Vec<(u32, Command)>> {
    let xs = zigzag(ticks);
    (0..ticks)
        .map(|t| {
            let mut cmds: Vec<(u32, Command)> = Vec::new();
            if t % 13 == 3 {
                cmds.push((0, Command::Move { unit: 0, x: xs[t as usize], y: 5 }));
            }
            if t % 13 == 7 {
                cmds.push((1, Command::Move { unit: 1, x: 30 - xs[t as usize], y: 5 }));
            }
            if t % 29 == 9 {
                cmds.push((2, Command::Attack { unit: 2, target: 0 }));
            }
            cmds.sort_by_key(|(p, c)| (*p, c.sort_key()));
            cmds
        })
        .collect()
}

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "../oracle_data".into());
    let out = std::path::Path::new(&out_dir);
    std::fs::create_dir_all(out).unwrap();

    // 1. Golden run: seed 12345, 120 ticks.
    let mut sim = Sim::new(12345);
    let stream = golden_stream(120);
    for cmds in stream.iter() {
        sim.step(cmds).unwrap();
    }
    let golden = sim.hash();

    // 2. Economy run: seed 777, build Farm near (18,16), Gather wood, 261 steps.
    let mut e = Sim::new(777);
    let w2 = e.spawn_unit(0, 18, 16);
    'scan: for fy in 12i16..36 {
        for fx in 12i16..36 {
            if e.build_at(0, 1, fx, fy).is_ok() {
                break 'scan;
            }
        }
    }
    let mut s2_cmds: Vec<(u32, Command)> = vec![(0, Command::Gather { unit: w2, resource: 1 })];
    // command tick applied via same API as step()
    e.step(&mut s2_cmds).unwrap();
    for _ in 0..260 {
        e.step(&[]).unwrap();
    }
    let econ_hash = e.hash();
    let wood = e.players[0].stock.wood;

    let json = format!(
        "{{\"rts\":{{\"golden_hash\":{},\"economy_hash\":{},\"economy_wood\":{}}}}}",
        golden, econ_hash, wood
    );
    // Per-tick trace of golden run.
    let mut sim2 = Sim::new(12345);
    let mut per_tick = Vec::new();
    for cmds in stream.iter() {
        let _ = sim2.step(cmds);
        per_tick.push(sim2.hash());
    }
    let json2 = format!("{{\"golden_trace12345120\":[{}]}}", per_tick.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
    std::fs::write(out.join("rts_trace.json"), &json2).unwrap();

    // 3. Economy per-tick trace (777): step the gather batch then 260 idle steps.
    let mut e2 = Sim::new(777);
    let w3 = e2.spawn_unit(0, 18, 16);
    'scan2: for fy in 12i16..36 {
        for fx in 12i16..36 {
            if e2.build_at(0, 1, fx, fy).is_ok() { break 'scan2; }
        }
    }
    // Debug: print farm pos + gather state
    {
        let mut dbg = Sim::new(777);
        let u3 = dbg.spawn_unit(0, 18, 16);
        let mut fp = None;
        'scan3: for fy in 12i16..36 {
            for fx in 12i16..36 {
                if dbg.build_at(0, 1, fx, fy).is_ok() { fp = Some((fx, fy)); break 'scan3; }
            }
        }
        eprintln!("DEBUG farm={:?} unit={:?}", fp, u3);
        let mut cmd: Vec<(u32, Command)> = vec![(0, Command::Gather { unit: u3, resource: 1 })];
        let _ = dbg.step(&mut cmd);
        eprintln!("DEBUG step1 hash={} wood={}", dbg.hash(), dbg.players[0].stock.wood);
        // dump internals
        dbg.world.storages.iter::<game::economy::Building>().for_each(|(e, b)| eprintln!("DEBUG bld e={} kind={} owner={} status={:?} hp={} max={} timer={} pos={:?}", e.0, b.kind, b.owner, b.status, b.hp, b.max_hp, b.timer, dbg.world.storages.get::<game::sim::Pos>(e).map(|p| (p.x.to_int_trunc(), p.y.to_int_trunc()))));
        dbg.world.storages.iter::<game::sim::Unit>().for_each(|(e, u)| eprintln!("DEBUG unit e={} owner={} kind={} hp={} spd={} dmg={} rng={} pos={:?} order={:?}", e.0, u.owner, u.kind, u.hp, u.speed_q, u.dmg, u.range.0, dbg.world.storages.get::<game::sim::Pos>(e).map(|p| (p.x.to_int_trunc(), p.y.to_int_trunc())), dbg.world.storages.get::<game::sim::Order>(e)));
    }
    let mut econ_trace = Vec::new();
    let mut sc: Vec<(u32, Command)> = vec![(0, Command::Gather { unit: w3, resource: 1 })];
    let _ = e2.step(&mut sc);
    econ_trace.push(e2.hash());
    for _ in 0..260 {
        e2.step(&[]);
        econ_trace.push(e2.hash());
    }
    let json3 = format!("{{\"economy_trace777\":[{}]}}", econ_trace.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
    std::fs::write(out.join("rts_econ_trace.json"), &json3).unwrap();
    std::fs::write(out.join("rts_vectors.json"), &json).unwrap();
    println!("{}", json);
}