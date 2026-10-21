#pragma once
// Minimal deterministic ECS mirroring engine/crates/ecs (Rust): typed sparse-vec
// storages keyed by type id, entity index ascending iteration order.

#include <algorithm>
#include <cstdint>
#include <memory>
#include <optional>
#include <tuple>
#include <utility>
#include <vector>

#include "sim/engine.hpp"

namespace sim {

struct Entity {
    uint32_t id;
    bool operator==(const Entity&) const = default;
    bool operator<(const Entity& o) const noexcept { return id < o.id; }
};

namespace detail {

template <typename>
struct Slot; // forward declare

inline int64_t next_type_id() {
    static int64_t counter = 0;
    return counter++;
}
template <typename T>
int64_t tid() {
    static const int64_t id = next_type_id();
    return id;
}

struct SlotBase {
    virtual ~SlotBase() = default;
};

template <typename T>
struct Slot : SlotBase {
    std::vector<std::optional<T>> items;
};

} // namespace detail

class Storages {
public:
    Entity spawn() noexcept {
        const uint32_t id = next_entity_++;
        alive_.push_back(id);
        return Entity{id};
    }

    template <typename T>
    detail::Slot<T>& slot() noexcept {
        const int64_t id = detail::tid<T>();
        for (auto& [k, v] : stores_)
            if (k == id) return static_cast<detail::Slot<T>&>(*v);
        stores_.push_back({id, std::make_unique<detail::Slot<T>>()});
        return static_cast<detail::Slot<T>&>(*stores_.back().second);
    }

    template <typename T>
    void add(Entity e, T value) {
        auto& s = slot<T>();
        const auto i = e.id;
        if (s.items.size() <= i) s.items.resize(i + 1, std::nullopt);
        s.items[i] = std::move(value);
    }

    template <typename T>
    detail::Slot<T>* find_slot() noexcept { return &slot<T>(); }

    template <typename T>
    const detail::Slot<T>* find_slot() const noexcept {
        const int64_t id = detail::tid<T>();
        for (auto& [k, v] : stores_)
            if (k == id) return static_cast<const detail::Slot<T>*>(v.get());
        return nullptr;
    }

    template <typename T>
    T* get(Entity e) noexcept {
        auto* s = find_slot<T>();
        if (!s || e.id >= s->items.size() || !s->items[e.id]) return nullptr;
        return std::addressof(*s->items[e.id]);
    }

    template <typename T>
    const T* get(Entity e) const noexcept {
        const auto* s = find_slot<T>();
        if (!s || e.id >= s->items.size() || !s->items[e.id]) return nullptr;
        return std::addressof(*s->items[e.id]);
    }

    template <typename T>
    void remove(Entity e) noexcept {
        auto* s = find_slot<T>();
        if (s && e.id < s->items.size()) s->items[e.id].reset();
    }

    template <typename T, typename Fn>
    void each(Fn&& fn) const {
        const auto* s = find_slot<T>();
        if (!s) return;
        for (uint32_t i = 0; i < s->items.size(); ++i)
            if (s->items[i].has_value()) fn(Entity{i}, *s->items[i]);
    }

    template <typename T>
    std::vector<Entity> entities() const {
        std::vector<Entity> out;
        const auto* s = find_slot<T>();
        if (!s) return out;
        for (uint32_t i = 0; i < s->items.size(); ++i)
            if (s->items[i].has_value()) out.push_back(Entity{i});
        return out;
    }

    template <typename T>
    std::size_t count() const {
        const auto* s = find_slot<T>();
        if (!s) return 0;
        std::size_t n = 0;
        for (const auto& o : s->items)
            if (o.has_value()) ++n;
        return n;
    }

    void despawn(Entity e) noexcept {
        alive_.erase(std::find(alive_.begin(), alive_.end(), e.id));
    }

    const std::vector<uint32_t>& alive() const noexcept { return alive_; }

private:
    uint32_t next_entity_ = 0;
    std::vector<uint32_t> alive_;
    std::vector<std::pair<int64_t, std::unique_ptr<detail::SlotBase>>> stores_;
};

struct World {
    Storages storages;
    engine::Rng rng;
    engine::Tick tick = 0;
    explicit World(uint64_t seed) : rng(seed) {}
};

} // namespace sim