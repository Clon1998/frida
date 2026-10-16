//! `frida-git`: a thin wrapper around the system `git` CLI for the git panel (PLAN.md §12).
//!
//! Shelling out (instead of a git library) reuses the user's SSH keys, credential helpers and
//! config. Every call goes through [`Repo::run`], which only allows the subcommands the panel
//! needs: nothing destructive (no `reset`, `clean`, `checkout -- file` or force push) is
//! possible through this crate.
//!
//! All functions block (they wait for `git`), so the app calls them from `spawn_blocking`.

#![doc = "See PLAN.md §12."]

mod parse;
mod run;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

pub use parse::parse_status;

/// Why a git operation failed. Messages are shown in the UI.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git was not found (install it, or set its path in Settings → Git)")]
    NotInstalled,

    #[error("{0} is not inside a git repository")]
    NotARepo(PathBuf),

    /// `git` ran and failed; `message` is what it printed.
    #[error("{message}")]
    Command { command: String, message: String },

    /// The operation was refused before running git (e.g. switching with changes).
    #[error("{0}")]
    Blocked(String),

    #[error("git {0} took too long and was stopped")]
    Timeout(String),

    #[error("could not run git: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// One changed file. Status letters as in `git status --short` (`M`, `A`, `D`, `R`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Relative to the folder the repo was opened for (the collection).
    pub path: String,
    /// The old path of a rename.
    pub orig_path: Option<String>,
    /// Change in the index (staged), if any.
    pub staged: Option<char>,
    /// Change in the working tree (not staged), if any.
    pub unstaged: Option<char>,
    pub untracked: bool,
    /// Has merge conflicts.
    pub conflicted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// `None` when HEAD is detached.
    pub branch: Option<String>,
    /// e.g. `origin/main`.
    pub upstream: Option<String>,
    /// Commits not pushed / not pulled yet (as of the last fetch).
    pub ahead: u32,
    pub behind: u32,
    /// The repository has no commits yet.
    pub no_commits: bool,
    pub files: Vec<FileChange>,
}

impl Status {
    /// Uncommitted changes to tracked files (untracked files don't block a branch switch).
    pub fn has_tracked_changes(&self) -> bool {
        self.files
            .iter()
            .any(|f| !f.untracked && (f.staged.is_some() || f.unstaged.is_some() || f.conflicted))
    }
}

/// Files bigger than this aren't loaded into the diff viewer.
pub const MAX_DIFF_FILE_BYTES: usize = 2 * 1024 * 1024;

/// The two sides of one file's change, for a side-by-side or unified diff viewer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileVersions {
    /// Before the change: `HEAD` for a staged change, the index for an unstaged one. `None` for
    /// a new file (and when `binary` or `too_large`).
    pub old: Option<String>,
    /// After the change: the index for a staged change, the working tree for an unstaged one.
    /// `None` for a deleted file (and when `binary` or `too_large`).
    pub new: Option<String>,
    /// The old path of a renamed file.
    pub old_path: Option<String>,
    /// One side isn't text; no content is sent.
    pub binary: bool,
    /// One side is bigger than [`MAX_DIFF_FILE_BYTES`]; no content is sent.
    pub too_large: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub current: bool,
    /// A remote-tracking branch (`origin/feature`) without a local branch yet.
    pub remote: bool,
}

/// A git repository, used from the folder `dir` (a collection, which may be the repository
/// root or a folder inside it). File lists and "stage all" are limited to `dir`.
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

/// The installed git's version, e.g. `2.50.1`. Errors with [`GitError::NotInstalled`].
pub fn version() -> Result<String> {
    version_of(&run::executable())
}

/// The version of the git program at `executable` (for "Test" in the settings).
pub fn version_of(executable: &Path) -> Result<String> {
    let out = run::run_program(
        executable,
        Path::new("."),
        &["--version"],
        run::LOCAL_TIMEOUT,
        &[],
    )?;
    Ok(out.trim().trim_start_matches("git version ").to_string())
}

/// Which git program every call runs: a path from the settings, or `None` for `git` from the
/// PATH. Applies to the whole process.
pub fn set_executable(path: Option<PathBuf>) {
    run::set_executable(path);
}

/// Turns `dir` into a git repository (`git init`).
pub fn init(dir: &Path) -> Result<Repo> {
    run::git(dir, &["init"], run::LOCAL_TIMEOUT)?;
    Ok(Repo {
        dir: dir.to_path_buf(),
    })
}

impl Repo {
    /// Opens the repository containing `dir`.
    pub fn open(dir: &Path) -> Result<Self> {
        let repo = Self {
            dir: dir.to_path_buf(),
        };
        match repo.run(&["rev-parse", "--is-inside-work-tree"]) {
            Ok(out) if out.trim() == "true" => Ok(repo),
            Ok(_) => Err(GitError::NotARepo(dir.to_path_buf())),
            Err(GitError::Command { message, .. }) if message.contains("not a git repository") => {
                Err(GitError::NotARepo(dir.to_path_buf()))
            }
            Err(err) => Err(err),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The repository's top-level folder.
    pub fn root(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.run(&["rev-parse", "--show-toplevel"])?.trim(),
        ))
    }

    /// Branch, upstream, ahead/behind and the changed files below the folder.
    pub fn status(&self) -> Result<Status> {
        let out = self.run(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ])?;
        let mut status = parse_status(&out);
        // Paths come relative to the repository root; show them relative to the folder.
        let prefix = self
            .run(&["rev-parse", "--show-prefix"])?
            .trim()
            .to_string();
        if !prefix.is_empty() {
            for file in &mut status.files {
                if let Some(rel) = file.path.strip_prefix(&prefix) {
                    file.path = rel.to_string();
                }
                if let Some(orig) = &file.orig_path {
                    file.orig_path = orig.strip_prefix(&prefix).map(str::to_string);
                }
            }
        }
        Ok(status)
    }

    pub fn stage(&self, paths: &[String]) -> Result<()> {
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    /// Stages every change below the folder, including new and deleted files.
    pub fn stage_all(&self) -> Result<()> {
        self.run(&["add", "--all", "--", "."]).map(drop)
    }

    /// Takes files out of the index again. The files themselves are not touched.
    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        let mut args: Vec<&str> = if self.has_commits()? {
            vec!["restore", "--staged", "--"]
        } else {
            // Without a commit there is nothing to restore from; this only removes the files
            // from the index (`--cached`), never from disk.
            vec!["rm", "--cached", "-r", "--quiet", "--"]
        };
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    pub fn unstage_all(&self) -> Result<()> {
        self.unstage(&[".".to_string()])
    }

    /// The old and new content of `path` for the staged change (`HEAD` → index) or the working
    /// tree change (index → file on disk). Read-only: uses `git show <rev>:./<path>`, where
    /// `./` makes the path relative to the collection folder instead of the repository root.
    pub fn file_versions(&self, path: &str, staged: bool) -> Result<FileVersions> {
        let status = self.status()?;
        let file = status.files.iter().find(|f| f.path == path);
        let old_path = file.and_then(|f| f.orig_path.clone());
        let (old, new) = if staged {
            let letter = file.and_then(|f| f.staged);
            let old = if letter == Some('A') || status.no_commits {
                None
            } else {
                let from = old_path.as_deref().unwrap_or(path);
                Some(self.show(&format!("HEAD:./{from}"))?)
            };
            let new = if letter == Some('D') {
                None
            } else {
                Some(self.show(&format!(":./{path}"))?)
            };
            (old, new)
        } else {
            let old = if file.is_some_and(|f| f.untracked) {
                None
            } else {
                Some(self.show(&format!(":./{path}"))?)
            };
            let new = if file.and_then(|f| f.unstaged) == Some('D') {
                None
            } else {
                let full = self.dir.join(path);
                Some(std::fs::read(&full)?)
            };
            (old, new)
        };

        let mut versions = FileVersions {
            old_path,
            ..FileVersions::default()
        };
        // `Option::iter().chain(..)` walks both sides that exist, without cloning them.
        let sides = old.iter().chain(new.iter());
        if sides.clone().any(|b| b.len() > MAX_DIFF_FILE_BYTES) {
            versions.too_large = true;
            return Ok(versions);
        }
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).ok().filter(|t| !t.contains('\0'));
        let old_text = old.map(text);
        let new_text = new.map(text);
        // `Some(None)`: the side exists but isn't text.
        if matches!(old_text, Some(None)) || matches!(new_text, Some(None)) {
            versions.binary = true;
            return Ok(versions);
        }
        versions.old = old_text.flatten();
        versions.new = new_text.flatten();
        Ok(versions)
    }

    /// The raw bytes of `git show <object>` (`HEAD:./file`, `:./file` for the index).
    fn show(&self, object: &str) -> Result<Vec<u8>> {
        run::git_bytes(&self.dir, &["show", object], run::LOCAL_TIMEOUT)
    }

    pub fn commit(&self, message: &str) -> Result<()> {
        if message.trim().is_empty() {
            return Err(GitError::Blocked("Enter a commit message.".into()));
        }
        self.run(&["commit", "--quiet", "-m", message]).map(drop)
    }

    /// Updates the remote-tracking branches, so ahead/behind are current.
    pub fn fetch(&self) -> Result<()> {
        self.run_network(&["fetch", "--quiet"]).map(drop)
    }

    /// Pushes the current branch. A branch without upstream is pushed to `origin` and set up
    /// to track it. Never forced.
    pub fn push(&self) -> Result<()> {
        let status = self.status()?;
        let Some(branch) = status.branch else {
            return Err(GitError::Blocked(
                "HEAD is detached; switch to a branch before pushing.".into(),
            ));
        };
        if status.upstream.is_some() {
            self.run_network(&["push", "--quiet"]).map(drop)
        } else {
            self.run_network(&["push", "--quiet", "--set-upstream", "origin", &branch])
                .map(drop)
        }
    }

    /// Pulls with `--ff-only`: if the branches diverged, git refuses and its message explains
    /// why; resolving that is left to the user outside Frida.
    pub fn pull(&self) -> Result<()> {
        self.run_network(&["pull", "--ff-only", "--quiet"])
            .map(drop)
    }

    /// Local branches, then remote-tracking branches that have no local branch.
    pub fn branches(&self) -> Result<Vec<Branch>> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(refname)\t%(HEAD)",
            "refs/heads",
            "refs/remotes",
        ])?;
        let mut local = Vec::new();
        let mut remote = Vec::new();
        for line in out.lines() {
            let (refname, head) = line.split_once('\t').unwrap_or((line, ""));
            if let Some(name) = refname.strip_prefix("refs/heads/") {
                local.push(Branch {
                    name: name.to_string(),
                    current: head == "*",
                    remote: false,
                });
            } else if let Some(name) = refname.strip_prefix("refs/remotes/")
                && !name.ends_with("/HEAD")
            {
                remote.push(name.to_string());
            }
        }
        for name in remote {
            let short = name.split_once('/').map(|(_, b)| b).unwrap_or(&name);
            if !local.iter().any(|b| b.name == short) {
                local.push(Branch {
                    name,
                    current: false,
                    remote: true,
                });
            }
        }
        Ok(local)
    }

    /// Switches to `branch` (for `origin/x`, creates a local `x` tracking it). Refused while
    /// tracked files have uncommitted changes, so nothing is lost or carried over by surprise.
    pub fn switch(&self, branch: &str) -> Result<()> {
        if self.status()?.has_tracked_changes() {
            return Err(GitError::Blocked(
                "There are uncommitted changes. Commit them first, then switch branches.".into(),
            ));
        }
        let remote_branch = self
            .branches()?
            .into_iter()
            .find(|b| b.name == branch && b.remote);
        match remote_branch {
            Some(_) => {
                let local = branch.split_once('/').map(|(_, b)| b).unwrap_or(branch);
                self.run(&["switch", "--quiet", "--track", "-c", local, branch])
                    .map(drop)
            }
            None => self.run(&["switch", "--quiet", branch]).map(drop),
        }
    }

    /// Creates a branch from the current commit and switches to it (changes are kept).
    pub fn create_branch(&self, name: &str) -> Result<()> {
        self.run(&["switch", "--quiet", "-c", name]).map(drop)
    }

    fn has_commits(&self) -> Result<bool> {
        match self.run(&["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Ok(_) => Ok(true),
            Err(GitError::Command { .. }) => Ok(false),
            Err(err) => Err(err),
        }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        run::git(&self.dir, args, run::LOCAL_TIMEOUT)
    }

    fn run_network(&self, args: &[&str]) -> Result<String> {
        run::git(&self.dir, args, run::NETWORK_TIMEOUT)
    }
}
