//! TUI state machine. Pure with respect to the terminal, so it is tested without one.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

use crate::candidates::{self, Entry};
use crate::context::Context;
use crate::create::{self, Done, Hit, Hub, Job, Plan, RECIPES, Recipe};
use crate::guard;
use crate::ops;
use crate::services::play_trash_sound;

use super::preview::PreviewMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Active,
    Archived,
}

/// Ctrl+R moves to the trash. Ctrl+A toggles the archived state: archive in the active list,
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

/// The New view (Ctrl+N): the query line becomes the name, the list makes way for the options.
#[derive(Default)]
pub struct NewForm {
    pub open: bool,
    /// Index into `RECIPES`.
    pub recipe: usize,
    /// Download Git LFS files of Hugging Face clones.
    pub lfs: bool,
    pub search: Option<SearchResults>,
    /// What Enter would do with the current input, recomputed after every change.
    pub plan: Option<anyhow::Result<Plan>>,
}

impl NewForm {
    pub fn recipe(&self) -> Recipe {
        RECIPES[self.recipe]
    }

    /// While results are shown the name and kind are locked, so they always match the results.
    pub fn results(&self) -> Option<&SearchResults> {
        self.search.as_ref()
    }
}

pub struct SearchResults {
    pub hub: Hub,
    pub query: String,
    pub hits: Vec<Hit>,
    pub cursor: usize,
}

pub struct View {
    pub idx: usize,
    /// Character positions matched by the query, for highlighting.
    pub hits: Vec<u32>,
}

struct Running {
    rx: Receiver<Done>,
    label: String,
    since: Instant,
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
    /// The shortcut list is open on top of everything; keys go to it instead of the query.
    pub help: bool,
    pub new: NewForm,
    /// Paths on screen are shortened to `~/...` with this.
    pub home: PathBuf,
    /// A create, clone or search running on a worker thread; one at a time.
    job: Option<Running>,
    /// First visible row of the preview. Reset by the UI when the previewed entry or mode changes.
    pub preview_scroll: u16,
    active: Vec<Entry>,
    archived: Vec<Entry>,
    scores: HashMap<PathBuf, f64>,
    matcher: Matcher,
}

impl App {
    pub fn new(ctx: &Context) -> anyhow::Result<Self> {
        let lists = candidates::lists(ctx)?;
        let mut app = Self {
            kind: ListKind::Active,
            preview_mode: PreviewMode::Markdown,
            query: String::new(),
            cursor: 0,
            offset: 0,
            marks: HashSet::new(),
            status: None,
            view: Vec::new(),
            panes: Panes::default(),
            help: false,
            new: NewForm::default(),
            home: ctx.env.home.clone(),
            job: None,
            preview_scroll: 0,
            active: lists.active,
            archived: lists.archived,
            scores: HashMap::new(),
            matcher: Matcher::new(MatcherConfig::DEFAULT),
        };
        app.forget(ctx, &lists.missing);
        app.refilter();
        Ok(app)
    }

    pub fn list(&self) -> &[Entry] {
        match self.kind {
            ListKind::Active => &self.active,
            ListKind::Archived => &self.archived,
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
        sort_by_score(&mut self.active, &self.scores);
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

    /// Mouse input, routed by the pane under the pointer:
    /// - wheel over the list moves the cursor, over the preview scrolls it;
    /// - left click on a list row moves the cursor there;
    /// - right click on a list row marks / unmarks it (what Tab does, but the cursor stays put).
    ///
    /// Returns whether the screen needs a redraw: terminals also report plain mouse motion, which
    /// must not cost a redraw each time.
    pub fn on_mouse(&mut self, event: MouseEvent) -> bool {
        // The list under the help window or the New view is hidden; never click or scroll it blindly.
        if self.help || self.new.open {
            return false;
        }
        let at = Position::new(event.column, event.row);
        let direction = match event.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::Down(MouseButton::Left) => {
                return match self.row_at(at) {
                    Some(index) if index != self.cursor => {
                        self.cursor = index;
                        true
                    }
                    _ => false,
                };
            }
            MouseEventKind::Down(MouseButton::Right) => {
                return match self.row_at(at) {
                    Some(index) => {
                        self.toggle_mark_at(index);
                        true
                    }
                    None => false,
                };
            }
            _ => return false,
        };
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

    pub fn on_paste(&mut self, ctx: &Context, text: &str) {
        if self.help || (self.new.open && (self.new.search.is_some() || self.is_busy())) {
            return;
        }
        // Pasted text is data, never commands: control characters are dropped.
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        if self.new.open {
            self.refresh_plan(ctx);
        } else {
            self.refilter();
        }
    }

    pub fn on_key(&mut self, ctx: &Context, key: KeyEvent) -> Outcome {
        if key.kind == KeyEventKind::Release {
            return Outcome::Continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // While the help is open, nothing else may run: an action on the hidden list would surprise.
        if self.help {
            match (key.code, ctrl) {
                (KeyCode::Char('c'), true) => return Outcome::Quit,
                (KeyCode::Esc | KeyCode::Char('?'), _) => self.help = false,
                _ => {}
            }
            return Outcome::Continue;
        }
        if self.new.open {
            return self.on_new_key(ctx, key, ctrl);
        }
        match (key.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), true) => return Outcome::Quit,
            (KeyCode::Enter, _) => {
                return match self.current() {
                    Some(e) => Outcome::Cd(e.path.clone()),
                    None => Outcome::Continue,
                };
            }
            (KeyCode::Char('t'), true) => self.toggle_list(),
            (KeyCode::Char('a'), true) => self.execute(ctx, Op::ToggleArchived),
            (KeyCode::Char('p'), true) => self.preview_mode = self.preview_mode.toggled(),
            (KeyCode::Char('r'), true) => self.execute(ctx, Op::Trash),
            (KeyCode::Char('o'), true) => self.open_current(ctx),
            (KeyCode::Char('q'), true) => self.clear_marks(),
            (KeyCode::Char('n'), true) => {
                self.new.open = true;
                self.status = None;
                self.refresh_plan(ctx);
            }
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
            (KeyCode::Char('?'), false) => self.help = true,
            (KeyCode::Char(c), false) if !key.modifiers.contains(KeyModifiers::ALT) => {
                self.query.push(c);
                self.refilter();
            }
            _ => {}
        }
        Outcome::Continue
    }

    /// Only name editing and the New view's own keys. Actions on the hidden list (archive, trash,
    /// open) are deliberately not reachable from here.
    ///
    /// Search results lock the name and kind (Esc unlocks them by clearing the results), and so
    /// does a running job, so results never belong to text that is no longer there.
    fn on_new_key(&mut self, ctx: &Context, key: KeyEvent, ctrl: bool) -> Outcome {
        let showing = self.new.search.is_some();
        let editable = !showing && !self.is_busy();
        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) => return Outcome::Quit,
            (KeyCode::Esc, _) if showing => {
                self.new.search = None;
                self.status = None;
            }
            (KeyCode::Esc, _) | (KeyCode::Char('n'), true) => {
                self.new.open = false;
                self.new.search = None;
                self.status = None;
                self.refilter();
            }
            (KeyCode::Char('?'), false) => self.help = true,
            (KeyCode::Char('l'), true) => {
                if matches!(self.new.recipe(), Recipe::Clone(hub) if hub.is_hugging_face()) {
                    self.new.lfs = !self.new.lfs;
                }
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), false) if showing => self.move_hit(1),
            (KeyCode::Up, _) | (KeyCode::Char('k'), false) if showing => self.move_hit(-1),
            (KeyCode::PageDown, _) => self.move_hit(10),
            (KeyCode::PageUp, _) => self.move_hit(-10),
            (KeyCode::Enter, _) => self.submit_new(ctx),
            (KeyCode::Tab, _) if editable => self.cycle_recipe(1),
            (KeyCode::BackTab, _) if editable => self.cycle_recipe(-1),
            (KeyCode::Char('u'), true) if editable => self.query.clear(),
            (KeyCode::Backspace, _) if editable => {
                self.query.pop();
            }
            (KeyCode::Char(c), false) if editable && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.query.push(c)
            }
            _ => {}
        }
        if self.new.open {
            self.refresh_plan(ctx);
        }
        Outcome::Continue
    }

    fn refresh_plan(&mut self, ctx: &Context) {
        self.new.plan = Some(self.new_plan(ctx));
    }

    fn cycle_recipe(&mut self, delta: isize) {
        let n = RECIPES.len() as isize;
        self.new.recipe = (self.new.recipe as isize + delta).rem_euclid(n) as usize;
    }

    fn move_hit(&mut self, delta: isize) {
        if let Some(results) = self.new.search.as_mut() {
            let last = results.hits.len().saturating_sub(1) as isize;
            results.cursor = (results.cursor as isize + delta).clamp(0, last) as usize;
        }
    }

    /// What Enter would do now; the New view shows it before it happens.
    pub fn new_plan(&self, ctx: &Context) -> anyhow::Result<Plan> {
        let recipe = self.new.recipe();
        if let (Recipe::Clone(hub), Some(results)) = (recipe, self.new.results())
            && let Some(hit) = results.hits.get(results.cursor)
        {
            return create::plan_clone(ctx, hub, &hit.id, self.new.lfs).map(Plan::Run);
        }
        create::plan(ctx, recipe, &self.query, self.new.lfs)
    }

    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    /// What the running job is doing, and for how long.
    pub fn running(&self) -> Option<(&str, Duration)> {
        self.job
            .as_ref()
            .map(|job| (job.label.as_str(), job.since.elapsed()))
    }

    fn submit_new(&mut self, ctx: &Context) {
        if self.is_busy() {
            return;
        }
        match self.new_plan(ctx) {
            Ok(Plan::MakeDir(dest)) => match fs::create_dir(&dest) {
                Ok(()) => self.created(ctx, dest),
                Err(e) => self.fail(
                    anyhow::Error::new(e).context(format!("failed to create {}", dest.display())),
                ),
            },
            Ok(Plan::Run(job)) => self.start(ctx, job),
            Err(e) => self.fail(e),
        }
    }

    fn start(&mut self, ctx: &Context, job: Job) {
        self.status = None;
        let label = job.describe();
        let forge = Arc::clone(&ctx.forge);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(create::run(forge.as_ref(), job));
        });
        self.job = Some(Running {
            rx,
            label,
            since: Instant::now(),
        });
    }

    /// Picks up a finished job without waiting. Returns whether the screen changed.
    pub fn poll_job(&mut self, ctx: &Context) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        match job.rx.try_recv() {
            Ok(done) => {
                self.job = None;
                self.finish(ctx, done);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.fail(anyhow::anyhow!("the background task stopped unexpectedly"));
                true
            }
        }
    }

    /// Blocks until the running job, if any, has finished. For tests.
    pub fn wait_for_job(&mut self, ctx: &Context) {
        if let Some(job) = self.job.take() {
            match job.rx.recv() {
                Ok(done) => self.finish(ctx, done),
                Err(_) => self.fail(anyhow::anyhow!("the background task stopped unexpectedly")),
            }
        }
    }

    fn finish(&mut self, ctx: &Context, done: Done) {
        match done {
            Done::Created(Ok(dest)) => self.created(ctx, dest),
            Done::Created(Err(e)) => self.fail(e),
            Done::Found { hub, query, hits } => match hits {
                Ok(hits) if hits.is_empty() => self.info(format!("nothing found: {query}")),
                // Closed meanwhile: results would lock a form nobody is looking at.
                Ok(_) if !self.new.open => {}
                Ok(hits) => {
                    self.info(format!("{} found", hits.len()));
                    self.new.search = Some(SearchResults {
                        hub,
                        query,
                        hits,
                        cursor: 0,
                    });
                }
                Err(e) => self.fail(e),
            },
        }
        if self.new.open {
            self.refresh_plan(ctx);
        }
    }

    /// Back to the Active list with the cursor on the new entry, so Enter goes straight there.
    fn created(&mut self, ctx: &Context, dest: PathBuf) {
        self.new.open = false;
        self.new.search = None;
        self.query.clear();
        if self.kind != ListKind::Active {
            self.kind = ListKind::Active;
            self.marks.clear();
        }
        self.reload(ctx);
        self.cursor = self
            .view
            .iter()
            .position(|v| self.list()[v.idx].path == dest)
            .unwrap_or(0);
        let name = dest.file_name().unwrap_or_default().to_string_lossy();
        self.info(format!("created: {name}"));
    }

    fn move_cursor(&mut self, delta: isize) {
        let last = self.view.len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    fn toggle_mark_at(&mut self, index: usize) {
        if let Some(view) = self.view.get(index) {
            let path = self.list()[view.idx].path.clone();
            if !self.marks.remove(&path) {
                self.marks.insert(path);
            }
        }
    }

    fn clear_marks(&mut self) {
        let count = self.marks.len();
        self.marks.clear();
        if count > 0 {
            self.info(format!("unmarked {count}"));
        }
    }

    fn mark_and_move(&mut self, delta: isize) {
        self.toggle_mark_at(self.cursor);
        self.move_cursor(delta);
    }

    /// Index into `view` of the list row under `at`, if there is one.
    fn row_at(&self, at: Position) -> Option<usize> {
        if !self.panes.list.contains(at) {
            return None;
        }
        let index = self.offset + (at.y - self.panes.list.y) as usize;
        (index < self.view.len()).then_some(index)
    }

    fn toggle_list(&mut self) {
        let next = match self.kind {
            ListKind::Active => ListKind::Archived,
            ListKind::Archived => ListKind::Active,
        };
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
            (ListKind::Active, Op::ToggleArchived) => {
                let archived = ops::archive(ctx, targets)?;
                let notes: Vec<String> = archived
                    .iter()
                    .filter(|m| !m.leftovers.is_empty())
                    .map(|m| format!("{} contains {}", m.name, m.leftovers.join(", ")))
                    .collect();
                let mut msg = format!(
                    "archived: {}",
                    names(archived.iter().map(|m| m.name.clone()).collect())
                );
                if !notes.is_empty() {
                    msg.push_str(&format!(" ({})", notes.join("; ")));
                }
                msg
            }
            (ListKind::Archived, Op::ToggleArchived) => {
                format!("restored: {}", names(ops::restore(ctx, targets)?))
            }
            (_, Op::Trash) => format!("moved to trash: {}", names(ops::remove(ctx, targets)?)),
        })
    }

    /// Re-reads the lists after a change on disk.
    fn reload(&mut self, ctx: &Context) {
        if let Ok(mut lists) = candidates::lists(ctx) {
            sort_by_score(&mut lists.active, &self.scores);
            self.active = lists.active;
            self.archived = lists.archived;
            self.forget(ctx, &lists.missing);
        }
        self.refilter();
    }

    /// Archives deleted or renamed by hand are dropped from the state, so a later directory with
    /// the same name does not start out hidden.
    fn forget(&mut self, ctx: &Context, missing: &[PathBuf]) {
        if let Err(e) = ops::forget(ctx, missing) {
            self.fail(e);
        }
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
            ListKind::Active => &self.active[..],
            ListKind::Archived => &self.archived[..],
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
