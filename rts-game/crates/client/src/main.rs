//! Headless client demo: cargo run -p client -- --ticks 300
//! Drives a MockRenderer + the deterministic sim loop, printing tick hashes.

use client::{MockRenderer, RenderView, Renderer};
use game::Sim;
use shared::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ticks = args
        .iter()
        .position(|a| a == "--ticks")
        .and_then(|i| args.get(i + 1)?.parse::<u64>().ok())
        .unwrap_or(30);

    let mut sim = Sim::new(1);
    let u1 = 3u32; // first Serf of player 0 spawned by Sim::new (entity id after side storages + TCs)
    let _u2 = sim.spawn_unit(1, 30, 30);
    let mut r = MockRenderer::new();
    let ev: [shared::SimEvent; 0] = [];
    for t in 0..ticks {
        let cmds = if t == 10 {
            vec![(
                0u32,
                Command::Move {
                    unit: u1,
                    x: 30,
                    y: 30,
                },
            )]
        } else {
            Vec::new()
        };
        let _ = sim.step(&cmds);
        let view = RenderView {
            tick: t,
            units: vec![],
            events: &ev,
        };
        r.render(&view);
        if t % 10 == 0 {
            println!("tick={} hash={:016x}", sim.world.tick, sim.hash());
        }
    }
    println!("done {} ticks, final hash {:016x}", ticks, sim.hash());
}
