//! eframe app: main menu, custom-match screen, live match view.
//!
//! The sim steps EXACTLY once per tick with canonical sorted batches; egui floats are view-only.

use crate::config::{MatchConfig, SlotKind};
use crate::state::{Screen, UiState};
use eframe::egui::{
    self, Button, Color32, ComboBox, RichText, Slider, TextureHandle, TextureOptions,
};
use game::economy::BUILDING_TABLE;
use game::sim::{Sim, Unit};
use render::png::Image;
use shared::Command;

const VIEW_W: usize = 1024;
const VIEW_H: usize = 1100;

pub struct MatchView {
    pub sim: Sim,
    pub schedule: Vec<shared::TickCommands>,
    pub schedule_idx: usize,
    pub live: Vec<(u32, Command)>,
    pub paused: bool,
    pub last_err: Option<String>,
}

impl MatchView {
    pub fn new(cfg: &MatchConfig) -> MatchView {
        let mut sim = Sim::new(cfg.seed);
        if let Some(p) = game::mapgen::preset_by_name(&cfg.preset) {
            sim.install_map(game::mapgen::generate_map_preset(cfg.seed, *p));
        }
        MatchView {
            sim,
            schedule: cfg.schedule(),
            schedule_idx: 0,
            live: Vec::new(),
            paused: false,
            last_err: None,
        }
    }

    pub fn tick(&mut self) -> Result<(), game::SimError> {
        let mut cmds: Vec<(u32, Command)> = Vec::new();
        if let Some(b) = self.schedule.get(self.schedule_idx) {
            if b.tick == self.sim.tick() {
                cmds.extend(b.cmds.iter().cloned());
                self.schedule_idx += 1;
            }
        }
        cmds.append(&mut self.live);
        cmds.sort_by(|(p, a), (q, b)| p.cmp(q).then_with(|| a.sort_key().cmp(&b.sort_key())));
        let r = self.sim.step(&cmds);
        self.last_err = r.as_ref().err().map(|e| format!("{e:?}"));
        r
    }

    pub fn unit_count(&self, owner: u32) -> usize {
        self.sim
            .world
            .storages
            .iter::<Unit>()
            .filter(|(_, u)| u.owner as u32 == owner)
            .count()
    }
}

pub struct UiApp {
    pub state: UiState,
    pub view: Option<MatchView>,
    img: Image,
    tex: Option<TextureHandle>,
    pub local: u32,
    pub sel: Vec<u32>,
    pub pending_place: Option<u16>,
}

impl Default for UiApp {
    fn default() -> Self {
        Self::new()
    }
}

impl UiApp {
    pub fn new() -> Self {
        UiApp {
            state: UiState::default(),
            view: None,
            img: Image::new(VIEW_W, VIEW_H),
            tex: None,
            local: 0,
            sel: Vec::new(),
            pending_place: None,
        }
    }

    pub fn start_match(&mut self) {
        self.view = Some(MatchView::new(&self.state.cfg));
        self.sel.clear();
        self.pending_place = None;
    }

    pub fn issue(&mut self, cmd: Command) {
        if let Some(v) = &mut self.view {
            v.live.push((self.local, cmd));
        }
    }

    fn frame_texture(&mut self, ctx: &egui::Context) {
        if let Some(v) = &self.view {
            let civ = v.sim.players[self.local as usize].civ;
            render::scene::render_frame(
                &v.sim,
                &mut self.img,
                civ,
                v.sim.tick(),
                &self.state.cfg.locale,
            );
        }
        let mut rgba: Vec<u8> = Vec::with_capacity(self.img.w * self.img.h * 4);
        for c in self.img.data.chunks(3) {
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([self.img.w, self.img.h], &rgba);
        match &mut self.tex {
            Some(t) => t.set(img, TextureOptions::NEAREST),
            None => self.tex = Some(ctx.load_texture("frame", img, TextureOptions::NEAREST)),
        }
    }

    /// First alive own-unit whose tile matches.
    fn find_unit_at(&self, tx: i16, ty: i16) -> Option<u32> {
        let v = self.view.as_ref()?;
        v.sim.world.storages.iter::<Unit>().find_map(|(e, u)| {
            if u.owner as u32 != self.local {
                return None;
            }
            v.sim
                .tile_of_unit(e.0)
                .and_then(|(x, y)| if x == tx && y == ty { Some(e.0) } else { None })
        })
    }

    fn find_enemy_unit_at(&self, tx: i16, ty: i16) -> Option<u32> {
        let v = self.view.as_ref()?;
        v.sim.world.storages.iter::<Unit>().find_map(|(e, u)| {
            if u.owner as u32 == self.local {
                return None;
            }
            v.sim
                .tile_of_unit(e.0)
                .and_then(|(x, y)| if x == tx && y == ty { Some(e.0) } else { None })
        })
    }
}

impl eframe::App for UiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.state.screen {
            Screen::Menu => self.menu_ui(ui),
            Screen::Custom => self.custom_ui(ui),
            Screen::Playing => self.playing_ui(ui),
        }
    }
}

fn slot_label(k: SlotKind, it: bool) -> &'static str {
    match (k, it) {
        (SlotKind::Human, true) => "Umano",
        (SlotKind::Human, false) => "Human",
        (SlotKind::Ai, true) => "Computer",
        (SlotKind::Ai, false) => "AI",
        (SlotKind::Off, true) => "Spento",
        (SlotKind::Off, false) => "Off",
    }
}

impl UiApp {
    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let it = self.state.cfg.locale == "it";
        let ctx = ui.ctx().clone();
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("Iron-Sim RTS").size(48.0).strong());
            ui.add_space(48.0);
            let create = if it {
                "Crea partita personalizzata"
            } else {
                "Create custom match"
            };
            if ui
                .add(
                    Button::new(RichText::new(create).size(24.0)).min_size(egui::vec2(400.0, 64.0)),
                )
                .clicked()
            {
                self.state.open_custom();
            }
            ui.add_space(16.0);
            let quit = if it { "Esci" } else { "Exit" };
            if ui
                .add(Button::new(RichText::new(quit).size(20.0)).min_size(egui::vec2(400.0, 48.0)))
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.add_space(32.0);
            ui.label(RichText::new("v0.3.0-ui · deterministic engine").weak());
        });
    }

    fn custom_ui(&mut self, ui: &mut egui::Ui) {
        let it = self.state.cfg.locale == "it";
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(if it {
                    "Partita personalizzata"
                } else {
                    "Custom match"
                })
                .size(32.0)
                .strong(),
            );
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if it { "Giocatori:" } else { "Players:" }).strong());
                    for i in 0..2usize {
                        ui.label(format!("P{}", i + 1));
                        let slot = self.state.cfg.players[i];
                        ComboBox::from_id_salt(format!("slot{i}"))
                            .selected_text(slot_label(slot, it))
                            .show_ui(ui, |ui| {
                                for k in [SlotKind::Human, SlotKind::Ai, SlotKind::Off] {
                                    ui.selectable_value(
                                        &mut self.state.cfg.players[i],
                                        k,
                                        slot_label(k, it),
                                    );
                                }
                            });
                    }
                    ui.checkbox(
                        &mut self.state.cfg.sandbox,
                        if it {
                            "Solo (nessun nemico)"
                        } else {
                            "Solo (no enemies)"
                        },
                    );
                });
            });

            ui.add_space(8.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if it { "Mappa:" } else { "Map:" }).strong());
                    let cur = self.state.cfg.preset.clone();
                    let shown = if cur.is_empty() {
                        (if it { "vecchia" } else { "legacy" }).to_string()
                    } else {
                        cur
                    };
                    ComboBox::from_id_salt("preset")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.state.cfg.preset,
                                String::new(),
                                if it { "vecchia (legacy)" } else { "legacy" },
                            );
                            for pm in game::mapgen::PRESETS {
                                ui.selectable_value(
                                    &mut self.state.cfg.preset,
                                    pm.name.to_string(),
                                    pm.name,
                                );
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if it { "Civiltà (P1):" } else { "Civ (P1):" }).strong(),
                    );
                    ComboBox::from_id_salt("civ")
                        .selected_text(if self.state.cfg.civ == 1 {
                            "Dominion"
                        } else {
                            "Adeptus"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.state.cfg.civ, 0u8, "Adeptus");
                            ui.selectable_value(&mut self.state.cfg.civ, 1u8, "Dominion");
                        });
                    ui.label(RichText::new(if it { "Lingua:" } else { "Locale:" }).strong());
                    ComboBox::from_id_salt("locale")
                        .selected_text(self.state.cfg.locale.clone())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.state.cfg.locale,
                                "en".to_string(),
                                "English",
                            );
                            ui.selectable_value(
                                &mut self.state.cfg.locale,
                                "it".to_string(),
                                "Italiano",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Seed:").strong());
                    let mut s = self.state.cfg.seed.to_string();
                    if ui.text_edit_singleline(&mut s).changed() {
                        if let Ok(v) = s.parse::<u64>() {
                            self.state.cfg.seed = v;
                        }
                    }
                    if ui.button(if it { "Casuale" } else { "Random" }).clicked() {
                        self.state.cfg.seed = (self.state.cfg.seed ^ (self.state.cfg.seed << 13))
                            .wrapping_mul(6364136223846793005);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if it {
                            "Durata (tick):"
                        } else {
                            "Duration (ticks):"
                        })
                        .strong(),
                    );
                    ui.add(Slider::new(&mut self.state.cfg.ticks, 600..=36000));
                });
            });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("< Menu").clicked() {
                    self.state.back_to_menu();
                }
                ui.add_space(24.0);
                let start = if it { "AVVIA PARTITA" } else { "START MATCH" };
                if ui
                    .add(
                        Button::new(RichText::new(start).size(24.0).color(Color32::GOLD))
                            .min_size(egui::vec2(280.0, 64.0)),
                    )
                    .clicked()
                    && self.state.start_match()
                {
                    self.start_match();
                }
            });
            if !self.state.cfg.valid() {
                ui.colored_label(
                    Color32::YELLOW,
                    if it {
                        "Configurazione non valida"
                    } else {
                        "Invalid configuration"
                    },
                );
            }
        });
    }
}

impl UiApp {
    fn playing_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.frame_texture(&ctx);
        let it = self.state.cfg.locale == "it";
        let tex_ready = self.tex.is_some();

        egui::Panel::right("side").exact_size(300.0).show(ui, |ui| {
            let (tick, paused, err) = match &self.view {
                Some(v) => (v.sim.tick(), v.paused, v.last_err.clone()),
                None => (0, false, None),
            };
            let winner = self
                .view
                .as_ref()
                .map_or(0, |v| v.sim.players[0].winner.max(v.sim.players[1].winner));

            ui.heading(if it { "Partita" } else { "Match" });
            ui.horizontal(|ui| {
                if ui.button(if paused { "▶" } else { "⏸" }).clicked() {
                    if let Some(v) = &mut self.view {
                        v.paused = !v.paused;
                    }
                }
                ui.label(format!("t{tick}"));
            });
            if let Some(e) = err {
                ui.colored_label(Color32::RED, e);
            }
            if winner != 0 {
                let msg = match winner {
                    1 | 2 => format!("P{} {}", winner, if it { "vince!" } else { "wins!" }),
                    _ => (if it { "Parità!" } else { "Draw!" }).to_string(),
                };
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(msg)
                            .size(20.0)
                            .strong()
                            .color(Color32::YELLOW),
                    );
                });
            }
            ui.separator();
            if let Some(v) = &self.view {
                let p = &v.sim.players[self.local as usize];
                ui.horizontal(|ui| {
                    ui.label("P1:");
                    ui.label(format!(
                        "F{} W{} G{} Age{}",
                        p.stock.food, p.stock.wood, p.stock.gold, p.age
                    ));
                });
                ui.label(format!("Pop {}", v.unit_count(self.local)));
            }

            ui.separator();
            ui.label(RichText::new(if it { "Costruzioni" } else { "Buildings" }).strong());
            for (i, b) in BUILDING_TABLE.iter().enumerate() {
                let cost = if b.wood > 0 {
                    format!("{}  {}w", b.name, b.wood)
                } else if b.gold > 0 {
                    format!("{}  {}g", b.name, b.gold)
                } else {
                    b.name.to_string()
                };
                let sel = self.pending_place == Some(i as u16);
                let mut btn = Button::new(cost).min_size(egui::vec2(270.0, 28.0));
                if sel {
                    btn = btn.fill(Color32::from_rgb(60, 80, 140));
                }
                if ui.add(btn).clicked() {
                    self.pending_place = if sel { None } else { Some(i as u16) };
                    self.sel.clear();
                }
            }
            if self.pending_place.is_some() {
                ui.colored_label(
                    Color32::YELLOW,
                    if it {
                        "Clicca la mappa"
                    } else {
                        "Click the map"
                    },
                );
            }

            if let Some(v) = &self.view {
                let bar = v.sim.first_building_entity_of(self.local, 3);
                if bar != 0 {
                    ui.separator();
                    ui.label(RichText::new(if it { "Addestra" } else { "Train" }).strong());
                    if ui.button(if it { "Milizia" } else { "Militia" }).clicked() {
                        self.issue(Command::Train {
                            building: bar,
                            unit_kind: 1,
                        });
                    }
                    if ui.button(if it { "Contadino" } else { "Serf" }).clicked() {
                        self.issue(Command::Train {
                            building: bar,
                            unit_kind: 0,
                        });
                    }
                }
            }

            ui.separator();
            if ui
                .button(if it { "Torna al menu" } else { "Back to menu" })
                .clicked()
            {
                self.state.back_to_menu();
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(view) = &mut self.view else { return };
            if !self.state.paused && !view.paused {
                if let Err(e) = view.tick() {
                    view.last_err = Some(format!("{e:?}"));
                }
            }
            if !tex_ready {
                return;
            }
            let resp = ui.add(
                egui::Image::from_texture(self.tex.as_ref().unwrap())
                    .fit_to_exact_size(egui::vec2(VIEW_W as f32, VIEW_H as f32))
                    .sense(egui::Sense::click()),
            );
            let pos = resp
                .interact_pointer_pos()
                .filter(|p| p.x >= 0.0 && p.y >= 0.0 && p.y < VIEW_H as f32);

            if let Some(p) = pos {
                let tx = (p.x / 8.0) as i16;
                let ty = (p.y / 8.0) as i16;
                if resp.clicked() {
                    if let Some(kind) = self.pending_place {
                        self.issue(Command::Build {
                            building: kind,
                            x: tx,
                            y: ty,
                        });
                        self.pending_place = None;
                    } else {
                        match self.find_unit_at(tx, ty) {
                            Some(e) => self.sel = vec![e],
                            None => self.sel.clear(),
                        }
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(target) = self.find_enemy_unit_at(tx, ty) {
                        let ids: Vec<u32> = self.sel.clone();
                        for id in ids {
                            self.issue(Command::Attack { unit: id, target });
                        }
                    } else {
                        let ids: Vec<u32> = self.sel.clone();
                        for id in ids {
                            self.issue(Command::Move {
                                unit: id,
                                x: tx,
                                y: ty,
                            });
                        }
                    }
                }
            }
        });
    }
}
