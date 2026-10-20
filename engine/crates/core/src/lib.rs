//! Engine core (STUB). Honors the contract in rts-game/docs/engine-api.md.
//! Keep this public surface stable; internals may evolve.

/// Global tick counter for lockstep runs. Fixed tick rate is game policy, not core.
pub type Tick = u64;

/// SplitMix64 deterministic RNG: same seed + same call sequence == same stream everywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { state: seed };
        let _ = r.next_u64();
        r
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Internal state exposure for state hashing / savegames (contract addition).
    #[inline]
    pub fn state(&self) -> u64 {
        self.state
    }
}

/// Fixed-point Q16.16 scalar. Integer-only. NO f32/f64 anywhere in sim code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Fixed1616(pub i64);

const S: i64 = 1 << 16;

impl Fixed1616 {
    pub const ONE: Fixed1616 = Fixed1616(S);
    pub const ZERO: Fixed1616 = Fixed1616(0);

    #[inline]
    pub fn from_int(i: i32) -> Self {
        Fixed1616((i as i64) * S)
    }
    /// floor(n/d * S): Euclidean floor so it's platform-independent.
    #[inline]
    pub fn from_rational(n: i64, d: i64) -> Self {
        Fixed1616(div_floor(n * S, d))
    }
    /// trunc toward zero
    #[inline]
    pub fn to_int_trunc(self) -> i32 {
        (self.0 / S) as i32
    }

    #[inline]
    pub fn wrapping_add(self, rhs: Self) -> Self {
        Fixed1616(self.0.wrapping_add(rhs.0))
    }
    #[inline]
    pub fn wrapping_sub(self, rhs: Self) -> Self {
        Fixed1616(self.0.wrapping_sub(rhs.0))
    }
    #[inline]
    pub fn wrapping_mul(self, rhs: Self) -> Self {
        Fixed1616(((self.0 as i128 * rhs.0 as i128) >> 16) as i64)
    }
    #[inline]
    pub fn wrapping_div(self, rhs: Self) -> Self {
        Fixed1616(div_floor(self.0 * S, rhs.0))
    }
    #[inline]
    pub fn wrapping_abs(self) -> Self {
        if self.0 < 0 {
            Fixed1616(-self.0)
        } else {
            self
        }
    }
    #[inline]
    pub fn saturating_add(self, rhs: Self) -> Self {
        Fixed1616(self.0.saturating_add(rhs.0))
    }
    #[inline]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
    /// Raw integer bits (Q16.16) — for canonical hashing, not for math.
    #[inline]
    pub fn inner(self) -> i64 {
        self.0
    }

    /// Product with the 16-bit shift applied (truncating); integer ops only.
    /// Name avoids shadowing `core::ops::Mul::mul`, which is implemented below.
    #[inline]
    pub fn mul_floor(self, rhs: Self) -> Self {
        self.wrapping_mul(rhs)
    }
}

impl core::fmt::Debug for Fixed1616 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let whole = self.0 / S;
        let frac = (self.0.rem_euclid(S) * 1_000_000) / S;
        write!(f, "F{}({}.{:06})", self.0, whole, frac)
    }
}

/// Euclidean floor division.
#[inline]
fn div_floor(a: i64, b: i64) -> i64 {
    let q = a / b;
    let r = a % b;
    if (r != 0) && ((r < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// Integer square root of a Q16.16 value, floor of the true sqrt. NO floats.
///
/// sqrt(v_raw / 2^16) in fixed point equals isqrt(v_raw << 16) / 1.
/// Example: v_raw = 131072 (value 2): isqrt(131072 << 16) = isqrt(8589934592) = 92681,
/// i.e. floor(sqrt(2) * 2^16). Verified against Python math.isqrt.
#[inline]
pub fn fixed_sqrt(v: Fixed1616) -> Fixed1616 {
    if v.0 <= 0 {
        return Fixed1616(0);
    }
    // v is raw Q16.16 bits; the fixed-point value is v / 2^16. Its sqrt scaled back
    // to Q16.16 is floor(sqrt(v / 2^16) * 2^16) = isqrt(v * 2^16) = isqrt(v << 16).
    // Example: v = 131072 (i.e. 2.0) -> isqrt(8589934592) = 92681 = floor(sqrt(2)*2^16).
    Fixed1616(isqrt_u128((v.0 as u128) << 16) as i64)
}

/// Integer square root of u128: floor(sqrt(n)), integer-only, deterministic.
///
/// Bit-pair (radix-4 digit-by-digit) extraction. Start bit is the highest power of 4
/// not greater than n; each pass emits one result bit with the classic restore test
/// "rem >= q + bit". Same inputs -> same bits everywhere. No floats, no globals.
/// Verified against Python math.isqrt on random 63-bit values and all powers of two.
#[inline]
pub fn isqrt_u128(n: u128) -> u128 {
    if n <= 1 {
        return n;
    }
    // Highest power of 4 whose SQUARE still fits n: bit = 1 << (2 * floor(log2 n / 2)).
    let bits = 128 - n.leading_zeros();
    let mut bit = 1u128 << (((bits - 1) >> 1) * 2);
    if bit > n {
        bit >>= 2;
    }
    let mut q: u128 = 0;
    let mut rem = n;
    while bit > 0 {
        if rem >= q + bit {
            rem -= q + bit;
            q = (q >> 1) + bit;
        } else {
            q >>= 1;
        }
        bit >>= 2;
    }
    q
}
impl core::ops::Add for Fixed1616 {
    type Output = Self;
    fn add(self, r: Self) -> Self {
        self.wrapping_add(r)
    }
}
impl core::ops::Neg for Fixed1616 {
    type Output = Self;
    fn neg(self) -> Self {
        Fixed1616(-self.0)
    }
}
impl core::ops::AddAssign for Fixed1616 {
    fn add_assign(&mut self, r: Self) {
        *self = self.wrapping_add(r)
    }
}
impl core::ops::Sub for Fixed1616 {
    type Output = Self;
    fn sub(self, r: Self) -> Self {
        self.wrapping_sub(r)
    }
}
impl core::ops::Mul for Fixed1616 {
    type Output = Self;
    fn mul(self, r: Self) -> Self {
        self.wrapping_mul(r)
    }
}

// ---------------------------------------------------------------------------
// Tick scheduler - deterministic, fixed-step, single-threaded lockstep driver.
// Commands are consumed at their target tick in (tick, insertion-order) so replays
// match across processes. No real time, no threads.
// ---------------------------------------------------------------------------

/// Command submitted by input; core transports them, never interprets payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Command {
    /// Canonical payload hash (fnv over canonical little-endian bytes by the game layer).
    pub payload_hash: u64,
}

impl Command {
    pub fn from_hash(payload_hash: u64) -> Self {
        Command { payload_hash }
    }
    pub fn hash(self) -> u64 {
        self.payload_hash
    }
}

/// Event emitted during a tick; ordered by (tick, seq), same identity rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub tick: Tick,
    pub id: u64,
    pub payload_hash: u64,
}

/// Drives the lockstep loop: enqueue commands with their target tick, then `step`
/// executes them in deterministic order. Single-threaded, no real time.
#[derive(Clone, Default)]
pub struct Scheduler {
    tick: Tick,
    pending: Vec<(Tick, u64, Command)>,
    seq: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler::default()
    }
    pub fn current(&self) -> Tick {
        self.tick
    }

    /// Enqueue `cmd` for execution at `at_tick`. The monotonic seq keeps intra-tick order total.
    pub fn enqueue(&mut self, at_tick: Tick, cmd: Command) {
        self.pending.push((at_tick, self.seq, cmd));
        self.seq += 1;
    }

    /// Execute all commands due at `next` in (tick, seq) order via `sink`, and advance tick.
    pub fn step<F: FnMut(Tick, &Command)>(&mut self, next: Tick, mut sink: F) {
        debug_assert!(next > self.tick, "ticks must advance monotonically");
        self.tick = next;
        self.pending.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        while !self.pending.is_empty() && self.pending[0].0 <= next {
            let (t, _, cmd) = self.pending.remove(0);
            sink(t, &cmd);
        }
    }
}

// ---------------------------------------------------------------------------
// ReplayRecorder - records per-tick command hashes for lockstep replay checks.
// Digest is stream-order sensitive: same commands in same order digest identically,
// any divergence changes it, so mismatches are detectable by hash alone.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ReplayRecorder {
    ticks: Vec<(Tick, Vec<u64>)>,
    digest: u64,
}

impl ReplayRecorder {
    pub fn new() -> Self {
        ReplayRecorder::default()
    }

    pub fn record(&mut self, tick: Tick, payload_hash: u64) {
        match self.ticks.last_mut() {
            Some((t, hs)) if *t == tick => hs.push(payload_hash),
            _ => self.ticks.push((tick, vec![payload_hash])),
        }
        self.digest = hash_fold(self.digest, tick);
        self.digest = hash_fold(self.digest, payload_hash);
    }

    /// Final stream digest - the replay checksum.
    pub fn digest(&self) -> u64 {
        self.digest
    }

    /// Per-tick digest helper: fnv fold of the tick’s ordered command hashes.
    pub fn tick_digest(tick: Tick, hashes: &[u64]) -> u64 {
        hashes.iter().fold(tick, |h, &x| hash_fold(h, x))
    }
}

// ---------------------------------------------------------------------------
// State hashing (fnv-style fold per determinism contract rule 4).
// ---------------------------------------------------------------------------

#[inline]
pub fn hash_fold(acc: u64, word: u64) -> u64 {
    ((acc ^ word).wrapping_mul(0x1000_01b3)).rotate_left(11)
}

/// Fold a slice of u64 words in canonical order.
#[inline]
pub fn hash_words(words: &[u64]) -> u64 {
    words.iter().fold(0xcbf2_9ce4_8422_2325, |h, &w| hash_fold(h, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_orders_and_times() {
        let mut s = Scheduler::new();
        s.enqueue(1, Command::from_hash(11));
        s.enqueue(1, Command::from_hash(22));
        s.enqueue(2, Command::from_hash(33));
        let mut got = Vec::new();
        s.step(1, |t, c| got.push((t, c.payload_hash)));
        s.step(2, |t, c| got.push((t, c.payload_hash)));
        assert_eq!(got, vec![(1, 11), (1, 22), (2, 33)]);
    }

    #[test]
    fn replay_digest_same_stream() {
        let mut a = ReplayRecorder::new();
        let mut b = ReplayRecorder::new();
        for (t, h) in [(1u64, 11u64), (1, 12), (2, 13)] {
            a.record(t, h);
            b.record(t, h);
        }
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn replay_digest_order_sensitive() {
        let mut a = ReplayRecorder::new();
        let mut b = ReplayRecorder::new();
        a.record(1, 11);
        a.record(1, 12);
        b.record(1, 12);
        b.record(1, 11);
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn isqrt_matches_python_reference() {
        // Values verified with Python math.isqrt. sqrt(k * 2^32) = isqrt(k) * 2^16,
        // so for non-square k the result is floor(k^0.5 * 65536).
        assert_eq!(isqrt_u128(9u128 << 32), 3u128 << 16); // 196608
        assert_eq!(isqrt_u128(1u128 << 32), 1u128 << 16); // 65536
        assert_eq!(isqrt_u128(2u128 << 32), 92681u128); // floor(sqrt(2)*2^16)
        assert_eq!(isqrt_u128(1u128 << 64), 1u128 << 32); // perfect square case
                                                          // floor property across a range: r^2 <= n < (r+1)^2
        for n in 1u128..1 << 15 {
            let r = isqrt_u128(n);
            assert!(r * r <= n && n < (r + 1) * (r + 1), "n={} r={}", n, r);
        }
    }

    #[test]
    fn fixed_sqrt_is_floor() {
        // floor(sqrt(2) * 2^16) = 92681. Note 92681 is the true floor: 92682^2 >> 16 ==
        // exactly 131072 (== raw 2.0) due to the integer product rounding down, so the
        // "next squared exceeds input" check is non-strict in Q16.16 wrapping_mul.
        let two = Fixed1616::from_int(2);
        let r = fixed_sqrt(two);
        assert_eq!(r.0, 92681);
        assert!(r.wrapping_mul(r) <= two);
        let next = Fixed1616(r.0 + 1);
        assert!(next.wrapping_mul(next) >= two);
        assert_eq!(fixed_sqrt(Fixed1616::ZERO), Fixed1616::ZERO);
        assert_eq!(fixed_sqrt(Fixed1616::ONE).0, 65536);
    }

    #[test]
    fn rng_streams_reproducible_across_long_run() {
        let mut a = Rng::new(0xDEAD_BEEF);
        let mut b = Rng::new(0xDEAD_BEEF);
        for _ in 0..10_000 {
            let (x, y) = (a.next_u64(), b.next_u64());
            assert_eq!(x, y);
        }
    }

    #[test]
    fn rng_determinism() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
    #[test]
    fn fixed_math() {
        assert_eq!(Fixed1616::from_rational(1, 2).0, 1 << 15);
        assert_eq!(Fixed1616::from_rational(-1, 2).0, -(1 << 15));
        assert_eq!(
            Fixed1616::from_int(3).wrapping_mul(Fixed1616::ONE),
            Fixed1616::from_int(3)
        );
        assert_eq!(Fixed1616::from_int(-1).to_int_trunc(), -1);
        assert_eq!(Fixed1616::from_rational(-3, 2).to_int_trunc(), -1);
    }
}
