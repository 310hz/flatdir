use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::config::Config;
use crate::paths::Env;
use crate::select::Selector;
use crate::services::{Prompter, ScoreSource, Trasher};

pub struct Context {
    pub env: Env,
    pub config: Config,
    /// 破壊的コマンドは対話端末でのみ実行する。テストが注入できるよう、環境変数ではなく値で持つ。
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

    /// アーカイブ済みディレクトリの置き場。管理 root は 1 つなので、この直下が全アーカイブ。
    pub fn archive_base(&self) -> Result<PathBuf> {
        self.env.archive_dest(self.root()?)
    }
}
