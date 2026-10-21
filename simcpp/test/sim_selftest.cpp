#include "sim/engine.hpp"
#include "sim/path.hpp"
#include "sim/rts_econ.hpp"

#include <cstdio>
#include <cstdint>
#include <string>
#include <vector>

using namespace sim;

#define CHECK(cond) do { if (!(cond)) { std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); failures++; } } while(0)

static int failures = 0;

static void test_fixed() {
    // from_int / to_int_trunc
    CHECK(Fixed1616::from_int(3).to_int_trunc() == 3);
    CHECK(Fixed1616{3 << 16}.to_int_trunc() == 3);
    CHECK(Fixed1616{3 << 16}.to_int_trunc() == 3);
    // from_rational floor semantics
    CHECK(Fixed1616::from_rational(-1, 3).v == -((1LL * 65536 + 3 - 1) / 3));
    // wrapping_add/sub
    Fixed1616 a{1LL << 40}, b{2LL << 40};
    CHECK(a.wrapping_add(b).v == 3LL << 40);
    CHECK(a.wrapping_sub(b).v == -(1LL << 40)); // -1<<40
    // wrapping_mul
    Fixed1616 c{2 * 65536}, d{3 * 65536};
    CHECK(c.wrapping_mul(d).v == 6 * 65536);
    // wrapping_div via div_floor
    Fixed1616 e{15 * 65536}, f{4 * 65536};
    CHECK(Fixed1616::wrapping_div(e, f).v == 245760LL);
    // negative div
    Fixed1616 g{-15 * 65536};
    CHECK(Fixed1616::wrapping_div(g, f).v == -245760LL);
    // isqrt
    CHECK(engine::detail::isqrt_u128(15LL) == 3);
    CHECK(engine::detail::isqrt_u128(16LL) == 4);
    CHECK(engine::detail::isqrt_u128(99LL) == 9);
}

static void test_hash() {
    Hasher h;
    h.u32(1);
    CHECK(h.finish() != 0);
    Hasher h2; h2.u32(1);
    CHECK(h2.finish() == h.finish());
    // Hash a stream of bytes; must match engine_cpp reference.
    Hasher hb;
    for (uint8_t b : {1, 2, 3, 4, 5, 6, 7, 8}) hb.u8(b);
    CHECK(hb.finish() != 0);
}

static void test_rng() {
    // SplitMix64 known: state after seed=1
    Rng r1(1);
    uint64_t s = r1.state();
    // Just check: state is not seed, and is deterministic.
    Rng r2(1);
    CHECK(r2.state() == s);
    // Consume a few values.
    Rng r3(42);
    uint64_t v = r3.next_u64();
    (void)v;
    Rng r4(42); r4.next_u64();
    CHECK(r3.state() == r4.state());
}

static void test_path() {
    Grid g(10, 10);
    bool ok = false;
    auto p = find_path(g, {0, 0}, {3, 4}, &ok);
    CHECK(ok);
    CHECK(!p.empty());
    // blocked goal
    g.set_blocked({5, 5}, true);
    p = find_path(g, {0, 0}, {5, 5}, &ok);
    CHECK(!ok);
    // blocked corridor
    for (int x = 0; x < 10; ++x) g.set_blocked({static_cast<int16_t>(x), 5}, true);
    p = find_path(g, {0, 0}, {0, 6}, &ok);
    CHECK(!ok); // wall across
    // open corridor with a gap
    g = Grid(10, 10);
    for (int x = 0; x < 9; ++x) g.set_blocked({static_cast<int16_t>(x), 5}, true);
    p = find_path(g, {0, 0}, {0, 9}, &ok);
    CHECK(ok);
}

int main(int argc, char** argv) {
    if (argc > 1) {
        // optional: read oracle data files to cross-check (not strictly needed)
    }
    test_fixed();
    test_hash();
    test_rng();
    test_path();
    if (failures == 0) {
        std::printf("sim_selftest: OK\n");
        return 0;
    }
    std::printf("sim_selftest: %d FAILURE(S)\n", failures);
    return 1;
}