use std::path::PathBuf;

use anyhow::{Context as _, Result};

/// Gathers every real-environment path in one place so tests can inject throwaway paths and never touch the real HOME.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    pub config_path: PathBuf,
    /// Which directories are archived (`archive::State`).
    pub state_path: PathBuf,
}

impl Env {
    pub fn from_process() -> Result<Self> {
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
        let xdg = |key: &str, default: &str| {
            std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(default))
        };
        let config_home = xdg("XDG_CONFIG_HOME", ".config");
        let state_home = xdg("XDG_STATE_HOME", ".local/state");
        Ok(Self {
            config_path: config_home.join("flatdir").join("config.toml"),
            state_path: state_home.join("flatdir").join("state.json"),
            home,
        })
    }
}
