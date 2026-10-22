use game::sim::Sim;
use shared::Command;

fn main() {
    let t0 = std::time::Instant::now();
    let sched = render::demo::demo_commands(12345, 324000);
    let mut sim = Sim::new(12345);
    let mut idx = 0usize;
    let mut total_cmds = 0usize;
    let ticks = 324_000u64;
    for tick in 0..ticks {
        let mut cmds: Vec<(u32, Command)> = Vec::new();
        while idx < sched.len() && sched[idx].tick == tick {
            for (p, c) in &sched[idx].cmds {
                cmds.push((*p, *c));
                total_cmds += 1;
            }
            idx += 1;
        }
        let _ = sim.step(&cmds);
        if tick % 32400 == 0 {
            println!(
                "t={} units={} p0F={} p1F={} p0W={} p1W={}",
                tick,
                sim.alive_units(),
                sim.players[0].stock.food,
                sim.players[1].stock.food,
                sim.players[0].stock.wood,
                sim.players[1].stock.wood
            );
        }
        if sim.players[0].winner != 0 {
            println!(
                "end tick={} winner={} hash={:016X}",
                sim.tick(),
                sim.players[0].winner,
                sim.hash()
            );
            break;
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "units={} cmds={} hash={:016X} ms={:.0} ticks/s={:.0}",
        sim.alive_units(),
        total_cmds,
        sim.hash(),
        secs * 1000.0,
        ticks as f64 / secs
    );
}
