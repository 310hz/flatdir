//! Everything runs on a throwaway HOME. Real paths, fzf and trash are never touched.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use flatdir::cmd;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::paths::Env;
use flatdir::select::{SelectOpts, Selector};
use flatdir::services::{Prompter, ScoreSource, Trasher, parse_zoxide};
use tempfile::TempDir;

type Chooser = Box<dyn Fn(&[String]) -> Vec<String>>;

struct FakeSelector(Chooser);

impl Selector for FakeSelector {
    fn select(&self, lines: &[String], _: &SelectOpts) -> anyhow::Result<Vec<String>> {
        Ok((self.0)(lines))
    }
}

/// Fails if called; verifies the non-interactive guard acts before fzf is ever consulted.
struct NeverSelector;

impl Selector for NeverSelector {
    fn select(&self, _: &[String], _: &SelectOpts) -> anyhow::Result<Vec<String>> {
        panic!("selector must not be called");
    }
}

struct FakeTrasher {
    dir: PathBuf,
    trashed: RefCell<Vec<PathBuf>>,
}

impl Trasher for FakeTrasher {
    fn trash(&self, path: &Path) -> anyhow::Result<()> {
        let n = self.trashed.borrow().len();
        fs::rename(
            path,
            self.dir.join(format!(
                "{n}-{}",
                path.file_name().unwrap().to_string_lossy()
            )),
        )?;
        self.trashed.borrow_mut().push(path.to_path_buf());
        Ok(())
    }
}

struct FakePrompter(bool);

impl Prompter for FakePrompter {
    fn confirm(&self, _: &str) -> bool {
        self.0
    }
}

struct FakeScores(HashMap<PathBuf, f64>);

impl ScoreSource for FakeScores {
    fn scores(&self) -> HashMap<PathBuf, f64> {
        self.0.clone()
    }
}

struct Sandbox {
    _tmp: TempDir,
    home: PathBuf,
    root: PathBuf,
    archive_root: PathBuf,
    trash_dir: PathBuf,
    config_path: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        // Canonicalize to avoid symlinks such as macOS /var -> /private/var.
        let base = fs::canonicalize(tmp.path()).unwrap();
        let home = base.join("home");
        let root = home.join("ws");
        for d in ["alpha", "beta", "gamma", ".hidden"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("file.txt"), "x").unwrap();
        let trash_dir = base.join("trash");
        fs::create_dir_all(&trash_dir).unwrap();
        Self {
            archive_root: base.join("data/archive"),
            config_path: base.join("cfg/config.toml"),
            _tmp: tmp,
            home,
            root,
            trash_dir,
        }
    }

    fn ctx(&self, selector: Box<dyn Selector>, interactive: bool) -> Context {
        Context {
            env: Env {
                home: self.home.clone(),
                config_path: self.config_path.clone(),
                archive_root: self.archive_root.clone(),
            },
            config: Config {
                root: Some(self.root.clone()),
                color: None,
            },
            interactive,
            selector,
            trasher: Box::new(FakeTrasher {
                dir: self.trash_dir.clone(),
                trashed: RefCell::new(vec![]),
            }),
            prompter: Box::new(FakePrompter(false)),
            scores: Box::new(FakeScores(HashMap::new())),
        }
    }

    fn names(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .map(|r| {
                r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
}

fn pick_names(names: &'static [&'static str]) -> Box<dyn Selector> {
    Box::new(FakeSelector(Box::new(move |lines| {
        lines
            .iter()
            .filter(|l| names.iter().any(|n| l.split('\t').next() == Some(n)))
            .cloned()
            .collect()
    })))
}

fn pick_all() -> Box<dyn Selector> {
    Box::new(FakeSelector(Box::new(|lines| lines.to_vec())))
}

fn archive_dir(sb: &Sandbox) -> PathBuf {
    sb.archive_root.join("ws")
}

#[test]
fn candidates_are_sorted_by_score_and_skip_hidden_and_files() {
    let sb = Sandbox::new();
    let mut ctx = sb.ctx(pick_names(&[]), true);
    ctx.scores = Box::new(FakeScores(HashMap::from([
        (sb.root.join("gamma"), 9.0),
        (sb.root.join("beta"), 3.0),
    ])));
    let names: Vec<String> = flatdir::candidates::candidates(&ctx)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["gamma", "beta", "alpha"]);
}

#[test]
fn select_returns_chosen_path() {
    let sb = Sandbox::new();
    let ctx = sb.ctx(pick_names(&["beta"]), true);
    assert_eq!(cmd::select(&ctx).unwrap(), Some(sb.root.join("beta")));
}

#[test]
fn select_commands_require_an_interactive_terminal() {
    let sb = Sandbox::new();
    let ctx = sb.ctx(Box::new(NeverSelector), false);
    assert!(
        cmd::select(&ctx)
            .unwrap_err()
            .to_string()
            .contains("interactive terminal")
    );
    assert!(
        cmd::select_archived(&ctx)
            .unwrap_err()
            .to_string()
            .contains("interactive terminal")
    );
}

#[test]
fn archive_moves_only_selected_without_writing_a_marker() {
    let sb = Sandbox::new();
    fs::write(sb.root.join("alpha/keep.txt"), "x").unwrap();
    let ctx = sb.ctx(pick_names(&["alpha"]), true);
    cmd::archive(&ctx).unwrap();

    assert_eq!(sb.names(&sb.root), [".hidden", "beta", "file.txt", "gamma"]);
    assert_eq!(sb.names(&archive_dir(&sb)), ["alpha"]);
    assert_eq!(sb.names(&archive_dir(&sb).join("alpha")), ["keep.txt"]);
}

#[test]
fn restore_returns_dir_and_removes_legacy_marker() {
    let sb = Sandbox::new();
    let legacy = archive_dir(&sb).join("old");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join(".flatdir_archived"), "").unwrap();
    fs::write(legacy.join("data.txt"), "x").unwrap();
    fs::create_dir_all(archive_dir(&sb).join(".hidden-archive")).unwrap();

    let ctx = sb.ctx(pick_names(&["old"]), true);
    cmd::restore(&ctx).unwrap();

    assert_eq!(sb.names(&sb.root.join("old")), ["data.txt"]);
    assert!(sb.names(&archive_dir(&sb)).iter().all(|n| n != "old"));
}

#[test]
fn legacy_archives_are_listed_without_relying_on_markers() {
    let sb = Sandbox::new();
    fs::create_dir_all(archive_dir(&sb).join("with-marker")).unwrap();
    fs::write(archive_dir(&sb).join("with-marker/.flatdir_archived"), "").unwrap();
    fs::create_dir_all(archive_dir(&sb).join("without-marker")).unwrap();

    let ctx = sb.ctx(pick_names(&[]), true);
    let names: Vec<String> = flatdir::candidates::archived(&ctx)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["with-marker", "without-marker"]);
}

#[test]
fn remove_trashes_only_selected() {
    let sb = Sandbox::new();
    let ctx = sb.ctx(pick_names(&["beta"]), true);
    cmd::remove(&ctx).unwrap();

    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "file.txt", "gamma"]
    );
    assert_eq!(sb.names(&sb.trash_dir), ["0-beta"]);
}

#[test]
fn destructive_commands_refuse_select_all() {
    for run in [
        cmd::remove as fn(&Context) -> anyhow::Result<()>,
        cmd::archive,
    ] {
        let sb = Sandbox::new();
        let ctx = sb.ctx(pick_all(), true);
        let err = run(&ctx).unwrap_err().to_string();
        assert!(err.contains("every candidate"), "{err}");
        assert_eq!(
            sb.names(&sb.root),
            [".hidden", "alpha", "beta", "file.txt", "gamma"]
        );
        assert!(sb.names(&sb.trash_dir).is_empty());
        assert!(!archive_dir(&sb).exists());
    }

    let sb = Sandbox::new();
    for d in ["a1", "a2"] {
        fs::create_dir_all(archive_dir(&sb).join(d)).unwrap();
    }
    let err = cmd::restore(&sb.ctx(pick_all(), true))
        .unwrap_err()
        .to_string();
    assert!(err.contains("every candidate"), "{err}");
    assert_eq!(sb.names(&archive_dir(&sb)), ["a1", "a2"]);
}

#[test]
fn destructive_commands_refuse_when_not_interactive_before_asking_fzf() {
    for run in [
        cmd::remove as fn(&Context) -> anyhow::Result<()>,
        cmd::archive,
        cmd::restore,
    ] {
        let sb = Sandbox::new();
        let ctx = sb.ctx(Box::new(NeverSelector), false);
        assert!(
            run(&ctx)
                .unwrap_err()
                .to_string()
                .contains("interactive terminal")
        );
        assert_eq!(
            sb.names(&sb.root),
            [".hidden", "alpha", "beta", "file.txt", "gamma"]
        );
    }
}

#[test]
fn unknown_row_from_selector_changes_nothing() {
    let sb = Sandbox::new();
    let evil = Box::new(FakeSelector(Box::new(|_| vec!["x\t/etc".to_string()])));
    let err = cmd::remove(&sb.ctx(evil, true)).unwrap_err().to_string();
    assert!(err.contains("unknown row"), "{err}");
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
}

#[test]
fn archive_refuses_when_destination_exists_and_moves_nothing() {
    let sb = Sandbox::new();
    fs::create_dir_all(archive_dir(&sb).join("beta")).unwrap();
    let ctx = sb.ctx(pick_names(&["alpha", "beta"]), true);
    let err = cmd::archive(&ctx).unwrap_err().to_string();
    assert!(err.contains("destination exists"), "{err}");
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "beta", "file.txt", "gamma"]
    );
}

#[test]
fn restore_refuses_when_destination_exists() {
    let sb = Sandbox::new();
    fs::create_dir_all(archive_dir(&sb).join("alpha")).unwrap();
    fs::create_dir_all(archive_dir(&sb).join("zeta")).unwrap();
    let err = cmd::restore(&sb.ctx(pick_names(&["alpha"]), true))
        .unwrap_err()
        .to_string();
    assert!(err.contains("destination exists"), "{err}");
    assert_eq!(sb.names(&archive_dir(&sb)), ["alpha", "zeta"]);
}

#[test]
fn archive_cleanup_only_trashes_when_confirmed() {
    for (answer, trashed) in [(true, true), (false, false)] {
        let sb = Sandbox::new();
        fs::create_dir_all(sb.root.join("alpha/node_modules")).unwrap();
        let mut ctx = sb.ctx(pick_names(&["alpha"]), true);
        ctx.prompter = Box::new(FakePrompter(answer));
        cmd::archive(&ctx).unwrap();
        assert_eq!(
            archive_dir(&sb).join("alpha/node_modules").exists(),
            !trashed
        );
        assert_eq!(!sb.names(&sb.trash_dir).is_empty(), trashed);
    }
}

#[test]
fn track_sets_a_single_root() {
    let sb = Sandbox::new();
    let other = sb.home.join("other");
    fs::create_dir_all(&other).unwrap();

    let mut ctx = sb.ctx(pick_names(&[]), false);
    ctx.config.root = None;

    let (root, added) = cmd::track(&mut ctx, &sb.root).unwrap();
    assert_eq!((root.as_path(), added), (sb.root.as_path(), true));
    assert_eq!(
        Config::load(&sb.config_path).unwrap().root.as_deref(),
        Some(sb.root.as_path())
    );

    assert!(!cmd::track(&mut ctx, &sb.root).unwrap().1);
    let err = cmd::track(&mut ctx, &other).unwrap_err().to_string();
    assert!(err.contains("already tracking"), "{err}");

    cmd::untrack(&mut ctx).unwrap();
    assert_eq!(Config::load(&sb.config_path).unwrap().root, None);
}

#[test]
fn track_rejects_paths_outside_home() {
    let sb = Sandbox::new();
    let outside = sb._tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let mut ctx = sb.ctx(pick_names(&[]), false);
    ctx.config.root = None;
    assert!(cmd::track(&mut ctx, &outside).is_err());
    assert!(cmd::track(&mut ctx, &sb.home).is_err());
}

#[test]
fn init_creates_plain_dir_and_rejects_escaping_names() {
    let sb = Sandbox::new();
    let ctx = sb.ctx(pick_names(&[]), false);
    for bad in ["../x", "a/b", ".hidden2", "", ".."] {
        assert!(cmd::init(&ctx, bad, false).is_err(), "{bad:?}");
    }
    assert!(cmd::init(&ctx, "alpha", false).is_err());

    fs::create_dir_all(archive_dir(&sb).join("archived-name")).unwrap();
    assert!(cmd::init(&ctx, "archived-name", false).is_err());

    assert_eq!(
        cmd::init(&ctx, "fresh", false).unwrap(),
        sb.root.join("fresh")
    );
    assert!(sb.root.join("fresh").is_dir());
}

#[test]
fn config_ignores_unknown_legacy_keys_and_roundtrips_color() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.config_path.parent().unwrap()).unwrap();
    fs::write(
        &sb.config_path,
        "root = \"/x\"\nfzf_opts = [\"--filter=x\"]\ncolor = \"light\"\n",
    )
    .unwrap();
    let config = Config::load(&sb.config_path).unwrap();
    assert_eq!(config.root.as_deref(), Some(Path::new("/x")));
    assert_eq!(config.color.as_deref(), Some("light"));
}

#[test]
fn zoxide_output_with_spaces_in_paths_is_parsed() {
    let m = parse_zoxide("  12.5 /a/b c\n   3.0 /d\nbroken\n");
    assert_eq!(m[Path::new("/a/b c")], 12.5);
    assert_eq!(m[Path::new("/d")], 3.0);
    assert_eq!(m.len(), 2);
}

#[test]
fn status_reports_counts_and_survives_a_missing_root() {
    let sb = Sandbox::new();
    for d in ["a1", "a2"] {
        fs::create_dir_all(archive_dir(&sb).join(d)).unwrap();
    }
    let ctx = sb.ctx(pick_names(&[]), false);
    let status = cmd::status(&ctx);
    let root = status.root.as_ref().unwrap();
    assert_eq!(
        (root.path.as_path(), root.directories, root.archived),
        (sb.root.as_path(), Some(3), 2)
    );
    assert_eq!(root.archive_dir, archive_dir(&sb));
    assert!(status.tools.iter().any(|t| t.name == "zoxide"));
    let text = status.to_string();
    assert!(
        text.contains("directories: 3") && text.contains("archived:    2"),
        "{text}"
    );

    let mut missing = sb.ctx(pick_names(&[]), false);
    missing.config.root = Some(sb.home.join("nowhere"));
    let status = cmd::status(&missing);
    assert_eq!(status.root.as_ref().unwrap().directories, None);
    assert!(status.to_string().contains("not a readable directory"));

    let mut unset = sb.ctx(pick_names(&[]), false);
    unset.config.root = None;
    assert!(cmd::status(&unset).to_string().contains("not set"));
}
