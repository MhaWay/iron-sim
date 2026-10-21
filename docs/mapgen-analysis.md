# Map generation analysis — AoE/SC practice and our plan

## 1. How the reference games generate maps (public design knowledge)

### Age of Empires (2/3/4)
- **Mirror-symmetric random maps** are the baseline for competitive play: the generator produces
  one half and mirrors it (2- and 4-fold rotational/mirror symmetry) so every player starts with
  identical terrain/resource geometry at mirrored positions. This is the cheapest way to guarantee
  fairness: geometry fairness becomes structural, not statistical.
- **Layered procedural layers**: base terrain (biome/height), then *decoration* layers (forests,
  gold/stone/relic/deer placements) applied per symmetric slot, then **placement constraints**
  (spawn area must clear a disk of ~16-20 tiles, resources at defined distance bands, no entity
  on ramp/path tiles, choke-point distance rules).
- **Named map archetypes** (arena, black forest, highland, islands, coast) = a preset *parameter
  script* over the same primitives, not hand-authored geometry: lake count, forest density, cliff
  frequency, gold cluster count/size, starting resources, player-slot distance, choke width.
  Regicide/Look-Alike = same script + hero spawn constraints.
- **Validation pass**: AoE3/4 run a post-generation "fairness check" (resource count per player,
  walkable connectivity to every spawn, gold quantity bounds); bad seeds retry/repair.
- **Editor + random generator share one script API** (Genius Scenario Editor in AoE, Galaxy
  Editor in SC2) — the random generator is literally an in-game script using the same primitives
  (SetTerrain, createGroup, createUnit) that a designer can also drive by hand. That is the
  architecture to copy: a declarative/DSL layer over imperative primitives.

### StarCraft II (Blizzard map maker + random generator)
- **Region graph**: the map is decomposed into regions (expansions) connected by chokepoints;
  the generator first places *expansion bases* (equal-size plateaus at equal pairwise distances),
  then roads, then decor. Fairness = equal regions, mirrored expansion rings, equal geodesic
  distances between expansions.
- **Random distribution tables**: weighted item placements per region (minerals per base = exact
  count, geysers = fixed count per expansion), i.e. resource *quantities are invariant*, only
  positions vary. This is the key lesson vs pure noise: resources must be *placed by constraint*,
  not sampled by threshold.
- **Triggers + custom map scripts** (Galaxy) re-randomize per-game elements (loot, units) with a
  seed; same seed = same map, matching what a lockstep engine needs.
- **Blizzard's map pool** is curated random scripts (e.g. "2v2 at 12 o'clock/6/10 with 3x3
  mineral lines") — the *generator* is parameterized presets, not one blob of noise.

### What to extract (the "best" of both)
1. **Symmetry-first** (AoE mirror + SC2 expansion-ring): guarantee fairness structurally, make
   statistics secondary.
2. **Constraint-based placement, quantity-invariant** (SC2): fixed counts/distances of resources;
   only jitter is random. (AoE's gold = exact counts per slot too.)
3. **Region/chokepoint graph** (SC2) over pure noise: ensures connectivity + defensible chokes.
4. **Preset scripts as archetypes** (both): arena/forest/islands/mountains = parameter tables,
   not new code.
5. **Post-generation validation** (AoE3/4): reject or repair maps violating fairness/connectivity
   bounds. Deterministic validation — the retry draws from the seeded RNG stream.
6. **Same primitives for editor + random gen** — our future map editor must call exactly these
   functions (spawn_area, place_resource_ring, carve_road) so a human-tweaked map re-enters the
   same deterministic stream.

## 2. Our current generator (verified: rts-game/crates/game/src/sim.rs generate_map)
What exists today:
- Seeded SplitMix64 stream (correct for lockstep), 128x128 grid.
- 2-tile blocked border.
- 700 random blocked tiles (forest), skipped 25% (kind==3 continue) — pure scatter, no terrain
  structure, no clusters, no chokepoints.
- 24 resource nodes random-distributed with 10 food / 8 wood / 6 gold-ish slot split and distance
  bands from the two TC spawn spots (14,14)/(113,113).
- Spawn protection: no blocked/resource tiles within manhattan<14 of TC.

Concrete weaknesses (measured/inspected):
- **Unfair**: the "mirror" of spawn disks is only implicit; node counts per player depend on which
  random draws fall near each TC. A seed can leave one side 1-2 gold nodes short.
- **No structure**: random scatter makes uniform "flat empty field with random trees" maps, not
  playable archetypes; no chokes, no lakes, no roads.
- **No connectivity test**: blocked scatter can isolate pockets (A* fails silently during play).
- **No validation**: bad maps ship. No metrics, no repair.
- **Distance bands use manhattan-to-TC only**: mid-map resources (world gold) are random chance.

## 3. Spec for our generator (hybrid of AoE-style + SC2-style)

Pipeline (all deterministic from a single 64-bit seed stream, integer-only):

1. **Region skeleton (SC2-style)**: place P player bases (P=1..8) on a mirrored ring/rosette
   pattern with equal geodesic spacing (ring radius derived from map size). For even P, rotation
   symmetry P-fold; for 2P players, mirror axis. Record base centers as expansion nodes.
2. **Territory/plateau carving**: at each base carve a clear plateau (radius ~10) + a road graph
   between expansions (Dijkstra on geodesic, carve corridors width 3). Remaining land gets
   value-noise terrain (forests/mountains/water) with density = preset, applied *only outside*
   road corridors and plateaus.
3. **Chokepoints**: verify the road graph edges are the only cheap connections; measure and
   log edge cut distances (fairness metric: all players have equal geodesic distance to any
   opponent).
4. **Resource placement (constraint-first, SC2 table)**: per expansion, fixed counts by preset
   (e.g. minerals: exact count K1 on the plateau ring band 8-14; gold: exact count K2 at geodesic
   25-45; decorative berries/wood: count K3 within 18 of base). Same count and same *class-band*
   for every expansion by symmetry — jitter inside band via RNG.
5. **Decor layer** (AoE): trees, rocks, shrines at preset densities, mirror-symmetric: draw a few
   random points in the fundamental domain and reflect across symmetry ops.
6. **Validation & repair pass (AoE3/4)**: run after generation:
   - every base connected to every other (BFS from base centers),
   - per-player: exact resource counts per class, equal plateau area, equal count of gold bands,
   - if violated: local repair (open blocked tile / move node to nearest legal slot) deterministic
     and order-stable; if irreparable: redraw the whole map from the seed stream (max K retries).
7. **Output**: Grid + typed placement entities, hashable into the sim's state (map digest for
   replays).

Presets (first 4 archetypes, parameter tables — no new code per map):
- **Arena** (1v1 64x64, open, 4 gold spots, fast early)
- **Highland** (2v2 128x128, mountain choke, gold on cliffs)
- **Black Forest** (2v2, dense symmetric trees, hidden gold, 3 chokes)
- **Islands** (FFA 4p, lakes, naval-later or bridges)

## 4. Implementation plan (what a teammate builds)
Phase 1 (core):
- New crate or module `game/src/mapgen.rs`: symmetric primitives + region graph + validation;
  `generate_map(seed, preset)` keeps today's signature, new `generate_map_preset(seed, preset)`.
- Unit tests: fairness invariants (equal per-player counts over 1000 seeds), connectivity always,
  determinism (same seed same digest), symmetry check (mirror map equals rotated map).
- Golden: keep old stream path for SIM_VERSION compatibility or bump deliberately.
Phase 2: presets as parameter tables + CLI flags for demo_v2/server.
Phase 3: editor hooks (same primitives callable) + replay map-digest.

Metrics to log per map (for "get the best generator"): fairness delta (max-min per-player
resource value), choke count, avg geodesic symmetry error, gen time (ms/seed), reject-retry rate.
Target v1: 0 unfair maps (repair handles), <20 ms per 128x128 gen, >99% first-draw pass rate.
