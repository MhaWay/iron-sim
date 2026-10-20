//! Lockstep relay: canonicalizes command batches and checks peer agreement.
//! The simulation lives in the sim crate — the relay only orders and forwards.

use shared::TickCommands;

#[derive(Default)]
pub struct Relay {
    pub rounds: Vec<TickCommands>,
}

impl Relay {
    pub fn ingest(&mut self, mut batch: TickCommands) {
        batch.sort();
        self.rounds.push(batch);
    }

    /// True if every peer reported the same hash for `tick` and a batch exists at that tick.
    pub fn peers_agreed_at(&self, tick: u64, hash_by_peer: &[(u32, u64)]) -> bool {
        if hash_by_peer.len() < 2 {
            return false;
        }
        let first = hash_by_peer[0].1;
        hash_by_peer.iter().all(|&(_, h)| h == first) && self.rounds.iter().any(|b| b.tick == tick)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use shared::Command;

    fn batch(t: u64) -> TickCommands {
        TickCommands {
            tick: t,
            cmds: vec![
                (
                    0,
                    Command::Move {
                        pawn: 1,
                        x: 5,
                        y: 5,
                    },
                ),
                (0, Command::Wait { pawn: 2 }),
            ],
        }
    }

    #[test]
    fn ingest_sorts_and_stores() {
        let mut r = Relay::default();
        r.ingest(batch(0));
        assert_eq!(r.rounds.len(), 1);
        assert_eq!(r.rounds[0].cmds[0].1.sort_key(), (1, 0u8, 5, 5));
    }

    #[test]
    fn peers_agree_requires_two_and_existing_batch() {
        let mut r = Relay::default();
        assert!(!r.peers_agreed_at(0, &[(0, 42)]));
        r.ingest(batch(0));
        assert!(!r.peers_agreed_at(0, &[(0, 42), (1, 7)]));
        assert!(r.peers_agreed_at(0, &[(0, 42), (1, 42)]));
        assert!(!r.peers_agreed_at(9, &[(0, 42), (1, 42)]));
    }
}
