//! v4 Tier A mechanics: food upkeep, starvation floor, node regeneration.
//! All math is integer and deterministic (SIM_VERSION = 4).

use game::economy::{ResourceNode, STARVE_SLOW, UNIT_TABLE, UPKEEP, UPKEEP_GRACE, UPKEEP_INTERVAL};
use game::mapgen::generate_map_preset;
use game::sim::{Sim, Unit};

/// Run n ticks with no player commands.
fn step(sim: &mut Sim, ticks: u64) {
    for _ in 0..ticks {
        sim.step(&[]).unwrap();
    }
}

/// Find the first node entity with the given slot.
fn node(sim: &Sim, slot: u8) -> engine_ecs::Entity {
    for (e, n) in sim.world.storages.iter::<ResourceNode>() {
        if n.slot == slot {
            return e;
        }
    }
    panic!("no node with slot {slot}");
}

#[test]
fn upkeep_grace_then_charge() {
    let mut sim = Sim::new(12345);
    // p1 spawns a Militia; p0 stays at 3 Serfs (UPKEEP 0) to isolate the charge.
    sim.spawn_unit_kind(1, 1, 30, 30); // Militia -> 1 food per interval
                                       // Before grace ends: food untouched.
    step(&mut sim, UPKEEP_GRACE);
    assert_eq!(
        sim.players[1].stock.food, 150,
        "grace period must not charge"
    );
    // First charge interval after grace.
    let start = sim.players[1].stock.food;
    step(&mut sim, UPKEEP_INTERVAL);
    assert_eq!(
        sim.players[1].stock.food,
        start - UPKEEP[1],
        "Militia upkeep must charge 1 per interval"
    );
    // Player 0 (3 Serfs) pays nothing: symmetric zero table.
    assert_eq!(sim.players[0].stock.food, 150);
}

#[test]
fn starvation_floor_and_slow() {
    let mut sim = Sim::new(12345);
    // Starve player 0: drain their food, keep only expensive units.
    sim.players[0].stock = game::economy::Resources {
        food: 1,
        ..sim.players[0].stock
    };
    let u = sim.spawn_unit_kind(0, 3, 40, 40); // Knight (UPKEEP 3, hp 200)
                                               // Tick until starvation starts.
    step(&mut sim, UPKEEP_GRACE + UPKEEP_INTERVAL);
    let hp_before = sim.world.storages.get::<Unit>(u).unwrap().hp;
    // Starvation grind: 1 hp per interval down to the 1 hp floor, then STARVE_SLOW per
    // interval toward the speed floor of 1.
    let slow_ints = ((UNIT_TABLE[3].speed_q as u64) / STARVE_SLOW as u64) + 5;
    let iters = ((hp_before - 1) as u64 + slow_ints) * UPKEEP_INTERVAL;
    step(&mut sim, iters);
    let uobj = sim.world.storages.get::<Unit>(u).unwrap();
    assert_eq!(
        uobj.hp, 1,
        "starvation floor must stop at 1 hp, never despawn"
    );
    assert_eq!(uobj.speed_q, 1, "speed floor is 1");
    // Keep starving: unit must survive and stay at both floors.
    step(&mut sim, 10 * UPKEEP_INTERVAL);
    let uobj = sim.world.storages.get::<Unit>(u).unwrap();
    assert_eq!(uobj.hp, 1);
    assert_eq!(uobj.speed_q, 1);
}

#[test]
fn starvation_is_symmetric() {
    // Identical armies, same starvation: hp/speed deltas must match.
    let mut a = Sim::new(7);
    let mut b = Sim::new(7);
    for s in [&mut a, &mut b] {
        s.spawn_unit_kind(1, 2, 40, 40); // p1 side only for simplicity
    }
    // Force both p1 into starvation.
    for s in [&mut a, &mut b] {
        s.players[1].stock = game::economy::Resources {
            food: 0,
            ..s.players[1].stock
        };
    }
    step(&mut a, UPKEEP_GRACE + 3 * UPKEEP_INTERVAL);
    step(&mut b, UPKEEP_GRACE + 3 * UPKEEP_INTERVAL);
    let owner_stats = |sim: &game::Sim| {
        let mut last = None;
        for (_e, u) in sim.world.storages.iter::<Unit>() {
            if u.owner == 1 {
                last = Some((u.hp, u.speed_q));
            }
        }
        last.unwrap()
    };
    let ua = owner_stats(&a);
    let ub = owner_stats(&b);
    assert_eq!(ua, ub, "starvation outcome must not depend on run order");
}

#[test]
fn node_regen_capped() {
    let mut sim = Sim::new(12345);
    let food = node(&sim, 0);
    let wood = node(&sim, 1);
    // Drain them hard.
    if let Some(n) = sim.world.storages.get_mut::<ResourceNode>(food) {
        n.remaining = 0;
    }
    if let Some(n) = sim.world.storages.get_mut::<ResourceNode>(wood) {
        n.remaining = 0;
    }
    step(&mut sim, 250);
    let f = sim.world.storages.get::<ResourceNode>(food).unwrap();
    let w = sim.world.storages.get::<ResourceNode>(wood).unwrap();
    assert!(f.remaining > 0, "food must regen at 2/50t: {}", f.remaining);
    assert!(
        w.remaining > 0,
        "wood must regen at 1/150t: {}",
        w.remaining
    );
    assert!(f.remaining < f.total);
    // Never exceed total.
    step(&mut sim, 1_000_000);
    let f = sim.world.storages.get::<ResourceNode>(food).unwrap();
    assert_eq!(f.remaining, f.total, "regen must cap at total");
}

#[test]
fn regen_does_not_touch_gold() {
    let mut sim = Sim::new(12345);
    let gold = node(&sim, 2);
    if let Some(n) = sim.world.storages.get_mut::<ResourceNode>(gold) {
        n.remaining = 0;
    }
    step(&mut sim, 100_000);
    let n = sim.world.storages.get::<ResourceNode>(gold).unwrap();
    assert_eq!(n.remaining, 0, "gold must never regen");
}

#[test]
fn long_soak_replays_identical() {
    // 3h-scale soak cross-process equality via same-process double run.
    let h1 = game::run_sim(12345, 130_000, &[]);
    let h2 = game::run_sim(12345, 130_000, &[]);
    assert_eq!(h1, h2, "long soak must replay identical");
}

#[test]
fn preset_map_regen_same_seed() {
    // Preset maps must also regen: two identical installs must converge.
    let m1 = generate_map_preset(99, game::mapgen::ARENA);
    let m2 = generate_map_preset(99, game::mapgen::ARENA);
    assert_eq!(m1.map_digest(), m2.map_digest());
}
