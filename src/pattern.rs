//! Pattern compilation for search and substitute.
//!
//! Patterns are treated as regular expressions (via the `regex` crate). If a
//! pattern isn't valid regex, it falls back to a literal match so a stray `(`
//! or `*` never breaks search or `:s`.

use regex::Regex;

/// Compile a pattern into a [`Regex`], honoring vim's `\c`/`\C` case markers,
/// falling back to a literal match on invalid regex. `None` for empty input.
pub fn build(pattern: &str) -> Option<Regex> {
    build_opts(pattern, false)
}

/// Like [`build`], but `ignorecase` forces case-insensitive matching (the
/// `:s///i` flag). An embedded `\C` still forces case-sensitive.
pub fn build_opts(pattern: &str, ignorecase: bool) -> Option<Regex> {
    if pattern.is_empty() {
        return None;
    }
    let force_sensitive = pattern.contains("\\C");
    let force_insensitive = pattern.contains("\\c");
    let cleaned = pattern.replace("\\c", "").replace("\\C", "");
    if cleaned.is_empty() {
        return None;
    }
    let ci = !force_sensitive && (force_insensitive || ignorecase);
    let prefix = if ci { "(?i)" } else { "" };
    Regex::new(&format!("{prefix}{cleaned}"))
        .ok()
        .or_else(|| Regex::new(&format!("{prefix}{}", regex::escape(&cleaned))).ok())
}

/// Translate a vim-style `:s` replacement into the `regex` crate's syntax.
///
/// vim uses `\1`–`\9` for captures, `\0`/`&` for the whole match, `\&` for a
/// literal `&`, `\\` for a literal backslash, and treats `$` literally. The
/// regex crate uses `${1}` and `$$` for a literal `$`.
pub fn vim_replacement(src: &str) -> String {
    let mut out = String::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(d) if d.is_ascii_digit() => {
                    out.push_str(&format!("${{{d}}}"));
                }
                Some('&') => out.push('&'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            },
            '&' => out.push_str("${0}"),
            // `$` is literal in vim replacements; escape it for the regex crate.
            '$' => out.push_str("$$"),
            other => out.push(other),
        }
    }
    out
}

/// Char-index ranges `[start, end)` of every non-overlapping match of `re` in
/// `line`. Zero-width matches are skipped so highlighting can't loop.
pub fn match_ranges(re: &Regex, line: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for m in re.find_iter(line) {
        if m.start() == m.end() {
            continue;
        }
        let cstart = line[..m.start()].chars().count();
        let cend = line[..m.end()].chars().count();
        ranges.push((cstart, cend));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_fallback_for_invalid_regex() {
        // An unbalanced paren isn't valid regex; it should match literally.
        let re = build("(oops").unwrap();
        assert!(re.is_match("a (oops b"));
    }

    #[test]
    fn regex_metacharacters_work() {
        let re = build(r"\d+").unwrap();
        let ranges = match_ranges(&re, "ab123cd45");
        assert_eq!(ranges, vec![(2, 5), (7, 9)]);
    }

    #[test]
    fn empty_pattern_is_none() {
        assert!(build("").is_none());
    }

    #[test]
    fn ignorecase_flag_and_markers() {
        assert!(build_opts("foo", true).unwrap().is_match("FOO"));
        assert!(!build_opts("foo", false).unwrap().is_match("FOO"));
        // \c marker forces case-insensitive; \C forces sensitive.
        assert!(build("fo\\co").unwrap().is_match("FOO"));
        assert!(!build_opts("fo\\Co", true).unwrap().is_match("FOO"));
    }

    #[test]
    fn vim_replacement_translates_backrefs() {
        assert_eq!(vim_replacement(r"\1-\2"), "${1}-${2}");
        assert_eq!(vim_replacement("&!"), "${0}!");
        assert_eq!(vim_replacement(r"\&"), "&"); // literal ampersand
        assert_eq!(vim_replacement("$5.00"), "$$5.00"); // literal dollar
        assert_eq!(vim_replacement(r"a\\b"), r"a\b"); // literal backslash
    }

    #[test]
    fn char_indices_account_for_multibyte() {
        let re = build("x").unwrap();
        // "é" is two bytes but one char; the match at the 'x' should be char 2.
        let ranges = match_ranges(&re, "éqx");
        assert_eq!(ranges, vec![(2, 3)]);
    }
}
