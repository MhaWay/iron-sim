// Parity: SplitMix64 streams vs engine_core::Rng (Rust oracle).
#include <cstdint>
#include <string>
#include "engine/rng.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "rng_seeds");
    const auto* vecs = jsonlite::find(doc, "vectors");
    test::check(vecs != nullptr && vecs->is_arr(), "rng_seeds: vectors array");
    if (!vecs) return 1;
    for (const auto& v : vecs->arr()) {
        const auto* seedVal = jsonlite::find(v, "seed");
        const auto* valsVal = jsonlite::find(v, "values");
        if (!seedVal || !valsVal) { test::check(false, "rng_seeds: fields"); continue; }
        uint64_t seed = test::hex16(seedVal->str());
        engine::Rng rng(seed);
        const auto& vals = valsVal->arr();
        for (std::size_t i = 0; i < vals.size(); ++i) {
            const uint64_t want = test::hex16(vals[i].str());
            const uint64_t got = rng.next_u64();
            if (got != want) {
                std::printf("FAIL: rng seed %016llx idx %zu: got %016llx want %016llx\n",
                            static_cast<unsigned long long>(seed), i,
                            static_cast<unsigned long long>(got),
                            static_cast<unsigned long long>(want));
                ++test::g_checks; ++test::g_failures;
                break;
            }
            ++test::g_checks;
        }
    }
    // self-consistency: two same-seed streams agree
    engine::Rng a(0xCAFEBABE), b(0xCAFEBABE);
    for (int i = 0; i < 64; ++i) test::check(a.next_u64() == b.next_u64(), "rng reproducible");
    return test::finish("rng_parity");
}
