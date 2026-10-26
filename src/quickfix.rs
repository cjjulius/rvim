//! A small quickfix list for `:grep` — search files (and directories, recursed)
//! for a pattern and collect `(file, line, text)` matches to navigate with
//! `:cnext` / `:cprev` / `:copen`.

use crate::pattern;
use std::path::{Path, PathBuf};

/// One match in the quickfix list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QfEntry {
    pub file: PathBuf,
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// Maximum matches collected (keeps a runaway search bounded).
const MAX_MATCHES: usize = 1000;
/// Files larger than this are skipped (likely data/binaries, not source).
const MAX_FILE_BYTES: u64 = 2_000_000;

/// Search each path in `paths` (a file, or a directory searched recursively) for
/// `pattern`, returning the matches in traversal order. Non-UTF-8 / oversized
/// files and common noise directories (`.git`, `target`, `node_modules`) are
/// skipped. Returns an error only for an invalid pattern.
pub fn grep(pattern: &str, paths: &[PathBuf], ignorecase: bool) -> Result<Vec<QfEntry>, String> {
    let re = pattern::build_opts(pattern, ignorecase)
        .ok_or_else(|| format!("E486: invalid pattern: {pattern}"))?;
    let mut out = Vec::new();
    for p in paths {
        walk(p, &re, &mut out);
        if out.len() >= MAX_MATCHES {
            break;
        }
    }
    Ok(out)
}

/// Whether a directory entry's name is one we never descend into.
fn is_noise(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

fn walk(path: &Path, re: &regex::Regex, out: &mut Vec<QfEntry>) {
    if out.len() >= MAX_MATCHES {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else {
            return;
        };
        let mut children: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        children.sort(); // deterministic order
        for child in children {
            if let Some(name) = child.file_name().and_then(|n| n.to_str()) {
                if is_noise(name) {
                    continue;
                }
            }
            walk(&child, re, out);
            if out.len() >= MAX_MATCHES {
                return;
            }
        }
        return;
    }
    if meta.len() > MAX_FILE_BYTES {
        return;
    }
    // read_to_string fails on non-UTF-8 content, so binaries are skipped.
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for (i, line) in text.lines().enumerate() {
        if re.is_match(line) {
            out.push(QfEntry {
                file: path.to_path_buf(),
                line: i + 1,
                text: line.to_string(),
            });
            if out.len() >= MAX_MATCHES {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("rvim_qf_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn grep_finds_matches_recursively_and_skips_noise() {
        let dir = tmp();
        std::fs::write(dir.join("a.txt"), "alpha\nfind me\nbeta\n").unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.txt"), "nothing\nfind me too\n").unwrap();
        // A noise dir that must be skipped.
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("c.txt"), "find me not\n").unwrap();

        let hits = grep("find", std::slice::from_ref(&dir), false).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.text.contains("find")));
        assert!(hits.iter().any(|h| h.line == 2 && h.file.ends_with("a.txt")));
        assert!(hits.iter().any(|h| h.file.ends_with("b.txt")));
        assert!(!hits.iter().any(|h| h.file.to_string_lossy().contains(".git")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grep_ignorecase_and_bad_pattern() {
        let dir = tmp();
        let f = dir.join("case.txt");
        std::fs::write(&f, "Hello World\n").unwrap();
        let one = std::slice::from_ref(&f);
        assert_eq!(grep("hello", one, false).unwrap().len(), 0);
        assert_eq!(grep("hello", one, true).unwrap().len(), 1);
        // An invalid regex falls back to a literal match (never errors); an empty
        // pattern is the one rejected.
        assert_eq!(grep("(", one, false).unwrap().len(), 0);
        assert!(grep("", one, false).is_err());
        let _ = std::fs::remove_file(&f);
    }
}
