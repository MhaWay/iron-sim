
#include <chrono>
#include <cstdio>
#include <algorithm>
#include "sim/grid.hpp"
#include "sim/path.hpp"

int main() {
    sim::Grid g(128, 128);
    for (int y = 0; y < 128; ++y)
        for (int x = 60; x < 70; ++x)
            g.set_blocked({static_cast<int16_t>(x), static_cast<int16_t>(y)}, true);
    double best = 1e18;
    for (int k0 = 0; k0 < 3; ++k0) {
        auto t0 = std::chrono::steady_clock::now();
        for (uint64_t k = 0; k < 1000; ++k) {
            int x = (k % 120) + 1;
            bool ok = false;
            auto p = sim::find_path(g, {static_cast<int16_t>(x), 2}, {static_cast<int16_t>(x), 125}, &ok);
            (void)p;
        }
        std::chrono::duration<double> dt = std::chrono::steady_clock::now() - t0;
        best = std::min(best, dt.count());
    }
    printf("cpp path: %.3f ms / 1000 paths\n", best * 1e3);
    return 0;
}
