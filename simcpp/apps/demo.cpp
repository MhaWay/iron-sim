#include "sim/rts_sim.hpp"
#include "sim/rts_econ.hpp"
#include "sim/demo.hpp"
#include "sim/net.hpp"
#include "engine/replay.hpp"
#include <cstdio>
#include <string>

using namespace sim;
using namespace sim::rts;

static std::vector<PlayerCommand> ai_batch(uint64_t t) {
    std::vector<PlayerCommand> m;
    if (t % 11 == 0) { sim::rts::Command c{sim::rts::Kind::Move, 3, static_cast<int16_t>(40 + (t/11)), 20}; m.push_back({0, c}); }
    if (t % 11 == 5) { sim::rts::Command c{sim::rts::Kind::Move, 5, static_cast<int16_t>(80 - (t/11)), 100}; m.push_back({1, c}); }
    return m;
}

int main(int argc, char** argv) {
    const uint64_t seed = 1337;
    const uint64_t ticks = 180;
    const int scale = 4;
    const char* outdir = (argc > 1) ? argv[1] : ".";
    Sim sim(seed);
    engine::ReplayRecorder rec;
    for (uint64_t t = 0; t < ticks; ++t) {
        auto batch = ai_batch(t);
        TickCommands tc{t, batch}; tc.sort();
        std::vector<uint8_t> wire;
        net::encode_tickcommands(wire, tc);
        Hasher h; for (auto b : wire) h.u8(b);
        rec.record(t, h.finish());
        sim.step(tc.cmds);
        if (t % 30 == 0) {
            demo::Frame f;
            demo::render_frame(sim, scale, f);
            demo::write_png(std::string(outdir) + "/frame_" + std::to_string(t / 30) + ".png", f);
        }
    }
    std::printf("demo: seed=%llu ticks=%llu final_hash=%016llx replay_digest=%016llx\n",
        (unsigned long long)seed, (unsigned long long)ticks,
        (unsigned long long)sim.hash(), (unsigned long long)rec.digest());
    return 0;
}