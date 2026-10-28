use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::archive::State;
use crate::context::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
}

/// Hidden directories and symlinks are excluded.
/// fzf rows are TAB/newline separated, so names containing either are skipped too (a broken row could point at something else).
pub fn list_children(dir: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("not a directory: {}", dir.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') || name.contains(['\t', '\n']) {
            continue;
        }
        entries.push(Entry {
            name,
            path: entry.path(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// The root's children, split by the archive state.
pub struct Lists {
    pub active: Vec<Entry>,
    pub archived: Vec<Entry>,
    /// Archived paths of this root that are gone (deleted or renamed by hand). Only reported here,
    /// so reading stays free of writes; the TUI forgets them.
    pub missing: Vec<PathBuf>,
}

pub fn lists(ctx: &Context) -> Result<Lists> {
    let root = ctx.root()?;
    let children = list_children(root)?;
    let state = State::load(&ctx.env.state_path)?;
    let (archived, active) = children
        .into_iter()
        .partition::<Vec<_>, _>(|e| state.archived.contains_key(&e.path));
    let missing = state
        .archived
        .keys()
        .filter(|p| p.parent() == Some(root) && !archived.iter().any(|e| &e.path == *p))
        .cloned()
        .collect();
    Ok(Lists {
        active,
        archived,
        missing,
    })
}
