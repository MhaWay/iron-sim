//! Core deterministic RTS sim: units, orders, movement, combat + Phase-1 economy (seeded map,
//! resource nodes, buildings, training, harvest loops, fog, victory). Systems run in fixed
/// registration order via the engine-ecs Scheduler; the whole sim is single-threaded, no f32/f64,
// no default-hash HashMap, deterministic iteration by ECS entity index / Vec order.
// SIM_VERSION is owned by lib.rs; the golden hash test (tests/determinism.rs) pins the exact
// state hash this file produces.
use engine_core::{Fixed1616, Tick};
use engine_ecs::{Entity, Scheduler, World};
use engine_path::{find_path, Grid, TilePos};
use shared::{Command, Hasher, TickCommands, WriteHash};

use crate::economy::{
    building_spec, harvest_score_inc, pop_cap_for, train_gate, unit_spec, Building, BuildingStatus,
    HarvestState, NodePos, PlayerState, RallyPoint, ResourceNode, TrainItem, TrainQueue, UnitRally,
    Visibility, WorkJob, WorkMode, AGE_COST, AGE_UP_TICKS, CARRY, MAP_H, MAP_W, MATCH_TICKS,
    PICK_TICKS, REGEN_AMOUNT, REGEN_INTERVAL, SIGHT_BASE, STARVE_SLOW, UNIT_KIND_COUNT, UPKEEP,
    UPKEEP_GRACE, UPKEEP_INTERVAL,
};
use crate::SIM_VERSION;

pub const SPEED_SCALE: i64 = 1024;
/// Deposits slot used for training score credits (255 = impossible resource slot, collision-free).
const TRAIN_SCORE_MARKER: u8 = 255;
/// Deposits slot used for building-completion score credits.
const BUILD_SCORE_MARKER: u8 = 254;
/// Deposits owner used for wrecked-building footprint reopen (owner sentinel).
const REOPEN_OWNER: u8 = 255;
/// Deposits slot used for wrecked-building footprint reopen.
const REOPEN_MARKER: u8 = 252;
pub const MAX_PLAYERS: usize = 2;
/// Wonder must be held for this many ticks to win (15 min at 30 Hz).
pub const VICTORY_WONDER_HOLD: u64 = 30 * 30 * 60;
/// Age-gated training is enforced via economy::train_gate (building kind + minimum age); the
/// old civic-point threshold table is superseded by the v2 cost/timer table (economy::AGE_COST).
/// Wall damage applied per hit to buildings (combat_system).
pub const WALL_BUILDING_DMG: i32 = 15;
/// Building melee reach (1 tile, fixed-point) used when units attack structures.
const BUILDING_ATTACK_RANGE: i64 = 98_304;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimError {
    UnknownUnit,
    OutOfMap,
    PathUnreachable,
    Occupied,
    Unaffordable,
    BadCommand,
}

#[derive(Clone, Copy, Debug)]
pub struct Unit {
    pub owner: u8,
    pub kind: u16,
    pub hp: i32,
    pub speed_q: u32,
    pub dmg: i32,
    pub range: Fixed1616,
}

#[derive(Clone, Copy, Debug)]
pub struct Pos {
    pub x: Fixed1616,
    pub y: Fixed1616,
}

#[derive(Clone, Debug)]
pub enum Order {
    Idle,
    Move {
        path: Vec<TilePos>,
        waypoint: usize,
    },
    Attack {
        target: u32,
    },
    /// v2: attack-and-chase (melee) — movement walks the unit toward the target while idle.
    UnitAttack {
        target: u32,
    },
    Harvest {
        node: u32,
        path: Vec<TilePos>,
    },
    Construct {
        building: u32,
        path: Vec<TilePos>,
    },
}

#[inline]
fn tile_center(x: i16, y: i16) -> (Fixed1616, Fixed1616) {
    (
        Fixed1616(((x as i64) << 16) + (1 << 15)),
        Fixed1616(((y as i64) << 16) + (1 << 15)),
    )
}

#[inline]
fn manhattan(dx: i32, dy: i32) -> i32 {
    dx.abs() + dy.abs()
}

#[inline]
fn tile_of(f: Fixed1616) -> i16 {
    f.to_int_trunc() as i16
}

fn hash_path(path: &[TilePos], h: &mut Hasher) {
    h.write_u64(path.len() as u64);
    for p in path {
        h.write_i32(p.x as i32);
        h.write_i32(p.y as i32);
    }
}

impl Order {
    fn hash_into(&self, h: &mut Hasher) {
        match self {
            Order::Idle => h.write_u8(0),
            Order::Move { path, waypoint } => {
                h.write_u8(1);
                h.write_u64(*waypoint as u64);
                hash_path(path, h);
            }
            Order::Attack { target } => {
                h.write_u8(2);
                h.write_u32(*target);
            }
            Order::UnitAttack { target } => {
                h.write_u8(5);
                h.write_u32(*target);
            }
            Order::Harvest { node, path } => {
                h.write_u8(3);
                h.write_u32(*node);
                hash_path(path, h);
            }
            Order::Construct { building, path } => {
                h.write_u8(4);
                h.write_u32(*building);
                hash_path(path, h);
            }
        }
    }
}

/// Harvest deposits queued during a system pass; drained by Sim::step into the stockpiles.
#[derive(Default)]
pub struct Deposits(pub Vec<(u8, u8, i32)>);

/// Grid mirror for system-side pathfinding (systems only receive &mut World). Refreshed by
/// Sim::sync_mirrors at step start; the sim itself never reads it.
#[derive(Clone)]
pub struct GridSide {
    pub w: u16,
    pub h: u16,
    pub blocked: Vec<bool>,
}

#[inline]
fn gs_is_blocked(gs: &GridSide, x: i16, y: i16) -> bool {
    if x < 0 || y < 0 || x >= gs.w as i16 || y >= gs.h as i16 {
        return true;
    }
    gs.blocked[(y as usize) * (gs.w as usize) + (x as usize)]
}

/// Fog mirror: systems set bits here (Entity(2)); Sim::step folds the words back into Sim.vis
/// before hashing. The authoritative Visibility bitsets live on the Sim.
#[derive(Clone)]
pub struct VisMirror {
    pub words: [Visibility; 2],
}

impl Default for VisMirror {
    fn default() -> Self {
        VisMirror {
            words: [Visibility::new(0), Visibility::new(1)],
        }
    }
}

/// The complete simulation.
pub struct Sim {
    pub world: World,
    pub grid: Grid,
    pub scheduler: Scheduler,
    pub players: Vec<PlayerState>,
    pub vis: Vec<Visibility>,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut world = World::new(seed);
        let mut scheduler = Scheduler::new();
        // System order is the determinism contract; reorder == bump SIM_VERSION.
        scheduler.add_system("construction", construction_system);
        scheduler.add_system("training", training_system);
        scheduler.add_system("harvest", harvest_system);
        scheduler.add_system("movement", movement_system);
        scheduler.add_system("visibility", visibility_system);
        scheduler.add_system("combat", combat_system);
        world.storages.add(Entity(0), Deposits::default());
        world.storages.add(
            Entity(1),
            GridSide {
                w: MAP_W,
                h: MAP_H,
                blocked: vec![false; (MAP_W * MAP_H) as usize],
            },
        );
        world.storages.add(Entity(2), VisMirror::default());
        let grid = Self::generate_map(&mut world);
        let players = (0..MAX_PLAYERS).map(|_| PlayerState::new()).collect();
        let vis = (0..MAX_PLAYERS)
            .map(|i| Visibility::new(i as u32))
            .collect();
        let mut sim = Sim {
            world,
            grid,
            scheduler,
            players,
            vis,
        };
        sim.spawn_default_squads();
        sim
    }

    /// Install a preset mapgen map on a fresh `Sim::new(seed)`: despawns legacy node entities
    /// (anything with a NodePos that isn't a TC), clears the grid, applies the preset terrain,
    /// respawns the preset nodes, then re-blocks the 3x3 TC footprints.
    pub fn install_map(&mut self, map: crate::mapgen::MapGen) {
        use crate::mapgen::MAPGEN_VERSION as V;
        // reset movement grid, then rebuild via spawn_on (clears + repopulates node storages)
        for y in 0..MAP_H as i16 {
            for x in 0..MAP_W as i16 {
                self.grid.set_blocked(TilePos { x, y }, false);
            }
        }
        let _v = V;
        map.spawn_on(&mut self.world, &mut self.grid, true);
        // TC footprints (matches spawn_default_squads)
        for px in [14i16, 113i16] {
            for yy in px..px + 3 {
                for xx in px..px + 3 {
                    self.grid.set_blocked(TilePos { x: xx, y: yy }, true);
                }
            }
        }
        self.sync_mirrors();
    }
    pub fn tick(&self) -> Tick {
        self.world.tick
    }

    pub fn alive_units(&self) -> usize {
        self.world.storages.iter::<Unit>().count()
    }

    /// Refresh the GridSide / VisMirror side-storages so systems see current data.
    fn sync_mirrors(&mut self) {
        if let Some(gs) = self.world.storages.get_mut::<GridSide>(Entity(1)) {
            gs.blocked.clear();
            for y in 0..MAP_H as i16 {
                for x in 0..MAP_W as i16 {
                    gs.blocked.push(self.grid.is_blocked(TilePos { x, y }));
                }
            }
        }
        if let Some(vm) = self.world.storages.get_mut::<VisMirror>(Entity(2)) {
            for (i, v) in self.vis.iter().enumerate() {
                vm.words[i].words.clone_from(&v.words);
            }
        }
    }

    /// Fixed-tick step: sync mirrors -> canonical apply (failures are deterministic no-ops) ->
    /// tick++ -> systems -> fold visibility mirror -> drain harvest deposits.
    pub fn step(&mut self, cmds: &[(u32, Command)]) -> Result<(), SimError> {
        self.sync_mirrors();
        let mut sorted: Vec<(u32, &Command)> = cmds.iter().map(|(p, c)| (*p, c)).collect();
        sorted.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
        for (player, c) in sorted {
            match *c {
                Command::Move { unit, x, y } => {
                    let _ = self.apply_move(unit, x, y);
                }
                Command::Attack { unit, target } => {
                    let _ = self.apply_attack(unit, target);
                }
                Command::Stop { unit } => {
                    let _ = self.apply_stop(unit);
                }
                Command::Build { building, x, y } => {
                    let _ = self.build_at(player, building, x, y);
                }
                Command::Train {
                    building,
                    unit_kind,
                } => {
                    let _ = self.apply_train(building, unit_kind);
                }
                Command::Gather { unit, resource } => {
                    let _ = self.apply_gather(unit, resource);
                }
                Command::Sacrifice { .. } | Command::Evolve { .. } => {
                    // Structure-only (docs/design-factions.md): no state change in this slice.
                }
                Command::AgeUp { age } => {
                    let _ = self.apply_age_up(player, age);
                }
                Command::RallyPoint { building, tx, ty } => {
                    let _ = self.apply_rally_point(player, building, tx, ty);
                }
                Command::Rally { unit, tx, ty } => {
                    let _ = self.apply_rally(unit, tx, ty);
                }
            }
        }
        self.world.tick += 1;
        let tick = self.world.tick;
        self.scheduler.step(&mut self.world, tick);
        // Fold fog mirrors back into the authoritative bitsets (systems cannot touch Sim fields).
        if let Some(vm) = self.world.storages.get::<VisMirror>(Entity(2)).cloned() {
            for (i, v) in vm.words.iter().enumerate() {
                for (k, w) in v.words.iter().enumerate() {
                    self.vis[i].words[k] |= *w;
                }
            }
        }
        // Drain harvest deposits (insertion order = entity order, stable across runs).
        let deps = {
            let d = self.world.storages.get_mut::<Deposits>(Entity(0)).unwrap();
            std::mem::take(&mut d.0)
        };
        for (owner, slot, amount) in deps {
            if owner == REOPEN_OWNER && slot == REOPEN_MARKER {
                // amount packs (fx<<24)|(fy<<16)|(w<<8)|h for a wrecked footprint.
                let fx = (amount >> 24) & 255;
                let fy = (amount >> 16) & 255;
                let w = (amount >> 8) & 255;
                let h = amount & 255;
                for yy in fy..fy + h {
                    for xx in fx..fx + w {
                        self.grid.set_blocked(
                            TilePos {
                                x: xx as i16,
                                y: yy as i16,
                            },
                            false,
                        );
                    }
                }
                continue;
            }
            if (owner as usize) < self.players.len() {
                if slot == TRAIN_SCORE_MARKER || slot == BUILD_SCORE_MARKER {
                    self.players[owner as usize].score += amount as i64;
                } else {
                    self.players[owner as usize].stock.add(slot, amount as i64);
                    self.players[owner as usize].score += harvest_score_inc(amount);
                }
            }
        }
        // v2: age-up timers tick after system resolution (AgeUp command sets target + timer).
        for pl in self.players.iter_mut() {
            if pl.age_timer > 0 && (pl.age as u32) < pl.age_target as u32 {
                pl.age_timer -= 1;
                if pl.age_timer == 0 {
                    pl.age = pl.age_target.min(3);
                }
            }
        }
        // v4 (Tier A2): node regeneration — state-driven, no RNG; entity order keeps it fixed.
        if tick.is_multiple_of(50) {
            self.regen_nodes();
        }
        // v4 (Tier A1): army upkeep + starvation after UPKEEP_GRACE, every UPKEEP_INTERVAL.
        if tick.is_multiple_of(UPKEEP_INTERVAL) && tick > UPKEEP_GRACE {
            self.tick_upkeep();
        }
        // v2: win check — enemy TownCenter destroyed OR 20-min score cap. Runs AFTER systems so
        // destroyed buildings are settled; both players' winner flags update atomically.
        self.victory_check();
        Ok(())
    }

    /// Declare winner: 1) an enemy TownCenter (kind 0, status Done) has zero surviving copies
    /// while the player's own TC still exists -> player wins; 2) at MATCH_TICKS the higher score
    /// wins (ties resolve to player 0's id + 1 marker; no winner stays 0 while both alive).
    fn victory_check(&mut self) {
        if self.players.iter().any(|p| p.winner != 0) {
            return; // one decisive result already recorded and hashed
        }
        let tcs_alive = |owner: u32| -> bool {
            self.world
                .storages
                .iter::<Building>()
                .any(|(_e, b)| b.owner == owner && b.kind == 0 && b.status == BuildingStatus::Done)
        };
        let tcs_dead = |owner: u32| -> bool {
            let seen = self
                .world
                .storages
                .iter::<Building>()
                .any(|(_e, b)| b.owner == owner && b.kind == 0);
            seen && !tcs_alive(owner)
        };
        let any_tc_dead = tcs_dead(0) || tcs_dead(1);
        if any_tc_dead {
            if tcs_alive(0) && !tcs_alive(1) {
                self.players[0].winner = 1;
                self.players[1].winner = 1;
            } else if tcs_alive(1) && !tcs_alive(0) {
                self.players[0].winner = 2;
                self.players[1].winner = 2;
            } else {
                // Both destroyed (mutual annihilation) or one never existed: higher score wins.
                let w = if self.players[0].score >= self.players[1].score {
                    1
                } else {
                    2
                };
                self.players[0].winner = w;
                self.players[1].winner = w;
            }
            return;
        }
        if self.world.tick >= MATCH_TICKS {
            let w = if self.players[0].score >= self.players[1].score {
                1
            } else {
                2
            };
            self.players[0].winner = w;
            self.players[1].winner = w;
        }
    }

    /// v4 (Tier A2): deterministic node regen. Food +2 / 50 ticks, wood +1 / 150 ticks,
    /// gold none; capped at the node's total. Called every 50 ticks (wood gates by %150).
    /// Entity order iteration keeps the state hash fixed across replays.
    fn regen_nodes(&mut self) {
        let tick = self.world.tick;
        let nodes: Vec<(Entity, u8, i64)> = self
            .world
            .storages
            .iter::<ResourceNode>()
            .map(|(e, n)| (e, n.slot, n.total))
            .collect();
        for (e, slot, total) in nodes {
            let (iv, amt) = match slot {
                0 => (REGEN_INTERVAL[0], REGEN_AMOUNT[0]),
                1 => (REGEN_INTERVAL[1], REGEN_AMOUNT[1]),
                2 => (REGEN_INTERVAL[2], REGEN_AMOUNT[2]),
                _ => continue,
            };
            if iv == 0 || amt == 0 {
                continue;
            }
            if !tick.is_multiple_of(iv) {
                continue;
            }
            let cur = match self.world.storages.get::<ResourceNode>(e) {
                Some(n) => n.remaining,
                None => continue,
            };
            if cur >= total {
                continue;
            }
            if let Some(n) = self.world.storages.get_mut::<ResourceNode>(e) {
                n.remaining = (cur + amt).min(total);
            }
        }
    }

    /// v4 (Tier A1): army upkeep. Every UPKEEP_INTERVAL (after UPKEEP_GRACE grace ticks) each
    /// player pays SUM(UPKEEP[kind] per living unit) food. If their stock is negative after the
    /// drain, all their units take 1 starvation hp per interval (entity order); units at 1 hp
    /// are never killed by starvation — they take STARVE_SLOW speed penalty (floor 1) instead.
    /// Symmetric table: no player advantage.
    fn tick_upkeep(&mut self) {
        let mut units_by_owner: Vec<Vec<(Entity, u16)>> = vec![Vec::new(); self.players.len()];
        for (e, u) in self.world.storages.iter::<Unit>() {
            if (u.owner as usize) < self.players.len() {
                units_by_owner[u.owner as usize].push((e, u.kind));
            }
        }
        for (owner, list) in units_by_owner.iter().enumerate() {
            let mut due: i64 = 0;
            for (_e, kind) in list {
                let k = *kind as usize;
                if k < UPKEEP.len() {
                    due += UPKEEP[k];
                }
            }
            if due > 0 {
                // v4: pay what we can; remaining deficit makes the owner food-negative,
                // which triggers starvation below. `take` alone cannot express this.
                self.players[owner].stock.food -= due;
            }
            if self.players[owner].stock.food < 0 {
                let mut list: Vec<Entity> = list.iter().map(|(e, _)| *e).collect();
                list.sort();
                for e in list {
                    if let Some(u) = self.world.storages.get_mut::<Unit>(e) {
                        if u.hp > 1 {
                            u.hp = u.hp.saturating_sub(1);
                        } else {
                            u.speed_q = u.speed_q.saturating_sub(STARVE_SLOW).max(1);
                        }
                    }
                }
            }
        }
    }

    // ---- command application -------------------------------------------------

    fn unit_tile(&self, unit: u32) -> Result<(i16, i16), SimError> {
        let p = self
            .world
            .storages
            .get::<Pos>(Entity(unit))
            .ok_or(SimError::UnknownUnit)?;
        Ok((tile_of(p.x), tile_of(p.y)))
    }

    fn apply_move(&mut self, unit: u32, x: i16, y: i16) -> Result<(), SimError> {
        if !self.grid.contains(TilePos { x, y }) {
            return Err(SimError::OutOfMap);
        }
        let (ux, uy) = self.unit_tile(unit)?;
        if self.grid.is_blocked(TilePos { x, y }) {
            // Walls/structures block their cells: an order onto one never arrives (v2 rule).
            return Err(SimError::Occupied);
        }
        match find_path(&self.grid, TilePos { x: ux, y: uy }, TilePos { x, y }) {
            Some(path) => {
                self.world
                    .storages
                    .add(Entity(unit), Order::Move { path, waypoint: 0 });
                Ok(())
            }
            None => Err(SimError::PathUnreachable),
        }
    }

    /// Attack order: target is a unit or a building entity (v2). Order carries the target id;
    /// combat_system resolves unit-vs-unit and unit-vs-building with one representation.
    fn apply_attack(&mut self, unit: u32, target: u32) -> Result<(), SimError> {
        if self.world.storages.get::<Unit>(Entity(unit)).is_none() {
            return Err(SimError::UnknownUnit);
        }
        let is_unit = self.world.storages.get::<Unit>(Entity(target)).is_some();
        let is_building = self
            .world
            .storages
            .get::<Building>(Entity(target))
            .is_some();
        if !is_unit && !is_building {
            return Err(SimError::UnknownUnit);
        }
        self.world
            .storages
            .add(Entity(unit), Order::UnitAttack { target });
        Ok(())
    }

    fn apply_stop(&mut self, unit: u32) -> Result<(), SimError> {
        if self.world.storages.get::<Unit>(Entity(unit)).is_none() {
            return Err(SimError::UnknownUnit);
        }
        self.world.storages.add(Entity(unit), Order::Idle);
        self.world.storages.remove::<HarvestState>(Entity(unit));
        self.world.storages.remove::<WorkJob>(Entity(unit));
        Ok(())
    }

    /// Place a building on behalf of `owner`. Cost check is all-or-nothing; footprint must be
    /// inside the map and not blocked (buildings do NOT overwrite existing structures — Occupied).
    pub fn build_at(&mut self, owner: u32, kind: u16, x: i16, y: i16) -> Result<(), SimError> {
        let spec = building_spec(kind);
        let w = spec.w as i16;
        let h = spec.h as i16;
        if x < 0 || y < 0 || x + w > self.grid.w() as i16 || y + h > self.grid.h() as i16 {
            return Err(SimError::OutOfMap);
        }
        for yy in y..y + h {
            for xx in x..x + w {
                if self.grid.is_blocked(TilePos { x: xx, y: yy }) {
                    return Err(SimError::Occupied);
                }
            }
        }
        let stock = &mut self.players[owner as usize].stock;
        if stock.get(1) < spec.wood
            || stock.get(0) < spec.food
            || stock.get(2) < spec.gold
            || stock.get(3) < spec.mana
        {
            return Err(SimError::Unaffordable);
        }
        stock.take(1, spec.wood);
        stock.take(0, spec.food);
        stock.take(2, spec.gold);
        stock.take(3, spec.mana);
        let e = self.world.storages.spawn();
        self.world.storages.add(
            e,
            Building {
                owner,
                kind,
                status: BuildingStatus::Site,
                hp: 1,
                max_hp: spec.hp,
                timer: spec.build_time,
            },
        );
        let (cx, cy) = tile_center(x + w / 2, y + h / 2);
        self.world.storages.add(e, Pos { x: cx, y: cy });
        for yy in y..y + h {
            for xx in x..x + w {
                self.grid.set_blocked(TilePos { x: xx, y: yy }, true);
            }
        }
        Ok(())
    }

    /// Queue a unit at a finished building. Building must be Done; cost checked all-or-nothing at
    /// command time (no rollback rule from design-factions.md).
    fn apply_train(&mut self, building: u32, unit_kind: u16) -> Result<(), SimError> {
        let be = Entity(building);
        let b = self
            .world
            .storages
            .get::<Building>(be)
            .ok_or(SimError::BadCommand)?;
        if b.status != BuildingStatus::Done {
            return Err(SimError::BadCommand);
        }
        if unit_kind as usize >= UNIT_KIND_COUNT {
            return Err(SimError::BadCommand);
        }
        let (gate_kind, gate_age) = train_gate(unit_kind).ok_or(SimError::BadCommand)?;
        let owner = b.owner as usize;
        // v2 gates: the player needs one *done* building of the gate kind and age >= gate_age.
        {
            let has_gate = self.world.storages.iter::<Building>().any(|(_e, bb)| {
                bb.owner == b.owner && bb.kind == gate_kind && bb.status == BuildingStatus::Done
            });
            if !has_gate || self.players[owner].age < gate_age {
                return Err(SimError::BadCommand);
            }
            // Pop supply from TC+Farms vs. current population (sum of unit pop).
            let mut tcs = 0u32;
            let mut farms = 0u32;
            for (_e, bb) in self.world.storages.iter::<Building>() {
                if bb.owner != b.owner || bb.status != BuildingStatus::Done {
                    continue;
                }
                if bb.kind == 0 {
                    tcs += 1;
                } else if bb.kind == 1 {
                    farms += 1;
                }
            }
            let mut pop_used = 0u32;
            for (_e, u) in self.world.storages.iter::<Unit>() {
                if (u.owner as u32) == b.owner {
                    pop_used += unit_spec(u.kind).pop;
                }
            }
            let queue_pop: u32 = self
                .world
                .storages
                .get::<TrainQueue>(Entity(building))
                .map(|q| q.0.iter().map(|it| unit_spec(it.kind).pop).sum())
                .unwrap_or(0);
            let cap = pop_cap_for(tcs, farms);
            if pop_used + queue_pop + unit_spec(unit_kind).pop > cap {
                return Err(SimError::BadCommand); // pop cap reached (v2 rule)
            }
            // Training at this building must be its gate kind (no cross-building training).
            if b.kind != gate_kind {
                return Err(SimError::BadCommand);
            }
        }
        let spec = unit_spec(unit_kind);
        let stock = &mut self.players[b.owner as usize].stock;
        if stock.get(1) < spec.wood
            || stock.get(0) < spec.food
            || stock.get(2) < spec.gold
            || stock.get(3) < spec.mana
        {
            return Err(SimError::Unaffordable);
        }
        stock.take(1, spec.wood);
        stock.take(0, spec.food);
        stock.take(2, spec.gold);
        stock.take(3, spec.mana);
        let mut q = self
            .world
            .storages
            .get::<TrainQueue>(be)
            .cloned()
            .unwrap_or_default();
        q.0.push(TrainItem {
            kind: unit_kind,
            ticks_left: spec.train_time,
        });
        self.world.storages.add(be, q);
        Ok(())
    }

    /// Assign the nearest living node of `slot`: deterministic tie-break = lowest entity id (the
    /// iteration is ascending, strict `<` keeps the first draw).
    fn apply_gather(&mut self, unit: u32, resource: u8) -> Result<(), SimError> {
        if resource > 3 {
            return Err(SimError::BadCommand);
        }
        let (ux, uy) = self.unit_tile(unit)?;
        let nodes: Vec<Entity> = self
            .world
            .storages
            .iter::<ResourceNode>()
            .filter_map(|(e, n)| {
                if n.remaining > 0 && n.slot == resource {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        let mut best: Option<(i32, Entity, i16, i16)> = None;
        for e in nodes {
            let (nx, ny) = match self.world.storages.get::<NodePos>(e) {
                Some(np) => (np.x, np.y),
                None => continue,
            };
            let d = manhattan(ux as i32 - nx as i32, uy as i32 - ny as i32);
            if best.is_none() || d < best.as_ref().unwrap().0 {
                best = Some((d, e, nx, ny));
            }
        }
        let (_d, node, nx, ny) = best.ok_or(SimError::BadCommand)?;
        match find_path(
            &self.grid,
            TilePos { x: ux, y: uy },
            TilePos { x: nx, y: ny },
        ) {
            Some(path) => {
                self.world
                    .storages
                    .add(Entity(unit), Order::Harvest { node: node.0, path });
                self.world.storages.add(
                    Entity(unit),
                    HarvestState::ToNode {
                        node: node.0,
                        slot: resource,
                    },
                );
                Ok(())
            }
            None => Err(SimError::PathUnreachable),
        }
    }

    /// Apply an AgeUp command. Semantics: the player advances one age at a time
    /// (target must be exactly current_age+1, no skipping); costs are paid immediately and the
    /// upgrade completes after AGE_UP_TICKS[age] ticks of countdown (tick loop in step()).
    fn apply_age_up(&mut self, player: u32, age: u8) -> Result<(), SimError> {
        let age = age.min(3) as usize;
        let pl = &mut self.players[player as usize];
        if (pl.age as usize) + 1 != age {
            return Err(SimError::BadCommand); // one step at a time, no skipping
        }
        if pl.age_timer > 0 {
            return Err(SimError::BadCommand); // already advancing
        }
        let cost = AGE_COST[age];
        {
            let stock = &mut pl.stock;
            if stock.get(0) < cost[0] || stock.get(1) < cost[1] || stock.get(2) < cost[2] {
                return Err(SimError::Unaffordable);
            }
            stock.take(0, cost[0]);
            stock.take(1, cost[1]);
            stock.take(2, cost[2]);
        }
        pl.age_target = age as u8;
        pl.age_timer = AGE_UP_TICKS[age];
        Ok(())
    }

    /// v2: attach/replace a rally point on an owned, finished building.
    fn apply_rally_point(
        &mut self,
        player: u32,
        building: u32,
        tx: i16,
        ty: i16,
    ) -> Result<(), SimError> {
        let be = Entity(building);
        let b = self
            .world
            .storages
            .get::<Building>(be)
            .ok_or(SimError::BadCommand)?;
        if b.owner != player || b.status != BuildingStatus::Done {
            return Err(SimError::BadCommand);
        }
        self.world.storages.add(be, RallyPoint { tx, ty });
        Ok(())
    }

    /// v2: attach a unit rally target (harvest return hint).
    fn apply_rally(&mut self, unit: u32, tx: i16, ty: i16) -> Result<(), SimError> {
        if self.world.storages.get::<Unit>(Entity(unit)).is_none() {
            return Err(SimError::UnknownUnit);
        }
        self.world.storages.add(Entity(unit), UnitRally { tx, ty });
        Ok(())
    }

    // ---- spawning --------------------------------------------------------------

    pub fn spawn_unit_kind(&mut self, owner: u32, kind: u16, x: i16, y: i16) -> Entity {
        let spec = unit_spec(kind);
        let (cx, cy) = tile_center(x, y);
        let e = self.world.storages.spawn();
        self.world.storages.add(
            e,
            Unit {
                owner: owner as u8,
                kind,
                hp: spec.hp as i32,
                speed_q: spec.speed_q as u32,
                dmg: spec.dmg as i32,
                range: Fixed1616(spec.range_q),
            },
        );
        self.world.storages.add(e, Pos { x: cx, y: cy });
        self.world.storages.add(e, Order::Idle);
        e
    }

    /// Legacy demo helper (client main.rs): spawn one Militia unit for `owner`.
    pub fn spawn_unit(&mut self, owner: u32, x: i16, y: i16) -> u32 {
        self.spawn_unit_kind(owner, 1, x, y).0
    }

    /// Town Center (Done) + 3 Serfs for each side at the canonical spawn points (14,14)/(113,113).
    fn spawn_default_squads(&mut self) {
        for (owner, px) in [(0u32, 14i16), (1u32, 113i16)] {
            let spec = building_spec(0);
            let e = self.world.storages.spawn();
            self.world.storages.add(
                e,
                Building {
                    owner,
                    kind: 0,
                    status: BuildingStatus::Done,
                    hp: spec.hp,
                    max_hp: spec.hp,
                    timer: 0,
                },
            );
            let (cx, cy) = tile_center(px + 1, px + 1);
            self.world.storages.add(e, Pos { x: cx, y: cy });
            for yy in px..px + 3 {
                for xx in px..px + 3 {
                    self.grid.set_blocked(TilePos { x: xx, y: yy }, true);
                }
            }
            for i in 0..3 {
                self.spawn_unit_kind(owner, 0, px + 4 + i, px + 2);
            }
        }
    }

    // ---- map generation ----------------------------------------------------------
    // Strict SplitMix64 draw sequence from World::new(seed): identical map per seed.
    fn generate_map(world: &mut World) -> Grid {
        let mut grid = Grid::new(MAP_W, MAP_H);
        for x in 0..MAP_W as i16 {
            for b in 0..2i16 {
                grid.set_blocked(TilePos { x, y: b }, true);
                grid.set_blocked(
                    TilePos {
                        x,
                        y: MAP_H as i16 - 1 - b,
                    },
                    true,
                );
            }
        }
        for y in 0..MAP_H as i16 {
            for b in 0..2i16 {
                grid.set_blocked(TilePos { x: b, y }, true);
                grid.set_blocked(
                    TilePos {
                        x: MAP_W as i16 - 1 - b,
                        y,
                    },
                    true,
                );
            }
        }
        for _ in 0..700 {
            let v = world.rng.next_u64();
            let x = 16 + (v % (MAP_W as u64 - 32)) as i16;
            let y = 16 + ((v >> 16) % (MAP_H as u64 - 32)) as i16;
            let kind = ((v >> 32) % 4) as u8;
            if kind == 3 {
                continue;
            }
            if manhattan(x as i32 - 14, y as i32 - 14) < 14
                || manhattan(x as i32 - 113, y as i32 - 113) < 14
            {
                continue;
            }
            grid.set_blocked(TilePos { x, y }, true);
        }
        for i in 0..24u32 {
            let v = world.rng.next_u64();
            let slot = if i < 10 {
                0u8
            } else if i < 18 {
                1u8
            } else {
                2u8
            };
            let total: i64 = if i < 10 {
                2000 + (v % 1000) as i64
            } else if i < 18 {
                1500 + ((v >> 20) % 1000) as i64
            } else {
                500 + ((v >> 40) % 500) as i64
            };
            let x = 16 + (v % (MAP_W as u64 - 32)) as i16;
            let y = 16 + ((v >> 16) % (MAP_H as u64 - 32)) as i16;
            if grid.is_blocked(TilePos { x, y }) {
                continue;
            }
            if manhattan(x as i32 - 14, y as i32 - 14) < 10
                || manhattan(x as i32 - 113, y as i32 - 113) < 10
            {
                continue;
            }
            let e = world.storages.spawn();
            world.storages.add(
                e,
                ResourceNode {
                    slot,
                    total,
                    remaining: total,
                },
            );
            world.storages.add(e, NodePos { x, y });
        }
        grid
    }
}

// ===================== free functions (scheduler systems) =====================

/// Construction: Site->UnderConstruction (timer 0), UnderConstruction->Done (timer 0).
/// Completed Wonders bookkeep victory via the Building status only (victory_winner below).
fn construction_system(world: &mut World, _tick: Tick) {
    let ids: Vec<(Entity, Building)> = world
        .storages
        .iter::<Building>()
        .map(|(e, b)| (e, *b))
        .filter(|(_e, b)| {
            (b.status == BuildingStatus::Site || b.status == BuildingStatus::UnderConstruction)
                && b.timer > 0
        })
        .collect();
    for (e, mut b) in ids {
        b.timer -= 1;
        if b.timer == 0 {
            if b.status == BuildingStatus::Site {
                b.status = BuildingStatus::UnderConstruction;
                b.timer = building_spec(b.kind).build_time / 4;
            } else {
                b.status = BuildingStatus::Done;
                b.hp = b.max_hp;
                // v2 score: completed building +1 (tracked via Deposits marker drain in step()).
                if let Some(d) = world.storages.get_mut::<Deposits>(Entity(0)) {
                    d.0.push((b.owner as u8, BUILD_SCORE_MARKER, 1));
                }
            }
        }
        world.storages.add(e, b);
    }
}

/// Training: one tick off the front slot per Done building with a queue.
fn training_system(world: &mut World, _tick: Tick) {
    let list: Vec<(Entity, Building, TrainQueue, Pos)> = world
        .storages
        .iter::<Building>()
        .filter(|(_e, b)| b.status == BuildingStatus::Done)
        .filter_map(|(e, b)| {
            let q = world.storages.get::<TrainQueue>(e)?.clone();
            if q.0.is_empty() {
                return None;
            }
            let p = *world.storages.get::<Pos>(e)?;
            Some((e, *b, q, p))
        })
        .collect();
    for (e, _b, mut q, p) in list {
        q.0[0].ticks_left -= 1;
        if q.0[0].ticks_left == 0 {
            let item = q.0.remove(0);
            let bx = p.x.to_int_trunc() as i16;
            let by = p.y.to_int_trunc() as i16;
            spawn_unit_at(world, _b.owner, item.kind, bx + 1, by + 1);
            // v2 score: +2 per trained unit. training_system cannot see Sim.players, so it
            // records the credit into the Deposits side storage (owner, kind=TRAIN_MARKER, amt),
            // drained by Sim::step with the harvest deposits (same deterministic pass).
            if let Some(d) = world.storages.get_mut::<Deposits>(Entity(0)) {
                let inc = unit_spec(item.kind).pop as i32 * 2 + 2;
                d.0.push((_b.owner as u8, TRAIN_SCORE_MARKER, inc));
            }
            let _ = (bx, by);
        }
        world.storages.add(e, q);
    }
}

/// Harvest state machine. Picking completes to Carrying; Carrying walks to the owner's Town
/// Center and deposits (node decrement + deposit queue). ToNode walking is done by movement_system.
fn harvest_system(world: &mut World, _tick: Tick) {
    let units: Vec<(Entity, HarvestState)> = world
        .storages
        .iter::<HarvestState>()
        .map(|(e, s)| (e, *s))
        .collect();
    let mut deposits: Vec<(u8, u8, i32)> = Vec::new();
    let gs: Option<GridSide> = world.storages.get::<GridSide>(Entity(1)).cloned();
    for (e, st) in units {
        match st {
            HarvestState::ToNode { node, slot } => {
                if !node_alive(world, node) {
                    world.storages.remove::<HarvestState>(e);
                    world.storages.add(e, Order::Idle);
                    continue;
                }
                if matches!(
                    world.storages.get::<Order>(e),
                    Some(Order::Idle) | Some(Order::Harvest { .. })
                ) {
                    if let Some(np) = world.storages.get::<NodePos>(Entity(node)).cloned() {
                        let (px, py) = match world.storages.get::<Pos>(e) {
                            Some(pp) => (tile_of(pp.x), tile_of(pp.y)),
                            None => continue,
                        };
                        // A non-empty path is in flight: movement owns the unit.
                        let walking = matches!(
                            world.storages.get::<Order>(e),
                            Some(Order::Harvest { path, .. }) if !path.is_empty()
                        );
                        if walking {
                            continue;
                        }
                        if manhattan(px as i32 - np.x as i32, py as i32 - np.y as i32) <= 1 {
                            world.storages.remove::<HarvestState>(e);
                            world.storages.add(
                                e,
                                HarvestState::Picking {
                                    node,
                                    slot,
                                    ticks_left: PICK_TICKS,
                                },
                            );
                            continue;
                        }
                        // Idle but not adjacent (path exhausted mid-route or a
                        // blocked goal left no path): re-path toward the node.
                        if let Some(gs) = &gs {
                            if let Some(path) = bfs_path(gs, px, py, np.x, np.y) {
                                if path.is_empty() {
                                    world.storages.remove::<HarvestState>(e);
                                    world.storages.add(
                                        e,
                                        HarvestState::Picking {
                                            node,
                                            slot,
                                            ticks_left: PICK_TICKS,
                                        },
                                    );
                                    continue;
                                }
                                world.storages.add(e, Order::Move { path, waypoint: 0 });
                            }
                        }
                        world.storages.add(e, HarvestState::ToNode { node, slot });
                        continue;
                    } else {
                        world.storages.remove::<HarvestState>(e);
                        world.storages.add(e, Order::Idle);
                    }
                } else {
                    world.storages.add(e, HarvestState::ToNode { node, slot });
                }
            }
            HarvestState::Picking {
                node,
                slot,
                mut ticks_left,
            } => {
                if !node_alive(world, node) {
                    world.storages.remove::<HarvestState>(e);
                    world.storages.add(e, Order::Idle);
                    continue;
                }
                ticks_left = ticks_left.saturating_sub(1);
                if ticks_left == 0 {
                    world.storages.add(
                        e,
                        HarvestState::Carrying {
                            node,
                            slot,
                            amount: CARRY as i32,
                        },
                    );
                } else {
                    world.storages.add(
                        e,
                        HarvestState::Picking {
                            node,
                            slot,
                            ticks_left,
                        },
                    );
                }
            }
            HarvestState::Carrying { node, slot, amount } => {
                let owner = match world.storages.get::<Unit>(e) {
                    Some(u) => u.owner,
                    None => continue,
                };
                if let Some((tx, ty)) = find_tc(world, owner as u32) {
                    let (px, py) = match world.storages.get::<Pos>(e) {
                        Some(p) => (tile_of(p.x), tile_of(p.y)),
                        None => continue,
                    };
                    if manhattan((tx - px) as i32, (ty - py) as i32) <= 2 {
                        let take = if let Some(n) = world.storages.get::<ResourceNode>(Entity(node))
                        {
                            (amount as i64).min(n.remaining.max(0)) as i32
                        } else {
                            0
                        };
                        if let Some(n) = world.storages.get_mut::<ResourceNode>(Entity(node)) {
                            n.remaining -= take as i64;
                        }
                        if take > 0 {
                            deposits.push((owner, slot, take));
                        }
                        world.storages.remove::<HarvestState>(e);
                        world.storages.add(e, Order::Idle);
                        continue;
                    }
                }
                if let Some(gs) = &gs {
                    let (px, py) = match world.storages.get::<Pos>(e) {
                        Some(p) => (tile_of(p.x), tile_of(p.y)),
                        None => continue,
                    };
                    if let Some((tx, ty)) = find_tc(world, owner as u32) {
                        // The TC footprint itself is blocked, so BFS to the centre
                        // tile would fail. Scan the deterministic 8-neighbour ring
                        // around the TC; move to the nearest reachable tile and
                        // let the adjacent check above deposit at distance 1.
                        // The TC footprint blocks the centre and all radius-1 ring tiles,
                        // so scan the radius-2 ring; the deposit check is manhattan<=2.
                        let ring: [(i16, i16); 16] = [
                            (-2, 0),
                            (2, 0),
                            (0, -2),
                            (0, 2),
                            (-1, -2),
                            (-1, 2),
                            (1, -2),
                            (1, 2),
                            (-2, -1),
                            (-2, 1),
                            (2, -1),
                            (2, 1),
                            (-2, -2),
                            (-2, 2),
                            (2, -2),
                            (2, 2),
                        ];
                        let mut best: Option<(i32, (i16, i16))> = None;
                        for (ox, oy) in ring {
                            let gx = tx + ox;
                            let gy = ty + oy;
                            if gs_is_blocked(gs, gx, gy) {
                                continue;
                            }
                            if bfs_path(gs, px, py, gx, gy).is_none() {
                                continue;
                            }
                            let d = (gx - px).abs() as i32 + (gy - py).abs() as i32;
                            match best {
                                Some((bd, _)) if bd <= d => {}
                                _ => best = Some((d, (gx, gy))),
                            }
                        }
                        if let Some((_, (gx, gy))) = best {
                            if let Some(path) = bfs_path(gs, px, py, gx, gy) {
                                if path.is_empty() {
                                    // Already adjacent: deposit right here.
                                    let take = if let Some(n) =
                                        world.storages.get::<ResourceNode>(Entity(node))
                                    {
                                        (amount as i64).min(n.remaining.max(0)) as i32
                                    } else {
                                        0
                                    };
                                    if let Some(n) =
                                        world.storages.get_mut::<ResourceNode>(Entity(node))
                                    {
                                        n.remaining -= take as i64;
                                    }
                                    if take > 0 {
                                        deposits.push((owner, slot, take));
                                    }
                                    world.storages.remove::<HarvestState>(e);
                                    world.storages.add(e, Order::Idle);
                                    continue;
                                }
                                world.storages.add(e, Order::Move { path, waypoint: 0 });
                            }
                        }
                    }
                }
            }
        }
    }
    if !deposits.is_empty() {
        if let Some(d) = world.storages.get_mut::<Deposits>(Entity(0)) {
            d.0.extend(deposits);
        }
    }
}

#[inline]
fn node_alive(world: &World, node: u32) -> bool {
    matches!(world.storages.get::<ResourceNode>(Entity(node)), Some(n) if n.remaining > 0)
}

#[derive(Clone, Copy, Default)]
struct Pt {
    x: Fixed1616,
    y: Fixed1616,
}

impl Pt {
    #[inline]
    fn from_pos(p: &Pos) -> Pt {
        Pt { x: p.x, y: p.y }
    }
    #[inline]
    fn center(t: TilePos) -> Pt {
        let (x, y) = tile_center(t.x, t.y);
        Pt { x, y }
    }
}

/// One fixed-point march step toward `goal`; true when inside `step` of goal.
#[inline]
fn march(p: &mut Pt, goal: Pt, step: i64) -> bool {
    let step = Fixed1616(step);
    let dx = goal.x.0 - p.x.0;
    let dy = goal.y.0 - p.y.0;
    let adx = dx.abs();
    let ady = dy.abs();
    if adx <= step.0 && ady <= step.0 {
        p.x = goal.x;
        p.y = goal.y;
        return true;
    }
    if adx >= ady {
        let n = if dx > 0 {
            step.0.min(adx)
        } else {
            -step.0.min(adx)
        };
        p.x.0 += n;
    } else {
        let n = if dy > 0 {
            step.0.min(ady)
        } else {
            -step.0.min(ady)
        };
        p.y.0 += n;
    }
    false
}

fn movement_system(world: &mut World, _tick: Tick) {
    let gs: Option<GridSide> = world.storages.get::<GridSide>(Entity(1)).cloned();
    let units: Vec<(Entity, u32)> = world
        .storages
        .iter::<Unit>()
        .map(|(e, u)| (e, u.speed_q))
        .collect();
    for (e, speed_q) in units {
        if speed_q == 0 {
            continue;
        }
        let step = Fixed1616::from_rational(speed_q as i64, SPEED_SCALE).0;
        let mut p = match world.storages.get::<Pos>(e) {
            Some(p) => Pt::from_pos(p),
            None => continue,
        };
        let order = match world.storages.get::<Order>(e) {
            Some(o) => o.clone(),
            None => continue,
        };
        match order {
            Order::Move { path, waypoint } => {
                if waypoint >= path.len() {
                    world.storages.add(e, Order::Idle);
                    continue;
                }
                // v2: if a wall/structure was placed into the path while walking, halt and idle
                // (paths never cross blocked cells because build_at rejects occupied footprints).
                let goal = Pt::center(path[waypoint]);
                if gs.as_ref().is_some_and(|g| {
                    gs_is_blocked(
                        g,
                        goal.x.to_int_trunc() as i16,
                        goal.y.to_int_trunc() as i16,
                    )
                }) {
                    world.storages.add(e, Order::Idle);
                    continue;
                }
                let arrived = march(&mut p, goal, step);
                let wp = if arrived { waypoint + 1 } else { waypoint };
                world.storages.add(e, Pos { x: p.x, y: p.y });
                if wp >= path.len() {
                    world.storages.add(e, Order::Idle);
                } else {
                    world.storages.add(e, Order::Move { path, waypoint: wp });
                }
            }
            Order::Harvest { node, mut path } => {
                if path.is_empty() {
                    // Path exhausted: hand back to the harvest mirror, which
                    // owns the state machine (slot survives, no hardcode).
                    world.storages.add(e, Order::Idle);
                } else {
                    let goal = Pt::center(path[0]);
                    let arrived = march(&mut p, goal, step);
                    world.storages.add(e, Pos { x: p.x, y: p.y });
                    if arrived {
                        path.remove(0);
                    }
                    world.storages.add(e, Order::Harvest { node, path });
                }
            }
            Order::Construct { building, mut path } => {
                if path.is_empty() {
                    world.storages.add(e, Order::Idle);
                    world.storages.add(
                        e,
                        WorkJob {
                            building,
                            node: 0,
                            slot: 0,
                            mode: WorkMode::Build,
                        },
                    );
                } else {
                    let goal = Pt::center(path[0]);
                    let arrived = march(&mut p, goal, step);
                    world.storages.add(e, Pos { x: p.x, y: p.y });
                    if arrived {
                        path.remove(0);
                    }
                    world.storages.add(e, Order::Construct { building, path });
                }
            }
            Order::Idle | Order::Attack { .. } => {}
            Order::UnitAttack { target } => {
                // v2: walk toward the target's tile centre at normal speed until in range
                // (combat_system then applies damage; we do not path — straight-line chase with
                // the same march() maths as movement, so no new RNG/paths).
                let tp = match world.storages.get::<Pos>(Entity(target)) {
                    Some(pp) => Pt::from_pos(pp),
                    None => {
                        world.storages.add(e, Order::Idle);
                        continue;
                    }
                };
                let arrived = march(&mut p, tp, step);
                world.storages.add(e, Pos { x: p.x, y: p.y });
                if arrived {
                    // Staying adjacent keeps the order (attack loop) — march pins us to the tile.
                }
            }
        }
    }
}

// ===================== visibility + fog =====================

/// Each living unit reveals a SIGHT_BASE-radius diamond around its tile into the owner's fog
/// mirror (Entity(2)). Fog is IN the state hash (design-factions.md: lead decision).
fn visibility_system(world: &mut World, _tick: Tick) {
    let units: Vec<(Entity, u32, i16, i16)> = world
        .storages
        .iter::<Unit>()
        .filter_map(|(e, u)| {
            let p = world.storages.get::<Pos>(e)?;
            Some((e, u.owner as u32, tile_of(p.x), tile_of(p.y)))
        })
        .collect();
    let vm = match world.storages.get_mut::<VisMirror>(Entity(2)) {
        Some(vm) => vm,
        None => return,
    };
    for (_e, owner, x, y) in units {
        if owner >= 2 {
            continue;
        }
        let w = &mut vm.words[owner as usize];
        for dy in -SIGHT_BASE..=SIGHT_BASE {
            for dx in -SIGHT_BASE..=SIGHT_BASE {
                if manhattan(dx, dy) <= SIGHT_BASE {
                    w.set(x + dx as i16, y + dy as i16);
                }
            }
        }
    }
}

// ===================== combat =====================

/// Attack orders resolve in ascending entity order: attacker hits target if center distance is
/// within range (fixed-point squared compare). Deaths despawn after the pass (ascending order).
fn combat_system(world: &mut World, _tick: Tick) {
    let attackers: Vec<(Entity, u32, i32, Fixed1616, Pt)> = world
        .storages
        .iter::<Order>()
        .filter_map(|(e, o)| match o {
            Order::Attack { target } | Order::UnitAttack { target } => {
                let u = world.storages.get::<Unit>(e)?;
                let p = world.storages.get::<Pos>(e)?;
                Some((e, *target, u.dmg, u.range, Pt::from_pos(p)))
            }
            _ => None,
        })
        .collect();
    let mut dead: Vec<Entity> = Vec::new();
    let mut dead_units: Vec<Entity> = Vec::new();
    for (e, target, dmg, range, from) in attackers {
        if dead_units.contains(&e) {
            continue;
        }
        if world.storages.get::<Unit>(Entity(target)).is_some() {
            let tp = match world.storages.get::<Pos>(Entity(target)) {
                Some(p) => *p,
                None => continue,
            };
            let dx = tp.x.0 - from.x.0;
            let dy = tp.y.0 - from.y.0;
            let r = range.0;
            if dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) <= r.wrapping_mul(r) {
                if let Some(tu) = world.storages.get_mut::<Unit>(Entity(target)) {
                    tu.hp = tu.hp.saturating_sub(dmg);
                    if tu.hp <= 0 {
                        dead_units.push(Entity(target));
                    }
                }
            }
            continue;
        }
        // v2: target is a building — melee range (fixed reach), destructible walls/structures.
        if world.storages.get::<Building>(Entity(target)).is_none() {
            continue;
        }
        let tp = match world.storages.get::<Pos>(Entity(target)) {
            Some(p) => *p,
            None => continue,
        };
        let dx = tp.x.0 - from.x.0;
        let dy = tp.y.0 - from.y.0;
        let r = BUILDING_ATTACK_RANGE;
        if dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) <= r.wrapping_mul(r) {
            let mut b = match world.storages.get::<Building>(Entity(target)) {
                Some(b) => *b,
                None => continue,
            };
            // Site/UnderConstruction take full damage; Wreck stays dead (hp 0) — never revives.
            if b.status != BuildingStatus::Wreck {
                let applied = dmg.saturating_add(WALL_BUILDING_DMG);
                b.hp = b.hp.saturating_sub(applied as u32);
                if b.hp == 0 {
                    b.status = BuildingStatus::Wreck;
                    b.timer = 0;
                    dead.push(Entity(target));
                }
                world.storages.add(Entity(target), b);
            }
        }
    }
    dead_units.sort();
    dead_units.dedup();
    for e in dead_units {
        world.storages.remove::<Unit>(e);
        world.storages.remove::<Pos>(e);
        world.storages.remove::<Order>(e);
        world.storages.remove::<HarvestState>(e);
        world.storages.remove::<WorkJob>(e);
        world.storages.remove::<UnitRally>(e);
        world.storages.despawn(e);
    }
    dead.sort();
    dead.dedup();
    // v2: wrecked buildings reopen their footprint. Record (fx, fy, w, h) keys in Deposits
    // (REOPEN_OWNER/REOPEN_MARKER) and let Sim::step fold them into the authoritative grid.
    let mut reopen_keys: Vec<i32> = Vec::new();
    for e in dead.iter() {
        if let Some(b) = world.storages.get::<Building>(*e).copied() {
            if b.status != BuildingStatus::Wreck {
                continue;
            }
            let spec = building_spec(b.kind);
            if let Some(p) = world.storages.get::<Pos>(*e).copied() {
                let cx = p.x.to_int_trunc();
                let cy = p.y.to_int_trunc();
                let fx = cx - (spec.w as i32) / 2;
                let fy = cy - (spec.h as i32) / 2;
                reopen_keys
                    .push((fx << 24) | (fy << 16) | ((spec.w as i32) << 8) | (spec.h as i32));
            }
        }
    }
    if !reopen_keys.is_empty() {
        if let Some(d) = world.storages.get_mut::<Deposits>(Entity(0)) {
            for key in reopen_keys {
                d.0.push((REOPEN_OWNER, REOPEN_MARKER, key));
            }
        }
    }
    for e in dead {
        world.storages.remove::<Order>(e);
        world.storages.remove::<TrainQueue>(e);
        world.storages.remove::<RallyPoint>(e);
        world.storages.remove::<Pos>(e);
        world.storages.despawn(e);
    }
}

// ---- test-only helpers (cfg(test) only; zero impact on production determinism) ----
impl Sim {
    /// Thin wrapper over the private command path (same code as live play).
    pub fn apply_move_for_test(&mut self, unit: u32, x: i16, y: i16) -> Result<(), SimError> {
        self.apply_move(unit, x, y)
    }

    pub fn add_order_for_test(&mut self, unit: u32, target: u32) {
        self.world
            .storages
            .add(Entity(unit), Order::Attack { target });
    }

    /// Route through the production Command path (gate/pop/cost logic single-sourced).
    pub fn train_via_command(
        &mut self,
        player: u32,
        building: u32,
        unit_kind: u16,
    ) -> Result<(), SimError> {
        let _ = player;
        self.apply_train(building, unit_kind)
    }

    pub fn tile_of_unit(&self, unit: u32) -> Option<(i16, i16)> {
        self.unit_tile(unit).ok()
    }

    pub fn first_building_entity_of(&self, owner: u32, kind: u16) -> u32 {
        for (e, b) in self.world.storages.iter::<Building>() {
            if b.owner == owner && b.kind == kind && b.status == BuildingStatus::Done {
                return e.0;
            }
        }
        0
    }

    pub fn building_at(&self, owner: u32, kind: u16, x: i16, y: i16) -> u32 {
        for (e, b) in self.world.storages.iter::<Building>() {
            if b.owner == owner && b.kind == kind {
                if let Some(pp) = self.world.storages.get::<Pos>(e) {
                    if (tile_of(pp.x), tile_of(pp.y)) == (x, y) {
                        return e.0;
                    }
                }
            }
        }
        0
    }

    pub fn building_wrecked(&self, entity: u32) -> bool {
        self.world
            .storages
            .get::<Building>(Entity(entity))
            .map(|b| b.status == BuildingStatus::Wreck)
            .unwrap_or(false)
    }
}

// ===================== helper fns =====================

fn spawn_unit_at(world: &mut World, owner: u32, kind: u16, x: i16, y: i16) {
    let spec = unit_spec(kind);
    let (cx, cy) = tile_center(x, y);
    let e = world.storages.spawn();
    world.storages.add(
        e,
        Unit {
            owner: owner as u8,
            kind,
            hp: spec.hp as i32,
            speed_q: spec.speed_q as u32,
            dmg: spec.dmg as i32,
            range: Fixed1616(spec.range_q),
        },
    );
    world.storages.add(e, Pos { x: cx, y: cy });
    world.storages.add(e, Order::Idle);
}

fn find_tc(world: &World, owner: u32) -> Option<(i16, i16)> {
    for (e, b) in world.storages.iter::<Building>() {
        if b.owner == owner && b.kind == 0 && b.status == BuildingStatus::Done {
            if let Some(p) = world.storages.get::<Pos>(e) {
                return Some((tile_of(p.x), tile_of(p.y)));
            }
        }
    }
    None
}

/// Deterministic 4-connected BFS over the GridSide mirror (same neighbor order as engine-path:
/// N,E,S,W). Returns the tile path excluding the start tile.
fn bfs_path(gs: &GridSide, sx: i16, sy: i16, gx: i16, gy: i16) -> Option<Vec<TilePos>> {
    const NEIGH: [(i16, i16); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    if sx == gx && sy == gy {
        return Some(Vec::new());
    }
    if gs_is_blocked(gs, gx, gy) {
        return None;
    }
    let w = gs.w as usize;
    let h = gs.h as usize;
    if sx < 0
        || sy < 0
        || gx < 0
        || gy < 0
        || sx as usize >= w
        || sy as usize >= h
        || gx as usize >= w
        || gy as usize >= h
    {
        return None;
    }
    let mut prev: Vec<Option<usize>> = vec![None; w * h];
    let mut queue: Vec<usize> = Vec::new();
    queue.push(sy as usize * w + sx as usize);
    let goal = gy as usize * w + gx as usize;
    let mut head = 0;
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        if cur == goal {
            break;
        }
        let cx = (cur % w) as i16;
        let cy = (cur / w) as i16;
        for (dx, dy) in NEIGH {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= gs.w as i16 || ny >= gs.h as i16 {
                continue;
            }
            let idx = ny as usize * w + nx as usize;
            if prev[idx].is_some() || gs_is_blocked(gs, nx, ny) {
                continue;
            }
            prev[idx] = Some(cur);
            queue.push(idx);
        }
    }
    // None when goal unreachable (never overwritten in prev); walk-back below would loop, so bail.
    prev[goal]?;
    let mut path = Vec::new();
    let mut cur = goal;
    while cur != sy as usize * w + sx as usize {
        path.push(TilePos {
            x: (cur % w) as i16,
            y: (cur / w) as i16,
        });
        cur = prev[cur]?;
    }
    path.reverse();
    Some(path)
}

// ===================== canonical state hash + demo helpers =====================

impl Sim {
    /// Canonical state hash. SIM_VERSION is owned by lib.rs; field order is the golden contract.
    pub fn hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.write_u32(SIM_VERSION);
        h.write_u64(self.world.tick);
        h.write_u64(self.world.rng.state());
        h.write_u32(self.grid.w() as u32);
        h.write_u32(self.grid.h() as u32);
        h.write_u32(self.grid.blocked_count() as u32);
        for pl in &self.players {
            h.write_i64(pl.stock.food);
            h.write_i64(pl.stock.wood);
            h.write_i64(pl.stock.gold);
            h.write_i64(pl.stock.mana);
            h.write_u32(pl.age as u32);
            h.write_u32(pl.wonder);
            h.write_u64(pl.wonder_started);
            // v2 hashed state (new in SIM_VERSION=3).
            h.write_u32(pl.age_target as u32);
            h.write_u32(pl.age_timer);
            h.write_u32(pl.civ as u32);
            h.write_i64(pl.score);
            h.write_u32(pl.winner);
        }
        for v in &self.vis {
            v.hash_into(&mut h);
        }
        let mut bs: Vec<(Entity, &Building)> = self.world.storages.iter::<Building>().collect();
        bs.sort_by_key(|(e, _)| e.0);
        for (e, b) in &bs {
            h.write_u32(e.0);
            h.write_u32(b.owner);
            h.write_u16(b.kind);
            h.write_u8(b.status.code());
            h.write_u32(b.hp);
            h.write_u32(b.max_hp);
            h.write_u32(b.timer);
            if let Some(pp) = self.world.storages.get::<Pos>(*e) {
                h.write_i32(pp.x.to_int_trunc());
                h.write_i32(pp.y.to_int_trunc());
            }
            if let Some(q) = self.world.storages.get::<TrainQueue>(*e) {
                h.write_u64(q.0.len() as u64);
                for it in &q.0 {
                    h.write_u16(it.kind);
                    h.write_u32(it.ticks_left);
                }
            }
            if let Some(r) = self.world.storages.get::<RallyPoint>(*e) {
                r.hash_into(&mut h);
            }
        }
        let mut ns: Vec<(Entity, &ResourceNode)> =
            self.world.storages.iter::<ResourceNode>().collect();
        ns.sort_by_key(|(e, _)| e.0);
        for (e, n) in &ns {
            h.write_u32(e.0);
            n.hash_into(&mut h);
            if let Some(np) = self.world.storages.get::<NodePos>(*e) {
                np.hash_into(&mut h);
            }
        }
        let mut us: Vec<(Entity, &Unit)> = self.world.storages.iter::<Unit>().collect();
        us.sort_by_key(|(e, _)| e.0);
        for (e, u) in &us {
            h.write_u32(e.0);
            h.write_u8(u.owner);
            h.write_u16(u.kind);
            h.write_i32(u.hp);
            h.write_u32(u.speed_q);
            h.write_i32(u.dmg);
            h.write_i64(u.range.inner());
            if let Some(pp) = self.world.storages.get::<Pos>(*e) {
                h.write_i32(pp.x.to_int_trunc());
                h.write_i32(pp.y.to_int_trunc());
            }
            if let Some(o) = self.world.storages.get::<Order>(*e) {
                o.hash_into(&mut h);
            }
            if let Some(s) = self.world.storages.get::<HarvestState>(*e) {
                s.hash_into(&mut h);
            }
            if let Some(w) = self.world.storages.get::<WorkJob>(*e) {
                w.hash_into(&mut h);
            }
            if let Some(r) = self.world.storages.get::<UnitRally>(*e) {
                r.hash_into(&mut h);
            }
        }
        h.finish()
    }
}

/// Spawn three units for `owner` (determinism-test fixture; the economy map is seeded).
pub fn spawn_demo_units(sim: &mut Sim, owner: u32) {
    sim.spawn_unit_kind(owner, 1, 18, 16);
    sim.spawn_unit_kind(owner, 1, 19, 16);
    sim.spawn_unit_kind(owner, 1, 20, 16);
}

/// Replay a command log; `cmds[t]` is the (unordered) batch for tick t (step canonicalizes).
pub fn run_sim(seed: u64, ticks: u64, cmds: &[TickCommands]) -> u64 {
    let mut sim = Sim::new(seed);
    for t in 0..ticks {
        let batch: Vec<(u32, Command)> = cmds
            .get(t as usize)
            .map(|tc| tc.cmds.clone())
            .unwrap_or_default();
        let _ = sim.step(&batch);
    }
    sim.hash()
}
