# Colony sim — terrain layering (Minecraft × RimWorld)

User direction (m02010): height layers, Minecraft-like voxel/heightfield but RimWorld management.

## Design sketch (not yet implemented)
- Terrain becomes a 2.5D height field: Q16.16 height per tile, walkability depends on neighbor slope (slope <= max_slope).
- Mining/Excavation commands: dig command removes terrain, creates rubble/ore node.
- Build on floor: floor slab, walls, doors on floor cells.
- Multi-level: cells above floor get air/ceiling; roof/foundation layer.
- Colonists can travel between levels via stairs/ladders (cost).
- RimWorld management layer: needs, jobs, mood, combat, research.
- Performance: the heightfield is ideal for C++ SIMD (the "use C++ where it pays" boundary) — terrain query/meshing.
- Determinism: heights and operations must remain fixed-point and hash-visible.

This is the Phase-4+ direction for colony sim. Not started. The RTS demo is the immediate priority.
