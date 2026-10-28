use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::context::Context;

/// What `set_root` did.
#[derive(Debug, PartialEq, Eq)]
pub struct RootChange {
    pub root: PathBuf,
    /// The root that was replaced, if there was a different one.
    pub previous: Option<PathBuf>,
    /// `false` when the given path already was the root.
    pub changed: bool,
}

/// Sets the managed root, replacing any previous one. Archive records of the old root are kept and
/// apply again if it is set back.
pub fn set_root(ctx: &mut Context, path: &Path) -> Result<RootChange> {
    let abs =
        fs::canonicalize(path).with_context(|| format!("not a directory: {}", path.display()))?;
    if !abs.is_dir() {
        bail!("not a directory: {}", path.display());
    }

    // Even if HOME itself is reached through a symlink, store the path under env.home as spelled, so
    // it keeps matching the archive state's keys and the `~/` shortening on screen.
    let home = fs::canonicalize(&ctx.env.home)?;
    let rel = match abs.strip_prefix(&home) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_path_buf(),
        _ => bail!(
            "managed root must be under {}: {}",
            ctx.env.home.display(),
            abs.display()
        ),
    };
    let root = ctx.env.home.join(rel);

    let previous = ctx.config.root.clone();
    if previous.as_ref() == Some(&root) {
        return Ok(RootChange {
            root,
            previous: None,
            changed: false,
        });
    }
    ctx.config.root = Some(root.clone());
    ctx.config.save(&ctx.env.config_path)?;
    Ok(RootChange {
        root,
        previous,
        changed: true,
    })
}

/// Clears the managed root and returns the one that was set.
pub fn unset_root(ctx: &mut Context) -> Result<PathBuf> {
    let root = ctx.root()?.to_path_buf();
    ctx.config.root = None;
    ctx.config.save(&ctx.env.config_path)?;
    Ok(root)
}
