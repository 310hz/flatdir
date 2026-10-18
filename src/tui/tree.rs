use std::path::Path;

use ignore::WalkBuilder;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

const MAX_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 300;

struct Node {
    depth: usize,
    name: String,
    is_dir: bool,
}

/// A depth-limited tree. Hidden files and anything ignored by git are skipped, which keeps
/// `node_modules`, `target` and friends out without a hard-coded list.
pub fn render(root: &Path) -> Vec<Line<'static>> {
    let walker = WalkBuilder::new(root)
        .max_depth(Some(MAX_DEPTH))
        .require_git(false)
        .sort_by_file_path(|a, b| (!a.is_dir(), a).cmp(&(!b.is_dir(), b)))
        .build();

    let mut nodes = Vec::new();
    let mut truncated = false;
    for entry in walker.filter_map(Result::ok).filter(|e| e.depth() > 0) {
        if nodes.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        nodes.push(Node {
            depth: entry.depth(),
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: entry.file_type().is_some_and(|t| t.is_dir()),
        });
    }

    let dim = Style::default().add_modifier(Modifier::DIM);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut lines = vec![Line::from(Span::styled(format!("{root_name}/"), bold))];

    let mut last_at: Vec<bool> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let is_last = nodes[i + 1..]
            .iter()
            .find(|n| n.depth <= node.depth)
            .is_none_or(|n| n.depth < node.depth);
        last_at.truncate(node.depth - 1);
        let mut prefix: String = last_at
            .iter()
            .map(|&l| if l { "    " } else { "│   " })
            .collect();
        prefix.push_str(if is_last { "└── " } else { "├── " });
        last_at.push(is_last);

        let (name, style) = if node.is_dir {
            (format!("{}/", node.name), bold)
        } else {
            (node.name.clone(), Style::default())
        };
        lines.push(Line::from(vec![
            Span::styled(prefix, dim),
            Span::styled(name, style),
        ]));
    }
    if nodes.is_empty() {
        lines.push(Line::from(Span::styled("(empty)", dim)));
    }
    if truncated {
        lines.push(Line::from(Span::styled("...", dim)));
    }
    lines
}
