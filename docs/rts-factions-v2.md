# RTS v2 — AoE-style, 2 civilizations (user direction m02010)

The Phase-1 design-factions (3 factions) is superseded for the demo: we deliver 2 civilizations, AoE gameplay (ages, villages, walls, full match), visuals inspired by Warhammer 40K and StarCraft 2.

## Civilization A — "Adeptus Terra" (Warhammer 40K flavor)
- Economy: Imperium, Faith (gold) + Promethium (wood analogue) + Ore (stone).
- Units: Guardsmen (range), Officers (hero), Knights (heavy).
- Wall: Bastion (stone), Imperial Guard training at Barracks.
- Age/Relic: Inquisition (military upgrade), Machine Cult (tech upgrade).
- Visual: gothic, skulls, aquila emblem.

## Civilization B — "Terran Dominion" (StarCraft 2 flavor)
- Economy: Minerals (gold) + Vespene (wood analogue) + Terrazine (stone).
- Units: Marines (range), Marines MKII, Sieges (heavy).
- Wall: Wall, Pylon (power).
- Age: Ghost Academy (telepathic), Engineering (mech upgrades).
- Visual: neon, marine armor, drop pods.

## Shared AoE loop (both civs)
1. Gather: Peasants/SCVs mine and build farms.
2. Age-up (Dark Age → Feudal → Castle → Imperial) by spending resource + building.
3. Build: Town Center, Barracks, Farm, Wall.
4. Train: units gated by building + age.
5. Combat: move/attack; walls block; win conditions: destroy enemy TC, Wonder, or score by 20 min.

## Determinism rules
- SIM_VERSION stays 2. New Command variants: AgeUp, Wall, RallyPoint (placeholders).
- No f32/f64, no HashMap, no global RNG, ordered inputs.
- Golden hash invariant unless intentional bump.
- New systems: age_system, wall_system, train_system (gated by building + age).

## Demo deliverable (goal)
- Downloadable demo from GitHub Release, playable full match, procedural graphics (procedural sprites via software raster or shader-free procedural tiles), UI text 5 languages: en/it/de/fr/es.
