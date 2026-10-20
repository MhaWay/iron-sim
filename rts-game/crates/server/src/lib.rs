//! Authoritative lockstep relay (logic, not transport).
//!
//! Responsibilities modeled here: collect raw (player -> Command) per tick, sort them canonically,
//! emit a TickCommands batch with its hash, and report whether peer hashes agreed. A real socket
//! transport is out of scope; this is exercised by unit tests and a headless demo bin.

use shared::{Command, TickCommands};

/// Per-peer hash report for a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerHash {
    pub peer: u32,
    pub hash: u64,
}

/// One round of relay bookkeeping.
#[derive(Default)]
pub struct Relay {
    pub emitted: Vec<TickCommands>,
    /// (batch_hash, peer_reports) per emitted round.
    pub rounds: Vec<(u64, Vec<PeerHash>)>,
}

impl Relay {
    pub fn new() -> Self {
        Relay::default()
    }

    /// Sort raw commands, emit batch, record peer reports, return batch hash.
    pub fn emit_tick(&mut self, tick: u64, raw: Vec<(u32, Command)>, peers: &[PeerHash]) -> u64 {
        let mut batch = TickCommands { tick, cmds: raw };
        batch.sort();
        let hash = batch.hash();
        self.emitted.push(batch);
        self.rounds.push((hash, peers.to_vec()));
        hash
    }

    /// True if all peers on the latest round reported the same hash as each other.
    pub fn peers_agreed_latest(&self) -> Option<bool> {
        let (_, list) = self.rounds.last()?;
        if list.is_empty() {
            return None;
        }
        let first = list[0].hash;
        Some(list.iter().all(|p| p.hash == first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emit_sorts_canonically() {
        let mut relay = Relay::new();
        relay.emit_tick(
            0,
            vec![
                (1, Command::Stop { unit: 5 }),
                (
                    0,
                    Command::Move {
                        unit: 1,
                        x: 1,
                        y: 1,
                    },
                ),
                (0, Command::Attack { unit: 1, target: 3 }),
            ],
            &[],
        );
        let b = &relay.emitted[0];
        assert_eq!(b.cmds[0].0, 0);
        assert!(matches!(b.cmds[0].1, Command::Move { .. })); // Move before Attack for unit 1
        assert_eq!(b.cmds[1].0, 0);
        assert!(matches!(b.cmds[1].1, Command::Attack { .. }));
        assert_eq!(b.cmds[2].0, 1);
    }
    #[test]
    fn tick_affects_hash() {
        let mut relay = Relay::new();
        let cmds = vec![(0, Command::Stop { unit: 3 })];
        let h1 = relay.emit_tick(0, cmds.clone(), &[PeerHash { peer: 0, hash: 7 }]);
        let h2 = relay.emit_tick(1, cmds, &[PeerHash { peer: 0, hash: 7 }]);
        assert_ne!(h1, h2);
    }
    #[test]
    fn peers_agree_check() {
        let mut relay = Relay::new();
        relay.emit_tick(
            0,
            vec![(0, Command::Stop { unit: 3 })],
            &[PeerHash { peer: 0, hash: 7 }, PeerHash { peer: 1, hash: 7 }],
        );
        assert_eq!(relay.peers_agreed_latest(), Some(true));
        relay.emit_tick(
            1,
            vec![(0, Command::Stop { unit: 4 })],
            &[PeerHash { peer: 0, hash: 7 }, PeerHash { peer: 1, hash: 8 }],
        );
        assert_eq!(relay.peers_agreed_latest(), Some(false));
    }
}
