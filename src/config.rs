use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    pub root: Option<PathBuf>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
        };
        toml::from_str(&text).with_context(|| format!("invalid config: {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("config path has no parent")?;
        fs::create_dir_all(dir)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, toml::to_string(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}
