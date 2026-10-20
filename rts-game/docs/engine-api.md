# Assumed Engine API

This repo path-depends on the iron-sim engine workspace:

engine-core = { path = "../engine/crates/core" }
engine-ecs  = { path = "../engine/crates/ecs" }
engine-path = { path = "../engine/crates/path" }

The engine is developed concurrently. This file is the CONTRACT: our code compiles against
exactly this surface and nothing else. While engine crates are minimal stubs, everything
works end-to-end; when the engine team replaces internals, this surface must keep holding.

## engine-core
- pub type Tick = u64;
- pub struct Rng — SplitMix64. Rng::new(seed), .next_u64() -> u64, .state() -> u64.
  Determinism rule: same seed + same call order == same stream. No global RNG.
- pub struct Fixed1616(pub i64) — Q16.16 fixed point. NO f32/f64 in sim code.
  - const ONE, const ZERO
  - from_int(i32), from_rational(n, d) [floor], to_int_trunc() -> i32
  - wrapping_add / wrapping_sub / wrapping_mul (>>16) / wrapping_div / wrapping_abs / saturating_add
  - is_zero()
  - ops: +, -, * (wrapping semantics)

## engine-ecs
- pub struct Entity(pub u32) — Clone/Copy/Ord/Hash.
- pub struct Storages:
  - new(), spawn() -> Entity, add<T>(e, v), get<T>(e) -> Option<&T>, get_mut<T>(e),
    remove<T>(e), iter<T>() -> Iterator<(Entity, &T)> IN ENTITY-INDEX ASCENDING ORDER
    (the determinism-critical primitive), contains<T>() -> bool, despawn(e).
- pub struct World { pub storages: Storages, pub rng: engine_core::Rng, pub tick: Tick }
- pub struct Scheduler { new(), add_system(name, F), step(&mut World, Tick) }
  Systems run in registration order, once per step, single-threaded.

## engine-path
- pub struct TilePos { pub x: i16, pub y: i16 }
- pub struct Grid { new(w, h), set_blocked(p, b), is_blocked(p), contains(p) }
- pub fn find_path(grid, start, goal) -> Option<Vec<TilePos>>
  4-connected A*, fixed neighbor order N,E,S,W, total tie-break. Same input -> same path.

## NOT assumed
Rendering, net, input, audio: no engine API. Our client/server crates define their own traits.

## Determinism rules (apply to every sim crate)
1. No f32/f64. No HashMap default hasher in sim code. No global/static mutable RNG.
2. No threads or real time inside the simulation; sim time = Tick, fixed rate.
3. Iteration must be order-stable (BTreeMap / Vec / ecs.iter entity order) — never hash-order.
4. All state hashes use fnv (explicit) over canonical little-endian bytes.
5. When you change sim math: bump SIM_VERSION and update the golden hash deliberately.
