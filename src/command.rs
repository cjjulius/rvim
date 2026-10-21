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

/// Text alignment for `:left` / `:right` / `:center`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignKind {
    Left,
    Right,
    Center,
}

/// Which history list `:history` should show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// `:` ex-command history (the default).
    Cmd,
    /// `/` search-pattern history.
    Search,
    /// Both lists (`:history all`).
    All,
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
    /// `:w [file]` / `:w! [file]` (force overrides `:set readonly`)
    Write { path: Option<String>, force: bool },
    /// `:[range]w[rite][!] file` — write a line range to a file.
    WriteRange { range: SubRange, file: Option<String> },
    /// `:q` / `:q!`
    Quit { force: bool },
    /// `:wq [file]` / `:x` (and `:wq!` / `:x!`, which force past `:set readonly`)
    WriteQuit { path: Option<String>, force: bool },
    /// `:qa` / `:qall` / `:qa!` — quit all buffers.
    QuitAll { force: bool },
    /// `:wa` / `:wall` — write every modified buffer.
    WriteAll,
    /// `:wqa` / `:xa` / `:wqall` — write all buffers, then quit.
    WriteQuitAll { force: bool },
    /// `:e file` (optionally `:e +N file` to open at line N).
    Edit { path: String, line: Option<usize> },
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
    /// `:set smartindent` / `:set nosmartindent`
    ToggleSmartIndent(bool),
    /// `:set paste` / `:set nopaste`
    TogglePaste(bool),
    /// `:set readonly` / `:set noreadonly`
    ToggleReadonly(bool),
    ToggleAutowrite(bool),
    ToggleAutoread(bool),
    /// `:set title` / `:set notitle` — show the file name in the window title.
    ToggleTitle(bool),
    /// `:set backup` / `:set nobackup` — keep a `~` backup on overwrite.
    ToggleBackup(bool),
    /// `:set showmode|showcmd|ruler` (and `no…`) — status-line display toggles.
    ToggleShowMode(bool),
    ToggleShowCmd(bool),
    ToggleRuler(bool),
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
    /// `:set modeline` / `:set nomodeline` — honor in-file option comments.
    ToggleModeline(bool),
    /// `:set modelines=N` — lines scanned at each end of the file for a modeline.
    SetModelines(usize),
    /// `:set equalprg=<cmd>` — external program the `=` operator filters through
    /// (empty uses the built-in indenter).
    SetEqualprg(String),
    /// `:set textwidth=N` — wrap column for `gq` reflow (0 disables).
    SetTextWidth(usize),
    /// `:set ft=<lang>`
    SetFiletype(String),
    /// `:set language=<code>` — switch the UI language.
    SetLanguage(String),
    /// `:cd [dir]` — change the working directory (no arg: home).
    ChangeDir(Option<String>),
    /// `:pwd` — print the working directory.
    Pwd,
    /// `:set foldmethod=manual|indent` (`fdm=`).
    SetFoldMethod(String),
    /// `:set fileformat=unix|dos` (`ff=`).
    SetFileFormat(String),
    /// `:set foldcolumn=N` (`fdc=`) — width of the fold-indicator column.
    SetFoldColumn(usize),
    /// `:set splitbelow` / `:set nosplitbelow`.
    ToggleSplitBelow(bool),
    /// `:set splitright` / `:set nosplitright`.
    ToggleSplitRight(bool),
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
    /// `:set` (no args, `all` = false) or `:set all` (`all` = true) — list options.
    ShowOptions(bool),
    /// `:retab [N]` — normalise tabs/spaces to `tabstop` (optionally set to N).
    Retab(Option<usize>),
    /// `:[range]!cmd` — filter the range through a shell command (`range: None`
    /// for a bare `:!cmd`, which just runs the command).
    Filter { range: Option<SubRange>, cmd: String },
    /// `:s/pat/rep/c` — substitute with interactive confirmation per match.
    SubstituteConfirm(SubstituteSpec),
    /// `:iabbrev lhs rhs` — define an insert-mode abbreviation.
    Abbrev { lhs: String, rhs: String },
    /// `:abbreviate` (no args) — list abbreviations.
    AbbrevList,
    /// `:unabbreviate lhs` — remove an abbreviation.
    Unabbrev(String),
    /// `:nnoremap lhs rhs` — define a normal-mode key mapping (single-char lhs).
    MapKey { lhs: char, rhs: String },
    /// `:nnoremap` (no args) — list key mappings.
    MapList,
    /// `:command {Name} {expansion}` — define a user command (Name starts upper).
    DefineCommand { name: String, expansion: String },
    /// `:command` (no args) — list user commands.
    CommandList,
    /// `:delcommand {Name}` — remove a user command.
    DeleteCommand(String),
    /// `:nunmap lhs` — remove a normal-mode key mapping.
    Unmap(char),
    /// `:[range]left [indent]` / `:right [width]` / `:center [width]`.
    Align {
        range: SubRange,
        kind: AlignKind,
        width: Option<usize>,
    },
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
    ToggleGdefault(bool),
    /// `:set cursorline` / `:set nocursorline` — highlight the cursor's line.
    ToggleCursorLine(bool),
    /// `:set shiftround` / `:set noshiftround` — round `>`/`<` to a shiftwidth.
    ToggleShiftRound(bool),
    /// `:set joinspaces` / `:set nojoinspaces` — two spaces after sentence end.
    ToggleJoinSpaces(bool),
    /// `:set cursorcolumn` / `:set nocursorcolumn` — highlight the cursor's column.
    ToggleCursorColumn(bool),
    /// `:set colorcolumn=N[,M...]` — highlight these columns as guides (empty
    /// disables).
    SetColorColumn(Vec<usize>),
    /// `:set listchars=tab:xy,trail:z` — configure the `:set list` markers.
    SetListchars(String),
    /// `:set matchpairs=(:),{:}` — configure the pairs `%` jumps between.
    SetMatchPairs(String),
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
    /// `:[range]d[elete] [x]` — delete the range's lines (into register `x`).
    DeleteLines { range: SubRange, register: Option<char> },
    /// `:[range]y[ank] [x]` — yank the range's lines (into register `x`).
    YankLines { range: SubRange, register: Option<char> },
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
    /// `:sp[lit]` / `:vs[plit]` — split the focused window (horizontally, or
    /// `vertical` for side-by-side), optionally opening `file` in the new window.
    SplitWindow { vertical: bool, file: Option<String> },
    /// `:enew` — open a new empty, unnamed buffer in the current window.
    Enew,
    /// `:clo[se][!]` — close the focused window (`force` discards unsaved changes).
    CloseWindow { force: bool },
    /// `:on[ly][!]` — close every window except the focused one.
    OnlyWindow { force: bool },
    /// `:bufdo {cmd}` — run an ex command on every open buffer.
    BufDo(String),
    /// `:windo {cmd}` — run an ex command in every window.
    WinDo(String),
    /// `:ls` / `:buffers` — list open buffers.
    BufferList,
    /// `:bn` / `:bnext` / `:n` / `:next`
    BufferNext,
    /// `:bp` / `:bprev` / `:N` / `:prev`
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
    /// `:messages` — list recent status-line messages.
    Messages,
    /// `:changes` — list the change list.
    Changes,
    /// `:delmarks {marks}` — delete marks (`"!"` deletes all lowercase marks).
    DelMarks(String),
    /// `:history [:|/|all]` — list command-line / search history.
    History(HistoryKind),
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

    // Filter / run (`!cmd`, `%!sort`, `1,5!cmd`).
    if let Some(f) = parse_filter(trimmed) {
        return f;
    }

    // Range write (`1,5w file`, `%w file`). A bare `:w`/`:w file` falls through
    // to the generic word match below.
    if let Some(w) = parse_write_range(trimmed) {
        return w;
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

    // Text alignment, with an optional leading range (`center`, `1,5right 60`).
    if let Some(al) = parse_align(trimmed) {
        return al;
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
        "cd" | "chdir" | "cd!" | "chdir!" => ExCommand::ChangeDir(arg),
        "pwd" => ExCommand::Pwd,
        "w" | "write" => ExCommand::Write { path: arg, force: false },
        "w!" | "write!" => ExCommand::Write { path: arg, force: true },
        "q" | "quit" => ExCommand::Quit { force: false },
        "q!" | "quit!" => ExCommand::Quit { force: true },
        "wq" | "x" => ExCommand::WriteQuit { path: arg, force: false },
        "wq!" | "x!" => ExCommand::WriteQuit { path: arg, force: true },
        "qa" | "qall" | "quita" | "quitall" => ExCommand::QuitAll { force: false },
        "qa!" | "qall!" | "quita!" | "quitall!" => ExCommand::QuitAll { force: true },
        "wa" | "wall" => ExCommand::WriteAll,
        "wqa" | "xa" | "wqall" | "xall" => ExCommand::WriteQuitAll { force: false },
        "wqa!" | "xa!" | "wqall!" | "xall!" => ExCommand::WriteQuitAll { force: true },
        "e" | "edit" => match arg {
            Some(a) => {
                let (path, line) = parse_edit_arg(&a);
                ExCommand::Edit { path, line }
            }
            None => ExCommand::Reload { force: false },
        },
        "e!" | "edit!" => match arg {
            Some(a) => {
                let (path, line) = parse_edit_arg(&a);
                ExCommand::Edit { path, line }
            }
            None => ExCommand::Reload { force: true },
        },
        "r" | "re" | "read" => match arg {
            Some(a) => ExCommand::ReadFile(a),
            None => ExCommand::Passthrough {
                name: word.to_string(),
                args: String::new(),
            },
        },
        "iabbrev" | "iab" | "abbreviate" | "abbr" | "ab" => match arg {
            Some(a) if a.trim().contains(char::is_whitespace) => {
                let a = a.trim();
                let (lhs, rhs) = a.split_once(char::is_whitespace).unwrap();
                ExCommand::Abbrev {
                    lhs: lhs.to_string(),
                    rhs: rhs.trim().to_string(),
                }
            }
            _ => ExCommand::AbbrevList,
        },
        "iunabbrev" | "iunab" | "unabbreviate" | "unab" | "una" => match arg {
            Some(a) => ExCommand::Unabbrev(a.trim().to_string()),
            None => ExCommand::AbbrevList,
        },
        // Only the precise non-recursive, Normal-mode `:nnoremap` is supported;
        // `:map`/`:nmap`/`:noremap` are intentionally not aliased because they
        // imply recursive and/or multi-mode semantics rvim does not provide.
        "nnoremap" => match arg {
            Some(a) if a.trim().contains(char::is_whitespace) => {
                let a = a.trim();
                let (lhs, rhs) = a.split_once(char::is_whitespace).unwrap();
                match lhs.chars().next() {
                    Some(c) if lhs.chars().count() == 1 => ExCommand::MapKey {
                        lhs: c,
                        rhs: rhs.trim().to_string(),
                    },
                    _ => ExCommand::MapList, // multi-char lhs unsupported; show list
                }
            }
            _ => ExCommand::MapList,
        },
        "nunmap" => match arg.as_deref().map(str::trim) {
            Some(a) if a.chars().count() == 1 => ExCommand::Unmap(a.chars().next().unwrap()),
            _ => ExCommand::MapList,
        },
        "command" | "comm" | "command!" | "comm!" => match arg {
            Some(a) if a.trim().contains(char::is_whitespace) => {
                let a = a.trim();
                let (name, expansion) = a.split_once(char::is_whitespace).unwrap();
                ExCommand::DefineCommand {
                    name: name.to_string(),
                    expansion: expansion.trim().to_string(),
                }
            }
            _ => ExCommand::CommandList, // no args (or name only) -> list
        },
        "delcommand" | "delcom" => match arg.as_deref().map(str::trim) {
            Some(a) if !a.is_empty() => ExCommand::DeleteCommand(a.to_string()),
            _ => ExCommand::CommandList,
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
        "enew" | "ene" => ExCommand::Enew,
        "sp" | "split" | "new" => ExCommand::SplitWindow { vertical: false, file: arg },
        "vs" | "vsp" | "vsplit" | "vnew" => ExCommand::SplitWindow { vertical: true, file: arg },
        "clo" | "close" => ExCommand::CloseWindow { force: false },
        "clo!" | "close!" => ExCommand::CloseWindow { force: true },
        "on" | "only" => ExCommand::OnlyWindow { force: false },
        "on!" | "only!" => ExCommand::OnlyWindow { force: true },
        "ls" | "buffers" | "files" => ExCommand::BufferList,
        "bufdo" => ExCommand::BufDo(arg.unwrap_or_default()),
        "windo" => ExCommand::WinDo(arg.unwrap_or_default()),
        "bn" | "bnext" | "n" | "next" => ExCommand::BufferNext,
        "bp" | "bprev" | "bprevious" | "N" | "prev" | "previous" => ExCommand::BufferPrev,
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
        "mes" | "messages" | "message" => ExCommand::Messages,
        "changes" => ExCommand::Changes,
        "delmarks!" | "delm!" => ExCommand::DelMarks("!".into()),
        "delmarks" | "delm" => ExCommand::DelMarks(arg.unwrap_or_default()),
        "retab" | "ret" => {
            ExCommand::Retab(arg.as_deref().and_then(|a| a.trim().parse::<usize>().ok()))
        }
        "his" | "history" => {
            let kind = match arg.as_deref().map(str::trim) {
                Some("/") | Some("search") => HistoryKind::Search,
                Some("all") => HistoryKind::All,
                // `:`, `cmd`, empty, or anything else -> command history.
                _ => HistoryKind::Cmd,
            };
            ExCommand::History(kind)
        }
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

/// Split an `:edit` argument into a path and an optional `+N` line prefix
/// (vim's `:e +N file`). Without a valid `+N` the whole argument is the path.
fn parse_edit_arg(arg: &str) -> (String, Option<usize>) {
    let a = arg.trim();
    if let Some(rest) = a.strip_prefix('+') {
        let mut it = rest.splitn(2, char::is_whitespace);
        if let (Some(num), Some(path)) = (it.next(), it.next()) {
            if let Ok(n) = num.parse::<usize>() {
                let path = path.trim();
                if !path.is_empty() {
                    return (path.to_string(), Some(n));
                }
            }
        }
    }
    (a.to_string(), None)
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

/// Parse `:[range]w[rite][!] [file]` — write a range of lines to a file. Returns
/// `None` when there is no leading range (so the bare `:w` / `:w file` handling
/// applies) or the word after the range isn't `w`/`write`.
fn parse_write_range(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    if i == 0 {
        return None;
    }
    let after = &trimmed[i..];
    let rest = if let Some(r) = after.strip_prefix("write") {
        r
    } else if after.starts_with('w')
        && after[1..].chars().next().is_none_or(|c| c == '!' || c.is_whitespace())
    {
        &after[1..]
    } else {
        return None;
    };
    let rest = rest.strip_prefix('!').unwrap_or(rest);
    let file = rest.trim();
    let range = parse_range(&trimmed[..i])?;
    Some(ExCommand::WriteRange {
        range,
        file: (!file.is_empty()).then(|| file.to_string()),
    })
}

/// Parse `:[range]!cmd` (filter the range through a shell command) and bare
/// `:!cmd` (run only, `range: None`). Returns `None` when there's no `!` right
/// after an optional range prefix.
fn parse_filter(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let after = &trimmed[i..];
    let cmd = after.strip_prefix('!')?;
    let range_str = &trimmed[..i];
    let range = if range_str.is_empty() {
        None
    } else {
        Some(parse_range(range_str)?)
    };
    Some(ExCommand::Filter {
        range,
        cmd: cmd.trim().to_string(),
    })
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
    let spec = SubstituteSpec {
        range,
        pattern: pattern.to_string(),
        replacement: replacement.to_string(),
        global: flags.contains('g'),
        ignorecase: flags.contains('i'),
        count_only: flags.contains('n'),
    };
    // The `c` flag runs an interactive confirm instead of substituting at once.
    if flags.contains('c') {
        Some(ExCommand::SubstituteConfirm(spec))
    } else {
        Some(ExCommand::Substitute(spec))
    }
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
    // `d`/`delete`/`y`/`yank`/`j`/`join`: the leading alphabetic run names the
    // command (so `diffsplit`, `yankring`, … fall through via the `_` arm).
    let word: String = after.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = after[word.len()..].trim();
    let range = parse_range(range_str)?;
    let bang = rest.starts_with('!');
    // `:d`/`:y` accept an optional single-letter register argument (`:d a`).
    let reg_arg = match rest.chars().next() {
        Some(c) if rest.chars().count() == 1 && is_register_char(c) => Some(c),
        _ => None,
    };
    match word.as_str() {
        "d" | "delete" | "de" | "del" if rest.is_empty() || reg_arg.is_some() => {
            Some(ExCommand::DeleteLines { range, register: reg_arg })
        }
        "y" | "yank" | "ya" if rest.is_empty() || reg_arg.is_some() => {
            Some(ExCommand::YankLines { range, register: reg_arg })
        }
        "j" | "join" => Some(ExCommand::JoinLines { range, raw: bang }),
        _ => None,
    }
}

/// Whether `c` names a register usable as a `:d`/`:y` argument.
fn is_register_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '*' | '-')
}

/// Parse `:[range]left [indent]`, `:[range]right [width]`, `:[range]center
/// [width]`. Returns `None` to fall through unless the word is present and
/// properly terminated.
fn parse_align(trimmed: &str) -> Option<ExCommand> {
    let i = range_prefix_len(trimmed);
    let range_str = &trimmed[..i];
    let after = &trimmed[i..];
    // (canonical word, min abbreviation length, kind)
    let variants = [
        ("center", 2, AlignKind::Center),
        ("right", 2, AlignKind::Right),
        ("left", 2, AlignKind::Left),
    ];
    for (word, min, kind) in variants {
        // Match the full word or any prefix down to `min` chars (`ce`..`center`).
        let matched_len = (min..=word.len())
            .rev()
            .find(|&n| after.starts_with(&word[..n]));
        let Some(n) = matched_len else { continue };
        let tail = &after[n..];
        // Must end here or be followed by whitespace (rejects `centern`, `rightx`).
        match tail.chars().next() {
            None | Some(' ') | Some('\t') => {}
            _ => continue,
        }
        let arg = tail.trim();
        let width = if arg.is_empty() {
            None
        } else {
            Some(arg.parse::<usize>().ok()?)
        };
        let range = if range_str.is_empty() {
            SubRange::CurrentLine
        } else {
            parse_range(range_str)?
        };
        return Some(ExCommand::Align { range, kind, width });
    }
    None
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
    // `:set` with no option lists modified options; `:set all` lists every option.
    if opt.is_empty() {
        return ExCommand::ShowOptions(false);
    }
    if opt == "all" {
        return ExCommand::ShowOptions(true);
    }
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
        "gdefault" | "gd" => ExCommand::ToggleGdefault(true),
        "nogdefault" | "nogd" => ExCommand::ToggleGdefault(false),
        "cursorline" | "cul" => ExCommand::ToggleCursorLine(true),
        "nocursorline" | "nocul" => ExCommand::ToggleCursorLine(false),
        "cursorcolumn" | "cuc" => ExCommand::ToggleCursorColumn(true),
        "nocursorcolumn" | "nocuc" => ExCommand::ToggleCursorColumn(false),
        "shiftround" | "sr" => ExCommand::ToggleShiftRound(true),
        "noshiftround" | "nosr" => ExCommand::ToggleShiftRound(false),
        "joinspaces" | "js" => ExCommand::ToggleJoinSpaces(true),
        "nojoinspaces" | "nojs" => ExCommand::ToggleJoinSpaces(false),
        "smartindent" | "si" => ExCommand::ToggleSmartIndent(true),
        "nosmartindent" | "nosi" => ExCommand::ToggleSmartIndent(false),
        "paste" => ExCommand::TogglePaste(true),
        "nopaste" => ExCommand::TogglePaste(false),
        "readonly" | "ro" => ExCommand::ToggleReadonly(true),
        "noreadonly" | "noro" => ExCommand::ToggleReadonly(false),
        "autowrite" | "aw" => ExCommand::ToggleAutowrite(true),
        "noautowrite" | "noaw" => ExCommand::ToggleAutowrite(false),
        "autoread" | "ar" => ExCommand::ToggleAutoread(true),
        "noautoread" | "noar" => ExCommand::ToggleAutoread(false),
        "title" => ExCommand::ToggleTitle(true),
        "notitle" => ExCommand::ToggleTitle(false),
        "backup" | "bk" => ExCommand::ToggleBackup(true),
        "nobackup" | "nobk" => ExCommand::ToggleBackup(false),
        "showmode" | "smd" => ExCommand::ToggleShowMode(true),
        "noshowmode" | "nosmd" => ExCommand::ToggleShowMode(false),
        "showcmd" | "sc" => ExCommand::ToggleShowCmd(true),
        "noshowcmd" | "nosc" => ExCommand::ToggleShowCmd(false),
        "ruler" | "ru" => ExCommand::ToggleRuler(true),
        "noruler" | "noru" => ExCommand::ToggleRuler(false),
        "splitbelow" | "sb" => ExCommand::ToggleSplitBelow(true),
        "nosplitbelow" | "nosb" => ExCommand::ToggleSplitBelow(false),
        "splitright" | "spr" => ExCommand::ToggleSplitRight(true),
        "nosplitright" | "nospr" => ExCommand::ToggleSplitRight(false),
        "modeline" | "ml" => ExCommand::ToggleModeline(true),
        "nomodeline" | "noml" => ExCommand::ToggleModeline(false),
        _ => {
            if let Some(v) = opt
                .strip_prefix("listchars=")
                .or_else(|| opt.strip_prefix("lcs="))
            {
                ExCommand::SetListchars(v.to_string())
            } else if let Some(v) = opt
                .strip_prefix("matchpairs=")
                .or_else(|| opt.strip_prefix("mps="))
            {
                ExCommand::SetMatchPairs(v.to_string())
            } else if let Some(v) = opt
                .strip_prefix("equalprg=")
                .or_else(|| opt.strip_prefix("ep="))
            {
                ExCommand::SetEqualprg(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("language=")
                .or_else(|| opt.strip_prefix("lang="))
            {
                ExCommand::SetLanguage(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("ft=")
                .or_else(|| opt.strip_prefix("filetype="))
                .or_else(|| opt.strip_prefix("syntax="))
            {
                ExCommand::SetFiletype(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("foldmethod=")
                .or_else(|| opt.strip_prefix("fdm="))
            {
                ExCommand::SetFoldMethod(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("fileformat=")
                .or_else(|| opt.strip_prefix("ff="))
            {
                ExCommand::SetFileFormat(v.trim().to_string())
            } else if let Some(v) = opt
                .strip_prefix("foldcolumn=")
                .or_else(|| opt.strip_prefix("fdc="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetFoldColumn(n),
                    _ => unknown_set(opt),
                }
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
                .strip_prefix("modelines=")
                .or_else(|| opt.strip_prefix("mls="))
            {
                match v.trim().parse::<usize>() {
                    Ok(n) => ExCommand::SetModelines(n),
                    _ => unknown_set(opt),
                }
            } else if let Some(v) = opt
                .strip_prefix("colorcolumn=")
                .or_else(|| opt.strip_prefix("cc="))
            {
                // Comma-separated columns; 0/invalid entries are dropped, so
                // `cc=0` or `cc=` clears.
                let cols: Vec<usize> = v
                    .split(',')
                    .filter_map(|p| p.trim().parse::<usize>().ok())
                    .filter(|&n| n > 0)
                    .collect();
                ExCommand::SetColorColumn(cols)
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
