# rts-game

Deterministic lockstep RTS on the iron-sim engine (engine workspace at ../engine).

## Pitch
Small-scale space RTS: squad-sized fleets, position/velocity orders, simple range combat.
The game IS a command log: two identical command streams always evolve byte-identical state,
which makes lockstep netcode, replays and desync forensics trivial. No float math, no
hash-order iteration, no hidden randomness.

## Layout (Cargo workspace)
- crates/shared — Command / event types, TickCommands, canonical state hasher (fnv)
- crates/game   — deterministic simulation: fixed-point movement + A* orders + combat, tick loop
                  driven by the engine Scheduler
- crates/server — authoritative lockstep relay: collects commands per tick, emits batches,
                  checks peer state-hash agreement
- crates/client — stub Renderer trait + mock renderer (wgpu renderer lands later)

## How to run
```sh
source /root/.cargo/env
cargo build --workspace
cargo test  --workspace            # determinism: identical command logs -> identical hashes
cargo run   -p server -- --seed 7 --ticks 600   # headless lockstep relay demo
cargo run   -p client -- --ticks 600            # drives the renderer stub
```

## Dev loop
1. Edit sim code in crates/game (or shared command types).
2. Keep crates/game/tests/determinism.rs green.
3. If you intentionally change sim math: bump SIM_VERSION in crates/game/src/lib.rs AND refresh
   the golden hash printed by tests::golden_hash_check — deliberately, never accidentally.
4. cargo clippy -D warnings before pushing.

## Determinism contract (hard rules)
- Fixed 30 Hz tick. Single-threaded. Q16.16 fixed-point only. NO f32/f64, NO HashMap default
  hasher, NO global RNG, NO threads, NO real time in sim code.
- state(t+1) = f(state(t), cmds(t)); cmds are the only input (Command::Move / Attack / Stop).
- Unit movement: A* path to ordered tile, then fixed-step movement along waypoints (speed-scaled).
- Combat: each tick, units with Attack orders damage the nearest enemy in range (range in tiles).
- run_sim(seed, ticks, cmds) returns final StateHash; the test asserts hash(run) == hash(replay).

See docs/engine-api.md for the exact engine surface assumed.
