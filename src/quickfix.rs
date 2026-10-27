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
/// Maximum directory recursion depth — bounds pathological trees and breaks any
/// symlink cycle (since `metadata` follows symlinks).
const MAX_DEPTH: usize = 64;

/// Search each path in `paths` (a file, or a directory searched recursively) for
/// `pattern`, returning the matches in traversal order. Non-UTF-8 / oversized
/// files and common noise directories (`.git`, `target`, `node_modules`) are
/// skipped. Returns an error only for an invalid pattern.
pub fn grep(pattern: &str, paths: &[PathBuf], ignorecase: bool) -> Result<Vec<QfEntry>, String> {
    let re = pattern::build_opts(pattern, ignorecase)
        .ok_or_else(|| format!("E486: invalid pattern: {pattern}"))?;
    let mut out = Vec::new();
    for p in paths {
        walk(p, &re, None, &mut out, 0);
        if out.len() >= MAX_MATCHES {
            break;
        }
    }
    Ok(out)
}

/// Like [`grep`], but a path argument containing glob metacharacters (`*`, `?`) is
/// matched against file paths instead of taken literally: `*.rs` searches `.rs`
/// files in the directory, `**/*.rs` recurses, `src/*.py` scopes to `src`. Plain
/// paths (no metacharacters) behave exactly as in [`grep`].
pub fn grep_args(pattern: &str, raw_paths: &[String], ignorecase: bool) -> Result<Vec<QfEntry>, String> {
    let re = pattern::build_opts(pattern, ignorecase)
        .ok_or_else(|| format!("E486: invalid pattern: {pattern}"))?;
    let mut out = Vec::new();
    for raw in raw_paths {
        if raw.contains(['*', '?']) {
            // Glob: walk from the leading literal directory, filtering by the glob.
            let norm = raw.replace('\\', "/");
            let base = glob_base(&norm);
            let root = if base.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                base
            };
            let include = regex::Regex::new(&glob_to_regex(&norm)).ok();
            walk(&root, &re, include.as_ref(), &mut out, 0);
        } else {
            walk(&PathBuf::from(raw), &re, None, &mut out, 0);
        }
        if out.len() >= MAX_MATCHES {
            break;
        }
    }
    Ok(out)
}

/// The leading directory of a glob, up to (not including) the first segment that
/// contains a metacharacter. Empty when the glob starts with one (e.g. `*.rs`).
/// Taken as a substring (not rebuilt from segments) so absolute roots like
/// `C:/…` keep their shape across platforms.
fn glob_base(glob_norm: &str) -> PathBuf {
    let first_meta = glob_norm.find(['*', '?']).unwrap_or(glob_norm.len());
    match glob_norm[..first_meta].rfind('/') {
        Some(i) => PathBuf::from(&glob_norm[..i]),
        None => PathBuf::new(),
    }
}

/// Convert a `/`-normalized glob into an anchored regex: `**` matches any run
/// (including `/`), `*` matches within one path segment, `?` one non-`/` char.
fn glob_to_regex(glob_norm: &str) -> String {
    let chars: Vec<char> = glob_norm.chars().collect();
    let mut re = String::from("^");
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    re.push_str(".*"); // ** — across directories
                    i += 2;
                    if i < chars.len() && chars[i] == '/' {
                        i += 1; // `**/` also matches zero directories
                    }
                } else {
                    re.push_str("[^/]*"); // * — within one segment
                    i += 1;
                }
            }
            '?' => {
                re.push_str("[^/]");
                i += 1;
            }
            c => {
                re.push_str(&regex::escape(&c.to_string()));
                i += 1;
            }
        }
    }
    re.push('$');
    re
}

/// Normalize a walked path for glob matching: `\` to `/`, drop a leading `./`.
fn norm_for_match(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    s.strip_prefix("./").unwrap_or(&s).to_string()
}

/// Whether a directory entry's name is one we never descend into.
fn is_noise(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

fn walk(
    path: &Path,
    re: &regex::Regex,
    include: Option<&regex::Regex>,
    out: &mut Vec<QfEntry>,
    depth: usize,
) {
    if out.len() >= MAX_MATCHES {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if depth >= MAX_DEPTH {
            return; // too deep (or a symlink cycle) — stop descending
        }
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
            walk(&child, re, include, out, depth + 1);
            if out.len() >= MAX_MATCHES {
                return;
            }
        }
        return;
    }
    if meta.len() > MAX_FILE_BYTES {
        return;
    }
    // With a glob filter, only search files whose path matches it.
    if let Some(inc) = include {
        if !inc.is_match(&norm_for_match(path)) {
            return;
        }
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

    // A unique per-test directory so tests running in parallel never collide.
    fn tmp(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("rvim_qf_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn grep_finds_matches_recursively_and_skips_noise() {
        let dir = tmp("recurse");
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
    fn glob_to_regex_segments_and_recursion() {
        assert_eq!(glob_to_regex("*.rs"), "^[^/]*\\.rs$");
        assert_eq!(glob_to_regex("src/**/*.rs"), "^src/.*[^/]*\\.rs$");
        assert_eq!(glob_to_regex("a?b"), "^a[^/]b$");
    }

    #[test]
    fn grep_args_glob_filters_by_extension() {
        let dir = tmp("glob");
        std::fs::write(dir.join("a.rs"), "hit here\n").unwrap();
        std::fs::write(dir.join("b.txt"), "hit here\n").unwrap();
        let sub = dir.join("pkg");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("c.rs"), "hit here\n").unwrap();

        // `<dir>/*.rs` matches only a.rs (one segment, not recursive).
        let shallow = format!("{}/*.rs", dir.display());
        let hits = grep_args("hit", &[shallow], false).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].file.ends_with("a.rs"));

        // `<dir>/**/*.rs` recurses into pkg/ too.
        let deep = format!("{}/**/*.rs", dir.display());
        let hits = grep_args("hit", &[deep], false).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.file.ends_with("a.rs")));
        assert!(hits.iter().any(|h| h.file.ends_with("c.rs")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grep_ignorecase_and_bad_pattern() {
        let dir = tmp("case");
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
