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
    /// A mark address (`'a`, `'<`, `'>`): the line holding that mark.
    Mark(char),
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
    /// The `i` flag — case-insensitive matching.
    pub ignorecase: bool,
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
    /// `:qa` / `:qall` / `:qa!` — quit all buffers.
    QuitAll { force: bool },
    /// `:wa` / `:wall` — write every modified buffer.
    WriteAll,
    /// `:wqa` / `:xa` / `:wqall` — write all buffers, then quit.
    WriteQuitAll { force: bool },
    /// `:e file`
    Edit(String),
    /// `:r[ead] file` — insert the file's contents below the cursor line.
    ReadFile(String),
    /// `:theme [name]` / `:colorscheme [name]` — `None` lists/cycles.
    SetTheme(Option<String>),
    /// `:set number` / `:set nonumber`
    ToggleNumbers(bool),
    /// `:set relativenumber` / `:set norelativenumber`
    ToggleRelativeNumbers(bool),
    /// `:set autoindent` / `:set noautoindent`
    ToggleAutoIndent(bool),
    /// `:set expandtab` / `:set noexpandtab`
    ToggleExpandTab(bool),
    /// `:set shiftwidth=N`
    SetShiftWidth(usize),
    /// `:set tabstop=N`
    SetTabStop(usize),
    /// `:set scrolloff=N` — minimum lines of context kept above/below the cursor.
    SetScrollOff(usize),
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
    /// `:g/re/cmd`, `:g!/re/cmd`, `:v/re/cmd` — run `command` on lines matching
    /// (or, when `invert`, not matching) `pattern`.
    Global {
        pattern: String,
        invert: bool,
        command: String,
    },
    /// `:noh` / `:set hlsearch|nohlsearch` — toggle search-match highlighting.
    ToggleHlSearch(bool),
    /// `:set ignorecase` / `:set noignorecase` — case-insensitive search.
    ToggleIgnoreCase(bool),
    /// `:set smartcase` / `:set nosmartcase` — uppercase in pattern forces
    /// case-sensitive search (only meaningful with `ignorecase`).
    ToggleSmartCase(bool),
    /// `:set incsearch` / `:set noincsearch` — preview the first match while typing.
    ToggleIncSearch(bool),
    /// `:sort` / `:sort!` / `:sort u` — sort buffer lines.
    Sort { reverse: bool, unique: bool },
    /// `:[range]m[ove] {addr}` — move the range's lines to after `dest`.
    MoveLines { range: SubRange, dest: LineAddr },
    /// `:[range]t`/`:[range]co[py] {addr}` — copy the range's lines to after `dest`.
    CopyLines { range: SubRange, dest: LineAddr },
    /// `:[range]d[elete]` — delete the range's lines.
    DeleteLines(SubRange),
    /// `:[range]y[ank]` — yank the range's lines.
    YankLines(SubRange),
    /// `:[range]>` / `:[range]<` — shift the range right/left by `times` steps.
    ShiftLines { range: SubRange, dedent: bool, times: usize },
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

    // Global command (`g/re/cmd`, `v/re/cmd`).
    if let Some(g) = parse_global(trimmed) {
        return g;
    }

    // Substitution, possibly with a leading range (`s/`, `%s/`, `1,5s/`).
    if let Some(sub) = parse_substitute(trimmed) {
        return sub;
    }

    // Line move/copy, possibly with a leading range (`m0`, `1,5t$`, `.co.`).
    if let Some(mc) = parse_move_copy(trimmed) {
        return mc;
    }

    // Line delete/yank/shift, possibly with a leading range (`1,5d`, `%y`, `>>`).
    if let Some(op) = parse_line_op(trimmed) {
        return op;
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
        "qa" | "qall" | "quita" | "quitall" => ExCommand::QuitAll { force: false },
        "qa!" | "qall!" | "quita!" | "quitall!" => ExCommand::QuitAll { force: true },
        "wa" | "wall" => ExCommand::WriteAll,
        "wqa" | "xa" | "wqall" | "xall" => ExCommand::WriteQuitAll { force: false },
        "wqa!" | "xa!" | "wqall!" | "xall!" => ExCommand::WriteQuitAll { force: true },
        "e" | "edit" => match arg {
            Some(a) => ExCommand::Edit(a),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: String::new(),
            },
        },
        "r" | "re" | "read" => match arg {
            Some(a) => ExCommand::ReadFile(a),
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

/// Try to parse a `:g`/`:v` global command.
#[allow(clippy::question_mark)]
fn parse_global(trimmed: &str) -> Option<ExCommand> {
    let (invert, after) = if let Some(r) = trimmed.strip_prefix("global!") {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix("g!") {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix("vglobal") {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix("global") {
        (false, r)
    } else if let Some(r) = trimmed.strip_prefix('v') {
        (true, r)
    } else if let Some(r) = trimmed.strip_prefix('g') {
        (false, r)
    } else {
        return None;
    };
    // The delimiter follows immediately and must be non-alphanumeric (rejects
    // `version`, `goto`, …).
    let delim = after.chars().next()?;
    if delim.is_alphanumeric() || delim.is_whitespace() {
        return None;
    }
    let rest = &after[delim.len_utf8()..];
    let (pattern, command) = match rest.find(delim) {
        Some(i) => (rest[..i].to_string(), rest[i + delim.len_utf8()..].to_string()),
        None => (rest.to_string(), String::new()),
    };
    if pattern.is_empty() {
        return None;
    }
    Some(ExCommand::Global {
        pattern,
        invert,
        command: command.trim().to_string(),
    })
}

/// Try to parse a substitute command. Returns `None` if `trimmed` isn't a
/// `:s`-style command, so the caller can fall through to other commands.
/// Byte length of a leading line range (`1,5`, `%`, `.`, `$`, `'a`, `'<,'>`, …)
/// at the start of `s`. Mark addresses consume the quote and the name char.
fn range_prefix_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\'' {
            i += 1; // the quote
            if i < bytes.len() {
                i += 1; // the mark name (ASCII)
            }
        } else if c.is_ascii_digit() || matches!(c, ',' | '%' | '.' | '$') {
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn parse_substitute(trimmed: &str) -> Option<ExCommand> {
    // Consume an optional leading range.
    let bytes = trimmed.as_bytes();
    let i = range_prefix_len(trimmed);
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
        ignorecase: flags.contains('i'),
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

/// Parse `:[range]{m|move|t|co|copy} {dest}`. Returns `None` (so the caller falls
/// through) unless the command word and a valid destination address are present.
fn parse_move_copy(trimmed: &str) -> Option<ExCommand> {
    // Consume an optional leading range.
    let i = range_prefix_len(trimmed);
    let range_str = &trimmed[..i];
    let after = &trimmed[i..];
    // Identify the command word (longest match first) and whether it's a copy.
    let (copy, tok_len) = if let Some(r) = after.strip_prefix("move") {
        let _ = r;
        (false, 4)
    } else if after.starts_with("copy") {
        (true, 4)
    } else if after.starts_with("co") {
        (true, 2)
    } else if after.starts_with('m') {
        (false, 1)
    } else if after.starts_with('t') {
        (true, 1)
    } else {
        return None;
    };
    let dest = parse_addr(after[tok_len..].trim())?;
    let range = parse_range(range_str)?;
    if copy {
        Some(ExCommand::CopyLines { range, dest })
    } else {
        Some(ExCommand::MoveLines { range, dest })
    }
}

/// Parse `:[range]{d|delete|y|yank}` and `:[range]{>|<}...`. Returns `None` (so
/// the caller falls through) unless a recognized line operator is present.
fn parse_line_op(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let range_str = &trimmed[..i];
    let after = trimmed[i..].trim_start();
    if after.is_empty() {
        return None;
    }
    let first = after.chars().next()?;
    // `>`/`<` shift, repeated for extra steps (`>>` = two).
    if first == '>' || first == '<' {
        let times = after.chars().take_while(|&c| c == first).count();
        let rest = after[times..].trim();
        if !rest.is_empty() {
            return None;
        }
        let range = parse_range(range_str)?;
        return Some(ExCommand::ShiftLines {
            range,
            dedent: first == '<',
            times,
        });
    }
    // `d`/`delete`/`y`/`yank`: the leading alphabetic run must be exactly one of
    // these (so `diffsplit`, `yankring`, … fall through), trailing args ignored.
    let word: String = after.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = after[word.len()..].trim_start();
    // Reject a trailing word character run masquerading as args.
    if rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let range = parse_range(range_str)?;
    match word.as_str() {
        "d" | "delete" | "de" | "del" => Some(ExCommand::DeleteLines(range)),
        "y" | "yank" | "ya" => Some(ExCommand::YankLines(range)),
        _ => None,
    }
}

fn parse_addr(s: &str) -> Option<LineAddr> {
    let s = s.trim();
    match s {
        "." => Some(LineAddr::Current),
        "$" => Some(LineAddr::Last),
        _ => {
            // A mark address: `'` followed by a single mark name (`'a`, `'<`, `'>`).
            if let Some(rest) = s.strip_prefix('\'') {
                let mut chars = rest.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    return Some(LineAddr::Mark(c));
                }
                return None;
            }
            s.parse::<usize>().ok().map(LineAddr::Num)
        }
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
        "expandtab" | "et" => ExCommand::ToggleExpandTab(true),
        "noexpandtab" | "noet" => ExCommand::ToggleExpandTab(false),
        "ignorecase" | "ic" => ExCommand::ToggleIgnoreCase(true),
        "noignorecase" | "noic" => ExCommand::ToggleIgnoreCase(false),
        "smartcase" | "scs" => ExCommand::ToggleSmartCase(true),
        "nosmartcase" | "noscs" => ExCommand::ToggleSmartCase(false),
        "incsearch" | "is" => ExCommand::ToggleIncSearch(true),
        "noincsearch" | "nois" => ExCommand::ToggleIncSearch(false),
        _ => {
            if let Some(v) = opt
                .strip_prefix("ft=")
                .or_else(|| opt.strip_prefix("filetype="))
                .or_else(|| opt.strip_prefix("syntax="))
            {
                ExCommand::SetFiletype(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("shiftwidth=")
                .or_else(|| opt.strip_prefix("sw="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) if n > 0 => ExCommand::SetShiftWidth(n),
                    _ => unknown_set(opt),
                }
            } else if let Some(v) = opt
                .strip_prefix("tabstop=")
                .or_else(|| opt.strip_prefix("ts="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) if n > 0 => ExCommand::SetTabStop(n),
                    _ => unknown_set(opt),
                }
            } else if let Some(v) = opt
                .strip_prefix("scrolloff=")
                .or_else(|| opt.strip_prefix("so="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetScrollOff(n),
                    _ => unknown_set(opt),
                }
            } else {
                unknown_set(opt)
            }
        }
    }
}

fn unknown_set(opt: &str) -> ExCommand {
    ExCommand::Passthrough {
        name: "set".into(),
        args: opt.to_string(),
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
    fn move_and_copy_variants() {
        assert_eq!(
            parse("m0"),
            ExCommand::MoveLines { range: SubRange::CurrentLine, dest: LineAddr::Num(0) }
        );
        assert_eq!(
            parse("1,3m$"),
            ExCommand::MoveLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                dest: LineAddr::Last
            }
        );
        assert_eq!(
            parse("t."),
            ExCommand::CopyLines { range: SubRange::CurrentLine, dest: LineAddr::Current }
        );
        assert_eq!(
            parse("2,4copy0"),
            ExCommand::CopyLines {
                range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(4)),
                dest: LineAddr::Num(0)
            }
        );
        // `colo`/`colorscheme` must still reach the theme command, not copy.
        assert_eq!(parse("colo"), ExCommand::SetTheme(None));
    }

    #[test]
    fn mark_and_visual_range_addresses() {
        assert_eq!(
            parse("'<,'>d"),
            ExCommand::DeleteLines(SubRange::Range(
                LineAddr::Mark('<'),
                LineAddr::Mark('>')
            ))
        );
        assert_eq!(
            parse("'a,'bm0"),
            ExCommand::MoveLines {
                range: SubRange::Range(LineAddr::Mark('a'), LineAddr::Mark('b')),
                dest: LineAddr::Num(0)
            }
        );
        // A marked range also drives :s.
        match parse("'<,'>s/x/y/") {
            ExCommand::Substitute(spec) => assert_eq!(
                spec.range,
                SubRange::Range(LineAddr::Mark('<'), LineAddr::Mark('>'))
            ),
            other => panic!("expected substitute, got {other:?}"),
        }
    }

    #[test]
    fn line_op_variants() {
        assert_eq!(parse("d"), ExCommand::DeleteLines(SubRange::CurrentLine));
        assert_eq!(
            parse("1,5d"),
            ExCommand::DeleteLines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(5)))
        );
        assert_eq!(parse("%y"), ExCommand::YankLines(SubRange::WholeFile));
        assert_eq!(parse("delete"), ExCommand::DeleteLines(SubRange::CurrentLine));
        assert_eq!(
            parse(">>"),
            ExCommand::ShiftLines { range: SubRange::CurrentLine, dedent: false, times: 2 }
        );
        assert_eq!(
            parse("1,3<"),
            ExCommand::ShiftLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                dedent: true,
                times: 1
            }
        );
        // Longer d-/y-words must not be swallowed by :d / :y.
        assert!(matches!(parse("diffsplit"), ExCommand::Passthrough { .. }));
        assert_eq!(parse("bd"), ExCommand::BufferDelete);
    }

    #[test]
    fn write_quit_all_variants() {
        assert_eq!(parse("qa"), ExCommand::QuitAll { force: false });
        assert_eq!(parse("qall"), ExCommand::QuitAll { force: false });
        assert_eq!(parse("qa!"), ExCommand::QuitAll { force: true });
        assert_eq!(parse("wa"), ExCommand::WriteAll);
        assert_eq!(parse("wall"), ExCommand::WriteAll);
        assert_eq!(parse("wqa"), ExCommand::WriteQuitAll { force: false });
        assert_eq!(parse("xa"), ExCommand::WriteQuitAll { force: false });
        assert_eq!(parse("wqa!"), ExCommand::WriteQuitAll { force: true });
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
    fn set_indentation_options() {
        assert_eq!(parse("set expandtab"), ExCommand::ToggleExpandTab(true));
        assert_eq!(parse("set noet"), ExCommand::ToggleExpandTab(false));
        assert_eq!(parse("set shiftwidth=2"), ExCommand::SetShiftWidth(2));
        assert_eq!(parse("set sw=8"), ExCommand::SetShiftWidth(8));
        assert_eq!(parse("set tabstop=4"), ExCommand::SetTabStop(4));
        assert_eq!(parse("set ts=2"), ExCommand::SetTabStop(2));
    }

    #[test]
    fn set_scrolloff() {
        assert_eq!(parse("set scrolloff=5"), ExCommand::SetScrollOff(5));
        assert_eq!(parse("set so=3"), ExCommand::SetScrollOff(3));
        assert_eq!(parse("set scrolloff=0"), ExCommand::SetScrollOff(0));
    }

    #[test]
    fn set_case_options() {
        assert_eq!(parse("set ignorecase"), ExCommand::ToggleIgnoreCase(true));
        assert_eq!(parse("set ic"), ExCommand::ToggleIgnoreCase(true));
        assert_eq!(parse("set noignorecase"), ExCommand::ToggleIgnoreCase(false));
        assert_eq!(parse("set smartcase"), ExCommand::ToggleSmartCase(true));
        assert_eq!(parse("set scs"), ExCommand::ToggleSmartCase(true));
        assert_eq!(parse("set noscs"), ExCommand::ToggleSmartCase(false));
        assert_eq!(parse("set incsearch"), ExCommand::ToggleIncSearch(true));
        assert_eq!(parse("set is"), ExCommand::ToggleIncSearch(true));
        assert_eq!(parse("set noincsearch"), ExCommand::ToggleIncSearch(false));
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
    fn read_file_variants() {
        assert_eq!(parse("r notes.txt"), ExCommand::ReadFile("notes.txt".into()));
        assert_eq!(parse("read data.csv"), ExCommand::ReadFile("data.csv".into()));
        // No file name falls through to the plugin passthrough, not a crash.
        assert!(matches!(parse("r"), ExCommand::Passthrough { .. }));
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
                ignorecase: false,
            })
        );
    }

    #[test]
    fn global_commands() {
        assert_eq!(
            parse("g/foo/d"),
            ExCommand::Global { pattern: "foo".into(), invert: false, command: "d".into() }
        );
        assert_eq!(
            parse("v/foo/d"),
            ExCommand::Global { pattern: "foo".into(), invert: true, command: "d".into() }
        );
        assert_eq!(
            parse("g!/bar/d"),
            ExCommand::Global { pattern: "bar".into(), invert: true, command: "d".into() }
        );
        assert_eq!(
            parse("g/x/s/a/b/g"),
            ExCommand::Global { pattern: "x".into(), invert: false, command: "s/a/b/g".into() }
        );
    }

    #[test]
    fn global_does_not_hijack_other_commands() {
        assert!(!matches!(parse("version"), ExCommand::Global { .. }));
        assert!(!matches!(parse("wq"), ExCommand::Global { .. }));
    }

    #[test]
    fn substitute_ignorecase_flag() {
        assert_eq!(
            parse("s/foo/bar/gi"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: true,
                ignorecase: true,
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
                ignorecase: false,
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
                ignorecase: false,
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
                ignorecase: false,
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
                ignorecase: false,
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
