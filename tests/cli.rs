//! 実バイナリを、使い捨ての HOME と偽の fzf だけで動かす。環境変数は全て作り直す。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Sandbox {
    tmp: PathBuf,
    _guard: TempDir,
    home: PathBuf,
    root: PathBuf,
    fzf_log: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let guard = TempDir::new().unwrap();
        let tmp = fs::canonicalize(guard.path()).unwrap();
        let home = tmp.join("home");
        let root = home.join("ws");
        for d in ["alpha", "beta"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        let bin = tmp.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let fzf_log = tmp.join("fzf.log");
        write_exe(
            &bin.join("fzf"),
            "#!/bin/sh\nprintf '%s' \"${FZF_DEFAULT_OPTS-unset}\" > \"$FAKE_FZF_LOG\"\nhead -n1\n",
        );
        write_exe(
            &bin.join("trash"),
            "#!/bin/sh\necho trash-called >> \"$FAKE_FZF_LOG.trash\"\n",
        );
        Self {
            tmp,
            _guard: guard,
            home,
            root,
            fzf_log,
        }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let path = format!("{}:/usr/bin:/bin", self.tmp.join("bin").display());
        Command::new(env!("CARGO_BIN_EXE_flatdir"))
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", path)
            .env("FAKE_FZF_LOG", &self.fzf_log)
            .envs(extra_env.iter().copied())
            .output()
            .unwrap()
    }

    fn names(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

fn write_exe(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn track_list_and_select_round_trip() {
    let sb = Sandbox::new();
    assert!(
        sb.run(&["track", sb.root.to_str().unwrap()], &[])
            .status
            .success()
    );
    assert_eq!(stdout(&sb.run(&["list"], &[])), sb.root.to_str().unwrap());

    let out = sb.run(&[], &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), sb.root.join("alpha").to_str().unwrap());
}

/// 2026-10-18 の事故の再現: 非対話 + `--filter=` で全件選択された状態で remove / restore / archive を実行する。
#[test]
fn incident_regression_noninteractive_filter_env_changes_nothing() {
    let sb = Sandbox::new();
    assert!(
        sb.run(&["track", sb.root.to_str().unwrap()], &[])
            .status
            .success()
    );
    fs::create_dir_all(sb.tmp.join("home/.local/share/flatdir/archive/ws/old")).unwrap();

    for sub in ["remove", "archive", "restore"] {
        let out = sb.run(&[sub], &[("FZF_DEFAULT_OPTS", "--filter=")]);
        assert!(!out.status.success(), "{sub} must fail");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("対話端末"),
            "{sub}"
        );
    }

    assert_eq!(sb.names(&sb.root), ["alpha", "beta"]);
    assert_eq!(
        sb.names(&sb.tmp.join("home/.local/share/flatdir/archive/ws")),
        ["old"]
    );
    assert!(!sb.fzf_log.exists(), "fzf must not even start");
    assert!(!sb.tmp.join("fzf.log.trash").exists());
}

#[test]
fn dangerous_fzf_env_is_dropped_but_ordinary_one_is_kept() {
    let sb = Sandbox::new();
    assert!(
        sb.run(&["track", sb.root.to_str().unwrap()], &[])
            .status
            .success()
    );

    let out = sb.run(&[], &[("FZF_DEFAULT_OPTS", "--filter=")]);
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(&sb.fzf_log).unwrap(), "unset");
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning"));

    let out = sb.run(&[], &[("FZF_DEFAULT_OPTS", "--color=light")]);
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(&sb.fzf_log).unwrap(), "--color=light");
}
