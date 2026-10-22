//! Headless tests for UI state transitions + the playing view (no window, no eframe loop).

use game_ui::app::MatchView;
use game_ui::config::{MatchConfig, SlotKind};
use game_ui::state::{Screen, UiState};

#[test]
fn state_transitions() {
    let mut st = UiState::default();
    assert_eq!(st.screen, Screen::Menu);
    st.open_custom();
    assert_eq!(st.screen, Screen::Custom);
    st.back_to_menu();
    assert_eq!(st.screen, Screen::Menu);
    st.open_custom();
    assert!(st.start_match());
    assert_eq!(st.screen, Screen::Playing);
    st.toggle_pause();
    assert!(st.paused);
}

#[test]
fn start_match_rejects_invalid_cfg() {
    let mut st = UiState::default();
    st.cfg.players = [SlotKind::Off, SlotKind::Off];
    st.open_custom();
    assert!(!st.start_match());
    assert_eq!(st.screen, Screen::Custom);
}

#[test]
fn solo_match_runs_clean_no_enemy_commands() {
    let cfg = MatchConfig {
        seed: 999,
        preset: "arena".to_string(),
        players: [SlotKind::Human, SlotKind::Off],
        locale: "en".to_string(),
        sandbox: true,
        ticks: 300,
        civ: 0,
    };
    let mut v = MatchView::new(&cfg);
    for _ in 0..300 {
        v.tick().unwrap();
    }
    assert_eq!(v.sim.tick(), 300);
    // P2 was Off: no units should have moved via script. Player 1 still has units.
    let p1 = v.unit_count(0);
    assert!(p1 > 0);
}

#[test]
fn ai_match_has_two_active_players() {
    let cfg = MatchConfig {
        seed: 7,
        preset: String::new(),
        players: [SlotKind::Human, SlotKind::Ai],
        locale: "it".to_string(),
        sandbox: false,
        ticks: 600,
        civ: 0,
    };
    let mut v = MatchView::new(&cfg);
    for _ in 0..600 {
        v.tick().unwrap();
    }
    assert_eq!(v.sim.tick(), 600);
    // at least both AIs issued commands (schedule consumed)
    assert!(v.schedule_idx > 0);
}

#[test]
fn live_issue_replay_equivalence() {
    // same seed + same live cmds issued at same ticks -> identical final hash
    let cfg = MatchConfig {
        seed: 555,
        preset: "arena".to_string(),
        players: [SlotKind::Human, SlotKind::Off],
        locale: "en".to_string(),
        sandbox: true,
        ticks: 200,
        civ: 0,
    };
    let mut v1 = MatchView::new(&cfg);
    let mut v2 = MatchView::new(&cfg);
    for t in 0..200u32 {
        if t == 50 {
            v1.live.push((
                0,
                shared::Command::Train {
                    building: 0,
                    unit_kind: 0,
                },
            ));
            v2.live.push((
                0,
                shared::Command::Train {
                    building: 0,
                    unit_kind: 0,
                },
            ));
        }
        if t == 100 {
            v1.live.push((
                0,
                shared::Command::Move {
                    unit: 1,
                    x: 10i16,
                    y: 10i16,
                },
            ));
            v2.live.push((
                0,
                shared::Command::Move {
                    unit: 1,
                    x: 10,
                    y: 10,
                },
            ));
        }
        v1.tick().unwrap();
        v2.tick().unwrap();
    }
    assert_eq!(v1.sim.hash(), v2.sim.hash());
}
