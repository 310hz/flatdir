//! Runs the real binary inside a pseudo-terminal with a throwaway HOME and a rebuilt environment.
//! The destructive `remove` is not driven here: the real trash lives outside the sandbox.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tempfile::TempDir;

struct Sandbox {
    _guard: TempDir,
    tmp: PathBuf,
    home: PathBuf,
    root: PathBuf,
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
        fs::create_dir_all(tmp.join("bin")).unwrap();
        Self {
            _guard: guard,
            tmp,
            home,
            root,
        }
    }

    fn path_env(&self) -> String {
        format!("{}:/usr/bin:/bin", self.tmp.join("bin").display())
    }

    fn fake_zoxide(&self, scored: &Path) {
        let body = format!("#!/bin/sh\necho '  9.0 {}'\n", scored.display());
        let path = self.tmp.join("bin/zoxide");
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Without a terminal: stdin is null and stderr is a pipe.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_flatdir"))
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", self.path_env())
            .output()
            .unwrap()
    }

    fn track(&self) {
        let out = self.run(&["track", self.root.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
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

    fn archive_dir(&self) -> PathBuf {
        self.home.join(".local/share/flatdir/archive/ws")
    }

    /// Drives the binary in a pty: waits for `wait_for` to appear, sends `keys`, returns all output.
    fn run_in_pty(
        &self,
        args: &[&str],
        extra_env: &[(&str, &str)],
        wait_for: &str,
        keys: &str,
    ) -> String {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_flatdir"));
        cmd.args(args);
        cmd.env_clear();
        cmd.env("HOME", &self.home);
        cmd.env("PATH", self.path_env());
        cmd.env("TERM", "xterm-256color");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);

        let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));

        let (out, w) = (Arc::clone(&output), Arc::clone(&writer));
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];
                // The picker asks for the cursor position (ESC[6n) and waits for the terminal to answer.
                for _ in 0..chunk.windows(4).filter(|x| *x == b"\x1b[6n").count() {
                    let _ = w.lock().unwrap().write_all(b"\x1b[1;1R");
                }
                out.lock().unwrap().extend_from_slice(chunk);
            }
        });

        let deadline = Instant::now() + Duration::from_secs(15);
        let seen = |needle: &str| String::from_utf8_lossy(&output.lock().unwrap()).contains(needle);
        while !seen(wait_for) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(300));
        writer.lock().unwrap().write_all(keys.as_bytes()).unwrap();

        while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        drop(pair.master);
        let _ = reader_thread.join();
        String::from_utf8_lossy(&output.lock().unwrap()).into_owned()
    }
}

#[test]
fn track_and_status_round_trip() {
    let sb = Sandbox::new();
    sb.track();
    let config = fs::read_to_string(sb.home.join(".config/flatdir/config.toml")).unwrap();
    assert!(config.contains(sb.root.to_str().unwrap()), "{config}");

    let out = sb.run(&["status"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && text.contains("directories: 2"),
        "{text}"
    );
}

#[test]
fn picking_a_directory_prints_its_path() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\r");
    assert!(
        text.contains(sb.root.join("alpha").to_str().unwrap()),
        "{text}"
    );
}

#[test]
fn zoxide_score_decides_the_initial_order() {
    let sb = Sandbox::new();
    sb.track();
    sb.fake_zoxide(&sb.root.join("beta"));
    let text = sb.run_in_pty(&[], &[], "beta", "\r");
    assert!(
        text.contains(sb.root.join("beta").to_str().unwrap()),
        "{text}"
    );
    assert!(
        !text.contains(sb.root.join("alpha").to_str().unwrap()),
        "{text}"
    );
}

#[test]
fn archive_then_restore_through_the_picker_keeps_data() {
    let sb = Sandbox::new();
    sb.track();
    fs::write(sb.root.join("alpha/keep.txt"), "x").unwrap();

    sb.run_in_pty(&["archive"], &[], "alpha", "\r");
    assert_eq!(sb.names(&sb.root), ["beta"]);
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha"]);
    assert!(sb.archive_dir().join("alpha/keep.txt").is_file());

    sb.run_in_pty(&["restore"], &[], "alpha", "\r");
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"]);
    assert!(sb.root.join("alpha/keep.txt").is_file());
    assert!(sb.names(&sb.archive_dir()).is_empty());
}

/// Neither the old FZF_DEFAULT_OPTS nor skim's own variable can change how the picker behaves.
#[test]
fn environment_filter_options_cannot_make_the_picker_non_interactive() {
    let sb = Sandbox::new();
    sb.track();
    let env = [
        ("FZF_DEFAULT_OPTS", "--filter="),
        ("SKIM_DEFAULT_OPTIONS", "--filter="),
    ];
    let text = sb.run_in_pty(&["archive"], &env, "alpha", "\x03");
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"], "{text}");
    assert!(sb.names(&sb.archive_dir()).is_empty());
}

/// Reproduces the 2026-10-18 incident: destructive commands run without a terminal, with `--filter=` in the environment.
#[test]
fn incident_regression_noninteractive_filter_env_changes_nothing() {
    let sb = Sandbox::new();
    sb.track();
    fs::create_dir_all(sb.archive_dir().join("old")).unwrap();

    for sub in ["remove", "archive", "restore"] {
        let out = Command::new(env!("CARGO_BIN_EXE_flatdir"))
            .arg(sub)
            .env_clear()
            .env("HOME", &sb.home)
            .env("PATH", sb.path_env())
            .env("FZF_DEFAULT_OPTS", "--filter=")
            .env("SKIM_DEFAULT_OPTIONS", "--filter=")
            .output()
            .unwrap();
        assert!(!out.status.success(), "{sub} must fail");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("interactive terminal"),
            "{sub}"
        );
    }
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"]);
    assert_eq!(sb.names(&sb.archive_dir()), ["old"]);
}

/// The built-in color scheme must stay on the terminal's own palette (0-15), so it works on light and dark backgrounds.
#[test]
fn picker_uses_only_palette_colors() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\x03");

    assert!(
        text.contains("\x1b[7m"),
        "expected reverse video for the current line: {text:?}"
    );

    let mut bad = Vec::new();
    for marker in ["38;5;", "48;5;", "58;5;"] {
        for (i, _) in text.match_indices(marker) {
            let n: String = text[i + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if n.parse::<u16>().map_or(true, |n| n > 15) {
                bad.push(format!("{marker}{n}"));
            }
        }
    }
    assert!(bad.is_empty(), "non-palette colors: {bad:?}");
    assert!(!text.contains(";2;"), "truecolor escape found");
}
