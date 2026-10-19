//! TUI state machine. Pure with respect to the terminal, so it is tested without one.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

use crate::candidates::{self, Entry};
use crate::context::Context;
use crate::guard;
use crate::ops;
use crate::services::play_trash_sound;

use super::preview::PreviewMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Tracked,
    Archived,
}

/// Ctrl+R moves to the trash. Ctrl+T toggles the archived state: archive in the tracked list,
/// restore in the archived list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Trash,
    ToggleArchived,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Quit,
    Cd(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

/// Where the panes were last drawn; the mouse wheel is routed by position.
#[derive(Debug, Default, Clone, Copy)]
pub struct Panes {
    pub list: Rect,
    /// `None` when the terminal is too narrow for a preview.
    pub preview: Option<Rect>,
}

/// Rows per wheel notch.
const LIST_WHEEL_STEP: isize = 1;
const PREVIEW_WHEEL_STEP: isize = 3;

pub struct View {
    pub idx: usize,
    /// Character positions matched by the query, for highlighting.
    pub hits: Vec<u32>,
}

pub struct App {
    pub kind: ListKind,
    pub preview_mode: PreviewMode,
    pub query: String,
    pub cursor: usize,
    pub offset: usize,
    pub marks: HashSet<PathBuf>,
    pub status: Option<Status>,
    pub view: Vec<View>,
    pub panes: Panes,
    /// First visible row of the preview. Reset by the UI when the previewed entry or mode changes.
    pub preview_scroll: u16,
    tracked: Vec<Entry>,
    /// Loaded on first use; most sessions never open the archive.
    archived: Option<Vec<Entry>>,
    scores: HashMap<PathBuf, f64>,
    matcher: Matcher,
}

impl App {
    pub fn new(ctx: &Context) -> anyhow::Result<Self> {
        let tracked = candidates::list_children(ctx.root()?)?;
        let mut app = Self {
            kind: ListKind::Tracked,
            preview_mode: PreviewMode::Markdown,
            query: String::new(),
            cursor: 0,
            offset: 0,
            marks: HashSet::new(),
            status: None,
            view: Vec::new(),
            panes: Panes::default(),
            preview_scroll: 0,
            tracked,
            archived: None,
            scores: HashMap::new(),
            matcher: Matcher::new(MatcherConfig::DEFAULT),
        };
        app.refilter();
        Ok(app)
    }

    pub fn list(&self) -> &[Entry] {
        match self.kind {
            ListKind::Tracked => &self.tracked,
            ListKind::Archived => self.archived.as_deref().unwrap_or(&[]),
        }
    }

    pub fn current(&self) -> Option<&Entry> {
        self.view.get(self.cursor).map(|v| &self.list()[v.idx])
    }

    /// Scores arrive after the first frame (zoxide is slow), so the order is corrected in place.
    pub fn apply_scores(&mut self, scores: HashMap<PathBuf, f64>) {
        let keep = (self.cursor > 0 || !self.query.is_empty())
            .then(|| self.current().map(|e| e.path.clone()))
            .flatten();
        self.scores = scores;
        sort_by_score(&mut self.tracked, &self.scores);
        self.refilter();
        if let Some(path) = keep
            && let Some(pos) = self
                .view
                .iter()
                .position(|v| self.list()[v.idx].path == path)
        {
            self.cursor = pos;
        }
    }

    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
    }

    /// The wheel moves the cursor over the list and scrolls the preview over the preview pane.
    /// Returns whether the screen needs a redraw: terminals also report plain mouse motion, which
    /// must not cost a redraw each time.
    pub fn on_mouse(&mut self, event: MouseEvent) -> bool {
        let direction = match event.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => return false,
        };
        let at = Position::new(event.column, event.row);
        if self.panes.preview.is_some_and(|p| p.contains(at)) {
            let next = self.preview_scroll as isize + direction * PREVIEW_WHEEL_STEP;
            self.preview_scroll = next.clamp(0, u16::MAX as isize) as u16;
            true
        } else if self.panes.list.contains(at) {
            self.move_cursor(direction * LIST_WHEEL_STEP);
            true
        } else {
            false
        }
    }

    pub fn on_paste(&mut self, text: &str) {
        // Pasted text is data, never commands: control characters are dropped.
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        self.refilter();
    }

    pub fn on_key(&mut self, ctx: &Context, key: KeyEvent) -> Outcome {
        if key.kind == KeyEventKind::Release {
            return Outcome::Continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), true) => return Outcome::Quit,
            (KeyCode::Enter, _) => {
                return match self.current() {
                    Some(e) => Outcome::Cd(e.path.clone()),
                    None => Outcome::Continue,
                };
            }
            (KeyCode::Char('a'), true) => self.toggle_list(ctx),
            (KeyCode::Char('t'), true) => self.execute(ctx, Op::ToggleArchived),
            (KeyCode::Char('p'), true) => self.preview_mode = self.preview_mode.toggled(),
            (KeyCode::Char('r'), true) => self.execute(ctx, Op::Trash),
            (KeyCode::Char('o'), true) => self.open_current(ctx),
            (KeyCode::Char('u'), true) => {
                self.query.clear();
                self.refilter();
            }
            (KeyCode::Tab, _) => self.mark_and_move(1),
            (KeyCode::BackTab, _) => self.mark_and_move(-1),
            (KeyCode::Down, _) => self.move_cursor(1),
            (KeyCode::Up, _) => self.move_cursor(-1),
            (KeyCode::PageDown, _) => self.move_cursor(10),
            (KeyCode::PageUp, _) => self.move_cursor(-10),
            (KeyCode::Backspace, _) => {
                self.query.pop();
                self.refilter();
            }
            (KeyCode::Char(c), false) if !key.modifiers.contains(KeyModifiers::ALT) => {
                self.query.push(c);
                self.refilter();
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn move_cursor(&mut self, delta: isize) {
        let last = self.view.len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    fn mark_and_move(&mut self, delta: isize) {
        if let Some(path) = self.current().map(|e| e.path.clone())
            && !self.marks.remove(&path)
        {
            self.marks.insert(path);
        }
        self.move_cursor(delta);
    }

    fn toggle_list(&mut self, ctx: &Context) {
        let next = match self.kind {
            ListKind::Tracked => ListKind::Archived,
            ListKind::Archived => ListKind::Tracked,
        };
        if next == ListKind::Archived && self.archived.is_none() {
            match candidates::archived(ctx) {
                Ok(list) => self.archived = Some(list),
                Err(e) => return self.fail(e),
            }
        }
        self.kind = next;
        self.marks.clear();
        self.cursor = 0;
        self.offset = 0;
        self.status = None;
        self.refilter();
    }

    fn execute(&mut self, ctx: &Context, op: Op) {
        let list = self.list();
        let targets: Vec<Entry> = if self.marks.is_empty() {
            self.current().cloned().into_iter().collect()
        } else {
            list.iter()
                .filter(|e| self.marks.contains(&e.path))
                .cloned()
                .collect()
        };
        if targets.is_empty() {
            return self.info("nothing to act on");
        }
        if let Err(e) = guard::reject_select_all(list.len(), targets.len()) {
            return self.fail(e);
        }

        let result = self.run_op(ctx, op, &targets);
        match result {
            Ok(message) => {
                if op == Op::Trash && ctx.config.sound {
                    play_trash_sound();
                }
                self.marks.clear();
                self.reload(ctx);
                self.info(message);
            }
            Err(e) => self.fail(e),
        }
    }

    fn open_current(&mut self, ctx: &Context) {
        let Some(entry) = self.current() else {
            return self.info("nothing to open");
        };
        let (name, path) = (entry.name.clone(), entry.path.clone());
        match ctx.opener.open(&path) {
            Ok(()) => self.info(format!("opened: {name}")),
            Err(e) => self.fail(e),
        }
    }

    fn run_op(&self, ctx: &Context, op: Op, targets: &[Entry]) -> anyhow::Result<String> {
        let names = |v: Vec<String>| v.join(", ");
        Ok(match (self.kind, op) {
            (ListKind::Tracked, Op::ToggleArchived) => {
                let moved = ops::archive(ctx, targets)?;
                let notes: Vec<String> = moved
                    .iter()
                    .filter(|m| !m.leftovers.is_empty())
                    .map(|m| format!("{} contains {}", m.name, m.leftovers.join(", ")))
                    .collect();
                let mut msg = format!(
                    "archived: {}",
                    names(moved.iter().map(|m| m.name.clone()).collect())
                );
                if !notes.is_empty() {
                    msg.push_str(&format!(" ({})", notes.join("; ")));
                }
                msg
            }
            (ListKind::Archived, Op::ToggleArchived) => {
                let moved = ops::restore(ctx, targets)?;
                format!(
                    "restored: {}",
                    names(moved.into_iter().map(|m| m.name).collect())
                )
            }
            (ListKind::Tracked, Op::Trash) => {
                let root = ctx.root()?.to_path_buf();
                format!(
                    "moved to trash: {}",
                    names(ops::remove(ctx, targets, &root)?)
                )
            }
            (ListKind::Archived, Op::Trash) => {
                let base = ctx.archive_base()?;
                format!(
                    "moved to trash: {}",
                    names(ops::remove(ctx, targets, &base)?)
                )
            }
        })
    }

    /// Re-reads the lists after a change on disk. The archive is reloaded lazily unless it is on screen.
    fn reload(&mut self, ctx: &Context) {
        if let Ok(mut list) = ctx.root().and_then(candidates::list_children) {
            sort_by_score(&mut list, &self.scores);
            self.tracked = list;
        }
        self.archived = None;
        if self.kind == ListKind::Archived {
            self.archived = candidates::archived(ctx).ok();
        }
        self.refilter();
    }

    fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            is_error: false,
        });
    }

    fn fail(&mut self, e: anyhow::Error) {
        self.status = Some(Status {
            text: format!("{e:#}"),
            is_error: true,
        });
    }

    fn refilter(&mut self) {
        let list = match self.kind {
            ListKind::Tracked => &self.tracked[..],
            ListKind::Archived => self.archived.as_deref().unwrap_or(&[]),
        };
        if self.query.is_empty() {
            self.view = (0..list.len())
                .map(|idx| View {
                    idx,
                    hits: Vec::new(),
                })
                .collect();
        } else {
            let pattern = Pattern::parse(&self.query, CaseMatching::Smart, Normalization::Smart);
            let mut buf = Vec::new();
            let mut scored = Vec::new();
            for (idx, entry) in list.iter().enumerate() {
                let hay = Utf32Str::new(&entry.name, &mut buf);
                if let Some(score) = pattern.score(hay, &mut self.matcher) {
                    let mut hits = Vec::new();
                    pattern.indices(hay, &mut self.matcher, &mut hits);
                    hits.sort_unstable();
                    hits.dedup();
                    scored.push((score, idx, hits));
                }
            }
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.view = scored
                .into_iter()
                .map(|(_, idx, hits)| View { idx, hits })
                .collect();
        }
        self.cursor = self.cursor.min(self.view.len().saturating_sub(1));
    }
}

fn sort_by_score(entries: &mut [Entry], scores: &HashMap<PathBuf, f64>) {
    let score = |e: &Entry| scores.get(&e.path).copied().unwrap_or(0.0);
    entries.sort_by(|a, b| {
        score(b)
            .total_cmp(&score(a))
            .then_with(|| a.name.cmp(&b.name))
    });
}
