use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use termimad::MadSkin;

use crate::services::command_exists;

const MAX_LINES: usize = 200;
const MAX_README_BYTES: u64 = 64 * 1024;

/// Preview text for a directory: its README rendered as Markdown, else a listing.
pub fn dir(path: &Path, width: usize) -> String {
    let readme = ["README.md", "README"]
        .iter()
        .map(|n| path.join(n))
        .find(|p| p.is_file());
    if let Some(readme) = readme {
        return render_markdown(&read_head(&readme), width);
    }
    if command_exists("eza")
        && let Some(out) = eza(path)
    {
        return out;
    }
    listing(path)
}

/// Preview text for a GitHub repository (`owner/repo`). `gh` is optional.
pub fn repo(name: &str, width: usize) -> String {
    match Command::new("gh").args(["repo", "view", name]).output() {
        Ok(out) if out.status.success() => {
            render_markdown(&String::from_utf8_lossy(&out.stdout), width)
        }
        Ok(_) => format!("gh repo view failed: {name}"),
        Err(_) => "gh is not installed".to_string(),
    }
}

fn read_head(path: &Path) -> String {
    let mut buf = Vec::new();
    let read = fs::File::open(path).and_then(|f| f.take(MAX_README_BYTES).read_to_end(&mut buf));
    if let Err(e) = read {
        return format!("cannot read {}: {e}", path.display());
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .take(MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The default skin uses gray levels chosen to stay readable on both light and dark terminals.
fn render_markdown(md: &str, width: usize) -> String {
    MadSkin::default().text(md, Some(width.max(20))).to_string()
}

fn eza(path: &Path) -> Option<String> {
    let out = Command::new("eza")
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
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Built-in fallback when `eza` is unavailable: directories first, then by extension and name.
fn listing(path: &Path) -> String {
    let entries = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => return format!("cannot read {}: {e}", path.display()),
    };
    let mut items: Vec<(bool, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().ok()?.is_dir();
            (!name.starts_with('.')).then_some((is_dir, name))
        })
        .collect();
    items.sort_by(|(da, a), (db, b)| {
        let ext = |n: &str| {
            n.rsplit_once('.')
                .map(|(_, e)| e.to_lowercase())
                .unwrap_or_default()
        };
        db.cmp(da)
            .then_with(|| ext(a).cmp(&ext(b)))
            .then_with(|| a.cmp(b))
    });

    let total = items.len();
    let mut lines: Vec<String> = items
        .into_iter()
        .take(MAX_LINES)
        .map(|(is_dir, name)| if is_dir { format!("{name}/") } else { name })
        .collect();
    if total > MAX_LINES {
        lines.push(format!("... and {} more", total - MAX_LINES));
    }
    lines.join("\n")
}
