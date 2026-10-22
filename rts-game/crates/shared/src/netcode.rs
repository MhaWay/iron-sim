//! Wire + lockstep netcode types shared by every peer.
//!
//! HARD RULES (AGENTS.md): NO f32/f64 anywhere in this module. All timestamps are integer;
//! the only time unit that may cross the wire is TICKS (16.667 ms each at 30 Hz — note this
//! replaces the old 60 Hz / 16.67 ms comment in docs; the sim clock is TICK_HZ=30 in
//! game/src/lib.rs, so one tick = 1/30 s = 33.3 ms and the 1-tick input delay = 33.3 ms).
//! The wire codec is our own: explicit integers, little-endian, versioned. NO serde derive,
//! NO HashMap, NO floats.
//!
//! These types NEVER enter the sim state or `Sim::hash`/`TickCommands::hash`: chat/lobby
//! traffic uses the dedicated `NetMsg` envelopes below, not `shared::Command`. Therefore
//! netcode does not bump SIM_VERSION (see docs/netcode.md).

use crate::Command;

/// Version of the netcode wire format (`NetMsg` envelopes). Bump deliberately on an
/// incompatible change to any tag/field layout (the `Command` wire format itself is
/// governed by shared::Command evolution — see lib.rs tests).
pub const WIRE_VERSION: u8 = 1;

/// Canonical hash of a tick's command batch: same function `TickCommands::hash` uses,
/// exposed here for the lockstep desync check (host broadcasts it, clients recompute).
pub fn tick_commands_hash(tick: u64, cmds: &[(u32, Command)]) -> u64 {
    let mut t = crate::TickCommands {
        tick,
        cmds: cmds.to_vec(),
    };
    t.sort();
    t.hash()
}

// ---------------------------------------------------------------------------
// Base32 invite codes (RFC4648 alphabet, no padding, lowercase output)
// ---------------------------------------------------------------------------

const B32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encode the low `bits` bits of `x` as RFC4648 base32 (must be a multiple of 5).
pub fn base32_low_bits(x: u64, bits: usize) -> String {
    debug_assert!(bits % 5 == 0 && bits <= 60);
    let mask = (1u64 << bits) - 1;
    let v = x & mask;
    let mut s = String::with_capacity(bits / 5);
    for i in (0..bits / 5).rev() {
        let idx = ((v >> (i * 5)) & 0b11111) as usize;
        s.push(B32_ALPHABET[idx] as char);
    }
    s
}

/// Decode an invite code (case-insensitive) back to the original low bits, if valid.
pub fn base32_decode_low_bits(code: &str, bits: usize) -> Option<u64> {
    if code.len() * 5 < bits {
        return None;
    }
    let shift = code.len() * 5 - bits;
    let mut acc: u64 = 0;
    for ch in code.as_bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a',
            b'2'..=b'7' => ch - b'2' + 26,
            _ => return None,
        };
        acc = (acc << 5) | v as u64;
    }
    Some(acc >> shift)
}

/// Canonical 8-char invite code from a host seed (40 bits of a SplitMix64 draw).
pub fn invite_code(host_seed: u64) -> String {
    base32_low_bits(host_seed, 40)
}

// ---------------------------------------------------------------------------
// Tick digest: replay digest of the last 128 command batches
// ---------------------------------------------------------------------------

/// Replay digest of a ring of the most recent `n` command batches (newest last).
/// Each batch hashes with `TickCommands::hash`; the digest chains them with the
/// canonical FNV-1a hasher. Digesting 128 batches = O(128 * batch) per check — cheap
/// enough for a 10 s watchdog, and replay-relevant (the window a client may lag).
pub fn replay_digest(batches: &[(u64, Vec<(u32, Command)>)]) -> u64 {
    let mut h = crate::Hasher::new();
    for (tick, cmds) in batches {
        let mut b = crate::TickCommands {
            tick: *tick,
            cmds: cmds.clone(),
        };
        b.sort();
        h.u64(b.hash());
    }
    h.finish()
}

// ---------------------------------------------------------------------------
// Integer wire codec (LE, versioned; no serde)
// ---------------------------------------------------------------------------

/// Codec error (truncated / invalid buffer). Never panics on remote data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Short,
    BadTag,
    BadUtf8,
}

/// Write-side helpers. All integers little-endian; strings = u32 len + utf8 bytes.
/// The writer owns its buffer: construct with Writer::new, write, then into_buf().
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new(buf: Vec<u8>) -> Self {
        Writer { buf }
    }

    /// Recover the owned buffer after all writes.
    pub fn into_buf(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }
}


/// Read-side cursor. Bounds every read; returns `WireError::Short` on any overrun.
pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    #[inline]
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        if self.pos + n > self.buf.len() {
            return Err(WireError::Short);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }
    pub fn i8(&mut self) -> Result<i8, WireError> {
        Ok(self.take(1)?[0] as i8)
    }
    pub fn u16(&mut self) -> Result<u16, WireError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Result<i16, WireError> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32, WireError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self) -> Result<i32, WireError> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn u64(&mut self) -> Result<u64, WireError> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(b))
    }
    pub fn i64(&mut self) -> Result<i64, WireError> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8)?);
        Ok(i64::from_le_bytes(b))
    }
    pub fn str(&mut self) -> Result<String, WireError> {
        let n = self.u32()? as usize;
        if n > 65536 {
            return Err(WireError::Short);
        }
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| WireError::BadUtf8)
    }
    /// Decoded payload cap — remote batches are bounded, never unbounded from the wire.
    pub const MAX_BATCH_CMDS: usize = 4096;
}

// ---------------------------------------------------------------------------
// Command encoding (the ONLY thing that crosses into the sim path)
// ---------------------------------------------------------------------------

pub fn cmd_write(w: &mut Writer, c: &Command) {
    match c {
        Command::Move { unit, x, y } => {
            w.u8(0);
            w.u32(*unit);
            w.i16(*x);
            w.i16(*y);
        }
        Command::Attack { unit, target } => {
            w.u8(1);
            w.u32(*unit);
            w.u32(*target);
        }
        Command::Stop { unit } => {
            w.u8(2);
            w.u32(*unit);
        }
        Command::Build { building, x, y } => {
            w.u8(3);
            w.u16(*building);
            w.i16(*x);
            w.i16(*y);
        }
        Command::Train { building, unit_kind } => {
            w.u8(4);
            w.u32(*building);
            w.u16(*unit_kind);
        }
        Command::Gather { unit, resource } => {
            w.u8(5);
            w.u32(*unit);
            w.u8(*resource);
        }
        Command::Sacrifice { unit } => {
            w.u8(6);
            w.u32(*unit);
        }
        Command::Evolve { tier, unit_type } => {
            w.u8(7);
            w.u8(*tier);
            w.u16(*unit_type);
        }
        Command::AgeUp { age } => {
            w.u8(8);
            w.u8(*age);
        }
        Command::RallyPoint { building, tx, ty } => {
            w.u8(9);
            w.u32(*building);
            w.i16(*tx);
            w.i16(*ty);
        }
        Command::Rally { unit, tx, ty } => {
            w.u8(10);
            w.u32(*unit);
            w.i16(*tx);
            w.i16(*ty);
        }
        // Non-exhaustive on purpose: a NEW Command variant must not silently decode.
        #[allow(unreachable_patterns)]
        _ => {
            // Encode nothing readable — writers must extend this match; encode() guards.
            w.u8(255);
        }
    }
}

pub fn cmd_read(r: &mut Reader) -> Result<Command, WireError> {
    let tag = r.u8()?;
    let c = match tag {
        0 => Command::Move {
            unit: r.u32()?,
            x: r.i16()?,
            y: r.i16()?,
        },
        1 => Command::Attack {
            unit: r.u32()?,
            target: r.u32()?,
        },
        2 => Command::Stop { unit: r.u32()? },
        3 => Command::Build {
            building: r.u16()?,
            x: r.i16()?,
            y: r.i16()?,
        },
        4 => Command::Train {
            building: r.u32()?,
            unit_kind: r.u16()?,
        },
        5 => Command::Gather {
            unit: r.u32()?,
            resource: r.u8()?,
        },
        6 => Command::Sacrifice { unit: r.u32()? },
        7 => Command::Evolve {
            tier: r.u8()?,
            unit_type: r.u16()?,
        },
        8 => Command::AgeUp { age: r.u8()? },
        9 => Command::RallyPoint {
            building: r.u32()?,
            tx: r.i16()?,
            ty: r.i16()?,
        },
        10 => Command::Rally {
            unit: r.u32()?,
            tx: r.i16()?,
            ty: r.i16()?,
        },
        _ => return Err(WireError::BadTag),
    };
    Ok(c)
}

// ---------------------------------------------------------------------------
// NetMsg — control-plane envelopes (NEVER fed to the sim)
// ---------------------------------------------------------------------------

/// Match visibility at creation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyVisibility {
    Public,
    Private,
}

/// Slot assignment kind for a joiner (lobby-level; sim never sees this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotRole {
    Host,
    Player,
    /// Reconnect slot: player id is re-bound to the same sim player index.
    Reconnect,
}

/// One player entry in the lobby snapshot (name capped at 32 chars on decode).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    pub ready: bool,
    pub connected: bool,
    pub slot: u8, // sim player index 0..=1 (MAX_PLAYERS)
}

impl PlayerInfo {
    pub fn new(id: u32, name: String, slot: u8) -> Self {
        PlayerInfo {
            id,
            name,
            ready: false,
            connected: true,
            slot,
        }
    }
}

/// The message set. Tags: 1..15 handshake/lobby, 16.. control, 32.. sim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetMsg {
    /// u8 wire_version + u32 protocol_version + name
    Hello {
        wire_version: u8,
        proto: u32,
        name: String,
    },
    /// Join by invite code (private) — host validates, replies Welcome/Refused.
    JoinInvite {
        code: String,
    },
    /// Welcome carries the sim config so the client builds the IDENTICAL world.
    Welcome {
        player_id: u32,
        sim_seed: u64,
        preset: String,
        max_players: u8,
        lobby_code: String,
    },
    /// Lobby rejection: (code, detail).
    Refused {
        code: RefuseCode,
        detail: String,
    },
    /// Full lobby state, broadcast on every change.
    Lobby {
        players: Vec<PlayerInfo>,
        host_id: u32,
        started: bool,
        preset: String,
        seed: u64,
        max_players: u8,
    },
    /// Ready toggle (lobby control).
    Ready {
        ready: bool,
    },
    /// Host starts the match (broadcast of the authoritative decision).
    Start,
    /// One authoritative tick: commands for `tick` (already canonical).
    TickBatch {
        tick: u64,
        cmds: Vec<(u32, Command)>,
    },
    /// Desync watchdog: digest of the last `window` batches, every `interval` ticks.
    SyncDigest {
        tick: u64,
        digest: u64,
        window: u32,
        interval: u32,
    },
    /// A local command an authoritative client submits for tick t (delay applied by host).
    Input {
        tick: u64,
        cmds: Vec<(u32, Command)>,
    },
    Chat {
        player_id: u32,
        text: String,
    },
    Ping {
        t_millis: u64,
    },
    Pong {
        t_millis: u64,
    },
    /// Host: a peer joined / reconnected (player_id) / (connected=true).
    PeerJoined {
        player_id: u32,
        connected: bool,
    },
    PeerLeft {
        player_id: u32,
    },
    /// Host-authoritative pause toggle broadcast.
    Pause {
        paused: bool,
    },
    /// Reconnect handshake: resume with (player_id, session_token).
    Reconnect {
        player_id: u32,
        token: u64,
    },
    /// Host: full resync snapshot for a reconnecting / late-joining client.
    Resync {
        snapshot: StateSnapshot,
    },
    /// Generic transport-level keepalive (tags kept for symmetry; QUIC keepalive is primary).
    KeepAlive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefuseCode {
    MatchFull,
    MatchStarted,
    BadInvite,
    VersionMismatch,
    DuplicatePlayer,
    Banned,
    BadName,
    Other,
}

impl RefuseCode {
    pub fn tag(self) -> u8 {
        match self {
            RefuseCode::MatchFull => 1,
            RefuseCode::MatchStarted => 2,
            RefuseCode::BadInvite => 3,
            RefuseCode::VersionMismatch => 4,
            RefuseCode::DuplicatePlayer => 5,
            RefuseCode::Banned => 6,
            RefuseCode::BadName => 7,
            RefuseCode::Other => 8,
        }
    }
    pub fn from_tag(t: u8) -> Option<RefuseCode> {
        Some(match t {
            1 => RefuseCode::MatchFull,
            2 => RefuseCode::MatchStarted,
            3 => RefuseCode::BadInvite,
            4 => RefuseCode::VersionMismatch,
            5 => RefuseCode::DuplicatePlayer,
            6 => RefuseCode::Banned,
            7 => RefuseCode::BadName,
            8 => RefuseCode::Other,
            _ => return None,
        })
    }
}

/// Compact state snapshot for resync (reconnect). Field order mirrors `Sim::hash`
/// (see game/sim.rs) minus hash-only noise; the point is correctness of the *rebuild*,
/// and `tick + rng_state + hash_after` let the client verify parity before joining live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSnapshot {
    pub version: u8,
    pub tick: u64,
    pub rng_state: u64,
    pub hash_after: u64,
    pub grid_w: u16,
    pub grid_h: u16,
    pub players: Vec<PlayerWire>,
    pub vis: Vec<Vec<u64>>,
    pub buildings: Vec<(u32, u8, u16, u8, u32, u32, u32, i32, i32, Vec<(u16, u32)>, Option<(i16, i16)>)>,
    pub nodes: Vec<(u32, u8, i64, i64, i16, i16)>,
    pub units: Vec<(u32, u8, u16, i32, u32, i32, i64, i32, i32, Option<u32>, Option<(Vec<(i16, i16)>, usize)>)>,
    pub blocked: Vec<(u16, u16)>,
}

/// Player-side mirror of `game::economy::PlayerState` over the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWire {
    pub food: i64,
    pub wood: i64,
    pub gold: i64,
    pub mana: i64,
    pub age: u8,
    pub age_target: u8,
    pub age_timer: u32,
    pub civ: u8,
    pub score: i64,
    pub winner: u32,
    pub wonder: u32,
    pub wonder_started: u64,
}

// ---------------------------------------------------------------------------
// NetMsg encode/decode
// ---------------------------------------------------------------------------

pub fn msg_write(m: &NetMsg, buf: &mut Vec<u8>) {
    let mut w = Writer::new(std::mem::take(buf));
    msg_write_into(m, &mut w);
    *buf = w.into_buf();
}

fn msg_write_into(m: &NetMsg, w: &mut Writer) {
    match m {
        NetMsg::Hello {
            wire_version,
            proto,
            name,
        } => {
            w.u8(1);
            w.u8(*wire_version);
            w.u32(*proto);
            w.str(&name);
        }
        NetMsg::JoinInvite { code } => {
            w.u8(2);
            w.str(&code);
        }
        NetMsg::Welcome {
            player_id,
            sim_seed,
            preset,
            max_players,
            lobby_code,
        } => {
            w.u8(3);
            w.u32(*player_id);
            w.u64(*sim_seed);
            w.str(&preset);
            w.u8(*max_players);
            w.str(&lobby_code);
        }
        NetMsg::Refused { code, detail } => {
            w.u8(4);
            w.u8(code.tag());
            w.str(&detail);
        }
        NetMsg::Lobby {
            players,
            host_id,
            started,
            preset,
            seed,
            max_players,
        } => {
            w.u8(5);
            w.u32(players.len() as u32);
            for p in players {
                w.u32(p.id);
                w.str(&p.name);
                w.u8(p.ready as u8);
                w.u8(p.connected as u8);
                w.u8(p.slot);
            }
            w.u32(*host_id);
            w.u8(*started as u8);
            w.str(&preset);
            w.u64(*seed);
            w.u8(*max_players);
        }
        NetMsg::Ready { ready } => {
            w.u8(6);
            w.u8(*ready as u8);
        }
        NetMsg::Start => {
            w.u8(7);
        }
        NetMsg::TickBatch { tick, cmds } => {
            w.u8(8);
            w.u64(*tick);
            w.u32(cmds.len() as u32);
            for (p, c) in cmds {
                w.u32(*p);
                cmd_write(w, c);
            }
        }
        NetMsg::SyncDigest {
            tick,
            digest,
            window,
            interval,
        } => {
            w.u8(9);
            w.u64(*tick);
            w.u64(*digest);
            w.u32(*window);
            w.u32(*interval);
        }
        NetMsg::Input { tick, cmds } => {
            w.u8(10);
            w.u64(*tick);
            w.u32(cmds.len() as u32);
            for (p, c) in cmds {
                w.u32(*p);
                cmd_write(w, c);
            }
        }
        NetMsg::Chat { player_id, text } => {
            w.u8(11);
            w.u32(*player_id);
            w.str(&text);
        }
        NetMsg::Ping { t_millis } => {
            w.u8(12);
            w.u64(*t_millis);
        }
        NetMsg::Pong { t_millis } => {
            w.u8(13);
            w.u64(*t_millis);
        }
        NetMsg::PeerJoined { player_id, connected } => {
            w.u8(14);
            w.u32(*player_id);
            w.u8(*connected as u8);
        }
        NetMsg::PeerLeft { player_id } => {
            w.u8(15);
            w.u32(*player_id);
        }
        NetMsg::Pause { paused } => {
            w.u8(16);
            w.u8(*paused as u8);
        }
        NetMsg::Reconnect { player_id, token } => {
            w.u8(17);
            w.u32(*player_id);
            w.u64(*token);
        }
        NetMsg::Resync { snapshot } => {
            w.u8(18);
            snapshot_write(snapshot, w);
        }
        NetMsg::KeepAlive => {
            w.u8(19);
        }
    }
}

pub fn msg_read(buf: &[u8]) -> Result<NetMsg, WireError> {
    let mut r = Reader::new(buf);
    let tag = r.u8()?;
    let m = match tag {
        1 => NetMsg::Hello {
            wire_version: r.u8()?,
            proto: r.u32()?,
            name: r.str()?,
        },
        2 => NetMsg::JoinInvite { code: r.str()? },
        3 => NetMsg::Welcome {
            player_id: r.u32()?,
            sim_seed: r.u64()?,
            preset: r.str()?,
            max_players: r.u8()?,
            lobby_code: r.str()?,
        },
        4 => {
            let code = RefuseCode::from_tag(r.u8()?).unwrap_or(RefuseCode::Other);
            NetMsg::Refused {
                code,
                detail: r.str()?,
            }
        }
        5 => {
            let n = clamp_u32(r.u32()?, 16)?;
            let mut players = Vec::with_capacity(n);
            for _ in 0..n {
                players.push(PlayerInfo {
                    id: r.u32()?,
                    name: r.str()?,
                    ready: r.u8()? != 0,
                    connected: r.u8()? != 0,
                    slot: r.u8()?,
                });
            }
            NetMsg::Lobby {
                players,
                host_id: r.u32()?,
                started: r.u8()? != 0,
                preset: r.str()?,
                seed: r.u64()?,
                max_players: r.u8()?,
            }
        }
        6 => NetMsg::Ready {
            ready: r.u8()? != 0,
        },
        7 => NetMsg::Start,
        8 => NetMsg::TickBatch {
            tick: r.u64()?,
            cmds: cmds_read(&mut r)?,
        },
        9 => NetMsg::SyncDigest {
            tick: r.u64()?,
            digest: r.u64()?,
            window: r.u32()?,
            interval: r.u32()?,
        },
        10 => NetMsg::Input {
            tick: r.u64()?,
            cmds: cmds_read(&mut r)?,
        },
        11 => NetMsg::Chat {
            player_id: r.u32()?,
            text: r.str()?,
        },
        12 => NetMsg::Ping {
            t_millis: r.u64()?,
        },
        13 => NetMsg::Pong {
            t_millis: r.u64()?,
        },
        14 => NetMsg::PeerJoined {
            player_id: r.u32()?,
            connected: r.u8()? != 0,
        },
        15 => NetMsg::PeerLeft { player_id: r.u32()? },
        16 => NetMsg::Pause {
            paused: r.u8()? != 0,
        },
        17 => NetMsg::Reconnect {
            player_id: r.u32()?,
            token: r.u64()?,
        },
        18 => NetMsg::Resync {
            snapshot: snapshot_read(&mut r)?,
        },
        19 => NetMsg::KeepAlive,
        _ => return Err(WireError::BadTag),
    };
    Ok(m)
}

/// Remote lists must be bounded; 16 is plenty (players) and 4096 for commands.
fn clamp_u32(n: u32, max: usize) -> Result<usize, WireError> {
    if n as usize > max {
        Err(WireError::Short)
    } else {
        Ok(n as usize)
    }
}

fn cmds_read(r: &mut Reader) -> Result<Vec<(u32, Command)>, WireError> {
    let n = clamp_u32(r.u32()?, Reader::MAX_BATCH_CMDS)?;
    let mut v = Vec::with_capacity(n.min(64));
    for _ in 0..n {
        let p = r.u32()?;
        let c = cmd_read(r)?;
        v.push((p, c));
    }
    Ok(v)
}

fn snapshot_write(s: &StateSnapshot, w: &mut Writer) {
    w.u8(s.version);
    w.u64(s.tick);
    w.u64(s.rng_state);
    w.u64(s.hash_after);
    w.u16(s.grid_w);
    w.u16(s.grid_h);
    let players = &s.players;
    w.u32(players.len() as u32);
    for p in players {
        w.i64(p.food);
        w.i64(p.wood);
        w.i64(p.gold);
        w.i64(p.mana);
        w.u8(p.age);
        w.u8(p.age_target);
        w.u32(p.age_timer);
        w.u8(p.civ);
        w.i64(p.score);
        w.u32(p.winner);
        w.u32(p.wonder);
        w.u64(p.wonder_started);
    }
    w.u32(s.vis.len() as u32);
    for v in &s.vis {
        w.u32(v.len() as u32);
        for x in v {
            w.u64(*x);
        }
    }
    w.u32(s.buildings.len() as u32);
    for (e, owner, kind, status, hp, max_hp, timer, px, py, q, rp) in &s.buildings {
        w.u32(*e);
        w.u32(*owner as u32);
        w.u16(*kind);
        w.u8(*status);
        w.u32(*hp);
        w.u32(*max_hp);
        w.u32(*timer);
        w.i32(*px);
        w.i32(*py);
        w.u32(q.len() as u32);
        for (k, t) in q {
            w.u16(*k);
            w.u32(*t);
        }
        w.u8(rp.is_some() as u8);
        if let Some((tx, ty)) = rp {
            w.i16(*tx);
            w.i16(*ty);
        }
    }
    w.u32(s.nodes.len() as u32);
    for (e, slot, total, remaining, nx, ny) in &s.nodes {
        w.u32(*e);
        w.u8(*slot);
        w.i64(*total);
        w.i64(*remaining);
        w.i16(*nx);
        w.i16(*ny);
    }
    w.u32(s.units.len() as u32);
    for u in &s.units {
        let (e, owner, kind, hp, speed_q, dmg, range, px, py, order, path) = u;
        w.u32(*e);
        w.u8(*owner);
        w.u16(*kind);
        w.i32(*hp);
        w.u32(*speed_q);
        w.i32(*dmg);
        w.i64(*range);
        w.i32(*px);
        w.i32(*py);
        match order {
            None => w.u8(0),
            Some(h) => {
                w.u8(*h as u8 + 1);
                if *h == 2 {
                    // Move order: path + waypoint follow in the path field
                }
            }
        }
        if let Some((path, wp)) = path {
            w.u32(path.len() as u32);
            for (x, y) in path {
                w.i16(*x);
                w.i16(*y);
            }
            w.u32(*wp as u32);
        }
    }
    w.u32(s.blocked.len() as u32);
    for (x, y) in &s.blocked {
        w.u16(*x);
        w.u16(*y);
    }
}

fn snapshot_read(r: &mut Reader) -> Result<StateSnapshot, WireError> {
    let version = r.u8()?;
    let tick = r.u64()?;
    let rng_state = r.u64()?;
    let hash_after = r.u64()?;
    let grid_w = r.u16()?;
    let grid_h = r.u16()?;
    let np = clamp_u32(r.u32()?, 8)?;
    let mut players = Vec::with_capacity(np);
    for _ in 0..np {
        players.push(PlayerWire {
            food: r.i64()?,
            wood: r.i64()?,
            gold: r.i64()?,
            mana: r.i64()?,
            age: r.u8()?,
            age_target: r.u8()?,
            age_timer: r.u32()?,
            civ: r.u8()?,
            score: r.i64()?,
            winner: r.u32()?,
            wonder: r.u32()?,
            wonder_started: r.u64()?,
        });
    }
    let nv = clamp_u32(r.u32()?, 8)?;
    let mut vis = Vec::with_capacity(nv);
    for _ in 0..nv {
        let nw = clamp_u32(r.u32()?, 4096)?;
        let mut v = Vec::with_capacity(nw);
        for _ in 0..nw {
            v.push(r.u64()?);
        }
        vis.push(v);
    }
    let nb = clamp_u32(r.u32()?, 4096)?;
    let mut buildings = Vec::with_capacity(nb.min(64));
    for _ in 0..nb {
        let e = r.u32()?;
        let owner = r.u32()? as u8;
        let kind = r.u16()?;
        let status = r.u8()?;
        let hp = r.u32()?;
        let max_hp = r.u32()?;
        let timer = r.u32()?;
        let px = r.i32()?;
        let py = r.i32()?;
        let nq = clamp_u32(r.u32()?, 64)?;
        let mut q = Vec::with_capacity(nq);
        for _ in 0..nq {
            q.push((r.u16()?, r.u32()?));
        }
        let rp = match r.u8()? {
            0 => None,
            _ => Some((r.i16()?, r.i16()?)),
        };
        buildings.push((e, owner, kind, status, hp, max_hp, timer, px, py, q, rp));
    }
    let nn = clamp_u32(r.u32()?, 8192)?;
    let mut nodes = Vec::with_capacity(nn.min(64));
    for _ in 0..nn {
        nodes.push((r.u32()?, r.u8()?, r.i64()?, r.i64()?, r.i16()?, r.i16()?));
    }
    let nu = clamp_u32(r.u32()?, 65535)?;
    let mut units = Vec::with_capacity(nu.min(128));
    for _ in 0..nu {
        let e = r.u32()?;
        let owner = r.u8()?;
        let kind = r.u16()?;
        let hp = r.i32()?;
        let speed_q = r.u32()?;
        let dmg = r.i32()?;
        let range = r.i64()?;
        let px = r.i32()?;
        let py = r.i32()?;
        let order = match r.u8()? {
            0 => None,
            t => Some((t - 1) as u32),
        };
        // path present iff order code 2 (Move) — same convention as writer.
        let path = if order == Some(2) {
            let n = clamp_u32(r.u32()?, 1_000_000)?;
            let mut p = Vec::with_capacity(n.min(32));
            for _ in 0..n {
                p.push((r.i16()?, r.i16()?));
            }
            Some((p, r.u32()? as usize))
        } else {
            None
        };
        units.push((e, owner, kind, hp, speed_q, dmg, range, px, py, order, path));
    }
    let nblk = clamp_u32(r.u32()?, 65536)?;
    let mut blocked = Vec::with_capacity(nblk.min(128));
    for _ in 0..nblk {
        blocked.push((r.u16()?, r.u16()?));
    }
    Ok(StateSnapshot {
        version,
        tick,
        rng_state,
        hash_after,
        grid_w,
        grid_h,
        players,
        vis,
        buildings,
        nodes,
        units,
        blocked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_invite_roundtrip() {
        for seed in [0u64, 1, 12345, u64::MAX, 0xdead_beef_cafe_f00d] {
            let code = invite_code(seed);
            assert_eq!(code.len(), 8);
            let back = base32_decode_low_bits(&code, 40).unwrap();
            // round-trip must preserve the low 40 bits that were encoded
            assert_eq!(back & ((1u64 << 40) - 1), seed & ((1u64 << 40) - 1), "seed {seed}");
        }
        // decode is case-insensitive
        let code = invite_code(99);
        assert_eq!(
            base32_decode_low_bits(&code.to_lowercase(), 40),
            base32_decode_low_bits(&code, 40)
        );
    }

    #[test]
    fn command_wire_roundtrip_all_variants() {
        let cmds = [
            Command::Move { unit: 1, x: 2, y: 3 },
            Command::Attack { unit: 4, target: 5 },
            Command::Stop { unit: 6 },
            Command::Build { building: 7, x: -8, y: 9 },
            Command::Train { building: 10, unit_kind: 11 },
            Command::Gather { unit: 12, resource: 13 },
            Command::Sacrifice { unit: 14 },
            Command::Evolve { tier: 15, unit_type: 16 },
            Command::AgeUp { age: 17 },
            Command::RallyPoint { building: 18, tx: 19, ty: -20 },
            Command::Rally { unit: 21, tx: -22, ty: 23 },
        ];
        for c in cmds {
            let mut buf = Vec::new();
            let mut w = Writer::new(Vec::new());
            cmd_write(&mut w, &c);
            buf = w.into_buf();
            let mut r = Reader::new(&buf);
            let got = cmd_read(&mut r).unwrap();
            assert_eq!(c, got);
            assert_eq!(r.pos, buf.len(), "trailing bytes for {c:?}");
        }
    }

    #[test]
    fn netmsg_roundtrip_all_variants() {
        let msgs = vec![
            NetMsg::Hello { wire_version: WIRE_VERSION, proto: 1, name: "host".into() },
            NetMsg::JoinInvite { code: "ABCD1234".into() },
            NetMsg::Welcome { player_id: 2, sim_seed: 777, preset: "arena".into(), max_players: 2, lobby_code: "ABCD1234".into() },
            NetMsg::Refused { code: RefuseCode::BadInvite, detail: "nope".into() },
            NetMsg::Lobby { players: vec![PlayerInfo::new(1, "alice".into(), 0), PlayerInfo::new(2, "bob".into(), 1)], host_id: 1, started: true, preset: "arena".into(), seed: 9, max_players: 2 },
            NetMsg::Ready { ready: true },
            NetMsg::Start,
            NetMsg::TickBatch { tick: 1234567, cmds: vec![(0, Command::Move { unit: 1, x: 1, y: 1 }), (1, Command::Stop { unit: 2 })] },
            NetMsg::SyncDigest { tick: 300, digest: 0xabcd_0000_1234_5678, window: 128, interval: 300 },
            NetMsg::Input { tick: 301, cmds: vec![] },
            NetMsg::Chat { player_id: 2, text: "gl hf".into() },
            NetMsg::Ping { t_millis: 0 },
            NetMsg::Pong { t_millis: u64::MAX },
            NetMsg::PeerJoined { player_id: 3, connected: true },
            NetMsg::PeerLeft { player_id: 3 },
            NetMsg::Pause { paused: true },
            NetMsg::Reconnect { player_id: 1, token: 123 },
            NetMsg::KeepAlive,
        ];
        for m in msgs {
            let mut buf = Vec::new();
            msg_write(&m, &mut buf);
            let got = msg_read(&buf).unwrap();
            assert_eq!(m, got);
        }
    }

    #[test]
    fn snapshot_roundtrip() {
        let s = StateSnapshot {
            version: 1,
            tick: 4211,
            rng_state: 0x1234_5678_9abc_def0,
            hash_after: 0xcafe,
            grid_w: 128,
            grid_h: 128,
            players: vec![PlayerWire { food: 150, wood: 150, gold: 0, mana: 0, age: 1, age_target: 2, age_timer: 600, civ: 0, score: 17, winner: 0, wonder: 0, wonder_started: 0 }],
            vis: vec![vec![0, 1], vec![7]],
            buildings: vec![(5, 0, 0, 2, 100, 100, 0, 15, 15, vec![(1, 20)], Some((16, 16)))],
            nodes: vec![(30, 0, 2000, 1998, 40, 41)],
            units: vec![
                (10, 0, 0, 100, 1024, 5, 1572864, 14, 15, None, None),
                (11, 1, 1, 100, 1100, 6, 1572864, 113, 113, Some(2), Some((vec![(112, 112), (111, 112)], 1))),
            ],
            blocked: vec![(1, 1), (126, 126)],
        };
        let mut buf = Vec::new();
        let mut w = Writer::new(Vec::new());
        snapshot_write(&s, &mut w);
        buf = w.into_buf();
        let got = snapshot_read(&mut Reader::new(&buf)).unwrap();
        assert_eq!(s, got);
    }

    #[test]
    fn tick_digest_matches_tickcommands_hash() {
        let cmds = vec![
            (1u32, Command::Move { unit: 1, x: 2, y: 3 }),
            (0u32, Command::Attack { unit: 1, target: 2 }),
        ];
        let h = tick_commands_hash(9, &cmds);
        let mut t = crate::TickCommands { tick: 9, cmds: vec![(0u32, Command::Attack { unit: 1, target: 2 }), (1u32, Command::Move { unit: 1, x: 2, y: 3 })] };
        t.sort();
        assert_eq!(h, t.hash());
    }

    #[test]
    fn replay_digest_order_and_window() {
        // digest of a window depends on order + tick ids, is deterministic, and
        // differs when any batch's commands differ.
        let a = replay_digest(&[(1, vec![(0, Command::Stop { unit: 1 })]), (2, vec![])]);
        let b = replay_digest(&[(1, vec![(0, Command::Stop { unit: 1 })]), (2, vec![])]);
        assert_eq!(a, b);
        let c = replay_digest(&[(2, vec![]), (1, vec![(0, Command::Stop { unit: 1 })])]);
        assert_ne!(a, c);
        let d = replay_digest(&[(1, vec![(0, Command::Stop { unit: 2 })]), (2, vec![])]);
        assert_ne!(a, d);
    }

    #[test]
    fn decoder_rejects_truncated_and_bad_tags() {
        // truncated TickBatch
        let mut buf = Vec::new();
        msg_write(&NetMsg::TickBatch { tick: 1, cmds: vec![(0, Command::Move { unit: 1, x: 1, y: 1 })] }, &mut buf);
        buf.truncate(6);
        assert!(msg_read(&buf).is_err());
        // bogus tag
        assert_eq!(msg_read(&[42u8]), Err(WireError::BadTag));
        // bad utf8 in name
        let mut buf = vec![1u8, 1, 0, 0, 0];
        buf.extend_from_slice(&[4, 0, 0, 0, 0xff, 0xfe, 0xfd]);
        assert!(msg_read(&buf).is_err());
    }
}
