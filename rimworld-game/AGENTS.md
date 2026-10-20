# AGENTS.md — Colony Sim (RimWorld-like)

## Missione
Colonie con pawns, bisogni, lavoro. Sim deterministica sopra l'engine.

## Ambientamento
```bash
source /root/.cargo/env
cd /root/Projects/iron-sim/rimworld-game
cargo test
```

## Architettura
- crates/shared  Command + hash canonico.
- crates/sim     ColonySim: needs_system (hunger/energy/mood, Eat/Sleep/Work),
                 job_system (need->claim, idle->job, chiavi totali (dist, entity_id)),
                 movement_system (dominant-axis Q16.16).
- crates/server  relay lockstep.
- crates/client  stub renderer.
- docs/engine-api.md: contract surface.

## Regole d'oro (determinismo)
1. NO f32/f64 in sim. Fixed-point.
2. NO HashMap default in sim (needs, inventari, job queue).
3. Job assignment deterministico: chiavi totali (score, pawn_id) / (dist, entity_id).
4. RNG solo SplitMix64 seedato.
5. Tick fisso.

## Test richiesti
- needs + jobs deterministici.
- replay: stesso seed + input -> stesso stato.

## Roadmap (NON ora)
- Multicore + GPU + MCP: [../docs/parallelism.md](../docs/parallelism.md),
  [../docs/gpu.md](../docs/gpu.md), [../docs/mcp-server.md](../docs/mcp-server.md).
- AI su GPU (Futuro): [../docs/roadmap.md](../docs/roadmap.md) Fase 4.
