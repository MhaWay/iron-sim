#pragma once
// Rimworld-like colony sim — mirrors rimworld-game/crates/sim/src/colony.rs
// and crates/shared/src/lib.rs. 64x64 map, three systems (needs -> jobs ->
// movement), total-order claim keys ((dist, entity_id)), Q16.16 movement.
#include <cstdint>
#include <optional>
#include <vector>
#include <algorithm>
#include "sim/engine.hpp"
#include "sim/ecs.hpp"
#include "sim/grid.hpp"
#include "sim/path.hpp"
#include "sim/rts_shared.hpp"

namespace sim::rim {

constexpr uint16_t MAP_W = 64;
constexpr uint16_t MAP_H = 64;
constexpr uint32_t SIM_VERSION = 1;
constexpr int64_t NEED_MAX = 65536;
constexpr int64_t HUNGER_DECAY = 16;
constexpr int64_t ENERGY_DECAY = 10;
constexpr int64_t MOOD_DECAY = 2;
constexpr int64_t SLEEP_RECOVER = 120;
constexpr int64_t EAT_RESTORE = 20000;
constexpr int64_t HUNGRY_THRESHOLD = 24576;
constexpr int64_t TIRED_THRESHOLD = 16384;
constexpr int32_t PAWN_SPEED_RAW = 16384; // 0.25 tile/tick
constexpr int64_t HALF = 32768;           // 0.5 in Q16.16

enum class Kind : uint8_t { Move = 0, Wait = 1, Build = 2 };

struct Command {
    Kind kind{Kind::Move};
    uint32_t a{0};    // pawn id (Move/Wait) or kind (Build)
    int16_t b{0};     // x/tile (Move/Build)
    int16_t c{0};     // y (Move/Build)
};

struct SortKey {
    int32_t e, d, p1, p2;
    bool operator<(const SortKey& o) const noexcept {
        return std::make_tuple(e, d, p1, p2) < std::make_tuple(o.e, o.d, o.p1, o.p2);
    }
};

inline SortKey sort_key(const Command& c) noexcept {
    switch (c.kind) {
        case Kind::Move:  return {static_cast<int32_t>(c.a), 0, c.b, c.c};
        case Kind::Wait:  return {static_cast<int32_t>(c.a), 1, 0, 0};
        case Kind::Build: return {static_cast<int32_t>(c.a), 2, c.b, c.c};
    }
    return {};
}

struct PlayerCommand {
    uint32_t player;
    Command cmd;
};
inline bool sort_less(const PlayerCommand& a, const PlayerCommand& b) noexcept {
    if (a.player != b.player) return a.player < b.player;
    return sort_key(a.cmd) < sort_key(b.cmd);
}

struct TickCommands {
    engine::Tick tick{0};
    std::vector<PlayerCommand> cmds;
    void sort() { std::stable_sort(cmds.begin(), cmds.end(), sort_less); }
};

struct Needs {
    int64_t hunger{NEED_MAX};
    int64_t energy{NEED_MAX};
    int64_t mood{NEED_MAX};
    void decay_all() noexcept {
        hunger -= HUNGER_DECAY; if (hunger < 0) hunger = 0;
        energy -= ENERGY_DECAY; if (energy < 0) energy = 0;
        mood -= MOOD_DECAY; if (mood < 0) mood = 0;
    }
};
struct Pos { Fixed1616 x{}, y{}; };

enum class ModeKind : uint8_t { Idle = 0, Go = 1, Eat = 2, Sleep = 3, Work = 4 };
struct Mode {
    ModeKind kind{ModeKind::Idle};
    std::vector<TilePos> path;      // Go
    size_t waypoint{0};             // Go
    TilePos site{0, 0};             // Go target (site) / Eat / Sleep / Work site
    uint8_t timer{0};               // Eat timer
    uint32_t job{0};                // Work job id

    void hash_into(Hasher& h) const noexcept {
        switch (kind) {
            case ModeKind::Idle: h.u8(0); break;
            case ModeKind::Go:
                h.u8(1);
                h.u32(static_cast<uint32_t>(waypoint));
                h.u32(static_cast<uint32_t>(path.size()));
                for (const auto& t : path) { h.i32(t.x); h.i32(t.y); }
                break;
            case ModeKind::Eat:
                h.u8(2); h.i32(site.x); h.i32(site.y); h.u8(timer); break;
            case ModeKind::Sleep: h.u8(3); h.i32(site.x); h.i32(site.y); break;
            case ModeKind::Work: h.u8(4); h.u32(job); h.i32(site.x); h.i32(site.y); break;
        }
    }
};

struct Pawn {
    uint32_t owner{0};
    Fixed1616 speed{Fixed1616{int64_t{PAWN_SPEED_RAW}}};
    Mode mode{};
};

struct Job {
    uint8_t kind{0};             // 0=haul food, 1=tidy (flavor)
    TilePos site{0, 0};
    std::optional<uint32_t> assigned{};
    bool done{false};
};

struct Site {
    uint8_t kind{0};             // 0=storage (food), 1=bed
    TilePos tile{0, 0};
    std::optional<uint32_t> occupied_by{};
};

class ColonySim {
public:
    explicit ColonySim(uint64_t seed);

    uint32_t spawn_pawn(uint32_t owner, int32_t x, int32_t y);
    void add_site(uint8_t kind, int16_t x, int16_t y);
    void add_job(uint8_t kind, int16_t x, int16_t y);

    void step(const std::vector<PlayerCommand>& cmds);
    uint64_t hash() const;

    World& world() noexcept { return world_; }
    const World& world() const noexcept { return world_; }
    Grid& grid() noexcept { return grid_; }
    const Grid& grid() const noexcept { return grid_; }

private:
    bool apply_command(const Command& c);
    bool pawn_tile(uint32_t pawn, TilePos& out) const;

    World world_;
    Grid grid_{MAP_W, MAP_H};
};

using SetupFn = void (*)(ColonySim&);
uint64_t run_colony(uint64_t seed, uint64_t ticks, const std::vector<TickCommands>& batches, SetupFn setup);

} // namespace sim::rim
