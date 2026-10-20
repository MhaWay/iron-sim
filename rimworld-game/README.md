# rimworld-game

A deterministic colony sim (RimWorld-like): pawns with needs, autonomous job assignment,
built on the iron-sim engine (lockstep, ECS, fixed-point, pathfinding).

## Game pitch

You run a small colony. Pawns have needs (hunger and energy) that drain every tick. Idle pawns
autonomously claim the nearest job — hauling food to storage, or sleeping on a bed. If hunger or
energy hits zero, a pawn collapses (HP drains until revived at a bed). The whole simulation is a
pure function of (seed, command stream) so replays and lockstep multiplayer work: two runs of the
same command log end at the same state hash.

## Layout

```
crates/
  shared/    Command / ColonyEvent types + canonical FNV-1a-64 state hasher
  sim/       ColonySim: pawns, needs, jobs, pathfinding; SIM_VERSION + determinism tests
  server/    lockstep relay: sorts + forwards tick batches, peers-agree check
  client/    Renderer trait stub (wgpu/soft later) + MockRenderer
docs/
  engine-api.md   assumed engine contract (../engine)
```

## How to run

```bash
source /root/.cargo/env
cargo build --workspace

# Headless sim demo (300 ticks), then a deterministic replay check
cargo run -p sim --bin simdemo -- --seed 42 --ticks 300

# Server relay demo
cargo run -p server --bin serverdemo -- --seed 42
```

## Dev loop

```bash
source /root/.cargo/env
cargo test --workspace          # determinism + unit tests
cargo clippy --workspace        # keep it clean
```

## Determinism contract

- 30 Hz fixed tick; sim code is single-threaded; Q16.16 fixed-point only (no floats in sim).
- `Sim::step` sorts incoming commands canonically before applying — transport order is irrelevant.
- State hash folds `SIM_VERSION` first; bump it when sim math changes (golden tests remind you).
- Full contract: `docs/engine-api.md` and `AGENTS.md`.
