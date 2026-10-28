//! Everything runs on a throwaway HOME. Real paths and the real trash are never touched.

mod common;

use std::fs;
use std::path::Path;

use common::Sandbox;
use flatdir::archive::{Record, State};
use flatdir::candidates;
use flatdir::cmd;
use flatdir::config::Config;
use flatdir::create::{self, Hub, Job, Plan, Recipe};
use flatdir::ops;
use flatdir::services::{parse_github_hits, parse_hf_hits, parse_zoxide};

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
fn archive_only_records_targets_and_leaves_them_in_place() {
    let sb = Sandbox::new();
    fs::write(sb.root.join("alpha/keep.txt"), "x").unwrap();
    fs::create_dir_all(sb.root.join("alpha/node_modules")).unwrap();

    assert_eq!(
        ops::archive(&sb.ctx(), &sb.entries(&["alpha"])).unwrap(),
        ["alpha"]
    );

    assert_eq!(sb.names(&sb.root), ROOT_ALL, "nothing moves");
    assert_eq!(
        sb.names(&sb.root.join("alpha")),
        ["keep.txt", "node_modules"],
        "nothing inside is cleaned up"
    );
    assert_eq!(sb.archived(), ["alpha"]);
}

#[test]
fn restore_only_drops_the_record() {
    let sb = Sandbox::new();
    sb.archive(&["alpha", "beta"]);

    assert_eq!(
        ops::restore(&sb.ctx(), &sb.entries(&["alpha"])).unwrap(),
        ["alpha"]
    );

    assert_eq!(sb.archived(), ["beta"]);
    assert_eq!(sb.names(&sb.root), ROOT_ALL);
}

#[test]
fn lists_split_the_root_by_the_state_and_report_vanished_archives() {
    let sb = Sandbox::new();
    sb.archive(&["beta", "gone"]);
    fs::remove_dir(sb.root.join("gone")).unwrap();
    let elsewhere = sb.home.join("other/beta");
    let mut state = State::load(&sb.state_path).unwrap();
    state
        .archived
        .insert(elsewhere.clone(), Record { archived_at: 1 });
    state.save(&sb.state_path).unwrap();

    let lists = candidates::lists(&sb.ctx()).unwrap();
    let names = |v: &[candidates::Entry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&lists.active), ["alpha", "gamma"]);
    assert_eq!(names(&lists.archived), ["beta"]);
    assert_eq!(
        lists.missing,
        [sb.root.join("gone")],
        "another root's record is not this root's business"
    );
    assert_eq!(sb.archived().len(), 3, "listing never writes");
}

#[test]
fn remove_trashes_only_targets() {
    let sb = Sandbox::new();
    sb.archive(&["beta", "gamma"]);
    let done = ops::remove(&sb.ctx(), &sb.entries(&["beta"])).unwrap();
    assert_eq!(done, ["beta"]);
    assert_eq!(
        sb.names(&sb.root),
        [".hidden", "alpha", "file.txt", "gamma"]
    );
    assert_eq!(sb.names(&sb.trash_dir), ["0-beta"]);
    assert_eq!(sb.archived(), ["gamma"], "a trashed archive is forgotten");
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
        assert!(ops::remove(&ctx, &targets).is_err());
    }
    assert_eq!(sb.names(&sb.root), ROOT_ALL);
    assert!(sb.names(&sb.trash_dir).is_empty());
    assert!(!sb.state_path.exists());
}

#[test]
fn archive_and_restore_refuse_targets_in_the_wrong_state_and_change_nothing() {
    let sb = Sandbox::new();
    sb.archive(&["beta"]);
    let err = ops::archive(&sb.ctx(), &sb.entries(&["alpha", "beta"]))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("already archived"), "{err}");
    assert_eq!(sb.archived(), ["beta"]);

    let err = ops::restore(&sb.ctx(), &sb.entries(&["beta", "alpha"]))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("not archived"), "{err}");
    assert_eq!(sb.archived(), ["beta"]);
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
    assert!(ops::remove(&sb.ctx(), std::slice::from_ref(&entry)).is_err());
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
fn the_root_can_be_set_replaced_and_unset() {
    let sb = Sandbox::new();
    let other = sb.home.join("other");
    fs::create_dir_all(&other).unwrap();

    let mut ctx = sb.ctx();
    ctx.config.root = None;

    let change = cmd::set_root(&mut ctx, &sb.root).unwrap();
    assert_eq!(
        (change.root.as_path(), change.previous, change.changed),
        (sb.root.as_path(), None, true)
    );
    assert_eq!(
        Config::load(&sb.config_path).unwrap().root.as_deref(),
        Some(sb.root.as_path())
    );

    let again = cmd::set_root(&mut ctx, &sb.root).unwrap();
    assert!(!again.changed, "setting the same root is a no-op");

    // A different root simply replaces the old one; no separate "unset" step is needed.
    let replaced = cmd::set_root(&mut ctx, &other).unwrap();
    assert_eq!(
        (replaced.root.as_path(), replaced.previous.as_deref()),
        (other.as_path(), Some(sb.root.as_path()))
    );
    assert_eq!(
        Config::load(&sb.config_path).unwrap().root.as_deref(),
        Some(other.as_path())
    );

    assert_eq!(cmd::unset_root(&mut ctx).unwrap(), other);
    assert_eq!(Config::load(&sb.config_path).unwrap().root, None);
    assert!(cmd::unset_root(&mut ctx).is_err(), "nothing left to unset");
}

#[test]
fn archives_of_a_previous_root_come_back_when_it_is_set_again() {
    let sb = Sandbox::new();
    let other = sb.home.join("other");
    fs::create_dir_all(other.join("old")).unwrap();
    sb.archive(&["old"]);

    let mut ctx = sb.ctx();
    cmd::set_root(&mut ctx, &other).unwrap();
    let lists = candidates::lists(&ctx).unwrap();
    assert!(
        lists.archived.is_empty() && lists.missing.is_empty(),
        "the same name under the other root is not archived"
    );
    assert_eq!(lists.active[0].name, "old");
    cmd::set_root(&mut ctx, &sb.root).unwrap();
    let names: Vec<String> = candidates::lists(&ctx)
        .unwrap()
        .archived
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["old"]);
}

#[test]
fn the_root_must_be_an_existing_directory_under_home() {
    let sb = Sandbox::new();
    let outside = sb.config_path.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let mut ctx = sb.ctx();
    ctx.config.root = None;
    assert!(cmd::set_root(&mut ctx, &outside).is_err());
    assert!(cmd::set_root(&mut ctx, &sb.home).is_err());
    assert!(cmd::set_root(&mut ctx, &sb.home.join("missing")).is_err());
    assert_eq!(ctx.config.root, None);
}

#[test]
fn new_names_must_be_plain_and_free() {
    let sb = Sandbox::new();
    let ctx = sb.ctx();
    for bad in ["../x", "a/b", ".hidden2", "", "..", "a\tb"] {
        assert!(
            create::plan(&ctx, Recipe::Directory, bad, false).is_err(),
            "{bad:?}"
        );
    }
    assert!(create::plan(&ctx, Recipe::Directory, "alpha", false).is_err());
    assert!(create::plan(&ctx, Recipe::GitHubRepo, "alpha", false).is_err());

    sb.archive(&["archived-name"]);
    assert!(create::plan(&ctx, Recipe::Directory, "archived-name", false).is_err());

    assert_eq!(
        create::plan(&ctx, Recipe::Directory, " fresh ", false).unwrap(),
        Plan::MakeDir(sb.root.join("fresh"))
    );
}

#[test]
fn clone_targets_are_urls_owner_name_or_search_words() {
    let sb = Sandbox::new();
    let ctx = sb.ctx();
    let plan = |recipe, input: &str, lfs| create::plan(&ctx, recipe, input, lfs);
    let clone = |url: &str, name: &str, skip_lfs| {
        Plan::Run(Job::Clone {
            url: url.to_string(),
            dest: sb.root.join(name),
            skip_lfs,
        })
    };

    assert_eq!(
        plan(Recipe::Clone(Hub::GitHub), "owner/repo.git", false).unwrap(),
        clone("git@github.com:owner/repo.git", "repo", false)
    );
    assert_eq!(
        plan(Recipe::Clone(Hub::HfModel), "openai/whisper", false).unwrap(),
        clone("https://huggingface.co/openai/whisper", "whisper", true)
    );
    assert_eq!(
        plan(Recipe::Clone(Hub::HfDataset), "rajpurkar/squad", true).unwrap(),
        clone(
            "https://huggingface.co/datasets/rajpurkar/squad",
            "squad",
            false
        )
    );
    assert_eq!(
        plan(Recipe::Clone(Hub::HfSpace), "a/b", false).unwrap(),
        clone("https://huggingface.co/spaces/a/b", "b", true)
    );
    assert_eq!(
        plan(
            Recipe::Clone(Hub::GitHub),
            "https://example.com/x/tool.git",
            false
        )
        .unwrap(),
        clone("https://example.com/x/tool.git", "tool", false)
    );
    assert_eq!(
        plan(Recipe::Clone(Hub::HfModel), "whisper", false).unwrap(),
        Plan::Run(Job::Search {
            hub: Hub::HfModel,
            query: "whisper".to_string()
        })
    );

    // Nothing is ever overwritten, and odd ids are refused rather than guessed.
    assert!(plan(Recipe::Clone(Hub::GitHub), "someone/alpha", false).is_err());
    for bad in ["a/b/c", "/b", "a/", "a/..", "a /b"] {
        assert!(
            plan(Recipe::Clone(Hub::GitHub), bad, false).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn search_results_are_parsed_from_gh_and_the_hugging_face_api() {
    let gh = parse_github_hits(
        r#"[{"fullName":"o/r","stargazersCount":12,"description":"d"},{"fullName":"o/s","stargazersCount":0,"description":null}]"#,
    )
    .unwrap();
    assert_eq!(gh[0].id, "o/r");
    assert_eq!(gh[0].meta, "12 stars");
    assert_eq!(gh[1].description, "");

    let hf = parse_hf_hits(r#"[{"id":"openai/whisper","likes":5,"downloads":9}]"#).unwrap();
    assert_eq!(hf[0].id, "openai/whisper");
    assert_eq!(hf[0].meta, "5 likes");
}

#[test]
fn config_ignores_unknown_legacy_keys_and_reads_sound() {
    let sb = Sandbox::new();
    fs::create_dir_all(sb.config_path.parent().unwrap()).unwrap();
    fs::write(
        &sb.config_path,
        "root = \"/x\"\nfzf_opts = [\"--filter=x\"]\ncolor = \"light\"\nsound = true\n",
    )
    .unwrap();
    let config = Config::load(&sb.config_path).unwrap();
    assert_eq!(config.root.as_deref(), Some(Path::new("/x")));
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
    sb.archive(&["a1", "a2"]);
    let status = cmd::status(&sb.ctx());
    let root = status.root.as_ref().unwrap();
    assert_eq!(
        (root.path.as_path(), root.directories, root.archived),
        (sb.root.as_path(), Some(5), 2)
    );
    assert_eq!(status.state_path, sb.state_path);
    assert!(status.tools.iter().any(|t| t.name == "zoxide"));
    let text = status.to_string();
    assert!(
        text.contains("directories: 5") && text.contains("archived:    2"),
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
