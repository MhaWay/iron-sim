// Parity vector generator (task-4). Generates engine_cpp/testdata/*.json
// FROM the reference Rust implementation: engine/crates/core and
// rts-game/crates/shared are used as READ-ONLY path dependencies; the Rust
// workspaces are NOT modified. Every C++ parity test asserts against values
// produced here, so the Rust oracle is the source of truth.
//
// Build & run: tools/run_dump.sh
use engine_core::*;
use shared::{Command as GameCommand, Hasher, TickCommands, WriteHash as _};

fn hx(v: u64) -> String { format!("{:016x}", v) }
fn words_json(ws: &[u64]) -> String {
    ws.iter().map(|v| format!("\"{}\"", hx(*v))).collect::<Vec<_>>().join(",")
}

fn rng_seeds() -> String {
    let seeds = [0u64, 1, 0xDEAD_BEEF, 0x1234_5678_9ABC_DEF0];
    let mut s = String::from("{\"format\":1,\"vectors\":[");
    for (i, seed) in seeds.iter().enumerate() {
        if i > 0 { s.push(','); }
        let mut r = Rng::new(*seed);
        s.push_str(&format!("{{\"seed\":\"{:016x}\",\"values\":[", seed));
        let mut vals = vec![];
        for _ in 0..256 { vals.push(format!("\"{}\"", hx(r.next_u64()))); }
        s.push_str(&vals.join(","));
        s.push_str("]}");
    }
    s.push_str("]}");
    s
}

fn hash_streams() -> String {
    let words: Vec<Vec<u64>> = vec![
        vec![1u64, 2, 3],
        vec![0, 1 << 32, 0xFFFFFFFF, 0xFFFFFFFFFFFFFFFF],
        vec![0xcbf29ce484222325, 0x100000001b3, 0xDEADBEEFCAFEBABE],
    ];
    let mut s = String::from("{\"format\":1,\"fold\":[");
    for (i, w) in words.iter().enumerate() {
        if i > 0 { s.push(','); }
        let mut acc = 0u64;
        let mut folds = vec![];
        for &x in w { acc = hash_fold(acc, x); folds.push(acc); }
        let wd = hash_words(w);
        s.push_str(&format!("{{\"words\":[{}],\"fold_chain\":[{}],\"hash_words\":\"{}\"}}",
            words_json(w), words_json(&folds), hx(wd)));
    }
    s.push_str("]}");
    s
}

fn fixed_vectors() -> String {
    let mut s = String::from("{\"format\":1,\"from_rational\":[");
    let pairs = [(1,1),(1,2),(-1,2),(3,4),(-3,4),(7,3),(-7,3),(1,65536),(65537,2),(-1,1),(0,5)];
    let mut first = true;
    for (n,d) in pairs {
        if !first { s.push(','); }
        first = false;
        s.push_str(&format!("{{\"n\":{},\"d\":{},\"raw\":{}}}", n, d, Fixed1616::from_rational(n,d).0));
    }
    s.push_str("],\"mul_div\":[");
    first = true;
    for (a,b) in [(1,1),(3,1),(1,3),(2,3),(-2,3),(5,5),(-1,3),(4,2)] {
        if !first { s.push(','); }
        first = false;
        let fa = Fixed1616::from_int(a);
        let fb = Fixed1616::from_int(b);
        let m = fa.wrapping_mul(fb);
        let dv = fa.wrapping_div(fb);
        s.push_str(&format!("{{\"a_raw\":{},\"b_raw\":{},\"mul_raw\":{},\"div_raw\":{}}}", fa.0, fb.0, m.0, dv.0));
    }
    s.push_str("]}");
    s
}

fn isqrt_vectors() -> String {
    let mut s = String::from("{\"format\":1,\"isqrt_u128\":[");
    let vals: Vec<u128> = vec![
        9u128 << 32, 1u128 << 32, 2u128 << 32, 1u128 << 64, 3u128 << 32,
        5u128 << 32, 0, 1, 2, 3, 4, 15, 16, 17, 255, 256,
        (1u128 << 63) - 1, 1u128 << 63,
        (1u128 << 127) - 1, 1u128 << 127,
        (1u128 << 64) + 1,
    ];
    let mut first = true;
    for v in &vals {
        if !first { s.push(','); }
        first = false;
        s.push_str(&format!("{{\"n\":\"{:032x}\",\"root\":\"{:016x}\"}}", v, isqrt_u128(*v)));
    }
    s.push_str("],\"fixed_sqrt\":[");
    first = true;
    for raw in [0i64, 65536, 131072, 163840, 262144, 3, 1, 1000, 123456789, -5] {
        if !first { s.push(','); }
        first = false;
        let r = fixed_sqrt(Fixed1616(raw));
        s.push_str(&format!("{{\"raw\":{},\"result_raw\":{}}}", raw, r.0));
    }
    s.push_str("]}");
    s
}

fn command_streams() -> String {
    // Explicit field tuples: kind=Move 0,Attack 1,Stop 2,Build 3,Train 4,
    // Gather 5,Sacrifice 6,Evolve 7,AgeUp 8. The JSON layout mirrors the
    // canonical Rust struct fields so C++ can rebuild byte-exact Commands.
    fn emit_cmd(player: u32, kind: u32, a: i64, b: i64, c: i64) -> String {
        format!("{{\"player\":{},\"kind\":{},\"a\":{},\"b\":{},\"c\":{}}}",
            player, kind, a, b, c)
    }
    fn emit_cmd_h(player: u32, kind: u32, a: i64, b: i64, c: i64, hash: &str) -> String {
        format!("{{\"player\":{},\"kind\":{},\"a\":{},\"b\":{},\"c\":{},\"hash\":\"{}\"}}",
            player, kind, a, b, c, hash)
    }
    // Build Rust command from canonical tuple (kind, a, b, c) — same encoding
    // as emit_cmd / C++ rebuild: a = primary u32 field, b = i16 second,
    // c = i16 third (unused where zero).
    fn build(kind: u32, a: i64, b: i64, c: i64) -> GameCommand {
        match kind {
            0 => GameCommand::Move { unit: a as u32, x: b as i16, y: c as i16 },
            1 => GameCommand::Attack { unit: a as u32, target: b as u32 },
            2 => GameCommand::Stop { unit: a as u32 },
            3 => GameCommand::Build { building: a as u16, x: b as i16, y: c as i16 },
            4 => GameCommand::Train { building: a as u32, unit_kind: b as u16 },
            5 => GameCommand::Gather { unit: a as u32, resource: b as u8 },
            6 => GameCommand::Sacrifice { unit: a as u32 },
            7 => GameCommand::Evolve { tier: b as u8, unit_type: a as u16 },
            8 => GameCommand::AgeUp { age: a as u8 },
            _ => panic!("bad kind"),
        }
    }
    let mut s = String::from("{\"format\":1,\"streams\":[");

    // stream 0: sorted batch (players 1,0,0,0) — canonical sorted hash
    let s0: Vec<(u32, u32, i64, i64, i64)> = vec![
        (1, 0, 3, 1, 2),
        (0, 1, 2, 1, 0),
        (0, 0, 2, 0, 0),
        (0, 2, 1, 0, 0),
    ];
    let tc0: Vec<(u32, GameCommand)> = s0.iter().map(|&(pl, k, a, b, c)| (pl, build(k, a, b, c))).collect();
    let mut t0 = TickCommands { tick: 3, cmds: tc0 };
    t0.sort();
    s.push_str(&format!("{{\"tick\":3,\"sorted\":true,\"cmds\":[", ));
    for (i, &(pl, k, a, b, c)) in s0.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&emit_cmd(pl, k, a, b, c));
    }
    s.push_str(&format!("],\"hash\":\"{}\"}}", hx(t0.hash())));

    // stream 1: all 9 kinds, one command per player 0..8, sorted
    let s1: Vec<(u32, u32, i64, i64, i64)> = vec![
        (0, 0, 1, 10, -10), (1, 1, 1, 9, 0), (2, 2, 1, 0, 0),
        (3, 3, 5, -1, 2), (4, 4, 7, 2, 0), (5, 5, 3, 1, 0),
        (6, 6, 4, 0, 0), (7, 7, 1, 2, 0), (8, 8, 3, 0, 0),
    ];
    let tc1: Vec<(u32, GameCommand)> = s1.iter().map(|&(pl, k, a, b, c)| (pl, build(k, a, b, c))).collect();
    let mut t1 = TickCommands { tick: 7, cmds: tc1 };
    t1.sort();
    s.push_str(",{\"tick\":7,\"cmds\":[");
    for (i, &(pl, k, a, b, c)) in s1.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&emit_cmd(pl, k, a, b, c));
    }
    s.push_str(&format!("],\"hash\":\"{}\"}}", hx(t1.hash())));

    // stream 2: per-command singletons, each in its own TickCommands
    s.push_str(",{\"tick\":0,\"per_command\":[");
    let singles: Vec<(u32, u32, i64, i64, i64)> = vec![
        (0, 0, -1, -1, 1),    // Move unit=0xFFFFFFFF x=-1 y=1
        (1, 1, 1, 2, 0),      // Attack target=2 (u32 in b)
        (2, 2, 3, 0, 0),
        (3, 3, 1, -32768, 32767),
        (4, 4, 2, 3, 0),
        (5, 5, 4, 3, 0),
        (6, 6, 5, 0, 0),
        (7, 7, 65535, 255, 0),  // Evolve: a=unit_type, b=tier
        (8, 8, 255, 0, 0),      // AgeUp: a=age
    ];
    let mut first = true;
    for &(pl, k, a, b, c) in &singles {
        if !first { s.push(','); }
        first = false;
        let t2 = TickCommands { tick: 0, cmds: vec![(pl, build(k, a, b, c))] };
        s.push_str(&emit_cmd_h(pl, k, a, b, c, &hx(t2.hash())));
    }
    s.push_str("]}]}");
    s
}
fn fnv_bytes() -> String {
    let mut s = String::from("{\"format\":1,\"byte_streams\":[");
    let streams: Vec<Vec<u8>> = vec![
        vec![], vec![b'a'], vec![b'a',b'b',b'c'],
        (0..=255u8).collect(),
        vec![0,255,128,1,127,126,254],
    ];
    let mut first = true;
    for st in &streams {
        if !first { s.push(','); }
        first = false;
        let mut h = Hasher::new();
        h.write_bytes(st);
        s.push_str(&format!("{{\"bytes\":[{}],\"digest\":\"{}\"}}",
            st.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","),
            hx(h.finish())));
    }
    s.push_str("]}");
    s
}

fn scheduler_trace() -> String {
    let mut sched = Scheduler::new();
    let mut got: Vec<(u64,u64)> = vec![];
    sched.enqueue(1, Command::from_hash(11));
    sched.enqueue(1, Command::from_hash(22));
    sched.enqueue(0, Command::from_hash(5));
    sched.enqueue(3, Command::from_hash(33));
    sched.enqueue(2, Command::from_hash(20));
    sched.step(1, |t,c| got.push((t, c.payload_hash)));
    sched.step(2, |t,c| got.push((t, c.payload_hash)));
    sched.step(5, |t,c| got.push((t, c.payload_hash)));
    let mut s = String::from("{\"format\":1,\"trace\":[");
    let mut first = true;
    for (t,h) in &got {
        if !first { s.push(','); }
        first = false;
        s.push_str(&format!("{{\"tick\":{},\"payload\":\"{}\"}}", t, hx(*h)));
    }
    s.push_str("]}");
    s
}

fn replay_digests() -> String {
    let mut a = ReplayRecorder::new();
    for (t,h) in [(1u64,11u64),(1,12),(2,13)] { a.record(t,h); }
    let mut b = ReplayRecorder::new();
    b.record(1, 12); b.record(1, 11);
    let td1 = ReplayRecorder::tick_digest(1, &[11,12,13]);
    let td2 = ReplayRecorder::tick_digest(7, &[]);
    let mut s = String::from("{\"format\":1,\"streams\":[");
    s.push_str(&format!("{{\"records\":[{{\"tick\":1,\"payload\":\"{}\"}},{{\"tick\":1,\"payload\":\"{}\"}},{{\"tick\":2,\"payload\":\"{}\"}}],\"digest\":\"{}\"}}",
        hx(11), hx(12), hx(13), hx(a.digest())));
    s.push_str(",{\"records\":[");
    s.push_str(&format!("{{\"tick\":1,\"payload\":\"{}\"}},{{\"tick\":1,\"payload\":\"{}\"}}],\"digest\":\"{}\"}}",
        hx(12), hx(11), hx(b.digest())));
    s.push_str("],\"tick_digests\":[");
    s.push_str(&format!("{{\"tick\":1,\"hashes\":[\"{}\",\"{}\",\"{}\"],\"digest\":\"{}\"}},",
        hx(11), hx(12), hx(13), hx(td1)));
    s.push_str(&format!("{{\"tick\":7,\"hashes\":[],\"digest\":\"{}\"}}", hx(td2)));
    s.push_str("]}");
    s
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR: set OUT_DIR to testdata/");
    std::fs::write(format!("{}/rng_seeds.json", out_dir), rng_seeds()).unwrap();
    std::fs::write(format!("{}/hash_streams.json", out_dir), hash_streams()).unwrap();
    std::fs::write(format!("{}/fixed_values.json", out_dir), fixed_vectors()).unwrap();
    std::fs::write(format!("{}/isqrt_vectors.json", out_dir), isqrt_vectors()).unwrap();
    std::fs::write(format!("{}/command_hashes.json", out_dir), command_streams()).unwrap();
    std::fs::write(format!("{}/fnv_bytes.json", out_dir), fnv_bytes()).unwrap();
    std::fs::write(format!("{}/scheduler_trace.json", out_dir), scheduler_trace()).unwrap();
    std::fs::write(format!("{}/replay_digests.json", out_dir), replay_digests()).unwrap();
    println!("vectors -> {}", out_dir);
}
