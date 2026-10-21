#pragma once
// Integer square root, floor, integer-only — mirrors engine_core::isqrt_u128
// (bit-pair radix-4 digit-by-digit extraction) and fixed_sqrt bit-for-bit.
// Portable 128-bit arithmetic via the u128_t struct. NO floats, no globals.
#include <cstdint>

namespace engine {

namespace detail {
using u128 = unsigned __int128;

constexpr int clzll(uint64_t x) noexcept {
    int n = 0;
    if (!(x & 0xFFFFFFFF00000000ULL)) { n += 32; x <<= 32; }
    if (!(x & 0xF000000000000000ULL)) { n += 4; x <<= 4; }
    if (!(x & 0xC000000000000000ULL)) { n += 2; x <<= 2; }
    if (!(x & 0x8000000000000000ULL)) { n += 1; }
    return n + (x == 0 ? 1 : 0);
}
constexpr int clz_u128(u128 x) noexcept {
    if (x >> 64) return clzll(static_cast<uint64_t>(x >> 64));
    return 64 + clzll(static_cast<uint64_t>(x));
}
constexpr u128 shl128(u128 v, int b) noexcept {
    if (b <= 0) return v;
    if (b >= 128) return 0;
    return (b < 64) ? (v << b) : (v << (b - 64)) << 64;
}
constexpr int leading_zeros128(u128 x) noexcept {
    return x == 0 ? 128 : clz_u128(x);
}

constexpr u128 isqrt_u128(u128 n) noexcept {
    if (n <= 1) return n;
    const int bits = 128 - leading_zeros128(n);
    u128 bit = u128{1} << ((((bits - 1) >> 1) * 2));
    if (bit > n) bit >>= 2;
    u128 q = 0, rem = n;
    while (bit) {
        if (rem >= q + bit) {
            rem -= q + bit;
            q = (q >> 1) + bit;
        } else {
            q >>= 1;
        }
        bit >>= 2;
    }
    return q;
}

} // namespace detail
} // namespace engine
