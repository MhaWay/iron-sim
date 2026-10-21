#pragma once
// Canonical FNV-1a surfaces: engine_core::hash_fold/hash_words and the
// byte-by-byte shared::Hasher. NO float, NO std::hash.
#include <cstddef>
#include <cstdint>

namespace engine {

constexpr uint64_t kFnvOffsetBasis = 0xcbf29ce484222325ULL;
constexpr uint64_t kFnvPrime = 0x100001b3ULL;
constexpr uint64_t kFnvBytePrime = 0x100000001b3ULL;

constexpr uint64_t hash_fold(uint64_t acc, uint64_t word) noexcept {
    const uint64_t m = (acc ^ word) * kFnvPrime;
    return (m << 11) | (m >> 53); // rotate_left(11)
}

constexpr uint64_t hash_words(const uint64_t* w, std::size_t n) noexcept {
    uint64_t h = kFnvOffsetBasis;
    for (std::size_t i = 0; i < n; ++i) h = hash_fold(h, w[i]);
    return h;
}

class Hasher {
public:
    Hasher() noexcept : acc_(kFnvOffsetBasis) {}

    void byte(uint8_t b) noexcept {
        acc_ ^= static_cast<uint64_t>(b);
        acc_ *= kFnvBytePrime;
    }
    void bytes(const uint8_t* p, std::size_t n) noexcept {
        for (std::size_t i = 0; i < n; ++i) byte(p[i]);
    }
    void write_bytes(const uint8_t* p, std::size_t n) noexcept { bytes(p, n); }

    void u8(uint8_t v) noexcept { byte(v); }
    void u16(uint16_t v) noexcept {
        const uint8_t b[2] = {static_cast<uint8_t>(v), static_cast<uint8_t>(v >> 8)};
        bytes(b, 2);
    }
    void u32(uint32_t v) noexcept {
        const uint8_t b[4] = {static_cast<uint8_t>(v), static_cast<uint8_t>(v >> 8),
                              static_cast<uint8_t>(v >> 16), static_cast<uint8_t>(v >> 24)};
        bytes(b, 4);
    }
    void u64(uint64_t v) noexcept {
        uint8_t b[8];
        for (int i = 0; i < 8; ++i) b[i] = static_cast<uint8_t>(v >> (8 * i));
        bytes(b, 8);
    }
    void i16(int16_t v) noexcept { u16(static_cast<uint16_t>(v)); }
    void i32(int32_t v) noexcept { u32(static_cast<uint32_t>(v)); }
    void i64(int64_t v) noexcept { u64(static_cast<uint64_t>(v)); }

    uint64_t finish() const noexcept { return acc_; }

private:
    uint64_t acc_;
};

} // namespace engine
