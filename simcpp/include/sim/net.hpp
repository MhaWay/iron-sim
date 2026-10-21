// sim::net — lockstep transport (P2P ring exchange + dedicated relay).
// Transport/packaging only (docs/cpp/networking.md): may use sockets; NEVER the sim.
// Commands moved on the wire are canonical; both sides apply identical sorted batches,
// so both reach the identical final hash. No threads/mutex/atomic — cooperative loop.
#pragma once
#include <cstdint>
#include <cstddef>
#include <vector>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#include <cerrno>
#include "sim/rts_shared.hpp"
#include "sim/engine.hpp"

namespace sim::net {

using Command = rts::Command;
using PlayerCommand = rts::PlayerCommand;
using TickCommands = rts::TickCommands;
using Kind = rts::Kind;

inline void put_u8(std::vector<uint8_t>& w, uint8_t v) { w.push_back(v); }
inline void put_u16(std::vector<uint8_t>& w, uint16_t v) { for (int i = 0; i < 2; ++i) w.push_back(static_cast<uint8_t>(v >> (8*i))); }
inline void put_u32(std::vector<uint8_t>& w, uint32_t v) { for (int i = 0; i < 4; ++i) w.push_back(static_cast<uint8_t>(v >> (8*i))); }
inline void put_u64(std::vector<uint8_t>& w, uint64_t v) { for (int i = 0; i < 8; ++i) w.push_back(static_cast<uint8_t>(v >> (8*i))); }
inline void put_i16(std::vector<uint8_t>& w, int16_t v) { put_u16(w, static_cast<uint16_t>(v)); }

inline void encode_command(std::vector<uint8_t>& w, const Command& c) {
    using K = Kind;
    put_u8(w, static_cast<uint8_t>(c.kind));
    switch (c.kind) {
        case K::Move:    put_u32(w, c.a); put_i16(w, c.b); put_i16(w, c.c); break;
        case K::Attack:  put_u32(w, c.a); put_u16(w, static_cast<uint16_t>(c.b)); break;
        case K::Stop:    put_u32(w, c.a); break;
        case K::Build:   put_u16(w, static_cast<uint16_t>(c.a)); put_i16(w, c.b); put_i16(w, c.c); break;
        case K::Train:   put_u32(w, c.a); put_u16(w, static_cast<uint16_t>(c.b)); break;
        case K::Gather:  put_u32(w, c.a); put_u8(w, static_cast<uint8_t>(c.b)); break;
        case K::Sacrifice: put_u32(w, c.a); break;
        case K::Evolve:  put_u8(w, static_cast<uint8_t>(c.b)); put_u16(w, static_cast<uint16_t>(c.a)); break;
        case K::AgeUp:   put_u8(w, static_cast<uint8_t>(c.a)); break;
    }
}

inline void encode_tickcommands(std::vector<uint8_t>& w, const TickCommands& tc) {
    put_u64(w, tc.tick);
    put_u32(w, static_cast<uint32_t>(tc.cmds.size()));
    for (const auto& pc : tc.cmds) {
        put_u32(w, pc.player);
        encode_command(w, pc.cmd);
    }
}

struct Reader {
    const uint8_t* p; const uint8_t* end;
    bool ok = true;
    uint8_t u8() { if (p+1>end){ok=false;return 0;} return *p++; }
    uint16_t u16() { if (p+2>end){ok=false;return 0;} uint16_t v=0; for(int i=0;i<2;++i) v |= uint16_t(p[i])<<(8*i); p+=2; return v; }
    int16_t i16() { return static_cast<int16_t>(u16()); }
    uint32_t u32() { if (p+4>end){ok=false;return 0;} uint32_t v=0; for(int i=0;i<4;++i) v |= uint32_t(p[i])<<(8*i); p+=4; return v; }
    uint64_t u64() { if (p+8>end){ok=false;return 0;} uint64_t v=0; for(int i=0;i<8;++i) v |= uint64_t(p[i])<<(8*i); p+=8; return v; }
};

inline Command decode_command(Reader& r) {
    using K = Kind;
    Command c{};
    const auto k = static_cast<K>(r.u8());
    c.kind = k;
    switch (k) {
        case K::Move: c.a=r.u32(); c.b=r.i16(); c.c=r.i16(); break;
        case K::Attack: c.a=r.u32(); c.b=static_cast<int16_t>(r.u16()); break;
        case K::Stop: c.a=r.u32(); break;
        case K::Build: c.a=r.u16(); c.b=r.i16(); c.c=r.i16(); break;
        case K::Train: c.a=r.u32(); c.b=static_cast<int16_t>(r.u16()); break;
        case K::Gather: c.a=r.u32(); c.b=r.u8(); break;
        case K::Sacrifice: c.a=r.u32(); break;
        case K::Evolve: c.b=r.u8(); c.a=r.u16(); break;
        case K::AgeUp: c.a=r.u8(); break;
    }
    return c;
}

inline TickCommands decode_tickcommands(const std::vector<uint8_t>& w) {
    Reader r{w.data(), w.data()+w.size(), true};
    TickCommands tc;
    tc.tick = r.u64();
    const uint32_t n = r.u32();
    for (uint32_t i = 0; i < n && r.ok; ++i) {
        PlayerCommand pc; pc.player = r.u32(); pc.cmd = decode_command(r);
        if (!r.ok) break;
        tc.cmds.push_back(pc);
    }
    return tc;
}

// UDP loopback socket; cooperative (no threads).
class UdpPort {
public:
    bool open(uint16_t port) {
        fd_ = ::socket(AF_INET, SOCK_DGRAM, 0);
        if (fd_ < 0) return false;
        int on = 1;
        ::setsockopt(fd_, SOL_SOCKET, SO_REUSEADDR, &on, sizeof on);
        sockaddr_in a{};
        a.sin_family = AF_INET;
        a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        a.sin_port = htons(port);
        if (::bind(fd_, reinterpret_cast<sockaddr*>(&a), sizeof a) != 0) return false;
        return true;
    }
    UdpPort() = default;
    ~UdpPort() { if (fd_ >= 0) ::close(fd_); }
    UdpPort(const UdpPort&) = delete;
    UdpPort& operator=(const UdpPort&) = delete;
    void send_to(uint16_t peer_port, const std::vector<uint8_t>& wire) {
        sockaddr_in a{};
        a.sin_family = AF_INET;
        a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        a.sin_port = htons(peer_port);
        ::sendto(fd_, wire.data(), wire.size(), 0, reinterpret_cast<sockaddr*>(&a), sizeof a);
    }
    bool recv_nonblock(std::vector<uint8_t>& out) {
        uint8_t buf[2048];
        sockaddr_in from{};
        socklen_t fl = sizeof from;
        const ssize_t n = ::recvfrom(fd_, buf, sizeof buf, MSG_DONTWAIT, reinterpret_cast<sockaddr*>(&from), &fl);
        if (n > 0) { out.assign(buf, buf + n); return true; }
        return false;
    }
private:
    int fd_ = -1;
};

} // namespace sim::net
