#include "sim/rim_sim.hpp"
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>
#include <fstream>
#include <sstream>
#include <algorithm>

using namespace sim::rim;

static std::string read_all(const std::string& path) {
    std::ifstream f(path); std::ostringstream o; o << f.rdbuf(); return o.str();
}
static uint64_t parse_u64(const std::string& s, size_t pos) { return std::strtoull(s.c_str() + pos, nullptr, 10); }

static void setup_full(ColonySim& s) {
    s.spawn_pawn(0, 5, 5);
    s.spawn_pawn(0, 20, 5);
    s.add_site(0, 8, 5);
    s.add_site(1, 30, 5);
    s.add_job(1, 12, 9);
    s.add_job(0, 14, 20);
}

int main(int argc, char** argv) {
    std::string dir = (argc > 1 ? argv[1] : "oracle_data") + std::string("/rim_vectors.json");
    std::string js = read_all(dir);
    if (js.empty()) { std::printf("rim_parity: MISSING oracle %s\n", dir.c_str()); return 1; }
    size_t cpos = js.find("\"configs\"");
    if (cpos == std::string::npos) { std::printf("rim_parity: no configs\n"); return 1; }
    size_t i = js.find('[', cpos);
    int fail = 0, ok = 0;
    int depth = 0; size_t obj_start = 0;
    for (size_t k = i + 1; k < js.size(); ++k) {
        if (js[k] == '{') { if (depth == 0) obj_start = k; ++depth; }
        else if (js[k] == '}') {
            --depth;
            if (depth == 0) {
                std::string o = js.substr(obj_start, k - obj_start + 1);
                auto num = [&](const char* key, uint64_t& out) {
                    std::string k2 = std::string("\"") + key + "\"";
                    size_t p = o.find(k2); if (p == std::string::npos) { out = 0; return; }
                    size_t c = o.find(':', p);
                    size_t e = o.find_first_not_of(" ", c + 1);
                    out = parse_u64(o, e);
                };
                uint64_t seed = 0, ticks = 0, hsh = 0;
                num("seed", seed); num("ticks", ticks); num("hash", hsh);
                uint64_t got = 0;
                if ((seed == 1234 && ticks == 100) || (seed == 7 && ticks == 30)) {
                    const bool reversed = (seed == 7);
                    std::vector<TickCommands> batches;
                    for (uint64_t t = 0; t < ticks; ++t) {
                        std::vector<PlayerCommand> cmds = {
                            PlayerCommand{0, Command{Kind::Move, 0, 10, 6}},
                            PlayerCommand{0, Command{Kind::Build, 0, 11, 11}}
                        };
                        if (reversed) {
                            // sort descending by (player, sort_key)
                            std::sort(cmds.begin(), cmds.end(), [](const PlayerCommand& x, const PlayerCommand& y) {
                                if (x.player != y.player) return x.player > y.player;
                                const auto& a = sort_key(x.cmd);
                                const auto& b = sort_key(y.cmd);
                                return b < a;
                            });
                        }
                        batches.push_back(TickCommands{t, cmds});
                    }
                    got = run_colony(seed, ticks, batches, setup_full);
                } else if ((seed == 42 && ticks == 60) || (seed == 1 && ticks == 200)) {
                    got = run_colony(seed, ticks, {}, setup_full);
                } else { continue; }
                if (got != hsh) {
                    std::printf("FAIL seed=%llu ticks=%llu got=%016llx oracle=%016llx\n",
                        (unsigned long long)seed, (unsigned long long)ticks,
                        (unsigned long long)got, (unsigned long long)hsh);
                    ++fail;
                } else { ++ok; }
            }
        }
    }
    if (fail) { std::printf("rim_parity: FAIL %d configs\n", fail); return 1; }
    std::printf("rim_parity: OK %d configs match\n", ok);
    return 0;
}
