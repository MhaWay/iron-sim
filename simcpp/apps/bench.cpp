
#include <chrono>
#include <cstdio>
#include <vector>
#include <algorithm>
#include "sim/rts_sim.hpp"

using namespace sim;
using namespace sim::rts;

static std::vector<int16_t> zigzag(uint64_t ticks) {
    std::vector<int16_t> v;
    v.reserve(ticks);
    int16_t x = 1, dir = 1;
    for (uint64_t i = 0; i < ticks; ++i) {
        v.push_back(x);
        x += dir;
        if (x > 20) dir = -1;
        if (x < 1) dir = 1;
    }
    return v;
}

static std::vector<TickCommands> golden_stream(uint64_t ticks) {
    auto xs = zigzag(ticks);
    std::vector<TickCommands> stream(ticks);
    for (uint64_t t = 0; t < ticks; ++t) {
        stream[t].tick = t;
        if (t % 13 == 3)
            stream[t].cmds.push_back({0, rts::Command{rts::Kind::Move, static_cast<uint32_t>(0), xs[t], 5}});
        if (t % 13 == 7)
            stream[t].cmds.push_back({1, rts::Command{rts::Kind::Move, static_cast<uint32_t>(1), static_cast<int16_t>(30 - xs[t]), 5}});
        if (t % 29 == 9)
            stream[t].cmds.push_back({2, rts::Command{rts::Kind::Attack, 2, static_cast<int16_t>(static_cast<uint16_t>(0)), 0}});
        stream[t].sort();
    }
    return stream;
}

int main() {
    const uint64_t n = 10000;
    auto stream = golden_stream(n);
    printf("cpp: warmup...\n");
    (void)run_sim(12345, 200, stream);
    double best = 1e18;
    for (int i = 0; i < 5; ++i) {
        auto t0 = std::chrono::steady_clock::now();
        auto h = run_sim(12345, n, stream);
        std::chrono::duration<double> dt = std::chrono::steady_clock::now() - t0;
        double tps = (double)n / dt.count();
        best = std::min(best, dt.count());
        printf("  hash=%016llx %.0f ticks/s (%.1f ms)\n", (unsigned long long)h, tps, dt.count()*1e3);
    }
    printf("cpp best: %.0f ticks/s (%.1f ms per %llu ticks)\n", (double)n / best, best*1e3, (unsigned long long)n);
    return 0;
}
