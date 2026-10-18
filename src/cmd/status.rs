use std::fmt;
use std::path::PathBuf;

use crate::candidates;
use crate::context::Context;
use crate::services::command_exists;

pub struct Tool {
    pub name: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    pub available: bool,
}

pub struct RootStatus {
    pub path: PathBuf,
    /// `None` when the root cannot be read (missing or not a directory).
    pub directories: Option<usize>,
    pub archive_dir: PathBuf,
    pub archived: usize,
}

pub struct Status {
    pub config_path: PathBuf,
    pub fzf_opts: Vec<String>,
    pub root: Option<RootStatus>,
    pub tools: Vec<Tool>,
}

const TOOLS: &[(&str, &str, bool)] = &[
    ("fzf", "selection UI", true),
    ("trash", "remove, archive cleanup, clone overwrite", true),
    ("zoxide", "order by frecency", false),
    ("eza", "directory preview", false),
    ("rich", "README preview", false),
    ("gh", "init --git, clone", false),
    ("git", "init --git, clone", false),
];

/// Read-only; never fails, so a broken setup is still reported.
pub fn status(ctx: &Context) -> Status {
    let root = ctx.config.root.as_ref().map(|path| RootStatus {
        path: path.clone(),
        directories: candidates::list_children(path).ok().map(|v| v.len()),
        archive_dir: ctx.archive_base().unwrap_or_default(),
        archived: candidates::archived(ctx).map(|v| v.len()).unwrap_or(0),
    });
    Status {
        config_path: ctx.env.config_path.clone(),
        fzf_opts: ctx.config.fzf_opts.clone(),
        root,
        tools: TOOLS
            .iter()
            .map(|&(name, purpose, required)| Tool {
                name,
                purpose,
                required,
                available: command_exists(name),
            })
            .collect(),
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.root {
            None => writeln!(f, "root:        (not set; run: flatdir track <path>)")?,
            Some(r) => {
                writeln!(f, "root:        {}", r.path.display())?;
                match r.directories {
                    Some(n) => writeln!(f, "directories: {n}")?,
                    None => writeln!(f, "directories: (root is not a readable directory)")?,
                }
                writeln!(f, "archive:     {}", r.archive_dir.display())?;
                writeln!(f, "archived:    {}", r.archived)?;
            }
        }
        writeln!(f, "config:      {}", self.config_path.display())?;
        if self.fzf_opts.is_empty() {
            writeln!(f, "fzf_opts:    (none)")?;
        } else {
            writeln!(f, "fzf_opts:    {}", self.fzf_opts.join(" "))?;
        }

        writeln!(f, "\ncommands:")?;
        for t in &self.tools {
            let state = match (t.available, t.required) {
                (true, _) => "ok",
                (false, true) => "MISSING (required)",
                (false, false) => "missing (optional)",
            };
            writeln!(f, "  {:<7} {:<20} {}", t.name, state, t.purpose)?;
        }
        Ok(())
    }
}
