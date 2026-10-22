//! Match configuration — the single source of truth for a GUI-launched match.
//!
//! Deterministic: MatchConfig + seed fully determine the command schedule and thus the sim
//! stream. No randomness outside the seeded SplitMix64 streams. The config itself never enters
//! the sim state (it is view/host-side); `schedule` is what gets fed to `Sim::step`.

use render::demo::demo_commands;
use shared::{Command, TickCommands};

/// One player slot in a custom match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Human, controls the slot live (commands come from the queue at runtime).
    Human,
    /// AI, commands come from the deterministic demo policy for that player index.
    Ai,
    /// No control: units exist but receive no commands ever (sandbox / solo).
    Off,
}

/// A match config: seed + preset + slots + locale + sandbox flag + tick limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchConfig {
    pub seed: u64,
    /// "arena" | "black_forest" | "" = legacy stream map.
    pub preset: String,
    pub players: [SlotKind; 2],
    /// "en" | "it" (falls back to en in economy::ui).
    pub locale: String,
    /// Sandbox: score cap / winner banner are informational; sim never auto-stops.
    pub sandbox: bool,
    /// Hard tick limit for the session view (default 36000 = MATCH_TICKS).
    pub ticks: u64,
    /// Player-1 cosmetic faction 0=Adeptus 1=Dominion. View-only (never enters the sim;
    /// changes the render palette only). Kept here so the config fully determines the session.
    pub civ: u8,
}

impl Default for MatchConfig {
    fn default() -> Self {
        MatchConfig {
            seed: 12345,
            preset: String::new(),
            players: [SlotKind::Human, SlotKind::Off],
            locale: "en".to_string(),
            sandbox: true,
            ticks: 36000,
            civ: 0,
        }
    }
}

impl MatchConfig {
    pub fn valid(&self) -> bool {
        self.ticks >= 1
            && matches!(self.players[0], SlotKind::Human | SlotKind::Ai)
            && matches!(self.locale.as_str(), "en" | "it")
    }

    /// Deterministic AI command schedule for ticks [0, self.ticks): the demo policy filtered so
    /// only commands belonging to AI-controlled slots survive. Human/Off slots get zero scripted
    /// commands (humans act live; Off never acts).
    pub fn schedule(&self) -> Vec<TickCommands> {
        let full = demo_commands(self.seed, self.ticks);
        full.into_iter()
            .map(|mut b| {
                b.cmds
                    .retain(|(player, _c)| matches!(self.players[*player as usize], SlotKind::Ai));
                // canonical order preserved (policy emits in canonical order already)
                b
            })
            .collect()
    }

    /// True iff the config must emit zero commands for the given player over all ticks.
    pub fn player_passive(&self, player: u32) -> bool {
        matches!(
            self.players.get(player as usize),
            None | Some(SlotKind::Off) | Some(SlotKind::Human)
        )
    }
}

/// Merge scheduled (AI) and live (human) commands for one tick step, in canonical lockstep
/// order (Sim::step sorts internally, but we keep the pre-sorted form to keep step input
/// canonical and replayable).
pub fn merge_cmds(scheduled: &[(u32, Command)], live: &[(u32, Command)]) -> Vec<(u32, Command)> {
    let mut out = scheduled.to_vec();
    out.extend_from_slice(live);
    out.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
    out
}
