#pragma once
// RTS command surface — mirrors rts-game/crates/shared (FNV-1a canonical hash).

#include <algorithm>
#include <cstdint>
#include <tuple>
#include <vector>

#include "sim/engine.hpp"

namespace sim::rts {

enum class Kind : uint8_t { Move = 0, Attack = 1, Stop = 2, Build = 3, Train = 4, Gather = 5, Sacrifice = 6, Evolve = 7, AgeUp = 8 };

struct Command { Kind kind; uint32_t a; int16_t b; int16_t c; };
struct PlayerCommand { uint32_t player; Command cmd; };
struct SortKey {
    int32_t e, d, p1, p2;
    bool operator<(const SortKey& o) const noexcept {
        return std::make_tuple(e, d, p1, p2) < std::make_tuple(o.e, o.d, o.p1, o.p2);
    }
};

SortKey sort_key(const Command& c) noexcept;
void hash_into(const Command& c, Hasher& h) noexcept;

struct SortKeyLess {
    bool operator()(const PlayerCommand& a, const PlayerCommand& b) const noexcept {
        if (a.player != b.player) return a.player < b.player;
        return sort_key(a.cmd) < sort_key(b.cmd);
    }
};

struct TickCommands {
    engine::Tick tick{0};
    std::vector<PlayerCommand> cmds;
    void sort() { std::stable_sort(cmds.begin(), cmds.end(), SortKeyLess{}); }
    uint64_t hash() const {
        Hasher h;
        h.u64(tick);
        h.u32(static_cast<uint32_t>(cmds.size()));
        for (const auto& pc : cmds) { h.u32(pc.player); hash_into(pc.cmd, h); }
        return h.finish();
    }
};

} // namespace sim::rts

#include "sim/rts_shared_impl.hpp"
