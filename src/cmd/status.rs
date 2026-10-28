use std::fmt;
use std::path::PathBuf;

use crate::candidates;
use crate::context::Context;
use crate::services::command_exists;

pub struct Tool {
    pub name: &'static str,
    pub purpose: &'static str,
    pub available: bool,
}

pub struct RootStatus {
    pub path: PathBuf,
    /// `None` when the root cannot be read (missing or not a directory).
    /// Every directory listed under the root, archived ones included.
    pub directories: Option<usize>,
    pub archived: usize,
}

pub struct Status {
    pub config_path: PathBuf,
    pub state_path: PathBuf,
    pub root: Option<RootStatus>,
    pub tools: Vec<Tool>,
}

/// Everything else is built in; these only enable extra features.
const TOOLS: &[(&str, &str)] = &[
    ("zoxide", "order by frecency"),
    ("git", "Git (private) and clones in the TUI's New view"),
    ("gh", "Git (private) and GitHub search"),
    ("curl", "Hugging Face search"),
    ("git-lfs", "large files of Hugging Face clones"),
];

/// Read-only; never fails, so a broken setup is still reported.
pub fn status(ctx: &Context) -> Status {
    let root = ctx.config.root.as_ref().map(|path| {
        let lists = candidates::lists(ctx).ok();
        RootStatus {
            path: path.clone(),
            directories: lists.as_ref().map(|l| l.active.len() + l.archived.len()),
            archived: lists.map_or(0, |l| l.archived.len()),
        }
    });
    Status {
        config_path: ctx.env.config_path.clone(),
        state_path: ctx.env.state_path.clone(),
        root,
        tools: TOOLS
            .iter()
            .map(|&(name, purpose)| Tool {
                name,
                purpose,
                available: command_exists(name),
            })
            .collect(),
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.root {
            None => writeln!(f, "root:        (not set; run: flatdir root <path>)")?,
            Some(r) => {
                writeln!(f, "root:        {}", r.path.display())?;
                match r.directories {
                    Some(n) => writeln!(f, "directories: {n}")?,
                    None => writeln!(f, "directories: (root is not a readable directory)")?,
                }
                writeln!(f, "archived:    {}", r.archived)?;
            }
        }
        writeln!(f, "config:      {}", self.config_path.display())?;
        writeln!(f, "state:       {}", self.state_path.display())?;

        writeln!(f, "\noptional commands:")?;
        for t in &self.tools {
            let state = if t.available { "ok" } else { "missing" };
            writeln!(f, "  {:<7} {:<8} {}", t.name, state, t.purpose)?;
        }
        Ok(())
    }
}
