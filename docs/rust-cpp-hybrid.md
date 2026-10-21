# Strategy: Rust + C++ hybrid (user decision m02010)

"Usiamo Rust dove conviene e C++ dove conviene."

## Division of labor
- **Rust (deterministic core)**: lockstep sim, ECS, A* pathfinding, fixed-point, FNV-1a hashes, golden hash, replay. The Rust suites are authoritative; the C++ sims stay at parity but are NOT required to evolve.
- **C++ (performance layer + tools)**: renderer, texture atlas, sprite rasterization, image IO, audio. FFI via extern "C". If a subsystem needs heavy SIMD (colony height field) or is already built, use C++.
- **FFI rule**: the C++ side must NEVER mutate sim state or feed the hash. Cross-boundary only via Command / Event / snapshot bytes.
- **Tooling**: CI runs both suites. The C++ side is required to pass parity tests (oracle) but its algorithm is free.

## Optimization rules (lead, m02010)
- Heavy optimizations are done by the lead, not guessed. Before optimizing a hotspot: microbenchmark (bench harnesses exist as `simcpp/apps/{bench,pathbench}.cpp` and `rts-game/crates/game/src/bin/bench.rs`); after optimizing: re-run the whole suite and golden parity.
- The golden hash (0x73FF5F0F7980D4A1) must remain stable unless SIM_VERSION intentionally bumps.
- Pathfinding uses the existing 4-connected A* with total-order tie-break; do not introduce float or unordered containers.
