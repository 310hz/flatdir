//! Rendering. Colors are limited to the terminal's own ANSI palette plus text attributes,
//! so the screen reads the same on light and dark backgrounds.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};
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
    // Title row, a blank row, then the content. The shortcuts live in the help window.
    let [title, _spacer, query, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_title(frame, app, title);
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
    if app.help {
        draw_help(frame, app);
    }
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

fn draw_title(frame: &mut Frame, app: &App, area: Rect) {
    let tab = |label: &'static str, active: bool| {
        let style = if active {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            dim()
        };
        Span::styled(format!(" {label} "), style)
    };
    // The count and "N marked" are padded, so the title never shifts as they change.
    let marked = if app.marks.is_empty() {
        String::new()
    } else {
        format!("{} marked", app.marks.len())
    };
    let title = Line::from(vec![
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
    ]);
    let help = Line::from(vec![
        Span::styled("?", bold()),
        Span::styled(" Help", dim()),
    ])
    .right_aligned();
    frame.render_widget(Paragraph::new(title), area);
    frame.render_widget(Paragraph::new(help), area);
}

/// Space between the program name and the Active / Archived tabs.
const TITLE_GAP: &str = "    ";

/// Room reserved for "NNN marked".
const COUNT_WIDTH: usize = 10;

/// Names the app `Opener` actually launches (`open` on macOS, `xdg-open` elsewhere).
const OPEN_LABEL: &str = if cfg!(target_os = "macos") {
    "Open in Finder"
} else {
    "Open in File Manager"
};

fn help_items(kind: ListKind) -> [(&'static str, &'static str); 10] {
    [
        ("Enter", "cd"),
        ("Tab / Shift+Tab", "Mark"),
        ("Ctrl+Q", "Clear Marks"),
        ("Ctrl+A", "Toggle Window Active/Archived"),
        ("Ctrl+T", toggle_label(kind)),
        ("Ctrl+R", "Move to Trash"),
        ("Ctrl+O", OPEN_LABEL),
        ("Ctrl+P", "Toggle Preview"),
        ("Ctrl+U", "Clear Query"),
        ("Esc", "Quit"),
    ]
}

/// The shortcut list, centered on top of the screen.
fn draw_help(frame: &mut Frame, app: &App) {
    const GAP: usize = 3;
    let items = help_items(app.kind);
    let key_width = items.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let lines: Vec<Line> = items
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!("{key:<key_width$}"), bold()),
                Span::raw(" ".repeat(GAP)),
                Span::raw(*label),
            ])
        })
        .collect();
    let content_width = lines.iter().map(Line::width).max().unwrap_or(0);

    let block = Block::bordered()
        .border_style(dim())
        .padding(Padding::new(2, 2, 1, 1))
        .title(Span::styled(" Help ", bold()))
        .title_bottom(Line::from(vec![
            Span::styled(" ? / Esc", bold()),
            Span::styled(" Close ", dim()),
        ]));
    let screen = frame.area();
    // Borders (2) and padding (2 + 2 across, 1 + 1 down) around the text.
    let width = ((content_width + 6) as u16).min(screen.width);
    let height = ((lines.len() + 4) as u16).min(screen.height);
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
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
