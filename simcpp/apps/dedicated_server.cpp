#include "sim/rts_sim.hpp"
#include "sim/net.hpp"
#include <cstdio>
#include <cstdint>
#include <vector>
using namespace sim;
using namespace sim::rts;

// Dedicated server: runs the authoritative sim, relays each player's commands
// to the other player and applies them on the same tick (lockstep server-authoritative).
int main(int argc, char** argv) {
    uint16_t port = (argc > 1) ? static_cast<uint16_t>(std::stoi(argv[1])) : 25100;
    net::UdpPort p;
    if (!p.open(port)) { std::printf("server bind failed\n"); return 1; }
    Sim sim(2024);
    std::printf("dedicated_server listening udp/loopback port=%u\n", port);
    for (uint64_t t = 0; t < 600; ++t) {
        std::vector<PlayerCommand> merged;
        // Collect from both clients: poll a few times, decode, append.
        std::vector<uint8_t> got;
        for (int spin = 0; spin < 3000000; ++spin) { if (p.recv_nonblock(got)) { auto tc = net::decode_tickcommands(got); for (auto& pc : tc.cmds) merged.push_back(pc); break; } }
        TickCommands tc{t, merged}; tc.sort();
        sim.step(tc.cmds);
    }
    std::printf("server final_hash=%016llx\n", (unsigned long long)sim.hash());
    return 0;
}
