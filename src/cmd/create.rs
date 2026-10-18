use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, anyhow, bail};

use crate::context::Context;
use crate::guard;
use crate::select::{Preview, SelectOpts, pick};

fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let status = cmd
        .status()
        .with_context(|| format!("required command not found: {program}"))?;
    if !status.success() {
        bail!("failed: {program}");
    }
    Ok(())
}

fn capture(cmd: &mut Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .output()
        .with_context(|| format!("required command not found: {program}"))?;
    if !out.status.success() {
        bail!("failed: {program}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn succeeds(cmd: &mut Command) -> bool {
    cmd.output().is_ok_and(|o| o.status.success())
}

/// Only single-level names are allowed, so `..` or `/` cannot create anything outside the root.
fn ensure_plain_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', '\t', '\n']) {
        bail!("invalid name: {name:?}");
    }
    Ok(())
}

/// Creating something with an archived name would make that archive impossible to restore.
fn ensure_no_archive_conflict(ctx: &Context, name: &str, what: &str) -> Result<()> {
    let conflict = ctx.archive_base()?.join(name);
    if conflict.exists() {
        bail!(
            "cannot {what}: archived path exists: {}",
            conflict.display()
        );
    }
    Ok(())
}

pub fn init(ctx: &Context, name: &str, git: bool) -> Result<PathBuf> {
    ensure_plain_name(name)?;
    let dest = ctx.root()?.join(name);
    if dest.exists() {
        bail!("already exists: {}", dest.display());
    }
    ensure_no_archive_conflict(ctx, name, "init")?;

    if !git {
        fs::create_dir_all(&dest)?;
        return Ok(dest);
    }

    run(Command::new("gh").args(["repo", "create", name, "--private"]))?;
    let ssh_url = capture(
        Command::new("gh").args(["repo", "view", name, "--json", "sshUrl", "-q", ".sshUrl"]),
    )?;
    if ssh_url.is_empty() {
        bail!("failed: could not determine repo sshUrl");
    }
    run(Command::new("git").args(["clone", &ssh_url]).arg(&dest))?;

    let git_in = |args: &[&str]| run(Command::new("git").args(args).current_dir(&dest));
    git_in(&["checkout", "-B", "main"])?;
    fs::write(dest.join("README.md"), "")?;
    git_in(&["add", "README.md"])?;
    git_in(&["commit", "-m", "Initial commit"])?;
    git_in(&["push", "-u", "origin", "main"])?;
    Ok(dest)
}

fn clone_search(ctx: &Context, query: &str) -> Result<String> {
    if query.is_empty() {
        bail!("usage: flatdir clone -s <query>");
    }
    let json = capture(Command::new("gh").args([
        "search",
        "repos",
        "--match=name",
        query,
        "--json",
        "fullName,stargazersCount",
        "--limit",
        "50",
    ]))?;
    let repos: Vec<serde_json::Value> = serde_json::from_str(&json)?;
    let rows: Vec<Vec<String>> = repos
        .iter()
        .filter_map(|r| {
            let name = r["fullName"].as_str()?.to_string();
            let stars = r["stargazersCount"].as_u64().unwrap_or(0).to_string();
            (!name.contains(['\t', '\n'])).then_some(vec![name, stars])
        })
        .collect();
    if rows.is_empty() {
        bail!("no repositories found: {query}");
    }
    let opts = SelectOpts {
        multi: false,
        prompt: Some("clone> ".to_string()),
        height_percent: 60,
        preview: Some(Preview::Clone),
    };
    guard::require_interactive(ctx, "clone -s")?;
    let picked = pick(ctx, &rows, &opts)?;
    let i = *picked.first().ok_or_else(|| anyhow!("no selection"))?;
    Ok(format!("git@github.com:{}.git", rows[i][0]))
}

/// Accepts a full git URL, `owner/repo`, or `repo` (your own account). Falls back to search when not found.
fn resolve_url(ctx: &Context, target: &str) -> Result<String> {
    if target.contains("://") || target.starts_with("git@") {
        return Ok(target.to_string());
    }
    let bare = target.strip_suffix(".git").unwrap_or(target);
    let full = if bare.contains('/') {
        bare.to_string()
    } else {
        let login = capture(Command::new("gh").args(["api", "user", "-q", ".login"]))?;
        if login.is_empty() {
            bail!("failed: could not determine github login");
        }
        format!("{login}/{bare}")
    };
    if succeeds(Command::new("gh").args(["repo", "view", &full])) {
        Ok(format!("git@github.com:{full}.git"))
    } else {
        eprintln!("repository not found: {full} — searching...");
        clone_search(ctx, target)
    }
}

fn repo_name(url: &str) -> Result<String> {
    let last = url.rsplit(['/', ':']).next().unwrap_or("");
    let name = last.strip_suffix(".git").unwrap_or(last);
    ensure_plain_name(name).map_err(|_| anyhow!("failed to derive repo name from: {url}"))?;
    Ok(name.to_string())
}

pub fn clone(ctx: &Context, search: bool, target: Option<&str>) -> Result<PathBuf> {
    let root = ctx.root()?.to_path_buf();
    let url = match (search, target) {
        (true, query) => clone_search(ctx, query.unwrap_or(""))?,
        (false, Some(t)) => resolve_url(ctx, t)?,
        (false, None) => bail!("usage: flatdir clone [-s] [<git-url | owner/repo | repo>]"),
    };

    let name = repo_name(&url)?;
    let dest = root.join(&name);
    ensure_no_archive_conflict(ctx, &name, "clone")?;

    if dest.exists() {
        overwrite_existing(ctx, &dest)?;
    }
    run(Command::new("git")
        .args(["clone", "--"])
        .arg(&url)
        .arg(&dest))?;
    Ok(dest)
}

/// Replacing an existing directory deletes it, so it is sent to `trash` only after confirmation on an interactive terminal.
fn overwrite_existing(ctx: &Context, dest: &Path) -> Result<()> {
    if !ctx.interactive {
        bail!(
            "destination exists: {} (not overwritten non-interactively)",
            dest.display()
        );
    }
    if !ctx.prompter.confirm(&format!(
        "destination exists. overwrite '{}'?",
        dest.display()
    )) {
        bail!("cancelled");
    }
    ctx.trasher.trash(dest)
}
