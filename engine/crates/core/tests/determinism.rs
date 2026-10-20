//! Determinism proof: two identical 10 000-tick runs must produce identical state hashes.
//! Rule (AGENTS.md): 2 run uguali -> stesso hash. This test is core-only (no ECS dep).

use engine_core::{Command, ReplayRecorder, Rng, Scheduler};

/// Deterministic command stream: per-tick hash derived from seed and tick,
/// so both runs enqueue exactly the same commands in the same order.
fn cmd_hash_for(seed: u64, t: u64) -> u64 {
    let mut r = Rng::new(seed ^ t.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    r.next_u64()
}

/// Run `n_ticks` ticks; return (final state hash, replay digest).
fn run(seed: u64, n_ticks: u64) -> (u64, u64) {
    let mut rng = Rng::new(seed);
    let mut sched = Scheduler::new();
    let mut rec = ReplayRecorder::new();
    let mut entities: Vec<(i32, i32, i32, i32)> = (0..4).map(|i| (i, i, 0, 1)).collect();
    let mut acc = 0u64;
    for t in 1..=n_ticks {
        let ch = cmd_hash_for(seed, t);
        sched.enqueue(t, Command::from_hash(ch));
        rec.record(t, ch);
        sched.step(t, |_t, c| acc ^= c.hash());
        let j = rng.next_u64();
        for e in entities.iter_mut() {
            e.2 = e.2.wrapping_add(e.3);
            e.3 = e.3.wrapping_add(((j >> 40) as i32) & 3);
        }
    }
    (state_hash(&entities, rng.state(), sched.current(), acc), rec.digest())
}

/// Canonical state hash: fnv fold over every state word in fixed field order.
fn state_hash(entities: &[(i32, i32, i32, i32)], rng_state: u64, tick: u64, acc: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut fold = |w: u64| h = (h ^ w).wrapping_mul(0x1000_01b3).rotate_left(11);
    fold(tick);
    fold(rng_state);
    fold(acc);
    for e in entities {
        for w in [e.0 as u64, e.1 as u64, e.2 as u64, e.3 as u64] {
            fold(w);
        }
    }
    h
}

#[test]
fn ten_thousand_ticks_twice_identical() {
    let (h1, d1) = run(0xC0FFEE, 10_000);
    let (h2, d2) = run(0xC0FFEE, 10_000);
    assert_eq!(h1, h2, "state hashes must match for identical 10k-tick runs");
    assert_eq!(d1, d2, "replay digests must match for identical 10k-tick runs");
}

#[test]
fn different_seed_diverges() {
    let (h1, _) = run(1, 1000);
    let (h2, _) = run(2, 1000);
    assert_ne!(h1, h2, "different seeds must produce different states");
}
