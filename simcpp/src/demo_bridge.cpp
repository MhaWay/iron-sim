#include "sim/demo.hpp"
#include "sim/rts_sim.hpp"
#include "sim/rts_econ.hpp"
#include <cstring>
#include <cstdio>
#include <algorithm>

namespace sim::demo {

static uint32_t crc32(const uint8_t* d, size_t n) {
    static uint32_t table[256] = {0};
    static bool init = false;
    if (!init) {
        for (uint32_t i = 0; i < 256; ++i) {
            uint32_t c = i;
            for (int k = 0; k < 8; ++k) c = (c & 1) ? (0xEDB88320u ^ (c >> 1)) : (c >> 1);
            table[i] = c;
        }
        init = true;
    }
    uint32_t c = 0xFFFFFFFFu;
    for (size_t i = 0; i < n; ++i) c = table[(c ^ d[i]) & 0xFFu] ^ (c >> 8);
    return c ^ 0xFFFFFFFFu;
}

static void be32(std::vector<uint8_t>& v, uint32_t x) { v.push_back(x>>24); v.push_back(x>>16); v.push_back(x>>8); v.push_back(x); }
static void chunk(std::vector<uint8_t>& v, const char* type, const std::vector<uint8_t>& data) {
    be32(v, static_cast<uint32_t>(data.size()));
    const size_t start = v.size();
    for (int i = 0; i < 4; ++i) v.push_back(type[i]);
    for (auto b : data) v.push_back(b);
    const uint32_t c = crc32(v.data() + start, v.size() - start);
    be32(v, c);
}

std::vector<uint8_t> encode_png(const Frame& f) {
    std::vector<uint8_t> v;
    const uint8_t sig[8] = {137,80,78,71,13,10,26,10};
    for (auto b : sig) v.push_back(b);
    std::vector<uint8_t> ihdr;
    be32(ihdr, (uint32_t)f.w); be32(ihdr, (uint32_t)f.h);
    ihdr.push_back(8); ihdr.push_back(2); ihdr.push_back(0); ihdr.push_back(0); ihdr.push_back(0);
    chunk(v, "IHDR", ihdr);
    const size_t row = (size_t)f.w * 3 + 1;
    const size_t raw = row * f.h;
    std::vector<uint8_t> raw_src(raw, 0);
    for (int y = 0; y < f.h; ++y) {
        for (int x = 0; x < f.w; ++x) {
            const RGB& c = f.px[(size_t)y * f.w + x];
            const size_t o = (size_t)y * row + 1 + (size_t)x * 3;
            raw_src[o] = c.r; raw_src[o + 1] = c.g; raw_src[o + 2] = c.b;
        }
    }
    std::vector<uint8_t> idat;
    idat.push_back(0x78); idat.push_back(0x01);
    for (size_t off = 0; off < raw; off += 65535) {
        const size_t len = std::min(size_t(65535), raw - off);
        idat.push_back((off + len == raw) ? 1 : 0);
        idat.push_back(len & 0xFF); idat.push_back(len >> 8);
        idat.push_back((~len) & 0xFF); idat.push_back((~len) >> 8);
        idat.insert(idat.end(), raw_src.begin() + (long)off, raw_src.begin() + (long)off + (long)len);
    }
    uint32_t a1 = 1, a2 = 0;
    for (size_t i = 0; i < raw_src.size(); ++i) { a1 = (a1 + raw_src[i]) % 65521u; a2 = (a2 + a1) % 65521u; }
    be32(idat, (a2 << 16) | a1);
    chunk(v, "IDAT", idat);
    chunk(v, "IEND", {});
    return v;
}

bool write_png(const std::string& path, const Frame& f) {
    auto bytes = encode_png(f);
    FILE* fp = std::fopen(path.c_str(), "wb");
    if (!fp) return false;
    fwrite(bytes.data(), 1, bytes.size(), fp);
    std::fclose(fp);
    return true;
}

void render_frame(::sim::rts::Sim& sim, int scale, Frame& out) {
    sim::Grid& g = sim.grid();
    const int gw = g.w(), gh = g.h();
    out.resize(gw * scale, gh * scale);
    for (int ty = 0; ty < gh; ++ty)
        for (int tx = 0; tx < gw; ++tx) {
            RGB c = g.is_blocked({static_cast<int16_t>(tx), static_cast<int16_t>(ty)}) ? RGB{18,18,24} : RGB{56,74,50};
            for (int dy = 0; dy < scale; ++dy) for (int dx = 0; dx < scale; ++dx) out.set(tx*scale+dx, ty*scale+dy, c);
        }

    static const RGB node_tbl[3] = { {220,180,60}, {150,90,50}, {120,140,200} };
    static const RGB owner_tbl[2] = { {90,200,90}, {200,90,90} };
    const auto node_c = [](int slot) { return node_tbl[slot < 3 ? slot : 0]; };
    const auto owner_c = [](int o) { return owner_tbl[o & 1]; };

    sim.world().storages.each<::sim::rts::ResourceNode>([&](sim::Entity e, const rts::ResourceNode& n){
        const ::sim::rts::NodePos* np = sim.world().storages.get<rts::NodePos>(e);
        if (!np) return;
        RGB c = node_c(n.slot % 3);
        out.rect(np->x * scale, np->y * scale, np->x*scale + scale - 1, np->y*scale + scale - 1, c);
    });

    sim.world().storages.each<::sim::rts::Building>([&](sim::Entity e, const rts::Building& b){
        const ::sim::rts::Pos* p = sim.world().storages.get<rts::Pos>(e);
        if (!p) return;
        const rts::BuildingSpec& sp = ::sim::rts::building_spec(b.kind);
        const int cx = p->x.to_int_trunc() * scale + (sp.w * scale)/2;
        const int cy = p->y.to_int_trunc() * scale + (sp.h * scale)/2;
        RGB col = owner_c(b.owner % 2);
        if (b.status == rts::BuildingStatus::Site) col = RGB{120,120,120};
        else if (b.status != rts::BuildingStatus::Done) { col.r/=2; col.g/=2; col.b/=2; }
        out.rect(cx - (sp.w*scale)/2, cy - (sp.h*scale)/2, cx + (sp.w*scale)/2 - 1, cy + (sp.h*scale)/2 - 1, col);
    });

    sim.world().storages.each<::sim::rts::Unit>([&](sim::Entity e, const rts::Unit& u){
        const rts::Pos* p = sim.world().storages.get<rts::Pos>(e);
        if (!p) return;
        const int cx = p->x.to_int_trunc() * scale + scale/2;
        const int cy = p->y.to_int_trunc() * scale + scale/2;
        out.circle(cx, cy, scale/2 + 1, owner_c(u.owner % 2));
    });
}

} // namespace sim::demo