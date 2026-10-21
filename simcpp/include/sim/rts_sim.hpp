#pragma once
// RTS game sim — mirrors rts-game/crates/game/src/sim.rs + economy.rs.
// Single-threaded, fixed tick 30Hz, Q16.16, no float/double, no unordered_map.
#include <cstdint>
#include <vector>
#include "sim/engine.hpp"
#include "sim/ecs.hpp"
#include "sim/grid.hpp"
#include "sim/path.hpp"
#include "sim/rts_shared.hpp"
#include "sim/rts_shared_impl.hpp"
#include "sim/rts_econ.hpp"
#include "sim/rts_shared_impl.hpp"

namespace sim::rts {

constexpr int64_t SPEED_SCALE = 1024;
constexpr size_t MAX_PLAYERS = 2;
constexpr int64_t VICTORY_WONDER_HOLD = 30 * 30 * 60;

struct Unit {
    uint8_t owner{0};
    uint16_t kind{0};
    int32_t hp{0};
    uint32_t speed_q{0};
    int32_t dmg{0};
    Fixed1616 range{};
};
struct Pos { Fixed1616 x{}, y{}; };

using Deposits = std::vector<std::tuple<uint8_t, uint8_t, int32_t>>;
struct GridSide {
    int w{0}, h{0};
    std::vector<uint8_t> blocked;
};
struct VisMirror {
    Visibility words[2];
};

class Sim {
public:
    explicit Sim(uint64_t seed);
    engine::Tick tick() const noexcept { return world_.tick; }
    int alive_units() const;

    void step(const std::vector<PlayerCommand>& cmds);
    uint64_t hash() const;

    // Public command helpers (used by tests + demo + server).
    Entity spawn_unit_kind(uint32_t owner, uint16_t kind, int16_t x, int16_t y);
    uint32_t spawn_unit(uint32_t owner, int16_t x, int16_t y); // legacy: Militia kind=1
    bool build_at(uint32_t owner, uint16_t kind, int16_t x, int16_t y);

    // Direct field access for tests/demo/net state inspection.
    World& world() noexcept { return world_; }
    const World& world() const noexcept { return world_; }
    std::vector<PlayerState>& players() noexcept { return players_; }
    std::vector<Visibility>& vis() noexcept { return vis_; }
    Grid& grid() noexcept { return grid_; }
    const Grid& grid() const noexcept { return grid_; }

private:
    void generate_map();
    void spawn_default_squads();
    void sync_mirrors();
    // command application: returns false = error, ignored by step.
    bool apply_move(uint32_t unit, int16_t x, int16_t y);
    bool apply_attack(uint32_t unit, uint32_t target);
    bool apply_stop(uint32_t unit);
    bool apply_train(uint32_t building, uint16_t unit_kind);
    bool apply_gather(uint32_t unit, uint8_t resource);

    World world_;
    Grid grid_;
    std::vector<PlayerState> players_;
    std::vector<Visibility> vis_;
};

/// Run sim(seed, ticks, stream) returning the state hash. Mirrors game::run_sim.
uint64_t run_sim(uint64_t seed, uint64_t ticks, const std::vector<TickCommands>& stream);

} // namespace sim::rts