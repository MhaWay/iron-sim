// sim::demo — software framebuffer renderer + minimal PNG encoder (no GL/SDL).
#pragma once
#include <cstdint>
#include <vector>
#include <string>

namespace sim { namespace rts { class Sim; } }

namespace sim::demo {


struct RGB { uint8_t r, g, b; };

struct Frame {
    int w = 0, h = 0;
    std::vector<RGB> px;
    void resize(int W, int H) { w = W; h = H; px.assign((size_t)W * H, RGB{0, 0, 0}); }
    void clear(RGB c) { for (auto& p : px) p = c; }
    void set(int x, int y, RGB c) { if (x >= 0 && x < w && y >= 0 && y < h) px[(size_t)y * w + x] = c; }
    void rect(int x0, int y0, int x1, int y1, RGB c) { for (int y = y0; y <= y1; ++y) for (int x = x0; x <= x1; ++x) set(x, y, c); }
    void circle(int cx, int cy, int r, RGB c) {
        for (int y = cy - r; y <= cy + r; ++y) for (int x = cx - r; x <= cx + r; ++x)
            if ((x-cx)*(x-cx)+(y-cy)*(y-cy) <= r*r) set(x, y, c);
    }
};

// Encode RGBA framebuffer to PNG bytes (truecolor, no filter, zlib stored blocks).
std::vector<uint8_t> encode_png(const Frame& f);
bool write_png(const std::string& path, const Frame& f);

void render_frame(::sim::rts::Sim&, int, Frame&);

} // namespace sim::demo