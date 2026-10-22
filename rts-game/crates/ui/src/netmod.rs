//! Session-thread bridge between the eframe UI and crates/net.
//!
//! Thread model (docs/netcode.md): the UI thread is render-only. A dedicated
//! "session thread" owns a single-threaded tokio runtime; the HostSession /
//! ClientSession state machines — and therefore the authoritative deterministic
//! sim stepping — run on that thread's tasks, NEVER on the UI thread. Host steps
//! via `host_tick_loop` (the `SendSession` wrapper from crates/server/src/main.rs);
//! clients step by applying authoritative `TickBatch`es on the client task
//! (`client_run` → `ClientSession::apply_batch`).
//!
//! Cross-boundary comms are UI-safe: `std::sync::mpsc` for both directions and the
//! net crate's tokio `watch` channel inside the session thread. Sessions are `!Send`
//! (ECS scheduler box) and live single-threaded behind their own task boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use game::sim::MAX_PLAYERS;
use net::{
    bind_host, client_run, event_channel, host_accept_loop, host_tick_loop, ClientSession,
    HostSession, SendSession, SessionCfg, SessionEvent,
};
use shared::netcode::{invite_code, NetMsg};

/// Live commands typed into the UI are queued to the session thread:
/// (sim_player_index, command). Host-side: the session loop stamps `tick +
/// INPUT_DELAY` when it queues. Client-side: `client_run` applies the delay.
pub type QueuedInput = (u32, shared::Command);

/// Join parameters typed on the Join menu.
#[derive(Clone, Debug)]
pub struct JoinTarget {
    pub addr: String,
    pub seed: u64,
    pub preset: String,
}

/// The authoritative lobby view mirrored from a live HostSession.
#[derive(Clone, Debug)]
pub struct LobbyView {
    pub lobby_code: String,
    /// (id, name, ready, connected)
    pub players: Vec<(u32, String, bool, bool)>,
    pub started: bool,
    pub max_players: u8,
}

impl LobbyView {
    pub fn connected_count(&self) -> usize {
        self.players.iter().filter(|(_, _, _, c)| *c).count()
    }
}

/// One UI frame's worth of session state mirrored from the session thread.
#[derive(Clone, Debug, Default)]
pub struct NetMirror {
    pub connected_peers: Vec<(u32, bool)>,
    pub last_rtt_millis: Option<u64>,
    /// (tick, hash) of the last applied batch — the in-game latency indicator.
    pub last_batch: Option<(u64, u64)>,
    /// (tick, expected, got) — set by SessionEvent::Desync, cleared on Resync/close.
    pub desync: Option<(u64, u64, u64)>,
    pub error: Option<String>,
    /// True once the match is live (host Start confirmed / client Welcome).
    pub started: bool,
    /// Authoritative lobby snapshot (host mirror) — last Lobby event.
    pub lobby: Option<LobbyView>,
    /// Remote pause state mirrored from the session.
    pub paused: bool,
    /// Latest mirrored sim hash / tick (host or client authoritative hash).
    pub last_hash: Option<u64>,
}

impl NetMirror {
    pub fn connected_count(&self) -> usize {
        self.connected_peers.iter().filter(|(_, c)| *c).count()
    }
}

/// Session-thread → UI-thread events (mirrored, then applied to NetMirror).
pub enum UiEvent {
    AppliedBatch {
        tick: u64,
        hash: u64,
    },
    Desync {
        tick: u64,
        expected: u64,
        got: u64,
    },
    Resynced {
        tick: u64,
    },
    PeerInfo {
        id: u32,
        connected: bool,
    },
    Rtt(u64),
    Started,
    /// Lobby snapshot mirrored from SessionEvent::Lobby.
    Lobby(LobbyView),
    /// Host paused/unpaused the match.
    Paused(bool),
    Error(String),
    Closed,
}

/// Shared slot: worker publishes the authoritative sim snapshot after each batch
/// (host: after its own tick loop step; client: after each applied batch), UI clones it.
type Snap = Arc<Mutex<Option<game::Sim>>>;

fn snap_pair() -> (Snap, Arc<AtomicBool>) {
    let s = Arc::new(Mutex::new(None));
    (s, Arc::new(AtomicBool::new(false)))
}

// (A snapshot is an owned Sim clone; no session state crosses threads.)

/// Commands the UI thread sends to the session thread.
enum SessionCommand {
    Start,
    Reconnect,
    Close,
    Pause(bool),
}

/// Handle into a live session thread, owned by UiApp::net.
#[derive(Clone)]
pub struct NetSessionHandle {
    cmd_tx: Sender<SessionCommand>,
    /// UI-safe input queue.
    pub input_tx: Sender<QueuedInput>,
    pub is_host: bool,
    pub seed: u64,
    pub lobby_code: String,
    pub player_id: u32,
    pub preset: String,
}

impl NetSessionHandle {
    pub fn start_match(&self) {
        let _ = self.cmd_tx.send(SessionCommand::Start);
    }

    pub fn request_reconnect(&self) {
        let _ = self.cmd_tx.send(SessionCommand::Reconnect);
    }

    pub fn close(&self) {
        let _ = self.cmd_tx.send(SessionCommand::Close);
    }
    pub fn pause(&self, paused: bool) {
        let _ = self.cmd_tx.send(SessionCommand::Pause(paused));
    }
}

/// Owns the session thread plus its event channels. One controller per UiApp;
/// `start_host` / `start_client` replace any previous session.
pub struct NetController {
    evt_tx: Sender<UiEvent>,
    evt_rx: Receiver<UiEvent>,
    handle: Option<NetSessionHandle>,
    worker: Option<JoinHandle<()>>,
    /// Session thread wakes immediately when the UI sends a command (zero latency).
    wake_tx: Sender<()>,
    /// Authoritative sim snapshot published by the live worker.
    snap: Snap,
    /// True while the worker publishes (set before join, cleared on stop).
    snap_on: Arc<AtomicBool>,
}

impl NetController {
    /// Clone the current authoritative sim snapshot, if any.
    pub fn snapshot(&self) -> Option<game::Sim> {
        self.snap.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

impl Default for NetController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NetController {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.close();
        }
    }
}

impl NetController {
    pub fn new() -> Self {
        let (evt_tx, evt_rx) = channel::<UiEvent>();
        let (wake_tx, _wake_rx) = channel::<()>();
        let (s_pair, s_on) = snap_pair();
        Self {
            evt_tx,
            evt_rx,
            handle: None,
            worker: None,
            wake_tx,
            snap: s_pair,
            snap_on: s_on,
        }
    }

    pub fn is_active(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    pub fn handle(&self) -> Option<&NetSessionHandle> {
        self.handle.as_ref()
    }

    fn wake(&self) {
        let _ = self.wake_tx.send(());
    }

    /// Drain this frame's session events (non-blocking) onto the UI mirror.
    pub fn drain_into(&mut self, m: &mut NetMirror) {
        while let Ok(e) = self.evt_rx.try_recv() {
            match e {
                UiEvent::AppliedBatch { tick, hash } => m.last_batch = Some((tick, hash)),
                UiEvent::Desync {
                    tick,
                    expected,
                    got,
                } => m.desync = Some((tick, expected, got)),
                UiEvent::Resynced { tick } => {
                    m.desync = None;
                    m.last_batch = Some((tick, 0));
                }
                UiEvent::PeerInfo { id, connected } => {
                    if let Some(p) = m.connected_peers.iter_mut().find(|(i, _)| *i == id) {
                        p.1 = connected;
                    } else {
                        m.connected_peers.push((id, connected));
                    }
                }
                UiEvent::Rtt(r) => m.last_rtt_millis = Some(r),
                UiEvent::Started => m.started = true,
                UiEvent::Lobby(l) => m.lobby = Some(l),
                UiEvent::Paused(p) => m.paused = p,
                UiEvent::Error(e) => m.error = Some(e),
                UiEvent::Closed => m.error = Some("connection closed".into()),
            }
        }
    }

    /// Tear down a live session (non-blocking: the worker exits asynchronously).
    pub fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.close();
        }
        if let Some(w) = self.worker.take() {
            if !w.is_finished() {
                // Never join from the UI thread — let a watchdog thread reap it.
                std::thread::spawn(move || {
                    let _ = w.join();
                });
            }
        }
    }

    /// Spawn a HostSession on a dedicated session thread. Binds `addr`.
    pub fn start_host(&mut self, seed: u64, preset: String, addr: String) -> Result<(), String> {
        self.stop();
        let (cmd_tx, cmd_rx) = channel::<SessionCommand>();
        let (input_tx, input_rx) = channel::<QueuedInput>();
        let (wake_ui_tx, wake_ui_rx) = std::sync::mpsc::channel::<()>();
        self.wake_tx = wake_ui_tx;
        let evt_tx = self.evt_tx.clone();
        let (tx, mut ev_rx) = event_channel();
        let cfg = SessionCfg {
            name: "host".into(),
            seed,
            preset: preset.clone(),
            is_host: true,
        };
        let lobby_code = invite_code(seed);
        let (snap, snap_on) = (self.snap.clone(), Arc::clone(&self.snap_on));
        *snap.lock().unwrap_or_else(|p| p.into_inner()) = None;
        snap_on.store(true, Ordering::Relaxed);

        let worker = std::thread::spawn(move || {
            // Single-threaded runtime: sessions stay pinned to this thread.
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = evt_tx.send(UiEvent::Error(format!("runtime: {e}")));
                    return;
                }
            };
            let host = Arc::new(Mutex::new(HostSession::new(cfg, tx)));
            rt.block_on(async move {
                let endpoint = match bind_host(&addr) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = evt_tx.send(UiEvent::Error(format!("bind {addr}: {e}")));
                        return;
                    }
                };
                let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
                let accept = tokio::spawn(host_accept_loop(
                    endpoint,
                    SendSession::wrap(Arc::clone(&host)),
                    stop_rx.clone(),
                ));
                let tick = tokio::spawn(host_tick_loop(
                    SendSession::wrap(Arc::clone(&host)),
                    stop_rx.clone(),
                ));

                // Wake channel: std → tokio bridge so UI commands act instantly.
                let (wake_tk_tx, mut wake_tk_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
                let wake_bridge = tokio::spawn(async move {
                    while wake_ui_rx.recv().is_ok() {
                        if wake_tk_tx.send(()).is_err() {
                            break;
                        }
                    }
                });

                let mut last_seen =
                    SessionEvent::Hello { wire_version: 0, proto: 0, name: String::new() };

                let mut stop_c = stop_rx.clone();
                loop {
                    tokio::select! {
                        _ = stop_c.changed() => break,
                        _ = wake_tk_rx.recv() => {}
                        _ = ev_rx.changed() => {}
                        _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    }
                    // 1. UI commands.
                    let mut stop_now = false;
                    while let Ok(c) = cmd_rx.try_recv() {
                        let h = &mut *host.lock().unwrap_or_else(|p| p.into_inner());
                        match c {
                            SessionCommand::Start => {
                                let _ = h.handle_msg(Some(1), None, NetMsg::Start);
                            }
                            SessionCommand::Reconnect => {}
                            SessionCommand::Close => stop_now = true,
                            SessionCommand::Pause(p) => {
                                let replies = h.handle_msg(Some(1), None, NetMsg::Pause { paused: p });
                                for m in replies {
                                    h.outbox.push_back(m);
                                }
                                h.paused = p;
                            }
                        }
                    }
                    // 2. Live inputs: host authority — pending[tick + INPUT_DELAY].
                    let next_tick =
                        host.lock().unwrap_or_else(|p| p.into_inner()).next_tick;
                    while let Ok((player, cmd)) = input_rx.try_recv() {
                        let h = &mut *host.lock().unwrap_or_else(|p| p.into_inner());
                        h.pending.entry(next_tick + net::INPUT_DELAY).or_default().push((player, cmd));
                    }
                    // 3. Mirror new session events to the UI (watch = last-value; dedupe).
                    let ev = ev_rx.borrow_and_update().clone();
                    let is_idle_hello =
                        matches!(&ev, SessionEvent::Hello { name, .. } if name.is_empty());
                    let same_idle = is_idle_hello
                        && matches!(&last_seen, SessionEvent::Hello { name, .. } if name.is_empty());
                    if !same_idle {
                        last_seen = ev.clone();
                        match ev {
                            SessionEvent::AppliedBatch { tick, hash, .. } => {
                                let _ = evt_tx.send(UiEvent::AppliedBatch { tick, hash });
                            }
                            SessionEvent::Desync { tick, expected, got } => {
                                let _ = evt_tx.send(UiEvent::Desync { tick, expected, got });
                            }
                            SessionEvent::PeerJoined { player_id, connected } => {
                                let _ = evt_tx.send(UiEvent::PeerInfo { id: player_id, connected });
                            }
                            SessionEvent::PeerLeft { player_id } => {
                                let _ = evt_tx.send(UiEvent::PeerInfo { id: player_id, connected: false });
                            }
                            SessionEvent::Resynced { tick } => {
                                let _ = evt_tx.send(UiEvent::Resynced { tick });
                            }
                            SessionEvent::Pong { rtt_millis } => {
                                let _ = evt_tx.send(UiEvent::Rtt(rtt_millis));
                            }
                            SessionEvent::Started => {
                                let _ = evt_tx.send(UiEvent::Started);
                            }
                            SessionEvent::Pause { paused } => {
                                let _ = evt_tx.send(UiEvent::Paused(paused));
                            }
                            _ => {}
                        }
                    }
                    // 4. Lobby snapshot straight from authoritative HostSession state
                    //    (SessionEvent::Lobby fires only on joins/leaves; the panel
                    //    also needs the pre-join lobby and the started flag).
                    {
                        let h = host.lock().unwrap_or_else(|p| p.into_inner());
                        let lv = LobbyView {
                            lobby_code: h.lobby_code.clone(),
                            players: h
                                .players
                                .iter()
                                .map(|pl| (pl.id, pl.name.clone(), pl.ready, pl.connected))
                                .collect(),
                            started: h.started,
                            max_players: MAX_PLAYERS as u8,
                        };
                        drop(h);
                        let _ = evt_tx.send(UiEvent::Lobby(lv));
                    }
                    if stop_now {
                        break;
                    }
                    // Publish authoritative sim snapshot (registered by the session thread).
                    if snap_on.load(Ordering::Relaxed) {
                        let started = host.lock().unwrap_or_else(|p| p.into_inner()).started;
                        if started {
                            if let Some(s) = net::snapshot_handle("host").get() {
                                *snap.lock().unwrap_or_else(|p| p.into_inner()) = Some(s);

                            }
                        }
                    }
                }
                let _ = stop_tx.send(true);
                accept.abort();
                tick.abort();
                wake_bridge.abort();
            });
        });

        self.handle = Some(NetSessionHandle {
            cmd_tx,
            input_tx,
            is_host: true,
            seed,
            lobby_code,
            player_id: 1,
            preset,
        });
        self.worker = Some(worker);
        self.wake();
        Ok(())
    }

    /// Spawn a ClientSession on a dedicated session thread: connects, sends
    /// Hello + JoinInvite(code = invite_code(seed)), then mirrors the host.
    pub fn start_client(&mut self, addr: String, seed: u64, preset: String) -> Result<(), String> {
        self.stop();
        let target: std::net::SocketAddr =
            addr.parse().map_err(|e| format!("bad addr {addr}: {e}"))?;
        let (cmd_tx, cmd_rx) = channel::<SessionCommand>();
        let (input_tx, input_rx) = channel::<QueuedInput>();
        let (wake_ui_tx, wake_ui_rx) = std::sync::mpsc::channel::<()>();
        self.wake_tx = wake_ui_tx;
        let evt_tx = self.evt_tx.clone();
        let (tx, mut ev_rx) = event_channel();
        let cfg = SessionCfg {
            name: "client".into(),
            seed,
            preset: preset.clone(),
            is_host: false,
        };
        let (snap, snap_on) = (self.snap.clone(), Arc::clone(&self.snap_on));
        *snap.lock().unwrap_or_else(|p| p.into_inner()) = None;
        snap_on.store(true, Ordering::Relaxed);

        let worker = std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = evt_tx.send(UiEvent::Error(format!("runtime: {e}")));
                    return;
                }
            };
            rt.block_on(async move {
                let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
                let client = ClientSession::new(cfg.clone(), tx);
                let (ui_input_tx, ui_input_rx) =
                    tokio::sync::mpsc::unbounded_channel::<(u64, Vec<(u32, shared::Command)>)>();
                let run = tokio::spawn(client_run(
                    target,
                    cfg.clone(),
                    client,
                    ui_input_rx,
                    stop_rx.clone(),
                ));
                // Bridge the std input queue → the tokio channel client_run reads.
                let pump = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        let mut batch: Vec<(u32, shared::Command)> = Vec::new();
                        while let Ok((p, c)) = input_rx.try_recv() {
                            batch.push((p, c));
                        }
                        if !batch.is_empty() && ui_input_tx.send((0, batch)).is_err() {
                            break;
                        }
                    }
                });
                let (wake_tk_tx, mut wake_tk_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
                let wake_bridge = tokio::spawn(async move {
                    while wake_ui_rx.recv().is_ok() {
                        if wake_tk_tx.send(()).is_err() {
                            break;
                        }
                    }
                });

                let mut stop_c = stop_rx.clone();
                loop {
                    tokio::select! {
                        _ = stop_c.changed() => break,
                        _ = wake_tk_rx.recv() => {}
                        _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    }
                    // UI commands (Close only; Reconnect not wired client-side here).
                    while let Ok(c) = cmd_rx.try_recv() {
                        match c {
                            SessionCommand::Close => {
                                let _ = stop_tx.send(true);
                            }
                            SessionCommand::Start
                            | SessionCommand::Reconnect
                            | SessionCommand::Pause(_) => {}
                        }
                    }
                    // Mirror session events (started / batches / desync / peers / rtt).
                    if snap_on.load(Ordering::Relaxed) {
                        if let Some(s) = net::snapshot_handle("client").get() {
                            *snap.lock().unwrap_or_else(|p| p.into_inner()) = Some(s);
                        }
                    }
                    let ev = ev_rx.borrow_and_update().clone();
                    let is_idle_hello =
                        matches!(&ev, SessionEvent::Hello { name, .. } if name.is_empty());
                    if is_idle_hello {
                        continue;
                    }
                    match ev {
                        SessionEvent::Started => {
                            let _ = evt_tx.send(UiEvent::Started);
                        }
                        SessionEvent::AppliedBatch { tick, hash, .. } => {
                            let _ = evt_tx.send(UiEvent::AppliedBatch { tick, hash });
                        }
                        SessionEvent::Desync {
                            tick,
                            expected,
                            got,
                        } => {
                            let _ = evt_tx.send(UiEvent::Desync {
                                tick,
                                expected,
                                got,
                            });
                        }
                        SessionEvent::PeerJoined {
                            player_id,
                            connected,
                        } => {
                            let _ = evt_tx.send(UiEvent::PeerInfo {
                                id: player_id,
                                connected,
                            });
                        }
                        SessionEvent::PeerLeft { player_id } => {
                            let _ = evt_tx.send(UiEvent::PeerInfo {
                                id: player_id,
                                connected: false,
                            });
                        }
                        SessionEvent::Resynced { tick } => {
                            let _ = evt_tx.send(UiEvent::Resynced { tick });
                        }
                        SessionEvent::Pong { rtt_millis } => {
                            let _ = evt_tx.send(UiEvent::Rtt(rtt_millis));
                        }
                        SessionEvent::Pause { paused } => {
                            let _ = evt_tx.send(UiEvent::Paused(paused));
                        }
                        SessionEvent::Refused { code, detail } => {
                            let _ =
                                evt_tx.send(UiEvent::Error(format!("refused {code:?}: {detail}")));
                        }
                        SessionEvent::Hello { name, .. } => {
                            if !name.is_empty() {
                                let _ = evt_tx.send(UiEvent::Error(name));
                            }
                        }
                        SessionEvent::Closed => {
                            let _ = evt_tx.send(UiEvent::Closed);
                            break;
                        }
                        _ => {}
                    }
                }
                let _ = stop_tx.send(true);
                run.abort();
                pump.abort();
                wake_bridge.abort();
            });
        });

        self.handle = Some(NetSessionHandle {
            cmd_tx,
            input_tx,
            is_host: false,
            seed,
            lobby_code: invite_code(seed),
            player_id: 0, // Welcome assigns the real id host-side; UI mirrors peers.
            preset,
        });
        self.worker = Some(worker);
        self.wake();
        Ok(())
    }
}
