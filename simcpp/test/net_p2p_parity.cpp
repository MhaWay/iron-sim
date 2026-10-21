#include "sim/rts_sim.hpp"
#include "sim/net.hpp"
#include <cstdio>
#include <cstdint>
#include <vector>
#include <algorithm>

using namespace sim;
using namespace sim::rts;

static std::vector<PlayerCommand> batch_for(uint64_t t) {
    std::vector<PlayerCommand> m;
    if (t % 7 == 0) { rts::Command mv{Kind::Move, 3, static_cast<int16_t>(6 + (t/2)), 5}; m.push_back({0, mv}); }
    if (t % 5 == 0) { rts::Command mv{Kind::Move, 4, static_cast<int16_t>(107 - (t/2)), 113}; m.push_back({1, mv}); }
    return m;
}

static uint64_t run_pair(uint64_t seed, uint64_t ticks, uint16_t pa_port, uint16_t pb_port) {
    net::UdpPort pa, pb;
    if (!pa.open(pa_port)) { std::printf("bind A failed\n"); return 0; }
    if (!pb.open(pb_port)) { std::printf("bind B failed\n"); return 0; }
    rts::Sim sa(seed), sb(seed);
    for (uint64_t t = 0; t < ticks; ++t) {
        auto ab = batch_for(t);
        // Split: player 0 cmds go to A, player 1 cmds go to B.
        std::vector<PlayerCommand> a_cmds, b_cmds;
        for (auto& pc : ab) (pc.player == 0 ? a_cmds : b_cmds).push_back(pc);
        std::vector<uint8_t> w1; net::encode_tickcommands(w1, TickCommands{t, a_cmds});
        std::vector<uint8_t> w2; net::encode_tickcommands(w2, TickCommands{t, b_cmds});
        pa.send_to(pb_port, w1);
        pb.send_to(pa_port, w2);
        std::vector<PlayerCommand> am = a_cmds, bm = b_cmds;
        std::vector<uint8_t> got;
        for (int spin = 0; spin < 2000000; ++spin) { if (pa.recv_nonblock(got)) { for (auto& pc : net::decode_tickcommands(got).cmds) am.push_back(pc); break; } }
        for (int spin = 0; spin < 2000000; ++spin) { if (pb.recv_nonblock(got)) { for (auto& pc : net::decode_tickcommands(got).cmds) bm.push_back(pc); break; } }
        TickCommands tam{t, am}; tam.sort();
        TickCommands tbm{t, bm}; tbm.sort();
        sa.step(tam.cmds);
        sb.step(tbm.cmds);
    }
    return sa.hash();
}

int main() {
    const uint64_t seed = 9999, ticks = 60;
    // Run one pair; verify its hash equals the non-networked single-process merged run.
    const uint64_t hp = run_pair(seed, ticks, 25101, 25102);
    rts::Sim sc(seed);
    for (uint64_t t = 0; t < ticks; ++t) {
        TickCommands tc{t, batch_for(t)}; tc.sort();
        sc.step(tc.cmds);
    }
    const uint64_t hs = sc.hash();
    const bool ok = (hp == hs) && hp != 0;
    std::printf("net_p2p_parity: pair=%016llx solo=%016llx %s\n",
        (unsigned long long)hp, (unsigned long long)hs, ok ? "OK" : "FAIL");
    return ok ? 0 : 1;
}
