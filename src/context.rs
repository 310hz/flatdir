use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::config::Config;
use crate::paths::Env;
use crate::select::Selector;
use crate::services::{Prompter, ScoreSource, Trasher};

pub struct Context {
    pub env: Env,
    pub config: Config,
    /// Destructive commands run only on an interactive terminal. Held as a value, not read from the environment, so tests can inject it.
    pub interactive: bool,
    pub selector: Box<dyn Selector>,
    pub trasher: Box<dyn Trasher>,
    pub prompter: Box<dyn Prompter>,
    pub scores: Box<dyn ScoreSource>,
}

impl Context {
    pub fn root(&self) -> Result<&Path> {
        self.config
            .root
            .as_deref()
            .ok_or_else(|| anyhow!("no managed root (did you run: flatdir track <path> ?)"))
    }

    /// Where archived directories live. There is a single managed root, so everything directly under this is an archive.
    pub fn archive_base(&self) -> Result<PathBuf> {
        self.env.archive_dest(self.root()?)
    }
}
