// Parity: ReplayRecorder digests + tick_digest vs engine_core::ReplayRecorder.
#include <cstdint>
#include <vector>
#include "engine/replay.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "replay_digests");

    const auto* streams = jsonlite::find(doc, "streams");
    test::check(streams != nullptr && streams->is_arr(), "replay: streams");
    if (!streams) return 1;
    for (const auto& entry : streams->arr()) {
        const auto* recs = jsonlite::find(entry, "records");
        const auto* digest = jsonlite::find(entry, "digest");
        if (!recs || !digest) { test::check(false, "replay: fields"); continue; }
        engine::ReplayRecorder r;
        for (const auto& rec : recs->arr()) {
            const uint64_t t = static_cast<uint64_t>(jsonlite::find(rec, "tick")->i64());
            const uint64_t p = test::hex16(jsonlite::find(rec, "payload")->str());
            r.record(t, p);
        }
        test::check_u64(r.digest(), test::hex16(digest->str()), "replay digest");
    }
    if (const auto* td = jsonlite::find(doc, "tick_digests"); td && td->is_arr()) {
        for (const auto& e : td->arr()) {
            const uint64_t t = static_cast<uint64_t>(jsonlite::find(e, "tick")->i64());
            std::vector<uint64_t> h;
            for (const auto& x : jsonlite::find(e, "hashes")->arr())
                h.push_back(test::hex16(x.str()));
            test::check_u64(engine::ReplayRecorder::tick_digest(t, h.data(), h.size()),
                            test::hex16(jsonlite::find(e, "digest")->str()), "tick_digest");
        }
    }
    return test::finish("replay_parity");
}
