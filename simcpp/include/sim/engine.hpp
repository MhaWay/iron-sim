#pragma once
// Re-exports of the M1 deterministic primitives (engine_cpp/include/engine/*).
// The RTS game layer adds sim::Command / sim::TickCommands as aliases over
// engine::game::Command / engine::game::TickCommands (canonical FNV-1a + sort).
#include "engine/command_game.hpp"
#include "engine/command.hpp"
#include "engine/event.hpp"
#include "engine/fixed1616.hpp"
#include "engine/hash.hpp"
#include "engine/isqrt.hpp"
#include "engine/replay.hpp"
#include "engine/rng.hpp"
#include "engine/scheduler.hpp"
#include "engine/tick.hpp"

namespace sim {
using engine::Command;
using engine::Event;
using engine::Fixed1616;
using engine::Hasher;
using engine::Tick;
using engine::Rng;

namespace game {
    using EngineCommand = engine::game::Command;
    using Kind = engine::game::Kind;
    using PlayerCommand = engine::game::PlayerCommand;

    struct TickCommands {
        uint64_t tick{0};
        std::vector<PlayerCommand> cmds;

        void sort() {
            std::stable_sort(cmds.begin(), cmds.end(), engine::game::SortKeyLess{});
        }

        uint64_t hash() const {
            return engine::game::TickCommands{tick, cmds}.hash();
        }
    };
} // namespace game
} // namespace sim
