//! Parsing `git status --porcelain=v2 --branch -z` (documented in `git help status`).
//!
//! Each record ends with a NUL byte. Header lines start with `#`; changed files with `1`
//! (ordinary), `2` (renamed/copied, followed by a second record with the old path), `u`
//! (unmerged) and `?` (untracked).

use crate::{FileChange, Status};

pub fn parse_status(output: &str) -> Status {
    let mut status = Status::default();
    let mut records = output.split('\0').filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" => status.no_commits = value == "(initial)",
                "branch.head" => {
                    status.branch = (value != "(detached)").then(|| value.to_string());
                }
                "branch.upstream" => status.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            status.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            status.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = record.chars().next().unwrap_or(' ');
        match kind {
            '1' | '2' => {
                // `1 XY sub mH mI mW hH hI path` / `2 XY sub mH mI mW hH hI Xscore path`
                let fields = if kind == '1' { 8 } else { 9 };
                let parts: Vec<&str> = record.splitn(fields + 1, ' ').collect();
                let xy = parts.get(1).copied().unwrap_or("..");
                let path = parts.get(fields).copied().unwrap_or_default().to_string();
                let orig_path = if kind == '2' {
                    records.next().map(str::to_string)
                } else {
                    None
                };
                status.files.push(FileChange {
                    path,
                    orig_path,
                    staged: letter(xy.chars().next()),
                    unstaged: letter(xy.chars().nth(1)),
                    untracked: false,
                    conflicted: false,
                });
            }
            'u' => {
                // `u XY sub m1 m2 m3 mW h1 h2 h3 path`
                let parts: Vec<&str> = record.splitn(11, ' ').collect();
                status.files.push(FileChange {
                    path: parts.get(10).copied().unwrap_or_default().to_string(),
                    orig_path: None,
                    staged: None,
                    unstaged: Some('U'),
                    untracked: false,
                    conflicted: true,
                });
            }
            '?' => status.files.push(FileChange {
                path: record[2..].to_string(),
                orig_path: None,
                staged: None,
                unstaged: Some('?'),
                untracked: true,
                conflicted: false,
            }),
            _ => {}
        }
    }
    status
}

/// `.` means "unchanged" in porcelain v2.
fn letter(c: Option<char>) -> Option<char> {
    c.filter(|c| *c != '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_headers_and_every_entry_kind() {
        let output = [
            "# branch.oid 1234",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "1 M. N... 100644 100644 100644 aaa bbb staged.yaml",
            "1 .M N... 100644 100644 100644 aaa bbb dir/with space.yaml",
            "2 R. N... 100644 100644 100644 aaa bbb R100 new.yaml",
            "old.yaml",
            "u UU N... 100644 100644 100644 100644 a b c conflict.yaml",
            "? untracked.yaml",
            "",
        ]
        .join("\0");
        let status = parse_status(&output);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
        assert!(!status.no_commits);

        let files: Vec<(&str, Option<char>, Option<char>)> = status
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.staged, f.unstaged))
            .collect();
        assert_eq!(
            files,
            vec![
                ("staged.yaml", Some('M'), None),
                ("dir/with space.yaml", None, Some('M')),
                ("new.yaml", Some('R'), None),
                ("conflict.yaml", None, Some('U')),
                ("untracked.yaml", None, Some('?')),
            ]
        );
        assert_eq!(status.files[2].orig_path.as_deref(), Some("old.yaml"));
        assert!(status.files[3].conflicted);
        assert!(status.files[4].untracked);
    }

    #[test]
    fn initial_and_detached() {
        let status = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert!(status.no_commits);
        assert_eq!(status.branch, None);
    }
}
