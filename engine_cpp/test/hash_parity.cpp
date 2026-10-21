// Parity: engine_core::hash_fold / hash_words vs Rust.
#include <cstdint>
#include <vector>
#include "engine/hash.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "hash_streams");
    const auto* fold = jsonlite::find(doc, "fold");
    test::check(fold != nullptr && fold->is_arr(), "hash_streams: fold array");
    if (!fold) return 1;
    for (const auto& entry : fold->arr()) {
        const auto* words = jsonlite::find(entry, "words");
        const auto* chain = jsonlite::find(entry, "fold_chain");
        const auto* hw = jsonlite::find(entry, "hash_words");
        if (!words || !chain || !hw) { test::check(false, "hash_streams: fields"); continue; }
        std::vector<uint64_t> w;
        for (const auto& x : words->arr()) w.push_back(test::hex16(x.str()));
        const auto& folds = chain->arr();
        uint64_t acc = 0;
        for (std::size_t i = 0; i < w.size(); ++i) {
            acc = engine::hash_fold(acc, w[i]);
            test::check_u64(acc, test::hex16(folds[i].str()), "hash_fold chain");
        }
        test::check_u64(engine::hash_words(w.data(), w.size()), test::hex16(hw->str()),
                        "hash_words");
    }
    // known constants: hash_words([]) == offset basis
    test::check_u64(engine::hash_words(nullptr, 0), 0xcbf29ce484222325ULL, "empty words");
    return test::finish("hash_parity");
}
