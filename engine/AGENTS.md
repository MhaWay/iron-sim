# AGENTS.md — Engine (crates/*)

## Missione
Engine deterministico in Rust per RTS + colony sim. Niente Unreal: qui serve
lockstep bit-esatto e pathfinding riproducibile.

## Ambientamento
```bash
source /root/.cargo/env
cd /root/Projects/iron-sim/engine
cargo test            # stable 1.99.1, tutto green
cargo fmt
cargo clippy -- -D warnings
```

## Regole d'oro (CI-enforced)
1. NO `f32`/`f64` in sim code. Fixed-point (`I64F64`/`Fixed1616`).
2. NO `HashMap` default hasher in sim. `BTreeMap`/`Vec` ordinati.
3. NO RNG globali/nondeterministiche. `SplitMix64` con seed esplicito.
4. Loop a tick fisso. Input sempre ordinati.
5. Nessun `thread_local`/global state nondeterministico nella sim.

## Struttura attuale
- crates/core  fixed-point, RNG, scheduler, replay, hash (fnv fold)
- crates/ecs   sparse-set deterministica, iter per entity-id, scheduler systems
- crates/path  A* + tie-break totale, riproducibile
- crates/sim   placeholder (member futuro per sim condivise)

## Roadmap (NON ora)
- Multicore deterministico: chunk per id, reduce associativa. [docs/parallelism.md](../docs/parallelism.md)
- GPU compute (wgpu) per flow field / needs. [docs/gpu.md](../docs/gpu.md)
- MCP server (crate mcp): agent può pilotare la sim. [docs/mcp-server.md](../docs/mcp-server.md)
- AI inference on GPU (Fase 4, futura): [docs/roadmap.md](../docs/roadmap.md).

## Test richiesti
- determinism: 2 run uguali -> stesso hash.
- path replay: stesso input -> stesso path.
- non usare sleep/tempo reale nei test di sim.
- golden hash + bump SIM_VERSION.

## Style
- cargo fmt, clippy -D warnings, commit convenzionali (feat/fix/chore/test).
- PR piccole; spiegare il *perché* del determinismo in ogni modifica.
