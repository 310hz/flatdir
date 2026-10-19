use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};

use crate::create::{Hit, Hub};

/// Boundary for side effects (external commands, prompts). Replaced by fakes in tests.
pub trait Trasher {
    fn trash(&self, path: &Path) -> Result<()>;
}

pub trait Opener {
    fn open(&self, path: &Path) -> Result<()>;
}

/// Creating and cloning repositories, and searching for them. Runs on a worker thread.
pub trait Forge: Send + Sync {
    /// A private GitHub repository named `name`, cloned to `dest` with a first commit pushed.
    fn new_github_repo(&self, name: &str, dest: &Path) -> Result<()>;
    fn clone_repo(&self, url: &str, dest: &Path, skip_lfs: bool) -> Result<()>;
    fn search(&self, hub: Hub, query: &str) -> Result<Vec<Hit>>;
}

pub trait ScoreSource: Send + Sync {
    fn scores(&self) -> HashMap<PathBuf, f64>;
}

/// Moves to the OS trash through the `trash` crate. There is deliberately no `rm` fallback.
pub struct SystemTrash;

impl Trasher for SystemTrash {
    fn trash(&self, path: &Path) -> Result<()> {
        #[cfg(target_os = "macos")]
        let result = {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            let mut ctx = trash::TrashContext::default();
            // The default (Finder via osascript) needs extra permissions and makes a sound.
            ctx.set_delete_method(DeleteMethod::NsFileManager);
            ctx.delete(path)
        };
        #[cfg(not(target_os = "macos"))]
        let result = trash::delete(path);

        result.with_context(|| format!("failed to move to trash: {}", path.display()))
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

/// Plays the Finder "move to trash" sound on macOS. Best effort and silent on failure.
pub fn play_trash_sound() {
    const SOUND: &str = "/System/Library/Components/CoreAudio.component/Contents/SharedSupport/SystemSounds/finder/move to trash.aif";
    if cfg!(target_os = "macos") && Path::new(SOUND).is_file() {
        let _ = Command::new("afplay")
            .arg(SOUND)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Opens a directory in the system file manager (Finder on macOS).
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open(&self, path: &Path) -> Result<()> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut child = Command::new(program)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to run {program}"))?;
        // Reap it in the background so a slow file manager never blocks the TUI.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// `gh`, `git` and `curl`, with their output captured: the TUI owns the terminal.
pub struct SystemForge;

const SEARCH_LIMIT: &str = "30";

impl Forge for SystemForge {
    fn new_github_repo(&self, name: &str, dest: &Path) -> Result<()> {
        quiet(Command::new("gh").args(["repo", "create", name, "--private"]))?;
        let ssh_url = quiet(
            Command::new("gh").args(["repo", "view", name, "--json", "sshUrl", "-q", ".sshUrl"]),
        )?;
        if ssh_url.is_empty() {
            bail!("could not determine the sshUrl of {name}");
        }
        quiet(git().args(["clone", "--"]).arg(&ssh_url).arg(dest))?;
        let git_in = |args: &[&str]| quiet(git().args(args).current_dir(dest));
        git_in(&["checkout", "-B", "main"])?;
        fs::write(dest.join("README.md"), "")?;
        git_in(&["add", "README.md"])?;
        git_in(&["commit", "-m", "Initial commit"])?;
        git_in(&["push", "-u", "origin", "main"])?;
        Ok(())
    }

    fn clone_repo(&self, url: &str, dest: &Path, skip_lfs: bool) -> Result<()> {
        let mut cmd = git();
        if skip_lfs {
            cmd.env("GIT_LFS_SKIP_SMUDGE", "1");
        }
        quiet(cmd.args(["clone", "--"]).arg(url).arg(dest))?;
        Ok(())
    }

    fn search(&self, hub: Hub, query: &str) -> Result<Vec<Hit>> {
        let kind = match hub {
            Hub::GitHub => {
                let json = quiet(Command::new("gh").args([
                    "search",
                    "repos",
                    "--match=name",
                    "--json",
                    "fullName,stargazersCount,description",
                    "--limit",
                    SEARCH_LIMIT,
                    "--",
                    query,
                ]))?;
                return parse_github_hits(&json);
            }
            Hub::HfModel => "models",
            Hub::HfDataset => "datasets",
            Hub::HfSpace => "spaces",
        };
        // The Hub API is public for search; curl does the URL encoding of the query.
        let json = quiet(
            Command::new("curl")
                .args(["-sSfL", "--max-time", "20", "-G"])
                .arg(format!("https://huggingface.co/api/{kind}"))
                .args(["--data-urlencode"])
                .arg(format!("search={query}"))
                .args(["-d", "sort=likes", "-d", "direction=-1", "-d"])
                .arg(format!("limit={SEARCH_LIMIT}")),
        )
        .with_context(|| format!("Hugging Face {kind} search failed"))?;
        parse_hf_hits(&json)
    }
}

pub fn parse_github_hits(json: &str) -> Result<Vec<Hit>> {
    let repos: Vec<serde_json::Value> = serde_json::from_str(json)?;
    Ok(repos
        .iter()
        .filter_map(|r| {
            Some(Hit {
                id: r["fullName"].as_str()?.to_string(),
                meta: format!("{} stars", r["stargazersCount"].as_u64().unwrap_or(0)),
                description: r["description"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect())
}

pub fn parse_hf_hits(json: &str) -> Result<Vec<Hit>> {
    let repos: Vec<serde_json::Value> = serde_json::from_str(json)?;
    Ok(repos
        .iter()
        .filter_map(|r| {
            Some(Hit {
                id: r["id"].as_str()?.to_string(),
                meta: format!("{} likes", r["likes"].as_u64().unwrap_or(0)),
                description: String::new(),
            })
        })
        .collect())
}

fn git() -> Command {
    let mut cmd = Command::new("git");
    // A password or passphrase prompt would draw over the TUI and wait forever; fail instead.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    cmd
}

/// Runs without a terminal and returns stdout; on failure the error carries stderr's last line.
fn quiet(cmd: &mut Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("{program} is not installed"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let last = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty());
        bail!("{program} failed: {}", last.unwrap_or("(no message)"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
