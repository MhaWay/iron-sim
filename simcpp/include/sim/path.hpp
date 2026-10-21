#pragma once
// Deterministic 4-connected A* — mirrors engine_path::find_path with the
// exact Rust BinaryHeap tie-break semantics (docs/cpp/determinism-cpp.md).
#include <cstdint>
#include <vector>
#include "sim/grid.hpp"

namespace sim {

constexpr int64_t kStepS = int64_t{1} << 16; // Q16.16 straight step (path S)

/// Returns the tile path excluding the start tile, or {} with *ok=false when
/// unreachable. N,E,S,W fixed neighbor order, total tie-break (min f, then
/// max order — Rust max-heap pops (min f, max order)).
std::vector<TilePos> find_path(const Grid& grid, TilePos start, TilePos goal, bool* ok);

} // namespace sim
