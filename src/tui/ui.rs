//! Rendering. Colors are limited to the terminal's own ANSI palette plus text attributes,
//! so the screen reads the same on light and dark backgrounds.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::{App, ListKind, Panes};
use super::preview::{self, PreviewMode, Shown};
use super::text;

const MIN_WIDTH_FOR_PREVIEW: u16 = 70;
/// Blank columns between the vertical line and the preview text, so the tree's own `│` never lines up with it.
pub const PREVIEW_PAD_LEFT: u16 = 2;
pub const MARK_COLOR: Color = Color::Magenta;
pub const DANGER_COLOR: Color = Color::Red;

#[derive(Default)]
pub struct PreviewCache {
    key: Option<(PathBuf, PreviewMode)>,
    lines: Vec<Line<'static>>,
    shown: Option<Shown>,
}

pub fn draw(frame: &mut Frame, app: &mut App, cache: &mut PreviewCache) {
    // Shortcuts are laid out item by item, so a narrow terminal gets more rows instead of a split item.
    let (title, hints) = header_lines(app, frame.area().width as usize);
    // Never let the header take more than half of the screen, so the list always stays visible.
    let max_header = (frame.area().height as usize / 2).max(2);
    // Title row, a blank row, then the shortcuts.
    let header_height = (2 + hints.len()).min(max_header) as u16;
    let [header, _spacer, query, body, status] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let mut lines = vec![title, Line::default()];
    lines.extend(hints);
    frame.render_widget(Paragraph::new(lines), header);
    draw_query(frame, app, query);

    if body.width >= MIN_WIDTH_FOR_PREVIEW {
        let [list, preview] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(body);
        app.panes = Panes {
            list,
            preview: Some(preview),
        };
        draw_list(frame, app, list);
        draw_preview(frame, app, cache, preview);
    } else {
        app.panes = Panes {
            list: body,
            preview: None,
        };
        draw_list(frame, app, body);
    }
    draw_status(frame, app, status);
}

fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// What Ctrl+T does in the current list.
pub fn toggle_label(kind: ListKind) -> &'static str {
    match kind {
        ListKind::Active => "Archive",
        ListKind::Archived => "Restore",
    }
}

fn header_lines(app: &App, width: usize) -> (Line<'static>, Vec<Line<'static>>) {
    let tab = |label: &'static str, active: bool| {
        let style = if active {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            dim()
        };
        Span::styled(format!(" {label} "), style)
    };
    // Everything that changes with the mode or the data is padded to its widest variant, so the
    // header never moves and never changes its number of rows (which would shift the content below).
    let marked = if app.marks.is_empty() {
        String::new()
    } else {
        format!("{} marked", app.marks.len())
    };
    let title = vec![
        Span::styled("flatdir", bold()),
        Span::raw(TITLE_GAP),
        tab("Active", app.kind == ListKind::Active),
        Span::raw(" "),
        tab("Archived", app.kind == ListKind::Archived),
        Span::styled(
            format!("  {:>4}/{:<4}", app.view.len(), app.list().len()),
            dim(),
        ),
        Span::styled(
            format!("  {marked:<COUNT_WIDTH$}"),
            Style::default().fg(MARK_COLOR).add_modifier(Modifier::BOLD),
        ),
    ];

    let other_list = match app.kind {
        ListKind::Active => "Show Archived",
        ListKind::Archived => "Show Active",
    };
    let preview_name = match app.preview_mode {
        PreviewMode::Markdown => "Markdown",
        PreviewMode::Tree => "Tree",
    };
    let other_list = pad_to_longest(other_list, &["Show Archived", "Show Active"]);
    let toggle = pad_to_longest(toggle_label(app.kind), &["Archive", "Restore"]);
    let preview_name = pad_to_longest(preview_name, &["Markdown", "Tree"]);
    let item = |key: &'static str, label: String| {
        vec![
            Span::styled(key, bold()),
            Span::styled(format!(" {label}"), dim()),
        ]
    };
    let items = vec![
        item("Enter", "Change Directory".into()),
        item("Tab", "Mark".into()),
        item("Ctrl+Q", "Clear Marks".into()),
        item("Ctrl+A", other_list),
        item("Ctrl+T", toggle),
        item("Ctrl+R", "Move to Trash".into()),
        item("Ctrl+O", OPEN_LABEL.into()),
        item("Ctrl+P", format!("Preview: {preview_name}")),
        item("Esc", "Quit".into()),
    ];

    (Line::from(title), flow(items, width))
}

/// Names the app `Opener` actually launches (`open` on macOS, `xdg-open` elsewhere).
const OPEN_LABEL: &str = if cfg!(target_os = "macos") {
    "Open in Finder"
} else {
    "Open in File Manager"
};

/// Space between the program name and the Active / Archived tabs.
const TITLE_GAP: &str = "    ";

/// Room reserved for "NNN marked".
const COUNT_WIDTH: usize = 10;

/// Pads `label` with spaces to the width of the longest of `variants`.
fn pad_to_longest(label: &str, variants: &[&str]) -> String {
    let longest = variants.iter().map(|v| v.width()).max().unwrap_or(0);
    let pad = longest.saturating_sub(label.width());
    format!("{label}{}", " ".repeat(pad))
}

/// Packs items into rows no wider than `width`, breaking only between items.
fn flow(items: Vec<Vec<Span<'static>>>, width: usize) -> Vec<Line<'static>> {
    const GAP: &str = "   ";
    let mut rows: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for item in items {
        let item_width: usize = item.iter().map(|s| s.content.width()).sum();
        if used > 0 && used + GAP.len() + item_width > width {
            rows.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        if used > 0 {
            spans.push(Span::raw(GAP));
            used += GAP.len();
        }
        spans.extend(item);
        used += item_width;
    }
    if !spans.is_empty() {
        rows.push(Line::from(spans));
    }
    rows
}

fn draw_query(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = "> ";
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prompt, bold()),
            Span::raw(app.query.clone()),
        ])),
        area,
    );
    let x = area.x
        + (prompt.width() + app.query.width()).min(area.width.saturating_sub(1) as usize) as u16;
    frame.set_cursor_position(Position::new(x, area.y));
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    app.ensure_visible(area.height as usize);
    let list = app.list();
    let mut lines: Vec<Line> = Vec::new();

    for (i, view) in app
        .view
        .iter()
        .enumerate()
        .skip(app.offset)
        .take(area.height as usize)
    {
        let entry = &list[view.idx];
        let marked = app.marks.contains(&entry.path);
        let current = i == app.cursor;

        let mut base = Style::default();
        if marked {
            base = base.fg(MARK_COLOR).add_modifier(Modifier::BOLD);
        }
        let mut spans = vec![if marked {
            Span::styled("▌ ", Style::default().fg(MARK_COLOR))
        } else {
            Span::raw("  ")
        }];
        for (pos, ch) in entry.name.chars().enumerate() {
            if text::is_invisible(ch) {
                continue;
            }
            let hit = view.hits.binary_search(&(pos as u32)).is_ok();
            let style = if hit {
                base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                base
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        let mut line = Line::from(spans);
        if current {
            line = line.style(Style::default().add_modifier(Modifier::REVERSED));
        }
        lines.push(line);
    }

    if lines.is_empty() {
        let text = if app.list().is_empty() {
            "(empty)"
        } else {
            "(no matches)"
        };
        lines.push(Line::from(Span::styled(format!("  {text}"), dim())));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_preview(frame: &mut Frame, app: &mut App, cache: &mut PreviewCache, area: Rect) {
    let current = app.current().map(|e| e.path.clone());
    let key = current.map(|p| (p, app.preview_mode));
    if cache.key != key {
        match &key {
            Some((path, mode)) => {
                let (lines, shown) = preview::render(path, *mode);
                cache.lines = lines;
                cache.shown = Some(shown);
            }
            None => {
                cache.lines.clear();
                cache.shown = None;
            }
        }
        cache.key = key;
        app.preview_scroll = 0;
    }

    let title = format!(" {} ", cache.shown.map_or("Preview", Shown::label));
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(dim())
        .padding(Padding::new(PREVIEW_PAD_LEFT, 1, 0, 0))
        .title(Span::styled(title, dim()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let paragraph = Paragraph::new(Text::from(cache.lines.clone())).wrap(Wrap { trim: false });
    // Scrolling stops at the last wrapped row instead of running into blank space.
    let max_scroll = paragraph
        .line_count(inner.width)
        .saturating_sub(inner.height as usize);
    app.preview_scroll = app
        .preview_scroll
        .min(max_scroll.min(u16::MAX as usize) as u16);
    frame.render_widget(paragraph.scroll((app.preview_scroll, 0)), inner);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.status {
        Some(s) => {
            let style = if s.is_error {
                Style::default().fg(DANGER_COLOR)
            } else {
                Style::default()
            };
            Line::from(Span::styled(text::sanitize(&s.text).into_owned(), style))
        }
        None => Line::default(),
    };
    frame.render_widget(Paragraph::new(line), area);
}
