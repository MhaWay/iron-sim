//! Deterministic save format: version + colony config + per-tick command log (the replay seed).
//! No raw world bytes — loading = replaying the exact canonical command stream, so the live
//! state_hash and the replay digest stay equal (save hash == replay digest, by construction).
//!
//! Wire format: self-contained structs (no serde deps on shared types), stable bincode layout.

use shared::{Command, TickCommands};

pub const SAVE_VERSION: u32 = 1;

/// Wire-level command (mirrors shared::Command 1:1, encodable via serde).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WireCommand {
    Move { pawn: u32, x: i16, y: i16 },
    Wait { pawn: u32 },
    Build { tile: i16, y: i16, kind: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireBatch {
    pub tick: u64,
    pub cmds: Vec<(u32, WireCommand)>,
}

pub fn to_wire(c: &Command) -> WireCommand {
    match *c {
        Command::Move { pawn, x, y } => WireCommand::Move { pawn, x, y },
        Command::Wait { pawn } => WireCommand::Wait { pawn },
        Command::Build { tile, y, kind } => WireCommand::Build { tile, y, kind },
    }
}

pub fn from_wire(c: &WireCommand) -> Command {
    match *c {
        WireCommand::Move { pawn, x, y } => Command::Move { pawn, x, y },
        WireCommand::Wait { pawn } => Command::Wait { pawn },
        WireCommand::Build { tile, y, kind } => Command::Build { tile, y, kind },
    }
}

pub fn batch_to_wire(b: &TickCommands) -> WireBatch {
    WireBatch {
        tick: b.tick,
        cmds: b.cmds.iter().map(|(p, c)| (*p, to_wire(c))).collect(),
    }
}

pub fn batch_from_wire(b: &WireBatch) -> TickCommands {
    TickCommands {
        tick: b.tick,
        cmds: b.cmds.iter().map(|(p, c)| (*p, from_wire(c))).collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub seed: u64,
    pub sandbox: bool,
    pub difficulty: u8,
    /// Preset index: 0=homestead 1=outpost 2=blank (setup is deterministic, re-run on load).
    pub preset: u8,
    /// Ticks already simulated when saved.
    pub ticks: u64,
    /// Canonical command log: batches[t] = commands issued at tick t (dense, sorted by tick).
    pub batches: Vec<WireBatch>,
}

pub fn encode(s: &SaveGame) -> Vec<u8> {
    bincode::serialize(s).expect("save encoding")
}

pub fn decode(b: &[u8]) -> Result<SaveGame, String> {
    bincode::deserialize(b).map_err(|e| format!("{:?}", e))
}

/// Rebuild the sim state from the decoded save: deterministic preset setup + canonical replay of
/// the command log. The resulting sim.hash() is the replay digest by construction — identical
/// across any run that produced this save (save/load round-trip determinism).
pub fn replay(s: &SaveGame) -> sim::ColonySim {
    let mut sim = sim::ColonySim::new(s.seed);
    crate::game::Preset::from_index(s.preset).setup(&mut sim);
    let max_tick = s
        .ticks
        .max(s.batches.iter().map(|b| b.tick + 1).max().unwrap_or(0));
    let mut idx = 0usize;
    while sim.world.tick < max_tick {
        let cmds: Vec<(u32, Command)> = if idx < s.batches.len() && s.batches[idx].tick == sim.world.tick {
            let c = s.batches[idx]
                .cmds
                .iter()
                .map(|(p, c)| (*p, from_wire(c)))
                .collect();
            idx += 1;
            c
        } else {
            Vec::new()
        };
        if sim.step(&cmds).is_err() {
            break;
        }
    }
    sim
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::run_colony;

    #[test]
    fn save_roundtrip_equal() {
        let s = SaveGame {
            version: SAVE_VERSION,
            seed: 42,
            sandbox: true,
            difficulty: 1,
            preset: 0,
            ticks: 9,
            batches: vec![WireBatch {
                tick: 4,
                cmds: vec![(0, WireCommand::Move { pawn: 1, x: 2, y: 3 })],
            }],
        };
        let b = encode(&s);
        let r = decode(&b).unwrap();
        assert_eq!(s, r);
    }

    #[test]
    fn replay_matches_two_run_headless() {
        let seed = 777u64;
        let mut batches: Vec<WireBatch> = (0..160u64)
            .map(|t| WireBatch { tick: t, cmds: vec![] })
            .collect();
        batches[5].cmds.push((0, WireCommand::Build { tile: 10, y: 11, kind: 0 }));
        batches[30].cmds.push((0, WireCommand::Move { pawn: 0, x: 10, y: 10 }));
        let h = run_colony(seed, 160, &denses(), |s| crate::game::Preset::Homestead.setup(s));
        let save = SaveGame {
            version: SAVE_VERSION,
            seed,
            sandbox: true,
            difficulty: 0,
            preset: 0,
            ticks: 160,
            batches: batches.clone(),
        };
        let loaded = replay(&save);
        assert_eq!(loaded.hash(), h);
    }

    fn denses() -> Vec<TickCommands> {
        (0..160u64)
            .map(|t| {
                let cmds = if t == 5 {
                    vec![(0, Command::Build { tile: 10, y: 11, kind: 0 })]
                } else if t == 30 {
                    vec![(0, Command::Move { pawn: 0, x: 10, y: 10 })]
                } else {
                    vec![]
                };
                TickCommands { tick: t, cmds }
            })
            .collect()
    }
}
