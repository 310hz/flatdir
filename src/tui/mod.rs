mod app;
mod markdown;
pub mod preview;
pub mod text;
mod tree;
pub mod ui;

use std::io::{self, BufWriter, Stderr, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::sync::Arc;

use crate::context::Context;
use crate::guard;

pub use app::{App, ListKind, NewForm, Outcome, Panes, SearchResults, Status};
pub use preview::{PreviewMode, Shown};

/// Draws on stderr: stdout is reserved for the selected path, which the shell wrapper captures.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(
            io::stderr(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        // If the terminal ever disagrees with ratatui about a character's width, a line that
        // overflows must not wrap into the left edge of the next row.
        io::stderr().write_all(AUTOWRAP_OFF)?;
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            default_hook(info);
        }));
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

const AUTOWRAP_OFF: &[u8] = b"\x1b[?7l";
const AUTOWRAP_ON: &[u8] = b"\x1b[?7h";

fn restore() {
    let _ = io::stderr().write_all(AUTOWRAP_ON);
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stderr(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}

/// Runs the TUI. `Some(path)` means the user pressed Enter on a directory.
pub fn run(ctx: &Context) -> Result<Option<PathBuf>> {
    guard::require_interactive(ctx, "flatdir")?;
    let mut app = App::new(ctx)?;

    // zoxide takes tens of milliseconds; the first frame must not wait for it.
    let (tx, scores_rx) = mpsc::channel();
    let source = Arc::clone(&ctx.scores);
    std::thread::spawn(move || {
        let _ = tx.send(source.scores());
    });

    let _guard = TerminalGuard::enter()?;
    let mut terminal: Terminal<CrosstermBackend<BufWriter<Stderr>>> =
        Terminal::new(CrosstermBackend::new(BufWriter::new(io::stderr())))?;
    let mut cache = ui::PreviewCache::default();
    let mut waiting_for_scores = true;
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|frame| ui::draw(frame, &mut app, &mut cache))?;
            dirty = false;
        }
        if waiting_for_scores {
            match scores_rx.try_recv() {
                Ok(scores) => {
                    app.apply_scores(scores);
                    waiting_for_scores = false;
                    dirty = true;
                    continue;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => waiting_for_scores = false,
            }
        }

        if app.poll_job(ctx) {
            dirty = true;
            continue;
        }

        let wait = if waiting_for_scores {
            Duration::from_millis(5)
        } else if app.is_busy() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(250)
        };
        if !event::poll(wait)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => match app.on_key(ctx, key) {
                Outcome::Quit => return Ok(None),
                Outcome::Cd(path) => return Ok(Some(path)),
                Outcome::Continue => dirty = true,
            },
            Event::Paste(text) => {
                app.on_paste(ctx, &text);
                dirty = true;
            }
            Event::Mouse(mouse) => dirty |= app.on_mouse(mouse),
            Event::Resize(..) => dirty = true,
            _ => {}
        }
    }
}
