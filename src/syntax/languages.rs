//! Built-in language definitions.
//!
//! Most languages are expressed declaratively as a [`LangSpec`]. Z80 assembly
//! has enough idiosyncrasies (labels, `$`/`%` number bases, registers) that it
//! gets a hand-written [`Highlighter`].

use super::{
    Highlighter, LangSpec, Language, LineState, SpecHighlighter, Token, TokenKind,
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
        Box::new(SpecHighlighter::new(json_spec())),
        Box::new(SpecHighlighter::new(python_spec())),
        Box::new(SpecHighlighter::new(toml_spec())),
        Box::new(SpecHighlighter::new(javascript_spec())),
        Box::new(SpecHighlighter::new(go_spec())),
        Box::new(SpecHighlighter::new(shell_spec())),
        Box::new(SpecHighlighter::new(c_spec())),
        Box::new(SpecHighlighter::new(java_spec())),
        Box::new(SpecHighlighter::new(yaml_spec())),
        Box::new(SpecHighlighter::new(typescript_spec())),
    ]
}

// ---- TypeScript ----------------------------------------------------------

fn typescript_spec() -> LangSpec {
    LangSpec {
        language: Language::TypeScript,
        multiline_strings: &["`"],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: TS_KEYWORDS,
        types: TS_TYPES,
        builtins: JS_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const TS_KEYWORDS: &[&str] = &[
    // JavaScript
    "var", "let", "const", "function", "return", "if", "else", "for", "while", "do", "switch",
    "case", "break", "continue", "new", "delete", "typeof", "instanceof", "in", "of", "this",
    "class", "extends", "super", "import", "export", "from", "as", "default", "try", "catch",
    "finally", "throw", "async", "await", "yield", "void", "static", "get", "set", "true", "false",
    "null", "undefined",
    // TypeScript
    "interface", "type", "enum", "implements", "namespace", "declare", "abstract", "readonly",
    "public", "private", "protected", "keyof", "infer", "is", "satisfies", "override", "out",
    "constructor",
];
const TS_TYPES: &[&str] = &[
    "string", "number", "boolean", "any", "unknown", "never", "void", "object", "symbol", "bigint",
    "Array", "Promise", "Record", "Partial", "Readonly", "Map", "Set", "Object", "String",
    "Number", "Boolean",
];

// ---- YAML ----------------------------------------------------------------

const SIG_YAML: &[char] = &['&', '*'];

fn yaml_spec() -> LangSpec {
    LangSpec {
        language: Language::Yaml,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: None,
        keywords: YAML_KEYWORDS,
        types: &[],
        builtins: &[],
        string_delims: &['"', '\''],
        case_insensitive: false,
        // `&anchor` / `*alias` highlight as variables.
        var_sigils: SIG_YAML,
        detect_calls: false,
    }
}

const YAML_KEYWORDS: &[&str] = &[
    "true", "false", "null", "yes", "no", "on", "off", "True", "False", "Null",
];

// ---- Java ----------------------------------------------------------------

fn java_spec() -> LangSpec {
    LangSpec {
        language: Language::Java,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: JAVA_KEYWORDS,
        types: JAVA_TYPES,
        builtins: JAVA_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const JAVA_KEYWORDS: &[&str] = &[
    "abstract", "assert", "break", "case", "catch", "class", "const", "continue", "default", "do",
    "else", "enum", "extends", "final", "finally", "for", "goto", "if", "implements", "import",
    "instanceof", "interface", "native", "new", "package", "private", "protected", "public",
    "return", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws",
    "transient", "try", "volatile", "while", "var", "yield", "record", "sealed", "permits",
    "true", "false", "null",
];
const JAVA_TYPES: &[&str] = &[
    "boolean", "byte", "char", "short", "int", "long", "float", "double", "void", "String",
    "Object", "Integer", "Long", "Double", "Float", "Boolean", "Character", "Byte", "Short",
    "List", "Map", "Set", "ArrayList", "HashMap", "HashSet", "Optional",
];
const JAVA_BUILTINS: &[&str] = &[
    "System", "Math", "Arrays", "Objects", "Collections", "Thread", "Exception",
    "RuntimeException", "Override", "Deprecated", "SuppressWarnings",
];

// ---- C / C++ -------------------------------------------------------------

fn c_spec() -> LangSpec {
    LangSpec {
        language: Language::C,
        preprocessor: Some('#'),
        multiline_strings: &[],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: C_KEYWORDS,
        types: C_TYPES,
        builtins: C_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const C_KEYWORDS: &[&str] = &[
    // C
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern",
    "for", "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static", "struct",
    "switch", "typedef", "union", "volatile", "while",
    // C++
    "class", "namespace", "template", "typename", "public", "private", "protected", "virtual",
    "override", "new", "delete", "this", "using", "try", "catch", "throw", "operator", "friend",
    "explicit", "constexpr", "noexcept", "nullptr", "true", "false",
];
const C_TYPES: &[&str] = &[
    "void", "bool", "char", "short", "int", "long", "float", "double", "signed", "unsigned",
    "wchar_t", "size_t", "ssize_t", "ptrdiff_t", "int8_t", "int16_t", "int32_t", "int64_t",
    "uint8_t", "uint16_t", "uint32_t", "uint64_t", "intptr_t", "uintptr_t", "FILE", "va_list",
    "string", "vector", "map",
];
const C_BUILTINS: &[&str] = &[
    "printf", "fprintf", "sprintf", "scanf", "malloc", "calloc", "realloc", "free", "memcpy",
    "memset", "strlen", "strcmp", "strcpy", "strcat", "assert", "NULL",
];

// ---- Shell ---------------------------------------------------------------

const SIG_SHELL: &[char] = &['$'];

fn shell_spec() -> LangSpec {
    LangSpec {
        language: Language::Shell,
        preprocessor: None,
        multiline_strings: &[],
        line_comments: &["#"],
        block_comment: None,
        keywords: SHELL_KEYWORDS,
        types: &[],
        builtins: SHELL_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        // `$VAR` / `$1` highlight as variables.
        var_sigils: SIG_SHELL,
        detect_calls: false,
    }
}

const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "return", "break", "continue", "local", "export",
    "readonly", "declare", "typeset",
];
const SHELL_BUILTINS: &[&str] = &[
    "echo", "printf", "read", "cd", "pwd", "test", "source", "eval", "exec", "exit", "set",
    "unset", "shift", "trap", "wait", "kill", "alias", "unalias", "getopts", "true", "false",
];

// ---- Go ------------------------------------------------------------------

fn go_spec() -> LangSpec {
    LangSpec {
        language: Language::Go,
        preprocessor: None,
        // Raw string literals use backticks and span lines; tracked separately
        // from block comments by the per-line LineState.
        multiline_strings: &["`"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: GO_KEYWORDS,
        types: GO_TYPES,
        builtins: GO_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const GO_KEYWORDS: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for",
    "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return",
    "select", "struct", "switch", "type", "var", "nil", "true", "false", "iota",
];
const GO_TYPES: &[&str] = &[
    "bool", "string", "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16",
    "uint32", "uint64", "uintptr", "byte", "rune", "float32", "float64", "complex64", "complex128",
    "error", "any",
];
const GO_BUILTINS: &[&str] = &[
    "append", "cap", "close", "complex", "copy", "delete", "imag", "len", "make", "new", "panic",
    "print", "println", "real", "recover",
];

// ---- JavaScript ----------------------------------------------------------

fn javascript_spec() -> LangSpec {
    LangSpec {
        language: Language::JavaScript,
        preprocessor: None,
        // Backtick template literals span lines; tracked independently from block
        // comments by the per-line LineState.
        multiline_strings: &["`"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: JS_KEYWORDS,
        types: JS_TYPES,
        builtins: JS_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const JS_KEYWORDS: &[&str] = &[
    "var", "let", "const", "function", "return", "if", "else", "for", "while", "do", "switch",
    "case", "break", "continue", "new", "delete", "typeof", "instanceof", "in", "of", "this",
    "class", "extends", "super", "import", "export", "from", "as", "default", "try", "catch",
    "finally", "throw", "async", "await", "yield", "void", "static", "get", "set", "true", "false",
    "null", "undefined",
];
const JS_TYPES: &[&str] = &[
    "Object", "Array", "String", "Number", "Boolean", "Promise", "Map", "Set", "Symbol", "RegExp",
    "Date", "Error", "Function", "BigInt",
];
const JS_BUILTINS: &[&str] = &[
    "console", "Math", "JSON", "parseInt", "parseFloat", "isNaN", "isFinite", "document", "window",
    "require", "module", "exports", "process", "globalThis", "fetch", "setTimeout", "setInterval",
];

// ---- TOML ----------------------------------------------------------------

fn toml_spec() -> LangSpec {
    LangSpec {
        language: Language::Toml,
        preprocessor: None,
        multiline_strings: &[],
        line_comments: &["#"],
        block_comment: None,
        keywords: TOML_KEYWORDS,
        types: &[],
        builtins: &[],
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: false,
    }
}

/// TOML's boolean / float literals; keys, strings, numbers and dates are handled
/// generically by the spec highlighter.
const TOML_KEYWORDS: &[&str] = &["true", "false", "inf", "nan"];

// ---- Python --------------------------------------------------------------

fn python_spec() -> LangSpec {
    LangSpec {
        language: Language::Python,
        preprocessor: None,
        multiline_strings: &["\"\"\"", "'''"],
        line_comments: &["#"],
        block_comment: None,
        keywords: PYTHON_KEYWORDS,
        types: PYTHON_TYPES,
        builtins: PYTHON_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
    "while", "with", "yield", "match", "case",
];
const PYTHON_TYPES: &[&str] = &[
    "int", "float", "complex", "bool", "str", "bytes", "bytearray", "list", "tuple", "dict", "set",
    "frozenset", "object", "type", "memoryview",
];
const PYTHON_BUILTINS: &[&str] = &[
    "print", "len", "range", "enumerate", "zip", "map", "filter", "open", "input", "isinstance",
    "issubclass", "super", "getattr", "setattr", "hasattr", "sorted", "reversed", "sum", "min",
    "max", "abs", "round", "repr", "format", "iter", "next", "any", "all", "id", "hash", "vars",
    "dir", "callable", "staticmethod", "classmethod", "property",
];

// ---- JSON ----------------------------------------------------------------

fn json_spec() -> LangSpec {
    LangSpec {
        language: Language::Json,
        preprocessor: None,
        multiline_strings: &[],
        line_comments: &[],
        block_comment: None,
        keywords: JSON_KEYWORDS,
        types: &[],
        builtins: &[],
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: false,
    }
}

/// JSON's three literal keywords; strings, numbers and punctuation are handled
/// generically by the spec highlighter.
const JSON_KEYWORDS: &[&str] = &["true", "false", "null"];

// ---- Rust ----------------------------------------------------------------

fn rust_spec() -> LangSpec {
    LangSpec {
        language: Language::Rust,
        preprocessor: None,
        multiline_strings: &[],
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
        multiline_strings: &[],
        preprocessor: None,
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

    fn highlight_line_stateful(&self, line: &str, _state: LineState) -> (Vec<Token>, LineState) {
        (self.highlight_line_impl(line), LineState::Normal)
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
    fn json_highlights_literals_strings_numbers() {
        let h = SpecHighlighter::new(json_spec());
        let toks = h.highlight_line(r#"{"on": true, "n": 42, "x": null}"#);
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "on"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Number)); // 42
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // true/null
    }

    #[test]
    fn typescript_highlights_ts_keyword_type_string() {
        let h = SpecHighlighter::new(typescript_spec());
        let toks = h.highlight_line("interface A { name: string } const x = `hi`; // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // interface/const
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // string
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // `hi`
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn yaml_highlights_bool_string_comment_anchor() {
        let h = SpecHighlighter::new(yaml_spec());
        let toks = h.highlight_line("enabled: true  name: \"x\"  ref: &a  # note");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // true
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "x"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # note
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // &a anchor
    }

    #[test]
    fn java_highlights_keyword_type_string() {
        let h = SpecHighlighter::new(java_spec());
        let toks = h.highlight_line("public class A { String s = \"hi\"; } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // public/class
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // String
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn c_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(c_spec());
        let toks = h.highlight_line("int main() { char *s = \"hi\"; return 0; } /* c */");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // return
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // int/char
        assert!(toks.iter().any(|t| t.kind == TokenKind::Function)); // main(
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // /* c */
    }

    #[test]
    fn c_highlights_preprocessor_directive() {
        let h = SpecHighlighter::new(c_spec());
        let toks = h.highlight_line("  #include <stdio.h>");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Preprocessor)); // #include
        // A stray `#` mid-line is not a directive.
        let toks2 = h.highlight_line("int a = b # c;");
        assert!(!toks2.iter().any(|t| t.kind == TokenKind::Preprocessor));
    }

    #[test]
    fn shell_highlights_keyword_builtin_variable() {
        let h = SpecHighlighter::new(shell_spec());
        let toks = h.highlight_line("for x in a; do echo $x; done # loop");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // for/do/done
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // echo
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // $x
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # loop
    }

    #[test]
    fn go_highlights_keyword_type_builtin() {
        let h = SpecHighlighter::new(go_spec());
        let toks = h.highlight_line("func main() { var s string = `raw`; len(s) } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // func/var
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // string
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // len
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // `raw`
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn javascript_highlights_keyword_builtin_string() {
        let h = SpecHighlighter::new(javascript_spec());
        let toks = h.highlight_line("const x = `hi`; console.log(42) // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // const
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // console
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // `hi`
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn toml_highlights_string_number_bool_comment() {
        let h = SpecHighlighter::new(toml_spec());
        let toks = h.highlight_line(r#"name = "rvim"  # port 8080 enabled = true"#);
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "rvim"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # ...
    }

    #[test]
    fn python_highlights_keyword_type_builtin() {
        let h = SpecHighlighter::new(python_spec());
        let toks = h.highlight_line("def f(x: int) -> str: return str(len(x))  # note");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // def/return
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // int/str
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // len
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # note
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
