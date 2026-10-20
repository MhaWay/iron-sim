//! Headless lockstep relay demo: cargo run -p server -- --seed 7 --ticks 600
//!
//! Spawns 3 units, emits synthetic Move batches, ticks the sim, prints final state hash and
//! confirms an identical replay produces an identical hash.

use game::run_sim;
use shared::TickCommands;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = parse_arg(&args, "--seed").unwrap_or(7);
    let ticks = parse_arg(&args, "--ticks").unwrap_or(60);

    let mut batches = Vec::new();
    for t in 0..ticks {
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        if t % 20 == 5 {
            b.cmds.push((
                0,
                shared::Command::Move {
                    unit: 0,
                    x: 10,
                    y: 10,
                },
            ));
        }
        if t % 20 == 15 {
            b.cmds.push((
                1,
                shared::Command::Move {
                    unit: 1,
                    x: 40,
                    y: 10,
                },
            ));
        }
        b.sort();
        batches.push(b);
    }
    let h1 = run_sim(seed, ticks, &batches);
    let h2 = run_sim(seed, ticks, &batches);
    println!("seed={} ticks={} final_hash={:016x}", seed, ticks, h1);
    assert_eq!(h1, h2, "replay mismatch");
    println!("replay identical: OK");
}

fn parse_arg(args: &[String], flag: &str) -> Option<u64> {
    let i = args.iter().position(|a| a == flag)? + 1;
    args.get(i)?.parse().ok()
}
