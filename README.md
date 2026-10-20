# Iron Sim — Monorepo

Due giochi + engine proprietario. Decisione chiave: determinismo lockstep e
A* riproducibile (motivo per cui non usiamo Unreal).

## Layout
- engine/          — engine core (Rust workspace)
- rts-game/        — RTS lockstep (usa engine)
- rimworld-game/   — Colony sim (usa engine)
- docs/            — architettura, regole, roadmap, MCP, parallelism, gpu

## Regola d'oro
Determinismo in primis: niente float in sim, niente HashMap non ordinate,
niente RNG globali. Vedi [docs/determinism.md](docs/determinism.md).

Requisiti chiave:
1. Determinismo bit-esatto. Vedi [docs/determinism.md](docs/determinism.md).
2. Multicore + GPU (futura, NON ora): [docs/parallelism.md](docs/parallelism.md),
   [docs/gpu.md](docs/gpu.md).
3. MCP server per pilotare l'engine da agent: [docs/mcp-server.md](docs/mcp-server.md).
4. AI su GPU per colony sim (molto futura): [docs/roadmap.md](docs/roadmap.md) Fase 4.

## Test
```bash
source /root/.cargo/env
cd engine && cargo test --workspace            # 17 green
cd ../rts-game && cargo test --workspace       # 12 green
cd ../rimworld-game && cargo test --workspace  # 9 green
```
`cargo fmt --check` e `cargo clippy --all-targets -D warnings` puliti.

## Note
- Ogni modifica alla sim deve mantenere il golden hash e bumpare SIM_VERSION.
- Il rendering può usare float, ma la sim no.
- La GPU (futura) non può toccare RNG né ordine eventi.
