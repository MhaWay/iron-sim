//! Simple sim headless demo: cargo run -p game --bin simdemo

use game::run_sim;
use shared::{Command, TickCommands};

fn main() {
    let mut batches: Vec<TickCommands> = Vec::new();
    for t in 0..180u64 {
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        if t % 30 == 10 {
            b.cmds.push((
                0,
                Command::Move {
                    unit: 3,
                    x: 32,
                    y: 32,
                },
            ));
        }
        batches.push(b);
    }
    let h = run_sim(7, 180, &batches);
    println!("simdemo final_hash={:016x}", h);
    let h2 = run_sim(7, 180, &batches);
    assert_eq!(h, h2);
    println!("simdemo replay identical: OK");
}
