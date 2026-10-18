use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};

use super::select::choose_many;
use crate::candidates::{self, Entry};
use crate::context::Context;
use crate::fsops::move_dir;
use crate::guard::{ensure_child_of, ensure_plain_dir, require_interactive};
use crate::paths::LEGACY_ARCHIVE_MARKER;

/// アーカイブ後に削除を提案する、再生成可能なもの。
const CLEANUP_CANDIDATES: &[&str] = &[
    ".DS_Store",
    ".venv",
    "node_modules",
    "__pycache__",
    ".ipynb_checkpoints",
];

fn candidates_or_bail(ctx: &Context) -> Result<Vec<Entry>> {
    let entries = candidates::candidates(ctx)?;
    if entries.is_empty() {
        bail!("no directories found (did you run: flatdir track <path> ?)");
    }
    Ok(entries)
}

pub fn remove(ctx: &Context) -> Result<()> {
    require_interactive(ctx, "remove")?;
    let root = ctx.root()?.to_path_buf();
    let targets = choose_many(ctx, candidates_or_bail(ctx)?)?;

    for t in &targets {
        ensure_child_of(&root, &t.path)?;
        ensure_plain_dir(&t.path)?;
    }
    for t in &targets {
        ctx.trasher.trash(&t.path)?;
        eprintln!("removed: {}", t.path.display());
    }
    Ok(())
}

pub fn archive(ctx: &Context) -> Result<()> {
    require_interactive(ctx, "archive")?;
    let root = ctx.root()?.to_path_buf();
    let base = ctx.archive_base()?;
    let targets = choose_many(ctx, candidates_or_bail(ctx)?)?;

    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
    for t in &targets {
        ensure_child_of(&root, &t.path)?;
        ensure_plain_dir(&t.path)?;
        let dest = base.join(&t.name);
        if dest.exists() {
            bail!("archive destination exists: {}", dest.display());
        }
        plan.push((t.path.clone(), dest));
    }

    fs::create_dir_all(&base)?;
    for (src, dest) in plan {
        move_dir(&src, &dest)?;

        for name in CLEANUP_CANDIDATES {
            let p = dest.join(name);
            if p.symlink_metadata().is_ok()
                && ctx.prompter.confirm(&format!("delete '{}'?", p.display()))
            {
                ctx.trasher.trash(&p)?;
            }
        }
        eprintln!("archived: {}", dest.display());
    }
    Ok(())
}

pub fn restore(ctx: &Context) -> Result<()> {
    require_interactive(ctx, "restore")?;
    let root = ctx.root()?.to_path_buf();
    let base = ctx.archive_base()?;
    let archived = candidates::archived(ctx)?;
    if archived.is_empty() {
        bail!("no archived directories found");
    }
    let targets = choose_many(ctx, archived)?;

    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
    for t in &targets {
        ensure_child_of(&base, &t.path)?;
        ensure_plain_dir(&t.path)?;
        let dest = root.join(&t.name);
        if dest.exists() {
            bail!("restore destination exists: {}", dest.display());
        }
        plan.push((t.path.clone(), dest));
    }

    for (src, dest) in plan {
        move_dir(&src, &dest)?;
        // 旧実装が置いた目印は不要になったので、復元と同時に取り除く。
        match fs::remove_file(dest.join(LEGACY_ARCHIVE_MARKER)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        eprintln!("restored: {}", dest.display());
    }
    Ok(())
}
