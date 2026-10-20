# AGENTS.md — RTS Game

## Missione
RTS real-time in lockstep deterministico, sopra l'engine (path ../engine).

## Ambientamento
```bash
source /root/.cargo/env
cd /root/Projects/iron-sim/rts-game
cargo test
```

## Architettura
- crates/shared  Command + FNV-1a hash canonico + sort totale.
- crates/game    sim lockstep (tick, unità, movimento, combattimento) + state hash.
- crates/server  relay lockstep + demo bin.
- crates/client  renderer trait stub (f64 ok in view, NON in sim).
- docs/engine-api.md: contract surface engine.

## Regole d'oro (determinismo)
1. NO f32/f64 nella sim. Fixed-point.
2. NO HashMap default in sim. Ordina sempre.
3. Sim single-thread deterministica; rendering anche float, ma non toccare la sim.
4. Input ordinati per (tick, player, entity).
5. Golden hash + bump SIM_VERSION.

## Test richiesti
- replay: stesso log input -> stesso hash finale.
- niente dipendenze casuali nei test.

## Roadmap (NON ora)
- Multicore / GPU / MCP / AI su GPU: [../docs/parallelism.md](../docs/parallelism.md),
  [../docs/gpu.md](../docs/gpu.md), [../docs/mcp-server.md](../docs/mcp-server.md),
  [../docs/roadmap.md](../docs/roadmap.md).
- Assunzioni engine API: vedi docs/engine-api.md. Se cambia, aggiorna.
