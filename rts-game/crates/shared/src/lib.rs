//! Shared command/event types + canonical state-hash helper.
//!
//! Commands are the ONLY input to the sim (lockstep). The relay sorts by (player, sort_key)
//! so replay ordering is identical everywhere.
//! Hard rules: NO f32/f64, NO default-hash HashMap. Canonical FNV-1a-64 hasher (see below).
//! New Phase-1 commands (Build/Train/Gather) + Phase-2 command variants (Sacrifice/Evolve/AgeUp)
//! carry a kind_disc so sorting stays a TOTAL order and kind_disc churn never changes history
//! (design doc docs/design-factions.md §6; Phase-2 variants are structure-only, no sim behavior).

/// Canonical fixed-point alias (engine-core is the source of truth).
pub type Fixed = engine_core::Fixed1616;

/// Resource enum for Gather. Canonical numeric order: Food, Wood, Gold, Mana.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Resource {
    Food = 0,
    Wood = 1,
    Gold = 2,
    Mana = 3,
}

impl Resource {
    pub fn from_u8(v: u8) -> Option<Resource> {
        match v {
            0 => Some(Resource::Food),
            1 => Some(Resource::Wood),
            2 => Some(Resource::Gold),
            3 => Some(Resource::Mana),
            _ => None,
        }
    }
}

/// Building kinds (faction 1 Kingdom of Aldren for MVP; kind ids are canonical).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum BuildingKind {
    TownCenter = 0,
    Farm = 1,
    House = 2,
    Barracks = 3,
    Watchtower = 4,
    MageTower = 5,
    Wonder = 6,
}

/// Unit kinds (faction 1 MVP; kind ids are canonical, used in Train).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum UnitKind {
    Serf = 0,
    Militia = 1,
    Archer = 2,
}

/// Player command. Tile coordinates are i16 integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Command {
    /// Move unit to tile (x, y); unit paths with engine-path and walks waypoints.
    Move { unit: u32, x: i16, y: i16 },
    /// Attack order onto a target unit.
    Attack { unit: u32, target: u32 },
    /// Clear all orders for unit.
    Stop { unit: u32 },
    /// Place and construct a building (building-kind, tile x, y). Costs checked at apply.
    Build { building: u16, x: i16, y: i16 },
    /// Enqueue training of unit-kind in building entity.
    Train { building: u32, unit_kind: u16 },
    /// Start worker harvest loop for resource.
    Gather { unit: u32, resource: u8 },
    /// Structure-only Phase-2 (faction 2): consume own unit for Essence (design doc §4.6). No sim behavior yet.
    Sacrifice { unit: u32 },
    /// Structure-only Phase-2 (faction 3): evolve tier by consuming units. No sim behavior yet.
    Evolve { tier: u8, unit_type: u16 },
    /// Structure-only Phase-2 (faction 1 Golden Age gate): advance age. No sim behavior yet.
    AgeUp { age: u8 },
}

impl Command {
    /// Canonical sort key inside a tick: (entity, discriminator, params) — total order.
    /// Discriminators (kind_disc) are frozen per design doc §6 — never renumbered.
    pub fn sort_key(&self) -> (u32, u8, i64, i64) {
        match self {
            Command::Move { unit, x, y } => (*unit, 0, *x as i64, *y as i64),
            Command::Attack { unit, target } => (*unit, 1, *target as i64, 0),
            Command::Stop { unit } => (*unit, 2, 0, 0),
            Command::Build { building, x, y } => (*building as u32, 3, *x as i64, *y as i64),
            Command::Train {
                building,
                unit_kind,
            } => (*building, 4, *unit_kind as i64, 0),
            Command::Gather { unit, resource } => (*unit, 5, *resource as i64, 0),
            Command::Sacrifice { unit } => (*unit, 6, 0, 0),
            Command::Evolve { tier, unit_type } => (*unit_type as u32, 7, *tier as i64, 0),
            Command::AgeUp { age } => (*age as u32, 8, 0, 0),
        }
    }
    pub fn kind_disc(&self) -> u8 {
        self.sort_key().1
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
                Command::Build { building, x, y } => {
                    h.u8(3);
                    h.u16(building);
                    h.i16(x);
                    h.i16(y);
                }
                Command::Train {
                    building,
                    unit_kind,
                } => {
                    h.u8(4);
                    h.u32(building);
                    h.u16(unit_kind);
                }
                Command::Gather { unit, resource } => {
                    h.u8(5);
                    h.u32(unit);
                    h.u8(resource);
                }
                Command::Sacrifice { unit } => {
                    h.u8(6);
                    h.u32(unit);
                }
                Command::Evolve { tier, unit_type } => {
                    h.u8(7);
                    h.u8(tier);
                    h.u16(unit_type);
                }
                Command::AgeUp { age } => {
                    h.u8(8);
                    h.u8(age);
                }
            }
        }
        h.finish()
    }
}

/// Canonical write-trait: any canonical hasher (local FNV-1a impl here, future engine impl).
pub trait WriteHash {
    fn write_bytes(&mut self, bytes: &[u8]);
    fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }
    fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn finish(&self) -> u64;
}
#[derive(Clone)]
pub struct Hasher(u64);
impl WriteHash for Hasher {
    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

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
    pub fn u16(&mut self, v: u16) {
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
    UnitSpawned {
        unit: u32,
        x: i16,
        y: i16,
    },
    UnitDied {
        unit: u32,
    },
    AttackFired {
        unit: u32,
        target: u32,
    },
    BuildingPlaced {
        building: u32,
        x: i16,
        y: i16,
    },
    BuildingCompleted {
        building: u32,
    },
    TrainingQueued {
        building: u32,
        kind: u16,
    },
    TrainingDone {
        building: u32,
        unit: u32,
    },
    GatherPickedUp {
        unit: u32,
        resource: u8,
        amount: i32,
    },
    GatherDropped {
        unit: u32,
        resource: u8,
        amount: i32,
    },
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
    fn new_commands_sort_by_disc() {
        // Build(3) < Train(4) < Gather(5) for the same entity.
        let a = Command::Build {
            building: 1,
            x: 5,
            y: 5,
        };
        let b = Command::Train {
            building: 1,
            unit_kind: 0,
        };
        assert_eq!(a.kind_disc(), 3);
        assert_eq!(b.kind_disc(), 4);
        // Phase-2 structure-only commands must occupy frozen disc slots 6/7/8.
        assert_eq!(Command::Sacrifice { unit: 1 }.kind_disc(), 6);
        assert_eq!(
            Command::Evolve {
                tier: 1,
                unit_type: 2
            }
            .kind_disc(),
            7
        );
        assert_eq!(Command::AgeUp { age: 2 }.kind_disc(), 8);
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
