// Test 2 (integration, #[ignore]): 10k ticks, host + 1 client in-process over the
// bridge (the same state machines the QUIC glue drives); assert identical final hash
// and zero desync. Run: cargo test -p net --test loopback -- --ignored
use net::bridge::{start_match, step_n};
use net::{event_channel, ClientSession, HostSession, SessionCfg};
use shared::Command;

#[test]
#[ignore = "10k ticks ≈ >60s; CI-gated"]
fn loopback_10k_identical_hash() {
    let seed = 1234;
    let (htx, _) = event_channel();
    let mut host = HostSession::new(
        SessionCfg {
            name: "host".into(),
            seed,
            preset: String::new(),
            is_host: true,
        },
        htx,
    );
    let (ct1, _) = event_channel();
    let c1 = ClientSession::new(
        SessionCfg {
            name: "c1".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        ct1,
    );
    let mut clients = [c1];
    start_match(&mut host, &mut clients);
    for t in 0..10_000u64 {
        if t % 40 == 5 {
            host.pending.entry(t).or_default().push((
                1,
                Command::Move {
                    unit: 15,
                    x: 63,
                    y: 63,
                },
            ));
        }
        if t % 40 == 25 {
            host.pending.entry(t).or_default().push((
                2,
                Command::Move {
                    unit: 113,
                    x: 64,
                    y: 64,
                },
            ));
        }
    }
    let (h, cs, desyncs) = step_n(&mut host, &mut clients, 10_000).unwrap();
    assert!(desyncs.is_empty(), "desyncs: {desyncs:?}");
    assert_eq!(
        h, cs[0],
        "final hash mismatch host={h:016x} client={:016x}",
        cs[0]
    );
}
