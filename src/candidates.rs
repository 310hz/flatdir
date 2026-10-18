use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

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

/// Children of the managed root, ordered by descending zoxide score.
pub fn candidates(ctx: &Context) -> Result<Vec<Entry>> {
    let mut entries = list_children(ctx.root()?)?;
    let scores = ctx.scores.scores();
    let score = |e: &Entry| scores.get(&e.path).copied().unwrap_or(0.0);
    entries.sort_by(|a, b| score(b).partial_cmp(&score(a)).unwrap_or(Ordering::Equal));
    Ok(entries)
}

pub fn archived(ctx: &Context) -> Result<Vec<Entry>> {
    let base = ctx.archive_base()?;
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    list_children(&base)
}
