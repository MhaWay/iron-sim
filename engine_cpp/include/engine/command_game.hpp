#pragma once
// Game Command — mirrors rts-game/crates/shared/src/lib.rs Command +
// TickCommands: 9 kinds with frozen kind_disc, total sort_key, canonical
// FNV-1a hash. Wire fields are little-endian.
#include <algorithm>
#include <cstdint>
#include <utility>
#include <vector>
#include "engine/hash.hpp"

namespace engine::game {

enum class Kind : uint8_t { // frozen discriminants — never renumbered
    Move = 0, Attack = 1, Stop = 2, Build = 3, Train = 4,
    Gather = 5, Sacrifice = 6, Evolve = 7, AgeUp = 8,
};

struct Command {
    Kind kind{Kind::Move};
    uint32_t a{0}; // primary id (unit / building / unit_type / age)
    int16_t b{};   // x / target-low / tier
    int16_t c{};   // y / padding

    // sort_key: (entity, kind_disc, params) — total order.
    struct SortKey { uint32_t e; uint8_t d; int64_t p1; int64_t p2; };
    SortKey sort_key() const noexcept {
        switch (kind) {
            case Kind::Move:     return {a, 0, b, c};
            case Kind::Attack:   return {a, 1, static_cast<int32_t>(static_cast<uint16_t>(b)), 0};
            case Kind::Stop:     return {a, 2, 0, 0};
            case Kind::Build:    return {a, 3, b, c};
            case Kind::Train:    return {a, 4, static_cast<int32_t>(static_cast<uint16_t>(b)), 0};
            case Kind::Gather:   return {a, 5, static_cast<int32_t>(static_cast<uint8_t>(b)), 0};
            case Kind::Sacrifice:return {a, 6, 0, 0};
            case Kind::Evolve:   return {a, 7, static_cast<int32_t>(static_cast<uint8_t>(b)), 0};
            case Kind::AgeUp:    return {a, 8, 0, 0};
        }
        return {};
    }
    uint8_t kind_disc() const noexcept { return static_cast<uint8_t>(kind); }

    void hash_into(Hasher& h) const noexcept {
        switch (kind) {
            case Kind::Move:
                h.u8(0); h.u32(a); h.i16(b); h.i16(c); break;
            case Kind::Attack:
                h.u8(1); h.u32(a); h.u32(static_cast<uint16_t>(b)); break;
            case Kind::Stop:
                h.u8(2); h.u32(a); break;
            case Kind::Build:
                h.u8(3); h.u16(static_cast<uint16_t>(a)); h.i16(b); h.i16(c); break;
            case Kind::Train:
                h.u8(4); h.u32(a); h.u16(static_cast<uint16_t>(b)); break;
            case Kind::Gather:
                h.u8(5); h.u32(a); h.u8(static_cast<uint8_t>(b)); break;
            case Kind::Sacrifice:
                h.u8(6); h.u32(a); break;
            case Kind::Evolve:
                h.u8(7); h.u8(static_cast<uint8_t>(b)); h.u16(static_cast<uint16_t>(a)); break;
            case Kind::AgeUp:
                h.u8(8); h.u8(static_cast<uint8_t>(a)); break;
        }
    }
};
constexpr bool operator==(const Command& a, const Command& b) noexcept {
    return a.kind == b.kind && a.a == b.a && a.b == b.b && a.c == b.c;
}

struct PlayerCommand { uint32_t player; Command cmd; };

struct SortKeyLess {
    bool operator()(const PlayerCommand& x, const PlayerCommand& y) const noexcept {
        if (x.player != y.player) return x.player < y.player;
        const auto sx = x.cmd.sort_key(), sy = y.cmd.sort_key();
        if (sx.e != sy.e) return sx.e < sy.e;
        if (sx.d != sy.d) return sx.d < sy.d;
        if (sx.p1 != sy.p1) return sx.p1 < sy.p1;
        return sx.p2 < sy.p2;
    }
};

/// TickCommands batch (relay wire format): tick + (player, Command) list.
struct TickCommands {
    uint64_t tick{0};
    std::vector<PlayerCommand> cmds;

    /// Canonical order: player, then Command::sort_key (stable within equal keys).
    void sort() noexcept { std::stable_sort(cmds.begin(), cmds.end(), SortKeyLess{}); }

    /// Canonical hash: tick, cmd count, then (player, canonical command bytes).
    uint64_t hash() const noexcept {
        Hasher h;
        h.u64(tick);
        h.u32(static_cast<uint32_t>(cmds.size()));
        for (const auto& pc : cmds) {
            h.u32(pc.player);
            pc.cmd.hash_into(h);
        }
        return h.finish();
    }
};

} // namespace engine::game
