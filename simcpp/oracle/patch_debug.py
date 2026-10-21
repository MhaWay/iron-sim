p = 'src/main.rs'
s = open(p).read()
i = s.index('// appended debug')
new_dbg = '''// appended debug: print state after 1 identical step
pub fn debug_tick1() {
    use sim::colony::{Job, Mode, Needs, Pawn, Pos, Site};
    let mut sim = ColonySim::new(1234);
    setup(&mut sim);
    let cmds = commands(1).cmds;
    let _ = sim.step(&cmds);
    eprintln!(\"DEBUG tick1_hash={:x}\", sim.hash());
    let ps: Vec<(u32, &Pawn)> = sim.world.storages.iter::<Pawn>().collect();
    for (e, p) in ps {
        let n = sim.world.storages.get::<Needs>(engine_ecs::Entity(e)).unwrap();
        let pos = sim.world.storages.get::<Pos>(engine_ecs::Entity(e)).unwrap();
        let m = match &p.mode {
            Mode::Idle => \"Idle\".to_string(),
            Mode::Go { path, waypoint } => format!(\"Go wp={} path=[{}]\", waypoint, path.iter().map(|t| format!(\"({},{})\", t.x, t.y)).collect::<Vec<_>>().join(\",\")),
            Mode::Eat { x, y, timer } => format!(\"Eat ({},{}) t={}\", x, y, timer),
            Mode::Sleep { x, y } => format!(\"Sleep ({},{})\", x, y),
            Mode::Work { job, x, y } => format!(\"Work j={} ({},{})\", job, x, y),
        };
        eprintln!(\"DEBUG pawn e={} owner={} spd={} mode={} n=({},{},{}) pos=({},{})\", e, p.owner, p.speed.inner(), m, n.hunger, n.energy, n.mood, pos.x.to_int_trunc(), pos.y.to_int_trunc());
    }
    let ss: Vec<(u32, &Site)> = sim.world.storages.iter::<Site>().collect();
    for (e, s) in ss {
        eprintln!(\"DEBUG site e={} kind={} tile=({},{}) occ={:?}\", e, s.kind, s.tile.x, s.tile.y, s.occupied_by);
    }
    let js: Vec<(u32, &Job)> = sim.world.storages.iter::<Job>().collect();
    for (e, j) in js {
        eprintln!(\"DEBUG job e={} kind={} site=({},{}) asgn={:?} done={}\", e, j.kind, j.site.x, j.site.y, j.assigned, j.done);
    }
}

fn debug_hook() { debug_tick1(); }
'''
s = s[:i] + new_dbg
s = s.replace('fn main() {', 'fn main() {\n    debug_hook();', 1)
if 'use engine_ecs' not in s:
    s = s.replace('use shared::{Command, TickCommands};', 'use shared::{Command, TickCommands};\nuse engine_ecs;')
open(p, 'w').write(s)
print('hooked:', 'debug_hook();' in s)