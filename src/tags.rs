//! Minimal ctags `tags` file support for `:tag` / `Ctrl-]` navigation.
//!
//! A `tags` file (produced by `ctags`) has one tag per line:
//! `name<TAB>file<TAB>address`, where the address is a line number or a vim
//! ex-search like `/^fn foo()$/`, optionally followed by a `;"` extension field.
//! Header lines beginning with `!_TAG_` are ignored.

use std::collections::HashMap;

/// One tag definition: the file it lives in and its address within that file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagEntry {
    pub file: String,
    /// A line number (e.g. `42`) or a search pattern (e.g. `/^fn foo$/`).
    pub address: String,
}

/// A parsed `tags` index mapping a tag name to its definitions.
#[derive(Debug, Default)]
pub struct Tags {
    map: HashMap<String, Vec<TagEntry>>,
}

impl Tags {
    /// Parse ctags-format text into an index.
    pub fn parse(text: &str) -> Self {
        let mut map: HashMap<String, Vec<TagEntry>> = HashMap::new();
        for line in text.lines() {
            if line.is_empty() || line.starts_with("!_TAG_") {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(name), Some(file), Some(rest)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if name.is_empty() || file.is_empty() {
                continue;
            }
            // The address runs up to an optional `;"` extension-field terminator.
            let address = match rest.find(";\"") {
                Some(i) => &rest[..i],
                None => rest,
            };
            map.entry(name.to_string()).or_default().push(TagEntry {
                file: file.to_string(),
                address: address.trim().to_string(),
            });
        }
        Tags { map }
    }

    /// The first definition for `name`, if any.
    pub fn find(&self, name: &str) -> Option<&TagEntry> {
        self.map.get(name).and_then(|v| v.first())
    }

    /// Number of distinct tag names.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Resolve a tag address to either a 1-based line number or a search pattern.
pub enum Address {
    Line(usize),
    Search(String),
}

/// Interpret a ctags address: a bare integer is a line number; a `/…/` or `?…?`
/// form is a search pattern (delimiters stripped).
pub fn parse_address(address: &str) -> Option<Address> {
    let a = address.trim();
    if let Ok(n) = a.parse::<usize>() {
        return Some(Address::Line(n));
    }
    let inner = a
        .strip_prefix('/')
        .and_then(|s| s.strip_suffix('/'))
        .or_else(|| a.strip_prefix('?').and_then(|s| s.strip_suffix('?')))?;
    if inner.is_empty() {
        return None;
    }
    Some(Address::Search(inner.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_and_skips_headers() {
        let text = "!_TAG_FILE_FORMAT\t2\t//\n\
                    main\tsrc/main.rs\t/^fn main() {$/;\"\tf\n\
                    Point\tsrc/geo.rs\t42\n";
        let tags = Tags::parse(text);
        assert_eq!(tags.len(), 2);
        assert_eq!(
            tags.find("main"),
            Some(&TagEntry { file: "src/main.rs".into(), address: "/^fn main() {$/".into() })
        );
        assert_eq!(
            tags.find("Point"),
            Some(&TagEntry { file: "src/geo.rs".into(), address: "42".into() })
        );
        assert!(tags.find("missing").is_none());
    }

    #[test]
    fn address_parses_line_and_search() {
        assert!(matches!(parse_address("42"), Some(Address::Line(42))));
        match parse_address("/^fn main$/") {
            Some(Address::Search(p)) => assert_eq!(p, "^fn main$"),
            _ => panic!("expected search"),
        }
        assert!(parse_address("").is_none());
    }
}
