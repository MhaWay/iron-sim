#pragma once
// Scheduler — mirrors engine_core::Scheduler (lockstep driver). Enqueue at a
// target tick with a monotonic seq; step(next) executes everything due at <=
// next in (tick, seq) order, then advances tick. Single-threaded, no real time.
#include <algorithm>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <vector>
#include "engine/command.hpp"
#include "engine/tick.hpp"

namespace engine {

class Scheduler {
public:
    Scheduler() = default;

    Tick current() const noexcept { return tick_; }

    void enqueue(Tick at_tick, Command cmd) noexcept {
        pending_.push_back({at_tick, seq_++, cmd});
    }

    void step(Tick next, const std::function<void(Tick, const Command&)>& sink) {
        assert(next > tick_ && "ticks must advance monotonically");
        tick_ = next;
        std::stable_sort(pending_.begin(), pending_.end(),
                         [](const Entry& a, const Entry& b) {
                             return a.first == b.first ? a.second < b.second
                                                       : a.first < b.first;
                         });
        std::size_t i = 0;
        while (i < pending_.size() && pending_[i].first <= next) {
            sink(pending_[i].first, pending_[i].third);
            ++i;
        }
        pending_.erase(pending_.begin(),
                       pending_.begin() + static_cast<std::ptrdiff_t>(i));
    }

    std::size_t pending_size() const noexcept { return pending_.size(); }

private:
    struct Entry { Tick first; uint64_t second; Command third; };
    Tick tick_{0};
    uint64_t seq_{0};
    std::vector<Entry> pending_;
};

} // namespace engine
