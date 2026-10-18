use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use flatdir::cmd;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::paths::Env;
use flatdir::preview;
use flatdir::select::Fzf;
use flatdir::services::{TrashCommand, TtyPrompter, Zoxide};

#[derive(Parser)]
#[command(
    version,
    about = "管理 root 直下のディレクトリを flat に一覧し、移動・整理する"
)]
struct Cli {
    /// アーカイブ済みディレクトリを選択する
    #[arg(short = 'a', long)]
    archived: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 管理 root を設定する(1 つだけ)
    Track { path: PathBuf },
    /// 管理 root の設定を解除する
    Untrack,
    /// 管理 root を表示する
    List,
    /// 選択したディレクトリを trash へ送る
    Remove,
    /// 選択したディレクトリをアーカイブへ移す
    Archive,
    /// アーカイブ済みを選択して元の場所へ戻す
    Restore,
    /// root 直下に新規ディレクトリを作る
    Init {
        name: String,
        /// GitHub に private リポジトリを作って clone する
        #[arg(long)]
        git: bool,
    },
    /// root 直下に git clone する
    Clone {
        /// GitHub を検索して選ぶ
        #[arg(short = 's')]
        search: bool,
        target: Option<String>,
    },
    #[command(name = "__preview", hide = true)]
    Preview { path: PathBuf },
    #[command(name = "__clone-preview", hide = true)]
    ClonePreview { repo: String },
}

fn context() -> Result<Context> {
    let env = Env::from_process()?;
    let config = Config::load(&env.config_path)?;
    let selector = Fzf {
        extra_opts: config.fzf_opts.clone(),
        exe: std::env::current_exe()?,
    };
    Ok(Context {
        env,
        config,
        interactive: std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        selector: Box::new(selector),
        trasher: Box::new(TrashCommand),
        prompter: Box::new(TtyPrompter),
        scores: Box::new(Zoxide),
    })
}

fn run(cli: Cli) -> Result<()> {
    match (cli.archived, cli.command) {
        (true, Some(_)) => bail!("-a はサブコマンドと併用できない"),
        (true, None) => print_path(cmd::select_archived(&context()?)?),
        (false, None) => print_path(cmd::select(&context()?)?),
        (false, Some(command)) => match command {
            Command::Preview { path } => return preview::dir(&path),
            Command::ClonePreview { repo } => return preview::clone_repo(&repo),
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
            Command::List => match cmd::list(&context()?) {
                Some(root) => println!("{}", root.display()),
                None => eprintln!("(empty)"),
            },
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
