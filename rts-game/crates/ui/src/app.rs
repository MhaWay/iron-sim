//! eframe app: main menu, custom-match screen, live match view.
//!
//! The sim steps EXACTLY once per tick with canonical sorted batches; egui floats are view-only.

use crate::config::{MatchConfig, SlotKind};
use crate::netmod::{LobbyView, NetController, NetMirror};
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
    /// Multiplayer mode: commands flow through the net session instead of local stepping.
    pub net_mode: bool,
    /// Mirror of the session's authoritative sim (only used when net_mode).
    pub net_sim: Option<Sim>,
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
            net_mode: false,
            net_sim: None,
        }
    }

    pub fn tick(&mut self) -> Result<(), game::SimError> {
        // Multiplayer: the session thread steps the authoritative sim (HostSession::
        // step_tick / ClientSession::apply_batch on its own tasks) — the UI thread
        // never calls Sim::step. netmode mirrors only.
        if self.net_mode {
            return Ok(());
        }
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
        self.sim_ref()
            .world
            .storages
            .iter::<Unit>()
            .filter(|(_, u)| u.owner as u32 == owner)
            .count()
    }

    /// The sim the view should READ: authoritative mirror in net mode, local sim otherwise.
    pub fn sim_ref(&self) -> &Sim {
        if self.net_mode {
            self.net_sim.as_ref().unwrap_or(&self.sim)
        } else {
            &self.sim
        }
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
    /// Session-thread controller (None = singleplayer / no live session).
    pub net: NetController,
    /// Mirrored session state consumed by the UI each frame.
    pub net_mirror: NetMirror,
    /// Host lobby view (None = lobby not yet broadcast).
    pub lobby: Option<LobbyView>,
    // --- menu form state ---
    pub host_addr: String,
    pub host_seed_str: String,
    pub join_addr: String,
    pub join_code: String,
    pub join_seed_str: String,
    pub net_err: Option<String>,
    pub net_status: Option<String>,
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
            net: NetController::new(),
            net_mirror: NetMirror::default(),
            lobby: None,
            host_addr: "127.0.0.1:4738".to_string(),
            host_seed_str: "12345".to_string(),
            join_addr: "127.0.0.1:4738".to_string(),
            join_code: String::new(),
            join_seed_str: "12345".to_string(),
            net_err: None,
            net_status: None,
        }
    }

    pub fn start_match(&mut self) {
        let is_net = self.net.handle().is_some();
        let v = MatchView::new(&self.state.cfg);
        self.view = Some(if is_net {
            let mut v = v;
            v.net_mode = true;
            v
        } else {
            v
        });
        self.sel.clear();
        self.pending_place = None;
        self.state.screen = Screen::Playing;
    }

    pub fn issue(&mut self, cmd: Command) {
        // Net mode: queue the command on the session thread (tick stamping happens there).
        if let Some(h) = self.net.handle() {
            let _ = h.input_tx.send((self.local, cmd));
            return;
        }
        if let Some(v) = &mut self.view {
            v.live.push((self.local, cmd));
        }
    }

    fn frame_texture(&mut self, ctx: &egui::Context) {
        if let Some(v) = &self.view {
            // Multiplayer renders the mirrored authoritative sim; singleplayer the local one.
            if v.net_mode {
                if let Some(s) = &v.net_sim {
                    let civ = s.players[self.local as usize].civ;
                    render::scene::render_frame(
                        s,
                        &mut self.img,
                        civ,
                        s.tick(),
                        &self.state.cfg.locale,
                    );
                }
            } else {
                let civ = v.sim.players[self.local as usize].civ;
                render::scene::render_frame(
                    &v.sim,
                    &mut self.img,
                    civ,
                    v.sim.tick(),
                    &self.state.cfg.locale,
                );
            }
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
        v.sim_ref()
            .world
            .storages
            .iter::<Unit>()
            .find_map(|(e, u)| {
                if u.owner as u32 != self.local {
                    return None;
                }
                v.sim_ref().tile_of_unit(e.0).and_then(|(x, y)| {
                    if x == tx && y == ty {
                        Some(e.0)
                    } else {
                        None
                    }
                })
            })
    }

    fn find_enemy_unit_at(&self, tx: i16, ty: i16) -> Option<u32> {
        let v = self.view.as_ref()?;
        v.sim_ref()
            .world
            .storages
            .iter::<Unit>()
            .find_map(|(e, u)| {
                if u.owner as u32 == self.local {
                    return None;
                }
                v.sim_ref().tile_of_unit(e.0).and_then(|(x, y)| {
                    if x == tx && y == ty {
                        Some(e.0)
                    } else {
                        None
                    }
                })
            })
    }
}

impl UiApp {
    /// Start the live match: host fires NetMsg::Start via the session thread; the
    /// view renders from the mirrored authoritative sim instead of stepping locally.
    pub fn start_match_net(&mut self) {
        let Some(h) = self.net.handle() else {
            self.start_match();
            return;
        };
        if h.is_host {
            h.start_match();
        }
        self.state.paused = false;
        self.state.screen = Screen::Playing;
        let mut v = MatchView::new(&self.state.cfg);
        v.net_mode = true;
        v.net_sim = None; // populated by authoritative snapshots in sync_net
        self.view = Some(v);
        self.local = if h.is_host { 0 } else { 1 };
        self.sel.clear();
        self.pending_place = None;
    }

    /// Called every frame: consume session events and sync the mirrored sim.
    fn sync_net(&mut self) {
        self.net.drain_into(&mut self.net_mirror);
        if let Some(lobby) = self.net_mirror.lobby.clone() {
            self.lobby = Some(lobby);
        }
        if let Some(v) = &mut self.view {
            if v.net_mode {
                if let Some(s) = self.net.snapshot() {
                    v.net_sim = Some(s);
                }
            }
        }
    }
}

impl eframe::App for UiApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        static LOGIC1: std::sync::Once = std::sync::Once::new();
        LOGIC1.call_once(|| crate::diag::line("first logic() call (window alive, event loop ok)"));
        // Continuous swap like Unity/Unreal: request a repaint EVERY frame so a lost
        // or dropped present cannot leave the window black forever (egui is reactive
        // by default and will sit idle if a swap is lost).
        _ctx.request_repaint();
        self.sync_net();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        static UI1: std::sync::Once = std::sync::Once::new();
        UI1.call_once(|| {
            let scr = format!("{:?}", self.state.screen);
            crate::diag::line(&format!(
                "first ui() draw, screen={scr}, pixels_per_point={}",
                ui.ctx().pixels_per_point()
            ));
        });
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
            let hostbtn = if it { "HOST PARTITA" } else { "HOST GAME" };
            if ui
                .add(
                    Button::new(
                        RichText::new(hostbtn)
                            .size(24.0)
                            .color(Color32::LIGHT_GREEN),
                    )
                    .min_size(egui::vec2(400.0, 64.0)),
                )
                .clicked()
            {
                self.net_err = None;
                self.net_status = None;
                self.lobby = None;
                let seed = self.host_seed_str.parse().unwrap_or(self.state.cfg.seed);
                if let Err(e) =
                    self.net
                        .start_host(seed, self.state.cfg.preset.clone(), self.host_addr.clone())
                {
                    self.net_err = Some(e);
                }
            }
            ui.add_space(16.0);
            let joinbtn = if it { "UNISCI PARTITA" } else { "JOIN GAME" };
            if ui
                .add(
                    Button::new(
                        RichText::new(joinbtn)
                            .size(24.0)
                            .color(Color32::from_rgb(150, 200, 255)),
                    )
                    .min_size(egui::vec2(400.0, 64.0)),
                )
                .clicked()
            {
                self.net_err = None;
                self.net_status = None;
                self.lobby = None;
                let seed = self.join_seed_str.parse().unwrap_or(self.state.cfg.seed);
                if let Err(e) = self.net.start_client(
                    self.join_addr.clone(),
                    seed,
                    self.state.cfg.preset.clone(),
                ) {
                    self.net_err = Some(e);
                }
            }
            ui.add_space(16.0);
            if let Some(e) = self.net_err.clone() {
                ui.colored_label(Color32::RED, e);
            } else if let Some(st) = self.net_status.clone() {
                ui.colored_label(Color32::YELLOW, st);
            }
            // Host controls: address + seed, and lobby info once live.
            if self.net.handle().is_some_and(|h| h.is_host) {
                ui.horizontal(|ui| {
                    ui.label(if it { "Indirizzo:" } else { "Address:" });
                    ui.add(egui::TextEdit::singleline(&mut self.host_addr).desired_width(160.0));
                    ui.label("Seed:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.host_seed_str).desired_width(100.0),
                    );
                });
                if let Some(lobby) = &self.lobby {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(if it { "Codice:" } else { "Code:" });
                            ui.label(RichText::new(lobby.lobby_code.clone()).strong().monospace());
                        });
                        ui.label(format!(
                            "{} / {}  {}",
                            lobby.connected_count(),
                            lobby.max_players,
                            if lobby.started {
                                if it {
                                    "avviata"
                                } else {
                                    "started"
                                }
                            } else {
                                "lobby"
                            }
                        ));
                        ui.label(if it {
                            "Code (base32 seed) = invito LAN"
                        } else {
                            "code (base32 of seed) for LAN invite"
                        });
                    });
                    if ui.button("START MATCH").clicked() {
                        self.net_status = None;
                        self.start_match_net();
                    }
                } else if let Some(h) = self.net.handle() {
                    let code = h.lobby_code.clone();
                    ui.label(if it {
                        "Host live — condividi il codice:"
                    } else {
                        "Host live — share the code:"
                    });
                    ui.label(RichText::new(code).strong().monospace().size(18.0));
                }
            } else if self.net.handle().is_some_and(|h| !h.is_host) {
                // Client joining: confirm addr+seed (invite code = base32 of seed).
                ui.horizontal(|ui| {
                    ui.label("Addr:");
                    ui.add(egui::TextEdit::singleline(&mut self.join_addr).desired_width(160.0));
                    ui.label("Seed:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.join_seed_str).desired_width(100.0),
                    );
                });
                ui.label(if it {
                    "Code (base32 del seed):"
                } else {
                    "code (base32 of seed):"
                });
                if let Some(lobby) = &self.lobby {
                    ui.label(RichText::new(lobby.lobby_code.clone()).strong().monospace());
                }
                if self.net_mirror.started {
                    ui.colored_label(
                        Color32::LIGHT_GREEN,
                        if it { "collegato" } else { "connected" },
                    );
                } else {
                    ui.colored_label(
                        Color32::YELLOW,
                        if it {
                            "connessione…"
                        } else {
                            "connecting…"
                        },
                    );
                }
            }
            ui.add_space(16.0);
            let quit = if it { "Esci" } else { "Exit" };
            if ui
                .add(Button::new(RichText::new(quit).size(20.0)).min_size(egui::vec2(400.0, 48.0)))
                .clicked()
            {
                self.net.stop();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.net.handle().is_some() {
                let label = if it {
                    "Chiudi sessione"
                } else {
                    "Close session"
                };
                if ui.button(label).clicked() {
                    self.net.stop();
                    self.lobby = None;
                    self.net_mirror = NetMirror::default();
                }
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
                    if self.net.handle().is_some() {
                        self.start_match_net();
                    } else {
                        self.start_match();
                    }
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
                Some(v) => (v.sim_ref().tick(), v.paused, v.last_err.clone()),
                None => (0, false, None),
            };
            let winner = self.view.as_ref().map_or(0, |v| {
                v.sim_ref().players[0]
                    .winner
                    .max(v.sim_ref().players[1].winner)
            });

            ui.heading(if it { "Partita" } else { "Match" });
            // --- Multiplayer overlay: peers, latency, desync/rejoin ---
            if self.view.as_ref().is_some_and(|v| v.net_mode) {
                let (peers, rtt, desync, err) = {
                    let m = &self.net_mirror;
                    (
                        m.connected_count(),
                        m.last_rtt_millis,
                        m.desync,
                        m.error.clone(),
                    )
                };
                let max_p = self
                    .net_mirror
                    .lobby
                    .as_ref()
                    .map_or(2, |l| l.max_players as usize);
                ui.label(format!("Peers: {peers}/{max_p}"));
                if let Some(r) = rtt {
                    ui.label(format!("RTT {r}ms"));
                }
                if let Some((tick, exp, got)) = desync {
                    ui.colored_label(
                        Color32::RED,
                        RichText::new(format!("DESYNC t{tick} (host {exp} vs local {got})"))
                            .size(16.0)
                            .strong(),
                    );
                }
                if (err.is_some() || desync.is_some())
                    && ui
                        .button(if it { "Riconnnetti (rejoin)" } else { "Rejoin" })
                        .clicked()
                {
                    if let Some(h) = self.net.handle() {
                        h.request_reconnect();
                    }
                }
                if let Some(e) = err {
                    ui.colored_label(Color32::RED, e);
                }
            }
            ui.horizontal(|ui| {
                if ui.button(if paused { "▶" } else { "⏸" }).clicked() {
                    if let Some(h) = self.net.handle() {
                        if h.is_host {
                            h.pause(!paused);
                        }
                    } else if let Some(v) = &mut self.view {
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
                let p = &v.sim_ref().players[self.local as usize];
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
                let bar = v.sim_ref().first_building_entity_of(self.local, 3);
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
                if self.net.handle().is_some() {
                    self.net.stop();
                    self.lobby = None;
                    self.net_mirror = NetMirror::default();
                    self.view = None;
                }
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
                            level: 1,
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
