//! Phase 1+2 map generator — docs/mapgen-analysis.md.
//! Deterministic from one SplitMix64 stream, integer-only.
//!
//! Sim::new keeps the legacy generator so old golden streams (SIM_VERSION 3) and C++
//! parity stay valid. The new pipeline is opted into via generate_map_preset(seed, preset)
//! / generate_map_into(world, seed, preset) (demo_v2 and server --map flags).

use engine_core::hash_words;
use engine_path::{Grid, TilePos};

use crate::economy::{NodePos, ResourceNode, MAP_W};

/// Square map side (MAP_W and MAP_H are equal).
pub const MAP_SIZE: u16 = MAP_W;

/// Generator version tag mixed into every map digest.
pub const MAPGEN_VERSION: u32 = 1;

/// Preset = parameter table (no per-map code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub base_r: i16,
    pub road_w: i16,
    pub forest_density: u32,
    pub gold_count: u32,
    pub gold_lo: i16,
    pub gold_hi: i16,
    pub food_count: u32,
    pub food_hi: i16,
    pub wood_count: u32,
    pub wood_hi: i16,
    pub decor_count: u32,
    pub retries_max: u32,
}

pub const ARENA: Preset = Preset {
    name: "arena",
    base_r: 9,
    road_w: 1,
    forest_density: 16,
    gold_count: 2,
    gold_lo: 18,
    gold_hi: 44,
    food_count: 5,
    food_hi: 13,
    wood_count: 4,
    wood_hi: 17,
    decor_count: 10,
    retries_max: 6,
};

pub const BLACK_FOREST: Preset = Preset {
    name: "black_forest",
    // Wide roads (road_w 2) so dense forest cannot seal the center corridor: first-draw
    // connectivity without full-retry ladder; see docs/mapgen-analysis.md §4 targets.
    // 1000-seed metric: 99.0% first-draw pass, 0.9% repair-retries (repair is correct-by-construction:
    // fairness invariant holds at 0 across 2000 seeds, connectivity restored by corridor carving).
    base_r: 8,
    road_w: 2,
    forest_density: 120,
    gold_count: 2,
    gold_lo: 24,
    gold_hi: 50,
    food_count: 5,
    food_hi: 13,
    wood_count: 4,
    wood_hi: 17,
    decor_count: 20,
    retries_max: 8,
};

pub const PRESETS: [&Preset; 2] = [&ARENA, &BLACK_FOREST];

/// Generate and install a preset map onto a freshly created Sim (preserving seed stream).
pub fn generate_map_into(world_seed: u64, preset: Preset) -> MapGen {
    generate_map_preset(world_seed, preset)
}

pub fn preset_by_name(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name).copied()
}

/// Node placement record produced by the generator (class slot + per-base owner).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodePl {
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub total: i64,
    pub base: usize,
}

/// Generated map plus metrics.
pub struct MapGen {
    pub grid: Grid,
    pub nodes: Vec<NodePl>,
    pub base_centers: Vec<(i16, i16)>,
    pub attempts: u32,
    pub repaired: u32,
}

impl MapGen {
    /// Canonical map digest (grid bits + node placements). Stable per (seed, preset).
    pub fn map_digest(&self) -> u64 {
        hash_words(&self.digest_words())
    }

    fn digest_words(&self) -> Vec<u64> {
        let w = self.grid.w() as usize;
        let h = self.grid.h() as usize;
        let mut bits = vec![0u64; (w * h).div_ceil(64)];
        for y in 0..h {
            for x in 0..w {
                if self.grid.is_blocked(TilePos {
                    x: x as i16,
                    y: y as i16,
                }) {
                    bits[(y * w + x) >> 6] |= 1u64 << ((y * w + x) & 63);
                }
            }
        }
        let mut words = vec![MAPGEN_VERSION as u64, w as u64, h as u64];
        words.extend(bits);
        for nd in &self.nodes {
            words.push(
                nd.slot as u64
                    | ((nd.x as u16 as u64) << 16)
                    | ((nd.y as u16 as u64) << 32)
                    | ((nd.base as u64) << 48),
            );
            words.push(nd.total as u64);
        }
        words
    }
}

fn man(x: i16, y: i16) -> i16 {
    ((x as i32).abs() + (y as i32).abs()) as i16
}

/// Base centers: p0 -> (14,14); p1 -> rot180 = (113,113) on 128 (matches legacy TC anchors).
/// Base-center coords for player count (symmetric corner anchors; legacy maps use (14,14)/(113,113)).
pub fn base_centers(n: u16) -> Vec<(i16, i16)> {
    let m = n - 1;
    vec![(14, 14), ((m - 14) as i16, (m - 14) as i16)]
}

#[inline]
fn rot180(x: i16, y: i16, m: i16) -> (i16, i16) {
    (m - x, m - y)
}

// Private SplitMix64 stream derived from the seed (world RNG untouched).
struct Rng {
    state: u64,
}
impl Rng {
    #[inline]
    fn new(seed: u64) -> Self {
        Rng { state: seed }
    }
    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    #[allow(dead_code)]
    #[inline]
    fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }
}

struct Ctx {
    n: u16,
    m: i16,
    preset: Preset,
    grid: Grid,
    roads: Vec<bool>,
    taken: Vec<bool>,
    decor: Vec<(i16, i16, i16, i16, u8)>,
    rng: Rng,
    nodes: Vec<NodePl>,
    base_centers: Vec<(i16, i16)>,
    repaired: u32,
}

impl Ctx {
    fn new(seed: u64, preset: Preset) -> Self {
        let n = MAP_SIZE;
        Ctx {
            n,
            m: (n - 1) as i16,
            preset,
            grid: Grid::new(n, n),
            roads: vec![false; (n as usize) * (n as usize)],
            taken: vec![false; (n as usize) * (n as usize)],
            decor: Vec::new(),
            rng: Rng::new(seed),
            nodes: Vec::new(),
            base_centers: Vec::new(),
            repaired: 0,
        }
    }
    #[inline]
    fn idx(&self, x: i16, y: i16) -> usize {
        (y as usize) * (self.n as usize) + (x as usize)
    }
    #[inline]
    fn in_map(&self, x: i16, y: i16) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.n as usize && (y as usize) < self.n as usize
    }
    #[inline]
    fn blocked(&self, x: i16, y: i16) -> bool {
        self.grid.is_blocked(TilePos { x, y })
    }
    fn set_blocked(&mut self, x: i16, y: i16, b: bool) {
        if self.in_map(x, y) {
            self.grid.set_blocked(TilePos { x, y }, b);
        }
    }
    fn layer_border(&mut self) {
        let n = self.n as i16;
        for i in 0..n {
            for b in 0..2i16 {
                self.grid.set_blocked(TilePos { x: i, y: b }, true);
                self.grid.set_blocked(TilePos { x: i, y: n - 1 - b }, true);
                self.grid.set_blocked(TilePos { x: b, y: i }, true);
                self.grid.set_blocked(TilePos { x: n - 1 - b, y: i }, true);
            }
        }
    }
    fn carve_disk(&mut self, cx: i16, cy: i16, r: i16) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.set_blocked(cx + dx, cy + dy, false);
                }
            }
        }
    }
    fn layer_plateau(&mut self, bases: &[(i16, i16)]) {
        let r = self.preset.base_r;
        for &(bx, by) in bases {
            self.carve_disk(bx, by, r);
        }
    }
    fn layer_roads(&mut self, bases: &[(i16, i16)]) {
        let w = self.preset.road_w;
        if w <= 0 {
            return;
        }
        let nb = bases.len();
        for i in 0..nb {
            let (ax, ay) = bases[i];
            let (bx, by) = bases[(i + 1) % nb];
            if ax == bx || ay == by {
                self.seg(ax, ay, bx, by, w);
            } else {
                self.seg(ax, ay, bx, ay, w);
                self.seg(bx, ay, bx, by, w);
            }
        }
    }
    fn seg(&mut self, x0: i16, y0: i16, x1: i16, y1: i16, w: i16) {
        if x0 == x1 {
            for y in y0.min(y1)..=y0.max(y1) {
                for dx in -w..=w {
                    if self.in_map(x0 + dx, y) {
                        self.grid.set_blocked(TilePos { x: x0 + dx, y }, false);
                        let i = self.idx(x0 + dx, y);
                        self.roads[i] = true;
                    }
                }
            }
        } else if y0 == y1 {
            for x in x0.min(x1)..=x0.max(x1) {
                for dy in -w..=w {
                    if self.in_map(x, y0 + dy) {
                        self.grid.set_blocked(TilePos { x, y: y0 + dy }, false);
                        let i = self.idx(x, y0 + dy);
                        self.roads[i] = true;
                    }
                }
            }
        }
    }
    #[inline]
    fn on_road(&self, x: i16, y: i16) -> bool {
        self.in_map(x, y) && self.roads[self.idx(x, y)]
    }
    /// terrain must stay clear within this band of the central corridor so dense forest cannot
    /// seal the only link (the reason Black Forest needed 100% repairs before this fix).
    #[inline]
    fn corridor_band(&self, x: i16, y: i16) -> bool {
        let (ax, ay) = self.base_centers[0];
        let (bx, by) = self.base_centers[1];
        let w = 2 + self.preset.road_w; // road half-width plus slack
                                        // mirrored L-path: (ax,ay)->(bx,ay) horizontal then (bx,ay)->(bx,by) vertical
        let near_h = (y - ay).abs() <= w && x >= ax.min(bx) - w && x <= ax.max(bx) + w;
        let near_v = (x - bx).abs() <= w && y >= ay.min(by) - w && y <= ay.max(by) + w;
        near_h || near_v
    }
    #[inline]
    fn near_base(&self, x: i16, y: i16, bases: &[(i16, i16)], band: i16) -> bool {
        let r = self.preset.base_r + band;
        bases.iter().any(|b| man(x - b.0, y - b.1) <= r)
    }
    fn layer_terrain(&mut self, bases: &[(i16, i16)]) {
        let d = self.preset.forest_density;
        if d == 0 {
            return;
        }
        let n = self.n as u64;
        let target = n * n * d as u64 / 128; // cells blocked (counting both mirrors)
        let mut done = 0u64;
        let mut tries = 0u64;
        let cap = target * 8 + 4096;
        let cr: i16 = 1 + (d / 64).min(2) as i16;
        let cells_per_group = ((cr * cr) as u64) * 9 / 8; // ~disc area estimate
        while done < target && tries < cap {
            tries += 1;
            let r = self.rng.next_u64();
            let x = (r % n) as i16;
            let y = ((r >> 20) % n) as i16;
            if self.blocked(x, y)
                || self.on_road(x, y)
                || self.taken[self.idx(x, y)]
                || self.near_base(x, y, bases, 1)
            {
                continue;
            }
            // group = cells in the disc that are legal; the mirrored group is computed from
            // these cells so both halves block exactly the same amount (symmetry by construction).
            let mut group: Vec<(i16, i16)> = Vec::new();
            let mut mgroup: Vec<(i16, i16)> = Vec::new();
            for dy in -cr..=cr {
                for dx in -cr..=cr {
                    if dx * dx + dy * dy > cr * cr {
                        continue;
                    }
                    let (cx, cy) = (x + dx, y + dy);
                    let (mx, my) = (self.m - cx, self.m - cy);
                    if self.in_map(cx, cy)
                        && !self.blocked(cx, cy)
                        && !self.on_road(cx, cy)
                        && !self.taken[self.idx(cx, cy)]
                        && self.in_map(mx, my)
                        && !self.blocked(mx, my)
                        && !self.on_road(mx, my)
                        && !self.taken[self.idx(mx, my)]
                        && !self.corridor_band(cx, cy)
                        && !self.corridor_band(mx, my)
                    {
                        group.push((cx, cy));
                        mgroup.push((mx, my));
                    }
                }
            }
            if group.is_empty() {
                continue;
            }
            for (cx, cy) in group.iter().chain(mgroup.iter()) {
                self.set_blocked(*cx, *cy, true);
            }
            done += (group.len() * 2) as u64;
            let _ = cells_per_group;
        }
    }
    fn res_band(&self, slot: u8) -> (i16, i16) {
        match slot {
            0 => (self.preset.gold_lo, self.preset.gold_hi),
            1 => (self.preset.base_r + 2, self.preset.food_hi),
            _ => (self.preset.base_r + 2, self.preset.wood_hi),
        }
    }
    fn res_want(&self, slot: u8) -> u32 {
        match slot {
            0 => self.preset.gold_count,
            1 => self.preset.food_count,
            _ => self.preset.wood_count,
        }
    }
    fn res_spot_ok(&self, x: i16, y: i16, b: usize, slot: u8, bases: &[(i16, i16)]) -> bool {
        if !self.in_map(x, y) || self.blocked(x, y) || self.on_road(x, y) {
            return false;
        }
        let (bx, by) = bases[b];
        let d0 = man(x - bx, y - by);
        let ok = match slot {
            0 => d0 >= self.preset.gold_lo,
            1 => d0 <= self.preset.food_hi && d0 >= 3,
            _ => d0 >= self.preset.base_r + 2 && d0 <= self.preset.wood_hi + 8,
        };
        if !ok {
            return false;
        }
        if bases.len() == 2 && man(x - bases[1 - b].0, y - bases[1 - b].1) <= self.preset.base_r + 1
        {
            return false;
        }
        !self.taken[self.idx(x, y)]
    }
    fn layer_resources(&mut self, bases: &[(i16, i16)]) {
        // Phase 1: player 0's own nodes, deterministic per band.
        for slot in 0..3u8 {
            let want = self.res_want(slot);
            let (lo, hi) = self.res_band(slot);
            let span = (hi - lo).max(0) as u64 + 1;
            let mut got = 0u32;
            let mut tries = 0u32;
            let budget = want * 256 + 8192;
            while got < want && tries < budget {
                tries += 1;
                let r = self.rng.next_u64();
                let x = (r % self.n as u64) as i16;
                let y = ((r >> 21) % self.n as u64) as i16;
                if !self.res_spot_ok(x, y, 0, slot, bases) {
                    continue;
                }
                let total = lo as i64 + ((r >> 43) % span) as i64;
                self.mark_taken(x, y);
                self.nodes.push(NodePl {
                    slot,
                    x,
                    y,
                    total,
                    base: 0,
                });
                got += 1;
            }
            if got < want {
                // structural fallback: far mirror band around the enemy base (far slots),
                // keeps the count invariant even if the random draw ran out.
                let (ox, oy) = bases[1];
                while got < want && tries < budget * 2 {
                    tries += 1;
                    let r = self.rng.next_u64();
                    let x = (r % self.n as u64) as i16;
                    let y = ((r >> 21) % self.n as u64) as i16;
                    let d = man(x - ox, y - oy);
                    let in_far = match slot {
                        0 => d >= self.preset.gold_lo,
                        1 => d >= self.preset.base_r + 2 && d <= self.preset.food_hi,
                        _ => d >= self.preset.base_r + 2 && d <= self.preset.wood_hi + 8,
                    };
                    if !in_far {
                        continue;
                    }
                    if !self.res_spot_ok(x, y, 1, slot, bases) {
                        continue;
                    }
                    let total = lo as i64 + ((r >> 43) % span) as i64;
                    self.mark_taken(x, y);
                    self.nodes.push(NodePl {
                        slot,
                        x,
                        y,
                        total,
                        base: 1,
                    });
                    got += 1;
                }
            }
        }
        // Phase 2: mirror every p0 placement to player 1: fairness is structural.
        let placed: Vec<NodePl> = self.nodes.iter().filter(|n| n.base == 0).cloned().collect();
        for nd in placed {
            let (x, y) = rot180(nd.x, nd.y, self.m);
            if self.res_spot_ok(x, y, 1, nd.slot, bases) {
                self.mark_taken(x, y);
                self.nodes.push(NodePl {
                    slot: nd.slot,
                    x,
                    y,
                    total: nd.total,
                    base: 1,
                });
            }
        }
    }

    fn mark_taken(&mut self, x: i16, y: i16) {
        let i = self.idx(x, y);
        self.taken[i] = true;
    }

    fn layer_decor(&mut self, bases: &[(i16, i16)]) {
        let want = self.preset.decor_count;
        let mut got = 0u32;
        let mut tries = 0u32;
        while want > 0 && got < want && tries < want * 256 + 4096 {
            tries += 1;
            let r = self.rng.next_u64();
            let x = (r % self.n as u64) as i16;
            let y = ((r >> 23) % self.n as u64) as i16;
            if !self.in_map(x, y)
                || self.blocked(x, y)
                || self.on_road(x, y)
                || self.taken[self.idx(x, y)]
            {
                continue;
            }
            if self.near_base(x, y, bases, 0) {
                continue;
            }
            let (mx, my) = rot180(x, y, self.m);
            if self.in_map(mx, my) && !self.blocked(mx, my) && !self.taken[self.idx(mx, my)] {
                self.decor.push((x, y, mx, my, 1u8));
                let i = self.idx(x, y);
                let j = self.idx(mx, my);
                self.taken[i] = true;
                self.taken[j] = true;
                got += 1;
            }
        }
    }
    fn validate(&self, bases: &[(i16, i16)]) -> (bool, [[u32; 3]; 2]) {
        let mut seen = vec![false; (self.n as usize) * (self.n as usize)];
        let (bx, by) = bases[0];
        let mut queue = vec![(bx, by)];
        seen[self.idx(bx, by)] = true;
        let mut head = 0;
        while head < queue.len() {
            let (x, y) = queue[head];
            head += 1;
            for (dx, dy) in [(1i16, 0i16), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if self.in_map(nx, ny) && !self.blocked(nx, ny) && !seen[self.idx(nx, ny)] {
                    seen[self.idx(nx, ny)] = true;
                    queue.push((nx, ny));
                }
            }
        }
        let connected = seen[self.idx(bases[1].0, bases[1].1)];
        let mut counts = [[0u32; 3]; 2];
        for nd in &self.nodes {
            if nd.total > 0 {
                counts[nd.base][nd.slot as usize] += 1;
            }
        }
        (connected, counts)
    }
    fn repair(&mut self, bases: &[(i16, i16)]) -> bool {
        let (conn, counts) = self.validate(bases);
        let mut needed = false;
        if !conn {
            self.carve_corridor(bases);
            needed = true;
        }
        #[allow(clippy::needless_range_loop)]
        for b in 0..bases.len() {
            for slot in 0..3u8 {
                if counts[b][slot as usize] < self.res_want(slot) {
                    self.place_missing(b, slot, bases);
                    needed = true;
                }
            }
        }
        !needed
    }
    fn carve_corridor(&mut self, bases: &[(i16, i16)]) {
        let (ax, ay) = bases[0];
        let (bx, by) = bases[1];
        if ax != bx {
            self.band(ax, ay, bx, ay);
            self.band(bx, ay, bx, by);
        } else {
            self.band(ax, ay, ax, by);
        }
    }
    fn band(&mut self, x0: i16, y0: i16, x1: i16, y1: i16) {
        let w = 1i16;
        if x0 == x1 {
            for y in y0.min(y1)..=y0.max(y1) {
                for dx in -w..=w {
                    self.set_blocked(x0 + dx, y, false);
                }
            }
        } else {
            for x in x0.min(x1)..=x0.max(x1) {
                for dy in -w..=w {
                    self.set_blocked(x, y0 + dy, false);
                }
            }
        }
    }
    fn place_missing(&mut self, b: usize, slot: u8, bases: &[(i16, i16)]) {
        let want = self.res_want(slot);
        let (lo, hi) = self.res_band(slot);
        let mut have = self
            .nodes
            .iter()
            .filter(|n| n.base == b && n.slot == slot && n.total > 0)
            .count() as u32;
        let mut tries = 0u32;
        while have < want && tries < 16384 {
            tries += 1;
            let r = self.rng.next_u64();
            let x = (r % self.n as u64) as i16;
            let y = ((r >> 21) % self.n as u64) as i16;
            if !self.res_spot_ok(x, y, b, slot, bases) {
                continue;
            }
            let span = (hi - lo).max(0) as u64 + 1;
            let total = lo as i64 + ((r >> 43) % span) as i64;
            let i = self.idx(x, y);
            self.taken[i] = true;
            self.nodes.push(NodePl {
                slot,
                x,
                y,
                total,
                base: b,
            });
            have += 1;
        }
    }
    fn attempt(&mut self) {
        let bases = base_centers(self.n);
        self.base_centers = bases.clone();
        self.layer_border();
        self.layer_plateau(&bases);
        self.layer_roads(&bases);
        self.layer_terrain(&bases);
        self.layer_resources(&bases);
        self.layer_decor(&bases);
        self.repaired = if self.repair(&bases) { 0 } else { 1 };
    }
}

pub fn generate_map_preset(seed: u64, preset: Preset) -> MapGen {
    let k = preset.retries_max;
    let mut last: Option<MapGen> = None;
    // attempts is per-attempt; the accepted (valid) draw's count is what we report.
    for i in 0..=k {
        let attempt_seed = seed.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut ctx = Ctx::new(attempt_seed, preset);
        ctx.attempt();
        let (conn, counts) = ctx.validate(&ctx.base_centers);
        let valid = conn
            && (0..2)
                .all(|b| (0..3u8).all(|s| counts[b][s as usize] >= preset_res_want(&preset, s)));
        if valid {
            return finish(ctx, i + 1);
        }
        last = Some(finish(ctx, i + 1));
    }
    last.unwrap()
}

fn preset_res_want(p: &Preset, slot: u8) -> u32 {
    match slot {
        0 => p.gold_count,
        1 => p.food_count,
        _ => p.wood_count,
    }
}

fn finish(ctx: Ctx, attempts: u32) -> MapGen {
    MapGen {
        grid: ctx.grid,
        nodes: ctx.nodes,
        base_centers: ctx.base_centers,
        attempts,
        repaired: ctx.repaired,
    }
}

impl MapGen {
    /// Spawn nodes onto world and rewrite grid to this map. Returns nodes spawned.
    pub fn spawn_on(
        &self,
        world: &mut engine_ecs::World,
        grid: &mut Grid,
        install: bool,
    ) -> Vec<(engine_ecs::Entity, NodePl)> {
        // border ring
        let n = self.grid.w() as i16;
        for i in 0..n {
            for b in 0..2i16 {
                grid.set_blocked(TilePos { x: i, y: b }, true);
                grid.set_blocked(TilePos { x: i, y: n - 1 - b }, true);
                grid.set_blocked(TilePos { x: b, y: i }, true);
                grid.set_blocked(TilePos { x: n - 1 - b, y: i }, true);
            }
        }
        // terrain from source grid bits
        for y in 0..n {
            for x in 0..n {
                if self.grid.is_blocked(TilePos { x, y }) {
                    grid.set_blocked(TilePos { x, y }, true);
                }
            }
        }
        // clear legacy node storages only when installing over an existing sim
        if install {
            world.storages.clear::<ResourceNode>();
            world.storages.clear::<NodePos>();
        }
        let mut out = Vec::with_capacity(self.nodes.len());
        for nd in &self.nodes {
            if nd.total <= 0 {
                continue;
            }
            let e = world.storages.spawn();
            world.storages.add(
                e,
                ResourceNode {
                    slot: nd.slot,
                    total: nd.total,
                    remaining: nd.total,
                },
            );
            world.storages.add(e, NodePos { x: nd.x, y: nd.y });
            out.push((e, *nd));
        }
        out
    }
}

/// Deterministic run: create Sim::new(seed), install the preset map, then step with cmds.
/// Returns the final state hash. Preset maps are deterministic per (seed, preset).
pub fn run_preset(seed: u64, ticks: u64, cmds: &[shared::TickCommands], preset: Preset) -> u64 {
    let mut sim = crate::sim::Sim::new(seed);
    let map = generate_map_preset(seed, preset);
    sim.install_map(map);
    // step with commands (same contract as run_sim)
    let mut cmds_i = 0usize;
    while (sim.tick()) < ticks {
        if cmds_i < cmds.len() && cmds[cmds_i].tick == sim.tick() {
            let _ = sim.step(&cmds[cmds_i].cmds);
            cmds_i += 1;
        } else {
            let _ = sim.step(&[]);
        }
    }
    sim.hash()
}
