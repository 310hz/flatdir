//! Shared throwaway environment. Real paths and the real trash are never touched.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flatdir::candidates;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::paths::Env;
use flatdir::select::{SelectOpts, Selector};
use flatdir::services::{Opener, Prompter, ScoreSource, Trasher};
use tempfile::TempDir;

pub struct NeverSelector;

impl Selector for NeverSelector {
    fn select(&self, _: &[String], _: &SelectOpts) -> anyhow::Result<Vec<String>> {
        panic!("selector must not be called");
    }
}

pub struct RowsSelector(pub Vec<String>);

impl Selector for RowsSelector {
    fn select(&self, _: &[String], _: &SelectOpts) -> anyhow::Result<Vec<String>> {
        Ok(self.0.clone())
    }
}

/// Moves into a throwaway directory instead of the real trash.
pub struct FakeTrasher {
    pub dir: PathBuf,
    pub trashed: RefCell<Vec<PathBuf>>,
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

pub struct FakeOpener(pub Arc<Mutex<Vec<PathBuf>>>);

impl Opener for FakeOpener {
    fn open(&self, path: &Path) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

pub struct FakePrompter(pub bool);

impl Prompter for FakePrompter {
    fn confirm(&self, _: &str) -> bool {
        self.0
    }
}

pub struct FakeScores(pub HashMap<PathBuf, f64>);

impl ScoreSource for FakeScores {
    fn scores(&self) -> HashMap<PathBuf, f64> {
        self.0.clone()
    }
}

pub struct Sandbox {
    _tmp: TempDir,
    pub home: PathBuf,
    pub root: PathBuf,
    pub archive_root: PathBuf,
    pub trash_dir: PathBuf,
    pub config_path: PathBuf,
    pub opened: Arc<Mutex<Vec<PathBuf>>>,
}

impl Sandbox {
    pub fn new() -> Self {
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
            opened: Arc::default(),
        }
    }

    pub fn ctx(&self) -> Context {
        Context {
            env: Env {
                home: self.home.clone(),
                config_path: self.config_path.clone(),
                archive_root: self.archive_root.clone(),
            },
            config: Config {
                root: Some(self.root.clone()),
                color: None,
                sound: false,
            },
            interactive: true,
            selector: Box::new(NeverSelector),
            trasher: Box::new(FakeTrasher {
                dir: self.trash_dir.clone(),
                trashed: RefCell::new(vec![]),
            }),
            prompter: Box::new(FakePrompter(false)),
            opener: Box::new(FakeOpener(Arc::clone(&self.opened))),
            scores: Arc::new(FakeScores(HashMap::new())),
        }
    }

    pub fn names(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .map(|r| {
                r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    pub fn archive_dir(&self) -> PathBuf {
        self.archive_root.join("ws")
    }

    pub fn entries(&self, names: &[&str]) -> Vec<candidates::Entry> {
        names
            .iter()
            .map(|n| candidates::Entry {
                name: n.to_string(),
                path: self.root.join(n),
            })
            .collect()
    }

    pub fn archived_entries(&self, names: &[&str]) -> Vec<candidates::Entry> {
        names
            .iter()
            .map(|n| candidates::Entry {
                name: n.to_string(),
                path: self.archive_dir().join(n),
            })
            .collect()
    }
}
