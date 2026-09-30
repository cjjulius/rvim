//! Parsing of `:` ex-commands.
//!
//! Parsing is separated from execution so it can be unit-tested without a
//! terminal. [`crate::app::App::run_ex`] interprets the [`ExCommand`] this
//! module produces, and unknown commands fall through to the plugin system.

/// A line address inside a substitute range. Symbolic addresses (`.`, `$`) are
/// resolved at execution time when the cursor / line count are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineAddr {
    /// The current line (`.`).
    Current,
    /// The last line (`$`).
    Last,
    /// A concrete 1-based line number.
    Num(usize),
}

/// The line range a `:s` command applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubRange {
    /// No range given — the current line only.
    CurrentLine,
    /// `%` — every line.
    WholeFile,
    /// `a,b` — an inclusive address range.
    Range(LineAddr, LineAddr),
}

/// A parsed `:s/pattern/replacement/flags` command (literal matching).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstituteSpec {
    pub range: SubRange,
    pub pattern: String,
    pub replacement: String,
    /// The `g` flag — replace all occurrences per line.
    pub global: bool,
}

/// A parsed ex-command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExCommand {
    /// `:w [file]`
    Write(Option<String>),
    /// `:q` / `:q!`
    Quit { force: bool },
    /// `:wq [file]` / `:x`
    WriteQuit(Option<String>),
    /// `:e file`
    Edit(String),
    /// `:theme [name]` / `:colorscheme [name]` — `None` lists/cycles.
    SetTheme(Option<String>),
    /// `:set number` / `:set nonumber`
    ToggleNumbers(bool),
    /// `:set relativenumber` / `:set norelativenumber`
    ToggleRelativeNumbers(bool),
    /// `:set autoindent` / `:set noautoindent`
    ToggleAutoIndent(bool),
    /// `:set ft=<lang>`
    SetFiletype(String),
    /// `:help`
    Help,
    /// `:version`
    Version,
    /// `:<n>` — jump to line n (1-based).
    Goto(usize),
    /// `:s/pat/rep/`, `:%s/pat/rep/g`, `:a,bs/pat/rep/`
    Substitute(SubstituteSpec),
    /// `:source <file>` — run ex-commands from a file.
    Source(String),
    /// `:noh` / `:set hlsearch|nohlsearch` — toggle search-match highlighting.
    ToggleHlSearch(bool),
    /// `:sort` / `:sort!` / `:sort u` — sort buffer lines.
    Sort { reverse: bool, unique: bool },
    /// `:ls` / `:buffers` — list open buffers.
    BufferList,
    /// `:bn` / `:bnext`
    BufferNext,
    /// `:bp` / `:bprev`
    BufferPrev,
    /// `:b <n>` — switch to buffer number n.
    Buffer(usize),
    /// `:bd` / `:bdelete`
    BufferDelete,
    /// Anything unrecognized — offered to plugins as (name, args).
    Passthrough { name: String, args: String },
    /// Empty input.
    Empty,
}

/// Parse a command line (without the leading `:`).
pub fn parse(input: &str) -> ExCommand {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ExCommand::Empty;
    }

    // Substitution, possibly with a leading range (`s/`, `%s/`, `1,5s/`).
    if let Some(sub) = parse_substitute(trimmed) {
        return sub;
    }

    // Pure line number → goto.
    if let Ok(n) = trimmed.parse::<usize>() {
        return ExCommand::Goto(n);
    }

    let (word, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((w, r)) => (w, r.trim()),
        None => (trimmed, ""),
    };
    let arg = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };

    match word {
        "w" | "write" => ExCommand::Write(arg),
        "q" | "quit" => ExCommand::Quit { force: false },
        "q!" | "quit!" => ExCommand::Quit { force: true },
        "wq" | "x" | "wq!" | "x!" => ExCommand::WriteQuit(arg),
        "e" | "edit" => match arg {
            Some(a) => ExCommand::Edit(a),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: String::new(),
            },
        },
        "theme" | "colorscheme" | "colo" => ExCommand::SetTheme(arg),
        "source" | "so" => match arg {
            Some(a) => ExCommand::Source(a),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: String::new(),
            },
        },
        "noh" | "nohl" | "nohlsearch" => ExCommand::ToggleHlSearch(false),
        "ls" | "buffers" | "files" => ExCommand::BufferList,
        "bn" | "bnext" => ExCommand::BufferNext,
        "bp" | "bprev" | "bprevious" => ExCommand::BufferPrev,
        "bd" | "bdelete" => ExCommand::BufferDelete,
        "b" | "bu" | "buf" | "buffer" => match arg.as_deref().and_then(|a| a.trim().parse::<usize>().ok()) {
            Some(n) => ExCommand::Buffer(n),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: rest.to_string(),
            },
        },
        "sort" | "sort!" | "sor" | "sor!" => {
            let reverse = word.ends_with('!');
            let unique = rest.contains('u');
            ExCommand::Sort { reverse, unique }
        }
        "help" | "h" => ExCommand::Help,
        "version" | "ver" => ExCommand::Version,
        "set" | "se" => parse_set(rest),
        _ => ExCommand::Passthrough {
            name: word.to_string(),
            args: rest.to_string(),
        },
    }
}

/// Try to parse a substitute command. Returns `None` if `trimmed` isn't a
/// `:s`-style command, so the caller can fall through to other commands.
fn parse_substitute(trimmed: &str) -> Option<ExCommand> {
    // Consume an optional leading range made of these characters.
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_digit() || matches!(c, ',' | '%' | '.' | '$') {
            i += 1;
        } else {
            break;
        }
    }
    // The command char must be exactly 's'.
    if bytes.get(i).copied() != Some(b's') {
        return None;
    }
    let range_str = &trimmed[..i];
    let after = &trimmed[i + 1..];
    // The next char is the delimiter and must be non-alphanumeric (this rejects
    // `set`, `split`, `sort`, … where 's' is just the first letter of a word).
    let delim = after.chars().next()?;
    if delim.is_alphanumeric() || delim.is_whitespace() {
        return None;
    }
    let rest = &after[delim.len_utf8()..];
    let parts: Vec<&str> = rest.splitn(3, delim).collect();
    let pattern = parts.first().copied().unwrap_or("");
    let replacement = parts.get(1).copied().unwrap_or("");
    let flags = parts.get(2).copied().unwrap_or("");
    let range = parse_range(range_str)?;
    Some(ExCommand::Substitute(SubstituteSpec {
        range,
        pattern: pattern.to_string(),
        replacement: replacement.to_string(),
        global: flags.contains('g'),
    }))
}

fn parse_range(s: &str) -> Option<SubRange> {
    let s = s.trim();
    if s.is_empty() {
        return Some(SubRange::CurrentLine);
    }
    if s == "%" {
        return Some(SubRange::WholeFile);
    }
    if let Some((a, b)) = s.split_once(',') {
        return Some(SubRange::Range(parse_addr(a)?, parse_addr(b)?));
    }
    let a = parse_addr(s)?;
    Some(SubRange::Range(a, a))
}

fn parse_addr(s: &str) -> Option<LineAddr> {
    match s.trim() {
        "." => Some(LineAddr::Current),
        "$" => Some(LineAddr::Last),
        other => other.parse::<usize>().ok().map(LineAddr::Num),
    }
}

fn parse_set(rest: &str) -> ExCommand {
    let opt = rest.trim();
    match opt {
        "number" | "nu" => ExCommand::ToggleNumbers(true),
        "nonumber" | "nonu" => ExCommand::ToggleNumbers(false),
        "relativenumber" | "rnu" => ExCommand::ToggleRelativeNumbers(true),
        "norelativenumber" | "nornu" => ExCommand::ToggleRelativeNumbers(false),
        "hlsearch" | "hls" => ExCommand::ToggleHlSearch(true),
        "nohlsearch" | "nohls" => ExCommand::ToggleHlSearch(false),
        "autoindent" | "ai" => ExCommand::ToggleAutoIndent(true),
        "noautoindent" | "noai" => ExCommand::ToggleAutoIndent(false),
        _ => {
            if let Some(v) = opt
                .strip_prefix("ft=")
                .or_else(|| opt.strip_prefix("filetype="))
                .or_else(|| opt.strip_prefix("syntax="))
            {
                ExCommand::SetFiletype(v.trim().to_string())
            } else {
                ExCommand::Passthrough {
                    name: "set".into(),
                    args: opt.to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(parse("  "), ExCommand::Empty);
    }

    #[test]
    fn write_variants() {
        assert_eq!(parse("w"), ExCommand::Write(None));
        assert_eq!(parse("w out.rs"), ExCommand::Write(Some("out.rs".into())));
        assert_eq!(parse("write foo"), ExCommand::Write(Some("foo".into())));
    }

    #[test]
    fn quit_variants() {
        assert_eq!(parse("q"), ExCommand::Quit { force: false });
        assert_eq!(parse("q!"), ExCommand::Quit { force: true });
    }

    #[test]
    fn writequit_variants() {
        assert_eq!(parse("wq"), ExCommand::WriteQuit(None));
        assert_eq!(parse("x"), ExCommand::WriteQuit(None));
        assert_eq!(parse("wq file.txt"), ExCommand::WriteQuit(Some("file.txt".into())));
    }

    #[test]
    fn goto_line() {
        assert_eq!(parse("42"), ExCommand::Goto(42));
    }

    #[test]
    fn theme() {
        assert_eq!(parse("theme cobalt"), ExCommand::SetTheme(Some("cobalt".into())));
        assert_eq!(parse("colorscheme"), ExCommand::SetTheme(None));
        assert_eq!(parse("colo matrix"), ExCommand::SetTheme(Some("matrix".into())));
    }

    #[test]
    fn set_number() {
        assert_eq!(parse("set number"), ExCommand::ToggleNumbers(true));
        assert_eq!(parse("set nonu"), ExCommand::ToggleNumbers(false));
    }

    #[test]
    fn set_relativenumber() {
        assert_eq!(parse("set relativenumber"), ExCommand::ToggleRelativeNumbers(true));
        assert_eq!(parse("set rnu"), ExCommand::ToggleRelativeNumbers(true));
        assert_eq!(parse("set nornu"), ExCommand::ToggleRelativeNumbers(false));
    }

    #[test]
    fn set_autoindent() {
        assert_eq!(parse("set autoindent"), ExCommand::ToggleAutoIndent(true));
        assert_eq!(parse("set ai"), ExCommand::ToggleAutoIndent(true));
        assert_eq!(parse("set noai"), ExCommand::ToggleAutoIndent(false));
    }

    #[test]
    fn set_filetype() {
        assert_eq!(parse("set ft=rust"), ExCommand::SetFiletype("rust".into()));
        assert_eq!(parse("set filetype=tsql"), ExCommand::SetFiletype("tsql".into()));
    }

    #[test]
    fn passthrough_for_plugins() {
        assert_eq!(
            parse("wordcount"),
            ExCommand::Passthrough {
                name: "wordcount".into(),
                args: String::new()
            }
        );
    }

    #[test]
    fn edit() {
        assert_eq!(parse("e main.rs"), ExCommand::Edit("main.rs".into()));
    }

    #[test]
    fn buffer_commands() {
        assert_eq!(parse("ls"), ExCommand::BufferList);
        assert_eq!(parse("buffers"), ExCommand::BufferList);
        assert_eq!(parse("bn"), ExCommand::BufferNext);
        assert_eq!(parse("bprev"), ExCommand::BufferPrev);
        assert_eq!(parse("bd"), ExCommand::BufferDelete);
        assert_eq!(parse("b 3"), ExCommand::Buffer(3));
        assert_eq!(parse("buffer 2"), ExCommand::Buffer(2));
    }

    #[test]
    fn sort_variants() {
        assert_eq!(parse("sort"), ExCommand::Sort { reverse: false, unique: false });
        assert_eq!(parse("sort!"), ExCommand::Sort { reverse: true, unique: false });
        assert_eq!(parse("sort u"), ExCommand::Sort { reverse: false, unique: true });
        assert_eq!(parse("sort! u"), ExCommand::Sort { reverse: true, unique: true });
    }

    #[test]
    fn nohlsearch_variants() {
        assert_eq!(parse("noh"), ExCommand::ToggleHlSearch(false));
        assert_eq!(parse("nohlsearch"), ExCommand::ToggleHlSearch(false));
        assert_eq!(parse("set hlsearch"), ExCommand::ToggleHlSearch(true));
        assert_eq!(parse("set nohls"), ExCommand::ToggleHlSearch(false));
    }

    #[test]
    fn source_command() {
        assert_eq!(parse("source ~/.rvimrc"), ExCommand::Source("~/.rvimrc".into()));
        assert_eq!(parse("so init.vim"), ExCommand::Source("init.vim".into()));
    }

    #[test]
    fn substitute_current_line() {
        assert_eq!(
            parse("s/foo/bar/"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: false,
            })
        );
    }

    #[test]
    fn substitute_whole_file_global() {
        assert_eq!(
            parse("%s/foo/bar/g"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::WholeFile,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: true,
            })
        );
    }

    #[test]
    fn substitute_numeric_range() {
        assert_eq!(
            parse("2,5s/x/y/"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(5)),
                pattern: "x".into(),
                replacement: "y".into(),
                global: false,
            })
        );
    }

    #[test]
    fn substitute_symbolic_range() {
        assert_eq!(
            parse(".,$s/a/b/g"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::Range(LineAddr::Current, LineAddr::Last),
                pattern: "a".into(),
                replacement: "b".into(),
                global: true,
            })
        );
    }

    #[test]
    fn substitute_empty_replacement_deletes() {
        assert_eq!(
            parse("s/drop//"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "drop".into(),
                replacement: "".into(),
                global: false,
            })
        );
    }

    #[test]
    fn substitute_does_not_hijack_other_commands() {
        // These start with 's' or contain digits but are not substitutions.
        assert!(!matches!(parse("set number"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("42"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("w"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("x"), ExCommand::Substitute(_)));
    }
}
