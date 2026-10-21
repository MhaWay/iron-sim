#pragma once
// Test-only Fixed1616 mirror: independent re-implementation of
// engine_core::Fixed1616 semantics via portable __int128 floor division, used
// to cross-check the shipped API in fixed_parity.cpp. NOT part of the sim API.
// (docs/cpp/determinism-cpp.md: overflow + floor semantics are explicit.)
#include <cstdint>

namespace tf {

constexpr int64_t kS = int64_t{1} << 16;

struct Fixed {
    int64_t v{0};
    static constexpr Fixed from_int(int32_t i) noexcept { return Fixed{static_cast<int64_t>(i) * kS}; }
    static Fixed from_rational(int64_t n, int64_t d) noexcept {
        const __int128 A = static_cast<__int128>(n) << 16;
        const __int128 B = d;
        __int128 q = A / B;
        const __int128 r = A % B;
        if (r != 0 && ((r < 0) != (B < 0))) q -= 1;
        return Fixed{static_cast<int64_t>(q)};
    }
    static Fixed wrapping_div(Fixed x, Fixed r) noexcept {
        const __int128 A = static_cast<__int128>(x.v) << 16;
        const __int128 B = r.v;
        __int128 q = A / B;
        const __int128 rem = A % B;
        if (rem != 0 && ((rem < 0) != (B < 0))) q -= 1;
        return Fixed{static_cast<int64_t>(q)};
    }
    static Fixed wrapping_mul(Fixed x, Fixed r) noexcept {
        const __int128 p = static_cast<__int128>(x.v) * static_cast<__int128>(r.v);
        return Fixed{static_cast<int64_t>(p >> 16)};
    }
};

constexpr bool operator==(Fixed a, Fixed b) noexcept { return a.v == b.v; }
constexpr Fixed operator*(Fixed a, Fixed b) noexcept { return Fixed::wrapping_mul(a, b); }
constexpr Fixed operator/(Fixed a, Fixed b) noexcept { return Fixed::wrapping_div(a, b); }
} // namespace tf
