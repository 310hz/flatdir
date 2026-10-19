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
use flatdir::services::{SystemForge, SystemOpener, SystemTrash, Zoxide};

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
        /// cd into the managed root (through the shell function, like Enter in the TUI)
        #[arg(long, conflicts_with_all = ["path", "unset"])]
        cd: bool,
    },
    /// Show the managed root, directory counts and required commands
    Status,
    /// Print the shell function that makes Enter cd (use: eval "$(flatdir shell)")
    // Hidden: it is set up once, and Enter without the function prints how to do it.
    #[command(hide = true)]
    Shell {
        /// The same function works in both, so this only exists for habit (`flatdir shell zsh`)
        #[arg(value_enum)]
        shell: Option<Shell>,
    },
}

fn context() -> Result<Context> {
    let env = Env::from_process()?;
    let config = Config::load(&env.config_path)?;
    Ok(Context {
        env,
        config,
        interactive: std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        trasher: Box::new(SystemTrash),
        opener: Box::new(SystemOpener),
        forge: Arc::new(SystemForge),
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
        Some(Command::Root { cd: true, .. }) => print_path(Some(context()?.root()?.to_path_buf())),
        Some(Command::Root { .. }) => {
            let ctx = context()?;
            println!("{}", ctx.root()?.display());
        }
        Some(Command::Shell { .. }) => print!("{}", flatdir::shell::SHELL_FUNCTION),
        Some(Command::Status) => println!("{}", cmd::status(&context()?)),
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
