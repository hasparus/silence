mod common;

use common::Repo;

#[test]
fn staged_mode_skips_a_file_that_also_has_unstaged_edits() {
    let repo = Repo::new("staged-skips-dirty");
    repo.commit_baseline("a.rs", "fn a() {}\n");

    repo.write("a.rs", "fn a() {}\nlet staged = 1; // staged comment\n");
    repo.git(&["add", "a.rs"]);
    repo.write(
        "a.rs",
        "fn a() {}\nlet staged = 1; // staged comment\nlet working = 2; // working comment\n",
    );

    let out = repo.silence(&["strip", "--staged"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success());
    assert_eq!(
        repo.read("a.rs"),
        "fn a() {}\nlet staged = 1; // staged comment\nlet working = 2; // working comment\n",
    );
    assert!(
        stderr.contains("a.rs") && stderr.contains("unstaged"),
        "expected a skip warning mentioning unstaged changes, got: {stderr}"
    );
}

#[test]
fn staged_mode_strips_a_cleanly_staged_file() {
    let repo = Repo::new("staged-clean");
    repo.commit_baseline("b.rs", "fn b() {}\n");

    repo.write("b.rs", "fn b() {}\nlet x = 1; // remove me\n");
    repo.git(&["add", "b.rs"]);

    let out = repo.silence(&["strip", "--staged"]);

    assert!(out.status.success());
    assert_eq!(repo.read("b.rs"), "fn b() {}\nlet x = 1;\n");
    let cached =
        String::from_utf8_lossy(&repo.git_output(&["diff", "--cached", "--", "b.rs"]).stdout)
            .into_owned();
    assert!(cached.contains("+let x = 1;"));
    assert!(!cached.contains("remove me"));
    assert!(
        repo.git_output(&["diff", "--", "b.rs"]).stdout.is_empty(),
        "--staged should restage stripped worktree changes"
    );
}

#[test]
fn changes_mode_widens_a_file_that_is_staged_and_modified() {
    let repo = Repo::new("changes-conflicted");
    repo.commit_baseline("c.rs", "fn c() {}\n// committed comment, must survive\n");

    repo.write(
        "c.rs",
        "fn c() {}\n// committed comment, must survive\nlet s = 1; // staged\n",
    );
    repo.git(&["add", "c.rs"]);
    repo.write(
        "c.rs",
        "fn c() {}\n// committed comment, must survive\nlet s = 1; // staged\nlet w = 2; // working\n",
    );

    let out = repo.silence(&["strip", "--changes"]);

    assert!(out.status.success());
    assert_eq!(
        repo.read("c.rs"),
        "fn c() {}\n// committed comment, must survive\nlet s = 1;\nlet w = 2;\n"
    );
}
