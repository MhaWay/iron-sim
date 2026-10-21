# RTS demo SVG assets

Vector sprite assets for the iron-sim RTS demo renderer (Phase-1, 30 Hz lockstep).
All files are hand-authored SVG: `rect`/`circle`/`path`/`line` primitives,
fixed `viewBox` sizes, no external fonts or raster data.
License: MIT OR Apache-2.0 (same as the rest of the workspace).

## Units — 64×64 (`units/`)
| file | faction | silhouette / palette |
|---|---|---|
| `villager-kingdom.svg` | Kingdom of Aldren | aproned worker, gold trim on steel |
| `soldier-kingdom.svg` | Kingdom of Aldren | speared, shielded steel plate with gold crest |
| `archer-kingdom.svg` | Kingdom of Aldren | longbow, gold cap, leather + quiver |
| `monster-cult.svg` | Cult of Vhorr | horned skull-haunch, crimson flesh, bone spikes |
| `drone-swarm.svg` | Chitin Swarm | six-legged chitin drone, green body, veined wings |

## Buildings — 64×64 (`buildings/`)
| file | role |
|---|---|
| `towncenter-kingdom.svg` | main HQ, resource drop-off |
| `farm.svg` | wood production (maps to the sim's `farm` building) |
| `barracks-kingdom.svg` | unit training |

## Tiles — 32×32 (`tiles/`)
| file | meaning |
|---|---|
| `grass.svg` | walkable base terrain (grass + speck pattern) |
| `tree.svg` | blocking decoration / wood flavour |
| `gold.svg` | blocking gold-node decoration |

## HUD icons (`icons.svg`, 64×32)
One sheet, two 32×32 cells:
- left cell — gather (hand holding gold coin);
- right cell — build (gold trowel with steel blade).

## Notes
- Colors are palette-coded per faction (Kingdom `#c9a227` gold / `#8d99ae` steel;
  Cult `#7a1f1f` crimson / `#e8dcc0` bone; Swarm `#2e7d32` chitin / `#b2fead` wing).
- Generated PNGs (if the demo renderer needs raster) can be produced with
  `python3` + `Pillow` (SVG → PNG via `cairosvg` or `svglib`, or hand-drawn with
  PIL as a fallback). Generated PNGs are not checked in.
- All SVG files are verified to parse with `python3` (`xml.etree.ElementTree`).
