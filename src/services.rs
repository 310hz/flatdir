use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};

/// 外部コマンド・対話といった副作用の境界。テストでは偽物に差し替える。
pub trait Trasher {
    fn trash(&self, path: &Path) -> Result<()>;
}

pub trait Prompter {
    fn confirm(&self, message: &str) -> bool;
}

pub trait ScoreSource {
    fn scores(&self) -> HashMap<PathBuf, f64>;
}

/// 削除は必ず `trash` コマンド経由。無ければ `rm` にフォールバックせずエラーにする。
pub struct TrashCommand;

impl Trasher for TrashCommand {
    fn trash(&self, path: &Path) -> Result<()> {
        let status = Command::new("trash")
            .arg(path)
            .status()
            .context("required command not found: trash")?;
        if !status.success() {
            bail!("failed: trash {}", path.display());
        }
        Ok(())
    }
}

/// stdin ではなく /dev/tty から読む。出力を取り込むシェル関数の下でも対話できる。
pub struct TtyPrompter;

impl Prompter for TtyPrompter {
    fn confirm(&self, message: &str) -> bool {
        let Ok(tty) = OpenOptions::new().read(true).write(true).open("/dev/tty") else {
            return false;
        };
        let mut out = &tty;
        if write!(out, "{message} [y/N] ")
            .and_then(|()| out.flush())
            .is_err()
        {
            return false;
        }
        let mut answer = String::new();
        BufReader::new(&tty).read_line(&mut answer).is_ok() && answer.trim() == "y"
    }
}

pub struct Zoxide;

impl ScoreSource for Zoxide {
    /// 1 回の呼び出しで全スコアを取る。パスごとに呼ぶと O(N) 回プロセスを起動することになる。
    fn scores(&self) -> HashMap<PathBuf, f64> {
        let Ok(out) = Command::new("zoxide")
            .args(["query", "-ls"])
            .stderr(Stdio::null())
            .output()
        else {
            return HashMap::new();
        };
        parse_zoxide(&String::from_utf8_lossy(&out.stdout))
    }
}

/// 各行は `  12.0 /path with spaces`。スコアの後ろ全体をパスとして扱う。
pub fn parse_zoxide(text: &str) -> HashMap<PathBuf, f64> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (score, path) = line.split_once(' ')?;
            Some((PathBuf::from(path.trim_start()), score.parse().ok()?))
        })
        .collect()
}
