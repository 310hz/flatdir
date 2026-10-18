use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::candidates::{self, Entry};
use crate::context::Context;
use crate::guard;
use crate::select::{SelectOpts, pick};

fn rows(entries: &[Entry]) -> Vec<Vec<String>> {
    entries
        .iter()
        .map(|e| vec![e.name.clone(), e.path.to_string_lossy().into_owned()])
        .collect()
}

pub fn select(ctx: &Context) -> Result<Option<PathBuf>> {
    let entries = candidates::candidates(ctx)?;
    if entries.is_empty() {
        bail!("no directories found (did you run: flatdir track <path> ?)");
    }
    let picked = pick(ctx, &rows(&entries), &SelectOpts::dirs(false))?;
    Ok(picked.first().map(|&i| entries[i].path.clone()))
}

pub fn select_archived(ctx: &Context) -> Result<Option<PathBuf>> {
    let entries = candidates::archived(ctx)?;
    if entries.is_empty() {
        bail!("no archived directories found");
    }
    let picked = pick(ctx, &rows(&entries), &SelectOpts::dirs(false))?;
    Ok(picked.first().map(|&i| entries[i].path.clone()))
}

/// Multi-select for destructive commands. Cancelling or selecting everything is an error.
pub(super) fn choose_many(ctx: &Context, entries: Vec<Entry>) -> Result<Vec<Entry>> {
    let picked = pick(ctx, &rows(&entries), &SelectOpts::dirs(true))?;
    if picked.is_empty() {
        bail!("no selection");
    }
    guard::reject_select_all(entries.len(), picked.len())?;
    Ok(picked.into_iter().map(|i| entries[i].clone()).collect())
}
