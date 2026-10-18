use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

/// Marker file the old shell implementation left in archived directories. No longer written, but cleaned up on restore.
pub const LEGACY_ARCHIVE_MARKER: &str = ".flatdir_archived";

/// Gathers every real-environment path in one place so tests can inject throwaway paths and never touch the real HOME.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    pub config_path: PathBuf,
    pub archive_root: PathBuf,
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
        let data_home = xdg("XDG_DATA_HOME", ".local/share");
        Ok(Self {
            config_path: config_home.join("flatdir").join("config.toml"),
            archive_root: data_home.join("flatdir").join("archive"),
            home,
        })
    }

    pub fn rel_from_home(&self, abs: &Path) -> Result<PathBuf> {
        match abs.strip_prefix(&self.home) {
            Ok(rel) if !rel.as_os_str().is_empty() => Ok(rel.to_path_buf()),
            _ => bail!("path is not under HOME: {}", abs.display()),
        }
    }

    pub fn abs_from_home_rel(&self, rel: &Path) -> Result<PathBuf> {
        if rel.as_os_str().is_empty() || rel.is_absolute() {
            bail!("invalid relpath: {}", rel.display());
        }
        Ok(self.home.join(rel))
    }

    /// The archive keeps the HOME-relative path as is (`~/a/b` -> `<archive_root>/a/b`).
    pub fn archive_dest(&self, target: &Path) -> Result<PathBuf> {
        Ok(self.archive_root.join(self.rel_from_home(target)?))
    }
}
