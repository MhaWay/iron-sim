// Test 3 (integration, #[ignore]): reconnect — client "restarts" (drop ClientSession),
// rejoins the same match via Reconnect{player_id,token} + Resync snapshot, and the
// continued game stays hash-identical on both nodes.
use net::bridge::step_n;
use net::{event_channel, reconnect_token, restore_sim, ClientSession, HostSession, SessionCfg};
use shared::netcode::{invite_code, NetMsg};
use shared::Command;

#[test]
#[ignore = "resync path; CI-gated"]
fn reconnect_join_same_match() {
    let seed = 42;
    let (htx, _) = event_channel();
    let mut host = HostSession::new(
        SessionCfg {
            name: "h".into(),
            seed,
            preset: String::new(),
            is_host: true,
        },
        htx,
    );
    let (ct1, _) = event_channel();
    let mut c1 = ClientSession::new(
        SessionCfg {
            name: "c1".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        ct1,
    );
    // join: the Welcome reply is delivered by the transport (admit_peer's tuple), not
    // in handle_msg's return — emulate "host saw the join": Welcome { player_id: 2 }.
    let _replies = host.handle_msg(
        None,
        None,
        NetMsg::JoinInvite {
            code: invite_code(seed),
        },
    );
    let pid = 2u32;
    let _ = host.handle_msg(Some(1), None, NetMsg::Start);
    c1.player_id = pid;
    c1.sim = game::Sim::new(seed);
    c1.started = true;
    for t in 0..300u64 {
        if t % 50 == 10 {
            host.pending
                .entry(t)
                .or_default()
                .push((1, Command::Stop { unit: 15 }));
        }
    }
    let mut cs1 = [c1];
    let (_, _, d) = step_n(&mut host, &mut cs1, 300).unwrap();
    assert!(d.is_empty());
    // "crash" c1: fresh session, rejoins with token.
    let token = reconnect_token(seed, pid);
    let (ct2, _) = event_channel();
    let mut c2 = ClientSession::new(
        SessionCfg {
            name: "c1b".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        ct2,
    );
    let _ = host.handle_msg(
        Some(pid),
        None,
        NetMsg::Reconnect {
            player_id: pid,
            token,
        },
    );
    c2.sim = game::Sim::new(seed);
    if let Some(NetMsg::Resync { snapshot }) = host.drain_resync() {
        restore_sim(&mut c2.sim, &snapshot).unwrap();
    }
    c2.started = true;
    // After Resync the client resumes where it diverged: next_tick = snapshot.tick (300),
    // then applies the host's next batch — host.next_tick is already 300.
    c2.next_tick = 300;
    c2.player_id = pid;
    // NOTE: in bridge mode step_n executes every tick immediately, so pending inputs
    // must be queued BEFORE the stepping loop begins (queueing mid-loop means the host
    // applies them while the client never sees them in the batch -> desync).
    for t in 300..600u64 {
        if t % 50 == 20 {
            host.pending.entry(t).or_default().push((
                1,
                Command::Move {
                    unit: 15,
                    x: 70,
                    y: 70,
                },
            ));
        }
    }
    println!(
        "after resync: host.next_tick={} c2.next_tick={} ring_len={}",
        host.next_tick,
        c2.next_tick,
        c2.ring.len()
    );
    let mut cs2 = [c2];
    let (h, cs, d2) = step_n(&mut host, &mut cs2, 300).unwrap();
    assert!(d2.is_empty(), "desync after reconnect: {d2:?}");
    assert_eq!(
        h, cs[0],
        "reconnected client diverged: host={h:016x} client={:016x}",
        cs[0]
    );
}
