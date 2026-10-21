#pragma once
// Economy / game state — mirrors rts-game/crates/game/src/economy.rs.
// All player-visible content is faction 1 (Kingdom of Aldren) expressed
// through canonical resource slots (Food=0, Wood=1, Gold=2, Mana=3).
#include <cstdint>
#include <vector>
#include "sim/engine.hpp"

namespace sim::rts {

constexpr uint16_t MAP_W = 128;
constexpr uint16_t MAP_H = 128;
constexpr int32_t SIGHT_BASE = 5;
constexpr int64_t CARRY = 10;
constexpr uint32_t PICK_TICKS = 150;
constexpr uint32_t SIM_VERSION = 2;

struct Resources {
    int64_t food{0};
    int64_t wood{0};
    int64_t gold{0};
    int64_t mana{0};
    int64_t get(uint8_t slot) const noexcept {
        return slot == 0 ? food : slot == 1 ? wood : slot == 2 ? gold : slot == 3 ? mana : 0;
    }
    void add(uint8_t slot, int64_t amount) noexcept {
        if (slot == 0) food += amount; else if (slot == 1) wood += amount;
        else if (slot == 2) gold += amount; else if (slot == 3) mana += amount;
    }
    void take(uint8_t slot, int64_t amount) noexcept { add(slot, -amount); }
    void hash_into(Hasher& h) const noexcept {
        h.i64(food); h.i64(wood); h.i64(gold); h.i64(mana);
    }
};

enum class BuildingStatus : uint8_t { Site = 0, UnderConstruction = 1, Done = 2, Wreck = 3 };

struct Building {
    uint32_t owner{0};
    uint16_t kind{0};
    BuildingStatus status{BuildingStatus::Site};
    uint32_t hp{0};
    uint32_t max_hp{0};
    uint32_t timer{0};
};

struct TrainItem { uint16_t kind{0}; uint32_t ticks_left{0}; };
using TrainQueue = std::vector<TrainItem>;

struct BuildingSpec {
    const char* name;
    uint16_t w; uint16_t h;
    uint32_t hp;
    uint32_t build_time;
    uint32_t workers;
    int64_t food, wood, gold, mana;
};
struct UnitSpec {
    const char* name;
    uint32_t hp;
    uint32_t dmg;
    int32_t speed_q;
    int64_t range_q;
    uint32_t train_time;
    int64_t food, wood, gold, mana;
};

constexpr BuildingSpec BUILDING_TABLE[7] = {
    {"TownCenter", 3, 3, 1400, 3600, 3, 0, 150, 0, 0},
    {"Farm",       2, 2,  300,  900, 2, 0,  50, 0, 0},
    {"House",      1, 1,  150,  450, 1, 0,  25, 0, 0},
    {"Barracks",   2, 2,  600, 1800, 2, 0, 100, 50, 0},
    {"Watchtower", 1, 1,  400, 1200, 1, 0,  75, 25, 0},
    {"MageTower",  2, 2,  500, 2400, 2, 0,  60, 0, 60},
    {"Wonder",     3, 3, 1800,27000, 3, 0, 600, 600, 120},
};
constexpr UnitSpec UNIT_TABLE[3] = {
    {"Serf",    50, 1, 1200,  65536, 300, 30, 0,  0, 0},
    {"Militia",100, 10, 1200,  65536, 450, 30, 10, 0, 0},
    {"Archer",  60, 8, 1024, 196608, 600, 20, 40, 10, 0},
};
inline const BuildingSpec& building_spec(uint16_t k) noexcept { return BUILDING_TABLE[k]; }
inline const UnitSpec& unit_spec(uint16_t k) noexcept { return UNIT_TABLE[k]; }

struct ResourceNode {
    uint8_t slot{0};
    int64_t total{0};
    int64_t remaining{0};
    void hash_into(Hasher& h) const noexcept {
        h.u8(slot); h.i64(total); h.i64(remaining);
    }
};
struct NodePos {
    int16_t x{0}, y{0};
    void hash_into(Hasher& h) const noexcept {
        h.i32(static_cast<int32_t>(x)); h.i32(static_cast<int32_t>(y));
    }
};

struct PlayerState {
    Resources stock{150, 150, 0, 0};
    uint8_t age{0};
    uint32_t wonder{0};
    uint64_t wonder_started{0};
};

/// Fog of war: packed bitset, row-major, part of the state hash.
struct Visibility {
    uint32_t owner{0};
    std::vector<uint64_t> words; // size = (MAP_W*MAP_H + 63)/64

    Visibility() : words((static_cast<size_t>(MAP_W) * MAP_H + 63) / 64, 0) {}
    explicit Visibility(uint32_t owner_) : owner(owner_),
        words((static_cast<size_t>(MAP_W) * MAP_H + 63) / 64, 0) {}

    void set(int x, int y) noexcept {
        if (x < 0 || y < 0 || x >= MAP_W || y >= MAP_H) return;
        const size_t i = static_cast<size_t>(y) * MAP_W + x;
        words[i >> 6] |= 1ull << (i & 63);
    }
    bool get(int x, int y) const noexcept {
        if (x < 0 || y < 0 || x >= MAP_W || y >= MAP_H) return false;
        const size_t i = static_cast<size_t>(y) * MAP_W + x;
        return (words[i >> 6] >> (i & 63)) & 1ull;
    }
    uint32_t revealed() const noexcept {
        uint32_t n = 0;
        for (uint64_t w : words) n += static_cast<uint32_t>(__builtin_popcountll(w));
        return n;
    }
    void hash_into(Hasher& h) const noexcept {
        h.u32(owner);
        h.u32(static_cast<uint32_t>(words.size()));
        for (uint64_t w : words) h.u64(w);
        h.u32(revealed());
    }
};

enum class WorkMode : uint8_t { Build = 0, Harvest = 1 };
struct WorkJob {
    uint32_t building{0};
    uint32_t node{0};
    uint8_t slot{0};
    WorkMode mode{WorkMode::Build};
    void hash_into(Hasher& h) const noexcept {
        h.u32(building); h.u32(node); h.u8(slot);
        h.u8(mode == WorkMode::Build ? 0 : 1);
    }
};

enum class HarvestKind : uint8_t { ToNode = 0, Picking = 1, Carrying = 2 };
struct HarvestState {
    HarvestKind kind{HarvestKind::ToNode};
    uint32_t node{0};
    uint8_t slot{0};
    uint32_t ticks_left{0};
    int32_t amount{0};
    void hash_into(Hasher& h) const noexcept {
        h.u8(static_cast<uint8_t>(kind));
        switch (kind) {
            case HarvestKind::ToNode:  h.u32(node); h.u8(slot); break;
            case HarvestKind::Picking: h.u32(node); h.u8(slot); h.u32(ticks_left); break;
            case HarvestKind::Carrying: h.u8(slot); h.i32(amount); break;
        }
    }
};

enum class OrderKind : uint8_t { Idle = 0, Move = 1, Attack = 2, Harvest = 3, Construct = 4 };

struct Order {
    OrderKind kind{OrderKind::Idle};
    uint64_t waypoint{0};       // Move: waypoint
    std::vector<TilePos> path;  // Move/Harvest/Construct
    uint32_t target{0};         // Attack target / Harvest node / Construct building

    void hash_into(Hasher& h) const noexcept {
        switch (kind) {
            case OrderKind::Idle: h.u8(0); break;
            case OrderKind::Move:
                h.u8(1); h.u64(waypoint);
                h.u64(static_cast<uint64_t>(path.size()));
                for (const auto& p : path) { h.i32(static_cast<int32_t>(p.x)); h.i32(static_cast<int32_t>(p.y)); }
                break;
            case OrderKind::Attack: h.u8(2); h.u32(target); break;
            case OrderKind::Harvest:
                h.u8(3); h.u32(target);
                h.u64(static_cast<uint64_t>(path.size()));
                for (const auto& p : path) { h.i32(static_cast<int32_t>(p.x)); h.i32(static_cast<int32_t>(p.y)); }
                break;
            case OrderKind::Construct:
                h.u8(4); h.u32(target);
                h.u64(static_cast<uint64_t>(path.size()));
                for (const auto& p : path) { h.i32(static_cast<int32_t>(p.x)); h.i32(static_cast<int32_t>(p.y)); }
                break;
        }
    }
};

inline void hash_order_into(const Order& o, Hasher& h) noexcept { o.hash_into(h); }

} // namespace sim::rts
