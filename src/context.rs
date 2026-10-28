use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::config::Config;
use crate::paths::Env;
use crate::services::{Forge, Opener, ScoreSource, Trasher};

pub struct Context {
    pub env: Env,
    pub config: Config,
    /// Destructive commands run only on an interactive terminal. Held as a value, not read from the environment, so tests can inject it.
    pub interactive: bool,
    pub trasher: Box<dyn Trasher>,
    pub opener: Box<dyn Opener>,
    pub forge: Arc<dyn Forge>,
    pub scores: Arc<dyn ScoreSource>,
}

impl Context {
    pub fn root(&self) -> Result<&Path> {
        self.config
            .root
            .as_deref()
            .ok_or_else(|| anyhow!("no managed root (run: flatdir root <path>)"))
    }
}
