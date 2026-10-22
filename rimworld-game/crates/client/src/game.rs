//! View-level session: owns a ColonySim, the live command queue, build modes and alerts.
//! The SIM is untouched — this layer only issues canonical shared::Command values via step_tick().

use shared::{Command, TickCommands};
use sim::ColonySim;

pub const LOCAL_PLAYER: u32 = 0;

/// Colony preset — view-side: chooses initial spawn/setup only (applied via setup, deterministic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Homestead,
    Outpost,
    Blank,
}

impl Preset {
    pub fn label(p: Preset) -> &'static str {
        match p {
            Preset::Homestead => "homestead",
            Preset::Outpost => "outpost",
            Preset::Blank => "blank",
        }
    }
    pub fn all() -> [Preset; 3] {
        [Preset::Homestead, Preset::Outpost, Preset::Blank]
    }
    pub fn index(p: Preset) -> u8 {
        match p {
            Preset::Homestead => 0,
            Preset::Outpost => 1,
            Preset::Blank => 2,
        }
    }
    pub fn from_index(i: u8) -> Preset {
        match i {
            1 => Preset::Outpost,
            2 => Preset::Blank,
            _ => Preset::Homestead,
        }
    }
    pub fn setup(self, sim: &mut ColonySim) {
        match self {
            Preset::Homestead => {
                sim.spawn_pawn(LOCAL_PLAYER, 12, 12);
                sim.spawn_pawn(LOCAL_PLAYER, 12, 13);
                sim.spawn_pawn(LOCAL_PLAYER, 13, 12);
                sim.add_site(0, 10, 10);
                sim.add_site(1, 16, 12);
            }
            Preset::Outpost => {
                sim.spawn_pawn(LOCAL_PLAYER, 14, 14);
                sim.spawn_pawn(LOCAL_PLAYER, 15, 14);
                sim.spawn_pawn(LOCAL_PLAYER, 14, 15);
                sim.spawn_pawn(LOCAL_PLAYER, 15, 15);
                sim.add_site(1, 12, 12);
                sim.add_site(1, 12, 13);
            }
            Preset::Blank => {}
        }
    }
}

/// Build mode selected in the toolbar. Maps onto Command::Build kinds (sim: 0=storage, 1=bed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildKind {
    Shelf,
    Bed,
    Farm,     // shelf placement = food storage (view-side naming)
    WorkZone, // bed placement = rest zone (view-side naming)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Starve,
    Exhaust,
    Disease,
    Raid,
}

pub fn alert_label(a: AlertKind) -> &'static crate::i18n::L {
    match a {
        AlertKind::Starve => &crate::i18n::ALERT_STARVE,
        AlertKind::Exhaust => &crate::i18n::ALERT_EXHAUST,
        AlertKind::Disease => &crate::i18n::ALERT_DISEASE,
        AlertKind::Raid => &crate::i18n::ALERT_RAID,
    }
}

/// A live session wrapping the deterministic ColonySim + live input queue.
pub struct Session {
    pub sim: ColonySim,
    pub seed: u64,
    pub preset: Preset,
    pub sandbox: bool,
    pub difficulty: u8,
    /// Live player commands, drained once per tick in canonical order.
    pub live: Vec<(u32, Command)>,
    /// All recorded batches (index = tick) for save/replay: canonical snapshots per tick.
    pub history: Vec<TickCommands>,
    pub paused: bool,
    pub speed: u8,
    pub last_err: Option<String>,
}

impl Session {
    pub fn new(seed: u64, preset: Preset, sandbox: bool, difficulty: u8) -> Session {
        let mut sim = ColonySim::new(seed);
        preset.setup(&mut sim);
        Session {
            sim,
            seed,
            preset,
            sandbox,
            difficulty,
            live: Vec::new(),
            history: Vec::new(),
            paused: false,
            speed: 1,
            last_err: None,
        }
    }

    /// Rebuild from a decoded save (used by tests): canonical batches + live queue = 0.
    pub fn from_replay(
        sim: ColonySim,
        seed: u64,
        preset: Preset,
        sandbox: bool,
        difficulty: u8,
        batches: &[TickCommands],
    ) -> Session {
        let mut s = Session {
            sim,
            seed,
            preset,
            sandbox,
            difficulty,
            live: Vec::new(),
            history: Vec::new(),
            paused: false,
            speed: 1,
            last_err: None,
        };
        for b in batches {
            while s.history.len() <= b.tick as usize {
                s.history.push(TickCommands {
                    tick: s.history.len() as u64,
                    cmds: Vec::new(),
                });
            }
            s.history[b.tick as usize] = b.clone();
        }
        s
    }

    pub fn issue(&mut self, cmd: Command) {
        self.live.push((LOCAL_PLAYER, cmd));
    }

    /// Advance exactly one tick; live commands are canonicalized (step does so internally too).
    pub fn step_tick(&mut self) {
        let tick = self.sim.world.tick;
        while self.history.len() <= tick as usize {
            self.history.push(TickCommands {
                tick: self.history.len() as u64,
                cmds: Vec::new(),
            });
        }
        let mut cmds: Vec<(u32, Command)> = std::mem::take(&mut self.live);
        cmds.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
        self.history[tick as usize].cmds.extend_from_slice(&cmds);
        if let Err(e) = self.sim.step(&cmds) {
            self.last_err = Some(format!("{:?}", e));
        }
    }

    /// Issue build command from toolbar mode.
    pub fn issue_build(&mut self, kind: BuildKind, x: i16, y: i16) {
        let k: u8 = match kind {
            BuildKind::Shelf | BuildKind::Farm => 0,
            BuildKind::Bed | BuildKind::WorkZone => 1,
        };
        self.issue(Command::Build { tile: x, y, kind: k });
    }

    /// First pawn with an idle mode and no job claim (for the "Assign" quick action).
    pub fn pick_unassigned_pawn(&self) -> Option<u32> {
        let s = &self.sim;
        let mut best: Option<u32> = None;
        for (e, p) in s.world.storages.iter::<sim::colony::Pawn>() {
            if p.owner != LOCAL_PLAYER {
                continue;
            }
            let mode_busy = !matches!(p.mode, sim::colony::Mode::Idle);
            if mode_busy {
                continue;
            }
            let claimed = s
                .world
                .storages
                .iter::<sim::colony::Site>()
                .any(|(_, st)| st.occupied_by == Some(e.0));
            let on_job = s.world.storages.iter::<sim::colony::Job>().any(|(_, j)| j.assigned == Some(e.0) && !j.done);
            if !claimed && !on_job {
                best = Some(e.0);
                break;
            }
        }
        best
    }

    /// View alerts derived from live state (needs thresholds; campaign-only flavor events).
    pub fn alerts(&self) -> Vec<AlertKind> {
        let mut out = Vec::new();
        let mut starving = false;
        let mut exhausted = false;
        let mut idle_needs = false;
        for (e, _p) in self.sim.world.storages.iter::<sim::colony::Pawn>() {
            let Some(n) = self.sim.world.storages.get::<sim::colony::Needs>(e) else {
                continue;
            };
            if n.hunger < 8192 {
                starving = true;
            }
            if n.energy < 8192 {
                exhausted = true;
            }
            if n.hunger < 16384 || n.energy < 8192 {
                idle_needs = true;
            }
        }
        if starving {
            out.push(AlertKind::Starve);
        }
        if exhausted {
            out.push(AlertKind::Exhaust);
        }
        if !self.sandbox {
            let t = self.sim.world.tick;
            let diff = self.difficulty.max(1) as u64;
            if t > 600 && (t % (2400 / diff)) < 30 {
                out.push(AlertKind::Disease);
            }
            if t > 1200 && (t % (4800 / diff)) < 30 {
                out.push(AlertKind::Raid);
            }
            let _ = idle_needs;
        }
        out
    }

    /// Commands for the save file: canonical per-tick batches up to current tick.
    pub fn live_batches_snapshot(&self) -> Vec<crate::save::WireBatch> {
        self.history.iter().map(crate::save::batch_to_wire).collect()
    }



    /// Build a live session from a decoded save: replay the command log against a fresh
    /// deterministic-preset sim, then continue from that point. History is restored too, so
    /// saving again after load re-produces the identical bytes.
    pub fn from_saved(sg: crate::save::SaveGame) -> Session {
        let preset = Preset::from_index(sg.preset);
        let mut sim = ColonySim::new(sg.seed);
        preset.setup(&mut sim);
        let mut history: Vec<TickCommands> = sg
            .batches
            .iter()
            .map(crate::save::batch_from_wire)
            .collect();
        let max_tick = sg.ticks.max(sg.batches.iter().map(|b| b.tick + 1).max().unwrap_or(0));
        while history.len() < max_tick as usize {
            history.push(TickCommands {
                tick: history.len() as u64,
                cmds: Vec::new(),
            });
        }
        let mut s = Session {
            sim,
            seed: sg.seed,
            preset,
            sandbox: sg.sandbox,
            difficulty: sg.difficulty,
            live: Vec::new(),
            history,
            paused: false,
            speed: 1,
            last_err: None,
        };
        while s.sim.world.tick < max_tick {
            let i = s.sim.world.tick as usize;
            let cmds = s.history[i].cmds.clone();
            if s.sim.step(&cmds).is_err() {
                break;
            }
        }
        s
    }

    /// View-side job assignment: mark the job as assigned via the canonical command stream.
    /// The scaffold sim assigns jobs automatically; this command keeps the pawn occupied so the
    /// claim (job assigned) is stable for display. Implemented as Wait (drop current) + Move to
    /// the job's site, which makes job_system bind the pawn to the closest open job deterministically.
    pub fn issue_assign_job(&mut self, job: u32, pawn: u32) {
        let site = self
            .sim
            .world
            .storages
            .get::<sim::colony::Job>(engine_ecs::Entity(job))
            .map(|j| j.site);
        self.issue(Command::Wait { pawn });
        if let Some(t) = site {
            self.issue(Command::Move { pawn, x: t.x, y: t.y });
        }
    }
}
