# iron-sim engine

Deterministic Rust engine powering two games (RTS + colony sim). Everything is built around a
single hard requirement: **determinism** — lockstep netcode and bit-exact replays must produce
the same state on every machine. That is why the engine is not on Unreal.

## Quickstart

```sh
source /root/.cargo/env
cd engine
cargo test --workspace          # all crates
cargo build --workspace
# CI-style gate (fmt + clippy + tests):
make check   # or: just check
```

Toolchain: stable Rust 1.99.1. No f32/f64 anywhere (grep gate), no default-hash HashMap.

## Crates

| Crate | Purpose |
|---|---|
| `engine-core` | Fixed-point math (Q16.16), SplitMix64 RNG, tick scheduler, Command/Event, ReplayRecorder, state hashing |
| `engine-ecs` | Minimal sparse-set ECS: `World`, `Entity(u32)`, component storages, registration-ordered systems; iteration always ascending by entity index (BTreeMap + sorted Vec, never HashMap) |
| `engine-path` | Deterministic A* over a grid: cost-then-y-major-index tie-breaking, 4-connected neighbors |
| `engine-sim` | Placeholder for `sim_rts` / `sim_colony` binaries (later) |

## Determinism contract (AGENTS.md)

1. **No floating-point types.** All geometry/physics math is `Fixed1616` (Q16.16, integer ops only).
   Floating point is forbidden engine-wide (see `AGENTS.md` rule 1) — reason: bit-exact cross-platform lockstep.
2. **No random-state hashing.** `BTreeMap` / sorted Vecs everywhere; `HashMap` with its default
   SipRandom state hasher is banned (see `AGENTS.md` rule 2).
3. **No global RNG, no thread-local nondeterminism.** Single-threaded semantics by design.
4. **Fixed tick loop.** `engine_core::Scheduler` consumes commands at their target tick, ordered
   by (tick, insertion sequence) — replays match across processes.

## Core API (exact signatures)

```rust
// Fixed-point, Q16.16, integer-only
pub struct Fixed1616(pub i64);
impl Fixed1616 {
    pub const ONE: Fixed1616; pub const ZERO: Fixed1616;
    pub fn from_int(i: i32) -> Self;               // i << 16
    pub fn from_rational(n: i64, d: i64) -> Self;  // floor(n/d * 2^16), Euclidean
    pub fn to_int_trunc(self) -> i32;
    pub fn wrapping_add/sub(self, rhs) -> Self;
    pub fn wrapping_mul(self, rhs) -> Self;        // (lhs.0 * rhs.0) >> 16 via i128
    pub fn wrapping_div(self, rhs) -> Self;        // div_floor(lhs.0 * S, rhs.0)
    pub fn wrapping_abs(self) -> Self; pub fn saturating_add(self, rhs) -> Self;
    pub fn is_zero(self) -> bool; pub fn inner(self) -> i64;
    pub fn mul_floor(self, rhs) -> Self;           // = wrapping_mul (ops::Mul is canonical)
}
impl Add/Sub/Mul/Neg/AddAssign for Fixed1616;      // ops defined in core::ops

pub fn fixed_sqrt(v: Fixed1616) -> Fixed1616;      // floor sqrt of Q16.16 value
pub fn isqrt_u128(n: u128) -> u128;                // floor(sqrt(n)), radix-4 bit-pair algo

// RNG — SplitMix64, seedable, no global state
pub struct Rng; impl Rng { pub fn new(seed: u64) -> Self; pub fn next_u64(&mut self) -> u64; pub fn state(&self) -> u64; }

// Scheduler — fixed-step lockstep driver
pub struct Scheduler {
    pub fn new() -> Self;
    pub fn enqueue(&mut self, at_tick: Tick, cmd: Command);
    pub fn step<F: FnMut(Tick, &Command)>(&mut self, next: Tick, sink: F);
    pub fn current(&self) -> Tick;
}

// Commands / events
pub struct Command { pub payload_hash: u64 }       // Command::from_hash(h), cmd.hash()
pub struct Event { pub tick: Tick, pub id: u64, pub payload_hash: u64 }

// Replay
pub struct ReplayRecorder {
    pub fn record(&mut self, tick: Tick, payload_hash: u64);
    pub fn digest(&self) -> u64;                   // stream-order-sensitive checksum
    pub fn tick_digest(tick: Tick, hashes: &[u64]) -> u64;
}
pub fn hash_fold(acc: u64, word: u64) -> u64;      // ((acc^w).wrapping_mul(0x100001b3)).rotl(11)
pub fn hash_words(words: &[u64]) -> u64;
```

## Pathfinding API (exact signature)

```rust
pub struct TilePos { pub x: i16, pub y: i16 }
pub struct Grid;                                   // new(w: u16, h: u16)
impl Grid { pub fn set_blocked(&mut self, p: TilePos, blocked: bool);
            pub fn is_blocked(&self, p: TilePos) -> bool;
            pub fn contains(&self, p: TilePos) -> bool; }

pub fn find_path(grid: &Grid, start: TilePos, goal: TilePos) -> Option<Vec<TilePos>>;
```

Deterministic A*: 4-connected neighbors in N,E,S,W order; open-set tie-break is
(f-cost, then y-major cell index); Manhattan heuristic in fixed-point.

## Tests

- `crates/core/src/lib.rs`: 8 unit tests (rng streams, fixed math, isqrt vs reference, scheduler ordering, replay digests).
- `crates/core/tests/determinism.rs`: 10 000 ticks run twice, identical state hash + replay digest.
- `crates/ecs/src/lib.rs`: 2 tests (iter order, system registration order).
- `crates/path/src/lib.rs`: 5 tests (determinism of same-input paths, blocked wall, exact straight path, no diagonal shortcut, start==goal).

## Tooling

`Makefile` / `justfile`: `build`, `test`, `fmt`, `clippy`, `lint` (grep gate forbidding `f32`/`f64` tokens
and `HashMap` — the authoritative float/hash ban; unstable clippy float lints are NOT declared in
`Cargo.toml` because they error as unknown_lints on stable), `check` = fmt + clippy + test.

## Roadmap (future — not yet built)

1. Multicore work-stealing parallelism with bit-exact deterministic reduction (order-independent hash-combine) and staged barriers.
2. Deterministic GPU compute (compute shaders) where it helps.
3. MCP server so agents can introspect/drive/inspect the engine.
4. GPU neural-net inference for the colony sim AI (video-card resident).

## Deep dive

Architecture details and design notes live in `docs/`. See `AGENTS.md` for the binding determinism rules.
