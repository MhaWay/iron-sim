// Test 1 (fast unit): canonical command order — remote (client-sourced) and local
// inputs, canonicalized identically via Command::sort_key, yield the same next-tick hash.
use net::bridge::{start_match, step_n};
use net::{event_channel, ClientSession, HostSession, SessionCfg};
use shared::Command;

fn cfg(name: &str, is_host: bool) -> SessionCfg {
    SessionCfg {
        name: name.into(),
        seed: 9,
        preset: String::new(),
        is_host,
    }
}

#[test]
fn canonical_order_local_eq_remote() {
    let (htx, _) = event_channel();
    let mut host = HostSession::new(cfg("host", true), htx);
    let (ct1, _) = event_channel();
    let c1 = ClientSession::new(cfg("c1", false), ct1);
    let mut clients = [c1];
    start_match(&mut host, &mut clients);
    // Commands as they'd be canonically sorted at tick 1 (move before stop).
    host.pending.entry(1).or_default().extend(vec![
        (
            1,
            Command::Move {
                unit: 15,
                x: 30,
                y: 30,
            },
        ),
        (1, Command::Stop { unit: 16 }),
        (
            2,
            Command::Gather {
                unit: 113,
                resource: 0,
            },
        ),
    ]);
    let (h, cs, desyncs) = step_n(&mut host, &mut clients, 2).unwrap();
    assert!(desyncs.is_empty(), "unexpected desync: {desyncs:?}");
    assert_eq!(h, cs[0]);
}
