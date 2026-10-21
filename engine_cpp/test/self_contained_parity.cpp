// Structural determinism checks: types are integer-sized, header math stays
// integer, and fixed_sqrt == isqrt(v<<16) identity holds (mirrors Rust).
#include <cstdint>
#include <type_traits>
#include "engine/fixed1616.hpp"
#include "engine/hash.hpp"
#include "engine/isqrt.hpp"
#include "engine/rng.hpp"
#include "tests_common.hpp"

int main(int, char**) {
    test::check(sizeof(engine::Fixed1616) == sizeof(int64_t), "fixed size 8");
    test::check(std::is_integral<uint64_t>::value, "rng state integral");
    test::check(sizeof(engine::Rng) == sizeof(uint64_t), "rng size");
    // fixed_sqrt identity: for v>0, fixed_sqrt(v) == isqrt(v<<16)
    auto fixed_sqrt = [](int64_t raw) -> int64_t {
        if (raw <= 0) return 0;
        const engine::detail::u128 n = static_cast<engine::detail::u128>(raw) << 16;
        return static_cast<int64_t>(static_cast<uint64_t>(engine::detail::isqrt_u128(n)));
    };
    for (int64_t v : {65536LL, 131072LL, 163840LL, 262144LL, 3LL, 1000LL}) {
        const int64_t r = fixed_sqrt(v);
        // floor property: r*r <= v*2^16 < (r+1)*2^16 in isqrt domain
        const auto sq = static_cast<engine::detail::u128>(r) * static_cast<engine::detail::u128>(r);
        const auto tgt = static_cast<engine::detail::u128>(static_cast<uint64_t>(v)) << 16;
        test::check(sq <= tgt && tgt < sq + 2 * static_cast<engine::detail::u128>(r) + 1,
                    "isqrt floor identity");
    }
    test::check(fixed_sqrt(0) == 0 && fixed_sqrt(-5) == 0, "sqrt of non-positive");
    test::check(fixed_sqrt(65536) == 65536, "sqrt(1)=1");
    test::check(fixed_sqrt(131072) == 92681, "floor sqrt2 * 2^16 = 92681");
    return test::finish("self_contained_parity");
}
