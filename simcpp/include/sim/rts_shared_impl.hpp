#pragma once
// Inline definitions of sort_key / hash_into. Command/Kind/Hasher must already be declared.
namespace sim::rts {
inline SortKey sort_key(const Command& c) noexcept {
    switch (c.kind) {
        case Kind::Move: return {static_cast<int32_t>(c.a), 0, c.b, c.c};
        case Kind::Attack: return {static_cast<int32_t>(c.a), 1, static_cast<int32_t>(static_cast<uint16_t>(c.b)), 0};
        case Kind::Stop: return {static_cast<int32_t>(c.a), 2, 0, 0};
        case Kind::Build: return {static_cast<int32_t>(c.a), 3, c.b, c.c};
        case Kind::Train: return {static_cast<int32_t>(c.a), 4, static_cast<int32_t>(static_cast<uint16_t>(c.b)), 0};
        case Kind::Gather: return {static_cast<int32_t>(c.a), 5, static_cast<int32_t>(static_cast<uint8_t>(c.b)), 0};
        case Kind::Sacrifice: return {static_cast<int32_t>(c.a), 6, 0, 0};
        case Kind::Evolve: return {static_cast<int32_t>(c.a), 7, static_cast<int32_t>(static_cast<uint8_t>(c.b)), 0};
        case Kind::AgeUp: return {static_cast<int32_t>(c.a), 8, 0, 0};
    }
    return {};
}

inline void hash_into(const Command& c, Hasher& h) noexcept {
    switch (c.kind) {
        case Kind::Move: h.u8(0); h.u32(c.a); h.i16(c.b); h.i16(c.c); break;
        case Kind::Attack: h.u8(1); h.u32(c.a); h.u32(static_cast<uint16_t>(c.b)); break;
        case Kind::Stop: h.u8(2); h.u32(c.a); break;
        case Kind::Build: h.u8(3); h.u16(static_cast<uint16_t>(c.a)); h.i16(c.b); h.i16(c.c); break;
        case Kind::Train: h.u8(4); h.u32(c.a); h.u16(static_cast<uint16_t>(c.b)); break;
        case Kind::Gather: h.u8(5); h.u32(c.a); h.u8(static_cast<uint8_t>(c.b)); break;
        case Kind::Sacrifice: h.u8(6); h.u32(c.a); break;
        case Kind::Evolve: h.u8(7); h.u8(static_cast<uint8_t>(c.b)); h.u16(static_cast<uint16_t>(c.a)); break;
        case Kind::AgeUp: h.u8(8); h.u8(static_cast<uint8_t>(c.a)); break;
    }
}
} // namespace sim::rts