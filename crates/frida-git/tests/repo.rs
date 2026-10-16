//! M7 acceptance (PLAN.md §15): a temporary repository with a local bare remote. Covers
//! status, stage, commit, push, fast-forward pull, a refused non-fast-forward pull and a
//! blocked branch switch.

use std::path::{Path, PathBuf};
use std::process::Command;

use frida_git::{GitError, Repo};

/// Raw git for test setup (the crate itself only runs its allowed commands).
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn configure(dir: &Path) {
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

/// A bare remote plus two clones of it.
struct Setup {
    _root: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
}

fn setup() -> Setup {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    std::fs::create_dir(&remote).unwrap();
    git(&remote, &["init", "--bare", "--initial-branch=main"]);

    let a = root.path().join("a");
    std::fs::create_dir(&a).unwrap();
    git(&a, &["init", "--initial-branch=main"]);
    configure(&a);
    git(&a, &["remote", "add", "origin", remote.to_str().unwrap()]);
    std::fs::write(a.join("collection.yaml"), "name: Test\n").unwrap();
    git(&a, &["add", "."]);
    git(&a, &["commit", "-m", "initial"]);
    git(&a, &["push", "-u", "origin", "main"]);

    let b = root.path().join("b");
    git(
        root.path(),
        &["clone", "--quiet", remote.to_str().unwrap(), "b"],
    );
    configure(&b);
    Setup { _root: root, a, b }
}

#[test]
fn status_stage_commit_and_push() {
    let s = setup();
    let repo = Repo::open(&s.a).unwrap();
    std::fs::write(s.a.join("collection.yaml"), "name: Changed\n").unwrap();
    std::fs::write(s.a.join("new request.yaml"), "name: New\n").unwrap();

    let status = repo.status().unwrap();
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    let changes: Vec<(&str, Option<char>, Option<char>, bool)> = status
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.staged, f.unstaged, f.untracked))
        .collect();
    assert_eq!(
        changes,
        vec![
            ("collection.yaml", None, Some('M'), false),
            ("new request.yaml", None, Some('?'), true),
        ]
    );
    let changed = repo.file_versions("collection.yaml", false).unwrap();
    assert_eq!(changed.new.as_deref(), Some("name: Changed\n"));
    let new = repo.file_versions("new request.yaml", false).unwrap();
    assert_eq!((new.old, new.new.as_deref()), (None, Some("name: New\n")));

    repo.stage(&["new request.yaml".into()]).unwrap();
    repo.stage_all().unwrap();
    repo.unstage(&["collection.yaml".into()]).unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.files[0].staged, None);
    assert_eq!(status.files[1].staged, Some('A'));
    let staged_new = repo.file_versions("new request.yaml", true).unwrap();
    assert_eq!(
        (staged_new.old, staged_new.new.as_deref()),
        (None, Some("name: New\n"))
    );

    assert!(matches!(repo.commit("  "), Err(GitError::Blocked(_))));
    repo.stage_all().unwrap();
    repo.commit("Change the collection").unwrap();
    let status = repo.status().unwrap();
    assert!(status.files.is_empty());
    assert_eq!(status.ahead, 1);

    repo.push().unwrap();
    assert_eq!(repo.status().unwrap().ahead, 0);
}

#[test]
fn pull_fast_forwards_and_refuses_diverged_branches() {
    let s = setup();
    let a = Repo::open(&s.a).unwrap();
    let b = Repo::open(&s.b).unwrap();

    // a pushes, b fast-forwards.
    std::fs::write(s.a.join("one.yaml"), "1\n").unwrap();
    a.stage_all().unwrap();
    a.commit("one").unwrap();
    a.push().unwrap();
    b.fetch().unwrap();
    assert_eq!(b.status().unwrap().behind, 1);
    b.pull().unwrap();
    assert!(s.b.join("one.yaml").exists());

    // Both commit: b's pull can't fast-forward and is refused with git's explanation.
    std::fs::write(s.a.join("two.yaml"), "2\n").unwrap();
    a.stage_all().unwrap();
    a.commit("two").unwrap();
    a.push().unwrap();
    std::fs::write(s.b.join("three.yaml"), "3\n").unwrap();
    b.stage_all().unwrap();
    b.commit("three").unwrap();
    match b.pull() {
        Err(GitError::Command { message, .. }) => {
            assert!(
                message.contains("Not possible to fast-forward") || message.contains("diverg"),
                "{message}"
            );
        }
        other => panic!("expected a refused pull, got {other:?}"),
    }
    // Nothing was lost.
    assert!(s.b.join("three.yaml").exists());
}

#[test]
fn switching_branches_is_blocked_with_uncommitted_changes() {
    let s = setup();
    let repo = Repo::open(&s.a).unwrap();
    repo.create_branch("feature").unwrap();
    std::fs::write(s.a.join("collection.yaml"), "name: Work in progress\n").unwrap();

    match repo.switch("main") {
        Err(GitError::Blocked(message)) => assert!(message.contains("uncommitted")),
        other => panic!("expected a blocked switch, got {other:?}"),
    }
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("feature"));

    repo.stage_all().unwrap();
    repo.commit("wip").unwrap();
    repo.switch("main").unwrap();
    assert_eq!(repo.status().unwrap().branch.as_deref(), Some("main"));

    let names: Vec<(String, bool)> = repo
        .branches()
        .unwrap()
        .into_iter()
        .map(|b| (b.name, b.current))
        .collect();
    assert!(names.contains(&("main".into(), true)));
    assert!(names.contains(&("feature".into(), false)));
}

#[test]
fn a_collection_inside_a_bigger_repo_sees_only_its_files() {
    let s = setup();
    let collection = s.a.join("collections/shop");
    std::fs::create_dir_all(&collection).unwrap();
    std::fs::write(collection.join("get.yaml"), "x\n").unwrap();
    std::fs::write(s.a.join("README.md"), "outside\n").unwrap();

    let repo = Repo::open(&collection).unwrap();
    let files: Vec<String> = repo
        .status()
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(files, vec!["get.yaml"]);
    repo.stage_all().unwrap();
    // The file outside the collection was not staged.
    let root = Repo::open(&s.a).unwrap();
    let readme = root
        .status()
        .unwrap()
        .files
        .into_iter()
        .find(|f| f.path == "README.md")
        .unwrap();
    assert!(readme.untracked);
}

#[test]
fn folders_without_a_repo_can_be_initialized() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(Repo::open(dir.path()), Err(GitError::NotARepo(_))));
    let repo = frida_git::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.yaml"), "a\n").unwrap();
    let status = repo.status().unwrap();
    assert!(status.no_commits);
    repo.stage_all().unwrap();
    // Unstaging works before the first commit too (index only).
    repo.unstage_all().unwrap();
    assert!(repo.status().unwrap().files[0].untracked);
    assert!(dir.path().join("a.yaml").exists());
}

#[test]
fn file_versions_for_staged_unstaged_new_deleted_renamed_and_binary_files() {
    let s = setup();
    let repo = Repo::open(&s.a).unwrap();
    std::fs::write(s.a.join("gone.yaml"), "name: Gone\n").unwrap();
    std::fs::write(s.a.join("old name.yaml"), "name: Moved\n").unwrap();
    git(&s.a, &["add", "."]);
    git(&s.a, &["commit", "-m", "more"]);

    // Staged edit, then a further unstaged edit of the same file.
    std::fs::write(s.a.join("collection.yaml"), "name: Staged\n").unwrap();
    git(&s.a, &["add", "collection.yaml"]);
    std::fs::write(s.a.join("collection.yaml"), "name: Working\n").unwrap();
    let staged = repo.file_versions("collection.yaml", true).unwrap();
    assert_eq!(staged.old.as_deref(), Some("name: Test\n"));
    assert_eq!(staged.new.as_deref(), Some("name: Staged\n"));
    let unstaged = repo.file_versions("collection.yaml", false).unwrap();
    assert_eq!(unstaged.old.as_deref(), Some("name: Staged\n"));
    assert_eq!(unstaged.new.as_deref(), Some("name: Working\n"));

    // New (untracked) and deleted files have one side only.
    std::fs::write(s.a.join("new.yaml"), "name: New\n").unwrap();
    let new = repo.file_versions("new.yaml", false).unwrap();
    assert_eq!((new.old, new.new.as_deref()), (None, Some("name: New\n")));
    std::fs::remove_file(s.a.join("gone.yaml")).unwrap();
    let gone = repo.file_versions("gone.yaml", false).unwrap();
    assert_eq!(
        (gone.old.as_deref(), gone.new),
        (Some("name: Gone\n"), None)
    );

    // A staged rename compares with the old path in HEAD.
    git(&s.a, &["mv", "old name.yaml", "new name.yaml"]);
    let renamed = repo.file_versions("new name.yaml", true).unwrap();
    assert_eq!(renamed.old_path.as_deref(), Some("old name.yaml"));
    assert_eq!(renamed.old.as_deref(), Some("name: Moved\n"));
    assert_eq!(renamed.new.as_deref(), Some("name: Moved\n"));

    // Binary files are flagged and their content isn't sent.
    std::fs::write(s.a.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
    let binary = repo.file_versions("logo.png", false).unwrap();
    assert!(binary.binary);
    assert_eq!((binary.old, binary.new), (None, None));
}

#[test]
fn file_versions_inside_a_bigger_repo_use_collection_relative_paths() {
    let s = setup();
    let sub = s.a.join("collections").join("api");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("req.yaml"), "url: /a\n").unwrap();
    git(&s.a, &["add", "."]);
    git(&s.a, &["commit", "-m", "sub"]);
    std::fs::write(sub.join("req.yaml"), "url: /b\n").unwrap();

    let repo = Repo::open(&sub).unwrap();
    let versions = repo.file_versions("req.yaml", false).unwrap();
    assert_eq!(versions.old.as_deref(), Some("url: /a\n"));
    assert_eq!(versions.new.as_deref(), Some("url: /b\n"));
}
