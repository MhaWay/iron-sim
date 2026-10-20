# Engine API (assumed contract — rimworld-game)

The engine workspace lives at `../engine` (path deps: `engine-core`, `engine-ecs`, `engine-path`).
These crates are under concurrent development. THIS GAME COMPILES AGAINST A STUB AND MUST KEEP
WORKING WHILE THE ENGINE IS SWAPPED IN. If you change anything below, fix every crate and bump
`SIM_VERSION` in `crates/sim`.

## engine_core

```rust
pub type Tick = u64;

/// SplitMix64 RNG. Same seed + same call sequence = same stream everywhere.
pub struct Rng { pub fn new(seed: u64) -> Self; pub fn next_u64(&mut self) -> u64; pub fn state(&self) -> u64 }

/// Q16.16 fixed point. The ONLY number type allowed in sim code. No f32/f64 anywhere in the sim.
pub struct Fixed1616(pub i64);
impl Fixed1616 {
    pub fn from_int(i: i32) -> Self;              // 1 == 1<<16
    pub fn from_rational(n: i64, d: i64) -> Self; // Euclidean div_floor(n<<16, d)
    pub fn to_int_trunc(self) -> i32;             // trunc toward zero
    pub fn wrapping_add / wrapping_sub / wrapping_mul((a*b)>>16) / wrapping_div(div_floor(a<<16,b))
    pub fn wrapping_abs / saturating_add(self, rhs) -> Self
    pub fn is_zero(self) -> bool
    pub fn inner(self) -> i64
}
// ops: Add / Sub / Mul / Neg / AddAssign; fn div_floor(a,b) -> i64 (Euclidean)
```

## engine_ecs

```rust
pub struct Entity(pub u32);
pub struct Storages {
    pub fn spawn(&mut self) -> Entity;
    pub fn add<T: 'static>(&mut self, e: Entity, v: T);
    pub fn get<T: 'static>(&self, e: Entity) -> Option<&T>;
    pub fn get_mut<T: 'static>(&mut self, e: Entity) -> Option<&mut T>;
    pub fn remove<T: 'static>(&mut self, e: Entity);
    pub fn despawn(&mut self, e: Entity);
    /// CRITICAL: yields (Entity, &T) in ASCENDING ENTITY INDEX order — the canonical iteration
    /// order for all sim systems and hashing.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)>;
    pub fn entities(&self) -> impl Iterator<Item = Entity>;
}
pub struct World { pub storages: Storages, pub rng: Rng, pub tick: engine_core::Tick }
impl World { pub fn new(seed: u64) -> Self }

pub struct Scheduler {
    pub fn new() -> Self;
    pub fn add_system<S: Into<&'static str>, F: FnMut(&mut World, Tick) + 'static>(&mut self, name: S, f: F);
    pub fn step(&mut self, world: &mut World, tick: Tick); // runs systems in registration order
```

## engine_path

```rust
pub struct TilePos { pub x: i16, pub y: i16 }   // NAMED fields
pub struct Grid { pub fn new(w: u16, h: u16); set_blocked; is_blocked; contains; w(); h() }
/// 4-connected A*, deterministic neighbor order N,E,S,W; returns Vec<TilePos> from start+1 to goal.
pub fn find_path(grid: &Grid, start: TilePos, goal: TilePos) -> Option<Vec<TilePos>>;
```

## NOT assumed (out of engine scope)

Rendering, networking, input, and timing live entirely in the game repos.

## Determinism contract (binding for all game code)

1. Single-threaded. No threads, no atomics, no time-of-day in sim logic.
2. Fixed tick rate: 30 Hz (`TICK_HZ`). Sim advances tick-by-tick; the host drives the clock.
3. No `f32`/`f64` in sim code — Q16.16 `Fixed1616` only. Client view types may use floats.
4. No `std::collections::HashMap` with the default RandomState hasher in sim code — use `fnv::FnvHashMap`
   or BTreeMap.
5. RNG: only `engine_core::Rng` (SplitMix64), explicit seed, never global.
6. Iteration order: ECS `iter<T>()` (entity index ascending) or Vec order. Never HashMap order.
7. Canonical command sorting: `Sim::step` sorts its raw input batch before applying — the transport order
   never matters.
8. State hash: FNV-1a 64-bit, little-endian words, canonical field order; `SIM_VERSION` folded in first.
   Bump `SIM_VERSION` deliberately whenever sim math changes.
9. No `sleep()`/`instant::now()` in tests.
