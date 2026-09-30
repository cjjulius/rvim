//! Syntax highlighting: language autodetection + pluggable per-language
//! highlighters built on a shared, spec-driven tokenizer.
//!
//! Adding a language is deliberately small: describe it with a [`LangSpec`]
//! (comment markers, keyword/type/builtin word lists, string rules), wrap it in
//! a [`SpecHighlighter`], and register it in [`Registry::with_builtins`]. More
//! exotic languages can instead implement [`Highlighter`] directly.

pub mod languages;

use std::collections::HashSet;
use std::path::Path;

/// The classes of token the renderer knows how to color. Each maps to a color
/// in every [`crate::theme::Theme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Builtin,
    String,
    Char,
    Number,
    Comment,
    Operator,
    Punctuation,
    Preprocessor,
    Label,
    Register,
    Variable,
    Ident,
}

/// A highlighted span within a single line, as byte offsets `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

impl Token {
    pub fn new(start: usize, end: usize, kind: TokenKind) -> Self {
        Self { start, end, kind }
    }
}

/// The set of supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    PlainText,
    Rust,
    SqlAnsi,
    TSql,
    PgSql,
    TrinoSql,
    SnowflakeSql,
    Z80,
}

impl Language {
    /// Human-readable name shown in the status line / `:set ft`.
    pub fn name(&self) -> &'static str {
        match self {
            Language::PlainText => "text",
            Language::Rust => "rust",
            Language::SqlAnsi => "sql",
            Language::TSql => "tsql",
            Language::PgSql => "pgsql",
            Language::TrinoSql => "trino",
            Language::SnowflakeSql => "snowflake",
            Language::Z80 => "z80",
        }
    }

    /// Resolve a filetype name (as typed in `:set ft=<x>`) to a language.
    pub fn from_name(name: &str) -> Option<Language> {
        Some(match name.to_ascii_lowercase().as_str() {
            "text" | "txt" | "plain" => Language::PlainText,
            "rust" | "rs" => Language::Rust,
            "sql" | "ansi" => Language::SqlAnsi,
            "tsql" | "mssql" | "sqlserver" => Language::TSql,
            "pgsql" | "postgres" | "postgresql" | "psql" => Language::PgSql,
            "trino" | "presto" | "starburst" => Language::TrinoSql,
            "snowflake" | "snow" | "snowsql" => Language::SnowflakeSql,
            "z80" | "asm" | "assembly" => Language::Z80,
            _ => return None,
        })
    }
}

/// Autodetect a language from a file path (by extension) and, as a fallback,
/// the first line of content.
pub fn detect_language(path: Option<&Path>, first_line: &str) -> Language {
    if let Some(path) = path {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_ascii_lowercase().as_str() {
                "rs" => return Language::Rust,
                "tsql" => return Language::TSql,
                "pgsql" | "psql" => return Language::PgSql,
                "trino" | "trinosql" | "presto" => return Language::TrinoSql,
                "snow" | "snowsql" | "snowflake" => return Language::SnowflakeSql,
                "z80" | "asm" | "s" => return Language::Z80,
                "sql" => {
                    // Refine a generic .sql file by a leading dialect hint comment,
                    // e.g. `-- dialect: pgsql`.
                    return sniff_sql_dialect(first_line).unwrap_or(Language::SqlAnsi);
                }
                "txt" | "text" | "md" => return Language::PlainText,
                _ => {}
            }
        }
    }
    // Content-based fallbacks.
    let l = first_line.trim_start();
    if l.starts_with(";") && l.to_ascii_lowercase().contains("z80") {
        return Language::Z80;
    }
    if let Some(dialect) = sniff_sql_dialect(first_line) {
        return dialect;
    }
    Language::PlainText
}

fn sniff_sql_dialect(line: &str) -> Option<Language> {
    let l = line.to_ascii_lowercase();
    if !l.contains("dialect") && !l.contains("--") {
        return None;
    }
    if l.contains("tsql") || l.contains("t-sql") || l.contains("sqlserver") {
        Some(Language::TSql)
    } else if l.contains("pgsql") || l.contains("postgres") {
        Some(Language::PgSql)
    } else if l.contains("trino") || l.contains("presto") || l.contains("starburst") {
        Some(Language::TrinoSql)
    } else if l.contains("snowflake") {
        Some(Language::SnowflakeSql)
    } else {
        None
    }
}

/// The interface every highlighter implements.
///
/// Highlighting is line-based, but a single `bool` of carry-over state
/// (`in_block` — "this line begins inside a block comment") lets multi-line
/// constructs like `/* … */` span lines. Stateless callers use
/// [`highlight_line`](Highlighter::highlight_line).
pub trait Highlighter: Send + Sync {
    fn language(&self) -> Language;

    /// Highlight `line`, given whether it starts inside a block comment.
    /// Returns the tokens and whether the *next* line starts inside a block
    /// comment.
    fn highlight_line_stateful(&self, line: &str, in_block: bool) -> (Vec<Token>, bool);

    /// Convenience: highlight a standalone line (not inside a block comment).
    fn highlight_line(&self, line: &str) -> Vec<Token> {
        self.highlight_line_stateful(line, false).0
    }
}

/// A declarative description of a language's lexical surface.
#[derive(Clone)]
pub struct LangSpec {
    pub language: Language,
    pub line_comments: &'static [&'static str],
    pub block_comment: Option<(&'static str, &'static str)>,
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub builtins: &'static [&'static str],
    /// String delimiters (e.g. `"`, `'`, `` ` ``).
    pub string_delims: &'static [char],
    /// Whether keyword matching ignores case (true for SQL).
    pub case_insensitive: bool,
    /// Sigils that introduce a variable token, e.g. `@`, `$`, `:` (SQL/TSQL).
    pub var_sigils: &'static [char],
    /// Mark `ident(` as a function call.
    pub detect_calls: bool,
}

/// A [`Highlighter`] driven entirely by a [`LangSpec`], plus prebuilt lookup
/// sets for fast keyword classification.
pub struct SpecHighlighter {
    spec: LangSpec,
    keywords: HashSet<String>,
    types: HashSet<String>,
    builtins: HashSet<String>,
}

impl SpecHighlighter {
    pub fn new(spec: LangSpec) -> Self {
        let norm = |w: &&'static str| {
            if spec.case_insensitive {
                w.to_ascii_lowercase()
            } else {
                w.to_string()
            }
        };
        Self {
            keywords: spec.keywords.iter().map(norm).collect(),
            types: spec.types.iter().map(norm).collect(),
            builtins: spec.builtins.iter().map(norm).collect(),
            spec,
        }
    }

    /// Build a highlighter from a base spec plus extra word lists merged into
    /// the lookup sets. Used by SQL dialects to extend a shared ANSI core
    /// without duplicating `'static` arrays.
    pub fn from_parts(
        spec: LangSpec,
        extra_keywords: &[&str],
        extra_types: &[&str],
        extra_builtins: &[&str],
    ) -> Self {
        let ci = spec.case_insensitive;
        let norm = |w: &str| if ci { w.to_ascii_lowercase() } else { w.to_string() };
        let mut h = Self::new(spec);
        h.keywords.extend(extra_keywords.iter().map(|w| norm(w)));
        h.types.extend(extra_types.iter().map(|w| norm(w)));
        h.builtins.extend(extra_builtins.iter().map(|w| norm(w)));
        h
    }

    fn classify_word(&self, word: &str) -> Option<TokenKind> {
        let key = if self.spec.case_insensitive {
            word.to_ascii_lowercase()
        } else {
            word.to_string()
        };
        if self.keywords.contains(&key) {
            Some(TokenKind::Keyword)
        } else if self.types.contains(&key) {
            Some(TokenKind::Type)
        } else if self.builtins.contains(&key) {
            Some(TokenKind::Builtin)
        } else {
            None
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn is_operator_char(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '?'
    )
}
fn is_punct_char(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '.')
}

impl Highlighter for SpecHighlighter {
    fn language(&self) -> Language {
        self.spec.language
    }

    fn highlight_line_stateful(&self, line: &str, in_block: bool) -> (Vec<Token>, bool) {
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let end_byte = line.len();
        let mut tokens = Vec::new();
        let mut i = 0usize;

        // Byte offset of char index `k` (or end of line).
        let byte_at = |k: usize| chars.get(k).map(|&(b, _)| b).unwrap_or(end_byte);

        // If we begin inside a block comment, consume up to its closer (or the
        // whole line, staying in-block).
        if in_block {
            if let Some((_open, close)) = self.spec.block_comment {
                if let Some(p) = line.find(close) {
                    let end = p + close.len();
                    tokens.push(Token::new(0, end, TokenKind::Comment));
                    while i < chars.len() && chars[i].0 < end {
                        i += 1;
                    }
                } else {
                    tokens.push(Token::new(0, end_byte, TokenKind::Comment));
                    return (tokens, true);
                }
            }
        }

        while i < chars.len() {
            let (start_b, c) = chars[i];

            // Whitespace: skip.
            if c.is_whitespace() {
                i += 1;
                continue;
            }

            // Line comments.
            let rest = &line[start_b..];
            if let Some(lc) = self
                .spec
                .line_comments
                .iter()
                .find(|lc| rest.starts_with(**lc))
            {
                let _ = lc;
                tokens.push(Token::new(start_b, end_byte, TokenKind::Comment));
                break;
            }

            // Block comment. If the closer is missing, the comment runs to EOL
            // and the next line begins inside the block.
            if let Some((open, close)) = self.spec.block_comment {
                if let Some(after_open) = rest.strip_prefix(open) {
                    match after_open.find(close) {
                        Some(p) => {
                            let close_at = start_b + open.len() + p + close.len();
                            tokens.push(Token::new(start_b, close_at, TokenKind::Comment));
                            while i < chars.len() && chars[i].0 < close_at {
                                i += 1;
                            }
                            continue;
                        }
                        None => {
                            tokens.push(Token::new(start_b, end_byte, TokenKind::Comment));
                            return (tokens, true);
                        }
                    }
                }
            }

            // Strings.
            if self.spec.string_delims.contains(&c) {
                let delim = c;
                let mut j = i + 1;
                while j < chars.len() {
                    let cj = chars[j].1;
                    if cj == '\\' && j + 1 < chars.len() {
                        j += 2;
                        continue;
                    }
                    if cj == delim {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                let end_b = byte_at(j);
                tokens.push(Token::new(start_b, end_b, TokenKind::String));
                i = j;
                continue;
            }

            // Variables with sigils (@x, $x, :x).
            if self.spec.var_sigils.contains(&c) {
                let mut j = i + 1;
                // allow a second sigil (e.g. TSQL @@ROWCOUNT)
                if j < chars.len() && self.spec.var_sigils.contains(&chars[j].1) {
                    j += 1;
                }
                while j < chars.len() && is_ident_continue(chars[j].1) {
                    j += 1;
                }
                if j > i + 1 {
                    tokens.push(Token::new(start_b, byte_at(j), TokenKind::Variable));
                    i = j;
                    continue;
                }
            }

            // Numbers.
            if c.is_ascii_digit() {
                let mut j = i + 1;
                // hex / binary / octal prefixes
                if c == '0' && j < chars.len() && matches!(chars[j].1, 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                    j += 1;
                }
                while j < chars.len() {
                    let cj = chars[j].1;
                    if cj.is_ascii_alphanumeric() || cj == '_' || cj == '.' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(Token::new(start_b, byte_at(j), TokenKind::Number));
                i = j;
                continue;
            }

            // Identifiers / keywords.
            if is_ident_start(c) {
                let mut j = i + 1;
                while j < chars.len() && is_ident_continue(chars[j].1) {
                    j += 1;
                }
                let end_b = byte_at(j);
                let word = &line[start_b..end_b];
                let kind = if let Some(k) = self.classify_word(word) {
                    k
                } else if self.spec.detect_calls && next_nonspace_is(&chars, j, '(') {
                    TokenKind::Function
                } else {
                    TokenKind::Ident
                };
                // Only emit non-plain idents to keep token lists small.
                if kind != TokenKind::Ident {
                    tokens.push(Token::new(start_b, end_b, kind));
                }
                i = j;
                continue;
            }

            // Operators (runs) and punctuation (single).
            if is_operator_char(c) {
                let mut j = i + 1;
                while j < chars.len() && is_operator_char(chars[j].1) {
                    j += 1;
                }
                tokens.push(Token::new(start_b, byte_at(j), TokenKind::Operator));
                i = j;
                continue;
            }
            if is_punct_char(c) {
                tokens.push(Token::new(start_b, byte_at(i + 1), TokenKind::Punctuation));
                i += 1;
                continue;
            }

            // Anything else: advance one char.
            i += 1;
        }

        (tokens, false)
    }
}

fn next_nonspace_is(chars: &[(usize, char)], from: usize, target: char) -> bool {
    let mut k = from;
    while k < chars.len() && chars[k].1.is_whitespace() {
        k += 1;
    }
    chars.get(k).map(|&(_, c)| c == target).unwrap_or(false)
}

/// A registry mapping languages to their highlighters.
pub struct Registry {
    highlighters: Vec<Box<dyn Highlighter>>,
}

impl Registry {
    /// Build a registry populated with every built-in language.
    pub fn with_builtins() -> Self {
        let mut r = Registry {
            highlighters: Vec::new(),
        };
        for h in languages::builtin_highlighters() {
            r.highlighters.push(h);
        }
        r
    }

    /// Register an additional highlighter (e.g. from a plugin).
    pub fn register(&mut self, h: Box<dyn Highlighter>) {
        self.highlighters.push(h);
    }

    /// Look up the highlighter for a language, if present.
    pub fn get(&self, lang: Language) -> Option<&dyn Highlighter> {
        self.highlighters
            .iter()
            .find(|h| h.language() == lang)
            .map(|b| b.as_ref())
    }

    /// Highlight a line under `lang`, returning an empty vec for plain text or
    /// unknown languages.
    pub fn highlight(&self, lang: Language, line: &str) -> Vec<Token> {
        match self.get(lang) {
            Some(h) => h.highlight_line(line),
            None => Vec::new(),
        }
    }

    /// Stateful highlight carrying block-comment state across lines. Returns the
    /// tokens and whether the next line starts inside a block comment.
    pub fn highlight_stateful(&self, lang: Language, line: &str, in_block: bool) -> (Vec<Token>, bool) {
        match self.get(lang) {
            Some(h) => h.highlight_line_stateful(line, in_block),
            None => (Vec::new(), false),
        }
    }

    /// Compute whether the line at `row` begins inside a block comment, by
    /// folding state from the top of the buffer.
    pub fn block_state_at(&self, lang: Language, lines: &[String], row: usize) -> bool {
        let mut in_block = false;
        for line in lines.iter().take(row) {
            in_block = self.highlight_stateful(lang, line, in_block).1;
        }
        in_block
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_by_extension() {
        assert_eq!(
            detect_language(Some(&PathBuf::from("main.rs")), ""),
            Language::Rust
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("q.tsql")), ""),
            Language::TSql
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("q.pgsql")), ""),
            Language::PgSql
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("boot.z80")), ""),
            Language::Z80
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("q.sql")), ""),
            Language::SqlAnsi
        );
    }

    #[test]
    fn detect_sql_dialect_from_hint() {
        assert_eq!(
            detect_language(Some(&PathBuf::from("q.sql")), "-- dialect: trino"),
            Language::TrinoSql
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("q.sql")), "-- dialect: snowflake"),
            Language::SnowflakeSql
        );
    }

    #[test]
    fn language_name_roundtrip() {
        for lang in [
            Language::Rust,
            Language::SqlAnsi,
            Language::TSql,
            Language::PgSql,
            Language::TrinoSql,
            Language::SnowflakeSql,
            Language::Z80,
        ] {
            assert_eq!(Language::from_name(lang.name()), Some(lang));
        }
    }

    #[test]
    fn registry_has_all_languages() {
        let r = Registry::with_builtins();
        for lang in [
            Language::Rust,
            Language::SqlAnsi,
            Language::TSql,
            Language::PgSql,
            Language::TrinoSql,
            Language::SnowflakeSql,
            Language::Z80,
        ] {
            assert!(r.get(lang).is_some(), "missing highlighter for {lang:?}");
        }
    }

    #[test]
    fn block_comment_spans_lines() {
        let r = Registry::with_builtins();
        // Opening without a closer leaves the next line in-block.
        let (toks, in_block) = r.highlight_stateful(Language::Rust, "let x = 1; /* start", false);
        assert!(in_block);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment));

        // A fully-commented middle line stays in-block.
        let (mid, still) = r.highlight_stateful(Language::Rust, "still comment", true);
        assert!(still);
        assert_eq!(mid.len(), 1);
        assert_eq!(mid[0].kind, TokenKind::Comment);

        // The closer ends the block; code after it is highlighted again.
        let (end, done) = r.highlight_stateful(Language::Rust, "done */ let y = 2;", true);
        assert!(!done);
        assert!(end.iter().any(|t| t.kind == TokenKind::Comment));
        assert!(end.iter().any(|t| t.kind == TokenKind::Keyword)); // `let`
    }

    #[test]
    fn block_state_at_folds_from_top() {
        let r = Registry::with_builtins();
        let lines: Vec<String> = ["code /* open", "inside", "close */ code", "after"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(!r.block_state_at(Language::Rust, &lines, 0)); // line 0 not in block
        assert!(r.block_state_at(Language::Rust, &lines, 1)); // line 1 inside
        assert!(r.block_state_at(Language::Rust, &lines, 2)); // line 2 starts inside
        assert!(!r.block_state_at(Language::Rust, &lines, 3)); // line 3 after close
    }

    #[test]
    fn single_line_block_comment_still_works() {
        let r = Registry::with_builtins();
        let (toks, in_block) = r.highlight_stateful(Language::Rust, "a /* c */ let b", false);
        assert!(!in_block);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword));
    }

    #[test]
    fn generic_string_and_number_tokens() {
        let r = Registry::with_builtins();
        let toks = r.highlight(Language::Rust, r#"let x = "hi" + 42;"#);
        assert!(toks.iter().any(|t| t.kind == TokenKind::String));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Number));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // `let`
    }
}
