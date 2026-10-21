#include "sim/rts_sim.hpp"
#include "sim/rts_shared_impl.hpp"

#include <algorithm>
#include <cstring>
#include <tuple>

namespace sim::rts {

// ---- helpers ----
inline TilePos tile_of(const Fixed1616& f) noexcept {
    return TilePos{static_cast<int16_t>(f.to_int_trunc()), static_cast<int16_t>(f.to_int_trunc())};
}
inline Fixed1616 tc_x(int16_t x) noexcept { return Fixed1616{static_cast<int64_t>(x) * (1 << 16) + (1 << 15)}; }
inline Fixed1616 tc_y(int16_t y) noexcept { return Fixed1616{static_cast<int64_t>(y) * (1 << 16) + (1 << 15)}; }
inline int16_t tile_of_x(const Fixed1616& f) noexcept { return static_cast<int16_t>(f.to_int_trunc()); }
inline int16_t tile_of_y(const Fixed1616& f) noexcept { return static_cast<int16_t>(f.to_int_trunc()); }

inline int32_t manhattan(int dx, int dy) noexcept {
    return (dx >= 0 ? dx : -dx) + (dy >= 0 ? dy : -dy);
}

struct Pt { Fixed1616 x{}, y{}; };

inline void march(Pt& p, Pt goal, int64_t step) noexcept {
    const int64_t dx = goal.x.v - p.x.v;
    const int64_t dy = goal.y.v - p.y.v;
    const int64_t adx = dx >= 0 ? dx : -dx;
    const int64_t ady = dy >= 0 ? dy : -dy;
    if (adx <= step && ady <= step) { p.x = goal.x; p.y = goal.y; return; }
    if (adx >= ady) p.x.v += (dx > 0 ? 1 : -1) * std::min(step, adx);
    else p.y.v += (dy > 0 ? 1 : -1) * std::min(step, ady);
}

inline void hash_path(const std::vector<TilePos>& p, Hasher& h) noexcept {
    h.u64(static_cast<uint64_t>(p.size()));
    for (const auto& t : p) { h.i32(static_cast<int32_t>(t.x)); h.i32(static_cast<int32_t>(t.y)); }
}

// ---- GridSide / bfs ----
static bool bfs_path(const GridSide& gs, int sx, int sy, int gx, int gy,
                     std::vector<TilePos>& out_path) noexcept {
    const int w = gs.w, h = gs.h;
    auto in_b = [&](int x, int y) { return x >= 0 && y >= 0 && x < w && y < h; };
    auto blocked = [&](int x, int y) { return !in_b(x, y) || gs.blocked[static_cast<size_t>(y) * w + x] != 0; };
    if (sx == gx && sy == gy) { out_path.clear(); return true; }
    if (blocked(gx, gy)) return false;
    if (!in_b(sx, sy) || !in_b(gx, gy)) return false;
    std::vector<int> prev(static_cast<size_t>(w) * h, -1);
    std::vector<int> q; q.reserve(static_cast<size_t>(w) * h);
    q.push_back(sy * w + sx);
    const int goal = gy * w + gx;
    static const int NEIGH[4][2] = {{0, -1}, {1, 0}, {0, 1}, {-1, 0}};
    for (size_t head = 0; head < q.size(); ++head) {
        const int cur = q[head];
        if (cur == goal) break;
        const int cx = cur % w, cy = cur / w;
        for (int k = 0; k < 4; ++k) {
            const int nx = cx + NEIGH[k][0], ny = cy + NEIGH[k][1];
            if (!in_b(nx, ny)) continue;
            const int idx = ny * w + nx;
            if (prev[idx] != -1 || blocked(nx, ny)) continue;
            prev[idx] = cur;
            q.push_back(idx);
        }
    }
    if (prev[goal] == -1) return false;
    out_path.clear();
    int cur = goal;
    while (cur != sy * w + sx) {
        out_path.push_back(TilePos{static_cast<int16_t>(cur % w), static_cast<int16_t>(cur / w)});
        cur = prev[cur];
    }
    std::reverse(out_path.begin(), out_path.end());
    return true;
}

static GridSide* gside(World& w) { auto* s = w.storages.find_slot<GridSide>(); if (s->items.size() < 2) return nullptr; return std::addressof(*s->items[1]); }
static VisMirror* vmirror(World& w) { auto* s = w.storages.find_slot<VisMirror>(); if (s->items.size() < 3) return nullptr; return std::addressof(*s->items[2]); }
static Deposits* deposits_slot(World& w) { auto* s = w.storages.find_slot<Deposits>(); if (s->items.size() < 1) return nullptr; return std::addressof(*s->items[0]); }

// ---- Systems ----
static void construction_system(World& w, engine::Tick) {
    struct Item { Entity e; Building b; };
    std::vector<Item> items;
    w.storages.each<Building>([&](Entity e, const Building& b) {
        if ((b.status == BuildingStatus::Site || b.status == BuildingStatus::UnderConstruction) && b.timer > 0)
            items.push_back({e, b});
    });
    for (auto& it : items) {
        if (--it.b.timer == 0) {
            if (it.b.status == BuildingStatus::Site) {
                it.b.status = BuildingStatus::UnderConstruction;
                it.b.timer = building_spec(it.b.kind).build_time / 4;
            } else {
                it.b.status = BuildingStatus::Done;
                it.b.hp = it.b.max_hp;
            }
        }
        w.storages.add(it.e, it.b);
    }
}

static void training_system(World& w, engine::Tick) {
    struct Item { Entity be; Building b; Pos p; TrainQueue q; };
    std::vector<Item> list;
    w.storages.each<Building>([&](Entity e, const Building& b) {
        if (b.status != BuildingStatus::Done) return;
        auto* qslot = w.storages.find_slot<TrainQueue>();
        if (!qslot || e.id >= qslot->items.size() || !qslot->items[e.id]) return;
        auto* p = w.storages.get<Pos>(e);
        if (!p) return;
        if (qslot->items[e.id]->empty()) return;
        list.push_back({e, b, *p, *qslot->items[e.id]});
    });
    for (auto& item : list) {
        if (!item.q.empty() && --item.q[0].ticks_left == 0) {
            const TrainItem front = item.q[0];
            item.q.erase(item.q.begin());
            const int16_t bx = static_cast<int16_t>(item.p.x.to_int_trunc());
            const int16_t by = static_cast<int16_t>(item.p.y.to_int_trunc());
            const auto& spec = unit_spec(front.kind);
            Entity ue = w.storages.spawn();
            w.storages.add(ue, Unit{static_cast<uint8_t>(item.b.owner), front.kind, static_cast<int32_t>(spec.hp),
                                    static_cast<uint32_t>(spec.speed_q), static_cast<int32_t>(spec.dmg),
                                    Fixed1616{spec.range_q}});
            w.storages.add(ue, Pos{tc_x(static_cast<int16_t>(bx + 1)), tc_y(static_cast<int16_t>(by + 1))});
            w.storages.add(ue, Order{});
        }
        w.storages.add(item.be, item.q);
    }
}

static void harvest_system(World& w, engine::Tick) {
    struct Snap {
        Entity e; HarvestState st;
        bool has_order; OrderKind order_kind; std::vector<TilePos> order_path;
        bool has_pos; Pos pos; bool has_unit; uint8_t owner;
    };
    std::vector<Snap> units;
    w.storages.each<HarvestState>([&](Entity e, const HarvestState& st) {
        Snap s{};
        s.e = e; s.st = st;
        if (const Order* o = w.storages.get<Order>(e)) {
            s.has_order = true; s.order_kind = o->kind; s.order_path = o->path;
        }
        if (const Pos* p = w.storages.get<Pos>(e)) { s.has_pos = true; s.pos = *p; }
        if (const Unit* u = w.storages.get<Unit>(e)) { s.has_unit = true; s.owner = u->owner; }
        units.push_back(std::move(s));
    });
    auto* gs_slot = w.storages.find_slot<GridSide>();
    const GridSide* gs = (gs_slot && gs_slot->items.size() > 1 && gs_slot->items[1]) ? std::addressof(*gs_slot->items[1]) : nullptr;
    Deposits* deps = nullptr;
    {
        auto* dslot = w.storages.find_slot<Deposits>();
        if (dslot && !dslot->items.empty() && dslot->items[0]) deps = std::addressof(*dslot->items[0]);
    }
    const auto node_alive = [&](uint32_t nid) {
        const auto* n = w.storages.get<ResourceNode>(Entity{nid});
        return n && n->remaining > 0;
    };
    const auto find_tc = [&](uint32_t owner) -> std::optional<TilePos> {
        std::optional<TilePos> out{};
        w.storages.each<Building>([&](Entity e, const Building& b) {
            if (out.has_value()) return;
            if (b.owner == owner && b.kind == 0 && b.status == BuildingStatus::Done) {
                const Pos* p = w.storages.get<Pos>(e);
                if (p) out = TilePos{static_cast<int16_t>(p->x.to_int_trunc()), static_cast<int16_t>(p->y.to_int_trunc())};
            }
        });
        return out;
    };
    (void)gs; (void)deps;

    const auto step_one = [&](Snap& u) {
        if (!u.has_pos) return;
        const auto px = static_cast<int16_t>(u.pos.x.to_int_trunc());
        const auto py = static_cast<int16_t>(u.pos.y.to_int_trunc());
        switch (u.st.kind) {
            case HarvestKind::ToNode: {
                if (!node_alive(u.st.node)) {
                    w.storages.remove<HarvestState>(u.e);
                    w.storages.add(u.e, Order{});
                    return;
                }
                const bool idle_or_harv = u.has_order && (u.order_kind == OrderKind::Idle || u.order_kind == OrderKind::Harvest);
                if (idle_or_harv) {
                    const NodePos* np = w.storages.get<NodePos>(Entity{u.st.node});
                    if (!np) { w.storages.remove<HarvestState>(u.e); w.storages.add(u.e, Order{}); return; }
                    const bool walking = u.has_order && u.order_kind == OrderKind::Harvest && !u.order_path.empty();
                    if (walking) return; // wait for movement_system
                    if (manhattan(px - np->x, py - np->y) <= 1) {
                        w.storages.add(u.e, HarvestState{HarvestKind::Picking, u.st.node, u.st.slot, PICK_TICKS, 0});
                        return;
                    }
                    if (gs) {
                        std::vector<TilePos> path;
                        if (bfs_path(*gs, px, py, np->x, np->y, path)) {
                            if (path.empty()) {
                                w.storages.add(u.e, HarvestState{HarvestKind::Picking, u.st.node, u.st.slot, PICK_TICKS, 0});
                            } else {
                                w.storages.add(u.e, Order{OrderKind::Harvest, 0, path, u.st.node});
                            }
                        }
                    }
                    return;
                }
                return; // keep ToNode
            }
            case HarvestKind::Picking: {
                if (!node_alive(u.st.node)) { w.storages.remove<HarvestState>(u.e); w.storages.add(u.e, Order{}); return; }
                if (u.st.ticks_left == 0) { w.storages.remove<HarvestState>(u.e); w.storages.add(u.e, Order{}); return; }
                const uint32_t nleft = u.st.ticks_left - 1;
                if (nleft == 0)
                    w.storages.add(u.e, HarvestState{HarvestKind::Carrying, u.st.node, u.st.slot, 0, CARRY});
                else
                    w.storages.add(u.e, HarvestState{HarvestKind::Picking, u.st.node, u.st.slot, nleft, 0});
                return;
            }
            case HarvestKind::Carrying: {
                const auto tc = find_tc(u.owner);
                const auto do_deposit = [&]() {
                    ResourceNode* node = w.storages.get<ResourceNode>(Entity{u.st.node});
                    int32_t take = 0;
                    if (node && node->remaining > 0) {
                        take = static_cast<int32_t>(std::min(static_cast<int64_t>(u.st.amount), node->remaining));
                        node->remaining -= take;
                    }
                    if (take > 0 && deps) deps->push_back({u.owner, u.st.slot, take});
                    w.storages.remove<HarvestState>(u.e);
                    w.storages.add(u.e, Order{});
                };
                if (tc && manhattan((*tc).x - px, (*tc).y - py) <= 2) { do_deposit(); return; }
                if (!tc || !gs) return;
                static const int ring[16][2] = {
                    {-2, 0}, {2, 0}, {0, -2}, {0, 2},
                    {-1, -2}, {-1, 2}, {1, -2}, {1, 2},
                    {-2, -1}, {-2, 1}, {2, -1}, {2, 1},
                    {-2, -2}, {-2, 2}, {2, -2}, {2, 2},
                };
                std::optional<std::pair<int, TilePos>> best{};
                for (int i = 0; i < 16; ++i) {
                    const int gx = (*tc).x + ring[i][0];
                    const int gy = (*tc).y + ring[i][1];
                    if (gx < 0 || gy < 0 || gx >= gs->w || gy >= gs->h) continue;
                    if (gs->blocked[static_cast<size_t>(gy) * gs->w + gx]) continue;
                    std::vector<TilePos> tmp;
                    if (!bfs_path(*gs, px, py, gx, gy, tmp)) continue;
                    const int d = manhattan(gx - px, gy - py);
                    if (!best.has_value() || d < best->first) best = {d, TilePos{static_cast<int16_t>(gx), static_cast<int16_t>(gy)}};
                }
                if (best.has_value()) {
                    std::vector<TilePos> path;
                    if (bfs_path(*gs, px, py, best->second.x, best->second.y, path)) {
                        if (path.empty()) do_deposit();
                        else w.storages.add(u.e, Order{OrderKind::Move, 0, path, 0});
                    }
                }
                return;
            }
        }
    };
    for (auto& u : units) step_one(u);
}

static void movement_system(World& w, engine::Tick) {
    struct Item { Entity e; Order order; Fixed1616 p_x, p_y; uint32_t speed_q; };
    std::vector<Item> items;
    w.storages.each<Unit>([&](Entity e, const Unit& u) {
        if (u.speed_q == 0) return;
        const Order* o = w.storages.get<Order>(e);
        const Pos* p = w.storages.get<Pos>(e);
        if (!o || !p) return;
        items.push_back({e, *o, p->x, p->y, u.speed_q});
    });
    for (auto& it : items) {
        const int64_t step_v = Fixed1616::from_rational(static_cast<int64_t>(it.speed_q), SPEED_SCALE).v;
        Pt p{it.p_x, it.p_y};
        bool moved = false;
        if (it.order.kind == OrderKind::Move) {
            if (it.order.waypoint >= it.order.path.size()) { w.storages.add(it.e, Order{}); continue; }
            const TilePos wp = it.order.path[it.order.waypoint];
            const auto before = p;
            march(p, Pt{tc_x(wp.x), tc_y(wp.y)}, step_v);
            moved = (before.x.v != p.x.v || before.y.v != p.y.v);
            if (p.x.v == tc_x(wp.x).v && p.y.v == tc_y(wp.y).v) {
                it.order.waypoint++;
                if (it.order.waypoint >= it.order.path.size()) w.storages.add(it.e, Order{});
                else w.storages.add(it.e, it.order);
            } else w.storages.add(it.e, it.order);
        } else if (it.order.kind == OrderKind::Harvest || it.order.kind == OrderKind::Construct) {
            if (it.order.path.empty()) {
                if (it.order.kind == OrderKind::Construct) {
                    w.storages.add(it.e, Order{});
                    w.storages.add(it.e, WorkJob{it.order.target, 0, 0, WorkMode::Build});
                } else w.storages.add(it.e, Order{});
                continue;
            }
            const TilePos wp = it.order.path[0];
            march(p, Pt{tc_x(wp.x), tc_y(wp.y)}, step_v);
            moved = true;
            if (p.x.v == tc_x(wp.x).v && p.y.v == tc_y(wp.y).v) it.order.path.erase(it.order.path.begin());
            w.storages.add(it.e, it.order);
        }
        if (moved) w.storages.add(it.e, Pos{p.x, p.y});
    }
}

static void visibility_system(World& w, engine::Tick) {
    auto* vm = vmirror(w);
    if (!vm) return;
    std::vector<std::pair<uint32_t, TilePos>> reveals;
    w.storages.each<Unit>([&](Entity e, const Unit& u) {
        if (u.owner >= 2) return;
        const Pos* p = w.storages.get<Pos>(e);
        if (!p) return;
        reveals.push_back({u.owner, TilePos{static_cast<int16_t>(p->x.to_int_trunc()), static_cast<int16_t>(p->y.to_int_trunc())}});
    });
    for (auto& [owner, pos] : reveals)
        for (int dy = -SIGHT_BASE; dy <= SIGHT_BASE; ++dy)
            for (int dx = -SIGHT_BASE; dx <= SIGHT_BASE; ++dx)
                if (manhattan(dx, dy) <= SIGHT_BASE) vm->words[owner].set(pos.x + dx, pos.y + dy);
}

static void combat_system(World& w, engine::Tick) {
    struct Att { Entity e; uint32_t target; int32_t dmg; Fixed1616 range; Fixed1616 fx, fy; };
    std::vector<Att> attackers;
    w.storages.each<Order>([&](Entity e, const Order& o) {
        if (o.kind != OrderKind::Attack) return;
        const Unit* u = w.storages.get<Unit>(e);
        const Pos* p = w.storages.get<Pos>(e);
        if (!u || !p) return;
        attackers.push_back({e, o.target, u->dmg, u->range, p->x, p->y});
    });
    std::vector<uint32_t> dead;
    for (auto& a : attackers) {
        Unit* tu = w.storages.get<Unit>(Entity{a.target});
        const Pos* tp = w.storages.get<Pos>(Entity{a.target});
        if (!tu || !tp) continue;
        const int64_t dx = static_cast<int64_t>(static_cast<uint64_t>(tp->x.v - a.fx.v) * static_cast<uint64_t>(tp->x.v - a.fx.v));
        const int64_t dy = static_cast<int64_t>(static_cast<uint64_t>(tp->y.v - a.fy.v) * static_cast<uint64_t>(tp->y.v - a.fy.v));
        const int64_t r = static_cast<int64_t>(static_cast<uint64_t>(a.range.v) * static_cast<uint64_t>(a.range.v));
        if (static_cast<int64_t>(static_cast<uint64_t>(dx) + static_cast<uint64_t>(dy)) <= r) {
            tu->hp -= a.dmg;
            if (tu->hp <= 0) dead.push_back(a.target);
        }
    }
    std::sort(dead.begin(), dead.end());
    dead.erase(std::unique(dead.begin(), dead.end()), dead.end());
    for (auto id : dead) {
        const Entity e{id};
        w.storages.remove<Unit>(e); w.storages.remove<Pos>(e); w.storages.remove<Order>(e);
        w.storages.remove<HarvestState>(e); w.storages.remove<WorkJob>(e);
        w.storages.despawn(e);
    }
}

// ---- Sim ----
Sim::Sim(uint64_t seed) : world_(seed), grid_(MAP_W, MAP_H) {
    world_.storages.add(Entity{0}, Deposits{});
    world_.storages.add(Entity{1}, GridSide{MAP_W, MAP_H, std::vector<uint8_t>(static_cast<size_t>(MAP_W) * MAP_H, 0)});
    world_.storages.add(Entity{2}, VisMirror{});
    generate_map();
    players_.push_back(PlayerState{});
    players_.push_back(PlayerState{});
    vis_.emplace_back(0);
    vis_.emplace_back(1);
    spawn_default_squads();
}

int Sim::alive_units() const { return static_cast<int>(world_.storages.count<Unit>()); }

void Sim::sync_mirrors() {
    if (auto* gs = gside(world_)) {
        gs->w = MAP_W; gs->h = MAP_H;
        gs->blocked.resize(static_cast<size_t>(MAP_W) * MAP_H);
        for (int y = 0; y < MAP_H; ++y)
            for (int x = 0; x < MAP_W; ++x)
                gs->blocked[static_cast<size_t>(y) * MAP_W + x] = grid_.is_blocked({static_cast<int16_t>(x), static_cast<int16_t>(y)}) ? 1 : 0;
    }
    if (auto* vm = vmirror(world_))
        for (size_t i = 0; i < vis_.size(); ++i) {
            vm->words[i].owner = vis_[i].owner;
            vm->words[i].words = vis_[i].words;
        }
}

void Sim::step(const std::vector<PlayerCommand>& cmds) {
    sync_mirrors();
    auto sorted = cmds;
    std::stable_sort(sorted.begin(), sorted.end(), SortKeyLess{});
    for (const auto& pc : sorted) {
        switch (pc.cmd.kind) {
            case Kind::Move:    apply_move(pc.cmd.a, pc.cmd.b, pc.cmd.c); break;
            case Kind::Attack:  apply_attack(pc.cmd.a, static_cast<uint32_t>(static_cast<uint16_t>(pc.cmd.b))); break;
            case Kind::Stop:    apply_stop(pc.cmd.a); break;
            case Kind::Build:   build_at(pc.player, static_cast<uint16_t>(pc.cmd.a), pc.cmd.b, pc.cmd.c); break;
            case Kind::Train:   apply_train(pc.cmd.a, static_cast<uint16_t>(pc.cmd.b)); break;
            case Kind::Gather:  apply_gather(pc.cmd.a, static_cast<uint8_t>(pc.cmd.b)); break;
            default: break;
        }
    }
    world_.tick += 1;
    construction_system(world_, world_.tick);
    training_system(world_, world_.tick);
    harvest_system(world_, world_.tick);
    movement_system(world_, world_.tick);
    visibility_system(world_, world_.tick);
    combat_system(world_, world_.tick);
    if (auto* vm = vmirror(world_)) {
        for (size_t i = 0; i < vis_.size(); ++i)
            for (size_t k = 0; k < vm->words[i].words.size(); ++k)
                vis_[i].words[k] |= vm->words[i].words[k];
    }
    if (auto* dp = deposits_slot(world_)) {
        for (auto& [owner, slot, amount] : *dp)
            if (owner < players_.size()) players_[owner].stock.add(slot, amount);
        dp->clear();
    }
}

void Sim::generate_map() {
    for (int x = 0; x < MAP_W; ++x)
        for (int b = 0; b < 2; ++b) {
            grid_.set_blocked({static_cast<int16_t>(x), static_cast<int16_t>(b)}, true);
            grid_.set_blocked({static_cast<int16_t>(x), static_cast<int16_t>(MAP_H - 1 - b)}, true);
        }
    for (int y = 0; y < MAP_H; ++y)
        for (int b = 0; b < 2; ++b) {
            grid_.set_blocked({static_cast<int16_t>(b), static_cast<int16_t>(y)}, true);
            grid_.set_blocked({static_cast<int16_t>(MAP_W - 1 - b), static_cast<int16_t>(y)}, true);
        }
    for (int i = 0; i < 700; ++i) {
        const uint64_t v = world_.rng.next_u64();
        const int x = 16 + (v % (MAP_W - 32));
        const int y = 16 + ((v >> 16) % (MAP_H - 32));
        const int kind = (v >> 32) % 4;
        if (kind == 3) continue;
        if (manhattan(x - 14, y - 14) < 14 || manhattan(x - 113, y - 113) < 14) continue;
        grid_.set_blocked({static_cast<int16_t>(x), static_cast<int16_t>(y)}, true);
    }
    for (uint32_t i = 0; i < 24; ++i) {
        const uint64_t v = world_.rng.next_u64();
        const int slot = i < 10 ? 0 : (i < 18 ? 1 : 2);
        const int64_t total = i < 10 ? 2000 + static_cast<int64_t>(v % 1000)
                            : i < 18 ? 1500 + static_cast<int64_t>((v >> 20) % 1000)
                                     : 500 + static_cast<int64_t>((v >> 40) % 500);
        const int x = 16 + (v % (MAP_W - 32));
        const int y = 16 + ((v >> 16) % (MAP_H - 32));
        if (grid_.is_blocked({static_cast<int16_t>(x), static_cast<int16_t>(y)})) continue;
        if (manhattan(x - 14, y - 14) < 10 || manhattan(x - 113, y - 113) < 10) continue;
        const Entity e = world_.storages.spawn();
        world_.storages.add(e, ResourceNode{static_cast<uint8_t>(slot), total, total});
        world_.storages.add(e, NodePos{static_cast<int16_t>(x), static_cast<int16_t>(y)});
    }
}

void Sim::spawn_default_squads() {
    const uint32_t owners[2] = {0, 1};
    const int pxs[2] = {14, 113};
    for (int i = 0; i < 2; ++i) {
        const int px = pxs[i];
        const BuildingSpec spec = building_spec(0);
        Entity e = world_.storages.spawn();
        world_.storages.add(e, Building{owners[i], 0, BuildingStatus::Done, spec.hp, spec.hp, 0});
        world_.storages.add(e, Pos{tc_x(static_cast<int16_t>(px + 1)), tc_y(static_cast<int16_t>(px + 1))});
        for (int y = px; y < px + 3; ++y)
            for (int x = px; x < px + 3; ++x)
                grid_.set_blocked({static_cast<int16_t>(x), static_cast<int16_t>(y)}, true);
        for (int j = 0; j < 3; ++j) spawn_unit_kind(owners[i], 0, static_cast<int16_t>(px + 4 + j), static_cast<int16_t>(px + 2));
    }
}

Entity Sim::spawn_unit_kind(uint32_t owner, uint16_t kind, int16_t x, int16_t y) {
    const auto& spec = unit_spec(kind);
    Entity e = world_.storages.spawn();
    world_.storages.add(e, Unit{static_cast<uint8_t>(owner), kind, static_cast<int32_t>(spec.hp),
                                static_cast<uint32_t>(spec.speed_q), static_cast<int32_t>(spec.dmg),
                                Fixed1616{spec.range_q}});
    world_.storages.add(e, Pos{tc_x(x), tc_y(y)});
    world_.storages.add(e, Order{});
    return e;
}

uint32_t Sim::spawn_unit(uint32_t owner, int16_t x, int16_t y) { return spawn_unit_kind(owner, 1, x, y).id; }

bool Sim::apply_move(uint32_t unit, int16_t x, int16_t y) {
    const Pos* p = world_.storages.get<Pos>(Entity{unit});
    if (!p) return false;
    if (!grid_.contains({x, y}) || grid_.is_blocked({x, y})) return false;
    const TilePos start{static_cast<int16_t>(p->x.to_int_trunc()), static_cast<int16_t>(p->y.to_int_trunc())};
    bool ok = false;
    auto path = find_path(grid_, start, {x, y}, &ok);
    if (!ok) return false;
    world_.storages.add(Entity{unit}, Order{OrderKind::Move, 0, path, 0});
    return true;
}

bool Sim::apply_attack(uint32_t unit, uint32_t target) {
    if (!world_.storages.get<Unit>(Entity{unit}) || !world_.storages.get<Unit>(Entity{target})) return false;
    world_.storages.add(Entity{unit}, Order{OrderKind::Attack, 0, {}, target});
    return true;
}

bool Sim::apply_stop(uint32_t unit) {
    if (!world_.storages.get<Unit>(Entity{unit})) return false;
    world_.storages.add(Entity{unit}, Order{});
    world_.storages.remove<HarvestState>(Entity{unit});
    world_.storages.remove<WorkJob>(Entity{unit});
    return true;
}

bool Sim::build_at(uint32_t owner, uint16_t kind, int16_t x, int16_t y) {
    if (owner >= players_.size()) return false;
    const auto& spec = building_spec(kind);
    if (x < 0 || y < 0 || x + spec.w > MAP_W || y + spec.h > MAP_H) return false;
    for (int yy = 0; yy < spec.h; ++yy)
        for (int xx = 0; xx < spec.w; ++xx)
            if (grid_.is_blocked({static_cast<int16_t>(x + xx), static_cast<int16_t>(y + yy)})) return false;
    auto& s = players_[owner].stock;
    if (s.wood < spec.wood || s.food < spec.food || s.gold < spec.gold || s.mana < spec.mana) return false;
    s.wood -= spec.wood; s.food -= spec.food; s.gold -= spec.gold; s.mana -= spec.mana;
    Entity e = world_.storages.spawn();
    world_.storages.add(e, Building{owner, kind, BuildingStatus::Site, 1, spec.hp, spec.build_time});
    world_.storages.add(e, Pos{tc_x(static_cast<int16_t>(x + spec.w / 2)), tc_y(static_cast<int16_t>(y + spec.h / 2))});
    for (int yy = 0; yy < spec.h; ++yy)
        for (int xx = 0; xx < spec.w; ++xx)
            grid_.set_blocked({static_cast<int16_t>(x + xx), static_cast<int16_t>(y + yy)}, true);
    return true;
}

bool Sim::apply_train(uint32_t building, uint16_t unit_kind) {
    Building* b = world_.storages.get<Building>(Entity{building});
    if (!b || b->status != BuildingStatus::Done || unit_kind > 2) return false;
    const auto& spec = unit_spec(unit_kind);
    if (b->owner >= players_.size()) return false;
    auto& s = players_[b->owner].stock;
    if (s.wood < spec.wood || s.food < spec.food || s.gold < spec.gold || s.mana < spec.mana) return false;
    s.wood -= spec.wood; s.food -= spec.food; s.gold -= spec.gold; s.mana -= spec.mana;
    TrainQueue* q = world_.storages.get<TrainQueue>(Entity{building});
    if (q) q->push_back({unit_kind, spec.train_time});
    else world_.storages.add(Entity{building}, TrainQueue{{unit_kind, spec.train_time}});
    return true;
}

bool Sim::apply_gather(uint32_t unit, uint8_t resource) {
    if (resource > 3) return false;
    const Pos* p = world_.storages.get<Pos>(Entity{unit});
    if (!p) return false;
    const auto ux = static_cast<int16_t>(p->x.to_int_trunc());
    const auto uy = static_cast<int16_t>(p->y.to_int_trunc());
    struct Best { int d; uint32_t id; int16_t x; int16_t y; };
    std::vector<std::pair<uint32_t, NodePos>> cands;
    world_.storages.each<ResourceNode>([&](Entity e, const ResourceNode& n) {
        if (n.slot == resource && n.remaining > 0) {
            const NodePos* np = world_.storages.get<NodePos>(e);
            if (np) cands.push_back({e.id, *np});
        }
    });
    std::optional<Best> best{};
    for (auto& [id, np] : cands) {
        const int d = manhattan(ux - np.x, uy - np.y);
        if (!best.has_value() || d < best->d) best = Best{d, id, np.x, np.y};
    }
    if (!best) return false;
    bool ok = false;
    auto path = find_path(grid_, {ux, uy}, {best->x, best->y}, &ok);
    if (!ok) return false;
    world_.storages.add(Entity{unit}, Order{OrderKind::Harvest, 0, path, best->id});
    world_.storages.add(Entity{unit}, HarvestState{HarvestKind::ToNode, best->id, resource, 0, 0});
    return true;
}

uint64_t Sim::hash() const {
    Hasher h;
    h.u32(SIM_VERSION);
    h.u64(world_.tick);
    h.u64(world_.rng.state());
    h.u32(static_cast<uint32_t>(grid_.w()));
    h.u32(static_cast<uint32_t>(grid_.h()));
    h.u32(static_cast<uint32_t>(grid_.blocked_count()));
    for (const auto& p : players_) {
        h.i64(p.stock.food); h.i64(p.stock.wood); h.i64(p.stock.gold); h.i64(p.stock.mana);
        h.u32(p.age); h.u32(p.wonder); h.u64(p.wonder_started);
    }
    for (const auto& v : vis_) v.hash_into(h);
    std::vector<Entity> bs = world_.storages.entities<Building>();
    std::sort(bs.begin(), bs.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    for (const auto& e : bs) {
        const Building* b = world_.storages.get<Building>(e);
        if (!b) continue;
        h.u32(e.id); h.u32(b->owner); h.u16(b->kind);
        h.u8(static_cast<uint8_t>(b->status));
        h.u32(b->hp); h.u32(b->max_hp); h.u32(b->timer);
        if (const Pos* p = world_.storages.get<Pos>(e)) {
            h.i32(static_cast<int32_t>(p->x.to_int_trunc()));
            h.i32(static_cast<int32_t>(p->y.to_int_trunc()));
        }
        if (const TrainQueue* q = world_.storages.get<TrainQueue>(e)) {
            h.u32(static_cast<uint32_t>(q->size()));
            for (const auto& it : *q) { h.u16(it.kind); h.u32(it.ticks_left); }
        }
    }
    std::vector<Entity> ns = world_.storages.entities<ResourceNode>();
    std::sort(ns.begin(), ns.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    for (const auto& e : ns) {
        const ResourceNode* n = world_.storages.get<ResourceNode>(e);
        if (!n) continue;
        h.u32(e.id);
        n->hash_into(h);
        if (const NodePos* np = world_.storages.get<NodePos>(e)) np->hash_into(h);
    }
    std::vector<Entity> us = world_.storages.entities<Unit>();
    std::sort(us.begin(), us.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    for (const auto& e : us) {
        const Unit* u = world_.storages.get<Unit>(e);
        if (!u) continue;
        h.u32(e.id); h.u8(u->owner); h.u16(u->kind); h.i32(u->hp); h.u32(u->speed_q); h.i32(u->dmg);
        h.i64(u->range.v);
        if (const Pos* p = world_.storages.get<Pos>(e)) {
            h.i32(static_cast<int32_t>(p->x.to_int_trunc()));
            h.i32(static_cast<int32_t>(p->y.to_int_trunc()));
        }
        if (const Order* o = world_.storages.get<Order>(e)) {
            h.u8(static_cast<uint8_t>(o->kind));
            switch (o->kind) {
                case OrderKind::Idle: break;
                case OrderKind::Move: { h.u64(o->waypoint); hash_path(o->path, h); break; }
                case OrderKind::Attack: h.u32(o->target); break;
                case OrderKind::Harvest: h.u32(o->target); hash_path(o->path, h); break;
                case OrderKind::Construct: h.u32(o->target); hash_path(o->path, h); break;
            }
        }
        if (const HarvestState* hs = world_.storages.get<HarvestState>(e)) {
            h.u8(static_cast<uint8_t>(hs->kind));
            switch (hs->kind) {
                case HarvestKind::ToNode: h.u32(hs->node); h.u8(hs->slot); break;
                case HarvestKind::Picking: h.u32(hs->node); h.u8(hs->slot); h.u32(hs->ticks_left); break;
                case HarvestKind::Carrying: h.u8(hs->slot); h.i32(hs->amount); break;
            }
        }
        if (const WorkJob* wj = world_.storages.get<WorkJob>(e)) {
            h.u32(wj->building); h.u32(wj->node); h.u8(wj->slot); h.u8(static_cast<uint8_t>(wj->mode));
        }
    }
    return h.finish();
}

uint64_t run_sim(uint64_t seed, uint64_t ticks, const std::vector<TickCommands>& stream) {
    Sim sim(seed);
    for (uint64_t t = 0; t < ticks; ++t) {
        std::vector<PlayerCommand> batch;
        if (t < stream.size()) batch = stream[t].cmds;
        sim.step(batch);
    }
    return sim.hash();
}

} // namespace sim::rts