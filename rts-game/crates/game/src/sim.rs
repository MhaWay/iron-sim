//! Core simulation: units, orders, movement, combat. Systems are free functions run by
//! engine-ecs Scheduler in fixed registration order (movement, then combat).

use engine_core::{Fixed1616, Tick};
use engine_ecs::{Entity, Scheduler, World};
use engine_path::{find_path, Grid, TilePos};
use shared::{Command, Hasher};

use crate::SIM_VERSION;

pub const MAP_W: u16 = 64;
pub const MAP_H: u16 = 64;
pub const SPEED_SCALE: i32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimError {
    UnknownUnit,
    OutOfMap,
    PathUnreachable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    pub owner: u32,
    pub hp: u32,
    pub speed_q: i32,
    pub dmg: u32,
    pub range: Fixed1616,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: Fixed1616,
    pub y: Fixed1616,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    Idle,
    Move { path: Vec<TilePos>, waypoint: u32 },
    Attack { target: u32 },
}

impl Order {
    fn hash_into(&self, h: &mut Hasher) {
        match self {
            Order::Idle => h.u8(0),
            Order::Move { path, waypoint } => {
                h.u8(1);
                h.u32(*waypoint);
                h.u32(path.len() as u32);
                for t in path {
                    h.i16(t.x);
                    h.i16(t.y);
                }
            }
            Order::Attack { target } => {
                h.u8(2);
                h.u32(*target);
            }
        }
    }
}

#[inline]
fn tile_center(t: i16) -> Fixed1616 {
    Fixed1616::from_int(t as i32) + Fixed1616::from_rational(1, 2)
}

pub struct Sim {
    pub world: World,
    pub grid: Grid,
    scheduler: Scheduler,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut sim = Sim {
            world: World::new(seed),
            grid: Grid::new(MAP_W, MAP_H),
            scheduler: Scheduler::new(),
        };
        // Register systems in fixed deterministic order.
        sim.scheduler.add_system("movement", movement_system);
        sim.scheduler.add_system("combat", combat_system);
        // Initialize storages explicitly (lazy creation is fine but be explicit here).
        sim.world.storages.add_storage::<Unit>();
        sim.world.storages.add_storage::<Pos>();
        sim.world.storages.add_storage::<Order>();
        sim
    }

    pub fn spawn_unit(&mut self, owner: u32, x: i16, y: i16) -> u32 {
        let e = self.world.storages.spawn();
        self.world.storages.add(
            e,
            Unit {
                owner,
                hp: 100,
                speed_q: SPEED_SCALE / 2,
                dmg: 10,
                range: Fixed1616::ONE,
            },
        );
        self.world.storages.add(
            e,
            Pos {
                x: tile_center(x),
                y: tile_center(y),
            },
        );
        self.world.storages.add(e, Order::Idle);
        e.0
    }

    /// Sort + apply commands, then advance one tick.
    pub fn step(&mut self, cmds: &[(u32, Command)]) -> Result<(), SimError> {
        let mut sorted: Vec<(u32, Command)> = cmds.to_vec();
        sorted.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
        for (_player, c) in sorted {
            self.apply_command(c)?;
        }
        self.world.tick += 1;
        let tick = self.world.tick;
        self.scheduler.step(&mut self.world, tick);
        Ok(())
    }

    fn apply_command(&mut self, cmd: Command) -> Result<(), SimError> {
        match cmd {
            Command::Move { unit, x, y } => {
                let (sx, sy) = self.unit_tile(unit)?;
                if !self.grid.contains(TilePos { x, y }) {
                    return Err(SimError::OutOfMap);
                }
                match find_path(&self.grid, TilePos { x: sx, y: sy }, TilePos { x, y }) {
                    Some(path) => {
                        self.world
                            .storages
                            .add(Entity(unit), Order::Move { path, waypoint: 0 });
                        Ok(())
                    }
                    None => Err(SimError::PathUnreachable),
                }
            }
            Command::Attack { unit, target } => {
                let _ = self.unit_tile(unit)?;
                if self.world.storages.get::<Unit>(Entity(target)).is_none() {
                    return Err(SimError::UnknownUnit);
                }
                self.world
                    .storages
                    .add(Entity(unit), Order::Attack { target });
                Ok(())
            }
            Command::Stop { unit } => {
                let _ = self.unit_tile(unit)?;
                self.world.storages.add(Entity(unit), Order::Idle);
                Ok(())
            }
        }
    }

    fn unit_tile(&self, id: u32) -> Result<(i16, i16), SimError> {
        let p = self
            .world
            .storages
            .get::<Pos>(Entity(id))
            .ok_or(SimError::UnknownUnit)?;
        Ok((p.x.to_int_trunc() as i16, p.y.to_int_trunc() as i16))
    }

    /// Canonical state hash — iteration strictly by entity index (ECS iter).
    pub fn hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.u32(SIM_VERSION);
        h.u64(self.world.tick);
        h.u64(self.world.rng.state());
        h.u32(self.grid.w() as u32);
        h.u32(self.grid.h() as u32);
        h.u32(self.grid.blocked_count() as u32);
        for (e, u) in self.world.storages.iter::<Unit>() {
            let p = self.world.storages.get::<Pos>(e).unwrap();
            let o = self.world.storages.get::<Order>(e).unwrap();
            h.u32(e.0);
            h.u32(u.owner);
            h.u32(u.hp);
            h.u32(u.speed_q as u32);
            h.u32(u.dmg);
            h.u64(u.range.inner() as u64);
            h.u64(p.x.inner() as u64);
            h.u64(p.y.inner() as u64);
            o.hash_into(&mut h);
        }
        h.finish()
    }

    pub fn alive_units(&self) -> u32 {
        self.world.storages.iter::<Unit>().count() as u32
    }
}

/// Movement system: advance along Move path one step (speed_q) along the dominant axis.
fn movement_system(world: &mut World, _tick: Tick) {
    let ids: Vec<Entity> = world.storages.iter::<Unit>().map(|(e, _)| e).collect();
    for e in ids {
        let order = match world.storages.get::<Order>(e) {
            Some(Order::Move { .. }) => world.storages.get::<Order>(e).unwrap().clone(),
            _ => continue,
        };
        if let Order::Move { path, waypoint } = order {
            if waypoint as usize >= path.len() {
                world.storages.add(e, Order::Idle);
                continue;
            }
            let t = path[waypoint as usize];
            let cx = tile_center(t.x);
            let cy = tile_center(t.y);
            let speed_q = world.storages.get::<Unit>(e).unwrap().speed_q;
            let step = Fixed1616::from_rational(speed_q as i64, SPEED_SCALE as i64);
            let p = *world.storages.get::<Pos>(e).unwrap();
            let dx = cx - p.x;
            let dy = cy - p.y;
            let adx = dx.wrapping_abs();
            let ady = dy.wrapping_abs();
            let half = Fixed1616::from_rational(1, 2);
            if (adx + ady) <= half {
                // arrived at waypoint
                world.storages.add(e, Pos { x: cx, y: cy });
                if (waypoint + 1) as usize >= path.len() {
                    world.storages.add(e, Order::Idle);
                } else {
                    world.storages.add(
                        e,
                        Order::Move {
                            path: path.clone(),
                            waypoint: waypoint + 1,
                        },
                    );
                }
            } else if adx >= ady && !dx.is_zero() {
                world.storages.add(
                    e,
                    Pos {
                        x: p.x + if dx.inner() > 0 { step } else { -step },
                        y: p.y,
                    },
                );
            } else if !dy.is_zero() {
                world.storages.add(
                    e,
                    Pos {
                        x: p.x,
                        y: p.y + if dy.inner() > 0 { step } else { -step },
                    },
                );
            } else {
                world.storages.add(
                    e,
                    Pos {
                        x: p.x + step,
                        y: p.y,
                    },
                );
            }
        }
    }
}

/// Combat system: Attack order fires when target within range (center distance). Damage applied
/// in ascending attacker entity order; dead units removed after the damage pass.
fn combat_system(world: &mut World, _tick: Tick) {
    let ids: Vec<Entity> = world.storages.iter::<Unit>().map(|(e, _)| e).collect();
    let mut hits: Vec<(Entity, Entity, u32)> = Vec::new();
    for e in ids {
        let u = *world.storages.get::<Unit>(e).unwrap();
        let order = match world.storages.get::<Order>(e) {
            Some(o) => o.clone(),
            None => continue,
        };
        if let Order::Attack { target } = order {
            let te = Entity(target);
            if world.storages.get::<Unit>(te).is_none() {
                world.storages.add(e, Order::Idle);
                continue;
            }
            let ap = world.storages.get::<Pos>(e).unwrap();
            let tp = world.storages.get::<Pos>(te).unwrap();
            let dx = ap.x - tp.x;
            let dy = ap.y - tp.y;
            let dist2 = dx.wrapping_mul(dx) + dy.wrapping_mul(dy);
            let r2 = u.range.wrapping_mul(u.range);
            if dist2 <= r2 {
                hits.push((e, te, u.dmg));
            }
        }
    }
    for (_att, tgt, dmg) in hits {
        if let Some(u) = world.storages.get_mut::<Unit>(tgt) {
            u.hp = u.hp.saturating_sub(dmg);
        }
    }
    let dead: Vec<Entity> = world
        .storages
        .iter::<Unit>()
        .filter(|(_, u)| u.hp == 0)
        .map(|(e, _)| e)
        .collect();
    for e in dead {
        world.storages.remove::<Unit>(e);
        world.storages.remove::<Pos>(e);
        world.storages.remove::<Order>(e);
        world.storages.despawn(e);
    }
}

/// Run a full deterministic sim: apply per-tick command batches (missing batch = empty),
/// then return the final state hash.
pub fn run_sim(seed: u64, ticks: u64, cmds: &[shared::TickCommands]) -> u64 {
    let mut sim = Sim::new(seed);
    // Spawn a small default squad to make runs meaningful even without spawn commands.
    for i in 0..3u32 {
        sim.spawn_unit(0, (1 + 2 * i) as i16, 1);
    }
    for t in 0..ticks {
        let batch = cmds
            .iter()
            .find(|c| c.tick == t)
            .map(|c| c.cmds.clone())
            .unwrap_or_default();
        let _ = sim.step(&batch);
    }
    state_hash(&sim)
}

pub fn state_hash(sim: &Sim) -> u64 {
    sim.hash()
}
