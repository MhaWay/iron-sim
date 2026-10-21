// Event/Tick/Command identity + sort_key sanity vs the Rust contract.
#include <cstdint>
#include "engine/command.hpp"
#include "engine/command_game.hpp"
#include "engine/tick.hpp"
#include "tests_common.hpp"

int main(int, char**) {
    using engine::game::Command;
    using engine::game::Kind;

    // Tick is uint64 (fixed-rate policy is the game layer's).
    test::check(sizeof(engine::Tick) == sizeof(uint64_t), "tick is u64");

    // Command identity: from_hash/hash round-trip.
    auto c = engine::Command::from_hash(0xDEADBEEFULL);
    test::check_u64(c.hash(), 0xDEADBEEFULL, "command hash");

    // kind_disc frozen (design-factions §6): Move=0..AgeUp=8.
    const uint8_t want_disc[9] = {0, 1, 2, 3, 4, 5, 6, 7, 8};
    const Command cmds[9] = {
        {Kind::Move, 1, 0, 0}, {Kind::Attack, 1, 0, 0}, {Kind::Stop, 1, 0, 0},
        {Kind::Build, 1, 0, 0}, {Kind::Train, 1, 0, 0}, {Kind::Gather, 1, 0, 0},
        {Kind::Sacrifice, 1, 0, 0}, {Kind::Evolve, 1, 255, 0}, {Kind::AgeUp, 1, 255, 0},
    };
    for (int i = 0; i < 9; ++i)
        test::check(cmds[i].kind_disc() == want_disc[i], "frozen kind_disc");

    // sort_key total order: same entity, disc ordering Build(3) < Train(4) < Gather(5)
    const Command b{Kind::Build, 1, 5, 5}, tr{Kind::Train, 1, 0, 0}, g{Kind::Gather, 1, 1, 0};
    const auto sk_b = b.sort_key(), sk_tr = tr.sort_key(), sk_g = g.sort_key();
    test::check(sk_b.e == 1 && sk_b.d == 3, "build sort_key");
    test::check(sk_tr.d == 4 && sk_g.d == 5, "train<gather disc");
    test::check((sk_b.d < sk_tr.d) && (sk_tr.d < sk_g.d), "sort order total");

    // Event struct
    engine::Event ev{42, 7, 0x123};
    test::check(ev.tick == 42 && ev.id == 7 && ev.payload_hash == 0x123, "event fields");
    return test::finish("event_tick_parity");
}
