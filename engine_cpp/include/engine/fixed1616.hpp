#pragma once
// Fixed1616 — Q16.16 fixed-point, int64 raw. Mirrors engine_core::Fixed1616
// bit-for-bit. NO floating point (CI grep gate, docs/cpp/determinism-cpp.md).
#include <cstdint>

namespace engine {

constexpr int64_t kFixedS = int64_t{1} << 16;

// Euclidean floor division (floor semantics like Rust div_floor; C++ '/' is
// truncating since C++11 so the fixup below is mandatory for negatives).
constexpr int64_t div_floor(int64_t a, int64_t b) noexcept {
    const int64_t q = a / b;
    const int64_t r = a % b;
    return ((r != 0) && ((r < 0) != (b < 0))) ? q - 1 : q;
}

struct Fixed1616 {
    int64_t v{0};

    static constexpr int64_t S = kFixedS;
    static constexpr Fixed1616 one() noexcept { return Fixed1616{S}; }
    static constexpr Fixed1616 zero() noexcept { return Fixed1616{0}; }

    static constexpr Fixed1616 from_int(int32_t i) noexcept {
        return Fixed1616{static_cast<int64_t>(i) * S};
    }

    /// floor(n/d * S) — Euclidean floor; matches Rust from_rational.
    static Fixed1616 from_rational(int64_t n, int64_t d) noexcept {
        const __int128 A = static_cast<__int128>(n) << 16;
        const __int128 B = d;
        __int128 q = A / B, r = A % B;
        if (r != 0 && ((r < 0) != (B < 0))) q -= 1;
        return Fixed1616{static_cast<int64_t>(q)};
    }

    constexpr int32_t to_int_trunc() const noexcept {
        return static_cast<int32_t>(v / S);
    }

    constexpr Fixed1616 wrapping_add(Fixed1616 r) const noexcept {
        return Fixed1616{static_cast<int64_t>(
            static_cast<uint64_t>(v) + static_cast<uint64_t>(r.v))};
    }
    constexpr Fixed1616 wrapping_sub(Fixed1616 r) const noexcept {
        return Fixed1616{static_cast<int64_t>(
            static_cast<uint64_t>(v) - static_cast<uint64_t>(r.v))};
    }
    /// i128 product, arithmetic >>16, low-64-bits wrap (matches Rust).
    constexpr Fixed1616 wrapping_mul(Fixed1616 r) const noexcept {
        const __int128 p = static_cast<__int128>(v) * static_cast<__int128>(r.v);
        return Fixed1616{static_cast<int64_t>(p >> 16)};
    }
    static Fixed1616 wrapping_div(Fixed1616 x, Fixed1616 r) noexcept {
        const __int128 A = static_cast<__int128>(x.v) << 16;
        const __int128 B = static_cast<__int128>(r.v);
        __int128 q = A / B, rem = A % B;
        if (rem != 0 && ((rem < 0) != (B < 0))) q -= 1;
        return Fixed1616{static_cast<int64_t>(q)};
    }
    constexpr Fixed1616 wrapping_abs() const noexcept {
        if (v == INT64_MIN) return Fixed1616{INT64_MIN}; // documented UB guard
        return v < 0 ? Fixed1616{-v} : *this;
    }
    constexpr Fixed1616 saturating_add(Fixed1616 r) const noexcept {
        const int64_t a = v, b = r.v;
        if (b > 0 && a > INT64_MAX - b) return Fixed1616{INT64_MAX};
        if (b < 0 && a < INT64_MIN - b) return Fixed1616{INT64_MIN};
        return Fixed1616{a + b};
    }
    constexpr bool is_zero() const noexcept { return v == 0; }
    constexpr int64_t inner() const noexcept { return v; }
    constexpr Fixed1616 mul_floor(Fixed1616 r) const noexcept {
        return wrapping_mul(r);
    }
};

constexpr bool operator==(Fixed1616 a, Fixed1616 b) noexcept { return a.v == b.v; }
constexpr bool operator!=(Fixed1616 a, Fixed1616 b) noexcept { return a.v != b.v; }
constexpr bool operator<(Fixed1616 a, Fixed1616 b) noexcept { return a.v < b.v; }
constexpr bool operator<=(Fixed1616 a, Fixed1616 b) noexcept { return a.v <= b.v; }
constexpr bool operator>(Fixed1616 a, Fixed1616 b) noexcept { return a.v > b.v; }
constexpr bool operator>=(Fixed1616 a, Fixed1616 b) noexcept { return a.v >= b.v; }
constexpr Fixed1616 operator+(Fixed1616 a, Fixed1616 b) noexcept { return a.wrapping_add(b); }
constexpr Fixed1616 operator-(Fixed1616 a, Fixed1616 b) noexcept { return a.wrapping_sub(b); }
constexpr Fixed1616 operator*(Fixed1616 a, Fixed1616 b) noexcept { return a.wrapping_mul(b); }
constexpr Fixed1616 operator-(Fixed1616 a) noexcept { return Fixed1616{-a.v}; }

} // namespace engine
