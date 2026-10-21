#pragma once
// SplitMix64 — mirrors engine_core::Rng exactly (advance + mix + new(seed)
// consumes one output). Same seed + same call sequence == same stream.
#include <cstdint>

namespace engine {

class Rng {
public:
    static constexpr uint64_t kGamma = 0x9E3779B97F4A7C15ULL;
    static constexpr uint64_t kM1 = 0xBF58476D1CE4E5B9ULL;
    static constexpr uint64_t kM2 = 0x94D049BB133111EBULL;

    // Rust: state = seed; then discard the first next_u64().
    explicit Rng(uint64_t seed) noexcept : state_(seed) { next_u64(); }

    uint64_t next_u64() noexcept {
        state_ += kGamma;
        uint64_t z = state_;
        z = (z ^ (z >> 30)) * kM1;
        z = (z ^ (z >> 27)) * kM2;
        return z ^ (z >> 31);
    }

    uint64_t state() const noexcept { return state_; }

private:
    uint64_t state_;
};

} // namespace engine
