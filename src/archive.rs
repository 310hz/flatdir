//! Archiving only hides a directory from the list; it stays in the root. Moving it out changed
//! its path, which broke editors, zoxide, virtualenvs and open shells.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// Written only by flatdir, so it is rewritten whole every time.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    /// Keyed by absolute path, so archives of another root never hide a same-named directory here.
    #[serde(default)]
    pub archived: BTreeMap<PathBuf, Record>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Unix seconds; a formatted date would need a date crate.
    pub archived_at: u64,
}

impl Record {
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self { archived_at: secs }
    }
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
        };
        serde_json::from_str(&text).with_context(|| format!("invalid state: {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("state path has no parent")?;
        fs::create_dir_all(dir)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)? + "\n")?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}
