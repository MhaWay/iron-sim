//! eframe app: menu, new-colony screen, live playing view.
//!
//! The sim advances one deterministic tick per frame step (speed = extra ticks per frame).
//! egui floats are view-only — never read back into sim state.

use crate::game::{alert_label, BuildKind, Preset, Session};
use crate::i18n::*;
use crate::save::{self, SaveGame};
use crate::state::{Config, Screen, UiState};
use crate::save_io;
use engine_ecs::Entity;
use eframe::egui::{self, Button, Color32, ComboBox, RichText};
use shared::Command;
use sim::colony::{Job, Mode, Needs, Pawn, Pos, Site};

const MAP: u16 = 64;
const CELL: f32 = 12.0;
const VIEW: f32 = MAP as f32 * CELL;

pub struct UiApp {
    pub state: UiState,
    pub session: Option<Session>,
    pub pending_build: Option<BuildKind>,
    pub selected_pawn: Option<u32>,
    pub save_feedback: Option<(String, bool)>,
}

impl Default for UiApp {
    fn default() -> Self {
        Self::new()
    }
}

impl UiApp {
    pub fn new() -> UiApp {
        UiApp {
            state: UiState::default(),
            session: None,
            pending_build: None,
            selected_pawn: None,
            save_feedback: None,
        }
    }

    pub fn start_session(cfg: &Config) -> Session {
        Session::new(cfg.seed, cfg.preset, cfg.sandbox, cfg.difficulty)
    }

    fn start_current(&mut self) {
        if self.state.start() {
            self.session = Some(Self::start_session(&self.state.cfg));
            self.selected_pawn = None;
            self.pending_build = None;
        }
    }

    fn frame_step(&mut self) {
        if self.state.paused {
            return;
        }
        if let Some(s) = &mut self.session {
            for _ in 0..self.state.speed.max(1) {
                s.step_tick();
            }
        }
    }

    fn find_pawn_at(&self, tx: i16, ty: i16) -> Option<u32> {
        let s = self.session.as_ref()?;
        s.sim
            .world
            .storages
            .iter::<Pos>()
            .find(|(e, p)| {
                let t = p.tile();
                t.x == tx && t.y == ty && s.sim.world.storages.get::<Pawn>(*e).is_some()
            })
            .map(|(e, _)| e.0)
    }

    fn needs_for(&self, id: u32) -> Option<Needs> {
        self.session
            .as_ref()?
            .sim
            .world
            .storages
            .get::<Needs>(Entity(id))
            .copied()
    }
}

impl eframe::App for UiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.state.screen {
            Screen::Menu => self.menu_ui(ui),
            Screen::NewColony => self.new_ui(ui),
            Screen::Playing => self.playing_ui(ui),
        }
    }
}

impl UiApp {
    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let locale = self.state.cfg.locale.clone();
        let ctx = ui.ctx().clone();
        let mut msg: Option<(String, bool)> = None;
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(MENU_TITLE.t(&locale)).size(48.0).strong());
            ui.add_space(40.0);
            if ui
                .add(
                    Button::new(RichText::new(START_NEW.t(&locale)).size(24.0))
                        .min_size(egui::vec2(400.0, 64.0)),
                )
                .clicked()
            {
                self.state.open_new();
            }
            ui.add_space(12.0);
            if ui
                .add(Button::new(RichText::new(CONTINUE.t(&locale)).size(20.0)).min_size(egui::vec2(400.0, 48.0)))
                .clicked()
            {
                match load_save() {
                    Some(s) => {
                        self.session = Some(s);
                        self.state.screen = Screen::Playing;
                        msg = Some((LOAD_OK.t(&locale).to_string(), true));
                    }
                    None => msg = Some((NO_SAVE.t(&locale).to_string(), false)),
                }
            }
            ui.add_space(12.0);
            if ui
                .add(Button::new(RichText::new(EXIT.t(&locale)).size(20.0)).min_size(egui::vec2(400.0, 48.0)))
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(LANGUAGE.t(&locale)).strong());
                ComboBox::from_id_salt("menu_locale")
                    .selected_text(locale.clone())
                    .show_ui(ui, |ui| {
                        for (code, name) in LOCALE_NAMES {
                            ui.selectable_value(&mut self.state.cfg.locale, code.to_string(), name);
                        }
                    });
            });
            ui.add_space(24.0);
            if let Some((m, ok)) = &self.save_feedback {
                let c = if *ok { Color32::GOLD } else { Color32::YELLOW };
                ui.colored_label(c, m);
            }
            ui.label(RichText::new("v0.1-client · deterministic core · eframe 0.37").weak());
        });
        if let Some(m) = msg {
            self.save_feedback = Some(m);
        }
    }

    fn new_ui(&mut self, ui: &mut egui::Ui) {
        let locale = self.state.cfg.locale.clone();
        let mut start_req = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(RichText::new(NEW_COLONY.t(&locale)).size(32.0).strong());
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(PRESET.t(&locale)).strong());
                    ComboBox::from_id_salt("preset")
                        .selected_text(Preset::label(self.state.cfg.preset))
                        .show_ui(ui, |ui| {
                            for p in Preset::all() {
                                ui.selectable_value(&mut self.state.cfg.preset, p, Preset::label(p));
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(DIFFICULTY.t(&locale)).strong());
                    for i in 0..3u8 {
                        let name = ["easy", "normal", "hard"][i as usize];
                        if ui.selectable_value(&mut self.state.cfg.difficulty, i, name).clicked() {
                            break;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(MODE_L.t(&locale)).strong());
                    ui.selectable_value(&mut self.state.cfg.sandbox, true, SANDBOX_MODE.t(&locale));
                    ui.selectable_value(&mut self.state.cfg.sandbox, false, CAMPAIGN_MODE.t(&locale));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(SEED.t(&locale)).strong());
                    let mut s = self.state.cfg.seed.to_string();
                    if ui.text_edit_singleline(&mut s).changed() {
                        if let Ok(v) = s.parse::<u64>() {
                            self.state.cfg.seed = v;
                        }
                    }
                    if ui.button(RANDOM.t(&locale)).clicked() {
                        self.state.cfg.seed = self
                            .state
                            .cfg
                            .seed
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                    }
                });
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button(BACK.t(&locale)).clicked() {
                    self.state.back_to_menu();
                }
                ui.add_space(24.0);
                let b = ui.add(
                    Button::new(RichText::new(PLAY_BTN.t(&locale)).size(24.0).color(Color32::GOLD))
                        .min_size(egui::vec2(280.0, 64.0)),
                );
                if b.clicked() {
                    start_req = true;
                }
            });
            if !self.state.cfg.valid() {
                ui.colored_label(Color32::YELLOW, INVALID_CFG.t(&locale));
            }
        });
        if start_req {
            self.start_current();
        }
    }

    fn playing_ui(&mut self, ui: &mut egui::Ui) {
        let locale = self.state.cfg.locale.clone();
        self.frame_step();
        let (tick, alerts) = match &self.session {
            Some(s) => (s.sim.world.tick, s.alerts()),
            None => (0, Vec::new()),
        };

        egui::Panel::top("hud").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if self.state.paused {
                        PLAY_SPEED.t(&locale)
                    } else {
                        PAUSE_BTN.t(&locale)
                    })
                    .clicked()
                {
                    self.state.toggle_pause();
                }
                ui.label(SPEED.t(&locale));
                for s in [1u8, 2, 3] {
                    let mut btn = Button::new(format!("{}x", s)).small();
                    if self.state.speed == s {
                        btn = btn.fill(Color32::from_rgb(60, 80, 140));
                    }
                    if ui.add(btn).clicked() {
                        self.state.set_speed(s);
                    }
                }
                ui.separator();
                ui.label(format!("{} {}", TICK.t(&locale), tick));
                if let Some(s) = &self.session {
                    if !s.sandbox {
                        ui.colored_label(Color32::YELLOW, "CAMPAIGN");
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(BACK_MENU.t(&locale)).clicked() {
                    self.state.back_to_menu();
                    self.session = None;
                    return;
                }
                if ui.button(SAVE_GAME.t(&locale)).clicked() {
                    let (msg, ok) = match &self.session {
                        None => (SAVE_FAIL.t(&locale).to_string(), false),
                        Some(s) => {
                            let sg = save_from_session(s);
                            match save_io::write_bytes(&save_io::save_path(), &save::encode(&sg)) {
                                Ok(()) => (SAVE_OK.t(&locale).to_string(), true),
                                Err(_) => (SAVE_FAIL.t(&locale).to_string(), false),
                            }
                        }
                    };
                    self.save_feedback = Some((msg, ok));
                }
                if ui.button(LOAD_GAME.t(&locale)).clicked() {
                    match load_save() {
                        Some(s) => {
                            self.session = Some(s);
                            self.save_feedback = Some((LOAD_OK.t(&locale).to_string(), true));
                        }
                        None => self.save_feedback = Some((NO_SAVE.t(&locale).to_string(), false)),
                    }
                }
                if let Some((msg, ok)) = self.save_feedback.clone() {
                    let c = if ok { Color32::from_rgb(90, 200, 90) } else { Color32::YELLOW };
                    ui.colored_label(c, msg);
                }
                for a in alerts {
                    ui.colored_label(Color32::LIGHT_RED, alert_label(a).t(&locale));
                }
            });
        });

        egui::Panel::right("side").exact_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(COLONY_PANEL.t(&locale));
                if self.session.is_none() {
                    return;
                }
                let (pop_count, job_rows) = {
                    let s = self.session.as_ref().unwrap();
                    let pop = s.sim.world.storages.iter::<Pawn>().count();
                    let rows: Vec<(u32, bool)> = s
                        .sim
                        .world
                        .storages
                        .iter::<Job>()
                        .filter(|(_, j)| !j.done)
                        .map(|(e, j)| (e.0, j.assigned.is_some()))
                        .collect();
                    (pop, rows)
                };
                ui.label(format!("{} {}", POP.t(&locale), pop_count));
                ui.separator();
                ui.label(RichText::new(BUILDINGS.t(&locale)).strong());
                let sel = self.pending_build;
                let buttons = [
                    (BuildKind::Shelf, STOR_SHELF.t(&locale)),
                    (BuildKind::Bed, BED_BUILD.t(&locale)),
                    (BuildKind::Farm, FARM_BUILD.t(&locale)),
                    (BuildKind::WorkZone, WORK_ZONE.t(&locale)),
                ];
                for (k, lbl) in buttons {
                    let mut btn = Button::new(lbl).min_size(egui::vec2(270.0, 26.0));
                    if sel == Some(k) {
                        btn = btn.fill(Color32::from_rgb(60, 80, 140));
                    }
                    if ui.add(btn).clicked() {
                        self.pending_build = if sel == Some(k) { None } else { Some(k) };
                    }
                }
                if self.pending_build.is_some() {
                    ui.colored_label(Color32::YELLOW, CLICK_MAP.t(&locale));
                }
                ui.separator();

                ui.label(RichText::new(JOB_QUEUE.t(&locale)).strong());
                if job_rows.is_empty() {
                    ui.label(NO_JOBS.t(&locale));
                }
                for (id, assigned) in job_rows.iter().take(12) {
                    ui.horizontal(|ui| {
                        ui.label(format!("#{} {}", id, if *assigned { "assigned" } else { "open" }));
                        if !assigned && ui.button(ASSIGN.t(&locale)).clicked() {
                            if let Some(s) = self.session.as_mut() {
                                if let Some(pawn) = s.pick_unassigned_pawn() {
                                    s.issue_assign_job(*id, pawn);
                                }
                            }
                        }
                    });
                }
                ui.separator();

                ui.label(RichText::new(PAWNS_PANEL.t(&locale)).strong());
                let pawns: Vec<(u32, String)> = {
                    let s = self.session.as_ref().unwrap();
                    s.sim
                        .world
                        .storages
                        .iter::<Pawn>()
                        .map(|(e, _)| (e.0, mode_label(s, e.0, &locale)))
                        .collect()
                };
                for (id, mode) in pawns.iter().take(8) {
                    let is_sel = self.selected_pawn == Some(*id);
                    ui.horizontal(|ui| {
                        let mut btn = Button::new(format!("{} #{}", mode, id)).min_size(egui::vec2(150.0, 22.0));
                        if is_sel {
                            btn = btn.fill(Color32::from_rgb(50, 90, 150));
                        }
                        if ui.add(btn).clicked() {
                            self.selected_pawn = Some(*id);
                        }
                    });
                    if is_sel {
                        if let Some(n) = self.needs_for(*id) {
                            bar(ui, HUNGER.t(&locale), n.hunger);
                            bar(ui, ENERGY.t(&locale), n.energy);
                            bar(ui, MOOD.t(&locale), n.mood);
                            bar(ui, HEALTH.t(&locale), n.hunger.min(n.energy));
                        }
                        ui.horizontal(|ui| {
                            if ui.button(IDLE.t(&locale)).clicked() {
                                if let Some(s) = self.session.as_mut() {
                                    s.issue(Command::Wait { pawn: *id });
                                }
                            }
                            if ui.button("X").clicked() {
                                self.selected_pawn = None;
                            }
                        });
                        ui.separator();
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(s) = &self.session else { return };
            let mut grid: Vec<Color32> = vec![Color32::from_rgb(38, 66, 40); (MAP as usize) * (MAP as usize)];
            for (_, sit) in s.sim.world.storages.iter::<Site>() {
                let t = sit.tile;
                if sit.kind == 1 {
                    let i = (t.y.max(0) as usize) * (MAP as usize) + (t.x.max(0) as usize);
                    if i < grid.len() {
                        grid[i] = Color32::from_rgb(170, 80, 90);
                    }
                } else {
                    let i = (t.y.max(0) as usize) * (MAP as usize) + (t.x.max(0) as usize);
                    if i < grid.len() {
                        grid[i] = Color32::from_rgb(150, 110, 50);
                    }
                }
            }
            for (e, p) in s.sim.world.storages.iter::<Pos>() {
                if s.sim.world.storages.get::<Pawn>(e).is_none() {
                    continue;
                }
                let t = p.tile();
                let x = (t.x.max(0) as usize).min(MAP as usize - 1);
                let y = (t.y.max(0) as usize).min(MAP as usize - 1);
                let i = y * (MAP as usize) + x;
                grid[i] = Color32::from_rgb(198, 208, 224);
            }

            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(VIEW, VIEW),
                egui::Sense::click() | egui::Sense::hover(),
            );
            if !ui.is_rect_visible(rect) {
                return;
            }
            let p = rect.left_top();
            for (i, c) in grid.iter().enumerate() {
                let x = (i % MAP as usize) as f32;
                let y = (i / MAP as usize) as f32;
                let tl = egui::pos2(p.x + x * CELL, p.y + y * CELL);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(tl, egui::vec2(CELL - 1.0, CELL - 1.0)),
                    1.0,
                    *c,
                );
            }

            if let Some(pos) = resp.interact_pointer_pos() {
                let tx = ((pos.x - p.x) / CELL) as i16;
                let ty = ((pos.y - p.y) / CELL) as i16;
                let in_map = tx >= 0 && ty >= 0 && tx < MAP as i16 && ty < MAP as i16;
                if resp.clicked() {
                    if let Some(kind) = self.pending_build.take() {
                        if in_map {
                            self.issue_build(kind, tx, ty);
                        }
                    } else if let Some(id) = self.find_pawn_at(tx, ty) {
                        self.selected_pawn = Some(id);
                    } else {
                        self.selected_pawn = None;
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(kind) = self.pending_build {
                        if in_map {
                            self.issue_build(kind, tx, ty);
                        }
                    } else if let Some(id) = self.find_pawn_at(tx, ty) {
                        self.selected_pawn = Some(id);
                    } else if self.selected_pawn.is_some() && in_map {
                        self.issue_move((tx, ty));
                    }
                }
            }
        });
    }
}

impl UiApp {
    fn issue_build(&mut self, kind: BuildKind, x: i16, y: i16) {
        if let Some(s) = &mut self.session {
            s.issue_build(kind, x, y);
        }
    }
    fn issue_move(&mut self, tile: (i16, i16)) {
        if let (Some(s), Some(pawn)) = (self.session.as_mut(), self.selected_pawn) {
            s.issue(Command::Move { pawn, x: tile.0, y: tile.1 });
        }
    }
}

fn bar(ui: &mut egui::Ui, label: &str, v: i64) {
    let frac = (v.max(0) as f32).min(65536.0) / 65536.0f32;
    ui.horizontal(|ui| {
        ui.label(format!("{:>7}", label));
        ui.add(egui::ProgressBar::new(frac).desired_width(140.0));
    });
}

fn mode_label(s: &Session, id: u32, locale: &str) -> String {
    match s.sim.world.storages.get::<Pawn>(Entity(id)) {
        None => IDLE.t(locale).to_string(),
        Some(p) => match &p.mode {
            Mode::Idle => IDLE.t(locale).to_string(),
            Mode::Go { .. } => WALKING.t(locale).to_string(),
            Mode::Eat { .. } => EATING.t(locale).to_string(),
            Mode::Sleep { .. } => SLEEPING.t(locale).to_string(),
            Mode::Work { .. } => WORKING.t(locale).to_string(),
        },
    }
}

fn save_from_session(s: &Session) -> SaveGame {
    SaveGame {
        version: save::SAVE_VERSION,
        seed: s.seed,
        sandbox: s.sandbox,
        difficulty: s.difficulty,
        preset: Preset::index(s.preset),
        ticks: s.sim.world.tick,
        batches: s.live_batches_snapshot(),
    }
}

fn load_save() -> Option<Session> {
    let p = save_io::load_path_if_exists()?;
    let bytes = save_io::read_bytes(&p).ok()?;
    let sg = save::decode(&bytes).ok()?;
    Some(Session::from_saved(sg))
}