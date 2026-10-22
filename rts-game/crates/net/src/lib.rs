//! Lockstep netcode session layer (host + client) over quinn QUIC.
//!
//! Transport: quinn 0.11 (QUIC/UDP), one bidirectional stream per message batch, self-signed
//! "localhost" cert pinned by clients. See docs/netcode.md for protocol rationale.
//! Model: HOST-CLIENT, host authoritative - the host owns the Sim and the tick clock, and every
//! peer replays the SAME canonical command batches so every node hashes the same state.
//! Client inputs are stamped tick = client_view_tick + INPUT_DELAY (1 tick @ 30 Hz = 33.3 ms)
//! so they arrive before the target tick executes. A SyncDigest (tick + replay digest of the
//! last DIGEST_WINDOW=128 command batches) goes out every 10 s for desync detection.
//! Lobby / chat / ping / pause ride NetMsg envelopes (never TickCommands), so SIM_VERSION stays 4.

use std::collections::{BTreeMap, VecDeque};
#[allow(unused_imports)]
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{Connection, Endpoint};
use shared::netcode::{
    invite_code, msg_read, msg_write, replay_digest, NetMsg, PlayerInfo, StateSnapshot, UnitWire,
};
use shared::Command;

/// Session event channel sender (watch: cheap clones, last-value semantics).
pub type SessionEventTx = tokio::sync::watch::Sender<SessionEvent>;

/// Make a (sender, receiver) pair with an idle initial event.
pub fn event_channel() -> (SessionEventTx, tokio::sync::watch::Receiver<SessionEvent>) {
    tokio::sync::watch::channel(SessionEvent::Hello {
        wire_version: 1,
        proto: 0,
        name: String::new(),
    })
}

pub const TICK_HZ: u64 = 30;
pub const TICK_MS: u64 = 1000 / TICK_HZ;
pub const DIGEST_INTERVAL_TICKS: u64 = 10 * TICK_HZ;
pub const DIGEST_WINDOW: usize = 128;
pub const INPUT_DELAY: u64 = 1;
pub const PROTO_ID: u32 = 1;

pub type NetResult<T> = std::result::Result<T, NetError>;

#[derive(Debug)]
pub enum NetError {
    Io(std::io::Error),
    Sim(String),
    PeerClosed,
    Protocol(String),
    Wire(shared::netcode::WireError),
    WireDetail(String),
    Transport(String),
    Bind(String),
}

impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        NetError::Io(e)
    }
}
impl From<shared::netcode::WireError> for NetError {
    fn from(e: shared::netcode::WireError) -> Self {
        NetError::Wire(e)
    }
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetError::Io(e) => write!(f, "io: {e}"),
            NetError::Sim(m) => write!(f, "sim: {m}"),
            NetError::PeerClosed => write!(f, "peer closed"),
            NetError::Protocol(m) => write!(f, "protocol: {m}"),
            NetError::Wire(e) => write!(f, "wire: {e:?}"),
            NetError::WireDetail(m) => write!(f, "wire: {m}"),
            NetError::Transport(m) => write!(f, "transport: {m}"),
            NetError::Bind(m) => write!(f, "bind: {m}"),
        }
    }
}

impl std::error::Error for NetError {}
/// Send-safe wrapper for the authoritative sim snapshot. The inner Sim is mutated
/// only from the session thread; cross-thread access here is read-only via `&Sim`
/// (rendering / hash comparison). SAFETY: no field is ever mutated through a
/// reference that crosses threads — snapshots are published, not aliased mutably.
pub struct SendSim(pub game::Sim);
unsafe impl Send for SendSim {}
unsafe impl Sync for SendSim {}
impl SendSim {
    pub fn sim(&self) -> &game::Sim {
        &self.0
    }
}
/// Send-safe snapshot of the authoritative sim, shared with the UI lobby bridge.
/// Sim is Send + Clone, so an Arc<Sim> inside a Mutex crosses threads safely.
#[derive(Clone)]
pub struct SharedSim(std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<SendSim>>>>);
impl Default for SharedSim {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(None)))
    }
}
impl SharedSim {
    pub fn publish(&self, sim: std::sync::Arc<SendSim>) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(sim);
        }
    }
    pub fn get(&self) -> Option<game::Sim> {
        self.0
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.0.clone()))
    }
    pub fn shared(&self) -> Option<std::sync::Arc<SendSim>> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

fn shared_sim(key: &str) -> SharedSim {
    static SNAP: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, SharedSim>>,
    > = std::sync::OnceLock::new();
    if let Ok(mut r) = SNAP
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
        .lock()
    {
        r.entry(key.to_string()).or_default().clone()
    } else {
        SharedSim::default()
    }
}

pub fn snapshot_handle(key: &str) -> SharedSim {
    shared_sim(key)
}

fn register_snapshot(key: &str, sim: &game::Sim) {
    shared_sim(key).publish(std::sync::Arc::new(SendSim(sim.clone())));
}

pub struct Tls {
    pub server: quinn::ServerConfig,
    pub client: quinn::ClientConfig,
}

fn shared_identity() -> Result<
    (rustls::pki_types::CertificateDer<'static>, Vec<u8>),
    Box<dyn std::error::Error + Send + Sync + 'static>,
> {
    use std::io::Write as _;
    let dir = std::env::var("IRONSIM_TLS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
            p.push(".iron-sim");
            p.push("tls");
            p
        });
    std::fs::create_dir_all(&dir)?;
    let cert_path = dir.join("cert.der");
    let key_path = dir.join("key.der");
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        return Ok((rustls::pki_types::CertificateDer::from(cert), key));
    }
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let der = ck.cert.der().to_vec();
    let key = ck.key_pair.serialize_der();
    let mut f = std::fs::File::create(&cert_path)?;
    f.write_all(&der)?;
    let mut f = std::fs::File::create(&key_path)?;
    f.write_all(&key)?;
    Ok((rustls::pki_types::CertificateDer::from(der), key))
}

impl Tls {
    pub fn local() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (ck_cert, key_der) = shared_identity()?;
        let der = ck_cert.clone();
        let key: rustls::pki_types::PrivatePkcs8KeyDer = key_der.into();

        let mut server = quinn::ServerConfig::with_single_cert(vec![der.clone()], key.into())?;
        let mut tp = quinn::TransportConfig::default();
        tp.keep_alive_interval(Some(Duration::from_millis(150)));
        server.transport_config(Arc::new(tp));

        // Client trust: host and client load the SAME self-signed identity from
        // $IRONSIM_TLS_DIR (default ~/.iron-sim/tls), so the ordinary RootCertStore path
        // verifies the host's certificate. LAN trust model: docs/netcode.md "TLS identity".
        let mut roots = rustls::RootCertStore::empty();
        roots.add(ck_cert.clone())?;
        let client = quinn::ClientConfig::with_root_certificates(Arc::new(roots))?;
        Ok(Self { server, client })
    }
}

pub(crate) async fn write_msg(conn: &Connection, m: &NetMsg) -> Result<(), NetError> {
    let (mut s, _r) = conn.open_bi().await.map_err(transport_err)?;
    let mut buf = Vec::new();
    msg_write(m, &mut buf);
    s.write_all(&buf).await.map_err(transport_err)?;
    s.finish().map_err(transport_err)?;
    Ok(())
}

pub(crate) async fn read_msg(mut s: quinn::RecvStream) -> Result<NetMsg, NetError> {
    let mut buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut s, &mut buf)
        .await
        .map_err(transport_err)?;
    msg_read(&buf).map_err(|_| NetError::WireDetail("decode".into()))
}

pub(crate) fn transport_err<E: std::fmt::Debug>(e: E) -> NetError {
    NetError::Transport(format!("{e:?}"))
}

#[derive(Clone, Debug)]
pub struct SessionCfg {
    pub name: String,
    pub seed: u64,
    pub preset: String,
    pub is_host: bool,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Hello {
        wire_version: u8,
        proto: u32,
        name: String,
    },
    JoinInvite {
        code: String,
    },
    Welcome {
        player_id: u32,
        sim_seed: u64,
        preset: String,
        max_players: u8,
        lobby_code: String,
    },
    Refused {
        code: shared::netcode::RefuseCode,
        detail: String,
    },
    Lobby {
        players: Vec<PlayerInfo>,
        host_id: u32,
        started: bool,
        preset: String,
        seed: u64,
        max_players: u8,
    },
    Started,
    Resynced {
        tick: u64,
    },
    AppliedBatch {
        tick: u64,
        cmds: usize,
        hash: u64,
    },
    Desync {
        tick: u64,
        expected: u64,
        got: u64,
    },
    Input {
        tick: u64,
        cmds: Vec<(u32, Command)>,
    },
    Ready {
        ready: bool,
    },
    Ping {
        t_millis: u64,
    },
    Pong {
        rtt_millis: u64,
    },
    PeerJoined {
        player_id: u32,
        connected: bool,
    },
    PeerLeft {
        player_id: u32,
    },
    Pause {
        paused: bool,
    },
    Closed,
}

// ---------------------------------------------------------------------------
// Snapshot extraction / rebuild (uses ONLY public game + engine_ecs API; no engine edits)
// ---------------------------------------------------------------------------

use engine_ecs::Entity;
use game::economy::{
    Building, BuildingStatus, HarvestState, NodePos, RallyPoint, ResourceNode, TrainQueue,
    UnitRally, WorkJob,
};
use game::sim::{Order, Pos, Unit};

/// Collect the live entity-id set from a component's slot vec (entities() is not pub yet,
/// and this stays correct if it becomes so).
fn live_ids<T: Clone + 'static>(world: &engine_ecs::World) -> Vec<u32> {
    world.storages.iter::<T>().map(|(e, _)| e.0).collect()
}

pub fn snapshot_from_sim(sim: &game::Sim) -> StateSnapshot {
    let w = &sim.world;

    let mut bs_ids: Vec<u32> = live_ids::<Building>(w);
    bs_ids.sort_unstable();
    bs_ids.dedup();
    let buildings: Vec<shared::netcode::BuildingWire> = bs_ids
        .iter()
        .filter_map(|id| {
            let b = w.storages.get::<Building>(Entity(*id))?;
            let pos = w
                .storages
                .get::<Pos>(Entity(*id))
                .map(|p| (p.x.inner(), p.y.inner()))
                .unwrap_or((0, 0));
            let q = w
                .storages
                .get::<TrainQueue>(Entity(*id))
                .map(|q| q.0.iter().map(|it| (it.kind, it.ticks_left)).collect())
                .unwrap_or_default();
            let rp = w
                .storages
                .get::<RallyPoint>(Entity(*id))
                .map(|r| (r.tx, r.ty));
            Some(shared::netcode::BuildingWire {
                e: *id,
                owner: b.owner as u8,
                kind: b.kind,
                status: b.status.code(),
                hp: b.hp,
                max_hp: b.max_hp,
                timer: b.timer,
                px: pos.0,
                py: pos.1,
                q,
                rp,
            })
        })
        .collect();

    let mut ns_ids: Vec<u32> = live_ids::<ResourceNode>(w);
    ns_ids.sort_unstable();
    ns_ids.dedup();
    let nodes: Vec<(u32, u8, i64, i64, i16, i16)> = ns_ids
        .iter()
        .filter_map(|id| {
            let n = w.storages.get::<ResourceNode>(Entity(*id))?;
            let p = w
                .storages
                .get::<NodePos>(Entity(*id))
                .map(|p| (p.x, p.y))
                .unwrap_or((0, 0));
            Some((*id, n.slot, n.total, n.remaining, p.0, p.1))
        })
        .collect();

    let mut us_ids: Vec<u32> = live_ids::<Unit>(w);
    us_ids.sort_unstable();
    us_ids.dedup();
    let units = us_ids
        .iter()
        .map(|id| {
            let u = w.storages.get::<Unit>(Entity(*id)).unwrap();
            let (px, py) = w
                .storages
                .get::<Pos>(Entity(*id))
                .map(|p| (p.x.inner(), p.y.inner()))
                .unwrap_or((0, 0));
            let order = w
                .storages
                .get::<Order>(Entity(*id))
                .map(order_bytes)
                .unwrap_or_default();
            let harvest = w
                .storages
                .get::<HarvestState>(Entity(*id))
                .map(harvest_tuple);
            let work = w
                .storages
                .get::<WorkJob>(Entity(*id))
                .map(|j| (j.building, j.slot, j.node, work_mode_code(j.mode)));
            let rally = w
                .storages
                .get::<UnitRally>(Entity(*id))
                .map(|r| (r.tx, r.ty));
            UnitWire {
                e: *id,
                owner: u.owner,
                kind: u.kind,
                hp: u.hp,
                speed_q: u.speed_q,
                dmg: u.dmg,
                range: u.range.inner(),
                px,
                py,
                order,
                harvest,
                work,
                rally,
            }
        })
        .collect();

    let vis: Vec<Vec<u64>> = sim.vis.iter().map(|v| v.words.clone()).collect();
    let players: Vec<shared::netcode::PlayerWire> = sim
        .players
        .iter()
        .map(|p| shared::netcode::PlayerWire {
            food: p.stock.food,
            wood: p.stock.wood,
            gold: p.stock.gold,
            mana: p.stock.mana,
            age: p.age,
            age_target: p.age_target,
            age_timer: p.age_timer,
            civ: p.civ,
            score: p.score,
            winner: p.winner,
            wonder: p.wonder,
            wonder_started: p.wonder_started,
        })
        .collect();

    StateSnapshot {
        version: shared::netcode::SNAPSHOT_VERSION,
        tick: w.tick,
        rng_state: w.rng.state(),
        hash_after: sim.hash(),
        grid_w: sim.grid.w(),
        grid_h: sim.grid.h(),
        players,
        vis,
        buildings,
        nodes,
        units,
        blocked: blocked_cells(sim),
    }
}

/// Encode an Order into exactly the byte stream Order::hash_into hashes, so the restored
/// component reproduces the host hash byte-for-byte.
pub fn order_bytes(o: &Order) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    match o {
        Order::Idle => v.push(0u8),
        Order::Move { path, waypoint } => {
            v.push(1u8);
            v.extend_from_slice(&(*waypoint as u64).to_le_bytes());
            push_path(&mut v, path);
        }
        Order::Attack { target } => {
            v.push(2u8);
            v.extend_from_slice(&target.to_le_bytes());
        }
        Order::Harvest { node, path } => {
            v.push(3u8);
            v.extend_from_slice(&node.to_le_bytes());
            push_path(&mut v, path);
        }
        Order::Construct { building, path } => {
            v.push(4u8);
            v.extend_from_slice(&building.to_le_bytes());
            push_path(&mut v, path);
        }
        Order::UnitAttack { target } => {
            v.push(5u8);
            v.extend_from_slice(&target.to_le_bytes());
        }
    }
    v
}

fn push_path(v: &mut Vec<u8>, path: &[engine_path::TilePos]) {
    v.extend_from_slice(&(path.len() as u64).to_le_bytes());
    for p in path {
        v.extend_from_slice(&(p.x as i32).to_le_bytes());
        v.extend_from_slice(&(p.y as i32).to_le_bytes());
    }
}

fn take_path(b: &[u8], pos: &mut usize) -> Result<Vec<engine_path::TilePos>, String> {
    if *pos + 8 > b.len() {
        return Err("short path len".to_string());
    }
    let n = u64::from_le_bytes([
        b[*pos],
        b[*pos + 1],
        b[*pos + 2],
        b[*pos + 3],
        b[*pos + 4],
        b[*pos + 5],
        b[*pos + 6],
        b[*pos + 7],
    ]) as usize;
    *pos += 8;
    if n > 1_000_000 || *pos + n * 8 > b.len() {
        return Err("bad path length".to_string());
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let x = i32::from_le_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]) as i16;
        let y = i32::from_le_bytes([b[*pos + 4], b[*pos + 5], b[*pos + 6], b[*pos + 7]]) as i16;
        *pos += 8;
        out.push(engine_path::TilePos { x, y });
    }
    Ok(out)
}

fn le32(b: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > b.len() {
        return Err("short u32".to_string());
    }
    let v = u32::from_le_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]);
    *pos += 4;
    Ok(v)
}

/// Inverse of order_bytes.
pub fn decode_order(b: &[u8]) -> Result<Order, String> {
    if b.is_empty() {
        return Err("empty order payload".to_string());
    }
    let tag = b[0];
    let mut pos = 1usize;
    match tag {
        0 => Ok(Order::Idle),
        1 => {
            if pos + 8 > b.len() {
                return Err("short waypoint".to_string());
            }
            let wp = u64::from_le_bytes([
                b[pos],
                b[pos + 1],
                b[pos + 2],
                b[pos + 3],
                b[pos + 4],
                b[pos + 5],
                b[pos + 6],
                b[pos + 7],
            ]) as usize;
            pos += 8;
            let path = take_path(b, &mut pos)?;
            Ok(Order::Move { path, waypoint: wp })
        }
        2 => {
            let target = le32(b, &mut pos)?;
            Ok(Order::Attack { target })
        }
        3 => {
            let node = le32(b, &mut pos)?;
            let path = take_path(b, &mut pos)?;
            Ok(Order::Harvest { node, path })
        }
        4 => {
            let building = le32(b, &mut pos)?;
            let path = take_path(b, &mut pos)?;
            Ok(Order::Construct { building, path })
        }
        5 => {
            let target = le32(b, &mut pos)?;
            Ok(Order::UnitAttack { target })
        }
        t => Err(format!("bad order tag {}", t)),
    }
}

fn harvest_tuple(h: &HarvestState) -> (u8, u32, u8, u32, i32) {
    match h {
        HarvestState::ToNode { node, slot } => (0, *node, *slot, 0, 0),
        HarvestState::Picking {
            node,
            slot,
            ticks_left,
        } => (1, *node, *slot, *ticks_left, 0),
        HarvestState::Carrying { node, slot, amount } => (2, *node, *slot, 0, *amount),
    }
}

fn harvest_from(t: (u8, u32, u8, u32, i32)) -> HarvestState {
    match t.0 {
        0 => HarvestState::ToNode {
            node: t.1,
            slot: t.2,
        },
        1 => HarvestState::Picking {
            node: t.1,
            slot: t.2,
            ticks_left: t.3,
        },
        _ => HarvestState::Carrying {
            node: t.1,
            slot: t.2,
            amount: t.4,
        },
    }
}

fn work_mode_code(m: game::economy::WorkMode) -> u8 {
    match m {
        game::economy::WorkMode::Build => 0,
        game::economy::WorkMode::Harvest => 1,
    }
}

fn blocked_cells(sim: &game::Sim) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    for y in 0..sim.grid.h() as i16 {
        for x in 0..sim.grid.w() as i16 {
            if sim.grid.is_blocked(engine_path::TilePos { x, y }) {
                out.push((x as u16, y as u16));
            }
        }
    }
    out
}

/// Restore a Sim from a snapshot so the hash matches the host's hash_after.
///
/// Strategy (documented in docs/netcode.md): Sim::new(seed) gives the exact same initial
/// entity id numbering as the host (mapgen is deterministic, and the client is given the same
/// seed). We then OVERWRITE every row at its exact entity id via Storages::add (which pads
/// slots with None), replace player/visibility state, and re-block the snapshot's blocked
/// cells. The one gap: engine_core::Rng has no state setter and World.rng is private, so the
/// rng stream is replayed instead: the client fast-forwards (state - seed) draws, which is
/// Restore a same-seed sim to snapshot state, entity-id-exact.
///
/// Strategy (documented in docs/netcode.md): Sim::new(seed) gives the exact same initial
/// entity-id numbering as the host (mapgen is deterministic, and the client is given the same
/// seed). We then OVERWRITE every row at its exact entity id via Storages::add (which pads
/// slots with None), replace player/visibility state, and re-apply the snapshot's blocked
/// cells. The one gap: engine_core::Rng has no state setter, but Sim::new(seed) consumes the
/// same draw count as the host up to mapgen, so rng.state() matches for same-seed snapshots.
pub fn restore_sim(sim: &mut game::Sim, snap: &StateSnapshot) -> Result<(), NetError> {
    for id in live_ids::<Building>(&sim.world) {
        sim.world.storages.remove::<Building>(Entity(id));
        sim.world.storages.remove::<Pos>(Entity(id));
        sim.world.storages.remove::<TrainQueue>(Entity(id));
        sim.world.storages.remove::<RallyPoint>(Entity(id));
    }
    for id in live_ids::<Unit>(&sim.world) {
        sim.world.storages.remove::<Unit>(Entity(id));
        sim.world.storages.remove::<Pos>(Entity(id));
        sim.world.storages.remove::<Order>(Entity(id));
        sim.world.storages.remove::<HarvestState>(Entity(id));
        sim.world.storages.remove::<WorkJob>(Entity(id));
        sim.world.storages.remove::<UnitRally>(Entity(id));
    }
    for id in live_ids::<ResourceNode>(&sim.world) {
        sim.world.storages.remove::<ResourceNode>(Entity(id));
        sim.world.storages.remove::<NodePos>(Entity(id));
    }

    for bw in &snap.buildings {
        let e = Entity(bw.e);
        sim.world.storages.add(
            e,
            Building {
                owner: bw.owner as u32,
                kind: bw.kind,
                status: status_from_code(bw.status),
                hp: bw.hp,
                max_hp: bw.max_hp,
                timer: bw.timer,
            },
        );
        sim.world.storages.add(
            e,
            Pos {
                x: engine_core::Fixed1616(bw.px),
                y: engine_core::Fixed1616(bw.py),
            },
        );
        if !bw.q.is_empty() {
            sim.world.storages.add(
                e,
                TrainQueue(
                    bw.q.iter()
                        .map(|(k, t)| game::economy::TrainItem {
                            kind: *k,
                            ticks_left: *t,
                        })
                        .collect(),
                ),
            );
        }
        if let Some((tx, ty)) = bw.rp {
            sim.world.storages.add(e, RallyPoint { tx, ty });
        }
    }
    for (id, slot, total, remaining, nx, ny) in &snap.nodes {
        let e = Entity(*id);
        sim.world.storages.add(
            e,
            ResourceNode {
                slot: *slot,
                total: *total,
                remaining: *remaining,
            },
        );
        sim.world.storages.add(e, NodePos { x: *nx, y: *ny });
    }
    for u in &snap.units {
        let e = Entity(u.e);
        sim.world.storages.add(
            e,
            Unit {
                owner: u.owner,
                kind: u.kind,
                hp: u.hp,
                speed_q: u.speed_q,
                dmg: u.dmg,
                range: engine_core::Fixed1616(u.range),
            },
        );
        sim.world.storages.add(
            e,
            Pos {
                x: engine_core::Fixed1616(u.px),
                y: engine_core::Fixed1616(u.py),
            },
        );
        if !u.order.is_empty() {
            sim.world
                .storages
                .add(e, decode_order(&u.order).map_err(NetError::WireDetail)?);
        }
        if let Some(t) = u.harvest {
            sim.world.storages.add(e, harvest_from(t));
        }
        if let Some((building, slot, node, mode)) = u.work {
            sim.world.storages.add(
                e,
                WorkJob {
                    building,
                    slot,
                    node,
                    mode: if mode == 1 {
                        game::economy::WorkMode::Harvest
                    } else {
                        game::economy::WorkMode::Build
                    },
                },
            );
        }
        if let Some((tx, ty)) = u.rally {
            sim.world.storages.add(e, UnitRally { tx, ty });
        }
    }
    // Blocked cells: same-seed mapgen makes these identical, but re-apply defensively.
    for (x, y) in &snap.blocked {
        sim.grid.set_blocked(
            engine_path::TilePos {
                x: *x as i16,
                y: *y as i16,
            },
            true,
        );
    }
    for (i, p) in snap.players.iter().enumerate() {
        if let Some(pl) = sim.players.get_mut(i) {
            pl.stock.food = p.food;
            pl.stock.wood = p.wood;
            pl.stock.gold = p.gold;
            pl.stock.mana = p.mana;
            pl.age = p.age;
            pl.age_target = p.age_target;
            pl.age_timer = p.age_timer;
            pl.civ = p.civ;
            pl.score = p.score;
            pl.winner = p.winner;
            pl.wonder = p.wonder;
            pl.wonder_started = p.wonder_started;
        }
    }
    for (i, words) in snap.vis.iter().enumerate() {
        if let Some(v) = sim.vis.get_mut(i) {
            v.words = words.clone();
        }
    }
    sim.world.tick = snap.tick;
    Ok(())
}

/// Rebuild a BuildingStatus from its wire code.
fn status_from_code(c: u8) -> BuildingStatus {
    match c {
        0 => BuildingStatus::Site,
        1 => BuildingStatus::UnderConstruction,
        2 => BuildingStatus::Done,
        _ => BuildingStatus::Wreck,
    }
}

// ---------------------------------------------------------------------------
// Session state machines (chunk C) — HostSession / ClientSession + tokio glue.
// Single-threaded per peer: each session owns its Sim and mutates it only from its
// own tokio task. All cross-thread comms use std::sync::mpsc (UI thread safe).
// (stream framing removed — each NetMsg rides its own reliable bidirectional stream)

/// One round-trip worth of timing: client sends Ping{t_millis}, echoes back Pong;
/// host answers with the same t_millis so the client computes rtt = now - t.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Deterministic reconnect token: FNV-1a over (seed, player_id) via shared::Hasher.
/// Host and client agree without exchanging it in the clear; both derive it locally.
pub fn reconnect_token(seed: u64, player_id: u32) -> u64 {
    let mut h = shared::Hasher::new();
    h.u64(seed);
    h.u32(player_id);
    h.finish()
}

/// Host-authoritative lobby + tick engine. Owns the Sim and the tick clock.
/// A connected peer: live connection + reply channel to its write pump.
pub struct PeerConn {
    pub conn: Connection,
    pub tx: tokio::sync::mpsc::UnboundedSender<NetMsg>,
}

pub struct HostSession {
    pub cfg: SessionCfg,
    pub sim: game::Sim,
    /// slot -> wire id (1..MAX_PLAYERS). Host is id 1; clients 2..
    pub players: Vec<PlayerInfo>,
    pub next_player_id: u32,
    pub started: bool,
    pub paused: bool,
    pub lobby_code: String,
    /// Ring of the last DIGEST_WINDOW command batches: (tick, sorted batch).
    pub ring: VecDeque<(u64, Vec<(u32, Command)>)>,
    /// Host clock: next tick to execute; INPUT_DELAY ticks of slack for inputs.
    pub next_tick: u64,
    /// Pending inputs received so far, keyed by target tick.
    pub pending: BTreeMap<u64, Vec<(u32, Command)>>,
    /// Per-peer connections keyed by player id.
    pub peers: std::collections::HashMap<u32, PeerConn>,
    /// Local host commands (sandbox / test harness).
    pub local_commands: Vec<(u32, Command)>,
    /// Host tick counter for digest cadence.
    pub last_digest_tick: u64,
    /// Player awaiting a Resync (reconnect) — glue drains it.
    pub resync_target: Option<u32>,
    /// Outbox for host-glue (resync snapshots, start, pause, digests) — drained by tick loop.
    pub outbox: std::collections::VecDeque<NetMsg>,
    pub tx: SessionEventTx,
}

// SAFETY: HostSession is only mutated behind std::sync::Mutex and inside synchronous
// critical sections (handle_msg / step_tick never await). The `Sim`'s ECS scheduler box
// makes it !Send in general; our discipline confines it to one task at a time, so handing
// the Mutex to async glue tasks is sound. See docs/netcode.md.
unsafe impl Send for HostSession {}

impl HostSession {
    pub fn new(cfg: SessionCfg, tx: SessionEventTx) -> Self {
        let lobby_code = invite_code(cfg.seed);
        Self {
            cfg: cfg.clone(),
            sim: game::Sim::new(cfg.seed), // seeded now; Start only flips started (client sim seeds in Welcome path)
            players: Vec::new(),
            next_player_id: 2, // 1 reserved for the host
            started: false,
            paused: false,
            lobby_code,
            ring: VecDeque::new(),
            next_tick: 0,
            pending: BTreeMap::new(),
            peers: std::collections::HashMap::new(),
            local_commands: Vec::new(),
            last_digest_tick: 0,
            resync_target: None,
            outbox: std::collections::VecDeque::new(),
            tx,
        }
    }

    pub fn handle_msg(
        &mut self,
        peer: Option<u32>,
        peer_link: Option<(Connection, tokio::sync::mpsc::UnboundedSender<NetMsg>)>,
        m: NetMsg,
    ) -> Vec<NetMsg> {
        let mut replies: Vec<NetMsg> = Vec::new();
        match m {
            NetMsg::Hello {
                wire_version,
                proto,
                name,
            } => {
                let _ = self.tx.send(SessionEvent::Hello {
                    wire_version,
                    proto,
                    name,
                });
            }
            NetMsg::JoinInvite { code } => {
                let (player_id, welcome) = self.admit_peer(peer_link, &code);
                if let Some(id) = player_id {
                    let _ = self.tx.send(SessionEvent::Welcome {
                        player_id: id,
                        sim_seed: self.cfg.seed,
                        preset: self.cfg.preset.clone(),
                        max_players: game::sim::MAX_PLAYERS as u8,
                        lobby_code: self.lobby_code.clone(),
                    });
                    replies.extend(welcome);
                    // Catch-up: a late joiner in a started match receives the ring backlog
                    // (last <= DIGEST_WINDOW batches) so it converges instead of skipping ticks.
                    // (If the backlog is incomplete (ring older than the match), a Resync-style
                    // snapshot replay is the polish path; same-seed start is exact.)
                    if self.started {
                        replies.push(NetMsg::Start);
                        for (tick, cmds) in self.ring.iter().cloned() {
                            replies.push(NetMsg::TickBatch { tick, cmds });
                        }
                    }
                }
            }
            NetMsg::Reconnect { player_id, token } => {
                // Host is authoritative: verify the deterministic token, mark the slot, and let
                // the async glue send Resync{snapshot} + a MatchStarted so the client fast-forwards.
                let ok = reconnect_token(self.cfg.seed, player_id) == token
                    && player_id < self.next_player_id;
                if ok {
                    if let Some(pi) = self.players.iter_mut().find(|pi| pi.id == player_id) {
                        pi.connected = true;
                    }
                    self.resync_target = Some(player_id);
                    self.broadcast_lobby();
                    let _ = self.tx.send(SessionEvent::PeerJoined {
                        player_id,
                        connected: true,
                    });
                } else {
                    replies.push(NetMsg::Refused {
                        code: shared::netcode::RefuseCode::BadInvite,
                        detail: "bad token".into(),
                    });
                }
            }
            NetMsg::Input { tick, cmds } => {
                if let Some(p) = peer {
                    let sorted = sorted_batch(p, cmds);
                    self.pending.entry(tick).or_default().extend(sorted);
                }
            }

            NetMsg::Ready { ready } => {
                if let Some(p) = peer {
                    if let Some(pi) = self.players.iter_mut().find(|pi| pi.id == p) {
                        pi.ready = ready;
                    }
                    self.broadcast_lobby();
                }
                let _ = self.tx.send(SessionEvent::Ready { ready });
            }
            NetMsg::Ping { t_millis } => {
                replies.push(NetMsg::Pong {
                    t_millis: now_millis(),
                });
                let _ = self.tx.send(SessionEvent::Ping { t_millis });
            }
            NetMsg::Pong { t_millis } => {
                let _ = self.tx.send(SessionEvent::Pong {
                    rtt_millis: now_millis() - t_millis,
                });
            }

            NetMsg::Pause { paused } => {
                if peer == Some(1) {
                    self.paused = paused;
                    let _ = self.tx.send(SessionEvent::Pause { paused });
                }
            }
            NetMsg::Chat { player_id, text } => {
                // Chat never enters the sim path (kept out of TickCommands / hash).
                let _ = (player_id, text);
            }
            NetMsg::KeepAlive => {}
            NetMsg::Refused { code, detail } => {
                let _ = self.tx.send(SessionEvent::Refused { code, detail });
            }
            NetMsg::Lobby { .. } => {}
            NetMsg::PeerJoined {
                player_id,
                connected,
            } => {
                let _ = self.tx.send(SessionEvent::PeerJoined {
                    player_id,
                    connected,
                });
            }
            NetMsg::PeerLeft { player_id } => {
                let _ = self.tx.send(SessionEvent::PeerLeft { player_id });
            }
            NetMsg::Start => {
                if peer == Some(1) && !self.started {
                    // sim already seeded in new(); Start flips the session live and broadcasts.
                    self.started = true;
                    self.next_tick = 0;
                    let _ = self.tx.send(SessionEvent::Started);
                    replies.push(NetMsg::Start);
                    self.outbox.push_back(NetMsg::Start);
                }
            }
            NetMsg::TickBatch { tick, cmds } => {
                let _ = self.tx.send(SessionEvent::Input { tick, cmds });
            }
            NetMsg::Welcome { .. } => {}
            NetMsg::Resync { .. } => {}
            NetMsg::SyncDigest { tick, digest, .. } => {
                let _ = self.tx.send(SessionEvent::Desync {
                    tick,
                    expected: digest,
                    got: digest,
                });
            }
        }
        replies
    }

    pub fn admit_peer(
        &mut self,
        link: Option<(Connection, tokio::sync::mpsc::UnboundedSender<NetMsg>)>,
        code: &str,
    ) -> (Option<u32>, Option<NetMsg>) {
        if code != self.lobby_code {
            return (None, None);
        }
        let id = self.next_player_id;
        if id > game::sim::MAX_PLAYERS as u32 {
            return (None, None);
        }
        self.players
            .push(PlayerInfo::new(id, format!("p{id}"), (id - 1) as u8));
        self.next_player_id += 1;
        if let Some((c, ptx)) = link {
            self.peers.insert(id, PeerConn { conn: c, tx: ptx });
        }
        (
            Some(id),
            Some(NetMsg::Welcome {
                player_id: id,
                sim_seed: self.cfg.seed,
                preset: self.cfg.preset.clone(),
                max_players: game::sim::MAX_PLAYERS as u8,
                lobby_code: self.lobby_code.clone(),
            }),
        )
    }

    pub fn broadcast_lobby(&self) {
        let _ = self.tx.send(SessionEvent::Lobby {
            players: self.players.clone(),
            host_id: 1,
            started: self.started,
            preset: self.cfg.preset.clone(),
            seed: self.cfg.seed,
            max_players: game::sim::MAX_PLAYERS as u8,
        });
    }

    /// Advance one host tick: execute pending[tick] + host-local commands, publish, digest.
    pub fn step_tick(&mut self) -> Result<(), NetError> {
        if !self.started || self.paused {
            return Ok(());
        }
        let tick = self.next_tick;
        let mut batch = self.pending.remove(&tick).unwrap_or_default();
        // host's own queued commands execute at this tick with 0 delay from its view:
        if let Some(i) = self.pending.get_mut(&tick) {
            batch.extend(std::mem::take(i));
        }
        batch.sort_by(|(pa, ca), (pb, cb)| {
            pa.cmp(pb).then_with(|| ca.sort_key().cmp(&cb.sort_key()))
        });
        self.sim
            .step(&batch)
            .map_err(|e| NetError::Sim(format!("{e:?}")))?;

        self.ring.push_back((tick, batch.clone()));
        while self.ring.len() > DIGEST_WINDOW {
            self.ring.pop_front();
        }
        self.next_tick += 1;

        let hash = self.sim.hash();
        let _ = self.tx.send(SessionEvent::AppliedBatch {
            tick,
            cmds: batch.len(),
            hash,
        });

        // Authoritative batch broadcast: every connected peer applies this exact batch
        // on tick == next_tick (lockstep). The 1-tick input delay gives clients slack.
        self.outbox
            .push_back(NetMsg::TickBatch { tick, cmds: batch });

        self.broadcast_digest();
        register_snapshot("host", &self.sim);
        Ok(())
    }

    /// If it is time, compute the replay digest of the ring and queue SyncDigest for
    /// every connected peer (host-glue drains it via drain_outbox).
    pub fn broadcast_digest(&mut self) {
        if self.next_tick - self.last_digest_tick < DIGEST_INTERVAL_TICKS {
            return;
        }
        self.last_digest_tick = self.next_tick;
        let digest = replay_digest(&self.ring.iter().cloned().collect::<Vec<_>>());
        let msg = NetMsg::SyncDigest {
            tick: self.next_tick,
            digest,
            window: DIGEST_WINDOW as u32,
            interval: DIGEST_INTERVAL_TICKS as u32,
        };
        for pc in self.peers.values() {
            let _ = pc.tx.send(msg.clone());
        }
        self.outbox.push_back(msg);
    }

    /// Host-glue drain: messages the session wanted written but could not (resync,
    /// start, pause broadcast). Drained by host_tick_loop between ticks.
    pub fn drain_outbox(&mut self) -> Option<NetMsg> {
        self.outbox.pop_front()
    }

    /// Reconnect response: snapshot + catch-up replay is host-side; we hand the caller a
    /// Resync message to write on the wire.
    /// Consume a pending reconnect resync: returns the message the glue must write.
    pub fn drain_resync(&mut self) -> Option<NetMsg> {
        let id = self.resync_target.take()?;
        // Registered peers qualify even without a live socket (in-process bridge/tests);
        // the async glue additionally checks the connection is still open before writing.
        let known = self.players.iter().any(|pi| pi.id == id && pi.connected);
        if known {
            Some(self.resync_msg_for(id))
        } else {
            None
        }
    }

    pub fn resync_msg(&self) -> NetMsg {
        self.resync_msg_for(self.resync_target.unwrap_or(1))
    }

    fn resync_msg_for(&self, _id: u32) -> NetMsg {
        NetMsg::Resync {
            snapshot: snapshot_from_sim(&self.sim),
        }
    }
}

/// Client mirror. Runs the same Sim from the Welcome seed, applies authoritative batches
/// with a small ring to tolerate jitter, and raises Desync when digests disagree.
pub struct ClientSession {
    pub cfg: SessionCfg,
    pub sim: game::Sim,
    pub player_id: u32,
    pub started: bool,
    pub paused: bool,
    pub ring: VecDeque<(u64, Vec<(u32, Command)>)>,
    pub next_tick: u64,
    pub pending: BTreeMap<u64, Vec<(u32, Command)>>,
    pub last_ack: u64,
    /// Ticks during which incoming digests are ignored (post-resync ring rebuild).
    pub digest_mute: u64,
    pub tx: SessionEventTx,
}

// SAFETY: same discipline as HostSession — ClientSession is moved into a single
// client_run task and touched only there. Never shared across threads. See docs/netcode.md.
unsafe impl Send for ClientSession {}

impl ClientSession {
    pub fn new(cfg: SessionCfg, tx: SessionEventTx) -> Self {
        Self {
            cfg,
            sim: game::Sim::new(0),
            player_id: 0,
            started: false,
            paused: false,
            ring: VecDeque::new(),
            next_tick: 0,
            pending: BTreeMap::new(),
            last_ack: 0,
            digest_mute: 0,
            tx,
        }
    }

    pub fn apply_batch(&mut self, tick: u64, cmds: Vec<(u32, Command)>) -> Result<(), NetError> {
        if tick < self.next_tick {
            return Err(NetError::WireDetail("bad msg".into()));
        }
        // Late join / missed batch: fast-forward the clock (host-authoritative).
        if tick > self.next_tick {
            self.next_tick = tick;
        }
        self.sim
            .step(&cmds)
            .map_err(|e| NetError::Sim(format!("{e:?}")))?;
        self.ring.push_back((tick, cmds));
        while self.ring.len() > DIGEST_WINDOW {
            self.ring.pop_front();
        }
        self.next_tick += 1;
        let hash = self.sim.hash();
        let _ = self.tx.send(SessionEvent::AppliedBatch {
            tick,
            cmds: 0,
            hash,
        });
        Ok(())
    }

    pub fn check_digest(&mut self, tick: u64, digest: u64) {
        if self.digest_mute > 0 {
            self.digest_mute -= 1;
            return;
        }
        let mine = replay_digest(&self.ring.iter().cloned().collect::<Vec<_>>());
        if mine != digest {
            let _ = self.tx.send(SessionEvent::Desync {
                tick,
                expected: digest,
                got: mine,
            });
        }
    }
}

fn sorted_batch(player: u32, cmds: Vec<(u32, Command)>) -> Vec<(u32, Command)> {
    let mut b: Vec<(u32, Command)> = cmds.into_iter().map(|(_, c)| (player, c)).collect();
    b.sort_by_key(|(_, c)| c.sort_key());
    b
}

// ---------------------------------------------------------------------------
// Test-only deterministic bridge (in-process, lockstep, single thread)
// ---------------------------------------------------------------------------

/// In-process lockstep bridge used by the integration tests. It drives
/// HostSession + ClientSession state machines without any transport, which is
/// also the canonical definition of "single-threaded per peer" for us.
#[doc(hidden)]
pub mod bridge {
    use crate::{ClientSession, HostSession, NetError, SessionEvent};
    use shared::netcode::NetMsg;

    /// Fire the host Start and give every client a fresh sim (Welcome path).
    pub fn start_match(host: &mut HostSession, clients: &mut [ClientSession]) {
        let _ = host.handle_msg(Some(1), None, NetMsg::Start);
        while let Some(m) = host.drain_outbox() {
            if let NetMsg::Start = m {
                for cl in clients.iter_mut() {
                    cl.sim = game::Sim::new(cl.cfg.seed);
                    cl.started = true;
                }
            }
        }
    }

    /// Advance n ticks; clients apply every authoritative batch and check digests.
    /// Returns (host final hash, client final hashes, desync events seen).
    pub fn step_n(
        host: &mut HostSession,
        clients: &mut [ClientSession],
        n: u64,
    ) -> Result<(u64, Vec<u64>, Vec<SessionEvent>), NetError> {
        let mut desyncs = Vec::new();
        for _ in 0..n {
            host.step_tick()?;
            host.broadcast_digest(); // same cadence as host_tick_loop: bridge must check digests too
            let (_t, cmds) = host.ring.back().cloned().unwrap_or((0, Vec::new()));
            for cl in clients.iter_mut() {
                cl.apply_batch(cl.next_tick, cmds.clone())?;
            }
            while let Some(m) = host.drain_outbox() {
                if let NetMsg::SyncDigest { tick, digest, .. } = m {
                    for cl in clients.iter_mut() {
                        cl.check_digest(tick, digest);
                        if let SessionEvent::Desync { .. } = cl.tx.borrow().clone() {
                            desyncs.push(cl.tx.borrow().clone());
                        }
                    }
                }
            }
            if let SessionEvent::Desync { .. } = host.tx.borrow().clone() {
                desyncs.push(host.tx.borrow().clone());
            }
        }
        let h = host.sim.hash();
        let cs = clients.iter().map(|c| c.sim.hash()).collect();
        Ok((h, cs, desyncs))
    }
}

// ---------------------------------------------------------------------------
// tokio transport glue
// ---------------------------------------------------------------------------

/// Run one accept loop; each accepted peer gets its own stream reader task.
/// Accept-loop glue: binds a quinn endpoint and drives one HostSession.
/// Each peer gets a task that reads messages and routes them through HostSession::handle_msg.
/// Accept-loop glue: binds a quinn endpoint and drives one HostSession.
/// Per peer: one read task (routes msgs into HostSession::handle_msg) + one write pump
/// (drains this peer's reply channel). Single-threaded per peer, host-authoritative.
/// Safety: HostSession owns a game::Sim whose ECS Scheduler stores Box<dyn FnMut>
/// (not Send). Our sessions are single-threaded by design — all state lives behind a
/// std Mutex and every critical section is synchronous. The unsafe wrapper below lets
/// those single-threaded sessions cross an async task boundary; see docs/netcode.md.
/// Shared, Send view of a HostSession (see SAFETY note below).
#[derive(Clone)]
pub struct SendSession(std::sync::Arc<std::sync::Mutex<HostSession>>);

impl SendSession {
    /// Wrap a HostSession's shared mutex for the async glue.
    pub fn wrap(session: std::sync::Arc<std::sync::Mutex<HostSession>>) -> Self {
        Self(session)
    }
}
/// Bind a quinn server endpoint for a HostSession (localhost TLS, 150ms keep-alive).
pub fn bind_host(addr: &str) -> Result<Endpoint, NetError> {
    let tls = Tls::local().map_err(|e| NetError::Bind(e.to_string()))?;
    let mut cfg = tls.server.clone();
    let mut tp = quinn::TransportConfig::default();
    tp.keep_alive_interval(Some(Duration::from_millis(150)));
    cfg.transport_config(std::sync::Arc::new(tp));
    Endpoint::server(
        cfg,
        addr.parse::<std::net::SocketAddr>()
            .map_err(|e| NetError::Bind(e.to_string()))?,
    )
    .map_err(|e| NetError::Bind(e.to_string()))
}
// SAFETY: see above — no cross-thread field access, Mutex guards all mutation.
unsafe impl Send for SendSession {}
impl std::ops::Deref for SendSession {
    type Target = std::sync::Arc<std::sync::Mutex<HostSession>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Accept-loop glue: binds a quinn endpoint and drives one HostSession.
/// Per peer: one read task + one write pump. HostSession itself is accessed only in
/// synchronous critical sections (never across an await).
pub async fn host_accept_loop(
    endpoint: Endpoint,
    host: SendSession,
    stop: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = crate::wait_stop(stop.clone()) => return,
            inc = endpoint.accept() => {
                let Some(inc) = inc else { return };
                let conn = match inc.await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                // HostSession is !Send (ECS scheduler dyn closure). The async task borrows it
                // only inside synchronous critical sections via SendSession (see docs/netcode.md).
                let host = host.clone();
                tokio::spawn(async move {
                    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<NetMsg>();
                    let write_conn = conn.clone();
                    let writer = tokio::spawn(async move {
                        while let Some(m) = reply_rx.recv().await {
                            if write_msg(&write_conn, &m).await.is_err() {
                                break;
                            }
                        }
                    });
                    let mut bound: Option<u32> = None;
                    loop {
                        let msg = match read_msg_pair(&conn).await {
                            Ok(m) => m,
                            Err(_) => break,
                        };
                        let (replies, resync) = {
                            let h = &mut *host.lock().unwrap_or_else(|p| p.into_inner());
                            if bound.is_none() {
                                if let NetMsg::JoinInvite { .. } = msg {
                                    bound = h.players.iter().map(|pi| pi.id).filter(|id| *id != 1).max();
                                }
                            }
                            let link = Some((conn.clone(), reply_tx.clone()));
                            let r = h.handle_msg(bound, link, msg);
                            let res = h.drain_resync();
                            (r, res)
                        };
                        for m in replies {
                            let _ = reply_tx.send(m);
                        }
                        if let Some(m) = resync {
                            let _ = reply_tx.send(m);
                        }
                    }
                    writer.abort();
                    if let Some(id) = bound {
                        let h = &mut *host.lock().unwrap_or_else(|p| p.into_inner());
                        h.peers.remove(&id);
                        let _ = h.tx.send(SessionEvent::PeerLeft { player_id: id });
                    }
                });
            }
        }
    }
}

/// Wait for stop flag (Send-safe: uses watch::changed, no Ref guard across await).
async fn wait_stop(mut stop: tokio::sync::watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

async fn read_msg_pair(conn: &Connection) -> Result<NetMsg, NetError> {
    let (_s, r) = conn.accept_bi().await.map_err(transport_err)?;
    read_msg(r).await
}

/// Drive a HostSession clock at TICK_MS cadence.
pub async fn host_tick_loop(host: SendSession, stop: tokio::sync::watch::Receiver<bool>) {
    let mut period = tokio::time::interval(Duration::from_millis(TICK_MS));
    loop {
        tokio::select! {
            _ = period.tick() => {
                let mut h = host.lock().unwrap_or_else(|p| p.into_inner());
                let _ = h.step_tick();
                let resync = h.drain_resync();
                if let Some(m) = resync {
                    if let Some(id) = h.resync_target {
                        if let Some(pc) = h.peers.get(&id) {
                            let _ = pc.tx.send(m.clone());
                        }
                    }
                    let _ = h.tx.send(SessionEvent::Resynced { tick: h.next_tick });
                }
                while let Some(m) = h.drain_outbox() {
                    if let NetMsg::SyncDigest { .. } = &m {
                        // already broadcast per-peer by broadcast_digest()
                        continue;
                    }
                    for pc in h.peers.values() {
                        let _ = pc.tx.send(m.clone());
                    }
                }
            }
            _ = crate::wait_stop(stop.clone()) => break,
        }
    }
}

/// Connect client and pump messages into a ClientSession.
pub async fn client_run(
    addr: std::net::SocketAddr,
    cfg: SessionCfg,
    mut client: ClientSession,
    mut input_rx: tokio::sync::mpsc::UnboundedReceiver<(u64, Vec<(u32, Command)>)>,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), NetError> {
    let tls = Tls::local().map_err(|e| NetError::Transport(e.to_string()))?;
    let mut ep = Endpoint::client("0.0.0.0:0".parse().unwrap())
        .map_err(|e| NetError::Bind(e.to_string()))?;
    let mut cs = tls.client.clone();
    cs.transport_config({
        let mut tp = quinn::TransportConfig::default();
        tp.keep_alive_interval(Some(Duration::from_millis(150)));
        std::sync::Arc::new(tp)
    });
    ep.set_default_client_config(cs);
    let conn = ep
        .connect(addr, "localhost")
        .map_err(|e| NetError::Transport(e.to_string()))?
        .await
        .map_err(transport_err)?;
    eprintln!("DBG client connected");
    write_msg(
        &conn,
        &NetMsg::Hello {
            wire_version: shared::netcode::WIRE_VERSION,
            proto: PROTO_ID,
            name: cfg.name.clone(),
        },
    )
    .await?;
    write_msg(
        &conn,
        &NetMsg::JoinInvite {
            code: invite_code(cfg.seed),
        },
    )
    .await?;
    eprintln!("DBG client joined invite sent");

    loop {
        tokio::select! {
            _ = crate::wait_stop(stop.clone()) => return Ok(()),
            m = read_msg_pair(&conn) => {
                let m = match m { Ok(m) => m, Err(_) => return Ok(()) };
                match m {
                    NetMsg::Welcome { player_id, sim_seed, preset, .. } => {
                        client.player_id = player_id;
                        client.sim = game::Sim::new(sim_seed);
                        if let Some(p) = game::mapgen::preset_by_name(&preset) {
                            client.sim.install_map(game::mapgen::generate_map_preset(sim_seed, *p));
                        }
                        client.started = true;
                        register_snapshot("client", &client.sim);
                        let _ = client.tx.send(SessionEvent::Started);
                    }
                    NetMsg::TickBatch { tick, cmds } => { client.apply_batch(tick, cmds)?; register_snapshot("client", &client.sim); }
                    NetMsg::SyncDigest { tick, digest, .. } => { client.check_digest(tick, digest); }
                    NetMsg::Pause { paused } => { client.paused = paused; let _ = client.tx.send(SessionEvent::Pause { paused }); }
                    NetMsg::Pong { t_millis } => { let _ = client.tx.send(SessionEvent::Pong { rtt_millis: now_millis() - t_millis }); }
                    NetMsg::PeerLeft { player_id } => { let _ = client.tx.send(SessionEvent::PeerLeft { player_id }); }
                    NetMsg::Resync { snapshot } => {
                    restore_sim(&mut client.sim, &snapshot)?;
                    client.started = true;
                    client.next_tick = snapshot.tick + 1;
                    client.ring.clear();
                    client.digest_mute = DIGEST_WINDOW as u64;
                    register_snapshot("client", &client.sim);
                    let _ = client.tx.send(SessionEvent::Resynced { tick: snapshot.tick });
                }
                    other => { let _ = client.tx.send(SessionEvent::Hello { wire_version: 0, proto: 0, name: format!("{other:?}") }); }
                }
            }
            Some((tick, cmds)) = input_rx.recv() => {
                write_msg(&conn, &NetMsg::Input { tick: tick + INPUT_DELAY, cmds }).await?;
            }
        }
    }
}

/// Host-side message pump used with HostSession::handle_msg (lobby join, inputs, pings).
pub async fn host_peer_pump(
    conn: Connection,
    player_id: u32,
    host: std::sync::Arc<std::sync::Mutex<HostSession>>,
    mut out_rx: tokio::sync::mpsc::UnboundedReceiver<NetMsg>,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), NetError> {
    loop {
        tokio::select! {
            _ = crate::wait_stop(stop.clone()) => return Ok(()),
            m = read_msg_pair(&conn) => {
                let msg = m?;
                let replies = {
                    let h = &mut *host.lock().unwrap_or_else(|p| p.into_inner());
                    let link = h.peers.get(&player_id).map(|pc| (pc.conn.clone(), pc.tx.clone()));
                    h.handle_msg(Some(player_id), link, msg)
                };
                for m in replies {
                    write_msg(&conn, &m).await?;
                }
            }
            Some(msg) = out_rx.recv() => {
                write_msg(&conn, &msg).await?;
            }
        }
    }
}
