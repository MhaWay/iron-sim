// Parity: game Command / TickCommands sort + hash vs rts-game shared oracle.
#include <cstdint>
#include <vector>
#include "engine/command_game.hpp"
#include "tests_common.hpp"

using engine::game::Command;
using engine::game::TickCommands;

static Command rebuild(const jsonlite::Value& e) {
    Command c;
    c.kind = static_cast<engine::game::Kind>(static_cast<uint8_t>(jsonlite::find(e, "kind")->i64()));
    c.a = static_cast<uint32_t>(static_cast<uint64_t>(jsonlite::find(e, "a")->i64()));
    c.b = static_cast<int16_t>(jsonlite::find(e, "b")->i64());
    c.c = static_cast<int16_t>(jsonlite::find(e, "c")->i64());
    return c;
}

static void run_stream(const jsonlite::Value& s, const char* label) {
    TickCommands t;
    t.tick = static_cast<uint64_t>(jsonlite::find(s, "tick")->i64());
    const auto* lst = jsonlite::find(s, "cmds");
    if (!lst) { test::check(false, label); return; }
    for (const auto& e : lst->arr())
        t.cmds.push_back({static_cast<uint32_t>(static_cast<uint64_t>(jsonlite::find(e, "player")->i64())),
                          rebuild(e)});
    t.sort();
    test::check_u64(t.hash(), test::hex16(jsonlite::find(s, "hash")->str()), label);
}

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : "testdata";
    auto doc = test::load_vectors(dir, "command_hashes");
    const auto* streams = jsonlite::find(doc, "streams");
    test::check(streams != nullptr && streams->is_arr(), "command: streams");
    if (!streams) return 1;
    const auto& arr = streams->arr();
    if (arr.size() >= 1) run_stream(arr[0], "command stream0 hash");
    if (arr.size() >= 2) run_stream(arr[1], "command stream1 hash");
    if (arr.size() >= 3) {
        const auto* per = jsonlite::find(arr[2], "per_command");
        if (per) for (const auto& e : per->arr()) {
            TickCommands t;
            t.tick = 0;  // single-command singleton tick (dumper uses 0)
            t.cmds.push_back({static_cast<uint32_t>(static_cast<uint64_t>(jsonlite::find(e, "player")->i64())),
                              rebuild(e)});
            test::check_u64(t.hash(), test::hex16(jsonlite::find(e, "hash")->str()), "command per-command hash");
        }
    }
    return test::finish("command_parity");
}
