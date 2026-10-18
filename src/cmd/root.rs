use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::context::Context;

/// 管理 root は 1 つだけ。入れ替えるときは先に untrack する。
/// 戻り値は (登録された root, 新規登録か)。
pub fn track(ctx: &mut Context, path: &Path) -> Result<(PathBuf, bool)> {
    let abs =
        fs::canonicalize(path).with_context(|| format!("not a directory: {}", path.display()))?;
    if !abs.is_dir() {
        bail!("not a directory: {}", path.display());
    }

    // HOME 自体が symlink 経由でも、保存するパスは env.home 基準に揃える(rel_from_home が前提にしている)。
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

    match &ctx.config.root {
        Some(current) if *current == root => return Ok((root, false)),
        Some(current) => {
            bail!(
                "already tracking {} (run: flatdir untrack)",
                current.display()
            )
        }
        None => {}
    }

    ctx.config.root = Some(root.clone());
    ctx.config.save(&ctx.env.config_path)?;
    Ok((root, true))
}

pub fn untrack(ctx: &mut Context) -> Result<PathBuf> {
    let root = ctx.root()?.to_path_buf();
    ctx.config.root = None;
    ctx.config.save(&ctx.env.config_path)?;
    Ok(root)
}

pub fn list(ctx: &Context) -> Option<&Path> {
    ctx.config.root.as_deref()
}
