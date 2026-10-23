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
    Json,
    Python,
    Toml,
    JavaScript,
    Go,
    Shell,
    C,
    Java,
    Yaml,
    TypeScript,
    Kotlin,
    Ruby,
    Lua,
    CSharp,
    Swift,
    Php,
    Scala,
    Dart,
    R,
    Haskell,
    Zig,
    Elixir,
}

/// One language's metadata, driving `name`, `from_name`, `line_comment_token`,
/// and detection by file extension. Adding a language is one row here plus its
/// highlighter (and any special shebang/dialect sniffing below).
struct LangInfo {
    lang: Language,
    /// Canonical name (status line, `:set ft`).
    name: &'static str,
    /// Extra `:set ft=<x>` names besides `name`.
    aliases: &'static [&'static str],
    /// File extensions (lowercase, no dot) that select this language.
    extensions: &'static [&'static str],
    /// Line-comment marker, if any.
    comment: Option<&'static str>,
}

/// `.sql` is intentionally absent from `extensions` (it is dialect-sniffed).
const LANG_TABLE: &[LangInfo] = &[
    LangInfo { lang: Language::PlainText, name: "text", aliases: &["txt", "plain"], extensions: &["txt", "text", "md"], comment: None },
    LangInfo { lang: Language::Rust, name: "rust", aliases: &["rs"], extensions: &["rs"], comment: Some("//") },
    LangInfo { lang: Language::SqlAnsi, name: "sql", aliases: &["ansi"], extensions: &[], comment: Some("--") },
    LangInfo { lang: Language::TSql, name: "tsql", aliases: &["mssql", "sqlserver"], extensions: &["tsql"], comment: Some("--") },
    LangInfo { lang: Language::PgSql, name: "pgsql", aliases: &["postgres", "postgresql", "psql"], extensions: &["pgsql", "psql"], comment: Some("--") },
    LangInfo { lang: Language::TrinoSql, name: "trino", aliases: &["presto", "starburst"], extensions: &["trino", "trinosql", "presto"], comment: Some("--") },
    LangInfo { lang: Language::SnowflakeSql, name: "snowflake", aliases: &["snow", "snowsql"], extensions: &["snow", "snowsql", "snowflake"], comment: Some("--") },
    LangInfo { lang: Language::Z80, name: "z80", aliases: &["asm", "assembly"], extensions: &["z80", "asm", "s"], comment: Some(";") },
    LangInfo { lang: Language::Json, name: "json", aliases: &[], extensions: &["json"], comment: None },
    LangInfo { lang: Language::Python, name: "python", aliases: &["py"], extensions: &["py", "pyw"], comment: Some("#") },
    LangInfo { lang: Language::Toml, name: "toml", aliases: &[], extensions: &["toml"], comment: Some("#") },
    LangInfo { lang: Language::JavaScript, name: "javascript", aliases: &["js", "node"], extensions: &["js", "mjs", "cjs", "jsx"], comment: Some("//") },
    LangInfo { lang: Language::Go, name: "go", aliases: &["golang"], extensions: &["go"], comment: Some("//") },
    LangInfo { lang: Language::Shell, name: "shell", aliases: &["sh", "bash", "zsh"], extensions: &["sh", "bash", "zsh"], comment: Some("#") },
    LangInfo { lang: Language::C, name: "c", aliases: &["cpp", "c++", "cxx", "cc", "h", "hpp"], extensions: &["c", "h", "cpp", "cc", "cxx", "hpp", "hh"], comment: Some("//") },
    LangInfo { lang: Language::Java, name: "java", aliases: &[], extensions: &["java"], comment: Some("//") },
    LangInfo { lang: Language::Yaml, name: "yaml", aliases: &["yml"], extensions: &["yaml", "yml"], comment: Some("#") },
    LangInfo { lang: Language::TypeScript, name: "typescript", aliases: &["ts", "tsx"], extensions: &["ts", "tsx", "mts", "cts"], comment: Some("//") },
    LangInfo { lang: Language::Kotlin, name: "kotlin", aliases: &["kt", "kts"], extensions: &["kt", "kts"], comment: Some("//") },
    LangInfo { lang: Language::Ruby, name: "ruby", aliases: &["rb"], extensions: &["rb"], comment: Some("#") },
    LangInfo { lang: Language::Lua, name: "lua", aliases: &[], extensions: &["lua"], comment: Some("--") },
    LangInfo { lang: Language::CSharp, name: "csharp", aliases: &["cs", "c#"], extensions: &["cs"], comment: Some("//") },
    LangInfo { lang: Language::Swift, name: "swift", aliases: &[], extensions: &["swift"], comment: Some("//") },
    LangInfo { lang: Language::Php, name: "php", aliases: &[], extensions: &["php", "phtml"], comment: Some("//") },
    LangInfo { lang: Language::Scala, name: "scala", aliases: &["sc"], extensions: &["scala", "sc"], comment: Some("//") },
    LangInfo { lang: Language::Dart, name: "dart", aliases: &[], extensions: &["dart"], comment: Some("//") },
    LangInfo { lang: Language::R, name: "r", aliases: &["rlang"], extensions: &["r"], comment: Some("#") },
    LangInfo { lang: Language::Haskell, name: "haskell", aliases: &["hs"], extensions: &["hs"], comment: Some("--") },
    LangInfo { lang: Language::Zig, name: "zig", aliases: &[], extensions: &["zig"], comment: Some("//") },
    LangInfo { lang: Language::Elixir, name: "elixir", aliases: &["ex", "exs"], extensions: &["ex", "exs"], comment: Some("#") },
];

impl Language {
    /// Human-readable name shown in the status line / `:set ft`.
    pub fn name(&self) -> &'static str {
        LANG_TABLE
            .iter()
            .find(|e| e.lang == *self)
            .map(|e| e.name)
            .unwrap_or("text")
    }

    /// Resolve a filetype name (as typed in `:set ft=<x>`) to a language.
    pub fn from_name(name: &str) -> Option<Language> {
        let n = name.to_ascii_lowercase();
        LANG_TABLE
            .iter()
            .find(|e| e.name == n || e.aliases.contains(&n.as_str()))
            .map(|e| e.lang)
    }
}

/// Whether `foldmethod=syntax` should fold this language by indentation rather
/// than by brace blocks. True for indentation-structured languages that have no
/// `{`…`}` regions (Python, YAML) or use word-delimited blocks (Ruby, Lua, Shell),
/// where indentation is the reliable structural cue.
pub fn prefers_indent_folds(lang: Language) -> bool {
    matches!(
        lang,
        Language::Python
            | Language::Yaml
            | Language::Ruby
            | Language::Lua
            | Language::Shell
            | Language::Haskell
            | Language::Elixir
    )
}

/// All highlighter language names (for `:help` / `:set ft=`), excluding plain text.
pub fn language_names() -> Vec<&'static str> {
    LANG_TABLE
        .iter()
        .filter(|e| e.lang != Language::PlainText)
        .map(|e| e.name)
        .collect()
}

/// The primary line-comment marker for a language (for comment toggling).
pub fn line_comment_token(lang: Language) -> Option<&'static str> {
    LANG_TABLE
        .iter()
        .find(|e| e.lang == lang)
        .and_then(|e| e.comment)
}

/// Autodetect a language from a file path (by extension) and, as a fallback,
/// the first line of content.
pub fn detect_language(path: Option<&Path>, first_line: &str) -> Language {
    if let Some(path) = path {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if ext == "sql" {
                // Refine a generic .sql file by a leading dialect hint comment,
                // e.g. `-- dialect: pgsql`.
                return sniff_sql_dialect(first_line).unwrap_or(Language::SqlAnsi);
            }
            if let Some(e) = LANG_TABLE
                .iter()
                .find(|e| e.extensions.contains(&ext.as_str()))
            {
                return e.lang;
            }
        }
    }
    // Content-based fallbacks.
    let l = first_line.trim_start();
    if l.starts_with("#!") && l.to_ascii_lowercase().contains("python") {
        return Language::Python;
    }
    if l.starts_with("#!") && l.to_ascii_lowercase().contains("node") {
        return Language::JavaScript;
    }
    if l.starts_with("#!") {
        let low = l.to_ascii_lowercase();
        if low.contains("bash") || low.contains("zsh") || low.ends_with("sh") || low.contains("/sh")
        {
            return Language::Shell;
        }
    }
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

/// Carry-over state between lines: whether a line begins inside a multi-line
/// construct. A single value distinguishes block comments from each multi-line
/// string delimiter, so a language with both (e.g. JavaScript) tracks them
/// independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    /// Not inside any multi-line construct.
    #[default]
    Normal,
    /// Inside a block comment (`/* … */`).
    Block,
    /// Inside a multi-line string opened with `multiline_strings[idx]`.
    MultiStr(usize),
}

/// The interface every highlighter implements.
///
/// Highlighting is line-based, but a [`LineState`] of carry-over state lets
/// multi-line constructs (block comments, triple-quoted / template strings)
/// span lines. Stateless callers use [`highlight_line`](Highlighter::highlight_line).
pub trait Highlighter: Send + Sync {
    fn language(&self) -> Language;

    /// Highlight `line`, given the carry-over state it begins in. Returns the
    /// tokens and the state the *next* line begins in.
    fn highlight_line_stateful(&self, line: &str, state: LineState) -> (Vec<Token>, LineState);

    /// Convenience: highlight a standalone line (not inside any construct).
    fn highlight_line(&self, line: &str) -> Vec<Token> {
        self.highlight_line_stateful(line, LineState::Normal).0
    }

    /// Whether this language has any construct that spans lines (block comments
    /// or multi-line strings). When false, per-line state never carries, so the
    /// renderer can skip folding state from the top of the buffer.
    fn has_multiline(&self) -> bool {
        false
    }

    /// A cheap short-circuit for advancing carry-over state without tokenizing.
    /// Given the state a line *begins* in, returns `Some(end_state)` when the
    /// result is certain without a full scan, or `None` when the caller must run
    /// [`highlight_line_stateful`](Highlighter::highlight_line_stateful). The
    /// default is always `None` (no short-circuit).
    fn fast_block_state(&self, line: &str, state: LineState) -> Option<LineState> {
        let _ = (line, state);
        None
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
    /// Multi-line string delimiters (e.g. Python's `"""` / `'''`). A string opened
    /// with one of these runs until the next matching delimiter, across lines.
    pub multiline_strings: &'static [&'static str],
    /// A character that, as the first non-blank on a line, introduces a
    /// preprocessor directive (e.g. C's `#include`). The `#` plus the directive
    /// word is highlighted as [`TokenKind::Preprocessor`].
    pub preprocessor: Option<char>,
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

/// Byte offset within `s` of the first *unescaped* occurrence of `delim`.
/// Backslash escapes are honored only for single-character delimiters (e.g. a
/// JavaScript template-literal backtick); multi-character delimiters like `"""`
/// cannot be meaningfully escaped, so they match literally.
fn find_unescaped(s: &str, delim: &str) -> Option<usize> {
    if delim.len() != 1 {
        return s.find(delim);
    }
    let dch = delim.as_bytes()[0] as char;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == dch {
            return Some(i);
        }
    }
    None
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
    fn has_multiline(&self) -> bool {
        self.spec.block_comment.is_some() || !self.spec.multiline_strings.is_empty()
    }

    fn fast_block_state(&self, line: &str, state: LineState) -> Option<LineState> {
        // Only short-circuit from Normal. The next line can leave Normal only if
        // a block comment or a multi-line string *opens* here, and either requires
        // its delimiter substring to be present. When none appear, no construct can
        // open, so the next line is still Normal — no tokenization needed. (A line
        // that does contain such a delimiter falls through to the full scan, which
        // correctly handles delimiters sitting inside strings or line comments.)
        if state != LineState::Normal {
            return None;
        }
        let has_delim = self
            .spec
            .block_comment
            .is_some_and(|(open, _)| line.contains(open))
            || self
                .spec
                .multiline_strings
                .iter()
                .any(|d| line.contains(d));
        if has_delim {
            None
        } else {
            Some(LineState::Normal)
        }
    }

    fn language(&self) -> Language {
        self.spec.language
    }

    fn highlight_line_stateful(&self, line: &str, state: LineState) -> (Vec<Token>, LineState) {
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let end_byte = line.len();
        let mut tokens = Vec::new();
        let mut i = 0usize;

        // Byte offset of char index `k` (or end of line).
        let byte_at = |k: usize| chars.get(k).map(|&(b, _)| b).unwrap_or(end_byte);

        // If we begin inside a block comment, consume up to its closer (or the
        // whole line, staying in-block).
        match state {
            LineState::Normal => {}
            LineState::Block => {
                if let Some((_open, close)) = self.spec.block_comment {
                    if let Some(p) = line.find(close) {
                        let end = p + close.len();
                        tokens.push(Token::new(0, end, TokenKind::Comment));
                        while i < chars.len() && chars[i].0 < end {
                            i += 1;
                        }
                    } else {
                        tokens.push(Token::new(0, end_byte, TokenKind::Comment));
                        return (tokens, LineState::Block);
                    }
                }
            }
            LineState::MultiStr(idx) => {
                // Resuming a multi-line string: run to its own closing delimiter,
                // or the whole line (staying in-string). An out-of-range idx (which
                // should not happen for the current language) falls through as
                // Normal rather than sticking in-string forever.
                if let Some(&delim) = self.spec.multiline_strings.get(idx) {
                    match find_unescaped(line, delim) {
                        Some(p) => {
                            let end = p + delim.len();
                            tokens.push(Token::new(0, end, TokenKind::String));
                            while i < chars.len() && chars[i].0 < end {
                                i += 1;
                            }
                        }
                        None => {
                            tokens.push(Token::new(0, end_byte, TokenKind::String));
                            return (tokens, LineState::MultiStr(idx));
                        }
                    }
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

            // Preprocessor directive: the `#` plus its directive word, when `#` is
            // the first non-blank on the line (e.g. C's `#include`).
            if let Some(pp) = self.spec.preprocessor {
                if c == pp && line[..start_b].trim().is_empty() {
                    let mut j = i + 1;
                    while j < chars.len() && is_ident_continue(chars[j].1) {
                        j += 1;
                    }
                    tokens.push(Token::new(start_b, byte_at(j), TokenKind::Preprocessor));
                    i = j;
                    continue;
                }
            }

            let rest = &line[start_b..];

            // Block comment first: its opener can share a prefix with a line
            // comment (e.g. Lua's `--[[` vs `--`), so it must win when both match.
            // For every language whose block/line tokens don't overlap the order is
            // irrelevant. If the closer is missing, the comment runs to EOL and the
            // next line begins inside the block.
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
                            return (tokens, LineState::Block);
                        }
                    }
                }
            }

            // Line comments.
            if self
                .spec
                .line_comments
                .iter()
                .any(|lc| rest.starts_with(*lc))
            {
                tokens.push(Token::new(start_b, end_byte, TokenKind::Comment));
                break;
            }

            // Multi-line strings (e.g. Python triple quotes, JS template literals).
            // Checked before the single-char string rule so `"""` is not read as
            // an empty `""`.
            if let Some(idx) = self
                .spec
                .multiline_strings
                .iter()
                .position(|d| rest.starts_with(*d))
            {
                let open = self.spec.multiline_strings[idx];
                let after = start_b + open.len();
                match find_unescaped(&line[after..], open) {
                    Some(p) => {
                        let close_at = after + p + open.len();
                        tokens.push(Token::new(start_b, close_at, TokenKind::String));
                        while i < chars.len() && chars[i].0 < close_at {
                            i += 1;
                        }
                        continue;
                    }
                    None => {
                        tokens.push(Token::new(start_b, end_byte, TokenKind::String));
                        return (tokens, LineState::MultiStr(idx));
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

        (tokens, LineState::Normal)
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

    /// Stateful highlight carrying multi-line state across lines. Returns the
    /// tokens and the state the next line begins in.
    pub fn highlight_stateful(
        &self,
        lang: Language,
        line: &str,
        state: LineState,
    ) -> (Vec<Token>, LineState) {
        match self.get(lang) {
            Some(h) => h.highlight_line_stateful(line, state),
            None => (Vec::new(), LineState::Normal),
        }
    }

    /// Advance only the carry-over state across `line`, without producing tokens.
    /// Uses the cheap [`Highlighter::fast_block_state`] short-circuit where possible
    /// (most lines cannot change state), falling back to a full scan otherwise — so
    /// skipping hidden lines (e.g. inside a closed fold) avoids tokenizing them.
    pub fn advance_block_state(&self, lang: Language, line: &str, state: LineState) -> LineState {
        match self.get(lang) {
            Some(h) => h
                .fast_block_state(line, state)
                .unwrap_or_else(|| h.highlight_line_stateful(line, state).1),
            None => LineState::Normal,
        }
    }

    /// Compute the carry-over state the line at `row` begins in, by folding state
    /// from the top of the buffer.
    /// Fold ranges `[start, end]` for `foldmethod=syntax`: brace-delimited blocks
    /// (`{` … `}`) spanning more than one line, with braces inside strings, chars
    /// and comments ignored via the tokenizer. Nesting produces nested folds.
    /// Languages without braces (or highlighting) simply yield no folds.
    pub fn syntax_fold_ranges(&self, lang: Language, lines: &[String]) -> Vec<(usize, usize)> {
        let mut state = LineState::Normal;
        let mut stack: Vec<usize> = Vec::new();
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let (tokens, next) = self.highlight_stateful(lang, line, state);
            for (b, ch) in line.char_indices() {
                if ch != '{' && ch != '}' {
                    continue;
                }
                // Skip braces that fall inside a string/char/comment token.
                let masked = tokens.iter().any(|t| {
                    b >= t.start
                        && b < t.end
                        && matches!(
                            t.kind,
                            TokenKind::String | TokenKind::Char | TokenKind::Comment
                        )
                });
                if masked {
                    continue;
                }
                if ch == '{' {
                    stack.push(i);
                } else if let Some(start) = stack.pop() {
                    if i > start {
                        out.push((start, i));
                    }
                }
            }
            state = next;
        }
        out
    }

    pub fn block_state_at(&self, lang: Language, lines: &[String], row: usize) -> LineState {
        // Languages without any line-spanning construct never carry state, so skip
        // folding from the top of the buffer entirely.
        let h = match self.get(lang) {
            Some(h) if h.has_multiline() => h,
            _ => return LineState::Normal,
        };
        let mut state = LineState::Normal;
        for line in lines.iter().take(row) {
            // Skip full tokenization on lines that provably cannot change state
            // (the common case: most lines carry no comment/string delimiters).
            state = match h.fast_block_state(line, state) {
                Some(s) => s,
                None => h.highlight_line_stateful(line, state).1,
            };
        }
        state
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
    fn every_language_variant_is_in_the_table() {
        // The exhaustive match is a tripwire: adding a Language variant fails to
        // compile here until it is listed, and the assert then requires a table row
        // (restoring the guarantee the old exhaustive match arms gave).
        let all = [
            Language::PlainText, Language::Rust, Language::SqlAnsi, Language::TSql,
            Language::PgSql, Language::TrinoSql, Language::SnowflakeSql, Language::Z80,
            Language::Json, Language::Python, Language::Toml, Language::JavaScript,
            Language::Go, Language::Shell, Language::C, Language::Java, Language::Yaml,
            Language::TypeScript, Language::Kotlin, Language::Ruby, Language::Lua,
            Language::CSharp, Language::Swift, Language::Php, Language::Scala,
            Language::Dart, Language::R, Language::Haskell, Language::Zig,
            Language::Elixir,
        ];
        for lang in all {
            match lang {
                Language::PlainText | Language::Rust | Language::SqlAnsi | Language::TSql
                | Language::PgSql | Language::TrinoSql | Language::SnowflakeSql | Language::Z80
                | Language::Json | Language::Python | Language::Toml | Language::JavaScript
                | Language::Go | Language::Shell | Language::C | Language::Java
                | Language::Yaml | Language::TypeScript | Language::Kotlin | Language::Ruby
                | Language::Lua | Language::CSharp | Language::Swift | Language::Php
                | Language::Scala | Language::Dart | Language::R | Language::Haskell
                | Language::Zig | Language::Elixir => {}
            }
            assert!(
                LANG_TABLE.iter().any(|e| e.lang == lang),
                "{lang:?} missing from LANG_TABLE"
            );
        }
    }

    #[test]
    fn lang_table_names_round_trip() {
        // Every language's canonical name resolves back to itself, and each
        // extension and alias resolves to its language.
        for e in LANG_TABLE {
            assert_eq!(e.lang.name(), e.name);
            assert_eq!(Language::from_name(e.name), Some(e.lang), "name {}", e.name);
            for a in e.aliases {
                assert_eq!(Language::from_name(a), Some(e.lang), "alias {a}");
            }
            for x in e.extensions {
                assert_eq!(
                    detect_language(Some(&PathBuf::from(format!("f.{x}"))), ""),
                    e.lang,
                    "ext {x}"
                );
            }
        }
    }

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
        assert_eq!(
            detect_language(Some(&PathBuf::from("pkg.json")), ""),
            Language::Json
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("app.py")), ""),
            Language::Python
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("Cargo.toml")), ""),
            Language::Toml
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("app.js")), ""),
            Language::JavaScript
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("main.go")), ""),
            Language::Go
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("run.sh")), ""),
            Language::Shell
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("main.c")), ""),
            Language::C
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("app.hpp")), ""),
            Language::C
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("App.java")), ""),
            Language::Java
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("config.yml")), ""),
            Language::Yaml
        );
        assert_eq!(
            detect_language(Some(&PathBuf::from("app.ts")), ""),
            Language::TypeScript
        );
    }

    #[test]
    fn detect_shell_by_shebang() {
        assert_eq!(detect_language(None, "#!/bin/bash"), Language::Shell);
        assert_eq!(detect_language(None, "#!/usr/bin/env sh"), Language::Shell);
    }

    #[test]
    fn detect_python_by_shebang() {
        assert_eq!(
            detect_language(None, "#!/usr/bin/env python3"),
            Language::Python
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
        let (toks, in_block) =
            r.highlight_stateful(Language::Rust, "let x = 1; /* start", LineState::Normal);
        assert_eq!(in_block, LineState::Block);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment));

        // A fully-commented middle line stays in-block.
        let (mid, still) = r.highlight_stateful(Language::Rust, "still comment", LineState::Block);
        assert_eq!(still, LineState::Block);
        assert_eq!(mid.len(), 1);
        assert_eq!(mid[0].kind, TokenKind::Comment);

        // The closer ends the block; code after it is highlighted again.
        let (end, done) =
            r.highlight_stateful(Language::Rust, "done */ let y = 2;", LineState::Block);
        assert_eq!(done, LineState::Normal);
        assert!(end.iter().any(|t| t.kind == TokenKind::Comment));
        assert!(end.iter().any(|t| t.kind == TokenKind::Keyword)); // `let`
    }

    #[test]
    fn has_multiline_flags_languages_with_spanning_constructs() {
        let r = Registry::with_builtins();
        assert!(r.get(Language::Rust).unwrap().has_multiline()); // block comments
        assert!(r.get(Language::Python).unwrap().has_multiline()); // triple strings
        assert!(!r.get(Language::Json).unwrap().has_multiline());
        assert!(!r.get(Language::Toml).unwrap().has_multiline());
        // No-multiline languages still report a correct (Normal) state.
        let lines = vec!["{".to_string(), "  \"a\": 1".to_string(), "}".to_string()];
        assert_eq!(r.block_state_at(Language::Json, &lines, 2), LineState::Normal);
    }

    #[test]
    fn python_triple_string_spans_lines() {
        let r = Registry::with_builtins();
        // Opening line: the docstring starts and does not close.
        let (t0, in_s) = r.highlight_stateful(Language::Python, "x = \"\"\"start", LineState::Normal);
        assert_eq!(in_s, LineState::MultiStr(0)); // """ is delimiter 0
        assert!(t0.iter().any(|t| t.kind == TokenKind::String));
        // Middle line: entirely string, still open.
        let (_t1, still) =
            r.highlight_stateful(Language::Python, "middle text", LineState::MultiStr(0));
        assert_eq!(still, LineState::MultiStr(0));
        // Closing line: ends the string.
        let (_t2, done) =
            r.highlight_stateful(Language::Python, "end\"\"\" + y", LineState::MultiStr(0));
        assert_eq!(done, LineState::Normal);
    }

    #[test]
    fn python_single_quote_triple_string_uses_second_delimiter() {
        let r = Registry::with_builtins();
        // Opening with ''' carries the second delimiter (index 1), so a stray
        // """ inside does not close it.
        let (_t, s) = r.highlight_stateful(Language::Python, "x = '''start", LineState::Normal);
        assert_eq!(s, LineState::MultiStr(1));
        let (_t2, s2) = r.highlight_stateful(Language::Python, "has \"\"\" inside", s);
        assert_eq!(s2, LineState::MultiStr(1)); // """ must not close a ''' block
        let (_t3, s3) = r.highlight_stateful(Language::Python, "end''' x", s2);
        assert_eq!(s3, LineState::Normal);
    }

    #[test]
    fn javascript_template_literal_escaped_backtick_does_not_close() {
        let r = Registry::with_builtins();
        // The escaped backtick must not end the template literal, so the state
        // stays in-string to the real (unescaped) closer on the next line.
        let (_t, s) = r.highlight_stateful(Language::JavaScript, "x = `a\\`b", LineState::Normal);
        assert_eq!(s, LineState::MultiStr(0));
        let (_t2, s2) = r.highlight_stateful(Language::JavaScript, "done`;", s);
        assert_eq!(s2, LineState::Normal);
    }

    #[test]
    fn javascript_template_literal_and_block_comment_track_independently() {
        let r = Registry::with_builtins();
        // A template literal opens and spans into the next line.
        let (_t, s) = r.highlight_stateful(Language::JavaScript, "const x = `line1", LineState::Normal);
        assert_eq!(s, LineState::MultiStr(0));
        let (_t2, s2) = r.highlight_stateful(Language::JavaScript, "still string`;", s);
        assert_eq!(s2, LineState::Normal);
        // A block comment is tracked separately from template-literal state.
        let (_c, cs) = r.highlight_stateful(Language::JavaScript, "a /* open", LineState::Normal);
        assert_eq!(cs, LineState::Block);
    }

    #[test]
    fn fast_block_state_matches_full_scan() {
        let r = Registry::with_builtins();
        let h = r.get(Language::Rust).unwrap();
        // Tricky lines: delimiters inside strings/line comments must NOT be
        // short-circuited into a wrong state. The fast path either agrees with
        // the full tokenizer or defers to it (returning None).
        let lines = [
            "let x = 1;",            // plain: fast path -> Normal
            "let s = \"a /* b\";",   // "/*" inside a string literal
            "// a /* comment */",    // "/*" inside a line comment
            "code /* open",          // genuinely opens a block comment
            "still inside",          // inside the block (begins in Block)
            "close */ done",         // closes the block
            "fn f() {}",             // plain again
        ];
        let mut state = LineState::Normal;
        for line in lines {
            let full = h.highlight_line_stateful(line, state).1;
            if let Some(fast) = h.fast_block_state(line, state) {
                assert_eq!(fast, full, "fast path disagreed on {line:?}");
            }
            state = full;
        }
        // Plain lines with no delimiters always take the fast path.
        assert_eq!(h.fast_block_state("let x = 1;", LineState::Normal), Some(LineState::Normal));
        // A line opening a comment defers to the full scan.
        assert_eq!(h.fast_block_state("a /* b", LineState::Normal), None);
        // The fast path never fires once inside a block.
        assert_eq!(h.fast_block_state("anything", LineState::Block), None);
    }

    #[test]
    fn advance_block_state_matches_full_scan() {
        let r = Registry::with_builtins();
        let lines = [
            "let x = 1;",
            "code /* open",   // enters a block comment
            "still inside",   // stays in the block
            "close */ done",  // leaves the block
            "fn f() {}",
        ];
        let mut fast = LineState::Normal;
        let mut full = LineState::Normal;
        for line in lines {
            fast = r.advance_block_state(Language::Rust, line, fast);
            full = r.highlight_stateful(Language::Rust, line, full).1;
            assert_eq!(fast, full, "state diverged on {line:?}");
        }
    }

    #[test]
    fn block_state_at_folds_from_top() {
        let r = Registry::with_builtins();
        let lines: Vec<String> = ["code /* open", "inside", "close */ code", "after"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(r.block_state_at(Language::Rust, &lines, 0), LineState::Normal); // line 0
        assert_eq!(r.block_state_at(Language::Rust, &lines, 1), LineState::Block); // inside
        assert_eq!(r.block_state_at(Language::Rust, &lines, 2), LineState::Block); // inside
        assert_eq!(r.block_state_at(Language::Rust, &lines, 3), LineState::Normal); // after close
    }

    #[test]
    fn syntax_folds_brace_blocks_and_nest() {
        let r = Registry::with_builtins();
        let lines: Vec<String> = ["fn f() {", "    if a {", "        b();", "    }", "}"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got = r.syntax_fold_ranges(Language::Rust, &lines);
        assert!(got.contains(&(0, 4))); // outer function body
        assert!(got.contains(&(1, 3))); // inner if block
    }

    #[test]
    fn syntax_folds_ignore_braces_in_strings_and_comments() {
        let r = Registry::with_builtins();
        let lines: Vec<String> = [
            "fn f() {",
            "    let s = \"a { b }\";",
            "    // stray } and {",
            "}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // Only the real braces on lines 0 and 3 form a fold.
        assert_eq!(r.syntax_fold_ranges(Language::Rust, &lines), vec![(0, 3)]);
    }

    #[test]
    fn syntax_folds_mask_braces_in_strings_with_utf8() {
        let r = Registry::with_builtins();
        // The braces inside the string sit at shifted byte offsets (é is 2 bytes)
        // and must be masked; only the real fn-body braces form a fold.
        let lines: Vec<String> = ["let s = \"café { x }\";", "fn f() {", "}"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(r.syntax_fold_ranges(Language::Rust, &lines), vec![(1, 2)]);
    }

    #[test]
    fn syntax_folds_mask_brace_in_continued_block_comment() {
        let r = Registry::with_builtins();
        // The `}` on line 1 is inside a carried-over block comment and must be
        // ignored; the real block is lines 2..=3.
        let lines: Vec<String> = ["/* open", "} still comment", "*/ fn f() {", "}"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(r.syntax_fold_ranges(Language::Rust, &lines), vec![(2, 3)]);
    }

    #[test]
    fn syntax_folds_empty_without_braces() {
        let r = Registry::with_builtins();
        let lines: Vec<String> = ["x = 1", "y = 2"].iter().map(|s| s.to_string()).collect();
        assert!(r.syntax_fold_ranges(Language::Rust, &lines).is_empty());
    }

    #[test]
    fn single_line_block_comment_still_works() {
        let r = Registry::with_builtins();
        let (toks, in_block) =
            r.highlight_stateful(Language::Rust, "a /* c */ let b", LineState::Normal);
        assert_eq!(in_block, LineState::Normal);
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
