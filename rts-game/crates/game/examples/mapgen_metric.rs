//! Metrics printout for the new generator. Usage:
//! cargo run -p game --release --example mapgen_metric [n_seeds]
use game::mapgen::{generate_map_preset, Preset, PRESETS};

fn gen(seed: u64, p: &Preset) -> game::mapgen::MapGen {
    generate_map_preset(seed, *p)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    println!("preset,seeds,ms_per_seed,max_ms,first_draw_pass_rate,repairs,fairness_delta");
    for preset in PRESETS.iter() {
        let mut ms_sum = 0f64;
        let mut ms_max = 0f64;
        let mut first_draw = 0u64;
        let mut repairs = 0u64;
        let mut fairness_sum = 0u64;
        for seed in 0..n {
            let t = std::time::Instant::now();
            let m = gen(seed, preset);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            ms_sum += ms;
            if ms > ms_max {
                ms_max = ms;
            }
            if m.repaired == 0 {
                first_draw += 1;
            } else {
                repairs += 1;
            }
            let mut c1 = [0u32; 3];
            let mut c2 = [0u32; 3];
            for nd in &m.nodes {
                if nd.total > 0 {
                    if nd.base == 0 {
                        c1[nd.slot as usize] += 1;
                    } else {
                        c2[nd.slot as usize] += 1;
                    }
                }
            }
            let mut d = 0u32;
            for s in 0..3 {
                d += c1[s].abs_diff(c2[s]);
            }
            fairness_sum += d as u64;
        }
        println!(
            "{},{},{:.3},{:.3},{:.4},{},{}",
            preset.name,
            n,
            ms_sum / n as f64,
            ms_max,
            first_draw as f64 / n as f64,
            repairs,
            fairness_sum / n,
        );
    }
}
