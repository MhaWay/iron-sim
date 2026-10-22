//! File diagnostics: every stage of startup / drawing writes a line to
//! iron-sim-diag.log next to the executable (fallback: current directory).
//! Purpose: black-window reports are impossible to debug remotely without a log.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn log_path() -> PathBuf {
    if let Ok(mut p) = std::env::current_exe() {
        p.pop();
        p.push("iron-sim-diag.log");
        if std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .is_ok()
        {
            return p;
        }
    }
    PathBuf::from("iron-sim-diag.log")
}

fn line_at(p: &std::path::Path, msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(f, "[{ts}] {msg}");
        let _ = f.flush();
    }
}

pub fn line(msg: &str) {
    line_at(&log_path(), msg);
}

/// Call once at process start, before eframe. Installs a panic hook that writes
/// panics into the log and records the banner.
pub fn init(bin: &str) {
    let banner = format!(
        "--- {bin} v{} startup args={:?} cwd={:?} ---",
        env!("CARGO_PKG_VERSION"),
        std::env::args().collect::<Vec<_>>(),
        std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
    );
    line(&banner);
    let path = log_path();
    std::panic::set_hook(Box::new(move |info| {
        line_at(&path, &format!("PANIC: {info}"));
    }));
}
