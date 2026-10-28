//! Operations on already-chosen directories. Selection and confirmation live elsewhere (the TUI),
//! so every caller goes through the same validation: all targets are checked before any is touched.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::archive::{Record, State};
use crate::candidates::Entry;
use crate::context::Context;
use crate::guard::{ensure_child_of, ensure_plain_dir};

/// Checks every target before anything changes, and returns the state as it is on disk now.
fn validate(ctx: &Context, targets: &[Entry], archived: bool) -> Result<State> {
    let root = ctx.root()?;
    let state = State::load(&ctx.env.state_path)?;
    for t in targets {
        ensure_child_of(root, &t.path)?;
        ensure_plain_dir(&t.path)?;
        match (archived, state.archived.contains_key(&t.path)) {
            (true, false) => bail!("not archived: {}", t.path.display()),
            (false, true) => bail!("already archived: {}", t.path.display()),
            _ => {}
        }
    }
    Ok(state)
}

pub fn archive(ctx: &Context, targets: &[Entry]) -> Result<Vec<String>> {
    let mut state = validate(ctx, targets, false)?;
    for t in targets {
        state.archived.insert(t.path.clone(), Record::now());
    }
    state.save(&ctx.env.state_path)?;
    Ok(targets.iter().map(|t| t.name.clone()).collect())
}

pub fn restore(ctx: &Context, targets: &[Entry]) -> Result<Vec<String>> {
    let mut state = validate(ctx, targets, true)?;
    for t in targets {
        state.archived.remove(&t.path);
    }
    state.save(&ctx.env.state_path)?;
    Ok(targets.iter().map(|t| t.name.clone()).collect())
}

/// Works on both lists; trashed archives are dropped from the state as well.
pub fn remove(ctx: &Context, targets: &[Entry]) -> Result<Vec<String>> {
    let root = ctx.root()?;
    for t in targets {
        ensure_child_of(root, &t.path)?;
        ensure_plain_dir(&t.path)?;
    }
    let mut done = Vec::new();
    for t in targets {
        ctx.trasher.trash(&t.path)?;
        done.push(t.name.clone());
    }
    forget(
        ctx,
        &targets.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
    )?;
    Ok(done)
}

/// Drops archived paths that no longer exist. Writes nothing when none of them is recorded.
pub fn forget(ctx: &Context, paths: &[PathBuf]) -> Result<()> {
    let mut state = State::load(&ctx.env.state_path)?;
    let before = state.archived.len();
    for p in paths {
        state.archived.remove(p);
    }
    if state.archived.len() == before {
        return Ok(());
    }
    state.save(&ctx.env.state_path)
}
