use std::fs;
use std::io::Read;
use std::path::Path;

use ratatui::text::Line;

use super::{markdown, tree};

const MAX_README_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Markdown,
    Tree,
}

impl PreviewMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Markdown => Self::Tree,
            Self::Tree => Self::Markdown,
        }
    }
}

/// What is actually shown. Markdown falls back to the tree when there is no README.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Markdown,
    Tree,
    TreeNoReadme,
}

impl Shown {
    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "README",
            Self::Tree => "Tree",
            Self::TreeNoReadme => "Tree (no README)",
        }
    }
}

pub fn render(dir: &Path, mode: PreviewMode) -> (Vec<Line<'static>>, Shown) {
    if mode == PreviewMode::Markdown
        && let Some(readme) = ["README.md", "README"]
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
    {
        return (markdown::render(&read_head(&readme)), Shown::Markdown);
    }
    let shown = if mode == PreviewMode::Markdown {
        Shown::TreeNoReadme
    } else {
        Shown::Tree
    };
    (tree::render(dir), shown)
}

fn read_head(path: &Path) -> String {
    let mut buf = Vec::new();
    match fs::File::open(path).and_then(|f| f.take(MAX_README_BYTES).read_to_end(&mut buf)) {
        Ok(_) => String::from_utf8_lossy(&buf).into_owned(),
        Err(e) => format!("cannot read {}: {e}", path.display()),
    }
}
