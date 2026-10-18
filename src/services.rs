use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};

/// Boundary for side effects (external commands, prompts). Replaced by fakes in tests.
pub trait Trasher {
    fn trash(&self, path: &Path) -> Result<()>;
}

pub trait Prompter {
    fn confirm(&self, message: &str) -> bool;
}

pub trait ScoreSource {
    fn scores(&self) -> HashMap<PathBuf, f64>;
}

/// Deletion always goes through the `trash` command. If it is missing this fails instead of falling back to `rm`.
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

/// Reads from /dev/tty rather than stdin, so prompting works even under a shell function that captures stdout.
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
    /// Fetches every score in a single call; querying per path would spawn O(N) processes.
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

/// Each line looks like `  12.0 /path with spaces`; everything after the score is the path.
pub fn parse_zoxide(text: &str) -> HashMap<PathBuf, f64> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (score, path) = line.split_once(' ')?;
            Some((PathBuf::from(path.trim_start()), score.parse().ok()?))
        })
        .collect()
}

pub fn command_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
}
