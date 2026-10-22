# Tier A — Long-match mechanics (proposal v1)

Goal (lead m00710 + acceptance bar): make matches beyond 20 minutes interesting and
fair without breaking existing streams. Additive design: new systems + new Command
variants, canonical-sort safe, every change gated by a deliberate SIM_VERSION bump.

## Current gaps for 3h streams (probe: render/examples/longsim.rs, seed 12345)
- MATCH_TICKS = 36000 hard stop (20 min @ 30 Hz); score wins by then.
- Nodes deplete permanently (gold/food/wood); after depletion the economy stalls
  (probe: food pinned at 150 because the demo AI loses its target node).
- No food upkeep/starvation (EAT_COST defined but unused) — idle armies persist
  forever; no pressure past TC destruction.
- Long-match fairness: node depletion is time-asymmetric (draw order, not geometry).

## A1. Food upkeep + starvation (SIM_VERSION 3 -> 4)
- Upkeep by unit kind (integer table UPKEEP[5], per 150 ticks, symmetric):
  Serf 0, Militia 1, Archer 1, Knight 3, Siege 4 food.
- Upkeep system drains food every 150 ticks (phase = tick % 150); if food < 0 the
  owner's units lose 1 hp / 150 ticks (starvation), symmetric cost.
- New system in scheduler AFTER harvest: deterministic, entity-order iteration.
- Metrics example: attrition rate p0/p1, income balance (net food per owner),
  fairness delta, ticks-to-starve.

## A2. Node regeneration (long-horizon economy)
- Depleted nodes refill REGEN_RATE per REGEN_INTERVAL ticks (state-driven, no RNG):
  gold none, wood 1 / 150 ticks, food 2 / 50 ticks, capped at original total.
- Keeps A1 starvation pressure real: passive economy survives, idle armies starve.

## A3. Siege/attrition (stretch — evaluate after A1 metrics)
- Wall hp 250 -> 400; idle buildings regen hp after 600 untouched ticks.
- TC defensive aura. Only if A1 alone leaves stalemating.

## Commands
- A1/A2 add NO Command variants: systems-only, canonical sort untouched.
- Replay streams stay Command-valid; state hashes change -> SIM_VERSION 3 -> 4 +
  oracle regen for v4 (rust oracle/dump_rts + simcpp vectors + C++ SIM_VERSION).

## C++ parity blocker (lead decision)
simcpp pins SIM_VERSION=2 (include/sim/rts_econ.hpp) and determinism_cpp CHECKs the
v2 golden 0x73FF5F0F7980D4A1. The Rust v3 (AoE loop, victory_check, age timers) was
never ported to C++ — 9aac02c only regenerated the Rust-side oracle JSON. Before any
SIM_VERSION bump, decide: (a) port v3 (+v4) systems to C++ and bump its SIM_VERSION,
or (b) relax C++ stream-parity. mapgen work avoids this by not touching Sim::new.


## v4 shipping note (2026-10-22)
- SIM_VERSION=4, golden re-baselined: 0x9588_DC2F_2A9A_F3A3 (10775104203620283299).
- Rust oracle JSON regenerated via simcpp/oracle/dump_rts (golden 10775104203620283299,
  economy vector re-run). v3+ stream parity is NOT ported to C++ — C++ test binaries
  pin the v2 legacy golden only and are labeled "legacy v2 oracle cross-check". Rust
  binaries are the delivery products.
