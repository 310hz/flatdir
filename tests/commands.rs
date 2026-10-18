//! Everything runs on a throwaway HOME. Real paths and the real trash are never touched.

mod common;

use std::fs;
use std::path::Path;

use common::{RowsSelector, Sandbox};
use flatdir::candidates;
use flatdir::cmd;
use flatdir::config::Config;
use flatdir::ops;
use flatdir::select::{SelectOpts, pick};
use flatdir::services::parse_zoxide;

const ROOT_ALL: [&str; 5] = [".hidden", "alpha", "beta", "file.txt", "gamma"];

#[test]
fn children_skip_hidden_dirs_and_files() {
    let sb = Sandbox::new();
    let names: Vec<String> = candidates::list_children(&sb.root)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["alpha", "beta", "gamma"]);
}

#[test]
fn archive_moves_only_targets_without_writing_a_marker() {
    let sb = Sandbox::new();
    fs::write(sb.root.join("alpha/keep.txt"), "x").unwrap();
    fs::create_dir_all(sb.root.join("alpha/node_modules")).unwrap();

    let moved = ops::archive(&sb.ctx(), &sb.entries(&["alpha"])).unwrap();

    assert_eq!(sb.names(&sb.root), [".hidden", "beta", "file.txt", "gamma"]);
    assert_eq!(sb.names(&sb.archive_dir()), ["alpha"]);
    assert_eq!(
        sb.names(&sb.archive_dir().join("alpha")),
        ["keep.txt", "node_modules"]
    );
    assert_eq!(
        moved[0].leftovers,
        ["node_modules"],
        "reported, but never deleted"
    );
}

#[test]
fn restore_returns_dir_and_removes_legacy_marker() {
    let sb = Sandbox::new();
    let legacy = sb.archive_dir().join("old");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join(".flatdir_archived"), "").unwrap();
    fs::write(legacy.join("data.txt"), "x").unwrap();

    ops::restore(&sb.ctx(), &sb.archived_entries(&["old"])).unwrap();

    assert_eq!(sb.names(&sb.root.join("old")), ["data.txt"]);
    assert!(sb.names(&sb.archive_dir()).is_empty());
}

#[test]
fn legacy_archives_are_listed_without_relying_on_markers() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.archive_dir().join("with-marker")).unwrap();
    fs::write(sb.archive_dir().join("with-marker/.flatdir_archived"), "").unwrap();
    fs::create_dir_all(sb.archive_dir().join("without-marker")).unwrap();
    fs::create_dir_all(sb.archive_dir().join(".hidden-archive")).unwrap();

    let names: Vec<String> = candidates::archived(&sb.ctx())
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["with-marker", "without-marker"]);
}

#[test]
fn remove_trashes_only_targets() {
    let sb = Sandbox::new();
    let done = ops::remove(&sb.ctx(), &sb.entries(&["beta"]), &sb.root).unwrap();
    assert_eq!(done, ["beta"]);
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "file.txt", "gamma"]
    );
    assert_eq!(sb.names(&sb.trash_dir), ["0-beta"]);
}

#[test]
fn operations_refuse_targets_outside_their_parent_and_change_nothing() {
    let sb = Sandbox::new();
    let ctx = sb.ctx();
    let outside = candidates::Entry {
        name: "x".into(),
        path: sb.home.join("x"),
    };
    fs::create_dir_all(&outside.path).unwrap();
    let nested = candidates::Entry {
        name: "n".into(),
        path: sb.root.join("alpha/n"),
    };
    fs::create_dir_all(&nested.path).unwrap();
    let hidden = candidates::Entry {
        name: ".hidden".into(),
        path: sb.root.join(".hidden"),
    };

    for bad in [&outside, &nested, &hidden] {
        let targets = [sb.entries(&["alpha"]).remove(0), bad.clone()];
        assert!(ops::archive(&ctx, &targets).is_err());
        assert!(ops::remove(&ctx, &targets, &sb.root).is_err());
    }
    assert_eq!(sb.names(&sb.root), ROOT_ALL);
    assert!(sb.names(&sb.trash_dir).is_empty());
    assert!(!sb.archive_dir().exists());
}

#[test]
fn archive_and_restore_refuse_existing_destinations_and_move_nothing() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.archive_dir().join("beta")).unwrap();
    let err = ops::archive(&sb.ctx(), &sb.entries(&["alpha", "beta"]))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("destination exists"), "{err}");
    assert_eq!(sb.names(&sb.root), ROOT_ALL);

    fs::create_dir_all(sb.archive_dir().join("alpha")).unwrap();
    let err = ops::restore(&sb.ctx(), &sb.archived_entries(&["alpha"]))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("destination exists"), "{err}");
}

#[test]
fn symlinks_are_not_followed_as_targets() {
    let sb = Sandbox::new();
    let outside = sb.home.join("precious");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, sb.root.join("link")).unwrap();

    let entry = candidates::Entry {
        name: "link".into(),
        path: sb.root.join("link"),
    };
    assert!(ops::remove(&sb.ctx(), std::slice::from_ref(&entry), &sb.root).is_err());
    assert!(ops::archive(&sb.ctx(), &[entry]).is_err());
    assert!(outside.is_dir());
    let listed: Vec<String> = candidates::list_children(&sb.root)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(!listed.contains(&"link".to_string()));
}

#[test]
fn unknown_row_from_the_selector_is_an_error() {
    let sb = Sandbox::new();
    let mut ctx = sb.ctx();
    ctx.selector = Box::new(RowsSelector(vec!["x\t/etc".to_string()]));
    let rows = vec![vec!["alpha".to_string(), "1".to_string()]];
    let opts = SelectOpts {
        multi: false,
        prompt: None,
        height_percent: 60,
        repo_preview: false,
    };
    let err = pick(&ctx, &rows, &opts).unwrap_err().to_string();
    assert!(err.contains("unknown row"), "{err}");
}

#[test]
fn track_sets_a_single_root() {
    let sb = Sandbox::new();
    let other = sb.home.join("other");
    fs::create_dir_all(&other).unwrap();

    let mut ctx = sb.ctx();
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
    let outside = sb.config_path.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let mut ctx = sb.ctx();
    ctx.config.root = None;
    assert!(cmd::track(&mut ctx, &outside).is_err());
    assert!(cmd::track(&mut ctx, &sb.home).is_err());
}

#[test]
fn init_creates_plain_dir_and_rejects_escaping_names() {
    let sb = Sandbox::new();
    let ctx = sb.ctx();
    for bad in ["../x", "a/b", ".hidden2", "", ".."] {
        assert!(cmd::init(&ctx, bad, false).is_err(), "{bad:?}");
    }
    assert!(cmd::init(&ctx, "alpha", false).is_err());

    fs::create_dir_all(sb.archive_dir().join("archived-name")).unwrap();
    assert!(cmd::init(&ctx, "archived-name", false).is_err());

    assert_eq!(
        cmd::init(&ctx, "fresh", false).unwrap(),
        sb.root.join("fresh")
    );
    assert!(sb.root.join("fresh").is_dir());
}

#[test]
fn config_ignores_unknown_legacy_keys_and_reads_color_and_sound() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.config_path.parent().unwrap()).unwrap();
    fs::write(
        &sb.config_path,
        "root = \"/x\"\nfzf_opts = [\"--filter=x\"]\ncolor = \"light\"\nsound = true\n",
    )
    .unwrap();
    let config = Config::load(&sb.config_path).unwrap();
    assert_eq!(config.root.as_deref(), Some(Path::new("/x")));
    assert_eq!(config.color.as_deref(), Some("light"));
    assert!(config.sound);
    assert!(!Config::default().sound, "sound is opt-in");
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
        fs::create_dir_all(sb.archive_dir().join(d)).unwrap();
    }
    let status = cmd::status(&sb.ctx());
    let root = status.root.as_ref().unwrap();
    assert_eq!(
        (root.path.as_path(), root.directories, root.archived),
        (sb.root.as_path(), Some(3), 2)
    );
    assert_eq!(root.archive_dir, sb.archive_dir());
    assert!(status.tools.iter().any(|t| t.name == "zoxide"));
    let text = status.to_string();
    assert!(
        text.contains("directories: 3") && text.contains("archived:    2"),
        "{text}"
    );

    let mut missing = sb.ctx();
    missing.config.root = Some(sb.home.join("nowhere"));
    let status = cmd::status(&missing);
    assert_eq!(status.root.as_ref().unwrap().directories, None);
    assert!(status.to_string().contains("not a readable directory"));

    let mut unset = sb.ctx();
    unset.config.root = None;
    assert!(cmd::status(&unset).to_string().contains("not set"));
}
