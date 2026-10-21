#include "sim/rts_sim.hpp"
#include <cstdio>
#include <cstdlib>
#include <fstream>

using namespace sim::rts;

int main(int argc, char** argv) {
    int failures = 0;
    uint64_t gold_hash = 0, econ_hash = 0;
    int64_t wood = -1;
    if (argc > 1) {
        std::string path = std::string(argv[1]) + "/rts_vectors.json";
        std::ifstream f(path);
        if (f) {
            std::string s((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
            auto p = s.find("\"golden_hash\":");
            if (p != std::string::npos) gold_hash = std::strtoull(s.c_str() + p + 14, nullptr, 10);
            p = s.find("\"economy_hash\":");
            if (p != std::string::npos) econ_hash = std::strtoull(s.c_str() + p + 15, nullptr, 10);
            p = s.find("\"economy_wood\":");
            if (p != std::string::npos) wood = (int64_t)std::strtoll(s.c_str() + p + 15, nullptr, 10);
        }
    }

    // Mirror economy test: seed 777, spawn_unit(0,18,16), Farm scan fy/fx 12..36 first ok, Gather wood, 261 steps.
    Sim s(777);
    auto worker = s.spawn_unit(0, 18, 16);
    bool built = false;
    for (int fy = 12; fy < 36 && !built; ++fy)
        for (int fx = 12; fx < 36 && !built; ++fx)
            built = s.build_at(0, 1, static_cast<int16_t>(fx), static_cast<int16_t>(fy));
    if (!built) { std::fprintf(stderr, "FAIL: farm build scan failed\n"); ++failures; }
    {
        std::vector<PlayerCommand> batch{{0, Command{Kind::Gather, worker, static_cast<int16_t>(static_cast<uint16_t>(1)), 0}}};
        s.step(batch);
    }
    for (int i = 0; i < 260; ++i) s.step({});
    auto h = s.hash();
    if (gold_hash != 0) {
        // Golden test handled elsewhere; here check economic parity.
        if (h != econ_hash) { std::fprintf(stderr, "FAIL: economy hash C++=%016llx oracle=%016llx\n", (unsigned long long)h, (unsigned long long)econ_hash); ++failures; }
        if (s.players()[0].stock.wood != wood) { std::fprintf(stderr, "FAIL: wood C++=%lld oracle=%lld\n", (long long)s.players()[0].stock.wood, (long long)wood); ++failures; }
    }
    // Determinism: run again same inputs -> same hash.
    Sim s2(777);
    auto w2 = s2.spawn_unit(0, 18, 16);
    for (int fy = 12; fy < 36; ++fy) { bool ok=false; for (int fx = 12; fx < 36 && !ok; ++fx) ok = s2.build_at(0, 1, (int16_t)fx, (int16_t)fy); if (ok) break; }
    std::vector<PlayerCommand> b2{{0, Command{Kind::Gather, w2, static_cast<int16_t>(static_cast<uint16_t>(1)), 0}}};
    s2.step(b2);
    for (int i = 0; i < 260; ++i) s2.step({});
    if (h != s2.hash()) { std::fprintf(stderr, "FAIL: non-deterministic economy run\n"); ++failures; }
    if (failures == 0) { std::printf("rts_economy_determinism_cpp: OK hash=%016llx wood=%lld\n", (unsigned long long)h, (long long)s.players()[0].stock.wood); return 0; }
    std::printf("rts_economy_determinism_cpp: %d FAILURE(S)\n", failures);
    return 1;
}
