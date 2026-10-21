#pragma once
// Command / Event — core transports payloads opaquely (engine_core::Command /
// Event). Game Command lives in command_game.hpp.
#include <cstdint>
#include "engine/tick.hpp"

namespace engine {

struct Command {
    uint64_t payload_hash{0};

    static Command from_hash(uint64_t h) noexcept { return Command{h}; }
    uint64_t hash() const noexcept { return payload_hash; }
};
constexpr bool operator==(const Command& a, const Command& b) noexcept {
    return a.payload_hash == b.payload_hash;
}

struct Event {
    Tick tick{0};
    uint64_t id{0};
    uint64_t payload_hash{0};
};
constexpr bool operator==(const Event& a, const Event& b) noexcept {
    return a.tick == b.tick && a.id == b.id && a.payload_hash == b.payload_hash;
}

} // namespace engine
