#pragma once
// Tiny test harness + vector loading helpers for the engine_cpp parity suite.
#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include "json.hpp"

namespace test {

inline int g_failures = 0;
inline int g_checks = 0;

inline void check(bool cond, const char* msg) noexcept {
    ++g_checks;
    if (!cond) {
        ++g_failures;
        std::printf("FAIL: %s\n", msg);
    }
}

inline void check_u64(uint64_t got, uint64_t want, const char* msg) noexcept {
    ++g_checks;
    if (got != want) {
        ++g_failures;
        std::printf("FAIL: %s: got %016llx want %016llx\n",
                    msg, static_cast<unsigned long long>(got),
                    static_cast<unsigned long long>(want));
    }
}

inline void check_i64(int64_t got, int64_t want, const char* msg) noexcept {
    ++g_checks;
    if (got != want) {
        ++g_failures;
        std::printf("FAIL: %s: got %lld want %lld\n",
                    msg, static_cast<long long>(got), static_cast<long long>(want));
    }
}

inline std::string read_file(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) {
        std::printf("FAIL: cannot open %s\n", path.c_str());
        ++g_failures;
        return {};
    }
    std::ostringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

inline jsonlite::Value load_vectors(const std::string& dir, const std::string& name) {
    const std::string path = dir + "/" + name + ".json";
    auto v = jsonlite::parse(read_file(path));
    if (!v) {
        std::printf("FAIL: cannot parse %s\n", path.c_str());
        ++g_failures;
        return jsonlite::Value{};
    }
    return *v;
}

// Parse a 16-hex-digit string into uint64.
inline uint64_t hex16(const std::string& s) noexcept {
    uint64_t x = 0;
    for (char c : s) {
        x <<= 4;
        if (c >= '0' && c <= '9') x |= static_cast<uint64_t>(c - '0');
        else if (c >= 'a' && c <= 'f') x |= static_cast<uint64_t>(c - 'a' + 10);
        else if (c >= 'A' && c <= 'F') x |= static_cast<uint64_t>(c - 'A' + 10);
    }
    return x;
}

inline int finish(const char* suite) {
    if (g_failures == 0) {
        std::printf("PASS %s: %d checks, 0 failures\n", suite, g_checks);
        return 0;
    }
    std::printf("FAILED %s: %d failures of %d checks\n", suite, g_failures, g_checks);
    return 1;
}

} // namespace test
