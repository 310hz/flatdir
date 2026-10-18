//! Operations on already-chosen directories. Selection and confirmation live elsewhere (the TUI),
//! so every caller goes through the same validation: all targets are checked before any is touched.

use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::candidates::Entry;
use crate::context::Context;
use crate::fsops::move_dir;
use crate::guard::{ensure_child_of, ensure_plain_dir};
use crate::paths::LEGACY_ARCHIVE_MARKER;

/// Regenerable items worth mentioning after archiving. They are reported, never deleted automatically.
const LEFTOVER_CANDIDATES: &[&str] =
    &[".venv", "node_modules", "__pycache__", ".ipynb_checkpoints"];

pub struct Moved {
    pub name: String,
    pub dest: PathBuf,
    /// Names from `LEFTOVER_CANDIDATES` found inside (archive only).
    pub leftovers: Vec<&'static str>,
}

pub fn archive(ctx: &Context, targets: &[Entry]) -> Result<Vec<Moved>> {
    let root = ctx.root()?.to_path_buf();
    let base = ctx.archive_base()?;

    let mut plan = Vec::new();
    for t in targets {
        ensure_child_of(&root, &t.path)?;
        ensure_plain_dir(&t.path)?;
        let dest = base.join(&t.name);
        if dest.exists() {
            bail!("archive destination exists: {}", dest.display());
        }
        plan.push((t, dest));
    }

    fs::create_dir_all(&base)?;
    let mut done = Vec::new();
    for (t, dest) in plan {
        move_dir(&t.path, &dest)?;
        let leftovers = LEFTOVER_CANDIDATES
            .iter()
            .copied()
            .filter(|n| dest.join(n).symlink_metadata().is_ok())
            .collect();
        done.push(Moved {
            name: t.name.clone(),
            dest,
            leftovers,
        });
    }
    Ok(done)
}

pub fn restore(ctx: &Context, targets: &[Entry]) -> Result<Vec<Moved>> {
    let root = ctx.root()?.to_path_buf();
    let base = ctx.archive_base()?;

    let mut plan = Vec::new();
    for t in targets {
        ensure_child_of(&base, &t.path)?;
        ensure_plain_dir(&t.path)?;
        let dest = root.join(&t.name);
        if dest.exists() {
            bail!("restore destination exists: {}", dest.display());
        }
        plan.push((t, dest));
    }

    let mut done = Vec::new();
    for (t, dest) in plan {
        move_dir(&t.path, &dest)?;
        // The old implementation left a marker file; it is obsolete, so remove it while restoring.
        match fs::remove_file(dest.join(LEGACY_ARCHIVE_MARKER)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        done.push(Moved {
            name: t.name.clone(),
            dest,
            leftovers: Vec::new(),
        });
    }
    Ok(done)
}

/// `parent` is the list the targets came from (the root, or the archive base).
pub fn remove(ctx: &Context, targets: &[Entry], parent: &std::path::Path) -> Result<Vec<String>> {
    for t in targets {
        ensure_child_of(parent, &t.path)?;
        ensure_plain_dir(&t.path)?;
    }
    let mut done = Vec::new();
    for t in targets {
        ctx.trasher.trash(&t.path)?;
        done.push(t.name.clone());
    }
    Ok(done)
}
