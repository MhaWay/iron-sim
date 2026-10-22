//! Interactive client (eframe). Client code is OUT of the sim determinism scope:
//! view types may use f32/f64 — the sim crates must not (see crates/sim AGENTS golden rules).

pub mod app;
pub mod game;
pub mod i18n;
pub mod save;
pub mod save_io;
pub mod state;
