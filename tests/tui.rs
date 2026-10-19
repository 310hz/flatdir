//! The TUI state machine and rendering, driven with synthetic key events on a throwaway HOME.

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use common::Sandbox;
use flatdir::context::Context;
use flatdir::create::{Hub, Recipe};
use flatdir::tui::preview::{self, PreviewMode, Shown};
use flatdir::tui::ui::{self, PreviewCache};
use flatdir::tui::{App, ListKind, Outcome};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn type_text(app: &mut App, ctx: &Context, text: &str) {
    for c in text.chars() {
        app.on_key(ctx, key(KeyCode::Char(c)));
    }
}

fn shown(app: &App) -> Vec<String> {
    app.view
        .iter()
        .map(|v| app.list()[v.idx].name.clone())
        .collect()
}

fn setup() -> (Sandbox, Context, App) {
    let sb = Sandbox::new();
    let ctx = sb.ctx();
    let app = App::new(&ctx).unwrap();
    (sb, ctx, app)
}

#[test]
fn starts_on_the_active_list_in_name_order() {
    let (_sb, _ctx, app) = setup();
    assert_eq!(app.kind, ListKind::Active);
    assert_eq!(shown(&app), ["alpha", "beta", "gamma"]);
}

#[test]
fn late_zoxide_scores_reorder_the_list_and_keep_a_moved_cursor_on_its_entry() {
    let (sb, ctx, mut app) = setup();
    app.apply_scores(HashMap::from([
        (sb.root.join("gamma"), 9.0),
        (sb.root.join("beta"), 3.0),
    ]));
    assert_eq!(shown(&app), ["gamma", "beta", "alpha"]);
    assert_eq!(app.cursor, 0);

    let (sb2, ctx2, mut moved) = setup();
    moved.on_key(&ctx2, key(KeyCode::Down));
    assert_eq!(moved.current().unwrap().name, "beta");
    moved.apply_scores(HashMap::from([
        (sb2.root.join("gamma"), 9.0),
        (sb2.root.join("beta"), 3.0),
    ]));
    assert_eq!(
        moved.current().unwrap().name,
        "beta",
        "cursor stays on the same entry"
    );
    let _ = ctx;
}

#[test]
fn typing_filters_fuzzily_and_enter_returns_the_current_path() {
    let (sb, ctx, mut app) = setup();
    type_text(&mut app, &ctx, "gm");
    assert_eq!(shown(&app), ["gamma"]);
    assert!(
        !app.view[0].hits.is_empty(),
        "matched characters are reported for highlighting"
    );
    assert_eq!(
        app.on_key(&ctx, key(KeyCode::Enter)),
        Outcome::Cd(sb.root.join("gamma"))
    );

    app.on_key(&ctx, ctrl('u'));
    assert_eq!(shown(&app).len(), 3);
    type_text(&mut app, &ctx, "zzz");
    assert!(shown(&app).is_empty());
    assert_eq!(app.on_key(&ctx, key(KeyCode::Enter)), Outcome::Continue);
}

#[test]
fn esc_and_ctrl_c_quit() {
    let (_sb, ctx, mut app) = setup();
    assert_eq!(app.on_key(&ctx, key(KeyCode::Esc)), Outcome::Quit);
    assert_eq!(app.on_key(&ctx, ctrl('c')), Outcome::Quit);
}

#[test]
fn tab_marks_and_moves_and_ctrl_t_resets_marks() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, key(KeyCode::Tab));
    assert_eq!(app.marks.len(), 2);
    assert!(
        app.marks.contains(&sb.root.join("alpha")) && app.marks.contains(&sb.root.join("beta"))
    );
    assert_eq!(app.current().unwrap().name, "gamma");

    app.on_key(&ctx, key(KeyCode::BackTab));
    assert!(
        app.marks.contains(&sb.root.join("gamma")),
        "Shift+Tab marks too and moves up"
    );

    app.on_key(&ctx, ctrl('t'));
    assert_eq!(app.kind, ListKind::Archived);
    assert!(app.marks.is_empty(), "marks reset when the list changes");
}

#[test]
fn the_archive_is_read_only_when_first_needed() {
    let (sb, ctx, mut app) = setup();
    fs::create_dir_all(sb.archive_dir().join("old")).unwrap();
    assert_eq!(
        shown(&app),
        ["alpha", "beta", "gamma"],
        "archive not read at startup"
    );
    app.on_key(&ctx, ctrl('t'));
    assert_eq!(shown(&app), ["old"]);
    assert_eq!(
        app.on_key(&ctx, key(KeyCode::Enter)),
        Outcome::Cd(sb.archive_dir().join("old"))
    );
    app.on_key(&ctx, ctrl('t'));
    assert_eq!(app.kind, ListKind::Active);
}

#[test]
fn ctrl_a_archives_the_current_entry_when_nothing_is_marked() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha"]);
    assert_eq!(shown(&app), ["beta", "gamma"], "the list is refreshed");
    assert!(
        app.status
            .as_ref()
            .is_some_and(|s| !s.is_error && s.text.starts_with("archived: alpha"))
    );
}

#[test]
fn ctrl_a_archives_all_marked_entries() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha", "beta"]);
    assert_eq!(shown(&app), ["gamma"]);
    assert!(app.marks.is_empty());
}

#[test]
fn ctrl_a_reports_leftovers_instead_of_deleting_them() {
    let (sb, ctx, mut app) = setup();
    fs::create_dir_all(sb.root.join("alpha/node_modules")).unwrap();
    app.on_key(&ctx, ctrl('a'));
    assert!(sb.archive_dir().join("alpha/node_modules").is_dir());
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .text
            .contains("alpha contains node_modules")
    );
}

#[test]
fn marking_every_entry_is_refused_by_both_keys_and_nothing_moves() {
    let (sb, ctx, mut app) = setup();
    for _ in 0..3 {
        app.on_key(&ctx, key(KeyCode::Tab));
    }
    assert_eq!(app.marks.len(), 3);

    for k in ['a', 'r'] {
        app.on_key(&ctx, ctrl(k));
        let status = app.status.as_ref().unwrap();
        assert!(
            status.is_error && status.text.contains("every candidate"),
            "Ctrl+{k}"
        );
        assert_eq!(
            sb.names(&sb.root),
            [".hidden", "alpha", "beta", "file.txt", "gamma"]
        );
        assert!(sb.names(&sb.trash_dir).is_empty() && !sb.archive_dir().exists());
    }
}

#[test]
fn ctrl_r_moves_the_current_entry_to_the_trash_and_refreshes_the_list() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Down));
    app.on_key(&ctx, ctrl('r'));

    assert_eq!(sb.names(&sb.trash_dir), ["0-beta"]);
    assert_eq!(shown(&app), ["alpha", "gamma"]);
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .text
            .starts_with("moved to trash: beta")
    );
    assert!(!sb.archive_dir().exists(), "Ctrl+R never archives");

    // The cursor stayed on the same row, which is now gamma; Ctrl+A then archives it.
    app.on_key(&ctx, ctrl('a'));
    assert!(sb.archive_dir().join("gamma").is_dir());
    assert_eq!(
        sb.names(&sb.trash_dir),
        ["0-beta"],
        "nothing else was trashed"
    );
}

#[test]
fn ctrl_r_trashes_every_marked_entry() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, ctrl('r'));
    assert_eq!(sb.names(&sb.trash_dir), ["0-alpha", "1-beta"]);
    assert_eq!(shown(&app), ["gamma"]);
    assert!(app.marks.is_empty());
}

#[test]
fn in_the_archived_list_ctrl_a_restores_and_ctrl_r_trashes() {
    let (sb, ctx, mut app) = setup();
    for d in ["old1", "old2", "old3"] {
        fs::create_dir_all(sb.archive_dir().join(d)).unwrap();
    }
    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma", "old1"]
    );
    assert_eq!(shown(&app), ["old2", "old3"]);
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .text
            .starts_with("restored: old1")
    );

    app.on_key(&ctx, ctrl('r'));
    assert_eq!(sb.names(&sb.trash_dir), ["0-old2"]);
    assert_eq!(sb.names(&sb.archive_dir()), ["old3"]);
    assert_eq!(
        sb.names(&sb.root)
            .iter()
            .filter(|n| n.starts_with("old"))
            .count(),
        1,
        "trashed, not restored"
    );
}

#[test]
fn pasted_text_is_data_and_never_triggers_commands() {
    let (sb, ctx, mut app) = setup();
    // Ctrl+A (0x01, archive) and Ctrl+R (0x12, trash) inside a paste must not act as shortcuts.
    app.on_paste(&ctx, "al\x01\x12pha\n");
    assert_eq!(app.query, "alpha");
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
    assert!(sb.names(&sb.trash_dir).is_empty() && !sb.archive_dir().exists());
    let _ = ctx;
}

#[test]
fn ctrl_p_toggles_the_preview_mode() {
    let (_sb, ctx, mut app) = setup();
    assert_eq!(app.preview_mode, PreviewMode::Markdown);
    app.on_key(&ctx, ctrl('p'));
    assert_eq!(app.preview_mode, PreviewMode::Tree);
    app.on_key(&ctx, ctrl('p'));
    assert_eq!(app.preview_mode, PreviewMode::Markdown);
}

#[test]
fn preview_falls_back_to_the_tree_without_a_readme() {
    let sb = Sandbox::new();
    let dir = sb.root.join("alpha");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.rs"), "").unwrap();

    let (_, shown_kind) = preview::render(&dir, PreviewMode::Markdown);
    assert_eq!(shown_kind, Shown::TreeNoReadme);
    assert_eq!(preview::render(&dir, PreviewMode::Tree).1, Shown::Tree);

    fs::write(dir.join("README.md"), "# Title\n\nhello **world**\n").unwrap();
    let (lines, shown_kind) = preview::render(&dir, PreviewMode::Markdown);
    assert_eq!(shown_kind, Shown::Markdown);
    let text: String = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Title") && text.contains("hello world"),
        "{text}"
    );
    assert_eq!(
        preview::render(&dir, PreviewMode::Tree).1,
        Shown::Tree,
        "Ctrl+P still reaches the tree"
    );
}

#[test]
fn tree_preview_shows_structure_and_skips_hidden_and_ignored_files() {
    let sb = Sandbox::new();
    let dir = sb.root.join("alpha");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join(".gitignore"), "node_modules\n").unwrap();
    fs::write(dir.join("src/main.rs"), "").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();

    let (lines, _) = preview::render(&dir, PreviewMode::Tree);
    let text: String = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("├── src/") && text.contains("│   └── main.rs"),
        "{text}"
    );
    assert!(text.contains("└── Cargo.toml"), "{text}");
    assert!(
        !text.contains("node_modules") && !text.contains(".git"),
        "{text}"
    );
}

thread_local! {
    /// Like the real event loop, keep the preview cache across frames (each test runs on its own thread).
    static CACHE: std::cell::RefCell<PreviewCache> = std::cell::RefCell::new(PreviewCache::default());
}

fn render(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    CACHE.with(|cache| {
        terminal
            .draw(|f| ui::draw(f, app, &mut cache.borrow_mut()))
            .unwrap();
    });
    terminal.backend().buffer().clone()
}

fn screen(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content
        .chunks(buffer.area.width as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_row(buffer: &ratatui::buffer::Buffer) -> String {
    screen(buffer).lines().last().unwrap().to_string()
}

#[test]
fn help_lists_the_shortcuts_and_what_ctrl_a_does_in_each_list() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Char('?')));
    let text = screen(&render(&mut app, 120, 20));
    for expected in [
        "Enter",
        "Tab / Shift+Tab",
        "Ctrl+Q",
        "Ctrl+T",
        "Ctrl+A            Archive",
        "Ctrl+R",
        "Ctrl+O",
        "Ctrl+P",
        "Esc",
    ] {
        assert!(text.contains(expected), "missing {expected:?}:\n{text}");
    }
    app.on_key(&ctx, key(KeyCode::Esc));
    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, key(KeyCode::Char('?')));
    let text = screen(&render(&mut app, 120, 20));
    assert!(text.contains("Ctrl+A            Restore"), "{text}");
}

#[test]
fn marked_rows_get_their_own_color_and_the_current_row_is_reversed() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    let buffer = render(&mut app, 110, 20);

    let find = |sym: &str| {
        buffer
            .content
            .iter()
            .find(|c| c.symbol() == sym)
            .unwrap_or_else(|| panic!("{sym} not drawn"))
    };
    assert_eq!(
        find("▌").fg,
        Color::Magenta,
        "the mark bar uses the mark color"
    );
    let reversed = buffer
        .content
        .iter()
        .any(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED));
    assert!(reversed, "the current row is reverse video");
}

#[test]
fn only_terminal_palette_colors_are_ever_used() {
    let (sb, ctx, mut app) = setup();
    fs::write(
        sb.root.join("alpha/README.md"),
        "# T\n\n`code` and [link](x)\n\n- a\n- b\n\n> quote\n",
    )
    .unwrap();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.status = Some(flatdir_status_error());

    for mode_toggle in [false, true] {
        if mode_toggle {
            app.on_key(&ctx, ctrl('p'));
        }
        let buffer = render(&mut app, 120, 24);
        for cell in &buffer.content {
            for color in [cell.fg, cell.bg, cell.underline_color] {
                assert!(
                    matches!(
                        color,
                        Color::Reset
                            | Color::Black
                            | Color::Red
                            | Color::Green
                            | Color::Yellow
                            | Color::Blue
                            | Color::Magenta
                            | Color::Cyan
                            | Color::Gray
                            | Color::DarkGray
                            | Color::LightRed
                            | Color::LightGreen
                            | Color::LightYellow
                            | Color::LightBlue
                            | Color::LightMagenta
                            | Color::LightCyan
                            | Color::White
                    ),
                    "non-palette color {color:?} on {:?}",
                    cell.symbol()
                );
            }
        }
    }
}

fn flatdir_status_error() -> flatdir::tui::Status {
    flatdir::tui::Status {
        text: "boom".into(),
        is_error: true,
    }
}

#[test]
fn narrow_terminals_drop_the_preview_instead_of_breaking() {
    let (_sb, _ctx, mut app) = setup();
    let text = screen(&render(&mut app, 50, 12));
    assert!(text.contains("alpha") && !text.contains("README"), "{text}");
    let _ = PathBuf::new();
}

/// Regression: `⚠️` (U+26A0 + VS16) in a Japanese sentence made terminals and ratatui disagree about
/// the row width, so the end of a line could spill into the left edge of the next row.
#[test]
fn width_ambiguous_invisible_characters_never_reach_the_screen() {
    let (sb, ctx, mut app) = setup();
    fs::write(
        sb.root.join("alpha/README.md"),
        "\u{FEFF}# \u{1F5F3}\u{FE0F} \u{30BF}\u{30A4}\u{30C8}\u{30EB}\n\n\u{30EB}\u{30A2}\u{30A6}\u{30C8}\u{3067}\u{26A0}\u{FE0F}\u{3092}\u{4F7F}\u{3048}\u{308B}\u{3088} \u{1F468}\u{200D}\u{1F469}\n\n```\n\u{0009}tabbed\n```\n",
    )
    .unwrap();
    fs::create_dir_all(sb.root.join("delta\u{FE0F}")).unwrap();
    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('t'));
    app = App::new(&ctx).unwrap();
    app.status = Some(flatdir::tui::Status {
        text: "archived: x\u{FE0F}y".into(),
        is_error: false,
    });

    let buffer = render(&mut app, 120, 24);
    for cell in &buffer.content {
        let sym = cell.symbol();
        assert!(
            !sym.chars()
                .any(|c| matches!(c, '\u{FE0F}' | '\u{FE0E}' | '\u{200D}' | '\u{FEFF}' | '\t')),
            "width-ambiguous character on screen: {sym:?}"
        );
    }
    let text = screen(&buffer);
    assert!(
        text.contains("\u{26A0}") && text.contains("tabbed"),
        "the visible text is kept:\n{text}"
    );
}

#[test]
fn sanitize_only_touches_invisible_characters_and_tabs() {
    use flatdir::tui::text::sanitize;
    assert_eq!(
        sanitize("plain \u{3072}\u{3089}\u{304C}\u{306A}"),
        "plain \u{3072}\u{3089}\u{304C}\u{306A}"
    );
    assert_eq!(sanitize("a\u{FE0F}b\u{200D}c\u{FEFF}"), "abc");
    assert_eq!(sanitize("\tx"), "    x");
    assert!(matches!(
        sanitize("untouched"),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn ctrl_o_opens_the_current_entry_in_the_file_manager_and_changes_nothing() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Down));
    app.on_key(&ctx, ctrl('o'));
    assert_eq!(*sb.opened.lock().unwrap(), [sb.root.join("beta")]);
    assert_eq!(app.status.as_ref().unwrap().text, "opened: beta");
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
    assert!(sb.names(&sb.trash_dir).is_empty() && !sb.archive_dir().exists());

    fs::create_dir_all(sb.archive_dir().join("old")).unwrap();
    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('o'));
    assert_eq!(
        sb.opened.lock().unwrap().last().unwrap(),
        &sb.archive_dir().join("old")
    );

    type_text(&mut app, &ctx, "zzz");
    app.on_key(&ctx, ctrl('o'));
    assert_eq!(
        sb.opened.lock().unwrap().len(),
        2,
        "nothing is opened when no entry matches"
    );
    assert_eq!(app.status.as_ref().unwrap().text, "nothing to open");
}

#[test]
fn the_title_row_points_to_the_help_at_its_right_end() {
    let (_sb, _ctx, mut app) = setup();
    for width in [60, 100] {
        let text = screen(&render(&mut app, width, 20));
        let title = text.lines().next().unwrap();
        assert!(title.starts_with("flatdir"), "{title:?}");
        assert!(title.ends_with("? Help"), "width {width}: {title:?}");
        assert!(
            !text.contains("Ctrl+"),
            "the shortcuts are only in the help"
        );
    }
}

#[test]
fn help_opens_on_top_and_shows_every_item_whole() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Char('?')));
    let items = [
        "Enter             cd",
        "Tab / Shift+Tab   Mark",
        "Ctrl+Q            Clear Marks",
        "Ctrl+T            Toggle List: Active/Archived",
        "Ctrl+A            Archive",
        "Ctrl+R            Move to Trash",
        "Ctrl+O            Open in",
        "Ctrl+P            Toggle Preview: README/Tree",
        "Ctrl+N            New Directory or Clone",
        "Ctrl+U            Clear Query",
        "Esc               Quit",
    ];
    for width in [60, 80, 100, 140] {
        let text = screen(&render(&mut app, width, 24));
        for item in items {
            assert!(
                text.lines().any(|row| row.contains(item)),
                "width {width}: {item:?} was split or lost:\n{text}"
            );
        }
    }
}

#[test]
fn help_takes_the_keys_until_it_is_closed() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Char('?')));
    assert!(app.help);
    assert!(
        app.query.is_empty(),
        "? opens the help instead of filtering"
    );

    // Nothing reaches the hidden list: no filtering, no actions, no mouse.
    app.on_key(&ctx, key(KeyCode::Char('b')));
    app.on_key(&ctx, ctrl('r'));
    app.on_key(&ctx, ctrl('a'));
    app.on_key(&ctx, key(KeyCode::Enter));
    app.on_paste(&ctx, "beta");
    render(&mut app, 100, 20);
    let list = app.panes.list;
    assert!(!click(&mut app, MouseButton::Right, list.x + 3, list.y + 1));
    assert!(app.query.is_empty() && app.marks.is_empty());
    assert_eq!(shown(&app), ["alpha", "beta", "gamma"]);
    assert_eq!(sb.names(&sb.archive_dir()), Vec::<String>::new());

    assert_eq!(app.on_key(&ctx, key(KeyCode::Esc)), Outcome::Continue);
    assert!(!app.help, "Esc closes the help instead of quitting");
    app.on_key(&ctx, key(KeyCode::Char('?')));
    app.on_key(&ctx, key(KeyCode::Char('?')));
    assert!(!app.help, "? closes it too");
    app.on_key(&ctx, key(KeyCode::Char('?')));
    assert_eq!(app.on_key(&ctx, ctrl('c')), Outcome::Quit);
}

/// The preview text must not touch the vertical separator; the tree's own `│` used to line up with it.
#[test]
fn the_preview_keeps_a_gap_from_the_vertical_line() {
    let (sb, ctx, mut app) = setup();
    let dir = sb.root.join("alpha");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.rs"), "").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    app.on_key(&ctx, ctrl('p'));

    let buffer = render(&mut app, 100, 20);
    let width = buffer.area.width as usize;
    let rows: Vec<Vec<&str>> = buffer
        .content
        .chunks(width)
        .map(|r| r.iter().map(|c| c.symbol()).collect())
        .collect();
    let first = app.panes.list.y as usize;
    let border_x = rows[first]
        .iter()
        .position(|s| *s == "│")
        .expect("vertical line on the first list row");

    let mut checked = 0;
    let height = app.panes.list.height as usize;
    for row in &rows[first + 1..first + height] {
        assert_eq!(row[border_x], "│", "the separator runs the full height");
        let gap = flatdir::tui::ui::PREVIEW_PAD_LEFT as usize;
        assert!(
            row[border_x + 1..=border_x + gap].iter().all(|s| *s == " "),
            "text touches the separator: {row:?}"
        );
        if row[border_x + gap + 1..].iter().any(|s| *s != " ") {
            checked += 1;
        }
    }
    assert!(
        checked >= 4,
        "the tree was drawn ({checked} rows with content)"
    );
}

/// Returns whether the app asked for a redraw.
fn wheel(app: &mut App, down: bool, column: u16, row: u16) -> bool {
    let kind = if down {
        MouseEventKind::ScrollDown
    } else {
        MouseEventKind::ScrollUp
    };
    app.on_mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn long_readme(sb: &Sandbox) {
    let text: String = (0..60).map(|i| format!("line {i}\n\n")).collect();
    fs::write(sb.root.join("alpha/README.md"), text).unwrap();
}

#[test]
fn the_wheel_over_the_list_moves_the_cursor_and_over_the_preview_scrolls_it() {
    let (sb, _ctx, mut app) = setup();
    long_readme(&sb);
    render(&mut app, 100, 20);
    let panes = app.panes;
    let preview = panes.preview.expect("wide terminals have a preview");
    assert!(
        panes.list.right() <= preview.left(),
        "the panes do not overlap"
    );

    // Over the preview: the preview scrolls and the cursor stays put.
    wheel(&mut app, true, preview.x + 5, preview.y + 5);
    assert_eq!((app.cursor, app.preview_scroll), (0, 3));
    let text = screen(&render(&mut app, 100, 20));
    assert!(
        !text.contains("line 0") && text.contains("line 2"),
        "{text}"
    );

    // Over the list: the cursor moves and the preview is untouched.
    wheel(&mut app, true, panes.list.x + 3, panes.list.y);
    assert_eq!(app.cursor, 1);
    wheel(&mut app, false, panes.list.x + 3, panes.list.y);
    assert_eq!(app.cursor, 0);
    assert_eq!(
        app.preview_scroll, 3,
        "the list wheel does not touch the preview"
    );
}

#[test]
fn the_preview_scroll_resets_when_the_previewed_entry_changes() {
    let (sb, ctx, mut app) = setup();
    long_readme(&sb);
    render(&mut app, 100, 20);
    let preview = app.panes.preview.unwrap();
    wheel(&mut app, true, preview.x + 5, preview.y + 5);
    render(&mut app, 100, 20);
    assert_eq!(app.preview_scroll, 3);

    app.on_key(&ctx, key(KeyCode::Down));
    render(&mut app, 100, 20);
    assert_eq!(app.preview_scroll, 0, "a different entry starts at the top");
}

#[test]
fn the_preview_scroll_stops_at_both_ends() {
    let (sb, _ctx, mut app) = setup();
    long_readme(&sb);
    render(&mut app, 100, 20);
    let preview = app.panes.preview.unwrap();
    let (x, y) = (preview.x + 5, preview.y + 5);

    wheel(&mut app, false, x, y);
    assert_eq!(app.preview_scroll, 0, "cannot scroll above the top");

    for _ in 0..200 {
        wheel(&mut app, true, x, y);
    }
    let text = screen(&render(&mut app, 100, 20));
    assert!(
        text.contains("line 59"),
        "the last line is reachable:\n{text}"
    );
    let at_bottom = app.preview_scroll;
    wheel(&mut app, true, x, y);
    render(&mut app, 100, 20);
    assert_eq!(app.preview_scroll, at_bottom, "cannot scroll past the end");
    assert!(
        at_bottom < 200 * 3,
        "clamped to the content, not to the wheel count"
    );
}

#[test]
fn the_wheel_elsewhere_and_other_mouse_events_do_nothing() {
    let (sb, _ctx, mut app) = setup();
    long_readme(&sb);
    render(&mut app, 100, 20);
    assert!(!wheel(&mut app, true, 5, 0), "header: no redraw");
    assert!(!wheel(&mut app, true, 5, 19), "status line: no redraw");
    assert_eq!((app.cursor, app.preview_scroll), (0, 0));

    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: app.panes.list.y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(
        !app.on_mouse(click),
        "a click is not handled, so it must not redraw"
    );
    let motion = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 5,
        row: app.panes.list.y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(!app.on_mouse(motion), "plain motion must not cost a redraw");
    assert_eq!((app.cursor, app.preview_scroll), (0, 0));
}

#[test]
fn without_a_preview_pane_the_wheel_always_drives_the_list() {
    let (_sb, _ctx, mut app) = setup();
    render(&mut app, 50, 20);
    assert!(app.panes.preview.is_none());
    let row = app.panes.list.y;
    wheel(&mut app, true, 45, row);
    assert_eq!(app.cursor, 1);
}

/// Finds `needle` in the rendered screen: (row, column), both 0-based.
fn find_on_screen(text: &str, needle: &str) -> Option<(usize, usize)> {
    text.lines()
        .enumerate()
        .find_map(|(row, line)| line.find(needle).map(|col| (row, col)))
}

/// The title row must not move when the mode changes, because that would shift everything below it.
#[test]
fn the_title_keeps_its_layout_when_the_mode_changes() {
    const MARKS: [&str; 5] = ["flatdir", "Active", "Archived", "? Help", ">"];
    for width in [50, 60, 80, 100, 140] {
        let (sb, ctx, mut app) = setup();
        fs::create_dir_all(sb.archive_dir().join("old")).unwrap();
        let snapshot = |app: &mut App| {
            let text = screen(&render(app, width, 24));
            let marks: Vec<_> = MARKS.iter().map(|k| find_on_screen(&text, k)).collect();
            (marks, app.panes.list.y)
        };
        let baseline = snapshot(&mut app);

        app.on_key(&ctx, ctrl('p'));
        assert_eq!(snapshot(&mut app), baseline, "width {width}: Ctrl+P");
        app.on_key(&ctx, key(KeyCode::Tab));
        assert_eq!(snapshot(&mut app), baseline, "width {width}: marking");
        app.on_key(&ctx, ctrl('t'));
        assert_eq!(snapshot(&mut app), baseline, "width {width}: Ctrl+T");
    }
}

fn click(app: &mut App, button: MouseButton, column: u16, row: u16) -> bool {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(button),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn left_click_moves_the_cursor_to_the_clicked_row() {
    let (_sb, _ctx, mut app) = setup();
    render(&mut app, 100, 20);
    let list = app.panes.list;
    let preview_x = app.panes.preview.unwrap().x + 5;

    assert!(click(&mut app, MouseButton::Left, list.x + 3, list.y + 2));
    assert_eq!(app.current().unwrap().name, "gamma");
    assert!(
        !click(&mut app, MouseButton::Left, list.x + 3, list.y + 2),
        "same row: nothing to redraw"
    );

    // Below the last entry, in the preview and in the header: nothing happens.
    assert!(!click(&mut app, MouseButton::Left, list.x + 3, list.y + 8));
    assert!(!click(&mut app, MouseButton::Left, preview_x, list.y));
    assert!(!click(&mut app, MouseButton::Left, 3, 0));
    assert_eq!(app.current().unwrap().name, "gamma");
    assert!(app.marks.is_empty(), "a left click never marks");
}

#[test]
fn clicking_accounts_for_a_scrolled_list() {
    let (sb, ctx, _app) = setup();
    for i in 0..30 {
        fs::create_dir_all(sb.root.join(format!("dir{i:02}"))).unwrap();
    }
    let mut app = App::new(&ctx).unwrap();
    for _ in 0..25 {
        app.on_key(&ctx, key(KeyCode::Down));
    }
    render(&mut app, 100, 16);
    assert!(app.offset > 0, "the list is scrolled");

    let list = app.panes.list;
    let expected = app.offset + 1;
    click(&mut app, MouseButton::Left, list.x + 3, list.y + 1);
    assert_eq!(app.cursor, expected);
}

#[test]
fn right_click_marks_the_clicked_row_and_leaves_the_cursor_alone() {
    let (sb, ctx, mut app) = setup();
    render(&mut app, 100, 20);
    let list = app.panes.list;

    assert!(click(&mut app, MouseButton::Right, list.x + 3, list.y + 1));
    assert_eq!(app.marks.len(), 1);
    assert!(app.marks.contains(&sb.root.join("beta")));
    assert_eq!(app.cursor, 0, "a right click never moves the cursor");
    assert_eq!(app.current().unwrap().name, "alpha");

    assert!(click(&mut app, MouseButton::Right, list.x + 3, list.y + 2));
    assert_eq!(app.marks.len(), 2);
    assert_eq!(app.cursor, 0);

    assert!(click(&mut app, MouseButton::Right, list.x + 3, list.y + 1));
    assert!(
        !app.marks.contains(&sb.root.join("beta")),
        "a second right click unmarks"
    );

    assert!(
        !click(&mut app, MouseButton::Right, list.x + 3, list.y + 9),
        "no row there"
    );
    assert_eq!(app.marks.len(), 1);

    // Marks made with the mouse are what the keyboard actions use.
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(sb.names(&sb.archive_dir()), ["gamma"]);
}

#[test]
fn ctrl_q_clears_every_mark_and_nothing_else() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, key(KeyCode::Tab));
    assert_eq!((app.marks.len(), app.cursor), (2, 2));

    app.on_key(&ctx, ctrl('q'));
    assert!(app.marks.is_empty());
    assert_eq!(app.cursor, 2, "the cursor stays where it was");
    assert_eq!(app.status.as_ref().unwrap().text, "unmarked 2");
    assert_eq!(app.kind, ListKind::Active);

    // Without marks the actions use the current row only, so a reset really does narrow them.
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(sb.names(&sb.archive_dir()), ["gamma"]);
    assert_eq!(shown(&app), ["alpha", "beta"]);
}

#[test]
fn ctrl_q_without_marks_is_a_quiet_no_op() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('q'));
    assert!(app.marks.is_empty() && app.status.is_none());
}

#[test]
fn the_title_has_room_before_the_tabs_and_a_blank_row_above_the_content() {
    let (_sb, _ctx, mut app) = setup();
    for width in [50, 100] {
        let text = screen(&render(&mut app, width, 24));
        let rows: Vec<&str> = text.lines().collect();

        let tab = rows[0].find("Active").expect("Active tab");
        assert!(
            tab - "flatdir".len() >= 4,
            "width {width}: tabs sit too close to the title: {:?}",
            rows[0]
        );
        assert!(
            rows[1].trim().is_empty(),
            "width {width}: blank row under the title"
        );
        assert!(
            rows[2].starts_with('>'),
            "width {width}: the content follows the blank row"
        );
    }
}

fn type_name(app: &mut App, ctx: &Context, text: &str) {
    for c in text.chars() {
        app.on_key(ctx, key(KeyCode::Char(c)));
    }
}

/// Tab moves through the kinds in `RECIPES` order: Directory, Git, GitHub, HF Model, Dataset, Space.
fn choose_kind(app: &mut App, ctx: &Context, steps: usize) {
    for _ in 0..steps {
        app.on_key(ctx, key(KeyCode::Tab));
    }
}

#[test]
fn ctrl_n_swaps_the_list_for_the_new_view_and_keeps_the_typed_text() {
    let (_sb, ctx, mut app) = setup();
    type_name(&mut app, &ctx, "fresh");
    app.on_key(&ctx, ctrl('n'));
    assert!(app.new.open);
    assert_eq!(app.query, "fresh");

    let text = screen(&render(&mut app, 100, 24));
    assert!(text.contains("+ fresh"), "{text}");
    assert!(!text.contains("alpha"), "the list is hidden:\n{text}");
    assert!(
        text.contains("Directory") && text.contains("Clone: HF Model"),
        "{text}"
    );
    assert!(
        text.contains("Enter creates"),
        "Enter says what it will do:\n{text}"
    );

    assert_eq!(app.on_key(&ctx, key(KeyCode::Esc)), Outcome::Continue);
    assert!(!app.new.open, "Esc goes back instead of quitting");
    app.on_key(&ctx, ctrl('n'));
    app.on_key(&ctx, ctrl('n'));
    assert!(!app.new.open, "Ctrl+N toggles");
}

#[test]
fn enter_in_the_new_view_creates_the_directory_and_lands_on_it() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('n'));
    type_name(&mut app, &ctx, "fresh");
    assert_eq!(app.on_key(&ctx, key(KeyCode::Enter)), Outcome::Continue);

    assert!(sb.root.join("fresh").is_dir());
    assert!(!app.new.open, "back to the list");
    assert_eq!(app.kind, ListKind::Active);
    assert!(app.query.is_empty());
    assert_eq!(app.current().unwrap().name, "fresh");
    assert_eq!(app.status.as_ref().unwrap().text, "created: fresh");
    assert_eq!(
        app.on_key(&ctx, key(KeyCode::Enter)),
        Outcome::Cd(sb.root.join("fresh"))
    );
}

#[test]
fn existing_or_invalid_names_are_refused_and_shown_before_enter() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    type_name(&mut app, &ctx, "alpha");
    let text = screen(&render(&mut app, 100, 24));
    assert!(text.contains("already exists"), "{text}");

    app.on_key(&ctx, key(KeyCode::Enter));
    assert!(app.new.open, "stays open to fix the name");
    assert!(app.status.as_ref().unwrap().is_error);
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
}

#[test]
fn list_actions_are_unreachable_from_the_new_view() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    for c in ['r', 'a', 'o', 't', 'p', 'q'] {
        app.on_key(&ctx, ctrl(c));
    }
    render(&mut app, 100, 24);
    assert!(!click(&mut app, MouseButton::Right, 3, 4));
    assert!(sb.forge.calls().is_empty());
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
    assert!(sb.names(&sb.trash_dir).is_empty() && !sb.archive_dir().exists());
    assert!(sb.opened.lock().unwrap().is_empty());
    assert_eq!(app.kind, ListKind::Active);
}

#[test]
fn git_kind_creates_a_private_repository_in_the_background() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    choose_kind(&mut app, &ctx, 1);
    type_name(&mut app, &ctx, "repo");
    app.on_key(&ctx, key(KeyCode::Enter));
    assert!(app.is_busy());
    app.wait_for_job(&ctx);

    assert_eq!(sb.forge.calls(), ["new repo"]);
    assert_eq!(app.current().unwrap().name, "repo");
    assert!(!app.new.open);
}

#[test]
fn a_running_job_shows_a_spinner_until_it_finishes() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    choose_kind(&mut app, &ctx, 2);
    type_name(&mut app, &ctx, "owner/tool");
    app.on_key(&ctx, key(KeyCode::Enter));

    let status = last_row(&render(&mut app, 100, 24));
    let spinner = status.chars().next().unwrap();
    assert!(('\u{2800}'..='\u{28ff}').contains(&spinner), "{status}");
    assert!(
        status.contains("cloning git@github.com:owner/tool.git 0s"),
        "{status}"
    );

    // Leaving the New view does not hide that something is still running.
    app.on_key(&ctx, key(KeyCode::Esc));
    assert!(last_row(&render(&mut app, 100, 24)).contains("cloning"));

    app.wait_for_job(&ctx);
    let status = last_row(&render(&mut app, 100, 24));
    assert!(status.starts_with("created: tool"), "{status}");
}

#[test]
fn clone_takes_owner_slash_name_directly() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    choose_kind(&mut app, &ctx, 2);
    type_name(&mut app, &ctx, "owner/tool");
    app.on_key(&ctx, key(KeyCode::Enter));
    app.wait_for_job(&ctx);
    assert_eq!(
        sb.forge.calls(),
        ["clone git@github.com:owner/tool.git skip_lfs=false"]
    );
    assert_eq!(app.current().unwrap().name, "tool");
}

#[test]
fn a_word_without_a_slash_is_searched_and_a_result_is_cloned() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    choose_kind(&mut app, &ctx, 4);
    type_name(&mut app, &ctx, "squad");
    app.on_key(&ctx, key(KeyCode::Enter));
    app.wait_for_job(&ctx);
    assert_eq!(sb.forge.calls(), ["search HF Dataset squad"]);

    let text = screen(&render(&mut app, 100, 24));
    assert!(
        text.contains("someone/found-one") && text.contains("other/found-two"),
        "{text}"
    );
    app.on_key(&ctx, key(KeyCode::Down));
    let text = screen(&render(&mut app, 100, 24));
    assert!(
        text.contains("clones https://huggingface.co/datasets/other/found-two"),
        "{text}"
    );

    app.on_key(&ctx, key(KeyCode::Enter));
    app.wait_for_job(&ctx);
    assert_eq!(
        sb.forge.calls().last().unwrap(),
        "clone https://huggingface.co/datasets/other/found-two skip_lfs=true"
    );
    assert!(sb.root.join("found-two").is_dir());
}

#[test]
fn search_results_lock_the_name_until_esc_clears_them() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    choose_kind(&mut app, &ctx, 2);
    type_name(&mut app, &ctx, "tool");
    app.on_key(&ctx, key(KeyCode::Enter));
    app.wait_for_job(&ctx);
    assert!(app.new.results().is_some());

    // Nothing edits the searched word or the kind while its results are on screen.
    type_name(&mut app, &ctx, "xs");
    app.on_key(&ctx, key(KeyCode::Backspace));
    app.on_key(&ctx, ctrl('u'));
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_paste(&ctx, "pasted");
    assert_eq!(app.query, "tool");
    assert_eq!(app.new.recipe(), Recipe::Clone(Hub::GitHub));
    assert!(app.new.results().is_some(), "the results stay");
    let text = screen(&render(&mut app, 100, 24));
    assert!(text.contains("someone/found-one"), "{text}");

    // j / k move like Down / Up, and stop at both ends.
    for (k, expected) in [('j', 1), ('j', 1), ('k', 0), ('k', 0)] {
        app.on_key(&ctx, key(KeyCode::Char(k)));
        assert_eq!(app.new.results().unwrap().cursor, expected, "{k}");
    }

    // Esc clears the results and gives the name back; a second Esc leaves the New view.
    app.on_key(&ctx, key(KeyCode::Esc));
    assert!(app.new.open && app.new.results().is_none());
    type_name(&mut app, &ctx, "s");
    assert_eq!(app.query, "tools");
    app.on_key(&ctx, key(KeyCode::Esc));
    assert!(!app.new.open);
    assert_eq!(sb.forge.calls(), ["search GitHub tool"]);
}

#[test]
fn without_results_j_and_k_are_just_letters() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    type_name(&mut app, &ctx, "jk");
    assert_eq!(app.query, "jk");
}

#[test]
fn ctrl_l_downloads_lfs_files_only_for_hugging_face() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    app.on_key(&ctx, ctrl('l'));
    assert!(!app.new.lfs, "no LFS option outside Hugging Face");

    choose_kind(&mut app, &ctx, 3);
    let text = screen(&render(&mut app, 100, 24));
    assert!(text.contains("LFS files: skip"), "{text}");
    app.on_key(&ctx, ctrl('l'));
    assert!(app.new.lfs);
    type_name(&mut app, &ctx, "openai/whisper");
    app.on_key(&ctx, key(KeyCode::Enter));
    app.wait_for_job(&ctx);
    assert_eq!(
        sb.forge.calls(),
        ["clone https://huggingface.co/openai/whisper skip_lfs=false"]
    );
}

#[test]
fn help_in_the_new_view_lists_its_own_keys() {
    let (_sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('n'));
    app.on_key(&ctx, key(KeyCode::Char('?')));
    assert!(app.help && app.query.is_empty());
    let text = screen(&render(&mut app, 100, 24));
    assert!(
        text.contains("Choose Kind") && text.contains("Back to List"),
        "{text}"
    );
    app.on_key(&ctx, key(KeyCode::Esc));
    assert!(app.new.open, "closing the help keeps the New view");
}

#[test]
fn the_title_marks_the_new_view_without_moving() {
    let (_sb, ctx, mut app) = setup();
    let before = screen(&render(&mut app, 100, 24));
    app.on_key(&ctx, ctrl('n'));
    let after = screen(&render(&mut app, 100, 24));
    for mark in ["Active", "Archived", "New", "? Help"] {
        assert_eq!(
            find_on_screen(&before, mark),
            find_on_screen(&after, mark),
            "{mark}"
        );
    }
}
