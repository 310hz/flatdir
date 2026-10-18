use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

/// fzf に渡すと選択が自動確定・全件通過になるオプション。破壊的コマンドの事故原因になる。
const FORBIDDEN_FZF_OPTS: &[&str] = &["-f", "--filter", "-1", "--select-1", "-0", "--exit-0"];

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub fzf_opts: Vec<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
        };
        let config: Self =
            toml::from_str(&text).with_context(|| format!("invalid config: {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("config path has no parent")?;
        fs::create_dir_all(dir)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, toml::to_string(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if let Some(opt) = self.fzf_opts.iter().find(|o| is_forbidden_fzf_opt(o)) {
            bail!("fzf_opts に使えないオプション: {opt}");
        }
        Ok(())
    }
}

pub fn is_forbidden_fzf_opt(opt: &str) -> bool {
    FORBIDDEN_FZF_OPTS.contains(&opt.split('=').next().unwrap_or(opt))
}

/// 環境変数 FZF_DEFAULT_OPTS を引き継いでよいか。配色などは活かし、危険なオプションを含むときだけ捨てる。
pub fn fzf_env_is_safe(value: &str) -> bool {
    !value.split_whitespace().any(is_forbidden_fzf_opt)
}
