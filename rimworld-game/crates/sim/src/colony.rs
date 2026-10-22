//! Colony model: pawns, needs (hunger/energy/mood), autonomous jobs, pathfinding,
//! campaign raids (raider hostiles) and farm/food economy.
//!
//! Determinism contract (docs/engine-api.md): systems run in fixed registration order
//! (needs -> jobs -> movement -> raid); iteration is ECS entity-index or sorted Vec; job/claim
//! keys are TOTAL orders ((dist, entity_id)) so ties never exist; Q16.16 integers only.
//!
//! SIM_VERSION 2 additions (hash-covered): Pawn.hit_points, Pawn.dying, Needs.injured,
//! ColonyEventQueue (cfg + wave counter), FoodStock, Mode::Attack { target }.

use engine_core::{Fixed1616, Tick};
use engine_ecs::{Entity, Scheduler, World};
use engine_path::{find_path, Grid, TilePos};
use shared::{Command, Hasher};

pub const SIM_VERSION: u32 = 2;
pub const MAP_W: u16 = 64;
pub const MAP_H: u16 = 64;

/// Need quantities are integer units out of NEED_MAX (65536 = full).
pub const NEED_MAX: i64 = 65536;

const HUNGER_DECAY: i64 = 16;
const ENERGY_DECAY: i64 = 10;
const MOOD_DECAY: i64 = 2;
const SLEEP_RECOVER: i64 = 120; // energy per tick while sleeping
const EAT_RESTORE: i64 = 20000; // meal restores this much hunger
const _EAT_TICKS: u8 = 8; // scaffold: eat-completion timing (unused)
const HUNGRY_THRESHOLD: i64 = 24576; // below -> seek food
const TIRED_THRESHOLD: i64 = 16384; // below -> seek bed
const STARVE_MOOD_PENALTY: i64 = 12; // extra mood decay/tick when hunger == 0
const INJURED_MOOD_PENALTY: i64 = 6; // extra mood decay/tick while injured
const BED_SLEEP_RECOVER: i64 = 360; // energy per tick sleeping in kind==1 bed
const FARM_YIELD: i64 = 20000; // food produced per farm per FARM_TICKS
const FARM_TICKS: u64 = 240;
const RAID_BASE_INTERVAL: u64 = 6000; // campaign waves every 6000 / difficulty ticks
const RAID_SIZE_BASE: i64 = 2; // wave size = 2 + wave/3 (raiders)
const RAID_HP: i64 = 100;
const RAID_ATK: i64 = 10; // hp damage per attack tick (adjacent raider)
const RAID_SPEED: i64 = 20480; // 0.3125 tiles/tick — pawns (16384) outpace them
const RAID_SPAWN_DIST: i32 = 8;
const HP_MAX: i64 = 100;
const MOOD_HEAL: i64 = 4; // mood recovery when well-fed and rested

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeedKind {
    Energy = 0,
    Hunger = 1,
}

#[derive(Clone, Copy, Debug)]
pub struct Needs {
    pub hunger: i64,
    pub energy: i64,
    pub mood: i64,
    /// True while hp < max: mood decays faster and work is dropped (starving/injured distress).
    pub injured: bool,
}

impl Needs {
    pub fn fresh() -> Self {
        Needs {
            hunger: NEED_MAX,
            energy: NEED_MAX,
            mood: NEED_MAX,
            injured: false,
        }
    }
    fn decay_all(&mut self) {
        self.hunger = (self.hunger - HUNGER_DECAY).max(0);
        self.energy = (self.energy - ENERGY_DECAY).max(0);
        let mut md = MOOD_DECAY;
        if self.hunger == 0 {
            md += STARVE_MOOD_PENALTY;
        }
        if self.injured {
            md += INJURED_MOOD_PENALTY;
        }
        let well = self.hunger > HUNGRY_THRESHOLD && self.energy > TIRED_THRESHOLD && !self.injured;
        if well {
            self.mood = (self.mood + MOOD_HEAL).min(NEED_MAX);
        } else {
            self.mood = (self.mood - md).max(0);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub x: Fixed1616,
    pub y: Fixed1616,
}

impl Pos {
    #[inline]
    pub fn tile(&self) -> TilePos {
        TilePos {
            x: self.x.to_int_trunc() as i16,
            y: self.y.to_int_trunc() as i16,
        }
    }
}

/// What a pawn is doing this tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    /// Walk a precomputed path (from Move command) or route to a claim target.
    Go {
        path: Vec<TilePos>,
        waypoint: usize,
    },
    Eat {
        site: TilePos,
        timer: u8,
    },
    Sleep {
        site: TilePos,
    },
    Work {
        job: u32,
        site: TilePos,
    },
    /// Hostile pawn engaged with a target (raider attacking colony pawns).
    Attack {
        target: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Pawn {
    pub owner: u32,
    pub speed: Fixed1616,
    pub mode: Mode,
    /// Hit points 0..=HP_MAX (hp == 0 means dead/despawned by raid_system).
    pub hit_points: i64,
    /// Hostile faction pawn (raider). Player pawns own this false.
    pub hostile: bool,
    /// True while hp < max: faster mood decay, work dropped (starve/injury distress).
    pub dying: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub kind: u8, // 0 = haul food, 1 = tidy (work completes on arrival; kind only flavor)
    pub site: TilePos,
    pub assigned: Option<u32>,
    pub done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    pub kind: u8,
    pub tile: TilePos,
    pub occupied_by: Option<u32>,
    /// Farm progress ticks since last harvest (0..FARM_TICKS; non-farm sites keep 0).
    pub progress: u64,
}

/// Colony-level configuration (view-pushed, canonical in the hash).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColonyConfig {
    /// 0 = sandbox (no raids), nonzero = campaign difficulty 1..=3 (raid cadence).
    pub difficulty: u8,
    pub campaign: bool,
}

/// Pending deterministic events: wave triggers issued by the view (or replay log).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimEvent {
    /// Fire a raid wave at this tick (sorted by tick).
    Raid { tick: u64 },
}

/// Deterministic event queue + wave bookkeeping (hash-covered).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventQueue {
    pub queue: Vec<(u64, SimEvent)>,
    pub next_wave: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoodStock {
    pub units: i64,
}

impl ColonySim {
    /// Shared food stock entity (deterministic: first entity tagged FoodStock).
    pub fn food_stock(&self) -> i64 {
        self.world
            .storages
            .iter::<FoodStock>()
            .min_by_key(|(e, _)| e.0)
            .map(|(_, f)| f.units)
            .unwrap_or(0)
    }

    pub fn set_food_stock(&mut self, units: i64) {
        let e = self
            .world
            .storages
            .iter::<FoodStock>()
            .min_by_key(|(e, _)| e.0)
            .map(|(e, _)| e)
            .unwrap_or_else(|| self.world.storages.spawn());
        self.world.storages.add(e, FoodStock { units });
    }

    /// Colony configuration (component singleton; systems read it).
    pub fn config(&self) -> ColonyConfig {
        self.world
            .storages
            .iter::<ColonyConfig>()
            .min_by_key(|(e, _)| e.0)
            .map(|(_, c)| *c)
            .unwrap_or(ColonyConfig {
                difficulty: 0,
                campaign: false,
            })
    }

    /// Event queue bookkeeping (component singleton).
    pub fn events(&self) -> EventQueue {
        self.world
            .storages
            .iter::<EventQueue>()
            .min_by_key(|(e, _)| e.0)
            .map(|(_, q)| q.clone())
            .unwrap_or_default()
    }

    fn events_mut(world: &mut World) -> (EventQueue, Option<Entity>) {
        let found = world
            .storages
            .iter::<EventQueue>()
            .min_by_key(|(e, _)| e.0)
            .map(|(e, _)| e);
        if let Some(e) = found {
            if let Some(q) = world.storages.get::<EventQueue>(e) {
                let v = q.clone();
                return (v, Some(e));
            }
        }
        (EventQueue::default(), None)
    }
}

#[derive(Debug)]
pub enum SimError {
    UnknownEntity,
    OutOfMap,
    PathUnreachable,
}

pub struct ColonySim {
    pub world: World,
    pub grid: Grid,
    scheduler: Scheduler,
}

impl ColonySim {
    pub fn new(seed: u64) -> Self {
        let mut world = World::new(seed);
        world.storages.add_storage::<Needs>();
        world.storages.add_storage::<Pos>();
        world.storages.add_storage::<Pawn>();
        world.storages.add_storage::<Job>();
        world.storages.add_storage::<Site>();
        world.storages.add_storage::<FoodStock>();
        world.storages.add_storage::<ColonyConfig>();
        world.storages.add_storage::<EventQueue>();
        // NOTE: component singletons (FoodStock, ColonyConfig, EventQueue) are created LAZILY by
        // the first write that needs them. Spawning them eagerly would occupy entity ids 0..2 and
        // shift setup-spawned pawns off ids 0..N — Command::Move{pawn:0} addresses pawns by entity
        // id, so pawns must keep the low ids. Singleton entity ids are never hashed.
        let mut scheduler = Scheduler::new();
        scheduler.add_system("needs", needs_system);
        scheduler.add_system("jobs", job_system);
        scheduler.add_system("movement", movement_system);
        scheduler.add_system("raid", raid_system);
        ColonySim {
            world,
            grid: Grid::new(MAP_W, MAP_H),
            scheduler,
        }
    }

    pub fn spawn_pawn(&mut self, owner: u32, x: i32, y: i32) -> u32 {
        self.spawn_impl(owner, x, y, 16384, false)
    }

    /// Spawn a hostile raider at (x,y) with owner faction 1 and hp RAID_HP.
    pub fn spawn_raider(&mut self, x: i32, y: i32) -> u32 {
        self.spawn_impl(1, x, y, RAID_SPEED, true)
    }

    fn spawn_impl(&mut self, owner: u32, x: i32, y: i32, speed: i64, hostile: bool) -> u32 {
        let e = self.world.storages.spawn();
        self.world.storages.add(e, Needs::fresh());
        self.world.storages.add(
            e,
            Pos {
                x: Fixed1616::from_int(x),
                y: Fixed1616::from_int(y),
            },
        );
        self.world.storages.add(
            e,
            Pawn {
                owner,
                speed: Fixed1616(speed),
                mode: Mode::Idle,
                hit_points: HP_MAX,
                hostile,
                dying: false,
            },
        );
        e.0
    }

    /// Set the colony configuration (campaign/difficulty). View-side calls are recorded by the
    /// save layer as commands; headless drivers call this directly. Deterministic either way.
    pub fn set_config(&mut self, campaign: bool, difficulty: u8) {
        let e = self
            .world
            .storages
            .iter::<ColonyConfig>()
            .min_by_key(|(e, _)| e.0)
            .map(|(e, _)| e)
            .unwrap_or_else(|| self.world.storages.spawn());
        self.world.storages.add(
            e,
            ColonyConfig {
                campaign,
                difficulty,
            },
        );
    }

    /// Schedule a raid wave at `tick` (campaign only: sandbox drops it as a no-op).
    pub fn schedule_raid(&mut self, tick: u64) {
        if !self.config().campaign {
            return;
        }
        let (mut q, e) = Self::events_mut(&mut self.world);
        q.queue.push((tick, SimEvent::Raid { tick }));
        match e {
            Some(e) => self.world.storages.add(e, q),
            None => {
                let e = self.world.storages.spawn();
                self.world.storages.add(e, q);
            }
        }
    }

    /// kind 0 = storage/food, 1 = bed, 2 = farm (food production).
    pub fn add_site(&mut self, kind: u8, x: i16, y: i16) {
        let e = self.world.storages.spawn();
        self.world.storages.add(
            e,
            Site {
                kind,
                tile: TilePos { x, y },
                occupied_by: None,
                progress: 0,
            },
        );
    }

    pub fn add_job(&mut self, kind: u8, x: i16, y: i16) {
        let e = self.world.storages.spawn();
        self.world.storages.add(
            e,
            Job {
                kind,
                site: TilePos { x, y },
                assigned: None,
                done: false,
            },
        );
    }

    /// Canonical-sort commands, apply, advance tick, run systems.
    pub fn step(&mut self, cmds: &[(u32, Command)]) -> Result<(), SimError> {
        let mut sorted: Vec<(u32, Command)> = cmds.to_vec();
        sorted.sort_by(|(pa, a), (pb, b)| pa.cmp(pb).then_with(|| a.sort_key().cmp(&b.sort_key())));
        for (_, c) in sorted {
            self.apply_command(c)?;
        }
        self.world.tick += 1;
        let tick = self.world.tick;
        self.scheduler.step(&mut self.world, tick);
        Ok(())
    }

    fn apply_command(&mut self, c: Command) -> Result<(), SimError> {
        match c {
            Command::Move { pawn, x, y } => {
                let start = self.pawn_tile(pawn)?;
                let goal = TilePos { x, y };
                if !self.grid.contains(goal) {
                    return Err(SimError::OutOfMap);
                }
                if start.x == x && start.y == y {
                    return Ok(());
                }
                let path = find_path(&self.grid, start, goal).ok_or(SimError::PathUnreachable)?;
                if path.is_empty() {
                    return Ok(());
                }
                let e = Entity(pawn);
                if let Some(p) = self.world.storages.get::<Pawn>(e) {
                    let (owner, speed, hp, hostile, dying) =
                        (p.owner, p.speed, p.hit_points, p.hostile, p.dying);
                    self.world.storages.add(
                        e,
                        Pawn {
                            owner,
                            speed,
                            mode: Mode::Go { path, waypoint: 0 },
                            hit_points: hp,
                            hostile,
                            dying,
                        },
                    );
                }
                Ok(())
            }
            Command::Wait { pawn } => {
                let e = Entity(pawn);
                if self.world.storages.get::<Pawn>(e).is_none() {
                    return Err(SimError::UnknownEntity);
                }
                if let Some(p) = self.world.storages.get::<Pawn>(e) {
                    let (owner, speed, hp, hostile, dying) =
                        (p.owner, p.speed, p.hit_points, p.hostile, p.dying);
                    self.world.storages.add(
                        e,
                        Pawn {
                            owner,
                            speed,
                            mode: Mode::Idle,
                            hit_points: hp,
                            hostile,
                            dying,
                        },
                    );
                }
                Ok(())
            }
            Command::Build { tile, y, kind } => {
                if !self.grid.contains(TilePos { x: tile, y }) {
                    return Err(SimError::OutOfMap);
                }
                self.add_site(BuildKind::normalize(kind), tile, y);
                Ok(())
            }
        }
    }

    fn pawn_tile(&self, pawn: u32) -> Result<TilePos, SimError> {
        let e = Entity(pawn);
        if self.world.storages.get::<Pawn>(e).is_none() {
            return Err(SimError::UnknownEntity);
        }
        let p = self
            .world
            .storages
            .get::<Pos>(e)
            .ok_or(SimError::UnknownEntity)?;
        Ok(p.tile())
    }

    /// Canonical state hash (FNV-1a, little-endian, entity-index iteration).
    pub fn hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.write_u32(SIM_VERSION);
        h.write_u64(self.world.tick);
        h.write_u64(self.world.rng.state());
        h.write_u32(self.grid.w() as u32);
        h.write_u32(self.grid.h() as u32);
        h.write_u64(self.grid.blocked_count() as u64);
        let pawns: Vec<(u32, Pawn, Needs, Pos)> = self
            .world
            .storages
            .iter::<Pawn>()
            .filter_map(|(e, p)| {
                let n = *self.world.storages.get::<Needs>(e)?;
                let pos = *self.world.storages.get::<Pos>(e)?;
                Some((e.0, p.clone(), n, pos))
            })
            .collect();
        h.write_u32(pawns.len() as u32);
        for (id, p, n, pos) in pawns {
            h.write_u32(id);
            h.write_u32(p.owner);
            h.write_u64(p.speed.inner() as u64);
            mode_canon(&mut h, &p.mode);
            h.write_i64(n.hunger);
            h.write_i64(n.energy);
            h.write_i64(n.mood);
            h.write_u64(pos.x.inner() as u64);
            h.write_u64(pos.y.inner() as u64);
            h.write_i64(p.hit_points);
            h.write_u8(p.hostile as u8);
            h.write_u8(p.dying as u8);
            h.write_u8(n.injured as u8);
        }
        let sites: Vec<(u32, Site)> = self
            .world
            .storages
            .iter::<Site>()
            .map(|(e, s)| (e.0, *s))
            .collect();
        h.write_u32(sites.len() as u32);
        for (_, s) in sites {
            h.write_u8(s.kind);
            h.write_i32(s.tile.x as i32);
            h.write_i32(s.tile.y as i32);
            h.write_u32(s.occupied_by.map_or(0, |v| v + 1));
            h.write_u64(s.progress);
        }
        let jobs: Vec<(u32, Job)> = self
            .world
            .storages
            .iter::<Job>()
            .map(|(e, j)| (e.0, *j))
            .collect();
        h.write_u32(jobs.len() as u32);
        for (_, j) in jobs {
            h.write_u8(j.kind);
            h.write_i32(j.site.x as i32);
            h.write_i32(j.site.y as i32);
            h.write_u32(j.assigned.map_or(0, |v| v + 1));
            h.write_u8(j.done as u8);
        }
        // SIM_VERSION 2 state: event queue, wave counter, food stock. Colony config (campaign /
        // difficulty) is NOT hashed — it is replay-critical INPUT stored in the save, like the
        // seed; hash covers mutable world state only (raiders land in the pawn list).
        let ev = self.events();
        // Events are stored as (tick, tick) and drained exactly at trigger in raid_system;
        // the hash reads them as-is (no mutation) so repeated hash calls are side-effect free.
        let mut q = ev.queue.clone();
        q.sort_by_key(|e| *e);
        h.write_u32(q.len() as u32);
        for (t, _) in q {
            h.write_u64(t);
        }
        h.write_u64(ev.next_wave);
        h.write_i64(self.food_stock());
        h.finish()
    }
}

/// Build-kind wire values accepted by Command::Build: 0=storage/shelf, 1=bed, 2=farm.
/// Wire values outside the table normalize by (kind % 3) to keep the small enum total.
pub struct BuildKind;
impl BuildKind {
    pub const STORAGE: u8 = 0;
    pub const BED: u8 = 1;
    pub const FARM: u8 = 2;
    pub fn normalize(kind: u8) -> u8 {
        kind % 3
    }
}

fn mode_canon(h: &mut Hasher, m: &Mode) {
    match m {
        Mode::Idle => h.write_u8(0),
        Mode::Go { path, waypoint } => {
            h.write_u8(1);
            h.write_u32(*waypoint as u32);
            h.write_u32(path.len() as u32);
            for t in path {
                h.write_i32(t.x as i32);
                h.write_i32(t.y as i32);
            }
        }
        Mode::Eat { site, timer } => {
            h.write_u8(2);
            h.write_i32(site.x as i32);
            h.write_i32(site.y as i32);
            h.write_u8(*timer);
        }
        Mode::Sleep { site } => {
            h.write_u8(3);
            h.write_i32(site.x as i32);
            h.write_i32(site.y as i32);
        }
        Mode::Work { job, site } => {
            h.write_u8(4);
            h.write_u32(*job);
            h.write_i32(site.x as i32);
            h.write_i32(site.y as i32);
        }
        Mode::Attack { target } => {
            h.write_u8(5);
            h.write_u32(*target);
        }
    }
}

#[inline]
fn manhattan(a: TilePos, bx: i32, by: i32) -> i64 {
    (a.x as i64 - bx as i64).abs() + (a.y as i64 - by as i64).abs()
}

#[inline]
fn pick_need(hunger: i64, energy: i64) -> Option<NeedKind> {
    let hungry = hunger < HUNGRY_THRESHOLD;
    let tired = energy < TIRED_THRESHOLD;
    if hungry {
        Some(NeedKind::Hunger)
    }
    // food beats rest
    else if tired {
        Some(NeedKind::Energy)
    } else {
        None
    }
}

/// SYSTEM 1: decay needs; complete Sleep/Eat/Work arrivals.
fn needs_system(world: &mut World, _tick: Tick) {
    // phase 1: pure reads
    struct Upd {
        e: Entity,
        n: Needs,
        mode: Mode,
        release: Option<u8>,
        hp: i64,
        hostile: bool,
        ate: bool,
    }
    let mut ups: Vec<Upd> = Vec::new();
    // Farm harvest: each farm site accumulates progress; FARM_TICKS -> +FARM_YIELD food.
    {
        let mut food_delta: i64 = 0;
        let farm_updates: Vec<(Entity, u64)> = world
            .storages
            .iter::<Site>()
            .filter(|(_, s)| s.kind == BuildKind::FARM)
            .map(|(e, s)| {
                let p = s.progress + 1;
                if p >= FARM_TICKS {
                    food_delta += FARM_YIELD;
                    (e, 0)
                } else {
                    (e, p)
                }
            })
            .collect();
        for (e, p) in farm_updates {
            if let Some(s) = world.storages.get::<Site>(e) {
                let v = *s;
                world.storages.add(e, Site { progress: p, ..v });
            }
        }
        if food_delta > 0 {
            // single food stock: lowest-id FoodStock entity, lazily created (deterministic scan).
            let e = world
                .storages
                .iter::<FoodStock>()
                .min_by_key(|(e, _)| e.0)
                .map(|(e, _)| e)
                .unwrap_or_else(|| world.storages.spawn());
            let cur = world
                .storages
                .get::<FoodStock>(e)
                .map(|f| f.units)
                .unwrap_or(0);
            world.storages.add(
                e,
                FoodStock {
                    units: cur + food_delta,
                },
            );
        }
    }
    for (e, p) in world.storages.iter::<Pawn>() {
        let Some(n0) = world.storages.get::<Needs>(e) else {
            continue;
        };
        let mut n = *n0;
        let mut mode = p.mode.clone();
        let mut release: Option<u8> = None;
        let mut ate = false;
        let hp = p.hit_points;
        // injured flag mirrors hp < max (kept in Needs for cheap UI/hash view).
        n.injured = p.hostile || hp < HP_MAX;
        match &p.mode {
            Mode::Sleep { site } => {
                // Bed (kind==1) recovers much faster than sleeping on the floor.
                let on_bed = world
                    .storages
                    .iter::<Site>()
                    .any(|(_, s)| s.kind == 1 && s.occupied_by == Some(e.0) && s.tile == *site);
                let rate = if on_bed {
                    BED_SLEEP_RECOVER
                } else {
                    SLEEP_RECOVER
                };
                n.energy = (n.energy + rate).min(NEED_MAX);
                if n.energy >= NEED_MAX {
                    mode = Mode::Idle;
                    release = Some(1);
                } else {
                    mode = Mode::Sleep { site: *site };
                }
            }
            Mode::Eat { site, timer } => {
                let timer = *timer;
                if timer <= 1 {
                    n.hunger = (n.hunger + EAT_RESTORE).min(NEED_MAX);
                    mode = Mode::Idle;
                    release = Some(0);
                    ate = true;
                } else {
                    mode = Mode::Eat {
                        site: *site,
                        timer: timer - 1,
                    };
                }
            }
            Mode::Work { job, site } => {
                let job = *job;
                let site = *site;
                let done = world
                    .storages
                    .get::<Job>(Entity(job))
                    .map(|j| j.done)
                    .unwrap_or(true);
                // Distress: starving or injured pawns DROP WORK (need system releases the job).
                if done || p.hit_points < HP_MAX || n.hunger == 0 {
                    mode = Mode::Idle;
                } else {
                    mode = Mode::Work { job, site };
                }
            }
            _ => n.decay_all(),
        }
        ups.push(Upd {
            e,
            n,
            mode,
            release,
            hp: p.hit_points,
            hostile: p.hostile,
            ate,
        });
    }
    // phase 2: mutations. Clone the pawn table first so storages mutations never
    // overlap the immutable borrow held by get::<Pawn>.
    let snapshot: Vec<(Entity, Pawn)> = world
        .storages
        .iter::<Pawn>()
        .map(|(e, p)| (e, p.clone()))
        .collect();
    let snap_map: std::collections::BTreeMap<Entity, Pawn> = snapshot.into_iter().collect();
    for u in ups {
        world.storages.add(u.e, u.n);
        if let Some(p) = snap_map.get(&u.e) {
            let (owner, speed, pmode) = (p.owner, p.speed, p.mode.clone());
            world.storages.add(
                u.e,
                Pawn {
                    owner,
                    speed,
                    mode: u.mode.clone(),
                    hit_points: u.hp,
                    hostile: u.hostile,
                    dying: u.n.injured,
                },
            );
            // Auto-eat consumed one unit of stock when the Eat completed (inside needs system).
            if u.ate {
                let target = world
                    .storages
                    .iter::<FoodStock>()
                    .min_by_key(|(e, _)| e.0)
                    .map(|(e, _)| e);
                if let Some(fe) = target {
                    if let Some(f) = world.storages.get::<FoodStock>(fe) {
                        let v = *f;
                        world.storages.add(
                            fe,
                            FoodStock {
                                units: (v.units - 1).max(0),
                            },
                        );
                    }
                }
            }
            // Work dropped by distress: unassign the job so another pawn may take it.
            if let Mode::Work { job, .. } = &pmode {
                if matches!(u.mode, Mode::Idle) && (u.n.injured || u.n.hunger == 0) {
                    if let Some(j) = world.storages.get::<Job>(Entity(*job)) {
                        let v = *j;
                        if v.assigned == Some(u.e.0) {
                            world.storages.add(
                                Entity(*job),
                                Job {
                                    assigned: None,
                                    ..v
                                },
                            );
                        }
                    }
                }
            }
        }
        if let Some(kind) = u.release {
            let mut hit: Option<(Entity, Site)> = None;
            for (se, s) in world.storages.iter::<Site>() {
                if s.kind == kind && s.occupied_by == Some(u.e.0) {
                    hit = Some((se, *s));
                    break;
                }
            }
            if let Some((se, s)) = hit {
                world.storages.add(
                    se,
                    Site {
                        occupied_by: None,
                        ..s
                    },
                );
            }
        }
    }
}

/// SYSTEM 2: need satisfaction (route to free site) + job assignment for idle pawns.
fn job_system(world: &mut World, _tick: Tick) {
    // phase 1: snapshot (immutable reads only)
    let pawns: Vec<(u32, i32, i32, Mode, Needs)> = world
        .storages
        .iter::<Pawn>()
        .filter_map(|(e, p)| {
            let pos = world.storages.get::<Pos>(e)?;
            let n = *world.storages.get::<Needs>(e)?;
            Some((
                e.0,
                pos.x.to_int_trunc(),
                pos.y.to_int_trunc(),
                p.mode.clone(),
                n,
            ))
        })
        .collect();
    let sites: Vec<(u32, Site)> = world
        .storages
        .iter::<Site>()
        .map(|(e, s)| (e.0, *s))
        .collect();
    let jobs: Vec<(u32, Job)> = world
        .storages
        .iter::<Job>()
        .map(|(e, j)| (e.0, *j))
        .collect();
    let food_units: i64 = world
        .storages
        .iter::<FoodStock>()
        .min_by_key(|(e, _)| e.0)
        .map(|(_, f)| f.units)
        .unwrap_or(0);

    // Phase A: hungry/tired pawns claim nearest FREE site (total key: (dist, id)).
    // A pawn that is starving (hunger == 0) or injured can still claim: eating/sleeping is
    // the cure for starvation, and idle-on-distress without food is worse. Hostiles never claim.
    let mut claimed_sites: Vec<u32> = Vec::new();
    let mut claims: Vec<(u32, u32, u8)> = Vec::new(); // (pawn, site, kind)
    let hostile: Vec<u32> = world
        .storages
        .iter::<Pawn>()
        .filter(|(_, p)| p.hostile)
        .map(|(e, _)| e.0)
        .collect();
    for (pid, px, py, mode, n) in &pawns {
        if *mode != Mode::Idle || hostile.contains(pid) {
            continue;
        }
        let Some(need) = pick_need(n.hunger, n.energy) else {
            continue;
        };
        let kind: u8 = match need {
            NeedKind::Hunger => 0,
            NeedKind::Energy => 1,
        };
        // Auto-eat gate: food only if the colony stock has food (EAT consumes 1 unit on arrival).
        if kind == 0 && food_units <= 0 {
            continue;
        }
        let mut best: Option<(i64, u32)> = None;
        for (sid, s) in &sites {
            if s.kind != kind || s.occupied_by.is_some() {
                continue;
            }
            // A shelf (kind 0) is any storage site; count only if it is the stock itself or a shelf.
            if claimed_sites.contains(sid) {
                continue;
            }
            let d = manhattan(s.tile, *px, *py);
            if let Some((bd, _)) = best {
                if d < bd {
                    best = Some((d, *sid));
                }
            } else {
                best = Some((d, *sid));
            }
        }
        if let Some((_, sid)) = best {
            claims.push((*pid, sid, kind));
            claimed_sites.push(sid);
        }
    }

    // Phase B: idle pawns without a need claim the closest open job. Key: (manhattan, job_id).
    let need_pawns: Vec<u32> = claims.iter().map(|(p, _, _)| *p).collect();
    let mut open_jobs: Vec<(u32, TilePos, bool)> = jobs
        .iter()
        .map(|(jid, j)| (*jid, j.site, !j.done && j.assigned.is_none()))
        .collect();
    let mut work: Vec<(u32, u32, TilePos)> = Vec::new(); // (pawn, job, site)
    for (pid, px, py, mode, _) in &pawns {
        if *mode != Mode::Idle || need_pawns.contains(pid) || hostile.contains(pid) {
            continue;
        }
        let mut best: Option<(i64, u32, usize, TilePos)> = None;
        for (i, (jid, t, avail)) in open_jobs.iter().enumerate() {
            if !*avail {
                continue;
            }
            let d = manhattan(*t, *px, *py);
            if let Some((bd, _, _, _)) = best {
                if d < bd {
                    best = Some((d, *jid, i, *t));
                }
            } else {
                best = Some((d, *jid, i, *t));
            }
        }
        if let Some((_, jid, i, t)) = best {
            work.push((*pid, jid, t));
            open_jobs[i] = (jid, t, false);
        }
    }

    // phase 2: mutations (all reads done)
    for (pid, sid, kind) in &claims {
        if let Some((_, s)) = sites.iter().find(|(id, _)| *id == *sid) {
            let s = *s;
            world.storages.add(
                Entity(*sid),
                Site {
                    occupied_by: Some(*pid),
                    ..s
                },
            );
        }
        let target = if let Some((_, s)) = sites.iter().find(|(id, _)| *id == *sid) {
            let s = *s;
            s.tile
        } else {
            continue;
        };
        let _ = kind;
        if let Some(p) = world.storages.get::<Pawn>(Entity(*pid)) {
            let (owner, speed, hp, hostile, dying) =
                (p.owner, p.speed, p.hit_points, p.hostile, p.dying);
            world.storages.add(
                Entity(*pid),
                Pawn {
                    owner,
                    speed,
                    mode: Mode::Go {
                        path: vec![target],
                        waypoint: 0,
                    },
                    hit_points: hp,
                    hostile,
                    dying,
                },
            );
        }
    }
    for (pid, jid, t) in &work {
        if let Some(j) = world.storages.get::<Job>(Entity(*jid)) {
            let v = *j;
            world.storages.add(
                Entity(*jid),
                Job {
                    assigned: Some(*pid),
                    ..v
                },
            );
        }
        if let Some(p) = world.storages.get::<Pawn>(Entity(*pid)) {
            let (owner, speed, hp, hostile, dying) =
                (p.owner, p.speed, p.hit_points, p.hostile, p.dying);
            world.storages.add(
                Entity(*pid),
                Pawn {
                    owner,
                    speed,
                    mode: Mode::Go {
                        path: vec![*t],
                        waypoint: 0,
                    },
                    hit_points: hp,
                    hostile,
                    dying,
                },
            );
        }
    }
}

/// SYSTEM 3: dominant-axis movement along the current path; arrival transitions.
fn movement_system(world: &mut World, _tick: Tick) {
    let mut updates: Vec<(Entity, Pawn, Pos)> = Vec::new();
    for (e, p) in world.storages.iter::<Pawn>() {
        if let Mode::Go { path, waypoint } = &p.mode {
            let Some(pos) = world.storages.get::<Pos>(e) else {
                continue;
            };
            let wi = *waypoint;
            let wi = wi.min(path.len().saturating_sub(1));
            let target = path[wi];
            let half = Fixed1616(32768);
            let cx = Fixed1616::from_int(target.x as i32).wrapping_add(half);
            let cy = Fixed1616::from_int(target.y as i32).wrapping_add(half);
            let dx = cx.wrapping_sub(pos.x);
            let dy = cy.wrapping_sub(pos.y);
            let mut nx = pos.x;
            let mut ny = pos.y;
            if dx.wrapping_abs().inner() <= half.inner()
                && dy.wrapping_abs().inner() <= half.inner()
            {
                nx = cx;
                ny = cy;
            } else if dx.wrapping_abs().inner() >= dy.wrapping_abs().inner() {
                nx = nx.wrapping_add(if dx.inner() > 0 {
                    p.speed
                } else {
                    Fixed1616(0) - p.speed
                });
            } else {
                ny = ny.wrapping_add(if dy.inner() > 0 {
                    p.speed
                } else {
                    Fixed1616(0) - p.speed
                });
            }
            let arrived = nx == cx && ny == cy;
            let mut mode = p.mode.clone();
            if arrived {
                if wi + 1 < path.len() {
                    mode = Mode::Go {
                        path: (*path).clone(),
                        waypoint: wi + 1,
                    };
                } else {
                    // Claimed-site arrival: pawn at an occupied site starts eating or sleeping.
                    mode = Mode::Idle;
                    let (tx, ty) = (nx.to_int_trunc(), ny.to_int_trunc());
                    for (_se, s) in world.storages.iter::<Site>() {
                        if s.occupied_by == Some(e.0)
                            && (s.tile.x as i32 - tx).abs() + (s.tile.y as i32 - ty).abs() <= 1
                        {
                            mode = match s.kind {
                                0 => Mode::Eat {
                                    site: s.tile,
                                    timer: 8,
                                },
                                1 => Mode::Sleep { site: s.tile },
                                _ => Mode::Idle,
                            };
                            break;
                        }
                    }
                }
            }
            updates.push((
                e,
                Pawn {
                    owner: p.owner,
                    speed: p.speed,
                    mode,
                    hit_points: p.hit_points,
                    hostile: p.hostile,
                    dying: p.dying,
                },
                Pos { x: nx, y: ny },
            ));
        }
    }
    for (e, p, pos) in updates {
        world.storages.add(e, p);
        world.storages.add(e, pos);
    }
}

/// SYSTEM 4: campaign raids (sandbox = complete no-op) + raid combat.
/// Cadence: while campaign and difficulty > 0, waves fire every RAID_BASE_INTERVAL / diff ticks
/// (diff 1 -> 6000, 2 -> 3000, 3 -> 2000). Wave size = RAID_SIZE_BASE + next_wave/3.
/// Spawn edge slots derive from world.rng draws (2 draws per raider). Combat (target selection +
/// adjacency damage) is deterministic without RNG. Raid state (hostile pawns, queue, wave counter,
/// rng stream) is fully hash-covered.
fn raid_system(world: &mut World, tick: Tick) {
    let cfg = world
        .storages
        .iter::<ColonyConfig>()
        .min_by_key(|(e, _)| e.0)
        .map(|(_, c)| *c)
        .unwrap_or(ColonyConfig {
            difficulty: 0,
            campaign: false,
        });
    if !cfg.campaign || cfg.difficulty == 0 {
        return; // sandbox: zero raid logic, zero rng consumption -> zero raid alerts
    }
    let interval = (RAID_BASE_INTERVAL / cfg.difficulty.max(1) as u64).max(1);

    let (ev_e, mut queue) = match world.storages.iter::<EventQueue>().min_by_key(|(e, _)| e.0) {
        Some((e, q)) => (e, q.clone()),
        None => {
            let e = world.storages.spawn();
            (e, EventQueue::default())
        }
    };
    if tick > 0 && tick.is_multiple_of(interval) {
        queue.queue.push((tick, SimEvent::Raid { tick }));
    }
    queue.queue.sort_by_key(|e| *e);
    // Drain every event due at or before this tick (no double-fires).
    let mut fired = 0usize;
    while queue.queue.first().is_some_and(|(t, _)| *t <= tick) {
        queue.queue.remove(0);
        fired += 1;
    }
    for _ in 0..fired {
        queue.next_wave += 1;
        let size = (RAID_SIZE_BASE + (queue.next_wave / 3) as i64).max(1);
        // Edge spawn slots from rng draws (2 per raider), deterministic sequence.
        let mut slots: Vec<(i32, i32)> = Vec::new();
        for _ in 0..size {
            let r1 = world.rng.next_u64();
            let r2 = world.rng.next_u64();
            let edge = (r1 % 4) as i32; // 0=N 1=E 2=S 3=W
            let lo = RAID_SPAWN_DIST as u64;
            let hi = (MAP_W as u64).saturating_sub(2 * lo).max(1);
            let along = (r2 % hi + lo) as i32;
            let (x, y) = match edge {
                0 => (along, 1),
                1 => (MAP_W as i32 - 2, along),
                2 => (along, MAP_H as i32 - 2),
                _ => (1, along),
            };
            slots.push((x, y));
        }
        for (x, y) in slots {
            let e = world.storages.spawn();
            world.storages.add(e, Needs::fresh());
            world.storages.add(
                e,
                Pos {
                    x: Fixed1616::from_int(x),
                    y: Fixed1616::from_int(y),
                },
            );
            world.storages.add(
                e,
                Pawn {
                    owner: 1,
                    speed: Fixed1616(RAID_SPEED),
                    mode: Mode::Idle,
                    hit_points: RAID_HP,
                    hostile: true,
                    dying: false,
                },
            );
        }
    }

    // Target acquisition + damage: victims = non-hostile pawns (sorted entity order).
    let victims: Vec<(u32, i32, i32, i64)> = world
        .storages
        .iter::<Pawn>()
        .filter(|(_, p)| !p.hostile)
        .filter_map(|(e, p)| {
            let pos = world.storages.get::<Pos>(e)?;
            Some((
                e.0,
                pos.x.to_int_trunc(),
                pos.y.to_int_trunc(),
                p.hit_points,
            ))
        })
        .collect();

    // First pass: raiders with no target pick nearest victim (manhattan, tie entity id);
    // raiders adjacent to a victim attack it.
    let mut updates: Vec<(Entity, Pawn, Mode)> = Vec::new();
    let mut dmg: Vec<(u32, i64)> = Vec::new();
    let raiders: Vec<(u32, Mode, i32, i32, i64)> = world
        .storages
        .iter::<Pawn>()
        .filter(|(_, p)| p.hostile)
        .filter_map(|(e, p)| {
            let pos = world.storages.get::<Pos>(e)?;
            Some((
                e.0,
                p.mode.clone(),
                pos.x.to_int_trunc(),
                pos.y.to_int_trunc(),
                p.hit_points,
            ))
        })
        .collect();
    for (rid, mode, ax, ay, _hp) in raiders {
        let e = Entity(rid);
        let (owner, speed, hp, dying) = match world.storages.get::<Pawn>(e) {
            Some(p) => (p.owner, p.speed, p.hit_points, p.dying),
            None => continue,
        };
        // Choose victim: the current target if still alive, else nearest.
        let mut target_id: Option<u32> = match &mode {
            Mode::Attack { target } => Some(*target),
            _ => None,
        };
        if let Some(t) = target_id {
            if !victims.iter().any(|(vid, _, _, _)| *vid == t) {
                target_id = None;
            }
        }
        if target_id.is_none() {
            target_id = victims
                .iter()
                .map(|(vid, vx, vy, _)| (*vid, (ax - vx).abs() + (ay - vy).abs()))
                .min_by_key(|(vid, d)| (*d, *vid))
                .map(|(vid, _)| vid);
        }
        let mode_new = match target_id {
            None => Mode::Idle,
            Some(t) => {
                let vx = victims
                    .iter()
                    .find(|(vid, ..)| *vid == t)
                    .map(|(_, vx, vy, _)| (*vx, *vy));
                if let Some((vx, vy)) = vx {
                    if (ax - vx).abs() + (ay - vy).abs() <= 1 {
                        dmg.push((t, RAID_ATK));
                        Mode::Attack { target: t }
                    } else {
                        // Step one tile toward target on the dominant axis.
                        let dx = vx - ax;
                        let dy = vy - ay;
                        let (sx, sy) = if dx.abs() >= dy.abs() {
                            (ax + dx.signum(), ay)
                        } else {
                            (ax, ay + dy.signum())
                        };
                        Mode::Go {
                            path: vec![TilePos {
                                x: sx as i16,
                                y: sy as i16,
                            }],
                            waypoint: 0,
                        }
                    }
                } else {
                    Mode::Idle
                }
            }
        };
        updates.push((
            e,
            Pawn {
                owner,
                speed,
                mode: mode_new,
                hit_points: hp,
                hostile: true,
                dying,
            },
            Mode::Idle,
        ));
    }
    // Aggregate damage per victim.
    let mut agg: Vec<(u32, i64)> = Vec::new();
    for (t, d) in dmg {
        if let Some(p) = agg.iter_mut().find(|(x, _)| *x == t) {
            p.1 += d;
        } else {
            agg.push((t, d));
        }
    }
    for (t, d) in agg {
        let e = Entity(t);
        if let Some(p) = world.storages.get::<Pawn>(e) {
            let (o, s, m, h, ho) = (p.owner, p.speed, p.mode.clone(), p.hit_points, p.hostile);
            let newhp = (h - d).max(0);
            world.storages.add(
                e,
                Pawn {
                    owner: o,
                    speed: s,
                    mode: m,
                    hit_points: newhp,
                    hostile: ho,
                    dying: newhp < HP_MAX,
                },
            );
            if let Some(n) = world.storages.get::<Needs>(e) {
                let mut nv = *n;
                nv.injured = newhp < HP_MAX || nv.hunger == 0;
                world.storages.add(e, nv);
            }
        }
    }
    // Apply raider mode updates (only Go/Attack/Idle transitions; skip dead raiders).
    for (e, p, _) in updates {
        if p.hit_points > 0 {
            world.storages.add(e, p);
        } else {
            // dead raider: despawn
            world.storages.despawn(e);
        }
    }
    world.storages.add(ev_e, queue);
}

/// Replay entry: setup callback runs first, then per-tick command batches, then idle ticks.
pub fn run_colony<F: FnOnce(&mut ColonySim)>(
    seed: u64,
    ticks: u64,
    batches: &[shared::TickCommands],
    setup: F,
) -> u64 {
    let mut sim = ColonySim::new(seed);
    setup(&mut sim);
    for (i, b) in batches.iter().enumerate() {
        if i as u64 >= ticks {
            break;
        }
        if b.tick != i as u64 {
            continue;
        }
        let _ = sim.step(&b.cmds);
    }
    while sim.world.tick < ticks {
        let _ = sim.step(&[]);
    }
    sim.hash()
}

pub fn state_hash(sim: &ColonySim) -> u64 {
    sim.hash()
}
