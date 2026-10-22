//! Headless tests for the custom-match config -> schedule pipeline (no window needed).

use game_ui::config::{merge_cmds, MatchConfig, SlotKind};
use shared::Command;

fn cfg() -> MatchConfig {
    MatchConfig {
        seed: 12345,
        preset: String::new(),
        players: [SlotKind::Human, SlotKind::Off],
        locale: "en".to_string(),
        sandbox: true,
        ticks: 1200,
        civ: 0,
    }
}

#[test]
fn default_config_is_valid() {
    assert!(MatchConfig::default().valid());
}

#[test]
fn invalid_when_first_slot_off() {
    let mut c = cfg();
    c.players = [SlotKind::Off, SlotKind::Ai];
    assert!(!c.valid());
}

#[test]
fn invalid_locale_rejected() {
    let mut c = cfg();
    c.locale = "xx".to_string();
    assert!(!c.valid());
}

#[test]
fn ai_slot_keeps_scripted_commands() {
    let mut c = cfg();
    c.players = [SlotKind::Human, SlotKind::Ai];
    let sched = c.schedule();
    let total = sched.iter().map(|b| b.cmds.len()).sum::<usize>();
    assert!(total > 0, "AI player must have scripted commands");
    // every surviving command belongs to player 1 (the Ai slot)
    for b in &sched {
        for (p, _c) in &b.cmds {
            assert_eq!(*p, 1);
        }
    }
}

#[test]
fn sandbox_solo_emits_no_scheduled_commands() {
    let c = cfg(); // Human + Off
    let sched = c.schedule();
    assert!(sched.iter().all(|b| b.cmds.is_empty()));
    assert!(c.player_passive(0) && c.player_passive(1));
}

#[test]
fn schedule_is_deterministic() {
    let mut c1 = cfg();
    let mut c2 = cfg();
    c1.ticks = 600;
    c2.ticks = 600;
    assert_eq!(c1.schedule().len(), c2.schedule().len());
    // compare command payloads: TickCommands doesn't impl PartialEq on Command, so count per tick
    for (a, b) in c1.schedule().iter().zip(c2.schedule().iter()) {
        assert_eq!(a.tick, b.tick);
        assert_eq!(a.cmds.len(), b.cmds.len());
    }
}

#[test]
fn merge_cmds_sorts_canonically() {
    let sched = vec![(1u32, Command::Stop { unit: 9 })];
    let live = vec![
        (
            0u32,
            Command::Move {
                unit: 3,
                x: 1i16,
                y: 1i16,
            },
        ),
        (
            0u32,
            Command::Move {
                unit: 2,
                x: 0i16,
                y: 0i16,
            },
        ),
    ];
    let m = merge_cmds(&sched, &live);
    assert_eq!(m[0].0, 0);
    assert_eq!(m[1].0, 0);
    assert_eq!(m[2].0, 1);
    // within same player: sort key by entity ascending
    assert!(m[0].1.sort_key() < m[1].1.sort_key());
}
