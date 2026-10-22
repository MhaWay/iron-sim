use game::mapgen::*;

fn gen(seed: u64, p: &Preset) -> MapGen {
    generate_map_preset(seed, *p)
}

#[test]
fn gen_smoke_all_presets() {
    for preset in PRESETS.iter() {
        for seed in 0..25u64 {
            let m = gen(seed, preset);
            assert_eq!(m.base_centers.len(), 2);
        }
    }
}

#[test]
fn digest_determinism_100x() {
    for p in PRESETS {
        for seed in 0..100u64 {
            assert_eq!(gen(seed, p).map_digest(), gen(seed, p).map_digest(), "seed {}", seed);
        }
    }
}

#[test]
fn symmetry_rot180() {
    for p in PRESETS {
        for seed in 0..30u64 {
            let m = gen(seed, p);
            for y in 0..128i16 {
                for x in 0..128i16 {
                    let b1 = m.grid.is_blocked(engine_path::TilePos { x, y });
                    let b2 = m.grid.is_blocked(engine_path::TilePos { x: 127 - x, y: 127 - y });
                    assert_eq!(b1, b2, "sym fail seed {} cell {} {}", seed, x, y);
                }
            }
        }
    }
}

fn counts(m: &MapGen) -> ([u32; 3], [u32; 3]) {
    let mut c1 = [0u32; 3];
    let mut c2 = [0u32; 3];
    for nd in &m.nodes {
        if nd.total > 0 {
            if nd.base == 0 { c1[nd.slot as usize] += 1 } else { c2[nd.slot as usize] += 1 }
        }
    }
    (c1, c2)
}

#[test]
fn fairness_and_connectivity() {
    for p in PRESETS {
        for seed in 0..100u64 {
            let m = gen(seed, p);
            let (c1, c2) = counts(&m);
            assert_eq!(c1, c2, "fair fail seed {} c1 {:?} c2 {:?}", seed, c1, c2);
            assert!(connected(&m), "connect fail seed {}", seed);
        }
    }
}

fn connected(m: &MapGen) -> bool {
    let mut seen = vec![false; 128 * 128];
    let (bx, by) = m.base_centers[0];
    if m.grid.is_blocked(engine_path::TilePos { x: bx, y: by }) {
        return false;
    }
    let mut q = vec![(bx, by)];
    seen[(by as usize) * 128 + (bx as usize)] = true;
    let mut head = 0;
    while head < q.len() {
        let (x, y) = q[head];
        head += 1;
        for (dx, dy) in [(1i16, 0i16), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < 128 && (ny as usize) < 128
                && !m.grid.is_blocked(engine_path::TilePos { x: nx, y: ny })
                && !seen[(ny as usize) * 128 + (nx as usize)]
            {
                seen[(ny as usize) * 128 + (nx as usize)] = true;
                q.push((nx, ny));
            }
        }
    }
    let (cx, cy) = m.base_centers[1];
    seen[(cy as usize) * 128 + (cx as usize)]
}

#[test]
fn metrics_print() {
    let mut max_ms = 0f64;
    let mut total_ms = 0f64;
    let mut n = 0u64;
    for preset in PRESETS.iter() {
        let mut repairs = 0u64;
        let mut fair_sum = 0u32;
        let mut attempts_sum = 0u64;
        for seed in 0..300u64 {
            let t1 = std::time::Instant::now();
            let m = gen(seed, preset);
            let ms = t1.elapsed().as_secs_f64() * 1e3;
            if ms > max_ms { max_ms = ms }
            total_ms += ms;
            n += 1;
            attempts_sum += m.attempts as u64;
            if m.repaired != 0 { repairs += 1 }
            let (c1, c2) = counts(&m);
            for s2 in 0..3 { fair_sum += c1[s2].abs_diff(c2[s2]); }
        }
        println!(
            "PRESET {}: ms/seed={:.3} (max={:.3}) first_draw_rate={:.4} repairs={} fairness_sum={} attempts_sum={}",
            preset.name,
            total_ms / n as f64,
            max_ms,
            1.0 - (repairs as f64) / 300.0,
            repairs,
            fair_sum,
            attempts_sum,
        );
    }
}

fn sim_map_digest(seed: u64, p: &Preset) -> (u64, u64) {
    let mut sim = game::Sim::new(seed);
    let m = gen(seed, p);
    let md = m.map_digest();
    sim.install_map(m);
    (md, sim.hash())
}

#[test]
fn install_map_determinism() {
    for p in PRESETS {
        for seed in 0..50u64 {
            let (d1, h1) = sim_map_digest(seed, p);
            let (d2, h2) = sim_map_digest(seed, p);
            assert_eq!((d1, h1), (d2, h2), "install determinism seed {}", seed);
        }
    }
}

#[test]
fn installed_map_nodes_match_preset() {
    for p in PRESETS {
        let m = gen(5, p);
        let expect = m.nodes.iter().filter(|n| n.total > 0).count();
        let mut sim = game::Sim::new(5);
        sim.install_map(m);
        let n = sim.world.storages.iter::<game::economy::NodePos>().count();
        assert_eq!(n, expect, "node count mismatch for preset {}", p.name);
    }
}
