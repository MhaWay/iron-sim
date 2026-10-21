//! Golden-hash dump: recomputes the 120-tick seed-12345 stream in THIS process and prints
//! the resulting 64-bit hash. Used to re-baseline tests/determinism.rs when SIM_VERSION moves.
//! Historical golden: 0x73FF5F0F7980D4A1 (SIM_VERSION 2). Current (v3, AoE loop): see below.
//! Run: cargo run --release --example golden_dump   (cross-process check: run twice).

use game::run_sim;
use shared::{Command, TickCommands};

fn zigzag(ticks: u64) -> Vec<i16> {
    let mut v = Vec::new();
    let mut x = 1i16;
    let mut dir = 1i16;
    for _ in 0..ticks {
        v.push(x);
        x += dir;
        if x > 20 {
            dir = -1;
        }
        if x < 1 {
            dir = 1;
        }
    }
    v
}

fn main() {
    let ticks = 120u64;
    let xs = zigzag(ticks);
    let mut batches: Vec<TickCommands> = Vec::new();
    for t in 0..ticks {
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        if t % 13 == 3 {
            b.cmds.push((
                0,
                Command::Move {
                    unit: 0,
                    x: xs[t as usize],
                    y: 5,
                },
            ));
        }
        if t % 13 == 7 {
            b.cmds.push((
                1,
                Command::Move {
                    unit: 1,
                    x: 30 - xs[t as usize],
                    y: 5,
                },
            ));
        }
        if t % 29 == 9 {
            b.cmds.push((2, Command::Attack { unit: 2, target: 0 }));
        }
        batches.push(b);
    }
    let h = run_sim(12345, ticks, &batches);
    println!(
        "SIM_VERSION={} golden={:016x} ({})",
        game::SIM_VERSION,
        h,
        h
    );
}
