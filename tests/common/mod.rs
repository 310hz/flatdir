//! Shared throwaway environment. Real paths and the real trash are never touched.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flatdir::archive::{Record, State};
use flatdir::candidates;
use flatdir::config::Config;
use flatdir::context::Context;
use flatdir::create::{Hit, Hub};
use flatdir::paths::Env;
use flatdir::services::{Forge, Opener, ScoreSource, Trasher};
use tempfile::TempDir;

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

/// Records what would have been run and creates the destination instead of using the network.
pub struct FakeForge {
    pub calls: Mutex<Vec<String>>,
    pub hits: Vec<Hit>,
}

impl FakeForge {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for FakeForge {
    fn default() -> Self {
        let hit = |id: &str| Hit {
            id: id.to_string(),
            meta: "1 stars".to_string(),
            description: String::new(),
        };
        Self {
            calls: Mutex::default(),
            hits: vec![hit("someone/found-one"), hit("other/found-two")],
        }
    }
}

impl Forge for FakeForge {
    fn new_github_repo(&self, name: &str, dest: &Path) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!("new {name}"));
        fs::create_dir(dest)?;
        Ok(())
    }

    fn clone_repo(&self, url: &str, dest: &Path, skip_lfs: bool) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("clone {url} skip_lfs={skip_lfs}"));
        fs::create_dir(dest)?;
        Ok(())
    }

    fn search(&self, hub: Hub, query: &str) -> anyhow::Result<Vec<Hit>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("search {} {query}", hub.label()));
        Ok(self.hits.clone())
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
    pub state_path: PathBuf,
    pub trash_dir: PathBuf,
    pub config_path: PathBuf,
    pub opened: Arc<Mutex<Vec<PathBuf>>>,
    pub forge: Arc<FakeForge>,
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
            state_path: base.join("state/state.json"),
            config_path: base.join("cfg/config.toml"),
            _tmp: tmp,
            home,
            root,
            trash_dir,
            opened: Arc::default(),
            forge: Arc::default(),
        }
    }

    pub fn ctx(&self) -> Context {
        Context {
            env: Env {
                home: self.home.clone(),
                config_path: self.config_path.clone(),
                state_path: self.state_path.clone(),
            },
            config: Config {
                root: Some(self.root.clone()),
                sound: false,
            },
            interactive: true,
            trasher: Box::new(FakeTrasher {
                dir: self.trash_dir.clone(),
                trashed: RefCell::new(vec![]),
            }),
            opener: Box::new(FakeOpener(Arc::clone(&self.opened))),
            forge: Arc::clone(&self.forge) as Arc<dyn Forge>,
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

    /// Creates the directories in the root and records them as archived, the way Ctrl+A leaves them.
    pub fn archive(&self, names: &[&str]) {
        let mut state = State::load(&self.state_path).unwrap();
        for n in names {
            let path = self.root.join(n);
            fs::create_dir_all(&path).unwrap();
            state.archived.insert(path, Record { archived_at: 1 });
        }
        state.save(&self.state_path).unwrap();
    }

    /// Recorded archives as paths relative to the root (or absolute, for another root), sorted.
    pub fn archived(&self) -> Vec<String> {
        State::load(&self.state_path)
            .unwrap()
            .archived
            .keys()
            .map(|p| {
                p.strip_prefix(&self.root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
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
}
