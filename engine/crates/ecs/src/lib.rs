//! Deterministic ECS + scheduler. Contract: iteration by entity index ascending;
//! systems run in registration order; single-threaded.
//!
//! Storages: components type-erased (Box<dyn Any>). Column deep-clone for
//! session snapshots requires registering a cloner via add_storage_clone::<T>().

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity(pub u32);

/// Per-component storage; iteration is entity-index ascending (determinism guarantee).
/// Components are stored type-erased (`Box<dyn Any>`) as a Vec<Option<T>> per type.
/// For deep-clone of the whole storage (session snapshots), components whose type
/// implements `Clone` must be registered via `add_storage_clone::<T>()`; the
/// registration is what enables `Storages::clone` to duplicate that column.
pub struct Storages {
    stores: BTreeMap<TypeId, Box<dyn Any>>,
    cloners: BTreeMap<TypeId, Cloner>,
    alive: Vec<Entity>,
    next_entity: u32,
}

/// Deep-clones one component column between storage maps.
type Cloner = fn(&BTreeMap<TypeId, Box<dyn Any>>, &mut Storages);

fn make_cloner<T: Clone + 'static>() -> Cloner {
    fn do_clone<T: Clone + 'static>(src: &BTreeMap<TypeId, Box<dyn Any>>, dst: &mut Storages) {
        if let Some(boxed) = src.get(&TypeId::of::<T>()) {
            if let Some(vs) = boxed.downcast_ref::<VecStorage<T>>() {
                dst.stores.insert(
                    TypeId::of::<T>(),
                    Box::new(VecStorage {
                        slots: vs.slots.clone(),
                    }),
                );
            }
        }
    }
    do_clone::<T>
}
struct VecStorage<T> {
    slots: Vec<Option<T>>,
}

impl Default for Storages {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Storages {
    fn clone(&self) -> Self {
        let mut dst = Storages {
            stores: BTreeMap::new(),
            cloners: self.cloners.clone(),
            alive: self.alive.clone(),
            next_entity: self.next_entity,
        };
        for (_id, cloner) in &self.cloners {
            cloner(&self.stores, &mut dst);
        }
        dst
    }
}
impl Storages {
    pub fn new() -> Self {
        Storages {
            stores: BTreeMap::new(),
            cloners: BTreeMap::new(),
            alive: Vec::new(),
            next_entity: 0,
        }
    }

    fn ensure_slot<T: Clone + 'static>(&mut self) {
        self.cloners.insert(TypeId::of::<T>(), make_cloner::<T>());
        let _ = self
            .stores
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(VecStorage::<T> { slots: Vec::new() }));
    }

    fn typed_slots_mut<T: Clone + 'static>(&mut self) -> &mut Vec<Option<T>> {
        self.ensure_slot::<T>();
        self.stores
            .get_mut(&TypeId::of::<T>())
            .unwrap()
            .downcast_mut::<VecStorage<T>>()
            .unwrap()
            .slots
            .as_mut()
    }

    fn typed_slots<T: 'static>(&self) -> Option<&[Option<T>]> {
        self.stores
            .get(&TypeId::of::<T>())
            .map(|b| b.downcast_ref::<VecStorage<T>>().unwrap().slots.as_slice())
    }
    pub fn add_storage<T: Clone + 'static>(&mut self) {
        self.ensure_slot::<T>();
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
    pub fn add<T: Clone + 'static>(&mut self, e: Entity, v: T) {
        let slots = self.typed_slots_mut::<T>();
        let idx = e.0 as usize;
        while slots.len() <= idx {
            slots.push(None);
        }
        slots[idx] = Some(v);
    }

    #[inline]
    pub fn get<T: 'static>(&self, e: Entity) -> Option<&T> {
        self.typed_slots::<T>()?.get(e.0 as usize)?.as_ref()
    }

    #[inline]
    pub fn get_mut<T: Clone + 'static>(&mut self, e: Entity) -> Option<&mut T> {
        self.typed_slots_mut::<T>().get_mut(e.0 as usize)?.as_mut()
    }

    #[inline]
    pub fn remove<T: Clone + 'static>(&mut self, e: Entity) {
        if let Some(slots) = self.stores.get_mut(&TypeId::of::<T>()) {
            let s = slots.downcast_mut::<VecStorage<T>>().unwrap().slots.as_mut_slice();
            if let Some(slot) = s.get_mut(e.0 as usize) {
                *slot = None;
            }
        }
    }

    /// (Entity, &T) in ascending entity-index order. This is the determinism-critical primitive.
    pub fn iter<T: Clone + 'static>(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        let slots = self.typed_slots::<T>();
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
    pub fn clear<T: Clone + 'static>(&mut self) {
        if let Some(b) = self.stores.get_mut(&TypeId::of::<T>()) {
            let vs = b.downcast_mut::<VecStorage<T>>().unwrap();
            for slot in vs.slots.iter_mut() {
                *slot = None;
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
// SAFETY (deterministic sim contract): component state is pure integer data mutated
// single-threaded by the sim; cross-thread use is read-only via snapshots (Arc<SendSim>).
#[allow(unused_unsafe, clippy::undocumented_unsafe_blocks)]
unsafe impl Send for Storages {}
unsafe impl Sync for Storages {}
unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

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

impl Clone for World {
    fn clone(&self) -> Self {
        World {
            storages: self.storages.clone(),
            rng: self.rng.clone(),
            tick: self.tick,
        }
    }
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
    #[derive(Debug, PartialEq, Clone)]
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
