use game::run_sim;
use shared::{Command, TickCommands};
use std::hint::black_box;
use std::time::Instant;

fn zigzag(t: u64) -> i16 {
    let mut x: i16 = 1;
    let mut dir: i16 = 1;
    for _ in 0..t {
        x += dir;
        if x > 20 {
            dir = -1;
        }
        if x < 1 {
            dir = 1;
        }
    }
    x
}

fn stream_for(ticks: u64) -> Vec<TickCommands> {
    let mut v = Vec::with_capacity(ticks as usize);
    for t in 0..ticks {
        let x = zigzag(t);
        let mut b = TickCommands {
            tick: t,
            cmds: Vec::new(),
        };
        if t % 13 == 3 {
            b.cmds.push((0, Command::Move { unit: 0, x, y: 5 }));
        }
        if t % 13 == 7 {
            b.cmds.push((
                1,
                Command::Move {
                    unit: 1,
                    x: 30 - x,
                    y: 5,
                },
            ));
        }
        if t % 29 == 9 {
            b.cmds.push((2, Command::Attack { unit: 2, target: 0 }));
        }
        b.sort();
        v.push(b);
    }
    v
}

fn main() {
    let n = 10_000u64;
    let stream = stream_for(n);
    println!("rust: warmup...");
    let _ = run_sim(12345, 200, &stream);
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t0 = Instant::now();
        let h = black_box(run_sim(12345, n, &stream));
        let dt = t0.elapsed().as_secs_f64();
        let tps = n as f64 / dt;
        best = best.min(dt);
        println!("  hash={:016x} {:.0} ticks/s ({:.1} ms)", h, tps, dt * 1e3);
    }
    println!(
        "rust best: {:.0} ticks/s ({:.1} ms per {} ticks)",
        n as f64 / best,
        best * 1e3,
        n
    );
}
