use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::context::Context;

/// Destructive commands never run non-interactively (pipes, redirects, automation).
pub fn require_interactive(ctx: &Context, command: &str) -> Result<()> {
    if !ctx.interactive {
        bail!("{command} requires an interactive terminal (stdin and stderr must be terminals)");
    }
    Ok(())
}

/// Rejects selecting every candidate; the last line of defense against accidents like a non-interactive fzf passing everything.
pub fn reject_select_all(total: usize, selected: usize) -> Result<()> {
    if total >= 2 && selected >= total {
        bail!(
            "refusing to act on every candidate ({selected}/{total}); select fewer entries if this is intended"
        );
    }
    Ok(())
}

/// Targets must be direct children of the managed directory.
pub fn ensure_child_of(parent: &Path, path: &Path) -> Result<()> {
    let name_ok = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| !n.is_empty() && !n.starts_with('.'));
    if path.parent() != Some(parent) || !name_ok {
        bail!(
            "{} is not a direct child directory of {}",
            path.display(),
            parent.display()
        );
    }
    Ok(())
}

pub fn ensure_plain_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Ok(()),
        _ => bail!("not a directory: {}", path.display()),
    }
}
