//! Running `git` safely: only allowed subcommands, never an interactive prompt, and a time
//! limit so a hanging network operation can't block the app forever.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::{GitError, Result};

pub(crate) const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// The git subcommands Frida may run (PLAN.md §12). `reset`, `clean`, `checkout`, `rebase`,
/// `push --force` and friends are deliberately missing.
const ALLOWED: &[&str] = &[
    "--version",
    "init",
    "rev-parse",
    "status",
    "add",
    "restore",
    "rm",
    // Read-only: file contents at HEAD or in the index, for the diff viewer.
    "show",
    "commit",
    "fetch",
    "push",
    "pull",
    "for-each-ref",
    "switch",
];

/// Flags that would make an allowed command destructive.
const FORBIDDEN_FLAGS: &[&str] = &[
    "--force",
    "-f",
    "--force-with-lease",
    "--hard",
    "--worktree",
];

/// The git program set in the app settings, or `None` for `git` from the PATH.
///
/// A `static` is one value for the whole process, like a Java `static` field. Rust only allows
/// mutable statics behind a lock, so threads can't race on it: `RwLock` lets many readers (every
/// git call) in at once and a writer (changing the setting) alone.
static EXECUTABLE: RwLock<Option<PathBuf>> = RwLock::new(None);

pub(crate) fn set_executable(path: Option<PathBuf>) {
    *EXECUTABLE.write().expect("git executable lock poisoned") = path;
}

/// The program to run for `git`.
pub(crate) fn executable() -> PathBuf {
    EXECUTABLE
        .read()
        .expect("git executable lock poisoned")
        .clone()
        .unwrap_or_else(|| PathBuf::from("git"))
}

pub(crate) fn git(dir: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    git_allowing(dir, args, timeout, &[])
}

/// Like [`git`], but returns stdout as raw bytes (file contents may not be UTF-8).
pub(crate) fn git_bytes(dir: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    run_program_bytes(&executable(), dir, args, timeout, &[])
}

/// Like [`git`], but exit codes in `ok_codes` count as success too.
pub(crate) fn git_allowing(
    dir: &Path,
    args: &[&str],
    timeout: Duration,
    ok_codes: &[i32],
) -> Result<String> {
    run_program(&executable(), dir, args, timeout, ok_codes)
}

/// Runs `program` and returns stdout as text (invalid UTF-8 is replaced).
pub(crate) fn run_program(
    program: &Path,
    dir: &Path,
    args: &[&str],
    timeout: Duration,
    ok_codes: &[i32],
) -> Result<String> {
    run_program_bytes(program, dir, args, timeout, ok_codes)
        .map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// Runs `program` (a git executable) with `args` after checking they are allowed.
pub(crate) fn run_program_bytes(
    program: &Path,
    dir: &Path,
    args: &[&str],
    timeout: Duration,
    ok_codes: &[i32],
) -> Result<Vec<u8>> {
    let subcommand = args.first().copied().unwrap_or_default();
    // Only flags before `--` matter; after it come file names.
    let flags = args.iter().take_while(|a| **a != "--");
    assert!(
        ALLOWED.contains(&subcommand) && !flags.clone().any(|a| FORBIDDEN_FLAGS.contains(a)),
        "git {subcommand} is not allowed in Frida"
    );
    // `rm` is only allowed as `rm --cached` (index only, files stay on disk).
    if subcommand == "rm" {
        assert!(
            args.contains(&"--cached"),
            "git rm without --cached is not allowed"
        );
    }
    if subcommand == "restore" {
        assert!(
            args.contains(&"--staged"),
            "git restore without --staged is not allowed"
        );
    }

    let mut command = Command::new(program);
    hide_console_window(&mut command);
    let mut child = command
        .args(args)
        .current_dir(dir)
        // Never wait for a username/password prompt nobody can see.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        // Don't take locks for read-only commands (the file watcher may run status often).
        .env("GIT_OPTIONAL_LOCKS", "0")
        // English, parseable messages.
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::Io(e),
        })?;

    // Read output on threads, so a full pipe can't block git while we wait for it to exit.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(GitError::Timeout(subcommand.to_string()));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).to_string();

    let code = status.code().unwrap_or(-1);
    if status.success() || ok_codes.contains(&code) {
        Ok(stdout)
    } else {
        let message = if stderr.trim().is_empty() {
            String::from_utf8_lossy(&stdout).trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        Err(GitError::Command {
            command: format!("git {}", args.join(" ")),
            message: if message.is_empty() {
                format!("git {subcommand} failed (exit code {code})")
            } else {
                message
            },
        })
    }
}

/// Frida is a GUI app without a console. On Windows, every console program it starts (like
/// `git`) would otherwise get its own console window that flashes open and shut.
/// `CREATE_NO_WINDOW` tells Windows not to create one.
///
/// `#[cfg(windows)]` is conditional compilation (a bit like a build profile that drops code):
/// on macOS/Linux the first function doesn't exist, the empty second one is compiled instead.
/// `CommandExt` is an *extension trait*: importing it adds the Windows-only `creation_flags`
/// method to `Command`.
#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// From the Win32 API (`winbase.h`).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_git_path_is_reported_as_not_found() {
        let err = crate::version_of(Path::new("/no/such/git")).unwrap_err();
        assert!(matches!(err, GitError::NotInstalled));
    }

    #[test]
    #[should_panic(expected = "not allowed")]
    fn destructive_commands_are_refused() {
        let _ = git(Path::new("."), &["reset", "--hard"], LOCAL_TIMEOUT);
    }

    #[test]
    #[should_panic(expected = "not allowed")]
    fn force_push_is_refused() {
        let _ = git(Path::new("."), &["push", "--force"], LOCAL_TIMEOUT);
    }

    #[test]
    #[should_panic(expected = "not allowed")]
    fn rm_must_keep_files() {
        let _ = git(Path::new("."), &["rm", "--", "file"], LOCAL_TIMEOUT);
    }
}
