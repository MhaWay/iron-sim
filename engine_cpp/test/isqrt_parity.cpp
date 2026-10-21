// Parity: isqrt_u128 (portable 128-bit) and fixed_sqrt vs engine_core oracle.
#include <cstdint>
#include "engine/isqrt.hpp"
#include "engine/fixed1616.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "isqrt_vectors");

    if (const auto* list = jsonlite::find(doc, "isqrt_u128"); list && list->is_arr()) {
        for (const auto& e : list->arr()) {
            const std::string& nhex = jsonlite::find(e, "n")->str();
            const std::string& rhex = jsonlite::find(e, "root")->str();
            // parse 32-hex-digit n as 128-bit
            engine::detail::u128 n = 0;
            for (char c : nhex) {
                n <<= 4;
                n |= static_cast<engine::detail::u128>(c <= '9' ? c - '0'
                                 : (c | 0x20) - 'a' + 10);
            }
            const uint64_t want = test::hex16(rhex);
            const engine::detail::u128 got = engine::detail::isqrt_u128(n);
            test::check(static_cast<uint64_t>(got >> 64) == 0, "isqrt root fits 64 bits");
            test::check_u64(static_cast<uint64_t>(got), want, "isqrt_u128");
        }
    }
    if (const auto* list = jsonlite::find(doc, "fixed_sqrt"); list && list->is_arr()) {
        for (const auto& e : list->arr()) {
            const int64_t raw = jsonlite::find(e, "raw")->i64();
            const int64_t want = jsonlite::find(e, "result_raw")->i64();
            // fixed_sqrt(v) = isqrt(v<<16) for v>0 else 0 — mirrors Rust.
            int64_t got = 0;
            if (raw > 0) {
                const engine::detail::u128 shifted =
                    (static_cast<engine::detail::u128>(static_cast<uint64_t>(raw)) << 16);
                const engine::detail::u128 r = engine::detail::isqrt_u128(shifted);
                got = static_cast<int64_t>(static_cast<uint64_t>(r));
            }
            test::check_i64(got, want, "fixed_sqrt");
        }
    }
    return test::finish("isqrt_parity");
}
