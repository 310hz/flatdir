//! Runs the real binary inside a pseudo-terminal with a throwaway HOME and a rebuilt environment.
//! Never send Ctrl+R (0x12, move to trash) here: the real trash lives outside the sandbox.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use flatdir::archive::{Record, State};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tempfile::TempDir;

/// macOS vets a freshly written executable the first time it runs, which can take hundreds of
/// milliseconds when many tests do it at once. Run it once here so the timing-sensitive part of a
/// test never pays for that.
fn warm_up(path: &Path) {
    let _ = Command::new(path)
        .stdin(std::process::Stdio::null())
        .output();
}

/// Row (1-based) of the first list entry: the title, a blank row and the query line come first.
const FIRST_LIST_ROW: u16 = 4;

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
        warm_up(&path);
    }

    /// Fake `open` / `xdg-open` that only log their argument, so no real file manager is launched.
    fn fake_open(&self) -> PathBuf {
        let log = self.tmp.join("open.log");
        let body = format!("#!/bin/sh\necho \"$1\" >> '{}'\n", log.display());
        for name in ["open", "xdg-open"] {
            let path = self.tmp.join("bin").join(name);
            fs::write(&path, &body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            warm_up(&path);
        }
        let _ = fs::remove_file(&log);
        log
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

    /// Puts the binary on PATH under its real name, the way an installed copy would be found.
    fn link_binary(&self) {
        let link = self.tmp.join("bin/flatdir");
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_flatdir"), link).unwrap();
    }

    fn track(&self) {
        let out = self.run(&["root", self.root.to_str().unwrap()]);
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

    fn state_path(&self) -> PathBuf {
        self.home.join(".local/state/flatdir/state.json")
    }

    /// Names recorded as archived, sorted.
    fn archived(&self) -> Vec<String> {
        State::load(&self.state_path())
            .unwrap()
            .archived
            .keys()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn archive(&self, name: &str) {
        let path = self.root.join(name);
        fs::create_dir_all(&path).unwrap();
        let mut state = State::load(&self.state_path()).unwrap();
        state.archived.insert(path, Record { archived_at: 1 });
        state.save(&self.state_path()).unwrap();
    }

    /// Drives the binary in a pty: waits for `wait_for` to appear, sends `keys`, returns all output.
    fn run_in_pty(
        &self,
        args: &[&str],
        extra_env: &[(&str, &str)],
        wait_for: &str,
        keys: &str,
    ) -> String {
        self.run_program_in_pty(
            env!("CARGO_BIN_EXE_flatdir"),
            args,
            extra_env,
            wait_for,
            keys,
        )
    }

    /// Same, for any program (used to run a real shell that has the flatdir function loaded).
    fn run_program_in_pty(
        &self,
        program: &str,
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
        let mut cmd = CommandBuilder::new(program);
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
        // Late work (zoxide scores arrive on a thread) redraws the screen; wait until the output has
        // been quiet for a moment instead of guessing a fixed delay, which is racy on a busy machine.
        let (mut last_len, mut quiet_since) = (0, Instant::now());
        while quiet_since.elapsed() < Duration::from_millis(250) && Instant::now() < deadline {
            let len = output.lock().unwrap().len();
            if len != last_len {
                (last_len, quiet_since) = (len, Instant::now());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // `|` in `keys` means: pause so the screen can react before the next key.
        for (i, chunk) in keys.split('|').enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(350));
            }
            writer.lock().unwrap().write_all(chunk.as_bytes()).unwrap();
        }

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
fn root_status_round_trip() {
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
fn enter_prints_the_path_of_the_current_directory() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\r");
    assert!(
        text.contains(sb.root.join("alpha").to_str().unwrap()),
        "{text}"
    );
    assert!(text.contains("Help"), "the title row points to the help");
}

#[test]
fn zoxide_score_decides_the_order_after_the_first_frame() {
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
fn ctrl_a_archives_and_ctrl_t_then_ctrl_a_restores() {
    let sb = Sandbox::new();
    sb.track();
    fs::write(sb.root.join("alpha/keep.txt"), "x").unwrap();

    sb.run_in_pty(&[], &[], "alpha", "\x01|\x03");
    assert_eq!(sb.archived(), ["alpha"]);
    assert!(
        sb.root.join("alpha/keep.txt").is_file(),
        "it stays in place"
    );

    sb.run_in_pty(&[], &[], "beta", "\x14|\x01|\x03");
    assert!(sb.archived().is_empty());
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"]);
}

#[test]
fn tab_marks_several_rows_for_one_action() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.root.join("gamma")).unwrap();
    sb.track();
    sb.run_in_pty(&[], &[], "alpha", "\t|\x1b[B|\t|\x01|\x03");
    assert_eq!(sb.archived(), ["alpha", "beta"]);
}

#[test]
fn typed_text_filters_and_a_paste_cannot_trigger_shortcuts() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\x1b[200~\x01\x01\x1b[201~|\x03");
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"], "{text}");
    assert!(sb.archived().is_empty());
}

/// Neither the old FZF_DEFAULT_OPTS nor skim's own variable can change how the program behaves.
#[test]
fn environment_filter_options_change_nothing() {
    let sb = Sandbox::new();
    sb.track();
    let env = [
        ("FZF_DEFAULT_OPTS", "--filter="),
        ("SKIM_DEFAULT_OPTIONS", "--filter="),
    ];
    let text = sb.run_in_pty(&[], &env, "alpha", "\x03");
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"], "{text}");
    assert!(text.contains("Help"), "the TUI still came up interactively");
}

/// Reproduces the 2026-10-18 incident: no terminal, `--filter=` in the environment.
/// The old destructive subcommands are gone, and the TUI refuses to start without a terminal.
#[test]
fn incident_regression_without_a_terminal_nothing_runs_and_nothing_changes() {
    let sb = Sandbox::new();
    sb.track();
    sb.archive("old");

    for args in [&[][..], &["remove"], &["archive"], &["restore"], &["-a"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_flatdir"))
            .args(args)
            .env_clear()
            .env("HOME", &sb.home)
            .env("PATH", sb.path_env())
            .env("FZF_DEFAULT_OPTS", "--filter=")
            .env("SKIM_DEFAULT_OPTIONS", "--filter=")
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?} must fail");
    }
    let out = sb.run(&[]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("interactive terminal"));
    assert_eq!(sb.names(&sb.root), ["alpha", "beta", "old"]);
    assert_eq!(sb.archived(), ["old"]);
}

/// The screen must stay on the terminal's own palette (0-15), so it works on light and dark backgrounds.
#[test]
fn the_screen_uses_only_palette_colors() {
    let sb = Sandbox::new();
    sb.track();
    fs::write(
        sb.root.join("alpha/README.md"),
        "# Title\n\n`code` **bold**\n",
    )
    .unwrap();
    let text = sb.run_in_pty(&[], &[], "alpha", "\t|\x1b[B|\x03");
    assert!(
        text.contains("\x1b[7m"),
        "expected reverse video for the current row"
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
    for marker in ["38;2;", "48;2;", "58;2;"] {
        assert!(!text.contains(marker), "truecolor escape found: {marker}");
    }
}

/// Autowrap is off while the TUI is up (so an overflowing row cannot spill into the next row's left edge)
/// and is switched back on when it exits.
#[test]
fn autowrap_is_disabled_only_while_the_tui_runs() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\x03");
    let off = text.find("\x1b[?7l").expect("autowrap off");
    let on = text.rfind("\x1b[?7h").expect("autowrap restored");
    assert!(off < on, "restored after being disabled");
}

#[test]
fn ctrl_o_runs_the_open_command_with_the_current_directory() {
    let sb = Sandbox::new();
    sb.track();
    let log = sb.fake_open();
    sb.run_in_pty(&[], &[], "alpha", "\x0f|\x03");
    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(logged.trim(), sb.root.join("alpha").to_str().unwrap());
    assert_eq!(sb.names(&sb.root), ["alpha", "beta"]);
}

/// SGR mouse wheel report: button 65 = wheel down, 64 = wheel up. Coordinates are 1-based.
#[test]
fn the_mouse_wheel_moves_the_cursor_over_the_list_but_not_over_the_preview() {
    let sb = Sandbox::new();
    sb.track();

    let over_list = sb.run_in_pty(
        &[],
        &[],
        "alpha",
        &format!("\x1b[<65;5;{FIRST_LIST_ROW}M|\r"),
    );
    assert!(
        over_list.contains(sb.root.join("beta").to_str().unwrap()),
        "{over_list}"
    );

    let over_preview = sb.run_in_pty(
        &[],
        &[],
        "alpha",
        &format!("\x1b[<65;80;{FIRST_LIST_ROW}M|\r"),
    );
    assert!(
        over_preview.contains(sb.root.join("alpha").to_str().unwrap()),
        "{over_preview}"
    );
}

/// Mouse reporting is switched on only while the TUI runs, so the shell gets normal text selection back.
#[test]
fn mouse_reporting_is_enabled_only_while_the_tui_runs() {
    let sb = Sandbox::new();
    sb.track();
    let text = sb.run_in_pty(&[], &[], "alpha", "\x03");
    let on = text.find("\x1b[?1000h").expect("mouse reporting on");
    let off = text.rfind("\x1b[?1000l").expect("mouse reporting restored");
    assert!(on < off);
}

/// SGR mouse reports: button 0 = left, 2 = right; `M` = press, `m` = release; coordinates are 1-based.
#[test]
fn clicking_a_row_moves_the_cursor_and_right_clicking_marks_it() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.root.join("gamma")).unwrap();
    sb.track();

    let row = FIRST_LIST_ROW + 1;
    let left_click_second_row = format!("\x1b[<0;5;{row}M\x1b[<0;5;{row}m|\r");
    let text = sb.run_in_pty(&[], &[], "alpha", &left_click_second_row);
    assert!(
        text.contains(sb.root.join("beta").to_str().unwrap()),
        "{text}"
    );

    // Right click marks the second row; Ctrl+A then archives exactly the marked entry.
    sb.run_in_pty(
        &[],
        &[],
        "alpha",
        &format!("\x1b[<2;5;{row}M\x1b[<2;5;{row}m|\x01|\x03"),
    );
    assert_eq!(sb.archived(), ["beta"]);
}

#[test]
fn ctrl_q_resets_the_marks_before_an_action() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.root.join("gamma")).unwrap();
    sb.track();
    // Mark alpha and beta (the cursor ends on gamma), reset with Ctrl+Q, then archive: only gamma moves.
    sb.run_in_pty(&[], &[], "alpha", "\t|\x1b[B|\t|\x1b[B|\x11|\x01|\x03");
    assert_eq!(sb.archived(), ["gamma"]);
}

#[test]
fn the_root_command_shows_sets_replaces_and_unsets() {
    let sb = Sandbox::new();
    let other = sb.home.join("other");
    fs::create_dir_all(&other).unwrap();

    let out = sb.run(&["root"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("flatdir root <path>"));

    sb.track();
    assert_eq!(stdout(&sb.run(&["root"])), sb.root.to_str().unwrap());

    // Setting another one replaces it without any extra step.
    let out = sb.run(&["root", other.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("was"),
        "mentions the replaced root"
    );
    assert_eq!(stdout(&sb.run(&["root"])), other.to_str().unwrap());

    assert!(sb.run(&["root", "--unset"]).status.success());
    assert!(!sb.run(&["root"]).status.success());
    assert!(
        !sb.run(&["root", "--unset"]).status.success(),
        "nothing to unset"
    );
    assert!(
        !sb.run(&["root", other.to_str().unwrap(), "--unset"])
            .status
            .success(),
        "path and --unset conflict"
    );
}

#[test]
fn the_old_track_commands_are_gone() {
    let sb = Sandbox::new();
    for args in [&["track", "/tmp"][..], &["untrack"]] {
        assert!(
            !sb.run(args).status.success(),
            "{args:?} must not exist any more"
        );
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The whole point of the shell function: pressing Enter must change the shell's directory.
/// Runs a real shell in a pty, loads the function from `flatdir shell`, and checks `pwd` afterwards.
fn assert_enter_changes_directory_in(shell: &str) {
    if !Path::new(shell).exists() {
        eprintln!("skipping: {shell} is not installed");
        return;
    }
    let sb = Sandbox::new();
    sb.track();
    sb.link_binary();
    let script = r#"eval "$(flatdir shell)"; flatdir; echo "PWD=$(pwd)""#;
    let text = sb.run_program_in_pty(shell, &["-c", script], &[], "alpha", "\r");
    let alpha = sb.root.join("alpha");
    assert!(
        text.contains(&format!("PWD={}", alpha.display())),
        "{shell}: the shell did not cd into {}:\n{text}",
        alpha.display()
    );
}

#[test]
fn enter_changes_the_directory_in_zsh() {
    assert_enter_changes_directory_in("/bin/zsh");
}

#[test]
fn enter_changes_the_directory_in_bash() {
    assert_enter_changes_directory_in("/bin/bash");
}

/// `root --cd` goes to the root through the same function, while plain `root` only prints it.
fn assert_root_cd_changes_directory_in(shell: &str) {
    if !Path::new(shell).exists() {
        eprintln!("skipping: {shell} is not installed");
        return;
    }
    let sb = Sandbox::new();
    sb.track();
    sb.link_binary();
    let script = r#"eval "$(flatdir shell)"; cd /; flatdir root; echo "AFTER_ROOT=$(pwd)"; flatdir root --cd; echo "PWD=$(pwd)""#;
    let text = sb.run_program_in_pty(shell, &["-c", script], &[], "PWD=", "");
    assert!(
        text.contains("AFTER_ROOT=/\r"),
        "{shell}: plain root moved:\n{text}"
    );
    assert!(
        text.contains(&format!("PWD={}", sb.root.display())),
        "{shell}: the shell did not cd into the root:\n{text}"
    );
}

#[test]
fn root_cd_changes_the_directory_in_zsh() {
    assert_root_cd_changes_directory_in("/bin/zsh");
}

#[test]
fn root_cd_changes_the_directory_in_bash() {
    assert_root_cd_changes_directory_in("/bin/bash");
}

#[test]
fn root_cd_without_the_function_prints_the_root_and_a_hint() {
    let sb = Sandbox::new();
    sb.track();
    let captured = sb.run(&["root", "--cd"]);
    assert!(captured.status.success());
    assert_eq!(stdout(&captured), sb.root.to_str().unwrap());
    assert!(
        captured.stderr.is_empty(),
        "no hint when the output is captured"
    );

    let shown = sb.run_in_pty(&["root", "--cd"], &[], "eval", "");
    assert!(shown.contains("flatdir shell"), "{shown}");

    let conflict = sb.run(&["root", "--cd", "--unset"]);
    assert!(!conflict.status.success());
}

#[test]
fn the_shell_function_is_printed_and_the_readme_loads_it() {
    let sb = Sandbox::new();
    let out = sb.run(&["shell"]);
    assert!(out.status.success());
    let printed = String::from_utf8_lossy(&out.stdout);
    assert_eq!(printed, flatdir::shell::SHELL_FUNCTION);
    assert!(
        printed.contains("command flatdir"),
        "must call the binary, not itself"
    );

    // Naming the shell is accepted (as with `zoxide init zsh`) and gives the same function.
    for shell in ["zsh", "bash"] {
        assert_eq!(
            String::from_utf8_lossy(&sb.run(&["shell", shell]).stdout),
            printed,
            "{shell}"
        );
    }
    let help = String::from_utf8_lossy(&sb.run(&["--help"]).stdout).into_owned();
    assert!(
        !help.contains("shell"),
        "a one-time setup command stays out of --help:\n{help}"
    );

    let fish = sb.run(&["shell", "fish"]);
    assert!(
        !fish.status.success() && fish.stdout.is_empty(),
        "unsupported shells are an error, not a guess"
    );

    let readme = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    assert!(
        readme.contains(r#"eval "$(flatdir shell)""#),
        "README tells how to load the function"
    );
}

/// Without the shell function the path is only printed; say so, but only when it would be seen.
#[test]
fn a_hint_explains_why_nothing_changed_unless_the_output_is_captured() {
    let sb = Sandbox::new();
    sb.track();

    let shown = sb.run_in_pty(&[], &[], "alpha", "\r");
    assert!(
        shown.contains("flatdir shell"),
        "stdout is the terminal: explain how to enable cd\n{shown}"
    );

    let file = sb.tmp.join("captured.txt");
    let script = format!("'{}' > '{}'", env!("CARGO_BIN_EXE_flatdir"), file.display());
    let captured = sb.run_program_in_pty("/bin/sh", &["-c", &script], &[], "alpha", "\r");
    assert!(
        !captured.contains("flatdir shell"),
        "captured like the shell function does: no hint\n{captured}"
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap().trim(),
        sb.root.join("alpha").to_str().unwrap()
    );
}

/// Creating and cloning moved into the TUI (Ctrl+N); the old subcommands are gone, not aliased.
#[test]
fn init_and_clone_are_no_longer_subcommands() {
    let sb = Sandbox::new();
    sb.track();
    for args in [&["init", "x"][..], &["clone", "owner/repo"]] {
        let out = sb.run(args);
        assert!(!out.status.success(), "{args:?}");
    }
    assert!(!sb.root.join("x").exists() && !sb.root.join("repo").exists());
}
