//! Economy layer: resource tables, building/unit specs, visibility, harvest state,
//! construction/harvest jobs, training queues, per-player state, node positions.
//!
//! Data-only faction handling (AGENTS.md): all player-visible content is faction 1 (Kingdom of
//! Aldren) expressed through the shared resource-kind slots (Food/Wood/Gold/Mana) — no
//! `if player == faction` branch anywhere. Costs/times/stockpiles are numeric tables; the design
//! doc (docs/design-factions.md) is the source of truth and the balance knob.
//!
//! v2 adds the AoE loop: age ladder (AgeUp costs + timer), walls (destructible, block movement),
//! Factory, training gated by building+age+pop, rally points, and a win-condition layer. Civ
//! display names (Adeptus Terra = Faith/Promethium/Ore; Terran Dominion = Minerals/Vespene/
//! Terrazine) are pure display data over the SAME numeric slots — zero "if player == faction"
//! branches in the sim, same wire format. Sacrifice/Evolve stay structure-only. The hash includes
//! fog visibility via an explicit Visibility component (no dead storage types).

use shared::WriteHash;

/// 128x128 tiles, deterministic generation from the seed.
pub const MAP_W: u16 = 128;
pub const MAP_H: u16 = 128;
pub const SIGHT_BASE: i32 = 5;
/// One resource payload per harvest trip.
pub const CARRY: i64 = 10;
/// Ticks to fill one payload at a node (rate is payload/ticks).
pub const PICK_TICKS: u32 = 150;
/// Food ticks spent eating when idle without work (keeps economy ticking).
pub const EAT_COST: i64 = 30;
/// Age-ladder length (Dark 0 -> Feudal 1 -> Castle 2 -> Imperial 3).
pub const MAX_AGE: u8 = 3;
/// Pop cap: TownCenter + Farms supply population (units).
pub const POP_PER_TC: u32 = 8;
pub const POP_PER_FARM: u32 = 4;
/// Wall kind id (BuildingKind::Wall) and its stats (table entry 8 is the source of truth).
pub const WALL_KIND: u16 = 8;
pub const FACTORY_KIND: u16 = 7;
/// AgeUp travel ticks per target age (0,600,900,1200 = 20/30/40 s @30 Hz).
pub const AGE_UP_TICKS: [u32; 4] = [0, 600, 900, 1200];
/// AgeUp costs per target age, indexed [to_age][slot 0=food(gold),1=wood(stone),2=gold(ore-stone)].
/// Wire slots stay food/wood/gold/mana; civ display names map slot0->Faith/Minerals etc.
pub const AGE_COST: [[i64; 3]; 4] = [[0, 0, 0], [300, 150, 0], [600, 350, 250], [900, 550, 450]];
/// Tick budget for the score win (20 min @ 30 Hz).
pub const MATCH_TICKS: u64 = 36000;

/// Canonical per-resource storage: fields indexed by the resource slot, NEVER a hashmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub food: i64,
    pub wood: i64,
    pub gold: i64,
    pub mana: i64,
}

impl Resources {
    pub const fn zero() -> Resources {
        Resources {
            food: 0,
            wood: 0,
            gold: 0,
            mana: 0,
        }
    }
    pub fn get(&self, slot: u8) -> i64 {
        match slot {
            0 => self.food,
            1 => self.wood,
            2 => self.gold,
            3 => self.mana,
            _ => 0,
        }
    }
    pub fn add(&mut self, slot: u8, amount: i64) {
        match slot {
            0 => self.food += amount,
            1 => self.wood += amount,
            2 => self.gold += amount,
            3 => self.mana += amount,
            _ => {}
        }
    }
    pub fn take(&mut self, slot: u8, amount: i64) -> bool {
        if self.get(slot) >= amount {
            self.add(slot, -amount);
            true
        } else {
            false
        }
    }
    pub fn hash_into<H: shared::WriteHash + ?Sized>(&self, h: &mut H) {
        h.write_i64(self.food);
        h.write_i64(self.wood);
        h.write_i64(self.gold);
        h.write_i64(self.mana);
    }
}

/// Building state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingStatus {
    Site,
    UnderConstruction,
    Done,
    Wreck,
}

impl BuildingStatus {
    pub fn code(self) -> u8 {
        match self {
            BuildingStatus::Site => 0,
            BuildingStatus::UnderConstruction => 1,
            BuildingStatus::Done => 2,
            BuildingStatus::Wreck => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    pub owner: u32,
    pub kind: u16,
    pub status: BuildingStatus,
    pub hp: u32,
    pub max_hp: u32,
    pub timer: u32,
}

/// One queued training job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainItem {
    pub kind: u16,
    pub ticks_left: u32,
}

/// Training queue component attached to a building entity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrainQueue(pub Vec<TrainItem>);

impl TrainQueue {
    pub fn hash_into(&self, h: &mut shared::Hasher) {
        h.write_u32(self.0.len() as u32);
        for it in &self.0 {
            h.write_u16(it.kind);
            h.write_u32(it.ticks_left);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingSpec {
    pub name: &'static str,
    pub w: u16,
    pub h: u16,
    pub hp: u32,
    pub build_time: u32,
    pub workers: u32,
    pub food: i64,
    pub wood: i64,
    pub gold: i64,
    pub mana: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSpec {
    pub name: &'static str,
    pub hp: u32,
    pub dmg: u32,
    pub speed_q: i32,
    pub range_q: i64,
    pub train_time: u32,
    pub food: i64,
    pub wood: i64,
    pub gold: i64,
    pub mana: i64,
    /// v2: minimum player age required to train (age gate, 0..=3).
    pub min_age: u8,
    /// v2: population cost charged against the pop cap (TownCenter/Farms supply).
    pub pop: u32,
}

/// Flat building table indexed by BuildingKind (data-only factions: balance lives here).
pub const BUILDING_TABLE: [BuildingSpec; 9] = [
    BuildingSpec {
        name: "TownCenter",
        w: 3,
        h: 3,
        hp: 1400,
        build_time: 3600,
        workers: 3,
        food: 0,
        wood: 150,
        gold: 0,
        mana: 0,
    },
    BuildingSpec {
        name: "Farm",
        w: 2,
        h: 2,
        hp: 300,
        build_time: 900,
        workers: 2,
        food: 0,
        wood: 50,
        gold: 0,
        mana: 0,
    },
    BuildingSpec {
        name: "House",
        w: 1,
        h: 1,
        hp: 150,
        build_time: 450,
        workers: 1,
        food: 0,
        wood: 25,
        gold: 0,
        mana: 0,
    },
    BuildingSpec {
        name: "Barracks",
        w: 2,
        h: 2,
        hp: 600,
        build_time: 1800,
        workers: 2,
        food: 0,
        wood: 100,
        gold: 50,
        mana: 0,
    },
    BuildingSpec {
        name: "Watchtower",
        w: 1,
        h: 1,
        hp: 400,
        build_time: 1200,
        workers: 1,
        food: 0,
        wood: 75,
        gold: 25,
        mana: 0,
    },
    BuildingSpec {
        name: "MageTower",
        w: 2,
        h: 2,
        hp: 500,
        build_time: 2400,
        workers: 2,
        food: 0,
        wood: 60,
        gold: 0,
        mana: 60,
    },
    BuildingSpec {
        name: "Wonder",
        w: 3,
        h: 3,
        hp: 1800,
        build_time: 27000,
        workers: 3,
        food: 0,
        wood: 600,
        gold: 600,
        mana: 120,
    },
    BuildingSpec {
        name: "Factory",
        w: 2,
        h: 2,
        hp: 900,
        build_time: 2100,
        workers: 2,
        food: 0,
        wood: 150,
        gold: 150,
        mana: 0,
    },
    BuildingSpec {
        name: "Wall",
        w: 1,
        h: 1,
        hp: 300,
        build_time: 150,
        workers: 1,
        food: 0,
        wood: 0,
        gold: 15,
        mana: 0,
    },
];
pub const BUILDING_KIND_COUNT: usize = BUILDING_TABLE.len();

/// Flat unit table indexed by UnitKind.
pub const UNIT_TABLE: [UnitSpec; 5] = [
    UnitSpec {
        name: "Serf",
        hp: 50,
        dmg: 1,
        speed_q: 1200,
        range_q: 65536,
        train_time: 300,
        food: 30,
        wood: 0,
        gold: 0,
        mana: 0,
        min_age: 0,
        pop: 1,
    },
    UnitSpec {
        name: "Militia",
        hp: 100,
        dmg: 10,
        speed_q: 1200,
        range_q: 65536,
        train_time: 450,
        food: 30,
        wood: 10,
        gold: 0,
        mana: 0,
        min_age: 1,
        pop: 1,
    },
    UnitSpec {
        name: "Archer",
        hp: 60,
        dmg: 8,
        speed_q: 1024,
        range_q: 196608,
        train_time: 600,
        food: 20,
        wood: 40,
        gold: 10,
        mana: 0,
        min_age: 1,
        pop: 1,
    },
    // Heavy melee, age 2 (Castle) gate; built at Barracks/Factory.
    UnitSpec {
        name: "Knight",
        hp: 200,
        dmg: 25,
        speed_q: 900,
        range_q: 65536,
        train_time: 900,
        food: 80,
        wood: 40,
        gold: 60,
        mana: 0,
        min_age: 2,
        pop: 2,
    },
    // Siege/ranged heavy, age 3 (Imperial) gate; built at Factory only.
    UnitSpec {
        name: "Siege",
        hp: 150,
        dmg: 35,
        speed_q: 700,
        range_q: 262144,
        train_time: 1200,
        food: 100,
        wood: 60,
        gold: 100,
        mana: 0,
        min_age: 3,
        pop: 2,
    },
];
pub const UNIT_KIND_COUNT: usize = UNIT_TABLE.len();

pub fn building_spec(kind: u16) -> &'static BuildingSpec {
    &BUILDING_TABLE[kind as usize]
}

pub fn unit_spec(kind: u16) -> &'static UnitSpec {
    &UNIT_TABLE[kind as usize]
}

/// Linear tile index of a corner (row-major, deterministic).
#[inline]
pub fn tile_index(x: i16, y: i16) -> usize {
    (y as usize) * (MAP_W as usize) + (x as usize)
}

/// Population supply: each done TownCenter gives POP_PER_TC, each done Farm POP_PER_FARM.
/// Computed by callers from the Building set (systems never see Sim.players).
#[inline]
pub fn pop_cap_for(tcs: u32, farms: u32) -> u32 {
    tcs * POP_PER_TC + farms * POP_PER_FARM
}

/// Integer score credit for a harvest deposit (amount/10, truncating; keeps score integral).
#[inline]
pub fn harvest_score_inc(amount: i32) -> i64 {
    (amount as i64) / 10
}

/// Training gate table: (building kind required, minimum player age) for UnitKind 0..=4.
/// Serf (age 0) is trainable anywhere at age 0? No: the design requires a training building.
/// Canonical: units 0/1/2 at Barracks (kind 3); Knight (3) needs Barracks + Castle age (2);
/// Siege (4) needs the Factory (kind 7) + Imperial age (3). Pop supply comes from TC/Farms.
#[inline]
pub fn train_gate(kind: u16) -> Option<(u16, u8)> {
    match kind {
        0..=2 => Some((3, 0)),
        3 => Some((3, 2)),
        4 => Some((7, 3)),
        _ => None,
    }
}

/// A natural resource node on the map (entity-attached).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceNode {
    pub slot: u8,
    pub total: i64,
    pub remaining: i64,
}

impl ResourceNode {
    pub fn hash_into(&self, h: &mut shared::Hasher) {
        h.write_u8(self.slot);
        h.write_i64(self.total);
        h.write_i64(self.remaining);
    }
}

/// Node origin (its tile), used by harvest targeting and rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodePos {
    pub x: i16,
    pub y: i16,
}

impl NodePos {
    pub fn hash_into(&self, h: &mut shared::Hasher) {
        h.write_i32(self.x as i32);
        h.write_i32(self.y as i32);
    }
}

/// Per-player global state (stockpiles, age ladder, win condition). v2 extends the hashable set:
/// age_target/age_timer (age system), civ (display-only id), score (20-min win), winner (game end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerState {
    pub stock: Resources,
    pub age: u8,
    pub wonder: u32,
    pub wonder_started: u64,
    pub age_target: u8,
    pub age_timer: u32,
    pub civ: u8,
    pub score: i64,
    pub winner: u32,
}

impl PlayerState {
    pub fn new() -> PlayerState {
        PlayerState {
            stock: Resources {
                food: 150,
                wood: 150,
                gold: 0,
                mana: 0,
            },
            age: 0,
            wonder: 0,
            wonder_started: 0,
            age_target: 0,
            age_timer: 0,
            civ: 0,
            score: 0,
            winner: 0,
        }
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Fog of war: revealed tiles as a packed bitset (part of the state hash).
#[derive(Clone)]
pub struct Visibility {
    pub owner: u32,
    pub words: Vec<u64>,
}

impl Visibility {
    pub fn new(owner: u32) -> Visibility {
        Visibility {
            owner,
            words: vec![0; (MAP_W as usize * MAP_H as usize).div_ceil(64)],
        }
    }
    #[inline]
    pub fn set(&mut self, x: i16, y: i16) {
        if x < 0 || y < 0 || (x as usize) >= MAP_W as usize || (y as usize) >= MAP_H as usize {
            return;
        }
        let i = tile_index(x, y);
        self.words[i / 64] |= 1u64 << (i % 64);
    }
    #[inline]
    pub fn get(&self, x: i16, y: i16) -> bool {
        if x < 0 || y < 0 || (x as usize) >= MAP_W as usize || (y as usize) >= MAP_H as usize {
            return false;
        }
        let i = tile_index(x, y);
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }
    /// Count of revealed tiles (hash-relevant).
    pub fn revealed(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }
    pub fn hash_into(&self, h: &mut shared::Hasher) {
        h.write_u32(self.owner);
        h.write_u32(self.words.len() as u32);
        for w in &self.words {
            h.write_u64(*w);
        }
        h.write_u32(self.revealed());
    }
}

/// Harvest loop sub-state (paired with an Order::Harvest walking state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarvestState {
    /// Walking to node `node` (path carried by the order).
    ToNode { node: u32, slot: u8 },
    /// Picking at `node`, counting down `ticks_left`.
    // note: node id kept so carrying knows which node lost stock
    /// Extracting at node: `ticks_left` until payload is full.
    Picking {
        node: u32,
        slot: u8,
        ticks_left: u32,
    },
    /// Hauling `amount` of `slot` back to the drop point (node: source to decrement).
    Carrying { node: u32, slot: u8, amount: i32 },
}

impl HarvestState {
    pub fn code(self) -> u8 {
        match self {
            HarvestState::ToNode { .. } => 0,
            HarvestState::Picking { .. } => 1,
            HarvestState::Carrying { .. } => 2,
        }
    }
    pub fn hash_into<H: shared::WriteHash + ?Sized>(self, h: &mut H) {
        h.write_u8(self.code());
        match self {
            HarvestState::ToNode { node, slot } => {
                h.write_u32(node);
                h.write_u8(slot);
            }
            HarvestState::Picking {
                node,
                slot,
                ticks_left,
            } => {
                h.write_u32(node);
                h.write_u8(slot);
                h.write_u32(ticks_left);
            }
            HarvestState::Carrying {
                node: _,
                slot,
                amount,
            } => {
                h.write_u8(slot);
                h.write_i32(amount);
            }
        }
    }
}

/// Rally point attached to a building entity (spawn/gather target hint). Hashed when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RallyPoint {
    pub tx: i16,
    pub ty: i16,
}
impl RallyPoint {
    pub fn hash_into<H: shared::WriteHash + ?Sized>(&self, h: &mut H) {
        h.write_i32(self.tx as i32);
        h.write_i32(self.ty as i32);
    }
}

/// Rally target attached to a unit (guide for the harvest return point). Hashed when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRally {
    pub tx: i16,
    pub ty: i16,
}
impl UnitRally {
    pub fn hash_into<H: shared::WriteHash + ?Sized>(&self, h: &mut H) {
        h.write_i32(self.tx as i32);
        h.write_i32(self.ty as i32);
    }
}

/// Construction job attached to a unit while it walks to and builds a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkJob {
    pub building: u32,
    pub slot: u8,
    pub node: u32,
    pub mode: WorkMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkMode {
    Build,
    Harvest,
}

impl WorkJob {
    pub fn hash_into(self, h: &mut shared::Hasher) {
        h.write_u32(self.building);
        h.write_u32(self.node);
        h.write_u8(self.slot);
        match self.mode {
            WorkMode::Build => h.write_u8(0),
            WorkMode::Harvest => h.write_u8(1),
        }
    }
}

/// Civilization ids (display metadata only; the sim never branches on these).
pub const CIV_ADEPTUS: u8 = 0;
pub const CIV_DOMINION: u8 = 1;

/// Slot -> civ-specific resource display name (slot 0 gold, 1 wood, 2 stone; slot 3 mana is
/// faction flavor only). Pure display table; NO sim branch reads this.
pub fn res_display(civ: u8, slot: u8) -> &'static str {
    let names = match civ {
        CIV_ADEPTUS => ["Faith", "Promethium", "Ore", "Essence"],
        _ => ["Minerals", "Vespene", "Terrazine", "Plasma"],
    };
    names.get(slot as usize).copied().unwrap_or("?")
}

/// Unit display name per civ. Unit ids are shared (Serf..Siege); flavor differs.
pub fn unit_display(civ: u8, kind: u16) -> &'static str {
    let names = match civ {
        CIV_ADEPTUS => ["Serf", "Guardsman", "Archer", "Knight", "Officer"],
        _ => ["SCV", "Marine", "Marine MKII", "Ghost", "Siege"],
    };
    names.get(kind as usize).copied().unwrap_or("?")
}

/// Building display name per civ.
pub fn building_display(civ: u8, kind: u16) -> &'static str {
    let names = match civ {
        CIV_ADEPTUS => [
            "Hive World",
            "Agri-World",
            "Hab-Block",
            "Barracks",
            "Watchtower",
            "Machine Shrine",
            "Aquila",
            "Manufactory",
            "Bastion",
        ],
        _ => [
            "Command Center",
            "Supply Depot",
            "Pylon",
            "Barracks",
            "Missile Turret",
            "Ghost Lab",
            "Crash Site",
            "Factory",
            "Wall",
        ],
    };
    names.get(kind as usize).copied().unwrap_or("?")
}

/// Localized UI strings (en/it/de/fr/es). 5x7 bitmap font + transliteration is done by the
/// renderer; these are plain UTF-8 (renderer transliterates glyphs outside ASCII).
pub const UI_STRINGS: &[(&str, &[&str; 5])] = &[
    ("age", &["Age", "Epoca", "Zeitalter", "Ère", "Edad"]),
    (
        "score",
        &["Score", "Punteggio", "Punkte", "Score", "Puntuación"],
    ),
    ("pop", &["Pop", "Pop", "Bev", "Pays", "Pob"]),
    (
        "victory",
        &["Victory", "Vittoria", "Sieg", "Victoire", "Victoria"],
    ),
    (
        "defeat",
        &["Defeat", "Sconfitta", "Niederlage", "Défaite", "Derrota"],
    ),
    (
        "match_over",
        &[
            "Match over",
            "Partita finita",
            "Spiel beendet",
            "Fin de partie",
            "Partida terminada",
        ],
    ),
    (
        "dark",
        &[
            "Dark Age",
            "Età Buia",
            "Dunkles Zeitalter",
            "Âge sombre",
            "Edad Oscura",
        ],
    ),
    (
        "feudal",
        &["Feudal", "Feudale", "Feudal", "Féodal", "Feudal"],
    ),
    (
        "castle",
        &["Castle", "Castello", "Burg", "Château", "Castillo"],
    ),
    (
        "imperial",
        &[
            "Imperial",
            "Imperiale",
            "Kaiserlich",
            "Impérial",
            "Imperial",
        ],
    ),
    ("gold", &["Gold", "Oro", "Gold", "Or", "Oro"]),
    ("wood", &["Wood", "Legno", "Holz", "Bois", "Madera"]),
    ("stone", &["Stone", "Pietra", "Stein", "Pierre", "Piedra"]),
];
pub const LOCALES: [&str; 5] = ["en", "it", "de", "fr", "es"];
pub fn ui(locale: &str, key: &str) -> &'static str {
    let idx = LOCALES.iter().position(|l| *l == locale).unwrap_or(0);
    UI_STRINGS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v[idx])
        .unwrap_or("?")
}
pub fn age_display(age: u8) -> &'static str {
    match age.min(3) {
        0 => "Dark",
        1 => "Feudal",
        2 => "Castle",
        _ => "Imperial",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources_get_add_take() {
        let mut r = Resources::zero();
        r.add(1, 60);
        assert_eq!(r.get(1), 60);
        assert!(!r.take(1, 61));
        assert_eq!(r.get(1), 60);
        assert!(r.take(1, 60));
        assert_eq!(r.get(1), 0);
        assert_eq!(r.get(9), 0);
    }

    #[test]
    fn visibility_bits_roundtrip() {
        let mut v = Visibility::new(0);
        v.set(5, 5);
        assert!(v.get(5, 5));
        assert!(!v.get(5, 6));
        assert_eq!(v.revealed(), 1);
        v.set(100, 100);
        assert_eq!(v.revealed(), 2);
        v.set(-1, 0);
        v.set(999, 999);
        assert_eq!(v.revealed(), 2);
    }

    #[test]
    fn tables_are_consistent_and_state_hashable() {
        for (i, s) in BUILDING_TABLE.iter().enumerate() {
            assert_eq!(building_spec(i as u16).name, s.name);
            assert!(s.workers >= 1);
        }
        let mut h1 = shared::Hasher::new();
        PlayerState::new().stock.hash_into(&mut h1);
        let mut h2 = shared::Hasher::new();
        PlayerState::default().stock.hash_into(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
        let q = TrainQueue(vec![TrainItem {
            kind: 1,
            ticks_left: 7,
        }]);
        let mut h3 = shared::Hasher::new();
        let mut h4 = shared::Hasher::new();
        q.hash_into(&mut h3);
        q.hash_into(&mut h4);
        assert_eq!(h3.finish(), h4.finish());
    }
    #[test]
    fn v2_tables_are_valid() {
        // Wall: cheap single-tile stone line defense, fast to raise.
        let w = building_spec(WALL_KIND);
        assert_eq!(w.w, 1);
        assert_eq!(w.h, 1);
        assert_eq!(w.gold, 15, "Wall must be stone(slot2)->gold-slot cost");
        assert_eq!(w.wood, 0);
        assert!(w.build_time <= 200);
        assert_eq!(building_spec(FACTORY_KIND).w, 2);
        // Age costs strictly monotone in age; slot order fixed.
        for a in 1..=MAX_AGE as usize {
            let c = &AGE_COST[a];
            let prev = &AGE_COST[a - 1];
            assert!(
                c.iter().sum::<i64>() > prev.iter().sum::<i64>(),
                "age cost must rise"
            );
        }
        assert_eq!(AGE_UP_TICKS[1], 600);
        // Pop supply: 1 TC + 1 Farm == POP_PER_TC + POP_PER_FARM.
        assert_eq!(pop_cap_for(1, 1), POP_PER_TC + POP_PER_FARM);
        // Unit gates: Knight age 2, Siege age 3 and Siege not trainable at age 2.
        assert_eq!(unit_spec(3).min_age, 2);
        assert_eq!(unit_spec(4).min_age, 3);
        // Score increments are integral.
        assert_eq!(harvest_score_inc(25), 2);
        assert_eq!(harvest_score_inc(9), 0);
        // Display layer: both civs, slot 0 name differs (wire format unaffected).
        assert_eq!(res_display(0, 0), "Faith");
        assert_eq!(res_display(1, 0), "Minerals");
        assert_eq!(unit_display(0, 3), "Knight");
        assert_eq!(building_display(1, 8), "Wall");
        assert_eq!(ui("it", "age"), "Epoca");
        assert_eq!(age_display(3), "Imperial");
    }
}
