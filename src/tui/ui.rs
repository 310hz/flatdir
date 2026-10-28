//! Rendering. Colors are limited to the terminal's own ANSI palette plus text attributes,
//! so the screen reads the same on light and dark backgrounds.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Confirm, Confirmable, ListKind, Panes};
use super::preview::{self, PreviewMode, Shown};
use super::text;
use crate::create::{Job, Plan, RECIPES, Recipe};

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

    if app.new.open {
        app.panes = Panes::default();
        draw_new(frame, app, body);
    } else if body.width >= MIN_WIDTH_FOR_PREVIEW {
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
    } else if let Some(confirm) = &app.confirm {
        draw_confirm(frame, confirm);
    }
}

fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// What Ctrl+A does in the current list.
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
    let marked = if app.marks.is_empty() || app.new.open {
        String::new()
    } else {
        format!("{} marked", app.marks.len())
    };
    // Blank but the same width in the New view, which has no list to count.
    let counts = if app.new.open {
        format!("  {:9}", "")
    } else {
        format!("  {:>4}/{:<4}", app.view.len(), app.list().len())
    };
    let listing = !app.new.open;
    let title = Line::from(vec![
        Span::styled("flatdir", bold()),
        Span::raw(TITLE_GAP),
        tab("Active", listing && app.kind == ListKind::Active),
        Span::raw(" "),
        tab("Archived", listing && app.kind == ListKind::Archived),
        Span::raw(" "),
        tab("New", app.new.open),
        Span::styled(counts, dim()),
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

fn help_items(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.new.open {
        return vec![
            ("Tab / Shift+Tab", "Choose Kind"),
            ("Enter", "Create / Search / Clone"),
            ("Up / Down, j / k", "Choose Search Result"),
            ("Ctrl+L", "Toggle LFS: On/Off"),
            ("Ctrl+U", "Clear Name"),
            ("Esc", "Edit Search, or Back to List"),
            ("Ctrl+N", "Back to List"),
            ("Ctrl+C", "Quit"),
        ];
    }
    vec![
        ("Enter", "cd"),
        ("Tab", "Mark"),
        ("Ctrl+Q", "Clear Marks"),
        ("Ctrl+T", "Toggle List: Active/Archived"),
        ("Ctrl+A", toggle_label(app.kind)),
        ("Ctrl+R", "Move to Trash"),
        ("Ctrl+Z", "Undo Archive/Restore"),
        ("Ctrl+O", OPEN_LABEL),
        ("Ctrl+P", "Toggle Preview: README/Tree"),
        ("Ctrl+N", "New Directory or Clone"),
        ("Ctrl+U", "Clear Query"),
        ("Esc", "Quit"),
    ]
}

/// The shortcut list, centered on top of the screen.
fn draw_help(frame: &mut Frame, app: &App) {
    const GAP: usize = 3;
    let items = help_items(app);
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
    draw_window(
        frame,
        Span::styled(" Help ", bold()),
        lines,
        Line::from(vec![
            Span::styled(" ? / Esc", bold()),
            Span::styled(" Close ", dim()),
        ]),
    );
}

/// What Enter would do, and to which directories. Long lists are cut to fit the screen.
fn draw_confirm(frame: &mut Frame, confirm: &Confirm) {
    let (title, prompt) = match confirm.action {
        Confirmable::Trash => (" Move to Trash ", "Move to the trash:"),
        Confirmable::UndoArchive => (" Undo Archive ", "Back to Active:"),
        Confirmable::UndoRestore => (" Undo Restore ", "Back to Archived:"),
    };
    let title_style = match confirm.action {
        Confirmable::Trash => bold().fg(DANGER_COLOR),
        _ => bold(),
    };
    // Borders and padding (4), the prompt and the blank row under it (2).
    let room = (frame.area().height as usize).saturating_sub(6).max(1);
    let names: Vec<&str> = confirm.targets.iter().map(|e| e.name.as_str()).collect();
    let shown = if names.len() > room {
        room - 1
    } else {
        names.len()
    };
    let mut lines = vec![Line::raw(prompt), Line::default()];
    lines.extend(
        names[..shown]
            .iter()
            .map(|n| Line::styled(format!("  {}", text::sanitize(n)), bold())),
    );
    if shown < names.len() {
        lines.push(Line::styled(
            format!("  and {} more", names.len() - shown),
            dim(),
        ));
    }
    draw_window(
        frame,
        Span::styled(title, title_style),
        lines,
        Line::from(vec![
            Span::styled(" Enter", bold()),
            Span::styled(" OK  ", dim()),
            Span::styled("Esc", bold()),
            Span::styled(" Cancel ", dim()),
        ]),
    );
}

/// A bordered window centered on top of the screen.
fn draw_window(frame: &mut Frame, title: Span, lines: Vec<Line>, bottom: Line) {
    let content_width = lines
        .iter()
        .map(Line::width)
        .chain([title.width(), bottom.width()])
        .max()
        .unwrap_or(0);
    let block = Block::bordered()
        .border_style(dim())
        .padding(Padding::new(2, 2, 1, 1))
        .title(title)
        .title_bottom(bottom);
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
    let prompt = if app.new.open { "+ " } else { "> " };
    // Locked by search results: dimmed and without a cursor, since typing does nothing.
    let locked = app.new.open && app.new.results().is_some();
    let query_style = if locked { dim() } else { Style::default() };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prompt, bold()),
            Span::styled(app.query.clone(), query_style),
        ])),
        area,
    );
    if locked {
        return;
    }
    let x = area.x
        + (prompt.width() + app.query.width()).min(area.width.saturating_sub(1) as usize) as u16;
    frame.set_cursor_position(Position::new(x, area.y));
}

/// The New view: the kinds to choose from, what Enter will do, and search results.
fn draw_new(frame: &mut Frame, app: &App, area: Rect) {
    let new = &app.new;
    let mut lines = vec![
        Line::default(),
        Line::from(vec![
            Span::styled("  Kind", bold()),
            Span::styled("  Tab / Shift+Tab", dim()),
        ]),
    ];
    let label_width = RECIPES.iter().map(|r| r.label().width()).max().unwrap_or(0);
    for (i, recipe) in RECIPES.iter().enumerate() {
        let chosen = i == new.recipe;
        let label = Span::styled(
            format!(" {:<label_width$} ", recipe.label()),
            if chosen {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default()
            },
        );
        let hint = if chosen { recipe.hint() } else { "" };
        lines.push(Line::from(vec![
            Span::raw("  "),
            label,
            Span::styled(format!("  {hint}"), dim()),
        ]));
    }

    if let Recipe::Clone(hub) = new.recipe()
        && hub.is_hugging_face()
    {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("  LFS files: ", bold()),
            Span::raw(if new.lfs {
                "download"
            } else {
                "skip (pointers only)"
            }),
            Span::styled("  Ctrl+L", dim()),
        ]));
    }

    lines.push(Line::default());
    lines.push(match &new.plan {
        Some(Ok(plan)) => Line::from(vec![
            Span::styled("  Enter ", bold()),
            Span::raw(text::sanitize(&describe(plan, &app.home)).into_owned()),
        ]),
        Some(Err(e)) => Line::from(Span::styled(
            format!("  {}", text::sanitize(&format!("{e:#}"))),
            Style::default().fg(DANGER_COLOR),
        )),
        None => Line::default(),
    });

    if let Some(results) = new.results() {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("  {} results", results.hub.label()), bold()),
            Span::styled("  Up / Down, j / k   Esc Edit Search", dim()),
        ]));
        let room = (area.height as usize).saturating_sub(lines.len());
        let offset = (results.cursor + 1).saturating_sub(room);
        let id_width = results.hits.iter().map(|h| h.id.width()).max().unwrap_or(0);
        for (i, hit) in results.hits.iter().enumerate().skip(offset).take(room) {
            let mut line = Line::from(vec![
                Span::raw(format!(
                    "  {:<id_width$}",
                    text::sanitize(&hit.id).into_owned()
                )),
                Span::styled(format!("  {:>11}", hit.meta), dim()),
                Span::styled(format!("  {}", text::sanitize(&hit.description)), dim()),
            ]);
            if i == results.cursor {
                line = line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            lines.push(line);
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn describe(plan: &Plan, home: &Path) -> String {
    let short = |p: &Path| match p.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    };
    match plan {
        Plan::MakeDir(dest) => format!("creates {}", short(dest)),
        Plan::Run(Job::NewGitHubRepo { name, dest }) => format!(
            "creates the private GitHub repository {name} and clones it to {}",
            short(dest)
        ),
        Plan::Run(Job::Clone {
            url,
            dest,
            skip_lfs,
        }) => format!(
            "clones {url} to {}{}",
            short(dest),
            if *skip_lfs {
                " (LFS files skipped)"
            } else {
                ""
            }
        ),
        Plan::Run(Job::Search { hub, query }) => format!("searches {} for {query:?}", hub.label()),
    }
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

/// Braille dots are narrow in every terminal (not East Asian Ambiguous), unlike `◐` and friends.
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const SPINNER_FRAME_MS: u128 = 80;

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    if let Some((label, elapsed)) = app.running() {
        let spinner = SPINNER[(elapsed.as_millis() / SPINNER_FRAME_MS) as usize % SPINNER.len()];
        let line = Line::from(vec![
            Span::raw(format!("{spinner} {} ", text::sanitize(label))),
            Span::styled(
                format!("{}s", elapsed.as_secs()),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);
        return frame.render_widget(Paragraph::new(line), area);
    }
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
