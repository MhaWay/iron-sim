// Reconstructed after teammate churn removed it (10:17). 300-tick host, fresh same-seed
// client, restore, assert hash equality — the original diag_snapshot_roundtrip shape.
use net::{event_channel, restore_sim, snapshot_from_sim, ClientSession, HostSession, SessionCfg};
use shared::netcode::NetMsg;
use shared::Command;

#[test]
fn diag_snapshot_roundtrip() {
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
    let _ = host.handle_msg(Some(1), None, NetMsg::Start);
    for t in 0..300u64 {
        if t % 50 == 10 {
            host.pending
                .entry(t)
                .or_default()
                .push((1, Command::Stop { unit: 15 }));
        }
        host.step_tick().unwrap();
    }
    let snap = snapshot_from_sim(&host.sim);
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
    c2.sim = game::Sim::new(seed);
    restore_sim(&mut c2.sim, &snap).unwrap();
    println!(
        "host.hash()={:016x} snap.hash_after={:016x} client={:016x}",
        host.sim.hash(),
        snap.hash_after,
        c2.sim.hash()
    );
    assert_eq!(host.sim.hash(), c2.sim.hash());
}
