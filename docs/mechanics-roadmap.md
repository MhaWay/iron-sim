# RTS long-match mechanics roadmap

Status: design only — no code yet. Ordered so the match stays deterministic and testable.
Current facts: 30 Hz fixed tick, 128×128 map, measured ~100k ticks/s single-thread (bench, 10k-tick stream)
=> ~14 ms/tick at 100× headroom. 3h match = 324k ticks, no wall-clock issue; replay/commands grow linearly (see §5).

## Tier A — makes long battles viable (do first)

1. **Sustain: unit upkeep / supply**
   Training + per-unit upkeep (tick cost scaled by tier). Kills the "spam once then 3 hours of stale
   board" failure mode. Forces economy to keep expanding = longer, more meaningful matches.
2. **Territory + regrowth**
   Buildings project territory; resource nodes regen in bounded waves (seeded, deterministic).
   Prevents the "map fully harvested, both sides stall forever" death spiral that long matches hit.
3. **Reclaim / salvage**
   Corpses and wreckage: dismantle for partial refund, reclaim territory (like the wall-debris idea
   from v2, extended). Gives the losing side comebacks and late-game attrition meaning.
4. **Siege and ranged pressure**
   Attack-speed/damage falloff vs walls (walls strong, siege units bonus vs buildings), healing between
   waves. Makes "continuous battles" a design mode rather than spam.
5. **Replay + time controls**
   Save/load state snapshots (state hash + command log), fast-forward, and a "sudden death" escalation
   curve at MATCH_TICKS. For an hours-long game, replay scrubbing + late-game pressure is mandatory.

## Tier B — depth (after Tier A lands green)

6. **Heroes**: persistent units with levels/XPs, respawn timer. One hero per side per civ (the MOBA-like
   Cult of Vhorr direction already sketched in docs/design-factions.md).
7. **Fog-of-war + scouting units + line-of-sight events.**
8. **Diplomacy/neutral structures**: capture points, mercenaries (hire cost, upkeep).
9. **Wonder/hold-the-point victory variant** (VICTORY_WONDER_HOLD is already in sim.rs) — extend
   victory conditions for longer matches beyond TC-destruction or 36k score.

## Tier C — long-match infrastructure

10. **Replay files**: ring buffer + compressed command log (bit-pack Command variants).
    3h × 10 cmds/tick ≈ ~10 MB raw, ~1-3 MB compressed. Deterministic digest per minute.
11. **Perf**: if 500+ units appear per side, A* and path-cache become hot; use engine-path grid + the
    measured headroom first, defer multicore/GPU (roadmap parked).
12. **Tick-rate tuning**: long matches might want 20 Hz (fewer ticks to replay, same wall-clock); keep
    30 Hz default, make it per-map config later.

## Non-goals for now

- Dynamic physics, scripting engine, mod system (out of scope for a deterministic demo slice).
- 3D/heightfield (colony sim's heightfield is a separate track).

## Acceptance test pattern (per mechanic)

New mechanics land behind Command variants (deterministic, canonical-sortable) + a regression test that
replays a scripted 3h-scale stream (or a scaled equivalent, e.g. 3× speed) and asserts hash equality
cross-process. Golden bumps only via SIM_VERSION.
