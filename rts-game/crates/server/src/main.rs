//! Headless lockstep relay demo: cargo run -p server -- --seed 7 --ticks 600
//!
//! Emits synthetic Move batches, ticks the sim, prints the final state hash and confirms an
//! identical replay produces an identical hash.

use game::run_sim;
use shared::{Command, TickCommands};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = parse_arg(&args, "--seed").unwrap_or(7);
    let ticks = parse_arg(&args, "--ticks").unwrap_or(60);

    // Batches are player-agnostic: run_sim wraps them as player 0, step canonicalizes.
    let batches: Vec<TickCommands> = (0..ticks)
        .map(|t| {
            let mut b = TickCommands {
                tick: t,
                cmds: Vec::new(),
            };
            if t % 20 == 5 {
                b.cmds.push((
                    0,
                    Command::Move {
                        unit: 3,
                        x: 20,
                        y: 20,
                    },
                ));
            }
            if t % 20 == 15 {
                b.cmds.push((
                    0,
                    Command::Move {
                        unit: 4,
                        x: 40,
                        y: 40,
                    },
                ));
            }
            b
        })
        .collect();
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
