#pragma once
// Deterministic grid — mirrors engine_path::Grid (engine/crates/path/src/lib.rs)
// field-for-field and operation-for-operation. NO float/double anywhere.
#include <cstdint>
#include <vector>

namespace sim {

struct TilePos {
    int16_t x{0};
    int16_t y{0};
};
constexpr bool operator==(const TilePos& a, const TilePos& b) noexcept {
    return a.x == b.x && a.y == b.y;
}
constexpr bool operator!=(const TilePos& a, const TilePos& b) noexcept {
    return !(a == b);
}

class Grid {
public:
    Grid() = default;
    Grid(int w, int h) : w_(w), h_(h), blocked_(static_cast<size_t>(w) * static_cast<size_t>(h), false) {}

    int w() const noexcept { return w_; }
    int h() const noexcept { return h_; }

    bool contains(TilePos p) const noexcept {
        return p.x >= 0 && p.x < w_ && p.y >= 0 && p.y < h_;
    }
    bool is_blocked(TilePos p) const noexcept {
        if (!contains(p)) return true;
        return blocked_[static_cast<size_t>(p.y) * static_cast<size_t>(w_) + static_cast<size_t>(p.x)];
    }
    void set_blocked(TilePos p, bool b) noexcept {
        if (contains(p)) blocked_[static_cast<size_t>(p.y) * static_cast<size_t>(w_) + static_cast<size_t>(p.x)] = b;
    }
    int blocked_count() const noexcept {
        int n = 0;
        for (bool b : blocked_) n += b ? 1 : 0;
        return n;
    }

private:
    int w_{0}, h_{0};
    std::vector<bool> blocked_;
};

} // namespace sim
