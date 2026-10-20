fn main() {
    println!("rimworld-game server relay demo");
    let mut relay = server::Relay::default();
    relay.ingest(shared::TickCommands {
        tick: 0,
        cmds: vec![],
    });
    println!("rounds recorded: {}", relay.rounds.len());
}
