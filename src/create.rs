//! Adding a directory under the root from the TUI: an empty directory, a new private GitHub
//! repository, or a clone from GitHub / Hugging Face.
//!
//! A new kind (say, a project already set up with `uv init`) is a new `Recipe` variant: give it a
//! label and a hint, decide in `plan` what Enter does with the typed text, and add its steps to
//! `run` (or to `Forge` when they call external commands).

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::context::Context;
use crate::services::Forge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hub {
    GitHub,
    HfModel,
    HfDataset,
    HfSpace,
}

impl Hub {
    pub fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::HfModel => "HF Model",
            Self::HfDataset => "HF Dataset",
            Self::HfSpace => "HF Space",
        }
    }

    pub fn is_hugging_face(self) -> bool {
        self != Self::GitHub
    }

    /// `id` is `owner/name`.
    pub fn clone_url(self, id: &str) -> String {
        match self {
            Self::GitHub => format!("git@github.com:{id}.git"),
            Self::HfModel => format!("https://huggingface.co/{id}"),
            Self::HfDataset => format!("https://huggingface.co/datasets/{id}"),
            Self::HfSpace => format!("https://huggingface.co/spaces/{id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    Directory,
    GitHubRepo,
    Clone(Hub),
}

/// The order the TUI cycles through with Tab.
pub const RECIPES: [Recipe; 6] = [
    Recipe::Directory,
    Recipe::GitHubRepo,
    Recipe::Clone(Hub::GitHub),
    Recipe::Clone(Hub::HfModel),
    Recipe::Clone(Hub::HfDataset),
    Recipe::Clone(Hub::HfSpace),
];

impl Recipe {
    pub fn label(self) -> &'static str {
        match self {
            Self::Directory => "Directory",
            Self::GitHubRepo => "Git (private)",
            Self::Clone(Hub::GitHub) => "Clone: GitHub",
            Self::Clone(Hub::HfModel) => "Clone: HF Model",
            Self::Clone(Hub::HfDataset) => "Clone: HF Dataset",
            Self::Clone(Hub::HfSpace) => "Clone: HF Space",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Directory => "an empty directory",
            Self::GitHubRepo => "a new private GitHub repository with a first commit (gh)",
            Self::Clone(_) => "owner/name or a URL; anything else is searched",
        }
    }
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// `owner/name`
    pub id: String,
    /// Popularity, already worded ("12 stars", "3 likes").
    pub meta: String,
    pub description: String,
}

/// Work that calls external commands, so it runs off the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    NewGitHubRepo {
        name: String,
        dest: PathBuf,
    },
    Clone {
        url: String,
        dest: PathBuf,
        /// Leave Git LFS files as pointers instead of downloading them.
        skip_lfs: bool,
    },
    Search {
        hub: Hub,
        query: String,
    },
}

impl Job {
    /// Shown while the job runs.
    pub fn describe(&self) -> String {
        match self {
            Self::NewGitHubRepo { name, .. } => format!("creating GitHub repository: {name}"),
            Self::Clone { url, .. } => format!("cloning {url}"),
            Self::Search { hub, query } => format!("searching {}: {query}", hub.label()),
        }
    }
}

#[derive(Debug)]
pub enum Done {
    Created(Result<PathBuf>),
    Found {
        hub: Hub,
        query: String,
        hits: Result<Vec<Hit>>,
    },
}

/// What Enter does with the typed text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    MakeDir(PathBuf),
    Run(Job),
}

pub fn plan(ctx: &Context, recipe: Recipe, input: &str, lfs: bool) -> Result<Plan> {
    let input = input.trim();
    if input.is_empty() {
        bail!("type a name");
    }
    match recipe {
        Recipe::Directory => Ok(Plan::MakeDir(free_dest(ctx, input)?)),
        Recipe::GitHubRepo => Ok(Plan::Run(Job::NewGitHubRepo {
            name: input.to_string(),
            dest: free_dest(ctx, input)?,
        })),
        Recipe::Clone(hub) => {
            if input.contains("://") || input.starts_with("git@") {
                let name = name_from_url(input)?;
                Ok(Plan::Run(clone_job(ctx, hub, input, &name, lfs)?))
            } else if input.contains('/') {
                plan_clone(ctx, hub, input, lfs).map(Plan::Run)
            } else {
                Ok(Plan::Run(Job::Search {
                    hub,
                    query: input.to_string(),
                }))
            }
        }
    }
}

/// Clones `owner/name` from `hub`.
pub fn plan_clone(ctx: &Context, hub: Hub, id: &str, lfs: bool) -> Result<Job> {
    let id = id.strip_suffix(".git").unwrap_or(id);
    let Some((owner, name)) = id.split_once('/') else {
        bail!("expected owner/name: {id}");
    };
    if owner.is_empty() || name.contains('/') || id.contains(char::is_whitespace) {
        bail!("expected owner/name: {id}");
    }
    clone_job(ctx, hub, &hub.clone_url(id), name, lfs)
}

fn clone_job(ctx: &Context, hub: Hub, url: &str, name: &str, lfs: bool) -> Result<Job> {
    Ok(Job::Clone {
        url: url.to_string(),
        dest: free_dest(ctx, name)?,
        skip_lfs: hub.is_hugging_face() && !lfs,
    })
}

pub fn run(forge: &dyn Forge, job: Job) -> Done {
    match job {
        Job::NewGitHubRepo { name, dest } => {
            Done::Created(forge.new_github_repo(&name, &dest).map(|()| dest))
        }
        Job::Clone {
            url,
            dest,
            skip_lfs,
        } => Done::Created(forge.clone_repo(&url, &dest, skip_lfs).map(|()| dest)),
        Job::Search { hub, query } => Done::Found {
            hits: forge.search(hub, &query),
            hub,
            query,
        },
    }
}

fn name_from_url(url: &str) -> Result<String> {
    let last = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("");
    let name = last.strip_suffix(".git").unwrap_or(last);
    ensure_plain_name(name).map_err(|_| anyhow::anyhow!("cannot tell the name from: {url}"))?;
    Ok(name.to_string())
}

/// `<root>/<name>`, refused when something is already there (nothing is ever overwritten).
fn free_dest(ctx: &Context, name: &str) -> Result<PathBuf> {
    ensure_plain_name(name)?;
    let dest = ctx.root()?.join(name);
    if dest.symlink_metadata().is_ok() {
        bail!("already exists: {}", dest.display());
    }
    // Taking an archived name would make that archive impossible to restore.
    let archived = ctx.archive_base()?.join(name);
    if archived.exists() {
        bail!(
            "an archived directory has this name: {}",
            archived.display()
        );
    }
    Ok(dest)
}

/// Only single-level names are allowed, so `..` or `/` cannot create anything outside the root.
fn ensure_plain_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\'])
        || name.contains(char::is_control)
    {
        bail!("invalid name: {name:?}");
    }
    Ok(())
}
