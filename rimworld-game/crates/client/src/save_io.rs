use std::path::{Path, PathBuf};

/// Canonical save name (client-side).
pub const SAVE_PATH: &str = "colony_save.bin";

pub fn save_dir() -> PathBuf {
    dirs_dir()
}

fn dirs_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map(|h| h.join(".iron-sim"))
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn save_path() -> PathBuf {
    dirs_dir().join(SAVE_PATH)
}

pub fn load_path_if_exists() -> Option<PathBuf> {
    let p = save_path();
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

pub fn write_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{e:?}"))?;
    }
    std::fs::write(path, data).map_err(|e| format!("{e:?}"))
}

pub fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{e:?}"))
}
