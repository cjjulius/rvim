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
    /// The `n` flag — report the match count without substituting.
    pub count_only: bool,
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
    /// `:e` / `:e!` with no file — reload the current file (`force` discards changes).
    Reload { force: bool },
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
    /// `:set sidescrolloff=N` — minimum columns of context kept left/right.
    SetSideScrollOff(usize),
    /// `:set textwidth=N` — wrap column for `gq` reflow (0 disables).
    SetTextWidth(usize),
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
    /// `:set {option}?` — show the option's current value.
    SetQuery(String),
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
    /// `:set list` / `:set nolist` — show tabs and trailing whitespace.
    ToggleList(bool),
    /// `:set wrapscan` / `:set nowrapscan` — whether searches wrap around the file.
    ToggleWrapScan(bool),
    /// `:set cursorline` / `:set nocursorline` — highlight the cursor's line.
    ToggleCursorLine(bool),
    /// `:set cursorcolumn` / `:set nocursorcolumn` — highlight the cursor's column.
    ToggleCursorColumn(bool),
    /// `:set colorcolumn=N` — highlight column N as a guide (0 disables).
    SetColorColumn(usize),
    /// `:sort` / `:sort!` / `:sort u` — sort buffer lines.
    Sort {
        range: SubRange,
        reverse: bool,
        unique: bool,
        numeric: bool,
        ignorecase: bool,
        /// Optional `/pattern/` to derive the sort key from.
        pattern: Option<String>,
        /// With a pattern, the `r` flag sorts on the matched text itself rather
        /// than on what follows it.
        use_match: bool,
    },
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
    /// `:[range]j[oin][!]` — join the range's lines (`!` keeps whitespace, like `gJ`).
    JoinLines { range: SubRange, raw: bool },
    /// `:[addr]pu[t] [reg]` — put a register's text as lines after `dest`
    /// (`register` is `None` for the unnamed register).
    PutRegister { dest: LineAddr, register: Option<char> },
    /// `:[range]norm[al] {keys}` — run `keys` as Normal-mode input, once at the
    /// cursor (`range` is `None`) or on every line of the range.
    Normal { range: Option<SubRange>, keys: String },
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
    /// `:b#` / `:e#` — switch to the alternate buffer.
    BufferAlternate,
    /// `:marks` — list the marks.
    Marks,
    /// `:reg` / `:registers` — list the registers.
    Registers,
    /// `:jumps` — list the jump list.
    Jumps,
    /// `:earlier [N]` — undo N times (default 1).
    Earlier(usize),
    /// `:later [N]` — redo N times (default 1).
    Later(usize),
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

    // Put a register, with an optional leading address (`put`, `0put`, `3put x`).
    if let Some(p) = parse_put(trimmed) {
        return p;
    }

    // Sort, with an optional leading range (`sort`, `%sort n`, `'<,'>sort u`).
    if let Some(s) = parse_sort(trimmed) {
        return s;
    }

    // `:[range]normal {keys}` — run keys as Normal-mode input.
    if let Some(n) = parse_normal(trimmed) {
        return n;
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
            None => ExCommand::Reload { force: false },
        },
        "e!" | "edit!" => match arg {
            Some(a) => ExCommand::Edit(a),
            None => ExCommand::Reload { force: true },
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
        "b#" | "e#" => ExCommand::BufferAlternate,
        "b" | "bu" | "buf" | "buffer" if arg.as_deref() == Some("#") => {
            ExCommand::BufferAlternate
        }
        "b" | "bu" | "buf" | "buffer" => match arg.as_deref().and_then(|a| a.trim().parse::<usize>().ok()) {
            Some(n) => ExCommand::Buffer(n),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: rest.to_string(),
            },
        },
        "marks" => ExCommand::Marks,
        "reg" | "registers" | "display" | "di" => ExCommand::Registers,
        "ju" | "jumps" => ExCommand::Jumps,
        "earlier" | "ea" => ExCommand::Earlier(parse_count_arg(&arg)),
        "later" | "lat" => ExCommand::Later(parse_count_arg(&arg)),
        "help" | "h" => ExCommand::Help,
        "version" | "ver" => ExCommand::Version,
        "set" | "se" => parse_set(rest),
        _ => ExCommand::Passthrough {
            name: word.to_string(),
            args: rest.to_string(),
        },
    }
}

/// Parse an optional leading count from a command argument, defaulting to 1
/// (for `:earlier` / `:later`).
fn parse_count_arg(arg: &Option<String>) -> usize {
    arg.as_deref()
        .and_then(|a| a.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(1)
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
        count_only: flags.contains('n'),
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
    let bang = rest.starts_with('!');
    match word.as_str() {
        "d" | "delete" | "de" | "del" => Some(ExCommand::DeleteLines(range)),
        "y" | "yank" | "ya" => Some(ExCommand::YankLines(range)),
        "j" | "join" => Some(ExCommand::JoinLines { range, raw: bang }),
        _ => None,
    }
}

/// Parse `:[range]sort[!] [flags]`. Bare `:sort` sorts the whole file; a leading
/// range limits it. Returns `None` (so the caller falls through) unless the
/// `sort`/`sor` word is present and properly terminated.
fn parse_sort(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let range_str = &trimmed[..i];
    let after = &trimmed[i..];
    let base = if after.starts_with("sort") {
        4
    } else if after.starts_with("sor") {
        3
    } else {
        return None;
    };
    let tail = &after[base..];
    // The word must end here, or be followed by `!`, or whitespace (rejects
    // `source`, `sortfoo`, …).
    match tail.chars().next() {
        None | Some('!') | Some(' ') | Some('\t') => {}
        _ => return None,
    }
    let reverse = tail.starts_with('!');
    let flags = if reverse { &tail[1..] } else { tail };
    // Pull out a `/pattern/` if present; the remaining characters are flags.
    let (pattern, flagstr) = match flags.find('/') {
        Some(start) => {
            let rest = &flags[start + 1..];
            match rest.find('/') {
                Some(end) => (
                    Some(rest[..end].to_string()),
                    format!("{}{}", &flags[..start], &rest[end + 1..]),
                ),
                None => (Some(rest.to_string()), flags[..start].to_string()),
            }
        }
        None => (None, flags.to_string()),
    };
    let range = if range_str.is_empty() {
        SubRange::WholeFile
    } else {
        parse_range(range_str)?
    };
    Some(ExCommand::Sort {
        range,
        reverse,
        unique: flagstr.contains('u'),
        numeric: flagstr.contains('n'),
        ignorecase: flagstr.contains('i'),
        pattern: pattern.filter(|p| !p.is_empty()),
        use_match: flagstr.contains('r'),
    })
}

/// Parse `:[range]norm[al][!] {keys}`. The keys are taken verbatim after exactly
/// one space. Returns `None` (so the caller falls through) unless the word is
/// present and properly terminated.
fn parse_normal(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let range_str = &trimmed[..i];
    let after = &trimmed[i..];
    let base = if after.starts_with("normal") {
        6
    } else if after.starts_with("norm") {
        4
    } else {
        return None;
    };
    let mut rest = &after[base..];
    if let Some(r) = rest.strip_prefix('!') {
        rest = r; // `:normal!` ignores mappings (we have none, so same behavior)
    }
    // The keys follow after exactly one space; anything else (e.g. `normalize`)
    // isn't this command.
    let keys = match rest.strip_prefix(' ') {
        Some(k) => k.to_string(),
        None if rest.is_empty() => return None, // nothing to run
        None => return None,
    };
    if keys.is_empty() {
        return None;
    }
    let range = if range_str.is_empty() {
        None
    } else {
        Some(parse_range(range_str)?)
    };
    Some(ExCommand::Normal { range, keys })
}

/// Parse `:[addr]pu[t] [reg]`. Returns `None` (so the caller falls through)
/// unless the `put`/`pu` word is present.
fn parse_put(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let addr_str = &trimmed[..i];
    let after = trimmed[i..].trim_start();
    let word: String = after.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if word != "put" && word != "pu" {
        return None;
    }
    let rest = after[word.len()..].trim();
    let register = rest.chars().next();
    let dest = if addr_str.is_empty() {
        LineAddr::Current
    } else {
        parse_addr(addr_str)?
    };
    Some(ExCommand::PutRegister { dest, register })
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
    // `:set opt?` queries the current value.
    if let Some(name) = opt.strip_suffix('?') {
        return ExCommand::SetQuery(name.trim().to_string());
    }
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
        "list" => ExCommand::ToggleList(true),
        "nolist" => ExCommand::ToggleList(false),
        "wrapscan" | "ws" => ExCommand::ToggleWrapScan(true),
        "nowrapscan" | "nows" => ExCommand::ToggleWrapScan(false),
        "cursorline" | "cul" => ExCommand::ToggleCursorLine(true),
        "nocursorline" | "nocul" => ExCommand::ToggleCursorLine(false),
        "cursorcolumn" | "cuc" => ExCommand::ToggleCursorColumn(true),
        "nocursorcolumn" | "nocuc" => ExCommand::ToggleCursorColumn(false),
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
                .strip_prefix("sidescrolloff=")
                .or_else(|| opt.strip_prefix("siso="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetSideScrollOff(n),
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
            } else if let Some(v) = opt
                .strip_prefix("colorcolumn=")
                .or_else(|| opt.strip_prefix("cc="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetColorColumn(n),
                    _ => unknown_set(opt),
                }
            } else if let Some(v) = opt
                .strip_prefix("textwidth=")
                .or_else(|| opt.strip_prefix("tw="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetTextWidth(n),
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
#[path = "command_tests.rs"]
mod tests;
