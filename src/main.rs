use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use flatdir::cmd;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::paths::Env;
use flatdir::select::SkimSelector;
use flatdir::services::{SystemOpener, SystemTrash, TtyPrompter, Zoxide};

#[derive(Parser)]
#[command(
    version,
    about = "Browse, archive and manage the directories directly under one managed root. Run without a subcommand to open the TUI."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Shells the function is written for. Both get exactly the same text.
#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
}

#[derive(Subcommand)]
enum Command {
    /// Show the managed root, or set it with a path (replaces the current one)
    Root {
        path: Option<PathBuf>,
        /// Unset the managed root
        #[arg(long, conflicts_with = "path")]
        unset: bool,
    },
    /// Show the managed root, directory counts and required commands
    Status,
    /// Print the shell function that makes Enter cd (use: eval "$(flatdir shell)")
    Shell {
        /// The same function works in both, so this only exists for habit (`flatdir shell zsh`)
        #[arg(value_enum)]
        shell: Option<Shell>,
    },
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
        opener: Box::new(SystemOpener),
        scores: Arc::new(Zoxide),
    })
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => print_path(flatdir::tui::run(&context()?)?),
        Some(Command::Root {
            path: Some(path), ..
        }) => {
            let change = cmd::set_root(&mut context()?, &path)?;
            match (change.changed, change.previous) {
                (false, _) => eprintln!("already the root: {}", change.root.display()),
                (true, Some(old)) => {
                    eprintln!("root: {} (was {})", change.root.display(), old.display())
                }
                (true, None) => eprintln!("root: {}", change.root.display()),
            }
        }
        Some(Command::Root { unset: true, .. }) => {
            eprintln!(
                "unset root: {}",
                cmd::unset_root(&mut context()?)?.display()
            )
        }
        Some(Command::Root { .. }) => {
            let ctx = context()?;
            println!("{}", ctx.root()?.display());
        }
        Some(Command::Shell { .. }) => print!("{}", flatdir::shell::SHELL_FUNCTION),
        Some(Command::Status) => println!("{}", cmd::status(&context()?)),
        Some(Command::Init { name, git }) => {
            let dest = cmd::init(&context()?, &name, git)?;
            eprintln!(
                "initialized{}: {}",
                if git { " (git)" } else { "" },
                dest.display()
            );
        }
        Some(Command::Clone { search, target }) => {
            let dest = cmd::clone(&context()?, search, target.as_deref())?;
            eprintln!("cloned: {}", dest.display());
        }
    }
    Ok(())
}

fn print_path(path: Option<PathBuf>) {
    if let Some(p) = path {
        println!("{}", p.display());
        // Captured by the shell function, stdout is a pipe. A terminal means nothing will cd.
        if std::io::stdout().is_terminal() {
            eprintln!("{}", flatdir::shell::MISSING_FUNCTION_HINT);
        }
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
