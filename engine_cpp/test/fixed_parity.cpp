// Parity: Fixed1616 arithmetic vs engine_core::Fixed1616. Exercises BOTH the
// shipped API (engine::) and the test-only mirror (tf::); both must agree and
// both must match the Rust testdata.
#include <cstdint>
#include "engine/fixed1616.hpp"
#include "fixed1616_mirror.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "fixed_values");

    if (const auto* fr = jsonlite::find(doc, "from_rational"); fr && fr->is_arr()) {
        for (const auto& e : fr->arr()) {
            const int64_t n = jsonlite::find(e, "n")->i64();
            const int64_t d = jsonlite::find(e, "d")->i64();
            const int64_t want = jsonlite::find(e, "raw")->i64();
            const int64_t shipped = engine::Fixed1616::from_rational(n, d).v;
            const int64_t mirror = tf::Fixed::from_rational(n, d).v;
            test::check_i64(shipped, want, "from_rational shipped");
            test::check_i64(mirror, want, "from_rational mirror");
        }
    }
    if (const auto* md = jsonlite::find(doc, "mul_div"); md && md->is_arr()) {
        for (const auto& e : md->arr()) {
            const engine::Fixed1616 ax{jsonlite::find(e, "a_raw")->i64()};
            const engine::Fixed1616 bx{jsonlite::find(e, "b_raw")->i64()};
            const int64_t wantMul = jsonlite::find(e, "mul_raw")->i64();
            const int64_t wantDiv = jsonlite::find(e, "div_raw")->i64();
            test::check_i64(ax.wrapping_mul(bx).v, wantMul, "wrapping_mul shipped");
            test::check_i64(tf::Fixed::wrapping_mul({ax.v}, {bx.v}).v, wantMul, "wrapping_mul mirror");
            test::check_i64(engine::Fixed1616::wrapping_div(ax, bx).v, wantDiv, "wrapping_div shipped");
            test::check_i64(tf::Fixed::wrapping_div({ax.v}, {bx.v}).v, wantDiv, "wrapping_div mirror");
        }
    }
    // fixed invariants from the Rust test suite
    test::check_i64(engine::Fixed1616::from_rational(1, 2).v, 1 << 15, "1/2");
    test::check_i64(engine::Fixed1616::from_rational(-1, 2).v, -(1 << 15), "-1/2");
    test::check_i64(engine::Fixed1616::from_rational(-1, 1).v, -65536, "-1/1");
    test::check_i64(engine::Fixed1616::from_int(3).wrapping_mul(engine::Fixed1616::one()).v,
                    engine::Fixed1616::from_int(3).v, "3*1");
    test::check(engine::Fixed1616::from_int(-1).to_int_trunc() == -1, "trunc -1");
    test::check(engine::Fixed1616::from_rational(-3, 2).to_int_trunc() == -1, "trunc -3/2");
    return test::finish("fixed_parity");
}
