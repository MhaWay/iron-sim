#include "sim/path.hpp"

#include <algorithm>
#include <cstdint>
#include <utility>
#include <vector>

namespace {
constexpr int NEIGH[4][2] = {{0, -1}, {1, 0}, {0, 1}, {-1, 0}};

inline int64_t heur(int ax, int ay, int bx, int by) noexcept {
    return (static_cast<int64_t>(ax >= bx ? ax - bx : bx - ax) +
            static_cast<int64_t>(ay >= by ? ay - by : by - ay)) * sim::kStepS;
}

struct Node {
    int64_t f{0};
    uint64_t order{0};
    uint32_t cell{0};
};

// better(a,b) true iff a should be popped before b (lower f; on ties earliest insertion, i.e. lower order).
inline bool better(const Node& a, const Node& b) noexcept {
    return a.f < b.f || (a.f == b.f && a.order < b.order);
}
} // namespace

namespace sim {

std::vector<TilePos> find_path(const Grid& grid, TilePos start, TilePos goal, bool* ok) {
    if (ok) *ok = true;
    if (start == goal) return {};
    if (grid.is_blocked(goal)) { if (ok) *ok = false; return {}; }
    const int w = grid.w();
    const int h = grid.h();
    const std::size_t n = static_cast<std::size_t>(w) * static_cast<std::size_t>(h);
    const uint32_t scell = static_cast<uint32_t>(start.y) * static_cast<uint32_t>(w) + static_cast<uint32_t>(start.x);
    const uint32_t gcell = static_cast<uint32_t>(goal.y) * static_cast<uint32_t>(w) + static_cast<uint32_t>(goal.x);
    std::vector<int64_t> gscore(n, INT64_MAX);
    std::vector<uint32_t> came(n, UINT32_MAX);
    std::vector<uint8_t> closed(n, 0);

    // Binary heap where top is "best" = min f, ties higher order.
    // Use std::push_heap with comparator "worse(a,b)" so front is best.
    std::vector<Node> heap;
    uint64_t order = 0;
    auto cmp_worse = [](const Node& a, const Node& b) { return better(b, a); };
    heap.push_back(Node{heur(start.x, start.y, goal.x, goal.y), order++, scell});
    std::push_heap(heap.begin(), heap.end(), cmp_worse);
    gscore[scell] = 0;

    while (!heap.empty()) {
        std::pop_heap(heap.begin(), heap.end(), cmp_worse);
        const Node cur = heap.back();
        heap.pop_back();
        if (closed[cur.cell]) continue;
        closed[cur.cell] = 1;
        if (cur.cell == gcell) {
            std::vector<TilePos> rev;
            uint32_t c = gcell;
            while (c != scell) {
                rev.push_back(TilePos{static_cast<int16_t>(c % w), static_cast<int16_t>(c / w)});
                c = came[c];
            }
            std::reverse(rev.begin(), rev.end());
            return rev;
        }
        const int cx = static_cast<int>(cur.cell) % w;
        const int cy = static_cast<int>(cur.cell) / w;
        for (int k = 0; k < 4; ++k) {
            const int nx = cx + NEIGH[k][0];
            const int ny = cy + NEIGH[k][1];
            if (nx < 0 || ny < 0) continue;
            TilePos np{static_cast<int16_t>(nx), static_cast<int16_t>(ny)};
            if (!grid.contains(np) || grid.is_blocked(np)) continue;
            const uint32_t ncell = static_cast<uint32_t>(ny) * static_cast<uint32_t>(w) + static_cast<uint32_t>(nx);
            if (closed[ncell]) continue;
            const int64_t ng = gscore[cur.cell] + kStepS;
            if (ng < gscore[ncell]) {
                gscore[ncell] = ng;
                came[ncell] = cur.cell;
                heap.push_back(Node{ng + heur(nx, ny, goal.x, goal.y), order++, ncell});
                std::push_heap(heap.begin(), heap.end(), cmp_worse);
            }
        }
    }
    if (ok) *ok = false;
    return {};
}

} // namespace sim