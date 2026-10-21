// Parity: Scheduler enqueue/step consumption trace vs engine_core::Scheduler.
#include <cstdint>
#include <vector>
#include "engine/command.hpp"
#include "engine/scheduler.hpp"
#include "tests_common.hpp"

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "scheduler_trace");
    const auto* trace = jsonlite::find(doc, "trace");
    test::check(trace != nullptr && trace->is_arr(), "scheduler: trace array");
    if (!trace) return 1;

    // Replay the exact Rust enqueue sequence, then step(1), step(2), step(5)
    // and compare (tick, payload) in consumption order.
    engine::Scheduler s;
    std::vector<std::pair<uint64_t, uint64_t>> got;
    auto rec = [&got](engine::Tick t, const engine::Command& c) {
        got.emplace_back(t, c.payload_hash);
    };
    s.enqueue(1, engine::Command::from_hash(11));
    s.enqueue(1, engine::Command::from_hash(22));
    s.enqueue(0, engine::Command::from_hash(5));
    s.enqueue(3, engine::Command::from_hash(33));
    s.enqueue(2, engine::Command::from_hash(20));
    s.step(1, rec);
    s.step(2, rec);
    s.step(5, rec);

    test::check(got.size() == trace->arr().size(), "scheduler trace length");
    for (std::size_t i = 0; i < trace->arr().size(); ++i) {
        const auto& e = trace->arr()[i];
        const uint64_t tW = static_cast<uint64_t>(jsonlite::find(e, "tick")->i64());
        const uint64_t pW = test::hex16(jsonlite::find(e, "payload")->str());
        if (i < got.size()) {
            test::check(got[i].first == tW, "scheduler trace tick");
            test::check_u64(got[i].second, pW, "scheduler trace payload");
        }
    }
    // current() advanced correctly
    test::check(s.current() == 5, "scheduler current after step(5)");
    return test::finish("scheduler_parity");
}
