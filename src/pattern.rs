//! Pattern compilation for search and substitute.
//!
//! Patterns are treated as regular expressions (via the `regex` crate). If a
//! pattern isn't valid regex, it falls back to a literal match so a stray `(`
//! or `*` never breaks search or `:s`.

use regex::Regex;

/// Compile a pattern into a [`Regex`], falling back to a literal match on
/// invalid regex. Returns `None` for an empty pattern.
pub fn build(pattern: &str) -> Option<Regex> {
    if pattern.is_empty() {
        return None;
    }
    Regex::new(pattern)
        .ok()
        .or_else(|| Regex::new(&regex::escape(pattern)).ok())
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
    fn char_indices_account_for_multibyte() {
        let re = build("x").unwrap();
        // "é" is two bytes but one char; the match at the 'x' should be char 2.
        let ranges = match_ranges(&re, "éqx");
        assert_eq!(ranges, vec![(2, 3)]);
    }
}
