//! The TUI state machine and rendering, driven with synthetic key events on a throwaway HOME.

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use common::Sandbox;
use flatdir::context::Context;
use flatdir::tui::preview::{self, PreviewMode, Shown};
use flatdir::tui::ui::{self, PreviewCache};
use flatdir::tui::{Action, App, ListKind, Outcome};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
fn starts_on_the_tracked_list_in_name_order_with_the_safe_action() {
    let (_sb, _ctx, app) = setup();
    assert_eq!((app.kind, app.action), (ListKind::Tracked, Action::Safe));
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
fn tab_marks_and_moves_and_ctrl_a_resets_marks() {
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
    assert_eq!(app.action, Action::Remove);
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(app.kind, ListKind::Archived);
    assert!(app.marks.is_empty(), "marks reset when the list changes");
    assert_eq!(
        app.action,
        Action::Safe,
        "the destructive action never carries over"
    );
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
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(shown(&app), ["old"]);
    assert_eq!(
        app.on_key(&ctx, key(KeyCode::Enter)),
        Outcome::Cd(sb.archive_dir().join("old"))
    );
    app.on_key(&ctx, ctrl('a'));
    assert_eq!(app.kind, ListKind::Tracked);
}

#[test]
fn ctrl_r_archives_the_current_entry_when_nothing_is_marked() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, ctrl('r'));
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha"]);
    assert_eq!(shown(&app), ["beta", "gamma"], "the list is refreshed");
    assert!(
        app.status
            .as_ref()
            .is_some_and(|s| !s.is_error && s.text.starts_with("archived: alpha"))
    );
}

#[test]
fn ctrl_r_archives_all_marked_entries() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, key(KeyCode::Tab));
    app.on_key(&ctx, ctrl('r'));
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha", "beta"]);
    assert_eq!(shown(&app), ["gamma"]);
    assert!(app.marks.is_empty());
}

#[test]
fn ctrl_r_reports_leftovers_instead_of_deleting_them() {
    let (sb, ctx, mut app) = setup();
    fs::create_dir_all(sb.root.join("alpha/node_modules")).unwrap();
    app.on_key(&ctx, ctrl('r'));
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
fn marking_every_entry_is_refused_and_nothing_moves() {
    let (sb, ctx, mut app) = setup();
    for _ in 0..3 {
        app.on_key(&ctx, key(KeyCode::Tab));
    }
    assert_eq!(app.marks.len(), 3);
    app.on_key(&ctx, ctrl('r'));
    assert!(app.status.as_ref().unwrap().is_error);
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .text
            .contains("every candidate")
    );
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );

    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('r'));
    assert!(
        sb.names(&sb.trash_dir).is_empty(),
        "the same guard covers removal"
    );
}

#[test]
fn removal_needs_the_toggle_and_the_action_resets_afterwards() {
    let (sb, ctx, mut app) = setup();
    app.on_key(&ctx, key(KeyCode::Down));
    app.on_key(&ctx, ctrl('t'));
    assert_eq!(app.action, Action::Remove);
    app.on_key(&ctx, ctrl('r'));

    assert_eq!(sb.names(&sb.trash_dir), ["0-beta"]);
    assert_eq!(shown(&app), ["alpha", "gamma"]);
    assert_eq!(
        app.action,
        Action::Safe,
        "the next Ctrl+R is a safe archive again"
    );
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .text
            .starts_with("moved to trash: beta")
    );

    // The cursor stayed on the same row, which is now gamma.
    app.on_key(&ctx, ctrl('r'));
    assert!(sb.archive_dir().join("gamma").is_dir(), "back to archiving");
    assert_eq!(
        sb.names(&sb.trash_dir),
        ["0-beta"],
        "nothing else was trashed"
    );
}

#[test]
fn ctrl_r_in_the_archived_list_restores_and_remove_trashes_from_the_archive() {
    let (sb, ctx, mut app) = setup();
    for d in ["old1", "old2", "old3"] {
        fs::create_dir_all(sb.archive_dir().join(d)).unwrap();
    }
    app.on_key(&ctx, ctrl('a'));
    app.on_key(&ctx, ctrl('r'));
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma", "old1"]
    );
    assert_eq!(shown(&app), ["old2", "old3"]);

    app.on_key(&ctx, ctrl('t'));
    app.on_key(&ctx, ctrl('r'));
    assert_eq!(sb.names(&sb.trash_dir), ["0-old2"]);
    assert_eq!(sb.names(&sb.archive_dir()), ["old3"]);
}

#[test]
fn pasted_text_is_data_and_never_triggers_commands() {
    let (sb, ctx, mut app) = setup();
    // Ctrl+T (0x14) and Ctrl+R (0x12) inside a paste must not act as shortcuts.
    app.on_paste("al\x14\x12pha\n");
    assert_eq!(app.query, "alpha");
    assert_eq!(app.action, Action::Safe);
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
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

fn render(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut cache = PreviewCache::default();
    terminal.draw(|f| ui::draw(f, app, &mut cache)).unwrap();
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

#[test]
fn the_header_lists_the_shortcuts_and_the_current_action() {
    let (_sb, ctx, mut app) = setup();
    let text = screen(&render(&mut app, 110, 20));
    for expected in [
        "Tracked", "Archived", "Enter", "Tab", "Ctrl+A", "Ctrl+R", "Archive", "Ctrl+T", "Ctrl+P",
        "Esc",
    ] {
        assert!(text.contains(expected), "missing {expected:?}:\n{text}");
    }
    app.on_key(&ctx, ctrl('t'));
    let text = screen(&render(&mut app, 110, 20));
    assert!(
        text.contains("REMOVE (to Trash)") && text.contains("REMOVE mode"),
        "{text}"
    );
    app.on_key(&ctx, ctrl('a'));
    let text = screen(&render(&mut app, 110, 20));
    assert!(text.contains("Restore"), "{text}");
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
    app.on_key(&ctx, ctrl('t'));
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
    app.on_key(&ctx, ctrl('a'));
    app.on_key(&ctx, ctrl('a'));
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
    app.on_key(&ctx, ctrl('a'));
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
fn the_header_mentions_ctrl_o() {
    let (_sb, _ctx, mut app) = setup();
    assert!(screen(&render(&mut app, 120, 20)).contains("Ctrl+O"));
}

#[test]
fn the_shortcut_line_wraps_on_narrow_terminals_without_losing_keys() {
    let (_sb, _ctx, mut app) = setup();
    for width in [40, 60, 80, 100] {
        let text = screen(&render(&mut app, width, 24));
        for expected in [
            "Enter", "Ctrl+A", "Ctrl+R", "Ctrl+T", "Ctrl+O", "Ctrl+P", "Esc quit",
        ] {
            assert!(
                text.contains(expected),
                "width {width}: missing {expected:?}\n{text}"
            );
        }
        assert!(
            text.contains("alpha"),
            "width {width}: the list is still shown\n{text}"
        );
    }
}
