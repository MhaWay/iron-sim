//! Deterministic ECS + scheduler (STUB). Honors the contract in rts-game/docs/engine-api.md.
//! Contract: iteration is by entity index ascending; systems run in registration order;
//! single-threaded. TODO(engine): archetype storage later, keep surface.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(pub u32);

/// Per-component storage; iteration is entity-index ascending (determinism guarantee).
pub struct Storages {
    stores: BTreeMap<TypeId, Box<dyn Any>>,
    alive: Vec<Entity>,
    next_entity: u32,
}

struct VecStorage<T> {
    slots: Vec<Option<T>>,
}
// Safety: T: 'static; erased as Any.

impl Default for Storages {
    fn default() -> Self {
        Self::new()
    }
}

impl Storages {
    pub fn new() -> Self {
        Storages {
            stores: BTreeMap::new(),
            alive: Vec::new(),
            next_entity: 0,
        }
    }

    #[inline]
    fn store_mut<T: 'static>(&mut self) -> &mut Vec<Option<T>> {
        self.stores
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(VecStorage::<T> { slots: Vec::new() }))
            .downcast_mut::<VecStorage<T>>()
            .unwrap()
            .slots
            .as_mut()
    }

    pub fn add_storage<T: 'static>(&mut self) {
        let _ = self.store_mut::<T>();
    }
    pub fn contains<T: 'static>(&self) -> bool {
        self.stores.contains_key(&TypeId::of::<T>())
    }

    #[inline]
    pub fn spawn(&mut self) -> Entity {
        let e = Entity(self.next_entity);
        self.next_entity += 1;
        self.alive.push(e);
        e
    }

    #[inline]
    pub fn add<T: 'static>(&mut self, e: Entity, v: T) {
        let slots = self.store_mut::<T>();
        let idx = e.0 as usize;
        while slots.len() <= idx {
            slots.push(None);
        }
        slots[idx] = Some(v);
    }

    #[inline]
    pub fn get<T: 'static>(&self, e: Entity) -> Option<&T> {
        self.stores
            .get(&TypeId::of::<T>())?
            .downcast_ref::<VecStorage<T>>()?
            .slots
            .get(e.0 as usize)?
            .as_ref()
    }

    #[inline]
    pub fn get_mut<T: 'static>(&mut self, e: Entity) -> Option<&mut T> {
        let store = self
            .stores
            .get_mut(&TypeId::of::<T>())?
            .downcast_mut::<VecStorage<T>>()?;
        store.slots.get_mut(e.0 as usize)?.as_mut()
    }

    #[inline]
    pub fn remove<T: 'static>(&mut self, e: Entity) {
        if let Some(b) = self.stores.get_mut(&TypeId::of::<T>()) {
            if let Some(vs) = b.downcast_mut::<VecStorage<T>>() {
                if let Some(slot) = vs.slots.get_mut(e.0 as usize) {
                    *slot = None;
                }
            }
        }
    }

    /// (Entity, &T) in ascending entity-index order. This is the determinism-critical primitive.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        let slots = self
            .stores
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<VecStorage<T>>())
            .map(|v| v.slots.as_slice());
        slots.into_iter().flat_map(|s| {
            s.iter()
                .enumerate()
                .filter_map(|(i, o)| o.as_ref().map(|v| (Entity(i as u32), v)))
        })
    }

    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.alive.iter().copied()
    }

    /// Drop every entry of type T (keeps entity ids alive; used by map installation).
    pub fn clear<T: 'static>(&mut self) {
        if let Some(b) = self.stores.get_mut(&TypeId::of::<T>()) {
            if let Some(vs) = b.downcast_mut::<VecStorage<T>>() {
                for slot in vs.slots.iter_mut() {
                    *slot = None;
                }
            }
        }
    }

        pub fn despawn(&mut self, e: Entity) {
        if let Some(i) = self.alive.iter().position(|x| *x == e) {
            self.alive.remove(i);
        }
    }
}

pub type SystemFn = Box<dyn FnMut(&mut World, engine_core::Tick)>;

/// Systems run in registration order, exactly once per step(). Single-threaded.
pub struct Scheduler {
    systems: Vec<(&'static str, SystemFn)>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler { systems: Vec::new() }
    }
    pub fn add_system<S: Into<&'static str>, F: FnMut(&mut World, engine_core::Tick) + 'static>(
        &mut self,
        name: S,
        f: F,
    ) {
        self.systems.push((name.into(), Box::new(f)));
    }
    pub fn step(&mut self, world: &mut World, tick: engine_core::Tick) {
        for (_n, sys) in self.systems.iter_mut() {
            sys(world, tick);
        }
    }
}

pub struct World {
    pub storages: Storages,
    pub rng: engine_core::Rng,
    pub tick: engine_core::Tick,
}

impl World {
    pub fn new(seed: u64) -> Self {
        World {
            storages: Storages::new(),
            rng: engine_core::Rng::new(seed),
            tick: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq)]
    struct Hp(u32);
    #[test]
    fn add_get_iter_order() {
        let mut w = World::new(1);
        let a = w.storages.spawn();
        let b = w.storages.spawn();
        w.storages.add(a, Hp(5));
        w.storages.add(b, Hp(7));
        let got: Vec<(u32, u32)> = w.storages.iter::<Hp>().map(|(e, h)| (e.0, h.0)).collect();
        assert_eq!(got, vec![(0, 5), (1, 7)]);
        assert_eq!(w.storages.get(a), Some(&Hp(5)));
    }
    #[test]
    fn scheduler_runs_in_registration_order() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let mut w = World::new(1);
        let mut s = Scheduler::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let l1 = log.clone();
        s.add_system("a", move |_, t| l1.borrow_mut().push(('a', t)));
        let l2 = log.clone();
        s.add_system("b", move |_, t| l2.borrow_mut().push(('b', t)));
        s.step(&mut w, 0);
        assert_eq!(*log.borrow(), vec![('a', 0u64), ('b', 0u64)]);
    }
}
