//! Headless lockstep relay demo: cargo run -p server -- --seed 7 --ticks 600
//!
//! Emits synthetic Move batches, ticks the sim, prints the final state hash and confirms an
//! identical replay produces an identical hash.

use game::mapgen::preset_by_name;
use game::run_sim;
use shared::netcode::invite_code;
use shared::netcode::NetMsg;
use shared::{Command, TickCommands};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--host") {
        let addr = parse_str_arg(&args, "--addr").unwrap_or_else(|| "127.0.0.1:4738".into());
        let seed = parse_arg(&args, "--seed").unwrap_or(7);
        let ticks = parse_arg(&args, "--ticks").unwrap_or(60);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(host_main(addr, seed, ticks));
        return;
    }
    if args.iter().any(|a| a == "--join") {
        let addr = parse_str_arg(&args, "--addr").unwrap_or_else(|| "127.0.0.1:4738".into());
        let seed = parse_arg(&args, "--seed").unwrap_or(7);
        let ticks = parse_arg(&args, "--ticks").unwrap_or(60);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(client_main(addr.parse().unwrap(), seed, ticks));
        return;
    }
    let seed = parse_arg(&args, "--seed").unwrap_or(7);
    let ticks = parse_arg(&args, "--ticks").unwrap_or(60);

    // Batches are player-agnostic: run_sim wraps them as player 0, step canonicalizes.
    let batches: Vec<TickCommands> = (0..ticks)
        .map(|t| {
            let mut b = TickCommands {
                tick: t,
                cmds: Vec::new(),
            };
            if t % 20 == 5 {
                b.cmds.push((
                    0,
                    Command::Move {
                        unit: 3,
                        x: 20,
                        y: 20,
                    },
                ));
            }
            if t % 20 == 15 {
                b.cmds.push((
                    0,
                    Command::Move {
                        unit: 4,
                        x: 40,
                        y: 40,
                    },
                ));
            }
            b
        })
        .collect();
    let map = preset_by_name(&parse_str_arg(&args, "--map").unwrap_or_default());
    let run = |seed: u64| -> u64 {
        match map {
            Some(p) => game::mapgen::run_preset(seed, ticks, &batches, *p),
            None => run_sim(seed, ticks, &batches),
        }
    };
    let h1 = run(seed);
    let h2 = run(seed);
    println!(
        "seed={} ticks={} map={} final_hash={:016x}",
        seed,
        ticks,
        map.map(|p| p.name).unwrap_or("legacy"),
        h1
    );
    assert_eq!(h1, h2, "replay mismatch");
    println!("replay identical: OK");
}

fn parse_str_arg(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)? + 1;
    args.get(i)?.clone().into()
}

fn parse_arg(args: &[String], flag: &str) -> Option<u64> {
    let i = args.iter().position(|a| a == flag)? + 1;
    args.get(i)?.parse().ok()
}

async fn host_main(addr: String, seed: u64, ticks: u64) {
    let (tx, rx) = net::event_channel();
    let host = std::sync::Arc::new(std::sync::Mutex::new(net::HostSession::new(
        net::SessionCfg {
            name: "host".into(),
            seed,
            preset: String::new(),
            is_host: true,
        },
        tx,
    )));
    let endpoint = net::bind_host(&addr).expect("bind");
    println!("host listening on {addr} lobby_code={}", invite_code(seed));

    {
        let h = &mut *host.lock().unwrap();
        h.handle_msg(Some(1), None, NetMsg::Start);
    }
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let accept = tokio::spawn(net::host_accept_loop(
        endpoint,
        net::SendSession::wrap(std::sync::Arc::clone(&host)),
        stop_rx.clone(),
    ));
    let tick = tokio::spawn(net::host_tick_loop(
        net::SendSession::wrap(std::sync::Arc::clone(&host)),
        stop_rx,
    ));

    let mut seen_desync = false;
    for _ in 0..ticks {
        tokio::time::sleep(std::time::Duration::from_millis(net::TICK_MS)).await;
        if let net::SessionEvent::Desync {
            tick,
            expected,
            got,
        } = &*rx.borrow()
        {
            if !seen_desync {
                println!("desync tick={tick} expected={expected:016x} got={got:016x}");
                seen_desync = true;
            }
        }
    }
    drop(rx);
    let final_hash = host.lock().unwrap().sim.hash();
    let _ = stop_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), accept).await;
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), tick).await;
    println!("seed={seed} ticks={ticks} final_hash={final_hash:016x} HOST_DONE");
}

async fn client_main(addr: std::net::SocketAddr, seed: u64, ticks: u64) {
    let (tx, rx) = net::event_channel();
    let client = net::ClientSession::new(
        net::SessionCfg {
            name: "client".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        tx,
    );
    let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut handle = tokio::spawn(net::client_run(
        addr,
        net::SessionCfg {
            name: "client".into(),
            seed,
            preset: String::new(),
            is_host: false,
        },
        client,
        tokio::sync::mpsc::unbounded_channel().1,
        stop_rx,
    ));
    let mut last = 0u64;
    let mut seen_desync = false;
    for _ in 0..ticks + 15 {
        tokio::time::sleep(std::time::Duration::from_millis(net::TICK_MS)).await;
        if let net::SessionEvent::AppliedBatch { hash, .. } = &*rx.borrow() {
            last = *hash;
        }
        if let net::SessionEvent::Desync {
            tick,
            expected,
            got,
        } = &*rx.borrow()
        {
            if !seen_desync {
                println!("client desync tick={tick} expected={expected:016x} got={got:016x}");
                seen_desync = true;
            }
        }
    }
    match tokio::time::timeout(std::time::Duration::from_millis(1500), &mut handle).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Err(e)) => println!("client error: {e}"),
        _ => handle.abort(),
    }
    println!("seed={seed} ticks={ticks} final_hash={last:016x} CLIENT_DONE");
}
