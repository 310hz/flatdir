//! Rendering. Colors are limited to the terminal's own ANSI palette plus text attributes,
//! so the screen reads the same on light and dark backgrounds.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::{Action, App, ListKind};
use super::preview::{self, PreviewMode, Shown};

const MIN_WIDTH_FOR_PREVIEW: u16 = 70;
pub const MARK_COLOR: Color = Color::Magenta;
pub const SAFE_COLOR: Color = Color::Green;
pub const DANGER_COLOR: Color = Color::Red;

#[derive(Default)]
pub struct PreviewCache {
    key: Option<(PathBuf, PreviewMode)>,
    lines: Vec<Line<'static>>,
    shown: Option<Shown>,
}

pub fn draw(frame: &mut Frame, app: &mut App, cache: &mut PreviewCache) {
    let [header, query, body, status] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, app, header);
    draw_query(frame, app, query);

    if body.width >= MIN_WIDTH_FOR_PREVIEW {
        let [list, preview] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(body);
        draw_list(frame, app, list);
        draw_preview(frame, app, cache, preview);
    } else {
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

pub fn action_label(kind: ListKind, action: Action) -> &'static str {
    match (kind, action) {
        (ListKind::Tracked, Action::Safe) => "Archive",
        (ListKind::Archived, Action::Safe) => "Restore",
        (_, Action::Remove) => "REMOVE (to Trash)",
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let tab = |label: &'static str, active: bool| {
        let style = if active {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            dim()
        };
        Span::styled(format!(" {label} "), style)
    };
    let mut title = vec![
        Span::styled("flatdir ", bold()),
        tab("Tracked", app.kind == ListKind::Tracked),
        Span::raw(" "),
        tab("Archived", app.kind == ListKind::Archived),
        Span::styled(format!("  {}/{}", app.view.len(), app.list().len()), dim()),
    ];
    if !app.marks.is_empty() {
        title.push(Span::styled(
            format!("  {} marked", app.marks.len()),
            Style::default().fg(MARK_COLOR).add_modifier(Modifier::BOLD),
        ));
    }

    let (other_list, switch) = match app.kind {
        ListKind::Tracked => ("Archived", "Archive/Remove"),
        ListKind::Archived => ("Tracked", "Restore/Remove"),
    };
    let action_style = match app.action {
        Action::Safe => Style::default().fg(SAFE_COLOR).add_modifier(Modifier::BOLD),
        Action::Remove => Style::default()
            .fg(DANGER_COLOR)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
    };
    let preview_name = match app.preview_mode {
        PreviewMode::Markdown => "Markdown",
        PreviewMode::Tree => "Tree",
    };
    let key = |k: &'static str| Span::styled(k, bold());
    let hints = Line::from(vec![
        key("Enter"),
        Span::styled(" cd   ", dim()),
        key("Tab"),
        Span::styled(" mark   ", dim()),
        key("Ctrl+A"),
        Span::styled(format!(" {other_list}   "), dim()),
        key("Ctrl+R"),
        Span::raw(" "),
        Span::styled(
            format!(" {} ", action_label(app.kind, app.action)),
            action_style,
        ),
        Span::raw("   "),
        key("Ctrl+T"),
        Span::styled(format!(" {switch}   "), dim()),
        key("Ctrl+P"),
        Span::styled(format!(" preview: {preview_name}   "), dim()),
        key("Esc"),
        Span::styled(" quit", dim()),
    ]);

    frame.render_widget(
        Paragraph::new(vec![Line::from(title), hints]).wrap(Wrap { trim: false }),
        area,
    );
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

fn draw_preview(frame: &mut Frame, app: &App, cache: &mut PreviewCache, area: Rect) {
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
    }

    let title = format!(" {} ", cache.shown.map_or("Preview", Shown::label));
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(dim())
        .title(Span::styled(title, dim()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Text::from(cache.lines.clone())).wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match (&app.status, app.action) {
        (Some(s), _) => {
            let style = if s.is_error {
                Style::default().fg(DANGER_COLOR)
            } else {
                Style::default()
            };
            Line::from(Span::styled(s.text.clone(), style))
        }
        (None, Action::Remove) => Line::from(Span::styled(
            "REMOVE mode: Ctrl+R moves the marked (or current) entries to the Trash. Ctrl+T switches back.",
            Style::default()
                .fg(DANGER_COLOR)
                .add_modifier(Modifier::BOLD),
        )),
        (None, Action::Safe) => Line::default(),
    };
    frame.render_widget(Paragraph::new(line), area);
}
