# Iron Sim — Monorepo

Due giochi e un engine proprietario, senza Unreal.
Decisione chiave: l'engine deve essere deterministico (lockstep + A* riproducibile).

## Layout

- engine/          — engine core (Rust, workspace)
- rts-game/        — RTS game (client + server, usa engine)
- rimworld-game/   — Colony sim (usa engine)
- docs/            — cross-cutting: architecture, determinism, networking

## Regola d'oro

Determinismo in primis: niente float, niente HashMap, niente RNG globali.
Vedi docs/determinism.md e i vari AGENTS.md.
