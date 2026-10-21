#pragma once
// ReplayRecorder — per-tick command-hash recording with a stream-order-
// sensitive digest (mirrors engine_core::ReplayRecorder exactly).
#include <cstddef>
#include <cstdint>
#include <utility>
#include <vector>
#include "engine/hash.hpp"
#include "engine/tick.hpp"

namespace engine {

class ReplayRecorder {
public:
    ReplayRecorder() = default;

    void record(Tick tick, uint64_t payload_hash) noexcept {
        if (!ticks_.empty() && ticks_.back().first == tick) {
            ticks_.back().second.push_back(payload_hash);
        } else {
            ticks_.emplace_back(tick, std::vector<uint64_t>{payload_hash});
        }
        digest_ = hash_fold(digest_, tick);
        digest_ = hash_fold(digest_, payload_hash);
    }

    uint64_t digest() const noexcept { return digest_; }

    static uint64_t tick_digest(Tick tick, const uint64_t* hashes, std::size_t n) noexcept {
        uint64_t h = tick;
        for (std::size_t i = 0; i < n; ++i) h = hash_fold(h, hashes[i]);
        return h;
    }

    const std::vector<std::pair<Tick, std::vector<uint64_t>>>& ticks() const noexcept {
        return ticks_;
    }

private:
    std::vector<std::pair<Tick, std::vector<uint64_t>>> ticks_;
    uint64_t digest_{0};
};

} // namespace engine
