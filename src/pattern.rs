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
    // vim's word-boundary atoms `\<` / `\>` map to the regex crate's `\b` (which
    // the crate would otherwise reject, dropping the pattern to a literal match).
    let cleaned = cleaned.replace("\\<", "\\b").replace("\\>", "\\b");
    if cleaned.is_empty() {
        return None;
    }
    let ci = !force_sensitive && (force_insensitive || ignorecase);
    let prefix = if ci { "(?i)" } else { "" };
    Regex::new(&format!("{prefix}{cleaned}"))
        .ok()
        .or_else(|| Regex::new(&format!("{prefix}{}", regex::escape(&cleaned))).ok())
}

/// Expand a vim-style `:s` replacement against a match's captures.
///
/// Supports `\1`–`\9` (captures), `\0` / `&` (whole match), `\&` (literal `&`),
/// `\\` (literal backslash), `\n` / `\t` / `\r`, and the case escapes `\u` / `\l`
/// (upper/lower the next character), `\U` / `\L` (…until `\E` / `\e`). `$` is
/// literal. Done as a closure expander (rather than a `${1}` template) precisely
/// so the case escapes can be applied, which the regex crate's templates cannot.
pub fn expand_replacement(caps: &regex::Captures, src: &str) -> String {
    /// Append `text`, applying the pending one-shot (`case_next`) or running
    /// (`case_run`) case transform. `Some(true)` = upper, `Some(false)` = lower.
    fn emit(
        text: &str,
        out: &mut String,
        case_next: &mut Option<bool>,
        case_run: &mut Option<bool>,
    ) {
        for ch in text.chars() {
            let how = case_next.take().or(*case_run);
            match how {
                Some(true) => out.extend(ch.to_uppercase()),
                Some(false) => out.extend(ch.to_lowercase()),
                None => out.push(ch),
            }
        }
    }

    let group = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let mut out = String::new();
    let mut case_next: Option<bool> = None;
    let mut case_run: Option<bool> = None;
    let mut it = src.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some(d) if d.is_ascii_digit() => {
                    emit(group(d as usize - '0' as usize), &mut out, &mut case_next, &mut case_run)
                }
                Some('&') => emit("&", &mut out, &mut case_next, &mut case_run),
                Some('\\') => emit("\\", &mut out, &mut case_next, &mut case_run),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('u') => case_next = Some(true),
                Some('l') => case_next = Some(false),
                Some('U') => case_run = Some(true),
                Some('L') => case_run = Some(false),
                Some('e') | Some('E') => case_run = None,
                Some(other) => {
                    let mut buf = [0u8; 4];
                    emit(other.encode_utf8(&mut buf), &mut out, &mut case_next, &mut case_run)
                }
                None => out.push('\\'),
            },
            '&' => emit(group(0), &mut out, &mut case_next, &mut case_run),
            other => {
                let mut buf = [0u8; 4];
                emit(other.encode_utf8(&mut buf), &mut out, &mut case_next, &mut case_run)
            }
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
    fn vim_word_boundaries_translate_to_b() {
        // `\<cat\>` matches "cat" as a whole word, not inside "category".
        let re = build(r"\<cat\>").unwrap();
        assert!(re.is_match("the cat sat"));
        assert!(!re.is_match("category"));
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
    fn expand_replacement_backrefs_and_case() {
        let re = Regex::new(r"(\w+)\s+(\w+)").unwrap();
        let caps = re.captures("foo bar").unwrap();
        assert_eq!(expand_replacement(&caps, r"\2-\1"), "bar-foo"); // backrefs
        assert_eq!(expand_replacement(&caps, "&!"), "foo bar!"); // whole match
        assert_eq!(expand_replacement(&caps, r"\&"), "&"); // literal ampersand
        assert_eq!(expand_replacement(&caps, "$5.00"), "$5.00"); // $ is literal
        assert_eq!(expand_replacement(&caps, r"a\\b"), r"a\b"); // literal backslash
        // Case escapes.
        assert_eq!(expand_replacement(&caps, r"\u\1"), "Foo"); // upper next char
        assert_eq!(expand_replacement(&caps, r"\U\1\E\2"), "FOObar"); // upper run to \E
    }

    #[test]
    fn char_indices_account_for_multibyte() {
        let re = build("x").unwrap();
        // "é" is two bytes but one char; the match at the 'x' should be char 2.
        let ranges = match_ranges(&re, "éqx");
        assert_eq!(ranges, vec![(2, 3)]);
    }
}
