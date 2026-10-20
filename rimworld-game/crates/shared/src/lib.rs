//! Command / event types and the canonical state hasher.
//!
//! Canonicalization: commands sorted by (player_id, command sort_key); hashing via
//! FNV-1a 64-bit little-endian (see docs/engine-api.md determinism contract).

/// Player-issued command. Transport order never matters — Sim::step sorts canonically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Move a pawn to a tile.
    Move { pawn: u32, x: i16, y: i16 },
    /// Order a pawn to idle (drops its current job/assignment).
    Wait { pawn: u32 },
    /// Designate a tile for building (0 = storage shelf, 1 = bed, 2 = table).
    /// Built instantly in this scaffold; sim only validates reachability.
    Build { tile: i16, y: i16, kind: u8 },
}

impl Command {
    /// Total order key for canonical sorting: (pawn, kind, x, y).
    pub fn sort_key(&self) -> (u32, u8, i16, i16) {
        match self {
            Command::Move { pawn, x, y } => (*pawn, 0, *x, *y),
            Command::Wait { pawn } => (*pawn, 1, 0, 0),
            Command::Build { tile, y, kind } => (*kind as u32, 2, *tile, *y),
        }
    }
}

/// A tick's batch of commands from all players: (player_id, command).
#[derive(Clone, Debug, Default)]
pub struct TickCommands {
    pub tick: u64,
    pub cmds: Vec<(u32, Command)>,
}

impl TickCommands {
    /// Canonical (player, sort_key) order — independent of transport order.
    pub fn sort(&mut self) {
        self.cmds
            .sort_by(|(pa, a), (pb, b)| pa.cmp(pb).then_with(|| a.sort_key().cmp(&b.sort_key())));
    }
}

/// Canonical state hasher: FNV-1a 64-bit, little-endian words, canonical field order.
#[derive(Clone)]
pub struct Hasher {
    h: u64,
}

impl Hasher {
    pub const OFFSET: u64 = 0xcbf29ce484222325;
    pub const PRIME: u64 = 0x100000001b3;
    pub fn new() -> Self {
        Hasher { h: Self::OFFSET }
    }
    pub fn write_u64(&mut self, n: u64) {
        let b = n.to_le_bytes();
        self.h ^= b[0] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[1] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[2] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[3] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[4] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[5] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[6] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
        self.h ^= b[7] as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
    }
    pub fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64)
    }
    pub fn write_i32(&mut self, n: i32) {
        self.write_u64(n as u64)
    }
    pub fn write_i64(&mut self, n: i64) {
        self.write_u64(n as u64)
    }
    pub fn write_u8(&mut self, n: u8) {
        self.h ^= n as u64;
        self.h = self.h.wrapping_mul(Self::PRIME);
    }
    pub fn finish(self) -> u64 {
        self.h
    }
}
impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fnv1a_known_vectors() {
        // FNV-1a 64, verified against an independent reference implementation.
        assert_eq!(Hasher::new().finish(), 0xcbf2_9ce4_8422_2325);
        let mut h = Hasher::new();
        h.write_u8(b'a');
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
        let mut h = Hasher::new();
        h.write_u8(b'a');
        h.write_u8(b'b');
        h.write_u8(b'c');
        assert_eq!(h.finish(), 0xe71f_a219_0541_574b);
    }
    #[test]
    fn command_sort_is_total() {
        let mut b = TickCommands {
            tick: 0,
            cmds: vec![
                (1, Command::Wait { pawn: 3 }),
                (
                    0,
                    Command::Move {
                        pawn: 1,
                        x: 5,
                        y: 2,
                    },
                ),
                (
                    1,
                    Command::Move {
                        pawn: 1,
                        x: 5,
                        y: 2,
                    },
                ),
                (
                    0,
                    Command::Move {
                        pawn: 1,
                        x: 9,
                        y: 2,
                    },
                ),
                (
                    1,
                    Command::Build {
                        tile: 1,
                        y: 1,
                        kind: 2,
                    },
                ),
            ],
        };
        let mut canonical = b.cmds.clone();
        canonical
            .sort_by(|(pa, a), (pb, b2)| pa.cmp(pb).then_with(|| a.sort_key().cmp(&b2.sort_key())));
        let mut shuffled: Vec<(u32, Command)> = b.cmds.clone();
        shuffled.swap(0, 4);
        shuffled.swap(1, 3);
        b.cmds = shuffled;
        b.sort();
        assert_eq!(b.cmds, canonical);
        // sort is idempotent
        let s2 = b.cmds.clone();
        b.sort();
        assert_eq!(b.cmds, s2);
    }
}
