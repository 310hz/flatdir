use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;

fn have(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
}

fn preview_columns() -> String {
    std::env::var("FZF_PREVIEW_COLUMNS").unwrap_or_else(|_| "80".to_string())
}

pub fn dir(path: &Path) -> Result<()> {
    let readme = ["README.md", "README"]
        .iter()
        .map(|n| path.join(n))
        .find(|p| p.is_file());
    if let Some(readme) = readme {
        if have("rich") {
            Command::new("rich")
                .args(["--max-width", &preview_columns(), "--"])
                .arg(&readme)
                .status()?;
        } else {
            let file = std::fs::File::open(&readme)?;
            for line in BufReader::new(file).lines().take(200) {
                println!("{}", line?);
            }
        }
        return Ok(());
    }

    if have("eza") {
        Command::new("eza")
            .args([
                "-lTF",
                "--icons",
                "--group-directories-first",
                "--no-user",
                "--no-permissions",
                "--no-filesize",
                "--time-style=long-iso",
                "--color=always",
                "-s=extension",
                "-L=1",
                "--",
            ])
            .arg(path)
            .status()?;
    } else {
        Command::new("ls").args(["-la", "--"]).arg(path).status()?;
    }
    Ok(())
}

pub fn clone_repo(repo: &str) -> Result<()> {
    if !have("rich") {
        Command::new("gh").args(["repo", "view", repo]).status()?;
        return Ok(());
    }
    let mut view = Command::new("gh")
        .args(["repo", "view", repo])
        .stdout(Stdio::piped())
        .spawn()?;
    let stdin = view
        .stdout
        .take()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);
    Command::new("rich")
        .args(["--markdown", "--max-width", &preview_columns(), "-"])
        .stdin(stdin)
        .status()?;
    view.wait()?;
    Ok(())
}
