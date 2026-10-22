// Component-diff probe for snapshot restore divergence (reconstructed 10:17 after churn).
use net::{event_channel, restore_sim, snapshot_from_sim, ClientSession, HostSession, SessionCfg};
use shared::netcode::NetMsg;
use shared::Command;

#[test]
fn diag_snapshot_roundtrip() {
    let seed = 42;
    let (htx, _) = event_channel();
    let mut host = HostSession::new(
        SessionCfg {
            name: "h".into(),
            seed,
            preset: String::new(),
            is_host: true,
        },
        htx,
    );
    let _ = host.handle_msg(Some(1), None, NetMsg::Start);
    for t in 0..300u64 {
        if t % 50 == 10 {
            host.pending
                .entry(t)
                .or_default()
                .push((1, Command::Stop { unit: 15 }));
        }
        host.step_tick().unwrap();
    }
    let snap = snapshot_from_sim(&host.sim);
    let (ct2, _) = event_channel();
    let mut c2 = ClientSession::new(
        SessionCfg {
            name: "c1b".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        ct2,
    );
    c2.sim = game::Sim::new(seed);
    restore_sim(&mut c2.sim, &snap).unwrap();
    let h = &host.sim;
    let c = &c2.sim;
    println!(
        "rng host={:016x} client={:016x} snap={:016x}",
        h.world.rng.state(),
        c.world.rng.state(),
        snap.rng_state
    );
    println!(
        "tick host={} client={} tick_digest host={:016x} snap.hash_after={:016x} client={:016x}",
        h.world.tick,
        c.world.tick,
        h.hash(),
        snap.hash_after,
        c.hash()
    );
    // units
    let mut hu: Vec<(u32, String)> = h
        .world
        .storages
        .iter::<game::sim::Unit>()
        .map(|(e, u)| {
            (
                e.0,
                format!(
                    "o{} k{} hp{} sp{} rn{} pos",
                    u.owner,
                    u.kind,
                    u.hp,
                    u.speed_q,
                    u.range.inner()
                ),
            )
        })
        .collect();
    let mut cu: Vec<(u32, String)> = c
        .world
        .storages
        .iter::<game::sim::Unit>()
        .map(|(e, u)| {
            (
                e.0,
                format!(
                    "o{} k{} hp{} sp{} rn{} pos",
                    u.owner,
                    u.kind,
                    u.hp,
                    u.speed_q,
                    u.range.inner()
                ),
            )
        })
        .collect();
    // add pos component per entity id (separate Pos storage)
    let hpos: Vec<(u32, String)> = h
        .world
        .storages
        .iter::<game::sim::Pos>()
        .map(|(e, p)| (e.0, format!("{} {}", p.x.inner(), p.y.inner())))
        .collect();
    let cpos: Vec<(u32, String)> = c
        .world
        .storages
        .iter::<game::sim::Pos>()
        .map(|(e, p)| (e.0, format!("{} {}", p.x.inner(), p.y.inner())))
        .collect();
    hu.sort_by_key(|a| a.0);
    cu.sort_by_key(|a| a.0);
    println!("units h={} c={}", hu.len(), cu.len());
    let mut d = 0;
    for i in 0..hu.len().max(cu.len()) {
        let a = hu.get(i);
        let b = cu.get(i);
        if a != b {
            if d < 4 {
                println!("UNIT i={} h={:?} c={:?}", i, a, b);
            }
            d += 1;
        }
    }
    println!("unit diffs={} (h={} c={})", d, hu.len(), cu.len());
    // pos
    let mut hp = hpos.clone();
    hp.sort_by_key(|a| a.0);
    let mut cp = cpos.clone();
    cp.sort_by_key(|a| a.0);
    println!("pos h={} c={}", hp.len(), cp.len());
    let mut pd = 0;
    for i in 0..hp.len().max(cp.len()) {
        let a = hp.get(i);
        let b = cp.get(i);
        if a != b {
            if pd < 4 {
                println!("POS i={} h={:?} c={:?}", i, a, b);
            }
            pd += 1;
        }
    }
    println!("pos diffs={}", pd);
    // buildings
    let mut hb: Vec<(u32, String)> = h
        .world
        .storages
        .iter::<game::economy::Building>()
        .map(|(e, b)| {
            (
                e.0,
                format!(
                    "o{} k{} st{} hp{}/{} t{}",
                    b.owner,
                    b.kind,
                    b.status.code(),
                    b.hp,
                    b.max_hp,
                    b.timer
                ),
            )
        })
        .collect();
    let mut cb: Vec<(u32, String)> = c
        .world
        .storages
        .iter::<game::economy::Building>()
        .map(|(e, b)| {
            (
                e.0,
                format!(
                    "o{} k{} st{} hp{}/{} t{}",
                    b.owner,
                    b.kind,
                    b.status.code(),
                    b.hp,
                    b.max_hp,
                    b.timer
                ),
            )
        })
        .collect();
    hb.sort_by_key(|a| a.0);
    cb.sort_by_key(|a| a.0);
    println!("bldg h={} c={}", hb.len(), cb.len());
    let mut bd = 0;
    for i in 0..hb.len().max(cb.len()) {
        let a = hb.get(i);
        let b = cb.get(i);
        if a != b {
            if bd < 4 {
                println!("BLDG i={} h={:?} c={:?}", i, a, b);
            }
            bd += 1;
        }
    }
    println!("bldg diffs={}", bd);
    // nodes
    let mut hn: Vec<(u32, String)> = h
        .world
        .storages
        .iter::<game::economy::ResourceNode>()
        .map(|(e, n)| (e.0, format!("s{} t{} r{}", n.slot, n.total, n.remaining)))
        .collect();
    let mut cn: Vec<(u32, String)> = c
        .world
        .storages
        .iter::<game::economy::ResourceNode>()
        .map(|(e, n)| (e.0, format!("s{} t{} r{}", n.slot, n.total, n.remaining)))
        .collect();
    hn.sort_by_key(|a| a.0);
    cn.sort_by_key(|a| a.0);
    println!("nodes h={} c={}", hn.len(), cn.len());
    let mut nd = 0;
    for i in 0..hn.len().max(cn.len()) {
        let a = hn.get(i);
        let b = cn.get(i);
        if a != b {
            if nd < 4 {
                println!("NODE i={} h={:?} c={:?}", i, a, b);
            }
            nd += 1;
        }
    }
    println!("node diffs={}", nd);
    // orders per unit entity id
    for (e, _u) in h.world.storages.iter::<game::sim::Unit>() {
        let o1 = h
            .world
            .storages
            .get::<game::sim::Order>(engine_ecs::Entity(e.0));
        let o2 = c
            .world
            .storages
            .get::<game::sim::Order>(engine_ecs::Entity(e.0));
        if format!("{o1:?}") != format!("{o2:?}") {
            println!("ORDER id={} h={:?} c={:?}", e.0, o1, o2);
        }
    }
    // players
    for i in 0..h.players.len().min(c.players.len()) {
        if h.players[i] != c.players[i] {
            println!("PLAYER {} h={:?} c={:?}", i, h.players[i], c.players[i]);
        }
    }
    println!("players h={} c={}", h.players.len(), c.players.len());
    // vis
    for i in 0..h.vis.len().min(c.vis.len()) {
        if h.vis[i].words != c.vis[i].words {
            let fd = h.vis[i]
                .words
                .iter()
                .zip(c.vis[i].words.iter())
                .position(|(x, y)| x != y);
            println!(
                "VIS {} words h={} c={} firstdiff={:?}",
                i,
                h.vis[i].words.len(),
                c.vis[i].words.len(),
                fd
            );
        }
    }
    println!("vis h={} c={}", h.vis.len(), c.vis.len());
    // blocked
    println!(
        "blocked h={} c={}",
        h.grid.blocked_count(),
        c.grid.blocked_count()
    );
    assert_eq!(h.hash(), c.hash());
}
