#include "sim/rts_sim.hpp"
#include "sim/net.hpp"
#include <cstdio>
#include <cstdint>
#include <vector>
#include <cstdlib>
using namespace sim;
using namespace sim::rts;

// Lockstep P2P client. Connects to a peer port; drives player slot from argv[3].
int main(int argc, char** argv) {
    uint16_t peer = (argc > 1) ? static_cast<uint16_t>(std::stoi(argv[1])) : 25101;
    uint16_t my = (argc > 2) ? static_cast<uint16_t>(std::stoi(argv[2])) : 25102;
    uint32_t player = (argc > 3) ? static_cast<uint32_t>(std::stoi(argv[3])) : 0;
    net::UdpPort me;
    if (!me.open(my)) { std::printf("client bind failed\n"); return 1; }
    Sim sim(9999);
    std::printf("client player=%u port=%u peer=%u\n", player, my, peer);
    for (uint64_t t = 0; t < 120; ++t) {
        std::vector<PlayerCommand> mine;
        // Emit an idle batch (no commands); a real client sends input.
        std::vector<uint8_t> w; net::encode_tickcommands(w, TickCommands{t, mine});
        me.send_to(peer, w);
        std::vector<PlayerCommand> merged = mine;
        std::vector<uint8_t> got;
        for (int spin = 0; spin < 3000000; ++spin) { if (me.recv_nonblock(got)) { auto tc = net::decode_tickcommands(got); for (auto& pc : tc.cmds) merged.push_back(pc); break; } }
        TickCommands tc{t, merged}; tc.sort();
        sim.step(tc.cmds);
    }
    std::printf("client final_hash=%016llx\n", (unsigned long long)sim.hash());
    return 0;
}
