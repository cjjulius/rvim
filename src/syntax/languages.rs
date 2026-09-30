//! Built-in language definitions.
//!
//! Most languages are expressed declaratively as a [`LangSpec`]. Z80 assembly
//! has enough idiosyncrasies (labels, `$`/`%` number bases, registers) that it
//! gets a hand-written [`Highlighter`].

use super::{
    Highlighter, LangSpec, Language, SpecHighlighter, Token, TokenKind,
};

/// Construct every built-in highlighter.
pub fn builtin_highlighters() -> Vec<Box<dyn Highlighter>> {
    vec![
        Box::new(SpecHighlighter::new(rust_spec())),
        Box::new(SpecHighlighter::new(sql_spec(Language::SqlAnsi, &[]))),
        Box::new(SpecHighlighter::from_parts(
            sql_spec(Language::TSql, SIG_AT),
            TSQL_KEYWORDS,
            TSQL_TYPES,
            TSQL_BUILTINS,
        )),
        Box::new(SpecHighlighter::from_parts(
            sql_spec(Language::PgSql, SIG_COLON),
            PGSQL_KEYWORDS,
            PGSQL_TYPES,
            PGSQL_BUILTINS,
        )),
        Box::new(SpecHighlighter::from_parts(
            sql_spec(Language::TrinoSql, &[]),
            TRINO_KEYWORDS,
            TRINO_TYPES,
            TRINO_BUILTINS,
        )),
        Box::new(SpecHighlighter::from_parts(
            sql_spec(Language::SnowflakeSql, SIG_DOLLAR),
            SNOWFLAKE_KEYWORDS,
            SNOWFLAKE_TYPES,
            SNOWFLAKE_BUILTINS,
        )),
        Box::new(Z80Highlighter::new()),
    ]
}

// ---- Rust ----------------------------------------------------------------

fn rust_spec() -> LangSpec {
    LangSpec {
        language: Language::Rust,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: RUST_KEYWORDS,
        types: RUST_TYPES,
        builtins: RUST_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
];
const RUST_TYPES: &[&str] = &[
    "bool", "char", "str", "String", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16",
    "u32", "u64", "u128", "usize", "f32", "f64", "Vec", "Option", "Result", "Box", "Rc", "Arc",
    "HashMap", "HashSet", "BTreeMap", "Cell", "RefCell", "Cow",
];
const RUST_BUILTINS: &[&str] = &[
    "println", "print", "eprintln", "eprint", "format", "vec", "panic", "assert", "assert_eq",
    "assert_ne", "write", "writeln", "unwrap", "expect", "Some", "None", "Ok", "Err", "todo",
    "unimplemented", "dbg", "matches", "into", "from", "clone",
];

// ---- SQL (shared ANSI core + per-dialect extensions) ---------------------

const SIG_AT: &[char] = &['@'];
const SIG_COLON: &[char] = &[':'];
const SIG_DOLLAR: &[char] = &['$'];

fn sql_spec(language: Language, var_sigils: &'static [char]) -> LangSpec {
    LangSpec {
        language,
        line_comments: &["--"],
        block_comment: Some(("/*", "*/")),
        keywords: SQL_KEYWORDS,
        types: SQL_TYPES,
        builtins: SQL_BUILTINS,
        string_delims: &['\'', '"'],
        case_insensitive: true,
        var_sigils,
        detect_calls: true,
    }
}

const SQL_KEYWORDS: &[&str] = &[
    "select", "from", "where", "and", "or", "not", "null", "insert", "into", "values", "update",
    "set", "delete", "create", "table", "view", "index", "drop", "alter", "add", "column",
    "primary", "key", "foreign", "references", "join", "inner", "left", "right", "full", "outer",
    "cross", "on", "group", "by", "order", "having", "limit", "offset", "distinct", "as", "union",
    "all", "except", "intersect", "case", "when", "then", "else", "end", "is", "in", "between",
    "like", "exists", "with", "recursive", "asc", "desc", "using", "natural", "default",
    "constraint", "unique", "check", "grant", "revoke", "commit", "rollback", "begin",
    "transaction", "if", "cast", "over", "partition", "and", "row", "rows", "range",
];
const SQL_TYPES: &[&str] = &[
    "int", "integer", "bigint", "smallint", "tinyint", "decimal", "numeric", "float", "real",
    "double", "char", "varchar", "text", "date", "time", "timestamp", "boolean", "bool", "blob",
    "clob",
];
const SQL_BUILTINS: &[&str] = &[
    "count", "sum", "avg", "min", "max", "coalesce", "nullif", "row_number", "rank", "dense_rank",
    "lead", "lag", "upper", "lower", "trim", "substring", "length", "abs", "round", "now",
    "current_date", "current_timestamp",
];

const TSQL_KEYWORDS: &[&str] = &[
    "declare", "procedure", "proc", "function", "returns", "return", "exec", "execute", "try",
    "catch", "throw", "merge", "output", "top", "identity", "nocount", "go", "pivot", "unpivot",
    "apply", "while", "break", "continue", "begin", "end", "print", "waitfor",
];
const TSQL_TYPES: &[&str] = &[
    "datetime", "datetime2", "nvarchar", "nchar", "money", "bit", "uniqueidentifier", "xml",
    "smalldatetime", "varbinary",
];
const TSQL_BUILTINS: &[&str] = &[
    "isnull", "getdate", "len", "charindex", "dateadd", "datediff", "datepart", "convert",
    "newid", "scope_identity", "object_id", "try_convert",
];

const PGSQL_KEYWORDS: &[&str] = &[
    "returning", "ilike", "lateral", "do", "language", "plpgsql", "replace", "unnest",
    "tablesample", "window", "filter", "conflict", "nothing", "values", "generated", "always",
];
const PGSQL_TYPES: &[&str] = &[
    "serial", "bigserial", "jsonb", "json", "uuid", "bytea", "inet", "cidr", "interval", "array",
    "money", "timestamptz",
];
const PGSQL_BUILTINS: &[&str] = &[
    "generate_series", "array_agg", "string_agg", "jsonb_build_object", "to_char", "to_timestamp",
    "coalesce", "now", "date_trunc", "regexp_replace",
];

const TRINO_KEYWORDS: &[&str] = &[
    "unnest", "ordinality", "lateral", "prepare", "describe", "analyze", "explain", "reset",
    "show", "catalogs", "schemas", "tables",
];
const TRINO_TYPES: &[&str] = &[
    "varbinary", "row", "map", "array", "json", "hyperloglog", "qdigest", "tdigest", "ipaddress",
    "uuid",
];
const TRINO_BUILTINS: &[&str] = &[
    "approx_distinct", "arbitrary", "array_agg", "cardinality", "contains", "element_at",
    "map_agg", "regexp_like", "split", "date_trunc", "from_unixtime", "to_unixtime", "try",
];

const SNOWFLAKE_KEYWORDS: &[&str] = &[
    "lateral", "flatten", "qualify", "pivot", "unpivot", "sample", "clone", "stream", "task",
    "warehouse", "copy", "stage", "put", "get", "merge",
];
const SNOWFLAKE_TYPES: &[&str] = &[
    "variant", "object", "array", "number", "string", "timestamp_ntz", "timestamp_tz",
    "timestamp_ltz", "geography", "geometry",
];
const SNOWFLAKE_BUILTINS: &[&str] = &[
    "flatten", "parse_json", "to_variant", "object_construct", "array_construct", "listagg",
    "iff", "dateadd", "datediff", "try_cast", "to_json", "get_path",
];

// ---- Z80 assembly (custom) ----------------------------------------------

/// A hand-written highlighter for Z80 assembly.
pub struct Z80Highlighter {
    mnemonics: std::collections::HashSet<&'static str>,
    registers: std::collections::HashSet<&'static str>,
    directives: std::collections::HashSet<&'static str>,
}

const Z80_MNEMONICS: &[&str] = &[
    "ld", "push", "pop", "add", "adc", "sub", "sbc", "and", "or", "xor", "cp", "inc", "dec",
    "rlca", "rrca", "rla", "rra", "daa", "cpl", "scf", "ccf", "halt", "nop", "di", "ei", "jp",
    "jr", "call", "ret", "reti", "retn", "rst", "djnz", "ex", "exx", "ldi", "ldir", "ldd",
    "lddr", "cpi", "cpir", "cpd", "cpdr", "in", "out", "ini", "inir", "ind", "indr", "outi",
    "otir", "outd", "otdr", "bit", "set", "res", "rlc", "rrc", "rl", "rr", "sla", "sra", "sll",
    "srl", "rld", "rrd", "neg", "im",
];
const Z80_REGISTERS: &[&str] = &[
    "a", "b", "c", "d", "e", "h", "l", "f", "i", "r", "af", "bc", "de", "hl", "sp", "pc", "ix",
    "iy", "ixh", "ixl", "iyh", "iyl", "nz", "z", "nc", "po", "pe", "m",
];
const Z80_DIRECTIVES: &[&str] = &[
    "org", "db", "dw", "ds", "defb", "defw", "defs", "defm", "equ", "end", "include", "incbin",
    "macro", "endm", "if", "endif", "align", "assert",
];

impl Z80Highlighter {
    pub fn new() -> Self {
        Self {
            mnemonics: Z80_MNEMONICS.iter().copied().collect(),
            registers: Z80_REGISTERS.iter().copied().collect(),
            directives: Z80_DIRECTIVES.iter().copied().collect(),
        }
    }
}

impl Default for Z80Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for Z80Highlighter {
    fn language(&self) -> Language {
        Language::Z80
    }

    fn highlight_line_stateful(&self, line: &str, _in_block: bool) -> (Vec<Token>, bool) {
        (self.highlight_line_impl(line), false)
    }
}

impl Z80Highlighter {
    fn highlight_line_impl(&self, line: &str) -> Vec<Token> {
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let end_byte = line.len();
        let byte_at = |k: usize| chars.get(k).map(|&(b, _)| b).unwrap_or(end_byte);
        let mut tokens = Vec::new();
        let mut i = 0usize;
        let mut seen_word = false; // have we passed the leading label/mnemonic slot?

        while i < chars.len() {
            let (sb, c) = chars[i];

            if c.is_whitespace() {
                i += 1;
                continue;
            }

            // Comments: ';' to end of line.
            if c == ';' {
                tokens.push(Token::new(sb, end_byte, TokenKind::Comment));
                break;
            }

            // Strings and char literals.
            if c == '"' || c == '\'' {
                let delim = c;
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j].1 == '\\' && j + 1 < chars.len() {
                        j += 2;
                        continue;
                    }
                    if chars[j].1 == delim {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                tokens.push(Token::new(sb, byte_at(j), TokenKind::String));
                i = j;
                seen_word = true;
                continue;
            }

            // Directive with a leading dot: `.org`, `.db`.
            if c == '.' && i + 1 < chars.len() && chars[i + 1].1.is_alphabetic() {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                    j += 1;
                }
                tokens.push(Token::new(sb, byte_at(j), TokenKind::Preprocessor));
                i = j;
                seen_word = true;
                continue;
            }

            // Numbers: $hex, %binary, decimal, 0x, and h/b/d/o suffixes.
            if c == '$' && i + 1 < chars.len() && chars[i + 1].1.is_ascii_hexdigit() {
                let mut j = i + 1;
                while j < chars.len() && chars[j].1.is_ascii_hexdigit() {
                    j += 1;
                }
                tokens.push(Token::new(sb, byte_at(j), TokenKind::Number));
                i = j;
                seen_word = true;
                continue;
            }
            if c == '%' && i + 1 < chars.len() && matches!(chars[i + 1].1, '0' | '1') {
                let mut j = i + 1;
                while j < chars.len() && matches!(chars[j].1, '0' | '1') {
                    j += 1;
                }
                tokens.push(Token::new(sb, byte_at(j), TokenKind::Number));
                i = j;
                seen_word = true;
                continue;
            }
            if c.is_ascii_digit() {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].1.is_ascii_alphanumeric()) {
                    j += 1;
                }
                tokens.push(Token::new(sb, byte_at(j), TokenKind::Number));
                i = j;
                seen_word = true;
                continue;
            }

            // Identifiers: labels, mnemonics, registers, directives.
            if c.is_alphabetic() || c == '_' {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                    j += 1;
                }
                let end_b = byte_at(j);
                let word = &line[sb..end_b];
                let lower = word.to_ascii_lowercase();
                // A trailing ':' (label) or an identifier in column 0 before any
                // mnemonic is treated as a label definition.
                let is_label = (j < chars.len() && chars[j].1 == ':')
                    || (sb == 0 && !self.mnemonics.contains(lower.as_str())
                        && !self.directives.contains(lower.as_str()));
                let kind = if is_label && !seen_word {
                    TokenKind::Label
                } else if self.directives.contains(lower.as_str()) {
                    TokenKind::Preprocessor
                } else if self.mnemonics.contains(lower.as_str()) {
                    TokenKind::Keyword
                } else if self.registers.contains(lower.as_str()) {
                    TokenKind::Register
                } else {
                    TokenKind::Ident
                };
                if kind != TokenKind::Ident {
                    tokens.push(Token::new(sb, end_b, kind));
                }
                i = j;
                seen_word = true;
                continue;
            }

            i += 1;
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_highlights_keyword_type_and_builtin() {
        let h = SpecHighlighter::new(rust_spec());
        let toks = h.highlight_line("pub fn main() { let v: Vec<u8> = vec![]; }");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Function)); // main(
    }

    #[test]
    fn sql_is_case_insensitive() {
        let h = SpecHighlighter::new(sql_spec(Language::SqlAnsi, &[]));
        let toks = h.highlight_line("SeLeCt * FROM t WHERE x = 1");
        // both SeLeCt and FROM and WHERE are keywords
        let kw = toks.iter().filter(|t| t.kind == TokenKind::Keyword).count();
        assert!(kw >= 3, "expected >=3 keywords, got {kw}");
    }

    #[test]
    fn tsql_variable_sigil() {
        let h = SpecHighlighter::from_parts(
            sql_spec(Language::TSql, SIG_AT),
            TSQL_KEYWORDS,
            TSQL_TYPES,
            TSQL_BUILTINS,
        );
        let toks = h.highlight_line("DECLARE @count INT = @@ROWCOUNT");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // DECLARE
    }

    #[test]
    fn snowflake_dollar_variable_and_builtin() {
        let h = SpecHighlighter::from_parts(
            sql_spec(Language::SnowflakeSql, SIG_DOLLAR),
            SNOWFLAKE_KEYWORDS,
            SNOWFLAKE_TYPES,
            SNOWFLAKE_BUILTINS,
        );
        let toks = h.highlight_line("SELECT PARSE_JSON($config) FROM t");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin));
    }

    #[test]
    fn z80_label_mnemonic_register_number_comment() {
        let h = Z80Highlighter::new();
        let toks = h.highlight_line("start:  ld a, $FF   ; load 255");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Label));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // ld
        assert!(toks.iter().any(|t| t.kind == TokenKind::Register)); // a
        assert!(toks.iter().any(|t| t.kind == TokenKind::Number)); // $FF
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn z80_directive_and_binary_number() {
        let h = Z80Highlighter::new();
        let toks = h.highlight_line("        org %10000000");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Preprocessor));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Number));
    }
}
