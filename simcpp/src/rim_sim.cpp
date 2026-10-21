#include "sim/rim_sim.hpp"

#include <algorithm>
#include <optional>
#include <vector>

namespace sim::rim {

namespace {
inline int32_t manhattan(int ax, int ay) noexcept { return (ax >= 0 ? ax : -ax) + (ay >= 0 ? ay : -ay); }
inline void wu32(Hasher& h, uint32_t v) noexcept { h.u64(static_cast<uint64_t>(v)); }
inline void wi32(Hasher& h, int32_t v) noexcept { h.i64(static_cast<int64_t>(v)); }
inline int32_t manhattan_xy(int ax, int ay, int bx, int by) noexcept { return manhattan(ax - bx, ay - by); }
} // namespace

ColonySim::ColonySim(uint64_t seed) : world_(seed) {}

uint32_t ColonySim::spawn_pawn(uint32_t owner, int32_t x, int32_t y) {
    Entity e = world_.storages.spawn();
    world_.storages.add(e, Needs{});
    world_.storages.add(e, Pos{Fixed1616::from_int(static_cast<int32_t>(x)), Fixed1616::from_int(static_cast<int32_t>(y))});
    world_.storages.add(e, Pawn{owner, Fixed1616{int64_t{PAWN_SPEED_RAW}}, Mode{}});
    return e.id;
}

void ColonySim::add_site(uint8_t kind, int16_t x, int16_t y) {
    Entity e = world_.storages.spawn();
    world_.storages.add(e, Site{kind, TilePos{x, y}, std::nullopt});
}

void ColonySim::add_job(uint8_t kind, int16_t x, int16_t y) {
    Entity e = world_.storages.spawn();
    world_.storages.add(e, Job{kind, TilePos{x, y}, std::nullopt, false});
}

bool ColonySim::pawn_tile(uint32_t pawn, TilePos& out) const {
    const auto* p = world_.storages.get<Pos>(Entity{pawn});
    if (!p) return false;
    out = TilePos{static_cast<int16_t>(p->x.to_int_trunc()), static_cast<int16_t>(p->y.to_int_trunc())};
    return true;
}

bool ColonySim::apply_command(const Command& c) {
    switch (c.kind) {
        case Kind::Move: {
            const auto* p = world_.storages.get<Pos>(Entity{c.a});
            if (!p) return false;
            const TilePos start{static_cast<int16_t>(p->x.to_int_trunc()), static_cast<int16_t>(p->y.to_int_trunc())};
            const TilePos goal{c.b, c.c};
            if (!grid_.contains(goal)) return false;
            if (start == goal) return true;
            bool ok = false;
            auto path = find_path(grid_, start, goal, &ok);
            if (!ok) return false;
            auto* pawn = world_.storages.get<Pawn>(Entity{c.a});
            if (!pawn) return false;
            pawn->mode = Mode{ModeKind::Go, path, 0, goal, 0, 0};
            return true;
        }
        case Kind::Wait: {
            auto* pawn = world_.storages.get<Pawn>(Entity{c.a});
            if (!pawn) return false;
            pawn->mode = Mode{};
            return true;
        }
        case Kind::Build: {
            if (!grid_.contains({c.b, c.c})) return false;
            add_site(static_cast<uint8_t>(c.a & 1), c.b, c.c);
            return true;
        }
    }
    return false;
}

void ColonySim::step(const std::vector<PlayerCommand>& cmds) {
    auto sorted = cmds;
    std::stable_sort(sorted.begin(), sorted.end(), sort_less);
    for (const auto& pc : sorted) apply_command(pc.cmd);
    world_.tick += 1;

    // systems: needs, jobs, movement — registration order.
    // 1) needs_system: phase 1 read + phase 2 write + release sites.
    struct NeedsSnap { Entity e; Needs n; Pawn p; };
    std::vector<NeedsSnap> pawns;
    world_.storages.each<Pawn>([&](Entity e, const Pawn& p) {
        const Needs* n = world_.storages.get<Needs>(e);
        if (n) pawns.push_back({e, *n, p});
    });
    std::vector<std::pair<uint32_t, int>> releases{}; // pawn, release kind (-1 none)
    std::vector<NeedsSnap> updated{};
    for (auto& s : pawns) {
        Needs& n = s.n;
        // phase 1: mode-specific read + decay.
        switch (s.p.mode.kind) {
            case ModeKind::Sleep:
                n.energy += SLEEP_RECOVER;
                if (n.energy > NEED_MAX) n.energy = NEED_MAX;
                if (n.energy == NEED_MAX) { s.p.mode = Mode{}; releases.push_back({s.e.id, 1}); }
                break;
            case ModeKind::Eat:
                if (s.p.mode.timer <= 1) {
                    n.hunger += EAT_RESTORE;
                    if (n.hunger > NEED_MAX) n.hunger = NEED_MAX;
                    s.p.mode = Mode{};
                    releases.push_back({s.e.id, 0});
                } else s.p.mode.timer -= 1;
                break;
            case ModeKind::Work: {
                const Job* j = world_.storages.get<Job>(Entity{s.p.mode.job});
                if (!j || j->done) s.p.mode = Mode{};
                break;
            }
            default: break;
        }
        // decay all modes.
        s.n.decay_all();
        updated.push_back(s);
    }
    // phase 2 writes: store needs/pawn, release first matching site.
    for (auto& s : updated) {
        world_.storages.add(s.e, s.n);
        world_.storages.add(s.e, s.p);
    }
    for (auto& [pawn_id, kind] : releases) {
        // Find first site (entity index ascending) with kind match and occupied_by == pawn.
        auto* sslot = world_.storages.find_slot<Site>();
        if (sslot) {
            for (auto& opt : sslot->items) {
                if (!opt.has_value()) continue;
                Site& site = *opt;
                if (!site.occupied_by.has_value()) continue;
                if (*site.occupied_by != pawn_id) continue;
                if (site.kind != kind) continue;
                site.occupied_by = std::nullopt;
                break;
            }
        }
    }

    // 2) jobs_system: phase A (need-driven), phase B (idle non-need).
    // collect idle pawns needing.
    std::vector<uint32_t> idle_need, idle_free;
    world_.storages.each<Pawn>([&](Entity e, const Pawn& p) {
        if (p.mode.kind != ModeKind::Idle) return;
        const Needs* n = world_.storages.get<Needs>(e);
        if (!n) return;
        if (n->hunger < HUNGRY_THRESHOLD) idle_need.push_back(e.id);
        else if (n->energy < TIRED_THRESHOLD) idle_need.push_back(e.id);
        else idle_free.push_back(e.id);
    });
    std::sort(idle_need.begin(), idle_need.end());
    std::sort(idle_free.begin(), idle_free.end());

    struct SiteRef { Entity e; Site s; };
    const auto sites_all = [&](std::vector<SiteRef>& sink) {
        world_.storages.each<Site>([&](Entity e, const Site& s) { sink.push_back({e, s}); });
    };
    const auto jobs_all = [&](std::vector<Job>& sink) {
        world_.storages.each<Job>([&](Entity, const Job& j) { sink.push_back(j); });
    };
    auto dist_of = [&](uint32_t pawn_id, TilePos t) {
        TilePos pt{0, 0};
        pawn_tile(pawn_id, pt);
        return manhattan(pt.x - t.x, pt.y - t.y);
    };
    const auto is_idle = [&](uint32_t pawn_id) {
        const Pawn* p = world_.storages.get<Pawn>(Entity{pawn_id});
        return p && p->mode.kind == ModeKind::Idle;
    };
    std::vector<std::pair<uint32_t, Entity>> need_claims{}; // (pawn, site_entity) for phase A later
    std::vector<std::pair<uint32_t, Entity>> job_assignments{}; // (pawn, job_entity)
    for (auto pid : idle_need) {
        const Needs* n = world_.storages.get<Needs>(Entity{pid});
        if (!n) continue;
        const int8_t kind = n->hunger < HUNGRY_THRESHOLD ? 0 : 1;
        std::vector<SiteRef> cands;
        sites_all(cands);
        std::sort(cands.begin(), cands.end(), [](const SiteRef& a, const SiteRef& b) { return a.e.id < b.e.id; });
        std::optional<std::pair<int, Entity>> best{};
        for (auto& cr : cands) {
            if (cr.s.kind != kind) continue;
            if (cr.s.occupied_by.has_value()) continue;
            const int d = dist_of(pid, cr.s.tile);
            if (!best.has_value() || d < best->first) best = {d, cr.e};
        }
        if (best.has_value()) {
            need_claims.push_back(std::pair<uint32_t, Entity>{pid, best->second});
            // Claim: occupied_by = pid, mode Go [site] wp 0.
            Site* s = world_.storages.get<Site>(best->second);
            if (s) s->occupied_by = pid;
            if (Site* s2 = world_.storages.get<Site>(best->second); s2) {
                Pawn* p2 = world_.storages.get<Pawn>(Entity{pid});
                if (p2) p2->mode = Mode{ModeKind::Go, std::vector<TilePos>{s2->tile}, 0, s2->tile, 0, 0};
            }
        }
    }
    for (auto pid : idle_free) {
        std::vector<Entity> job_e;
        world_.storages.each<Job>([&](Entity e, const Job& j) {
            if (!j.assigned.has_value() && !j.done) job_e.push_back(e);
        });
        std::sort(job_e.begin(), job_e.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
        std::optional<std::pair<int, Entity>> best{};
        for (auto e : job_e) {
            const Job* j = world_.storages.get<Job>(e);
            if (!j) continue;
            const int d = dist_of(pid, j->site);
            if (!best.has_value() || d < best->first) best = {d, e};
        }
        if (best.has_value()) {
            job_assignments.push_back(std::pair<uint32_t, Entity>{pid, best->second});
            Job* j = world_.storages.get<Job>(best->second);
            if (j) j->assigned = pid;
            if (Job* j2 = world_.storages.get<Job>(best->second); j2) {
                Pawn* p2 = world_.storages.get<Pawn>(Entity{pid});
                if (p2) p2->mode = Mode{ModeKind::Go, std::vector<TilePos>{j2->site}, 0, j2->site, 0, 0};
            }
        }
    }

    (void)need_claims;
    (void)job_assignments;
    // 3) movement_system: mode Go pawns.
    auto* pslot = world_.storages.find_slot<Pawn>();
    if (pslot) {
        for (uint32_t ei = 0; ei < pslot->items.size(); ++ei) {
            if (!pslot->items[ei].has_value()) continue;
            Pawn& p = *pslot->items[ei];
            const Entity e{ei};
            if (p.mode.kind != ModeKind::Go) continue;
            if (p.mode.path.empty()) { p.mode = Mode{}; continue; }
            const size_t wp = p.mode.waypoint < p.mode.path.size() ? p.mode.waypoint : p.mode.path.size() - 1;
            const TilePos target = p.mode.path[wp];
            const int64_t cx = static_cast<int64_t>(target.x) * (1 << 16) + HALF;
            const int64_t cy = static_cast<int64_t>(target.y) * (1 << 16) + HALF;
            Pos* pos = world_.storages.get<Pos>(e);
            if (!pos) { p.mode = Mode{}; continue; }
            const int64_t dx = cx - pos->x.v;
            const int64_t dy = cy - pos->y.v;
            const int64_t adx = dx >= 0 ? dx : -dx;
            const int64_t ady = dy >= 0 ? dy : -dy;
            const int64_t step_v = p.speed.v;
            if (adx <= HALF && ady <= HALF) {
                pos->x = Fixed1616{cx};
                pos->y = Fixed1616{cy};
            } else if (adx >= ady) {
                pos->x.v += (dx > 0 ? 1 : -1) * std::min(step_v, adx);
            } else {
                pos->y.v += (dy > 0 ? 1 : -1) * std::min(step_v, ady);
            }
            const bool arrived = pos->x.v == cx && pos->y.v == cy;
            if (arrived) {
                p.mode.waypoint++;
                if (p.mode.waypoint >= p.mode.path.size()) p.mode = Mode{};
            }
        }
    }
}

// mode_canon matching Rust mode_canon.
static void mode_canon(Hasher& h, const Mode& m) {
    switch (m.kind) {
        case ModeKind::Idle: h.u8(0); break;
        case ModeKind::Go:
            h.u8(1);
            wu32(h, static_cast<uint32_t>(m.waypoint));
            wu32(h, static_cast<uint32_t>(m.path.size()));
            for (auto& t : m.path) { wi32(h, static_cast<int32_t>(t.x)); wi32(h, static_cast<int32_t>(t.y)); }
            break;
        case ModeKind::Eat: h.u8(2); wi32(h, static_cast<int32_t>(m.site.x)); wi32(h, static_cast<int32_t>(m.site.y)); h.u8(m.timer); break;
        case ModeKind::Sleep: h.u8(3); wi32(h, static_cast<int32_t>(m.site.x)); wi32(h, static_cast<int32_t>(m.site.y)); break;
        case ModeKind::Work: h.u8(4); wu32(h, m.job); wi32(h, static_cast<int32_t>(m.site.x)); wi32(h, static_cast<int32_t>(m.site.y)); break;
    }
}


uint64_t ColonySim::hash() const {
    Hasher h;
    wu32(h, SIM_VERSION);
    h.u64(world_.tick);
    h.u64(world_.rng.state());
    wu32(h, static_cast<uint32_t>(grid_.w()));
    wu32(h, static_cast<uint32_t>(grid_.h()));
    h.u64(static_cast<uint64_t>(grid_.blocked_count()));
    // Pawns (entity-index ascending): id, owner, speed, mode, needs, pos.
    auto pawns = world_.storages.entities<Pawn>();
    std::sort(pawns.begin(), pawns.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    std::vector<Entity> pawns_present;
    for (auto e : pawns) {
        if (!world_.storages.get<Needs>(e)) continue;
        if (!world_.storages.get<Pos>(e)) continue;
        pawns_present.push_back(e);
    }
    wu32(h, static_cast<uint32_t>(pawns_present.size()));
    for (auto e : pawns_present) {
        const Pawn* p = world_.storages.get<Pawn>(e);
        const Needs* n = world_.storages.get<Needs>(e);
        const Pos* pos = world_.storages.get<Pos>(e);
        wu32(h, e.id);
        wu32(h, p->owner);
        h.u64(static_cast<uint64_t>(p->speed.v));
        mode_canon(h, p->mode);
        h.i64(n->hunger);
        h.i64(n->energy);
        h.i64(n->mood);
        h.u64(static_cast<uint64_t>(pos->x.v));
        h.u64(static_cast<uint64_t>(pos->y.v));
    }
    // Sites (entity-index ascending): kind, tile.x, tile.y, occupied_by+1 or 0.
    auto sites = world_.storages.entities<Site>();
    std::sort(sites.begin(), sites.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    wu32(h, static_cast<uint32_t>(sites.size()));
    for (auto e : sites) {
        const Site* s = world_.storages.get<Site>(e);
        if (!s) continue;
        h.u8(s->kind);
        wi32(h, static_cast<int32_t>(s->tile.x));
        wi32(h, static_cast<int32_t>(s->tile.y));
        wu32(h, s->occupied_by.has_value() ? *s->occupied_by + 1u : 0u);
    }
    // Jobs (entity-index ascending): kind, site.x, site.y, assigned+1 or 0, done.
    auto jobs = world_.storages.entities<Job>();
    std::sort(jobs.begin(), jobs.end(), [](const Entity& a, const Entity& b) { return a.id < b.id; });
    wu32(h, static_cast<uint32_t>(jobs.size()));
    for (auto e : jobs) {
        const Job* j = world_.storages.get<Job>(e);
        if (!j) continue;
        h.u8(j->kind);
        wi32(h, static_cast<int32_t>(j->site.x));
        wi32(h, static_cast<int32_t>(j->site.y));
        wu32(h, j->assigned.has_value() ? *j->assigned + 1u : 0u);
        h.u8(j->done ? 1 : 0);
    }
    return h.finish();
}
uint64_t run_colony(uint64_t seed, uint64_t ticks, const std::vector<TickCommands>& batches, SetupFn setup) {
    ColonySim sim(seed);
    if (setup) setup(sim);
    for (uint64_t t = 0; t < ticks; ++t) {
        std::vector<PlayerCommand> batch;
        if (t < batches.size()) batch = batches[t].cmds;
        sim.step(batch);
    }
    return sim.hash();
}

} // namespace sim::rim