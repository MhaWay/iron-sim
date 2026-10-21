// Parity: byte-by-byte FNV-1a (shared::Hasher) vs Rust oracle.
#include <cstdint>
#include <vector>
#include "engine/hash.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "fnv_bytes");
    const auto* streams = jsonlite::find(doc, "byte_streams");
    test::check(streams != nullptr && streams->is_arr(), "fnv: byte_streams");
    if (!streams) return 1;
    for (const auto& entry : streams->arr()) {
        const auto* bytes = jsonlite::find(entry, "bytes");
        const auto* digest = jsonlite::find(entry, "digest");
        if (!bytes || !digest) { test::check(false, "fnv: fields"); continue; }
        std::vector<uint8_t> b;
        for (const auto& x : bytes->arr()) b.push_back(static_cast<uint8_t>(x.i64()));
        engine::Hasher h;
        h.bytes(b.data(), b.size());
        test::check_u64(h.finish(), test::hex16(digest->str()), "fnv1a digest");
    }
    // known vectors (mirror of Rust fnv1a_known_vectors test)
    test::check_u64(engine::Hasher().finish(), 0xcbf29ce484222325ULL, "empty");
    engine::Hasher ha; ha.u8('a');
    test::check_u64(ha.finish(), 0xaf63dc4c8601ec8cULL, "'a'");
    engine::Hasher habc; habc.u8('a'); habc.u8('b'); habc.u8('c');
    test::check_u64(habc.finish(), 0xe71fa2190541574bULL, "'abc'");
    return test::finish("fnv_parity");
}
