//! Shared command/event types + canonical state-hash helper.
//!
//! Commands are the ONLY input to the sim (lockstep). Relay sorts commands by
//! (player, Command::sort_key) so replay ordering is identical everywhere.
//! Hard rules: NO f32/f64, NO default-hash HashMap. Hashing is our own canonical
//! FNV-1a-64 implementation (see Hasher below) so we control bit-exactness.

/// Canonical fixed-point alias (engine-core is the source of truth).
pub type Fixed = engine_core::Fixed1616;

/// Player command. Tile coordinates are i16 integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Command {
    /// Move unit to tile (x, y); unit paths with engine-path and walks waypoints.
    Move { unit: u32, x: i16, y: i16 },
    /// Attack order onto a target unit.
    Attack { unit: u32, target: u32 },
    /// Clear all orders for unit.
    Stop { unit: u32 },
}

impl Command {
    /// Canonical sort key inside a tick: (unit, discriminator, params) — total order.
    pub fn sort_key(&self) -> (u32, u8, i64, i64) {
        match self {
            Command::Move { unit, x, y } => (*unit, 0, *x as i64, *y as i64),
            Command::Attack { unit, target } => (*unit, 1, *target as i64, 0),
            Command::Stop { unit } => (*unit, 2, 0, 0),
        }
    }
}

/// Command batch assigned to tick t (the relay's wire format).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickCommands {
    pub tick: u64,
    pub cmds: Vec<(u32, Command)>,
}

impl TickCommands {
    /// Canonical order: player, then Command::sort_key.
    pub fn sort(&mut self) {
        self.cmds
            .sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
    }
    pub fn hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.u64(self.tick);
        h.u32(self.cmds.len() as u32);
        for (player, c) in &self.cmds {
            h.u32(*player);
            match *c {
                Command::Move { unit, x, y } => {
                    h.u8(0);
                    h.u32(unit);
                    h.i16(x);
                    h.i16(y);
                }
                Command::Attack { unit, target } => {
                    h.u8(1);
                    h.u32(unit);
                    h.u32(target);
                }
                Command::Stop { unit } => {
                    h.u8(2);
                    h.u32(unit);
                }
            }
        }
        h.finish()
    }
}

/// Canonical little-endian FNV-1a-64 hasher, implemented locally so bit-exactness is ours.
/// (Not std::HashMap hasher; deterministic across platforms.)
#[derive(Clone)]
pub struct Hasher(u64);
impl Hasher {
    pub fn new() -> Self {
        Hasher(0xcbf2_9ce4_8422_2325)
    }
    #[inline]
    pub fn u8(&mut self, v: u8) {
        self.byte(v)
    }
    #[inline]
    fn byte(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(0x100_0000_01b3);
    }
    #[inline]
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes())
    }
    #[inline]
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes())
    }
    #[inline]
    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes())
    }
    #[inline]
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes())
    }
    #[inline]
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.byte(x);
        }
    }
    pub fn finish(self) -> u64 {
        self.0
    }
}
impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-tick simulation event emitted for clients / replay UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimEvent {
    UnitSpawned { unit: u32, x: i16, y: i16 },
    UnitDied { unit: u32 },
    AttackFired { unit: u32, target: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_sort_is_total() {
        let mut t = TickCommands {
            tick: 0,
            cmds: vec![
                (
                    1,
                    Command::Move {
                        unit: 3,
                        x: 1,
                        y: 2,
                    },
                ),
                (0, Command::Attack { unit: 2, target: 1 }),
                (
                    0,
                    Command::Move {
                        unit: 2,
                        x: 0,
                        y: 0,
                    },
                ),
                (0, Command::Stop { unit: 1 }),
            ],
        };
        t.sort();
        assert_eq!(t.cmds[0].1.sort_key().0, 1);
        assert_eq!(t.cmds[1].1.sort_key().0, 2);
        assert_eq!(t.cmds[1].1.sort_key().1, 0); // Move(2) before Attack(2)
    }
    #[test]
    fn hash_stable_and_order_sensitive() {
        let mut t = TickCommands {
            tick: 3,
            cmds: vec![(
                0,
                Command::Move {
                    unit: 1,
                    x: 4,
                    y: 5,
                },
            )],
        };
        let h1 = t.hash();
        assert_eq!(h1, t.hash());
        t.cmds.push((1, Command::Stop { unit: 2 }));
        assert_ne!(h1, t.hash());
    }
    #[test]
    fn fnv1a_known_vectors() {
        // FNV-1a 64-bit, verified against an independent reference implementation.
        // Empty string = FNV offset basis. "a" and "abc" vectors from the same reference.
        assert_eq!(Hasher::new().finish(), 0xcbf2_9ce4_8422_2325);
        let mut h = Hasher::new();
        h.u8(b'a');
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
        let mut h = Hasher::new();
        h.u8(b'a');
        h.u8(b'b');
        h.u8(b'c');
        assert_eq!(h.finish(), 0xe71f_a219_0541_574b);
    }
}
