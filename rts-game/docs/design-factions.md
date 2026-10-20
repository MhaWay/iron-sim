# Design: Faction Identity, Resource Model, Tech Trees

**Status:** DRAFT for lead review — this document gates task-2 (RTS Phase 1 implementation).
**Scope:** Game identity, 3 first-playable factions, shared resource model, command surface
additions (Build/Train/Gather) and their ordering rules, fog-of-war approach, MVP slice.

---

## 1. Game identity

**Pitch (user's words, faithfully):** StarCraft 2 × Age of Empires — real-time, lockstep-deterministic.
Resources sit on the map (harvestable tiles, not abstract trickle); you build an empire (base,
economy, army). Setting is **medieval/historical with fantasy additions** — the tech-age progression
(Age of Empires) is fused with a power-center + ability model (StarCraft 2: hotkeys, ability-driven
units) and a persistent-hero fantasy layer.

**Determinism contract (non-negotiable, inherited from AGENTS.md):** fixed tick (30 Hz),
Q16.16 fixed-point everywhere in sim, SplitMix64 RNG only with explicit seed, no HashMap in sim
(BTreeMap/Vec + ordering), inputs sorted per (tick, player, entity, kind), single-threaded sim,
golden hash + SIM_VERSION bump on any behavioral change. All design below is expressible as **data**
(component fields + rule parameters), never special-cased per-faction code.

**Faction ≠ code.** Every faction is a data profile: resource set, unit cost tables, building stats,
rule-system parameters (thresholds, multipliers, timers — all Q16.16 integers). Adding faction #4
later is data + one component tag, no new systems.

---

## 2. Shared resource model (deterministic layer)

Global canonical resources — every faction uses the **same names and mechanics** at the data layer;
factions differ in *ratios, multipliers, and which rule systems consume them* (e.g. Mana is
"Ritual Essence" flavor, Biomass is harvested via sacrifice). Resource nodes are map tiles with a
Fixed1616 stock that depletes as workers Gather and can (MVP) not regenerate.

| Resource | Canonical name | Map presence | Gathered by |
|---|---|---|---|
| Food | Food | berries, farms, herded animals | workers (harvest tiles), hunters |
| Wood | Wood | trees (forests) | workers (chop) |
| Gold | Gold | rock/gold veins on map | workers (dig) |
| Mana | Mana | rare "ley-line" nodes, altars | ritual (faction 2), passive regen near buildings (faction 3), mages (faction 1) |

- Gather{unit, resource} command starts a harvest loop (see §6).
- Per-player stockpiles are ResourceStock components (Vec-based, canonical iteration order).
- All costs/prices in these four resources. No hidden currencies.

**Fog of war (deterministic, simple):** each entity has a Vision{radius_tiles: i16}. During
visibility_system (runs each tick), cells within any owned entity's radius are marked seen for
that player in a FogState (Vec<u8> per cell: unseen / seen / currently-visible bitflags, per-player).
Sight radius is a component value (data), so techs change it by patching components. No LOS raycasts
in MVP. Client reads the same FogState to hide units.

---

## 3. Faction 1 — "Kingdom of Aldren" (Medieval/Historical, balanced archetype)

### Identity
Balanced all-rounder: village economy (farmers), stone→wood→gold progression, classic ages,
upgrades via a **Golden Age** mechanic (the faction's rule system, §3.5). StarCraft-style: one key
ability per unit type.

### Resources & gathering
Food (farms/hunt), Wood (forests), Gold (mines), Mana (rare, only via Mage + Temple — used for
abilities, not basic units). Workers: Serf.

### Buildings
| Building | Cost (Food/Wood/Gold) | Function |
|---|---|---|
| Town Center (TC) | 0 (start) | spawn Serfs (≤5 free, then cost food), drop-off point, age-up anchor |
| Farm | 20 Food / 20 Wood | passive Food (worker assigns to work) |
| House | 15 Wood | +5 pop cap each |
| Barracks | 50 Wood / 25 Gold | train Swordsmen, Archers |
| Watchtower | 30 Wood / 30 Gold | +6 sight, cheap defense (8 dmg vs ground) |
| Mage Tower | 100 Wood / 50 Gold | train Mages (ability units), +1 Mana/min aura |
| Wonder | 300 Food / 300 Wood / 500 Gold | win-condition structure |

### Units (stat block: HP / speed / melee / range / sight; Q16.16)
| Unit | Food | Wood | Gold | HP | Spd (tile/tick) | DMG | Range | Sight | Ability |
|---|---|---|---|---|---|---|---|---|---|
| Serf | 30 | 0 | 0 | 40 | 1.0 | 0 | 0 | 4 | gather |
| Militia | 0 | 20 | 25 | 80 | 1.1 | 8 | 1 (melee) | 5 | — |
| Swordsman | 0 | 25 | 50 | 120 | 1.15 | 11 | 1 | 5 | Shield (tech) |
| Archer | 0 | 40 | 15 | 60 | 1.2 | 9 | 5 (proj) | 7 | Volley (tech) |
| Knight | 0 | 150 | 150 | 220 | 1.5 | 16 | 1 | 6 | Charge |
| Mage | 0 | 150 | 100 | 70 | 1.0 | 12 | 7 (proj) | 8 | Fireball (25 Mana) |

(HP/dmg shown as i32 units, sim scales to Q16.16. Speed shown Q16.16→tiles for readability.)

### Rule system — Golden Age
- KingdomState component per player: civic_points, age: 1..4, era_count: u8.
- Civic points (CP) earned by: buildings owned (data table), unique/tech units alive, first kills,
  relics captured (MVP: 1 relic on map).
- When CP ≥ thresholds {300, 700, 1200} player may AgeUp{age} (200/400/600 Food+Gold).
- Age-up grants **+10% gather and HP (115%, 115%)** and unlocks unit/tech tier.
- Golden Age trigger: if civic rate (CP/min) > threshold {150} while in age k → player enters
  "Golden Age" (timer 60 s = 1800 ticks): units +15% damage, +1 sight. Cooldown 120 s.
  All numbers in a golden_age.json-style data table.

### Tech tree (per age: prerequisites → costs → engine effect)
- Feudal (2): Husbandry (150F/100W) → Serf speed 1.25 via patch Speed component.
- Castle (3): Forging (300W/150G) → +1 tier melee dmg (+5) via Damage component patch; Masonry → +2 armor (reduces dmg received).
- Imperial (4): Banking (400F/600G) → Gold veins yield +50%; Theology (250M) → abilities cost −25% Mana.

### Win/loss
- Victory: Wonder survives 15 min (27000 ticks) held, OR destroy all enemy TCs + buildings.
- Defeat: lose all buildings.
- **Design intent — relic capture objective (NOT MVP, lead decision):** relics spawn on map,
  held relics grant civic points. Deferred to the map/objectives phase; documented here so the
  rule system shape survives.

---
## 4. Faction 2 — Cult of Vhorr (Eldritch / Sacrifice, hero-monster archetype)

### Identity
No worker economy, no building chain in the classical sense. **A single persistent Hero Monster
("Vhorr the Devouring") IS the army.** The player is a MOBA-style cultist: the monster is the power
center (abilities, evolution), the units are **fuel** for sacrifices that mutate/feed the beast.
The archetype maps to the "MOBA-like around an hero/monster with sacrifices" in the user's pitch.

### Resources
- **Essence** (uses Mana slot): generated by passive rituals, by sacrificing units, by killing enemies.
- **Souls** (uses Food slot, flavor only — data maps to Food field, name change in client).
- No Wood/Gold in gather sense — buildings are altars that consume resources.

### Buildings (Altars / Structures)
| Structure | Cost (Essence/Souls) | Function |
|---|---|---|
| Heart Altar (core) | 0 (start) | anchor; generates 1 Essence / 5 s; drop-off for Souls |
| Ritual Circle | 50 Essence | spawn Cultists (cheap fodder units), passive Essence aura |
| Flesh Shrine | 100 Essence / 80 Souls | train stronger fodder (Grunts, Spawnlings) |
| Dark Spire | 250 Essence / 200 Souls | unlock Hero abilities tier + sight |
| Black Obelisk | 400 Essence / 300 Souls | win condition (must survive 10 min, or destroy enemy core) |

### Units (all are *fodder* or *utility*, no traditional army)
| Unit | Essence | Souls | HP | Spd (tiles/tick) | DMG | Sight | Purpose |
|---|---|---|---|---|---|---|---|
| Cultist | 25 | 0 | 50 | 1.0 | 4 | 4 | cheap fodder, fast sacrifice |
| Spawnling | 60 | 30 | 90 | 1.2 | 10 | 5 | mid fodder, regen |
| Aberration | 150 | 80 | 180 | 1.0 | 18 | 6 | tank fodder, on death spawns spores |
| **Hero Monster (Vhorr)** | 0 | 0 | grows (see rule system) | 1.2 | grows | 8 | army power center |

### Rule system — Sacrifice and Devour
This is the core rule that replaces economy:
- Sacrifice command: any owned unit is consumed; grants Essence proportional to unit HP/cost (data table).
- **Devour** (Hero kills): the Hero absorbs a fraction of the killed unit's HP into its own max HP
  and gains permanent stat increments per type of kill (data table).
- **Evolution tiers**: when Hero stats cross thresholds {kills: 10, 30, 70, 140} it enters evolution
  stages; stages change HP, speed, damage, and unlock ability points for player-chosen abilities
  (abilities cost Essence: Slow Aura, Pounce, Regenerate, Terrify).
- The Hero does not use mana/energy; instead it consumes Souls generated by rituals and kills.

### Tech tree (Evolution stages; costs = Essence/Souls + sacrifices)
- Stage 1 (default): 350 HP, 25 dmg.
- Stage 2 (30 kills): 650 HP, 40 dmg — unlock Pounce (cost 80 Essence).
- Stage 3 (70 kills): 950 HP, 65 dmg — unlock Terrify (AoE panic 4 s, 150 Essence).
- Stage 4 (140 kills): 1400 HP, 110 dmg — unlock Devour Aura (allies +15% dmg near Hero).

### Win/loss
- Victory: destroy all enemy buildings/core, or Black Obelisk held 10 min.
- Defeat: **if Hero dies** (the Hero is the army).
- The Hero can regenerate: sacrifices + rituals heal it out of combat.

---

## 5. Faction 3 — The Chitin Swarm (Alien / Parasitic, swarm archetype)

### Identity
Biomass economy. **Units are consumed to evolve.** You spread creep, expand tiers by consuming your
own units, and overwhelm by numbers and mutations. The archetype matches "alien swarm" in the pitch.

### Resources
- **Biomass** (uses Food slot, flavor): collected by units that gather organic material.
- **Minerals** (uses Wood slot): harvested by drones from rocks / crystal deposits.
- **Creep** (special resource, tracked per player via CreepState component with a Vec of cells).
  No Mana used.

### Buildings (Hive structures)
| Structure | Cost (Biomass/Minerals) | Function |
|---|---|---|
| Spine Crawler | 0 (start) | starts movement; can become Hive |
| Hive Heart | 80 Biomass / 40 Minerals | spawn drones, generate creep |
| Creep Tumor | 30 Biomass | spreads creep + provides vision + defenses |
| Baneling Nest | 60 Biomass / 20 Minerals | spawn Banelings |
| Hatchery | 100 Biomass / 60 Minerals | tier expansion; +1 drone cap |
| Evolution Chamber | 150 Biomass / 100 Minerals | tech; +1 to unit tiers |
| Spire | 200 Biomass / 150 Minerals | unlock flying units |
| Leviathan Spore | 300 Biomass / 250 Minerals | win condition |

### Units (swarm)
| Unit | Biomass | Minerals | HP | Spd (tiles/tick) | DMG | Sight | Notes |
|---|---|---|---|---|---|---|---|
| Drone | 20 | 10 | 35 | 1.1 | 0 | 5 | worker, can become structure |
| Zergling | 25 | 0 | 70 | 1.4 | 8 | 5 | swarm meat |
| Hydralisk | 50 | 25 | 120 | 1.1 | 11 | 6 | ranged |
| Roach | 80 | 50 | 220 | 1.0 | 16 | 6 | tank |
| Baneling | 25 | 0 | 60 | 1.3 | 35 | 4 | suicide AoE |
| Mutalisk (flying) | 100 | 75 | 110 | 1.6 | 12 | 8 | fast air |

### Rule system — Biomass Mutation
- **Evolve** command: consumes X Biomass + Y of one or more of the player's own units of a given
  type to **permanently unlock a stronger unit type** or a stat upgrade (+hp, +dmg, +speed).
- Creep spread: CreepState.cells grows by 1 per 180 ticks / structure with creep aura (data);
  units built on creep get +1 sight and +10% hp (data).
- Swarm scaling: when player controls > N units (N = {25, 50, 100}) they get **Swarm Frenzy**:
  +10% attack speed for all units, and +2% gather rate per 10 units over threshold.
- Drone-to-structure: Drone can be converted via Build{building} (command consumes the drone
  in addition to resources; the building is placed at the drone's current tile).

### Tech tree (Swarm tiers: tier 1, 2, 3)
- Tier 1: unlock Zergling, Drone upgrades.
- Tier 2 (Evolution Chamber, 50 Biomass + consume 2 Zerglings): unlock Hydralisk and Roach +
  +10% HP (data).
- Tier 3 (Evolve: consume 4 Hydralisk + 150 Biomass): unlock Baneling and Mutalisk + flying,
  +15% dmg (data).

### Win/loss
- Victory: Leviathan Spore held 12 min or destroy enemy core.
- Defeat: all structures destroyed.

---

## 6. Command surface additions (Build / Train / Gather) — determinism rules

New commands live in `crates/shared::Command`, all must have `sort_key()` producing a **total order**.

- `Build { building: u16, x: i16, y: i16 }` — place and construct (owner derived from entity/player field of batch).
- `Train { building: u32, unit: u16 }` — enqueue training in a building.
- `Gather { unit: u32, resource: u8 }` — start worker harvest loop (walk -> harvest -> return -> repeat).
- `Sacrifice { unit: u32 }` — consume own unit, grant resources (factions 2/3; data-driven table).
- `Evolve { tier: u8, unit_type: u16 }` — faction 3 only (consuming units of unit_type or resources per data table).
- `AgeUp { age: u8 }` — faction 1 only (CP + resource threshold checked in system).

### Ordering rule (the answer to the lead's note)
The batch sorts by `(player, entity, kind_disc, params)` where kind_disc is the enum discriminant
of the Command variant (Move=0, Attack=1, Stop=2, Build=3, Train=4, Gather=5, Sacrifice=6, Evolve=7,
AgeUp=8). Two commands by the same player on the same entity in the same tick then order by variant
discriminant, then by numeric params (x, y / building id / resource id) ascending. This mirrors the
existing rts `sort_key` (entity, kind, params) and rimworld precedent. Consequences:
1. Two runs that feed the **same command multiset** in any line order produce identical sim state —
   enforced by a determinism test that shuffles command vectors per batch.
2. Build-before-Train by the same player on the same building: Build disc=3 < Train disc=4, so
   construction completes placement this tick before training reads the building (if both arrive
   in the same tick, the building is considered present at placement tile).
3. Gather commands sort before Sacrifice: a unit given both commands in one tick will start the
   harvest loop and then be consumed; documented as intentional, replay-stable order.
4. Costs are checked at execution time (phase 2 of the command application, single-threaded).
   Insufficient funds = command no-ops with a SimEvent for the client; no rollback (lockstep-safe).
5. Entity ids of new units/buildings are assigned by the ECS at execution time in the sorted order
   — so the id sequence is replay-stable.
6. `SIM_VERSION` bumps 1 -> 2 and the golden hash is regenerated when these commands land (task-2).

---

## 7. Determinism notes for the new systems (task-2 guidance)
- `visibility_system` after movement: writes per-player fog bitflags; iteration ascending entity index; radius from component.
- Economy/gather loop is a *state machine per worker* (component `Order` extended with `Harvest{node, carry}` states) — no timers based on real time, tick counters only.
- Construction/training timers count down in `construction_system` and `training_system` (registration order defined up-front, same phase1-read/phase2-write pattern as rimworld).
- RNG only used for map generation (seed) and combat tie-breaks that must never depend on wall clock.
- FogState must be part of `Sim::hash()` (per-player bit packed, canonical order) so replays hash it.

---

## 8. MVP slice (implement FIRST — one faction end-to-end)
Implement **Faction 1 (Kingdom of Aldren)** end-to-end before touching faction 2/3 mechanics:
1. **Map + resources (task-2 §1-2):** 128x128 seeded map (SplitMix64), terrain (grass/water/rock),
   resource nodes (Food berries / Wood trees / Gold veins) with Fixed1616 stock.
2. **Entities:** 1 Town Center + 5 Serfs at start.
3. **Commands:** `Build` (Farm, House, Barracks), `Train` (Serf, Militia, Archer), `Gather`.
4. **Economy:** worker harvest loop (A* to node, harvest tick cost, carry limit 10, return to
   nearest own TC/dropoff), construction timer (Farm 30 s, Barracks 45 s, House 20 s), pop cap.
5. **Combat unit:** Militia (melee) vs enemy spawned unit; simple attack targeting nearest enemy.
6. **Tier:** age 1 -> age 2 (`AgeUp`, threshold 1 CP earned by Barracks + 150 Food) — just the data gate.
7. **Win condition:** destroy all enemy buildings (test scenario: 1-player idle enemy, verify victory system fires at 27000-tick wonder/destroy rule).
8. **Fog:** sight-radius cells computed in sim, part of state hash.
9. **Tests:** new golden hash (SIM_VERSION=2); gather-economy sanity (1 Serf sustains the chain:
   income rate > build rate over 3000 ticks); build/train/gather replay determinism (shuffled batches);
   construction-progress determinism.

Faction 2/3 rule systems (Sacrifice, Evolve, Creep, Golden Age) land in Phase 2 — the data *slots*
(component fields, command variants) are defined NOW so no later breaking change to shared types.

---

## 9. Lead review RESOLVED (2026-10-20)
1. **Fog IN state hash — YES.** FogState is part of `Sim::hash()`; replays hash the seen reality.
2. **Golden Age thresholds — KEEP AS-IS** ({300,700,1200} CP; rate>150/min), balance via golden tests later.
3. **Sacrifice/Evolve/AgeUp command variants — DEFINE NOW in shared** with sort_key, but **no-op in
   systems** (structure only; behavior in Phase 2). Avoids kind_disc churn.
4. **Relic objective — SKIP MVP**, documented above as design intent.

## 10. Open questions (none — all resolved by lead)
1. Should fog be in the state hash (my recommendation: yes — it is derived, but deterministic)
   or client-side only? Affects task-2 hash design.
2. Faction 1 Golden Age thresholds: keep my data numbers ({300,700,1200} CP; rate>150/min) as-is?
3. Do we want `Sacrifice`/`Evolve` command variants defined in shared NOW (no behavior) to avoid
   a SIM_VERSION-3 re-sorting of kind_disc later? (I lean yes.)
4. 1-relic capture objective for faction 1 — in MVP slice or skip?
