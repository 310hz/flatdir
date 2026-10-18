use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use flatdir::cmd;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::paths::Env;
use flatdir::select::SkimSelector;
use flatdir::services::{SystemTrash, TtyPrompter, Zoxide};

#[derive(Parser)]
#[command(
    version,
    about = "Browse, archive and manage the directories directly under one managed root"
)]
struct Cli {
    /// Pick from archived directories instead
    #[arg(short = 'a', long)]
    archived: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Set the managed root (only one is supported)
    Track { path: PathBuf },
    /// Unset the managed root
    Untrack,
    /// Show the managed root, directory counts and required commands
    Status,
    /// Move the selected directories to the trash
    Remove,
    /// Move the selected directories to the archive
    Archive,
    /// Move archived directories back under the root
    Restore,
    /// Create a new directory under the root
    Init {
        name: String,
        /// Also create a private GitHub repository and clone it
        #[arg(long)]
        git: bool,
    },
    /// git clone into the root
    Clone {
        /// Search GitHub and pick a repository
        #[arg(short = 's')]
        search: bool,
        target: Option<String>,
    },
}

fn context() -> Result<Context> {
    let env = Env::from_process()?;
    let config = Config::load(&env.config_path)?;
    let selector = SkimSelector {
        color: config.color.clone(),
    };
    Ok(Context {
        env,
        config,
        interactive: std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        selector: Box::new(selector),
        trasher: Box::new(SystemTrash),
        prompter: Box::new(TtyPrompter),
        scores: Box::new(Zoxide),
    })
}

fn run(cli: Cli) -> Result<()> {
    match (cli.archived, cli.command) {
        (true, Some(_)) => bail!("-a cannot be combined with a subcommand"),
        (true, None) => print_path(cmd::select_archived(&context()?)?),
        (false, None) => print_path(cmd::select(&context()?)?),
        (false, Some(command)) => match command {
            Command::Track { path } => {
                let (root, added) = cmd::track(&mut context()?, &path)?;
                if added {
                    eprintln!("tracked: {}", root.display());
                } else {
                    eprintln!("already registered: {}", root.display());
                }
            }
            Command::Untrack => {
                eprintln!("untracked: {}", cmd::untrack(&mut context()?)?.display())
            }
            Command::Status => println!("{}", cmd::status(&context()?)),
            Command::Remove => cmd::remove(&context()?)?,
            Command::Archive => cmd::archive(&context()?)?,
            Command::Restore => cmd::restore(&context()?)?,
            Command::Init { name, git } => {
                let dest = cmd::init(&context()?, &name, git)?;
                eprintln!(
                    "initialized{}: {}",
                    if git { " (git)" } else { "" },
                    dest.display()
                );
            }
            Command::Clone { search, target } => {
                let dest = cmd::clone(&context()?, search, target.as_deref())?;
                eprintln!("cloned: {}", dest.display());
            }
        },
    }
    Ok(())
}

fn print_path(path: Option<PathBuf>) {
    if let Some(p) = path {
        println!("{}", p.display());
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
