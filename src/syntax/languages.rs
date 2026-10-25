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
        Box::new(SpecHighlighter::new(kotlin_spec())),
        Box::new(SpecHighlighter::new(ruby_spec())),
        Box::new(SpecHighlighter::new(lua_spec())),
        Box::new(SpecHighlighter::new(csharp_spec())),
        Box::new(SpecHighlighter::new(swift_spec())),
        Box::new(SpecHighlighter::new(php_spec())),
        Box::new(SpecHighlighter::new(scala_spec())),
        Box::new(SpecHighlighter::new(dart_spec())),
        Box::new(SpecHighlighter::new(r_spec())),
        Box::new(SpecHighlighter::new(haskell_spec())),
        Box::new(SpecHighlighter::new(zig_spec())),
        Box::new(SpecHighlighter::new(elixir_spec())),
        Box::new(SpecHighlighter::new(perl_spec())),
        Box::new(SpecHighlighter::new(ocaml_spec())),
        Box::new(SpecHighlighter::new(julia_spec())),
        Box::new(SpecHighlighter::new(powershell_spec())),
        Box::new(SpecHighlighter::new(hcl_spec())),
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

// ---- PHP ------------------------------------------------------------------

fn php_spec() -> LangSpec {
    LangSpec {
        language: Language::Php,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["//", "#"],
        block_comment: Some(("/*", "*/")),
        keywords: PHP_KEYWORDS,
        types: PHP_TYPES,
        builtins: PHP_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &['$'], // $variable
        detect_calls: true,
    }
}

const PHP_KEYWORDS: &[&str] = &[
    "abstract", "and", "array", "as", "break", "callable", "case", "catch", "class", "clone",
    "const", "continue", "declare", "default", "do", "echo", "else", "elseif", "empty", "enum",
    "extends", "final", "finally", "fn", "for", "foreach", "function", "global", "goto", "if",
    "implements", "include", "include_once", "instanceof", "insteadof", "interface", "isset",
    "list", "match", "namespace", "new", "or", "print", "private", "protected", "public",
    "readonly", "require", "require_once", "return", "static", "switch", "throw", "trait", "try",
    "unset", "use", "var", "while", "xor", "yield", "true", "false", "null",
];
const PHP_TYPES: &[&str] = &[
    "int", "float", "bool", "string", "array", "object", "void", "mixed", "iterable", "self",
    "parent", "never", "null",
];
const PHP_BUILTINS: &[&str] = &[
    "count", "strlen", "var_dump", "print_r", "implode", "explode", "array_map", "array_filter",
    "in_array", "printf", "sprintf", "json_encode", "json_decode", "str_replace", "preg_match",
];

// ---- Swift ----------------------------------------------------------------

fn swift_spec() -> LangSpec {
    LangSpec {
        language: Language::Swift,
        // Swift multi-line string literals (`"""…"""`).
        multiline_strings: &["\"\"\""],
        preprocessor: Some('#'), // #if / #available / #selector compilation directives
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: SWIFT_KEYWORDS,
        types: SWIFT_TYPES,
        builtins: SWIFT_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const SWIFT_KEYWORDS: &[&str] = &[
    "func", "let", "var", "class", "struct", "enum", "protocol", "extension", "if", "else", "guard",
    "switch", "case", "default", "for", "in", "while", "repeat", "return", "break", "continue",
    "fallthrough", "do", "try", "catch", "throw", "throws", "rethrows", "defer", "import", "public",
    "private", "internal", "fileprivate", "open", "static", "final", "lazy", "weak", "unowned",
    "mutating", "nonmutating", "override", "convenience", "required", "init", "deinit", "self",
    "super", "nil", "true", "false", "as", "is", "where", "associatedtype", "typealias", "inout",
    "some", "any", "async", "await", "actor", "subscript", "operator", "indirect",
];
const SWIFT_TYPES: &[&str] = &[
    "Int", "Int8", "Int16", "Int32", "Int64", "UInt", "Double", "Float", "Bool", "String",
    "Character", "Array", "Dictionary", "Set", "Optional", "Any", "AnyObject", "Void", "Data",
    "Date", "URL", "Result", "CGFloat", "CGRect", "CGPoint",
];
const SWIFT_BUILTINS: &[&str] = &[
    "print", "map", "filter", "reduce", "compactMap", "flatMap", "append", "assert", "precondition",
    "fatalError", "min", "max", "abs", "zip", "sorted", "contains",
];

// ---- C# -------------------------------------------------------------------

fn csharp_spec() -> LangSpec {
    LangSpec {
        language: Language::CSharp,
        // C# 11 raw string literals (`"""…"""`) can span lines.
        multiline_strings: &["\"\"\""],
        preprocessor: Some('#'), // #region, #if, #define, …
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: CSHARP_KEYWORDS,
        types: CSHARP_TYPES,
        builtins: CSHARP_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const CSHARP_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "base", "break", "case", "catch", "checked", "class",
    "const", "continue", "default", "delegate", "do", "else", "enum", "event", "explicit", "extern",
    "false", "finally", "fixed", "for", "foreach", "goto", "if", "implicit", "in", "interface",
    "internal", "is", "lock", "namespace", "new", "null", "operator", "out", "override", "params",
    "private", "protected", "public", "readonly", "ref", "return", "sealed", "sizeof", "stackalloc",
    "static", "struct", "switch", "this", "throw", "true", "try", "typeof", "unchecked", "unsafe",
    "using", "virtual", "volatile", "while", "yield", "get", "set", "value", "record", "nameof",
    "when", "where", "partial",
];
const CSHARP_TYPES: &[&str] = &[
    "bool", "byte", "sbyte", "char", "decimal", "double", "float", "int", "uint", "long", "ulong",
    "short", "ushort", "object", "string", "void", "var", "dynamic", "nint", "nuint", "String",
    "Int32", "Int64", "Object", "List", "Dictionary", "IEnumerable", "Task", "Span",
];
const CSHARP_BUILTINS: &[&str] = &[
    "Console", "Math", "Convert", "Enumerable", "WriteLine", "ToString", "Exception",
    "ArgumentException", "Func", "Action",
];

// ---- Kotlin --------------------------------------------------------------

fn kotlin_spec() -> LangSpec {
    LangSpec {
        language: Language::Kotlin,
        // Triple-quoted raw strings span lines, like Python's `"""`.
        multiline_strings: &["\"\"\""],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: KOTLIN_KEYWORDS,
        types: KOTLIN_TYPES,
        builtins: KOTLIN_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const KOTLIN_KEYWORDS: &[&str] = &[
    "fun", "val", "var", "class", "object", "interface", "data", "sealed", "enum", "annotation",
    "when", "if", "else", "for", "while", "do", "return", "break", "continue", "in", "is", "as",
    "try", "catch", "finally", "throw", "import", "package", "this", "super", "null", "true",
    "false", "override", "open", "abstract", "final", "private", "protected", "public", "internal",
    "companion", "init", "constructor", "by", "suspend", "typealias", "where", "out", "reified",
    "inline", "operator", "infix", "vararg", "lateinit", "const", "get", "set", "it", "crossinline",
    "noinline", "tailrec", "external", "expect", "actual",
];
const KOTLIN_TYPES: &[&str] = &[
    "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "String", "Unit", "Any",
    "Nothing", "Array", "List", "Map", "Set", "MutableList", "MutableMap", "MutableSet", "Pair",
    "Triple", "Sequence", "Collection", "Iterable", "Number",
];
const KOTLIN_BUILTINS: &[&str] = &[
    "println", "print", "listOf", "mapOf", "setOf", "mutableListOf", "mutableMapOf", "mutableSetOf",
    "arrayOf", "require", "check", "error", "TODO", "let", "also", "apply", "run", "with", "lazy",
    "takeIf", "takeUnless", "repeat",
];

// ---- Ruby ----------------------------------------------------------------

fn ruby_spec() -> LangSpec {
    LangSpec {
        language: Language::Ruby,
        // `=begin`/`=end` block comments are line-anchored, not inline, so they are
        // left out here; `#` line comments cover the common case.
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: None,
        keywords: RUBY_KEYWORDS,
        types: RUBY_TYPES,
        builtins: RUBY_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        // `@ivar`, `@@cvar`, `$global` read as variables.
        var_sigils: &['@', '$'],
        detect_calls: true,
    }
}

const RUBY_KEYWORDS: &[&str] = &[
    "def", "end", "class", "module", "if", "elsif", "else", "unless", "while", "until", "for",
    "in", "do", "then", "begin", "rescue", "ensure", "retry", "raise", "return", "yield", "next",
    "break", "redo", "case", "when", "require", "require_relative", "load", "include", "extend",
    "super", "self", "nil", "true", "false", "and", "or", "not", "lambda", "proc", "defined?",
    "__method__", "alias", "undef",
];
const RUBY_TYPES: &[&str] = &[
    "String", "Integer", "Float", "Array", "Hash", "Symbol", "Object", "Class", "Module", "Proc",
    "Range", "Regexp", "Exception", "StandardError", "NilClass", "TrueClass", "FalseClass",
    "Comparable", "Enumerable", "Numeric", "Struct",
];
const RUBY_BUILTINS: &[&str] = &[
    "puts", "print", "p", "pp", "gets", "attr_accessor", "attr_reader", "attr_writer", "new",
    "freeze", "loop", "format", "sprintf", "require", "catch", "throw", "fail",
];

// ---- Lua ------------------------------------------------------------------

fn lua_spec() -> LangSpec {
    LangSpec {
        language: Language::Lua,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["--"],
        // Lua's `--[[ ... ]]` long comments; the block opener is matched before the
        // `--` line comment (see the tokenizer) so it wins when both could apply.
        block_comment: Some(("--[[", "]]")),
        keywords: LUA_KEYWORDS,
        types: &[],
        builtins: LUA_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const LUA_BUILTINS: &[&str] = &[
    "print", "require", "pairs", "ipairs", "type", "tostring", "tonumber", "setmetatable",
    "getmetatable", "rawget", "rawset", "pcall", "xpcall", "error", "assert", "select", "next",
    "table", "string", "math", "io", "os", "coroutine", "unpack",
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

// ---- Scala ---------------------------------------------------------------

fn scala_spec() -> LangSpec {
    LangSpec {
        language: Language::Scala,
        // Scala triple-quoted multi-line strings (`"""…"""`).
        multiline_strings: &["\"\"\""],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: SCALA_KEYWORDS,
        types: SCALA_TYPES,
        builtins: SCALA_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const SCALA_KEYWORDS: &[&str] = &[
    "abstract", "case", "catch", "class", "def", "do", "else", "extends", "final", "finally",
    "for", "forSome", "given", "if", "implicit", "import", "lazy", "match", "new", "object",
    "override", "package", "private", "protected", "return", "sealed", "super", "this", "throw",
    "trait", "try", "type", "val", "var", "while", "with", "yield", "enum", "export", "inline",
    "then", "using", "end", "null", "true", "false",
];
const SCALA_TYPES: &[&str] = &[
    "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "String", "Unit",
    "Any", "AnyRef", "AnyVal", "Nothing", "Null", "Option", "Some", "None", "List", "Seq",
    "Vector", "Map", "Set", "Array", "Tuple", "Either", "Left", "Right", "Future", "Try",
];
const SCALA_BUILTINS: &[&str] = &[
    "println", "print", "map", "filter", "flatMap", "foreach", "fold", "foldLeft", "foldRight",
    "reduce", "collect", "require", "assert", "implicitly", "apply", "zip", "to", "until",
];

// ---- Dart ----------------------------------------------------------------

fn dart_spec() -> LangSpec {
    LangSpec {
        language: Language::Dart,
        // Dart triple-quoted multi-line strings (`'''…'''` and `"""…"""`).
        multiline_strings: &["\"\"\"", "'''"],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        keywords: DART_KEYWORDS,
        types: DART_TYPES,
        builtins: DART_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const DART_KEYWORDS: &[&str] = &[
    "abstract", "as", "assert", "async", "await", "break", "case", "catch", "class", "const",
    "continue", "covariant", "default", "deferred", "do", "dynamic", "else", "enum", "export",
    "extends", "extension", "external", "factory", "final", "finally", "for", "get", "hide", "if",
    "implements", "import", "in", "is", "late", "library", "mixin", "new", "on", "operator", "part",
    "required", "rethrow", "return", "sealed", "set", "show", "static", "super", "switch", "sync",
    "this", "throw", "try", "typedef", "var", "while", "with", "yield", "null", "true", "false",
];
const DART_TYPES: &[&str] = &[
    "int", "double", "num", "bool", "String", "List", "Map", "Set", "Iterable", "Future",
    "Stream", "Object", "void", "Function", "Symbol", "Runes", "Duration", "DateTime", "Uri",
    "Null", "Never", "Record",
];
const DART_BUILTINS: &[&str] = &[
    "print", "map", "where", "forEach", "toList", "add", "remove", "contains", "length",
    "fold", "reduce", "expand", "firstWhere", "identical", "assert",
];

// ---- R -------------------------------------------------------------------

fn r_spec() -> LangSpec {
    LangSpec {
        language: Language::R,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: None,
        keywords: R_KEYWORDS,
        types: R_TYPES,
        builtins: R_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const R_KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "repeat", "function", "return", "break", "next", "in",
    "TRUE", "FALSE", "NULL", "NA", "Inf", "NaN", "T", "F", "library", "require",
];
const R_TYPES: &[&str] = &[
    "numeric", "integer", "double", "character", "logical", "complex", "raw", "factor",
    "list", "vector", "matrix", "array", "data.frame", "data_frame", "tibble",
];
const R_BUILTINS: &[&str] = &[
    "c", "print", "cat", "paste", "paste0", "length", "nrow", "ncol", "dim", "names", "sum",
    "mean", "median", "sd", "var", "min", "max", "seq", "rep", "sapply", "lapply", "vapply",
    "apply", "mapply", "which", "head", "tail", "str", "summary", "plot", "read.csv", "rnorm",
];

// ---- Haskell -------------------------------------------------------------

fn haskell_spec() -> LangSpec {
    LangSpec {
        language: Language::Haskell,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["--"],
        block_comment: Some(("{-", "-}")),
        keywords: HASKELL_KEYWORDS,
        types: HASKELL_TYPES,
        builtins: HASKELL_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const HASKELL_KEYWORDS: &[&str] = &[
    "module", "where", "import", "qualified", "as", "hiding", "let", "in", "do", "case", "of",
    "if", "then", "else", "class", "instance", "data", "newtype", "type", "deriving", "default",
    "infix", "infixl", "infixr", "foreign", "family", "mdo", "proc", "rec",
];
const HASKELL_TYPES: &[&str] = &[
    "Int", "Integer", "Float", "Double", "Rational", "Bool", "Char", "String", "Maybe", "Either",
    "Ordering", "IO", "Word", "Ord", "Eq", "Show", "Read", "Num", "Functor", "Monad", "Applicative",
    "Foldable", "Traversable",
];
const HASKELL_BUILTINS: &[&str] = &[
    "map", "filter", "foldr", "foldl", "zip", "zipWith", "fmap", "return", "pure", "putStrLn",
    "print", "show", "read", "head", "tail", "length", "reverse", "concat", "elem", "maybe",
    "either", "const", "id", "flip", "curry", "uncurry", "error", "undefined", "Just", "Nothing",
    "True", "False",
];

// ---- Zig -----------------------------------------------------------------

fn zig_spec() -> LangSpec {
    LangSpec {
        language: Language::Zig,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["//"],
        block_comment: None,
        keywords: ZIG_KEYWORDS,
        types: ZIG_TYPES,
        builtins: ZIG_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        // `@import`, `@sizeOf`, … are builtin calls; highlight the `@name` token.
        var_sigils: &['@'],
        detect_calls: true,
    }
}

const ZIG_KEYWORDS: &[&str] = &[
    "const", "var", "fn", "pub", "return", "if", "else", "while", "for", "switch", "break",
    "continue", "defer", "errdefer", "try", "catch", "orelse", "unreachable", "struct", "enum",
    "union", "error", "comptime", "inline", "export", "extern", "async", "await", "suspend",
    "resume", "test", "and", "or", "usingnamespace", "threadlocal", "packed", "align", "null",
    "undefined", "true", "false",
];
const ZIG_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    "f16", "f32", "f64", "f128", "bool", "void", "type", "anytype", "anyerror", "noreturn",
    "c_int", "c_uint", "comptime_int", "comptime_float",
];
const ZIG_BUILTINS: &[&str] = &[
    "std", "allocator", "print", "assert", "expect", "alloc", "free", "init", "deinit",
];

// ---- Elixir --------------------------------------------------------------

fn elixir_spec() -> LangSpec {
    LangSpec {
        language: Language::Elixir,
        // Elixir heredocs (`"""` / `'''`).
        multiline_strings: &["\"\"\"", "'''"],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: None,
        keywords: ELIXIR_KEYWORDS,
        types: ELIXIR_TYPES,
        builtins: ELIXIR_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        var_sigils: &['@'],
        detect_calls: true,
    }
}

const ELIXIR_KEYWORDS: &[&str] = &[
    "def", "defp", "defmodule", "defmacro", "defmacrop", "defstruct", "defprotocol", "defimpl",
    "defguard", "do", "end", "fn", "if", "unless", "else", "case", "cond", "when", "with", "for",
    "receive", "try", "rescue", "catch", "after", "raise", "throw", "import", "alias", "require",
    "use", "quote", "unquote", "and", "or", "not", "in", "nil", "true", "false",
];
const ELIXIR_TYPES: &[&str] = &[
    "Enum", "Map", "List", "String", "Integer", "Float", "Atom", "Tuple", "Keyword", "Range",
    "Stream", "Process", "Agent", "GenServer", "Supervisor", "Task", "Kernel", "IO", "File",
];
const ELIXIR_BUILTINS: &[&str] = &[
    "map", "reduce", "filter", "each", "put", "get", "to_string", "inspect", "is_nil", "is_atom",
    "is_list", "is_map", "is_binary", "length", "hd", "tl", "elem", "spawn", "send", "self",
];

// ---- Perl ----------------------------------------------------------------

fn perl_spec() -> LangSpec {
    LangSpec {
        language: Language::Perl,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: None,
        keywords: PERL_KEYWORDS,
        types: PERL_TYPES,
        builtins: PERL_BUILTINS,
        string_delims: &['"', '\''],
        case_insensitive: false,
        // `$scalar`, `@array`, `%hash` sigils.
        var_sigils: &['$', '@', '%'],
        detect_calls: true,
    }
}

const PERL_KEYWORDS: &[&str] = &[
    "sub", "my", "our", "local", "use", "no", "require", "package", "if", "elsif", "else", "unless",
    "while", "until", "for", "foreach", "do", "return", "last", "next", "redo", "and", "or", "not",
    "eq", "ne", "lt", "gt", "le", "ge", "cmp", "qw", "wantarray", "undef", "defined",
    "bless", "ref",
];
const PERL_TYPES: &[&str] = &["SCALAR", "ARRAY", "HASH", "CODE", "GLOB", "Scalar", "Ref"];
const PERL_BUILTINS: &[&str] = &[
    "print", "printf", "say", "sprintf", "push", "pop", "shift", "unshift", "splice", "join",
    "split", "map", "grep", "sort", "reverse", "keys", "values", "each", "exists", "delete",
    "scalar", "chomp", "chop", "die", "warn", "open", "close", "chomp", "length", "substr",
];

// ---- OCaml ---------------------------------------------------------------

fn ocaml_spec() -> LangSpec {
    LangSpec {
        language: Language::OCaml,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &[],
        block_comment: Some(("(*", "*)")),
        keywords: OCAML_KEYWORDS,
        types: OCAML_TYPES,
        builtins: OCAML_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const OCAML_KEYWORDS: &[&str] = &[
    "let", "rec", "in", "and", "fun", "function", "match", "with", "when", "if", "then", "else",
    "type", "of", "module", "struct", "sig", "end", "open", "include", "begin", "for", "to",
    "downto", "while", "do", "done", "try", "raise", "exception", "mutable", "ref", "as", "val",
    "external", "lazy", "assert", "object", "class", "method", "inherit", "true", "false",
];
const OCAML_TYPES: &[&str] = &[
    "int", "float", "bool", "char", "string", "unit", "list", "array", "option", "ref", "exn",
    "bytes", "int32", "int64", "nativeint",
];
const OCAML_BUILTINS: &[&str] = &[
    "print_string", "print_endline", "print_int", "printf", "sprintf", "failwith", "ignore",
    "compare", "min", "max", "incr", "decr", "fst", "snd", "Some", "None", "List", "Array",
    "String", "Printf",
];

// ---- Julia ---------------------------------------------------------------

fn julia_spec() -> LangSpec {
    LangSpec {
        language: Language::Julia,
        // Julia triple-quoted strings.
        multiline_strings: &["\"\"\""],
        preprocessor: None,
        line_comments: &["#"],
        block_comment: Some(("#=", "=#")),
        keywords: JULIA_KEYWORDS,
        types: JULIA_TYPES,
        builtins: JULIA_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &['@'], // macros: @time, @assert
        detect_calls: true,
    }
}

const JULIA_KEYWORDS: &[&str] = &[
    "function", "end", "if", "elseif", "else", "while", "for", "in", "return", "break", "continue",
    "begin", "let", "do", "try", "catch", "finally", "struct", "mutable", "abstract", "primitive",
    "type", "module", "using", "import", "export", "const", "global", "local", "quote", "where",
    "macro", "true", "false", "nothing", "missing",
];
const JULIA_TYPES: &[&str] = &[
    "Int", "Int8", "Int16", "Int32", "Int64", "UInt", "Float16", "Float32", "Float64", "Bool",
    "Char", "String", "Symbol", "Array", "Vector", "Matrix", "Dict", "Set", "Tuple", "Any",
    "Number", "Real", "Complex", "Nothing",
];
const JULIA_BUILTINS: &[&str] = &[
    "println", "print", "push!", "pop!", "length", "size", "map", "filter", "reduce", "sum", "prod",
    "sort", "reverse", "zeros", "ones", "rand", "collect", "enumerate", "zip", "typeof", "error",
    "throw",
];

// ---- PowerShell ----------------------------------------------------------

fn powershell_spec() -> LangSpec {
    LangSpec {
        language: Language::PowerShell,
        multiline_strings: &[],
        preprocessor: None,
        line_comments: &["#"],
        // Block comments are `<# … #>`; checked before the `#` line comment.
        block_comment: Some(("<#", "#>")),
        keywords: POWERSHELL_KEYWORDS,
        types: POWERSHELL_TYPES,
        builtins: POWERSHELL_BUILTINS,
        // Both quoting styles; `'…'` is literal, `"…"` is expandable.
        string_delims: &['"', '\''],
        // PowerShell keywords, operators and cmdlets are case-insensitive.
        case_insensitive: true,
        var_sigils: &['$'], // $x, $_, $true, $env:PATH
        detect_calls: true,
    }
}

const POWERSHELL_KEYWORDS: &[&str] = &[
    // Language keywords.
    "begin", "break", "catch", "class", "continue", "data", "define", "do", "dynamicparam", "else",
    "elseif", "end", "enum", "exit", "filter", "finally", "for", "foreach", "from", "function",
    "hidden", "if", "in", "inlinescript", "parallel", "param", "process", "return", "sequence",
    "static", "switch", "throw", "trap", "try", "until", "using", "while", "workflow",
    // Word operators. The leading `-` tokenizes separately; the bare word matches.
    "eq", "ne", "gt", "ge", "lt", "le", "like", "notlike", "match", "notmatch", "contains",
    "notcontains", "notin", "is", "isnot", "and", "or", "xor", "not", "band", "bor", "bxor", "bnot",
    "shl", "shr", "replace", "split", "join", "ceq", "cne", "clike", "cmatch", "ieq", "ine",
];
const POWERSHELL_TYPES: &[&str] = &[
    "int", "long", "string", "char", "bool", "byte", "sbyte", "short", "ushort", "uint", "ulong",
    "double", "decimal", "single", "float", "array", "hashtable", "object", "void", "psobject",
    "pscustomobject", "datetime", "timespan", "guid", "version", "uri", "xml", "regex",
    "scriptblock", "type", "pscredential", "ipaddress", "ordered", "nullable", "bigint",
];
const POWERSHELL_BUILTINS: &[&str] = &[
    // Common terse cmdlet aliases.
    "echo", "write", "cat", "ls", "dir", "gci", "cd", "pwd", "cls", "gc", "sc", "gm", "gcm", "gal",
    "iwr", "irm", "cp", "cpi", "mv", "mi", "rm", "ri", "ni", "del", "type", "sort", "select",
    "where", "measure", "group", "tee", "ft", "fl", "pushd", "popd",
    // Full Verb-Noun cmdlets (highlighted as one token — see the hyphen-join rule).
    "Write-Host", "Write-Output", "Write-Error", "Write-Warning", "Write-Verbose", "Write-Debug",
    "Get-ChildItem", "Get-Content", "Set-Content", "Add-Content", "Get-Item", "Set-Item",
    "New-Item", "Remove-Item", "Copy-Item", "Move-Item", "Rename-Item", "Test-Path", "Join-Path",
    "Split-Path", "Resolve-Path", "Get-Command", "Get-Help", "Get-Member", "Get-Module",
    "Import-Module", "Export-ModuleMember", "Select-Object", "Where-Object", "ForEach-Object",
    "Sort-Object", "Measure-Object", "Group-Object", "Compare-Object", "Format-Table",
    "Format-List", "Out-File", "Out-Null", "Out-String", "Out-Host", "Read-Host",
    "Invoke-Expression", "Invoke-Command", "Invoke-WebRequest", "Invoke-RestMethod",
    "Start-Process", "Stop-Process", "Get-Process", "Start-Sleep", "Get-Date", "Set-Location",
    "Get-Location", "New-Object", "ConvertTo-Json", "ConvertFrom-Json", "ConvertTo-Csv",
    "ConvertFrom-Csv", "Export-Csv", "Import-Csv", "Select-String", "Get-Variable",
    "Set-Variable", "Write-Progress", "New-Variable",
];

// ---- HCL / Terraform -----------------------------------------------------

fn hcl_spec() -> LangSpec {
    LangSpec {
        language: Language::Hcl,
        multiline_strings: &[],
        preprocessor: None,
        // HCL allows `#` and `//` line comments and `/* */` blocks.
        line_comments: &["#", "//"],
        block_comment: Some(("/*", "*/")),
        keywords: HCL_KEYWORDS,
        types: HCL_TYPES,
        builtins: HCL_BUILTINS,
        string_delims: &['"'],
        case_insensitive: false,
        var_sigils: &[],
        detect_calls: true,
    }
}

const HCL_KEYWORDS: &[&str] = &[
    "resource", "variable", "provider", "module", "output", "data", "locals", "terraform",
    "backend", "provisioner", "connection", "lifecycle", "dynamic", "content", "depends_on",
    "count", "for_each", "each", "for", "in", "if", "else", "endfor", "endif", "var", "local",
    "path", "self", "true", "false", "null",
];
const HCL_TYPES: &[&str] = &[
    "string", "number", "bool", "list", "map", "set", "object", "tuple", "any", "null",
];
const HCL_BUILTINS: &[&str] = &[
    "length", "lookup", "merge", "concat", "element", "contains", "keys", "values", "zipmap",
    "flatten", "join", "split", "format", "formatlist", "replace", "regex", "regexall", "lower",
    "upper", "title", "trim", "trimspace", "trimprefix", "trimsuffix", "substr", "chomp", "indent",
    "file", "fileexists", "fileset", "filebase64", "templatefile", "jsonencode", "jsondecode",
    "yamlencode", "yamldecode", "base64encode", "base64decode", "coalesce", "coalescelist",
    "compact", "distinct", "range", "setproduct", "slice", "sort", "toset", "tolist", "tomap",
    "tostring", "tonumber", "tobool", "can", "try", "abs", "ceil", "floor", "max", "min", "pow",
    "signum", "cidrhost", "cidrnetmask", "cidrsubnet", "cidrsubnets", "timestamp", "timeadd",
    "formatdate", "uuid", "md5", "sha1", "sha256", "base64sha256",
];

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
    fn kotlin_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(kotlin_spec());
        let toks = h.highlight_line("fun main() { val s: String = \"hi\"; println(s) } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // fun/val
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // String
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // println
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn kotlin_raw_string_spans_lines() {
        let h = SpecHighlighter::new(kotlin_spec());
        let (_t, state) = h.highlight_line_stateful("val q = \"\"\"start", LineState::Normal);
        assert_eq!(state, LineState::MultiStr(0)); // raw string stays open
        let (_t2, state2) = h.highlight_line_stateful("end\"\"\"", state);
        assert_eq!(state2, LineState::Normal); // and closes
    }

    #[test]
    fn ruby_highlights_keyword_type_builtin_string_and_ivar() {
        let h = SpecHighlighter::new(ruby_spec());
        let toks = h.highlight_line("def f; @a = Array.new(\"x\"); puts @a; end # c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // def/end
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // Array
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // new/puts
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "x"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // @a
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # c
    }

    #[test]
    fn lua_highlights_keyword_builtin_string_and_line_comment() {
        let h = SpecHighlighter::new(lua_spec());
        let toks = h.highlight_line("local function f() print(\"hi\") end -- note");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // local/function/end
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // print
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // -- note
    }

    #[test]
    fn lua_long_comment_spans_lines_and_line_comment_still_works() {
        let h = SpecHighlighter::new(lua_spec());
        // `--[[` opens a block that runs until `]]`, across lines.
        let (t0, s0) = h.highlight_line_stateful("x = 1 --[[ open", LineState::Normal);
        assert_eq!(s0, LineState::Block);
        assert!(t0.iter().any(|t| t.kind == TokenKind::Comment));
        let (_t1, s1) = h.highlight_line_stateful("still comment ]] y = 2", s0);
        assert_eq!(s1, LineState::Normal); // closed on this line
        // A plain `--` line comment is NOT swallowed as a block.
        let (_t2, s2) = h.highlight_line_stateful("z = 3 -- just a note", LineState::Normal);
        assert_eq!(s2, LineState::Normal);
    }

    #[test]
    fn php_highlights_keyword_type_builtin_string_and_var() {
        let h = SpecHighlighter::new(php_spec());
        let toks = h.highlight_line("function f(int $n) { $s = \"hi\"; echo strlen($s); } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // function/echo
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // int
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // strlen
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // $n / $s
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn swift_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(swift_spec());
        let toks = h.highlight_line("func f() { let s: String = \"hi\"; print(s) } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // func/let
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // String
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // print
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn scala_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(scala_spec());
        let toks = h.highlight_line("def f(): Unit = { val s: String = \"hi\"; println(s) } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // def/val
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // String/Unit
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // println
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn dart_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(dart_spec());
        let toks = h.highlight_line("void main() { final List<int> x = []; print(x); } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // final/void
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // List/int
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // print
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn r_highlights_keyword_builtin_string_comment() {
        let h = SpecHighlighter::new(r_spec());
        let toks = h.highlight_line("f <- function(x) { print(c(1, 2)) } # a comment");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // function
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // print/c
        assert!(toks.iter().any(|t| t.kind == TokenKind::Number)); // 1, 2
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # ...
    }

    #[test]
    fn haskell_highlights_keyword_type_comment() {
        let h = SpecHighlighter::new(haskell_spec());
        let toks = h.highlight_line("main :: IO () -- entry\nmain = putStrLn \"hi\"");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // IO
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // -- entry
    }

    #[test]
    fn haskell_highlights_keyword_and_string() {
        let h = SpecHighlighter::new(haskell_spec());
        let toks = h.highlight_line("let x = map show [1,2] in \"done\"");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // let/in
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // map/show
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "done"
    }

    #[test]
    fn zig_highlights_keyword_type_string_comment() {
        let h = SpecHighlighter::new(zig_spec());
        let toks = h.highlight_line("const x: u32 = 1; // c  pub fn f() void {}");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // const/pub/fn
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // u32/void
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // c
    }

    #[test]
    fn elixir_highlights_keyword_builtin_string_comment() {
        let h = SpecHighlighter::new(elixir_spec());
        let toks = h.highlight_line("def run do Enum.map(xs) end # note");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // def/do/end
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // Enum
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # note
    }

    #[test]
    fn perl_highlights_keyword_builtin_var_comment() {
        let h = SpecHighlighter::new(perl_spec());
        let toks = h.highlight_line("sub f { my $x = shift; print \"hi\"; } # c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // sub/my
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // shift/print
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // $x
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # c
    }

    #[test]
    fn ocaml_highlights_keyword_type_comment() {
        let h = SpecHighlighter::new(ocaml_spec());
        let toks = h.highlight_line("let f (x : int) = (* c *) x + 1");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // let
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // int
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // (* c *)
    }

    #[test]
    fn julia_highlights_keyword_type_builtin_comment() {
        let h = SpecHighlighter::new(julia_spec());
        let toks = h.highlight_line("function f(x::Int) println(x) end # c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // function/end
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // Int
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // println
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # c
    }

    #[test]
    fn powershell_highlights_keyword_type_var_string_comment() {
        let h = SpecHighlighter::new(powershell_spec());
        let toks =
            h.highlight_line("function f { param([int]$n) $s = \"hi\"; echo $s } # c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // function/param
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // int
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // $n / $s
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // echo
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "hi"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # c
    }

    #[test]
    fn powershell_hyphenated_cmdlets_are_one_token() {
        let h = SpecHighlighter::new(powershell_spec());
        let line = "Get-ChildItem -Path $p | Where-Object { $_ -eq 1 }";
        let toks = h.highlight_line(line);
        // The full Verb-Noun cmdlet is a single Builtin token.
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Builtin && &line[t.start..t.end] == "Get-ChildItem"));
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Builtin && &line[t.start..t.end] == "Where-Object"));
        // The `-eq` operator is not merged into a cmdlet: `eq` stays a keyword.
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Keyword && &line[t.start..t.end] == "eq"));
        // `$_` remains a variable.
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Variable && &line[t.start..t.end] == "$_"));
    }

    #[test]
    fn hcl_highlights_keyword_type_builtin_string_comment() {
        let h = SpecHighlighter::new(hcl_spec());
        let toks = h.highlight_line(
            "resource \"aws_s3_bucket\" \"b\" { count = length(var.names) } # c",
        );
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // resource/count/var
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // length
        assert!(toks.iter().any(|t| t.kind == TokenKind::String)); // "aws_s3_bucket"
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // # c
    }

    #[test]
    fn hcl_type_in_variable_block_and_slash_comment() {
        let h = SpecHighlighter::new(hcl_spec());
        let toks = h.highlight_line("variable \"x\" { type = string } // note");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // variable/type
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // string
        assert!(toks.iter().any(|t| t.kind == TokenKind::Comment)); // // note
    }

    #[test]
    fn powershell_is_case_insensitive_and_block_comment_spans_lines() {
        let h = SpecHighlighter::new(powershell_spec());
        // Keywords match regardless of case.
        let toks = h.highlight_line("FOREACH ($x IN $xs) { }");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // FOREACH/IN
        assert!(toks.iter().any(|t| t.kind == TokenKind::Variable)); // $x / $xs
        // `<# … #>` opens a block that runs until `#>`, across lines.
        let (t0, s0) = h.highlight_line_stateful("x <# open", LineState::Normal);
        assert_eq!(s0, LineState::Block);
        assert!(t0.iter().any(|t| t.kind == TokenKind::Comment));
        let (_t1, s1) = h.highlight_line_stateful("still comment #> y", s0);
        assert_eq!(s1, LineState::Normal); // closed here
        // A bare `#` is a line comment, not swallowed as a block.
        let (_t2, s2) = h.highlight_line_stateful("$z = 3 # note", LineState::Normal);
        assert_eq!(s2, LineState::Normal);
    }

    #[test]
    fn csharp_highlights_keyword_type_builtin_string() {
        let h = SpecHighlighter::new(csharp_spec());
        let toks = h.highlight_line("public static void Main() { string s = \"hi\"; Console.WriteLine(s); } // c");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Keyword)); // public/static
        assert!(toks.iter().any(|t| t.kind == TokenKind::Type)); // void/string
        assert!(toks.iter().any(|t| t.kind == TokenKind::Builtin)); // Console
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
