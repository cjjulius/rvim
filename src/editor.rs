//! The editor state machine: cursor, viewport, motions and edit operations.
//!
//! `Editor` owns the buffer and all *self-contained* behavior (Normal / Insert /
//! Visual editing, incremental search). Ex commands (`:...`) touch other
//! subsystems (themes, plugins), so [`handle_key`](Editor::handle_key) returns
//! an [`Action`] the [`crate::app::App`] executes.

use crate::buffer::{Buffer, Position};
use crate::command::{AlignKind, HistoryKind, LineAddr, SubRange, SubstituteSpec};
use crate::menu::{MenuOutcome, MenuState};
use crate::mode::Mode;
use crate::pattern;
use std::collections::{HashMap, HashSet};
use crate::syntax::{detect_language, line_comment_token, Language};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;

/// What the app should do after the editor handled a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing further required.
    None,
    /// Execute this ex command (text after the leading `:`).
    RunEx(String),
}

/// A yank/delete register.
#[derive(Debug, Clone, Default)]
struct Register {
    text: String,
    linewise: bool,
}

/// Active insert-mode keyword completion (`Ctrl-n`/`Ctrl-p`): the column where
/// the replaced word starts, the candidate list, and the current index.
struct Completion {
    start_col: usize,
    candidates: Vec<String>,
    idx: usize,
}

/// State for an interactive `:s///c` session: the compiled pattern, the
/// replacement (regex-crate syntax), the range, the current scan position, and
/// running tallies. `match_bytes` is the current match on `row` awaiting a y/n.
struct SubstConfirm {
    re: Regex,
    repl: String,
    display: String,
    global: bool,
    end_row: usize,
    row: usize,
    byte_col: usize,
    match_bytes: Option<(usize, usize)>,
    count: usize,
    lines: HashSet<usize>,
    checkpointed: bool,
}

/// Word character for keyword completion (identifier characters).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The kind of text being entered on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Ex,
    SearchFwd,
    SearchBack,
}

/// The full editor state.
pub struct Editor {
    pub buffer: Buffer,
    pub cursor: Position,
    pub mode: Mode,
    pub top: usize,
    pub left: usize,
    pub language: Language,
    pub message: String,
    pub cmdline: String,
    pub show_line_numbers: bool,
    /// When true, non-current lines show their distance from the cursor
    /// (hybrid: the current line still shows its absolute number).
    pub relative_numbers: bool,
    /// Whether search matches are currently highlighted (`:noh` clears it until
    /// the next search).
    pub hlsearch: bool,
    /// Case-insensitive search (`:set ignorecase`).
    pub ignorecase: bool,
    /// With `ignorecase`, searches stay insensitive only while the pattern is
    /// all-lowercase; an uppercase letter makes it case-sensitive (`:set smartcase`).
    pub smartcase: bool,
    /// Preview the first match while typing a `/` or `?` search (`:set incsearch`).
    pub incsearch: bool,
    /// Whether searches wrap around the ends of the buffer (`:set wrapscan`).
    pub wrapscan: bool,
    /// Direction of the last search, so `n` repeats it and `N` reverses it
    /// (e.g. after `?foo`, `n` searches backward).
    search_forward: bool,
    /// Cursor position when a search was started, for incsearch preview/restore.
    search_origin: Position,
    /// Search highlight state saved on search entry, restored if the search is
    /// cancelled with Esc (so an incsearch preview leaves no trace).
    saved_search_re: Option<Regex>,
    saved_last_search: String,
    saved_hlsearch: bool,
    /// Whether Enter in insert mode copies the previous line's indentation.
    pub autoindent: bool,
    /// Insert spaces instead of a tab character (`:set expandtab`).
    pub expandtab: bool,
    /// Columns inserted/removed by `>>`/`<<` and `=` (`:set shiftwidth`).
    pub shiftwidth: usize,
    /// `:set shiftround` — round `>`/`<` indent to a multiple of `shiftwidth`.
    pub shiftround: bool,
    /// Visual width of a tab, and spaces inserted by Tab (`:set tabstop`).
    pub tabstop: usize,
    pub view_rows: usize,
    pub view_cols: usize,
    /// `:set scrolloff` — minimum lines of context kept above/below the cursor.
    pub scrolloff: usize,
    /// `:set sidescrolloff` — minimum columns of context kept left/right.
    pub sidescrolloff: usize,

    line_kind: LineKind,
    /// Ex-command and search history for Up/Down recall on the command line.
    cmd_history: Vec<String>,
    search_history: Vec<String>,
    hist_idx: Option<usize>,
    hist_saved: String,
    /// Active command-line Tab-completion cycle, if any.
    cmd_comp: Option<CmdComp>,
    register: Register,
    registers: HashMap<char, Register>,
    pending_register: Option<char>,
    expect_register: bool,
    visual_anchor: Position,
    /// The last visual selection (start, end, mode) for `gv`.
    last_visual: Option<(Position, Position, Mode)>,
    /// Where insert mode last ended, for `gi` and the `` `^ `` mark.
    last_insert: Position,
    last_search: String,
    search_re: Option<Regex>,
    /// Search offset from `/pat/e`, `/pat/+N`, … reapplied by `n`/`N`.
    search_offset: Option<SearchOffset>,
    last_subst: Option<SubstituteSpec>,
    pending_count: Option<usize>,
    pending_op: Option<char>,
    pending_op_count: Option<usize>,
    /// After `d`/`y`/`c` + `i`/`a`: the (operator, i-or-a, count) awaiting an
    /// object char. The count (e.g. `3` in `d3iw`) extends word/paragraph objects.
    pending_textobj: Option<(char, char, usize)>,
    /// In visual mode, `i`/`a` awaiting an object char (e.g. `viw`, `vi(`).
    pending_vis_obj: Option<char>,
    /// After `d`/`y`/`c` + `g`: the operator awaiting the second `g` (e.g. `dgg`).
    pending_op_gg: Option<char>,
    /// After `gu`/`gU`/`g~`: a case operator awaiting a motion/object.
    pending_case: Option<CaseOp>,
    /// After a case operator + `i`/`a`: awaiting an object char.
    pending_case_obj: Option<(CaseOp, char)>,
    /// After `gc`: a comment-toggle operator awaiting a motion.
    pending_comment: bool,
    /// `gq` was pressed, awaiting a motion (or `q`) that selects lines to reflow.
    pending_format: bool,
    /// `gq` + `i`/`a` was pressed, awaiting the text-object char (e.g. `gqip`).
    pending_format_obj: Option<char>,
    /// `:set textwidth` — wrap column for `gq` reflow (0 means use 79).
    pub textwidth: usize,
    /// `:set list` — show tabs and trailing whitespace with markers.
    pub list: bool,
    /// `:set listchars` — the tab marker `(head, fill)`, the trailing-space
    /// marker, and the end-of-line marker used when `list` is on.
    pub listchars_tab: (char, char),
    pub listchars_trail: char,
    pub listchars_eol: char,
    /// `:set cursorline` — highlight the line the cursor is on (default on).
    pub cursorline: bool,
    /// `:set cursorcolumn` — highlight the column the cursor is on (default off).
    pub cursorcolumn: bool,
    /// `:set colorcolumn` — 1-based column to highlight as a guide (0 = off).
    pub colorcolumn: usize,
    pending_replace: bool,
    pending_replace_count: usize,
    /// Replace-mode overtype history: `Some(orig)` for an overwritten char,
    /// `None` for one appended past EOL — used to restore on Backspace.
    replace_stack: Vec<Option<char>>,
    /// Count-insert state: repeat the current insert N times on Esc
    /// (`3ihi<Esc>` -> "hihihi"), what command opened it, the captured keys,
    /// and a guard so replay doesn't re-capture.
    insert_repeat: usize,
    insert_entry: char,
    insert_keys: Vec<KeyEvent>,
    insert_replaying: bool,
    /// After insert-mode `Ctrl-r`: the next key names the register to paste.
    insert_pending_reg: bool,
    /// Insert-mode `Ctrl-x` submode: the next key picks a completion source
    /// (currently `Ctrl-l` for whole-line completion).
    insert_ctrl_x: bool,
    /// Insert-mode `Ctrl-k` digraph entry: `None` = inactive, `Some(None)` =
    /// awaiting the first char, `Some(Some(c))` = have first char, awaiting second.
    insert_digraph: Option<Option<char>>,
    /// Insert-mode `Ctrl-v` literal / numeric entry: `None` = inactive.
    insert_literal: Option<InsertLiteral>,
    /// Suppress the next `insert_keys` capture (the key was already captured by an
    /// outer `handle_insert` that is re-dispatching it, e.g. a `Ctrl-v` run's
    /// terminating key).
    suppress_insert_capture: bool,
    /// Insert-mode `Ctrl-o` one-shot: 0 = off, 1 = armed (set on Ctrl-o),
    /// 2 = active (running the single Normal command; return to insert at rest).
    insert_oneshot: u8,
    /// Text typed during the current insert session, and the previous session's
    /// text (the read-only `".` register, and insert-mode `Ctrl-a`).
    cur_insert: String,
    last_insert_text: String,
    /// Active `Ctrl-n`/`Ctrl-p` keyword completion session, if any.
    completion: Option<Completion>,
    /// Active block insert (`Ctrl-v` then `I`/`A`): (rmin, rmax, col, append,
    /// to_eol). Applied to every row on Esc; `to_eol` appends at each line's own
    /// end (ragged-right `$A`).
    block_insert: Option<(usize, usize, usize, bool, bool)>,
    /// Set by `$` in visual-block: a following `A` appends at each line's end.
    block_dollar: bool,
    pending_find: Option<char>,
    /// The count that preceded a pending `f`/`F`/`t`/`T` (e.g. `3fx`).
    pending_find_count: usize,
    /// Operator awaiting an `f`/`F`/`t`/`T` target char: (op, find cmd, count),
    /// for `dfx`, `ct)`, `2dTx`, …
    pending_op_find: Option<(char, char, usize)>,
    /// Pending `[` / `]` prefix for section motions (`[[`, `]]`, `[]`, `][`),
    /// with the count that preceded it.
    pending_bracket: Option<char>,
    pending_bracket_count: usize,
    /// `Z` was pressed, awaiting the second key for `ZZ` (write & quit) or
    /// `ZQ` (quit without saving).
    pending_z_quit: bool,
    /// The partially-typed Normal-mode command (count + operator + …) shown by
    /// the showcmd indicator; cleared whenever the editor returns to rest.
    pending_keys: String,
    last_find: Option<(char, char)>,
    marks: HashMap<char, Position>,
    pending_mark: Option<PendingMark>,
    previous_pos: Position,
    /// Jump history for `Ctrl-o` / `Ctrl-i`; `jump_idx` points at the current
    /// slot (== `jumps.len()` when at the live position).
    jumps: Vec<Position>,
    jump_idx: usize,
    /// Positions of recent changes for `g;` / `g,`; `change_idx` points at the
    /// current slot (== `changelist.len()` when at the live position).
    changelist: Vec<Position>,
    change_idx: usize,
    /// `U` baseline: the last changed line's row and its content before the
    /// current streak of changes, restored (and toggled) by normal-mode `U`.
    line_undo: Option<(usize, String)>,
    /// Active `:s///c` interactive-confirm session, if any.
    subst_confirm: Option<SubstConfirm>,
    /// Insert-mode abbreviations (`:iabbrev lhs rhs`): lhs -> rhs, expanded when a
    /// non-keyword char is typed right after the lhs.
    abbreviations: HashMap<String, String>,
    /// Normal-mode key mappings (`:nnoremap lhs rhs`): a single character to a
    /// sequence of keys, expanded non-recursively from a resting state.
    nmap: HashMap<char, String>,
    /// Guard so an expanding mapping does not re-trigger itself.
    mapping_active: bool,
    recording: Option<char>,
    macros: HashMap<char, Vec<KeyEvent>>,
    last_macro: Option<char>,
    expect_macro: Option<MacroMode>,
    replay_depth: usize,
    // `.` repeat: keys of the change in progress, the finalized last change,
    // the buffer revision at the last resting point, and a replay guard.
    dot_capture: Vec<KeyEvent>,
    dot: Vec<KeyEvent>,
    dot_rev_at_rest: u64,
    dot_replaying: bool,
    /// The Alt-activated menu bar, when open.
    menu: Option<MenuState>,
}

/// Whether the key after `q` / `@` records into or replays a macro register.
#[derive(Debug, Clone, Copy)]
enum MacroMode {
    Record,
    Play,
}

/// What the next key after `m` / `` ` `` / `'` does with a mark.
#[derive(Debug, Clone, Copy)]
enum PendingMark {
    Set,
    JumpExact,
    JumpLine,
}


impl Editor {
    /// A fresh editor over an empty scratch buffer.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Position::default(),
            mode: Mode::Normal,
            top: 0,
            left: 0,
            language: Language::PlainText,
            message: String::new(),
            cmdline: String::new(),
            show_line_numbers: true,
            relative_numbers: false,
            hlsearch: true,
            ignorecase: false,
            smartcase: false,
            incsearch: true,
            wrapscan: true,
            search_forward: true,
            search_origin: Position::default(),
            saved_search_re: None,
            saved_last_search: String::new(),
            saved_hlsearch: true,
            autoindent: true,
            expandtab: true,
            shiftwidth: 4,
            shiftround: false,
            tabstop: 4,
            view_rows: 24,
            view_cols: 80,
            scrolloff: 0,
            sidescrolloff: 0,
            line_kind: LineKind::Ex,
            cmd_history: Vec::new(),
            search_history: Vec::new(),
            hist_idx: None,
            hist_saved: String::new(),
            cmd_comp: None,
            register: Register::default(),
            registers: HashMap::new(),
            pending_register: None,
            expect_register: false,
            visual_anchor: Position::default(),
            last_visual: None,
            last_insert: Position::default(),
            last_search: String::new(),
            search_re: None,
            search_offset: None,
            last_subst: None,
            pending_count: None,
            pending_op: None,
            pending_op_count: None,
            pending_textobj: None,
            pending_vis_obj: None,
            pending_op_gg: None,
            pending_case: None,
            pending_case_obj: None,
            pending_comment: false,
            pending_format: false,
            pending_format_obj: None,
            textwidth: 0,
            list: false,
            listchars_tab: ('▸', '·'),
            listchars_trail: '·',
            listchars_eol: '$',
            cursorline: true,
            cursorcolumn: false,
            colorcolumn: 0,
            pending_replace: false,
            pending_replace_count: 1,
            replace_stack: Vec::new(),
            insert_repeat: 1,
            insert_entry: 'i',
            insert_keys: Vec::new(),
            insert_replaying: false,
            insert_pending_reg: false,
            insert_ctrl_x: false,
            insert_digraph: None,
            insert_literal: None,
            suppress_insert_capture: false,
            insert_oneshot: 0,
            cur_insert: String::new(),
            last_insert_text: String::new(),
            completion: None,
            block_insert: None,
            block_dollar: false,
            pending_find: None,
            pending_find_count: 1,
            pending_op_find: None,
            pending_bracket: None,
            pending_bracket_count: 1,
            pending_z_quit: false,
            pending_keys: String::new(),
            last_find: None,
            marks: HashMap::new(),
            pending_mark: None,
            previous_pos: Position::default(),
            jumps: Vec::new(),
            jump_idx: 0,
            changelist: Vec::new(),
            change_idx: 0,
            line_undo: None,
            subst_confirm: None,
            abbreviations: HashMap::new(),
            nmap: HashMap::new(),
            mapping_active: false,
            recording: None,
            macros: HashMap::new(),
            last_macro: None,
            expect_macro: None,
            replay_depth: 0,
            dot_capture: Vec::new(),
            dot: Vec::new(),
            dot_rev_at_rest: 0,
            dot_replaying: false,
            menu: None,
        }
    }

    /// Open the menu bar (only from Normal mode).
    pub fn open_menu(&mut self, menus: Vec<crate::menu::Menu>) {
        if self.mode == Mode::Normal {
            self.menu = Some(MenuState::new(menus));
        }
    }

    /// Whether the menu bar is currently open.
    pub fn is_menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// The menu state, for rendering.
    pub fn menu(&self) -> Option<&MenuState> {
        self.menu.as_ref()
    }

    /// Jump the open menu to the top-level entry whose title starts with `ch`.
    pub fn menu_open_initial(&mut self, ch: char) {
        if let Some(menu) = self.menu.as_mut() {
            menu.open_initial(ch);
        }
    }

    /// Mouse: open top-level menu `index` (opening its dropdown).
    pub fn menu_click_top(&mut self, index: usize) {
        if let Some(menu) = self.menu.as_mut() {
            if index < menu.menus.len() {
                menu.top = index;
                menu.stack = vec![0];
            }
        }
    }

    /// Close the menu bar.
    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    /// Mouse: select item `idx` at dropdown `level` and activate it (open a
    /// submenu, or run a command and close the menu).
    pub fn menu_mouse_select(&mut self, level: usize, idx: usize) {
        let outcome = {
            let menu = match self.menu.as_mut() {
                Some(m) => m,
                None => return,
            };
            if level > menu.stack.len() {
                return;
            }
            menu.stack.truncate(level);
            menu.stack.push(idx);
            menu.enter()
        };
        match outcome {
            MenuOutcome::Run(cmd) => {
                self.menu = None;
                self.mode = Mode::Command;
                self.line_kind = LineKind::Ex;
                self.cmdline = cmd;
            }
            MenuOutcome::Close => self.menu = None,
            MenuOutcome::None => {}
        }
    }

    fn handle_menu_key(&mut self, key: KeyEvent) -> Action {
        let outcome = {
            let menu = match self.menu.as_mut() {
                Some(m) => m,
                None => return Action::None,
            };
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    menu.left();
                    MenuOutcome::None
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    menu.right();
                    MenuOutcome::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    menu.up();
                    MenuOutcome::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    menu.down();
                    MenuOutcome::None
                }
                KeyCode::Enter => menu.enter(),
                KeyCode::Esc => menu.esc(),
                _ => MenuOutcome::None,
            }
        };
        match outcome {
            MenuOutcome::Run(cmd) => {
                self.menu = None;
                self.mode = Mode::Command;
                self.line_kind = LineKind::Ex;
                self.cmdline = cmd;
            }
            MenuOutcome::Close => self.menu = None,
            MenuOutcome::None => {}
        }
        Action::None
    }

    /// Load a file into a new editor, autodetecting the language.
    pub fn from_file(path: &str) -> std::io::Result<Self> {
        let buffer = Buffer::from_file(path)?;
        let first = buffer.line(0).unwrap_or("").to_string();
        let lang = detect_language(Some(std::path::Path::new(path)), &first);
        let mut ed = Editor::new();
        ed.buffer = buffer;
        ed.language = lang;
        Ok(ed)
    }

    /// The active search query (empty if none).
    pub fn search_query(&self) -> &str {
        &self.last_search
    }

    /// The compiled search regex, for highlighting (only when `hlsearch` is on).
    pub fn search_regex(&self) -> Option<&Regex> {
        if self.hlsearch {
            self.search_re.as_ref()
        } else {
            None
        }
    }

    /// Whether a search for `pat` should ignore case, given the `ignorecase` /
    /// `smartcase` options. With smartcase, any uppercase letter in the pattern
    /// forces a case-sensitive search.
    fn effective_ignorecase(&self, pat: &str) -> bool {
        if !self.ignorecase {
            return false;
        }
        if self.smartcase && pat.chars().any(|c| c.is_uppercase()) {
            return false;
        }
        true
    }

    /// Begin a `/` (forward) or `?` (backward) search: switch to the command
    /// line and snapshot the cursor and highlight state for incsearch preview and
    /// Esc restore.
    fn enter_search(&mut self, forward: bool) {
        self.mode = Mode::Command;
        self.line_kind = if forward {
            LineKind::SearchFwd
        } else {
            LineKind::SearchBack
        };
        self.cmdline.clear();
        self.hist_idx = None;
        self.search_origin = self.cursor;
        self.saved_search_re = self.search_re.clone();
        self.saved_last_search = self.last_search.clone();
        self.saved_hlsearch = self.hlsearch;
    }

    /// Set the search pattern and (re)compile its regex, enabling highlight.
    fn set_search(&mut self, pat: String) {
        let ic = self.effective_ignorecase(&pat);
        self.search_re = pattern::build_opts(&pat, ic);
        self.last_search = pat;
        self.hlsearch = true;
    }

    /// Execute a `:g`/`:v` global command: run `command` on every line matching
    /// (or, when `invert`, not matching) `pattern`. Supports `d`/`delete` and a
    /// `:s` substitution. Returns the number of lines/substitutions affected.
    /// The 0-based rows matching `pattern` (or not, when `invert`), for `:g`/`:v`.
    pub fn global_rows(&self, pattern: &str, invert: bool) -> Vec<usize> {
        // An empty pattern reuses the last search pattern (vim's `:g//cmd`).
        let pat = if pattern.is_empty() {
            self.last_search.as_str()
        } else {
            pattern
        };
        let Some(re) = pattern::build(pat) else {
            return Vec::new();
        };
        (0..self.buffer.line_count())
            .filter(|&i| re.is_match(self.buffer.line(i).unwrap_or("")) != invert)
            .collect()
    }

    pub fn global(&mut self, pattern: &str, invert: bool, command: &str) -> usize {
        // Running `:g/pat/…` makes `pat` the current search pattern, as vim does.
        if !pattern.is_empty() {
            self.last_search = pattern.to_string();
        }
        let matches = self.global_rows(pattern, invert);
        if matches.is_empty() {
            return 0;
        }
        let cmd = command.trim();
        if cmd == "d" || cmd == "delete" {
            self.checkpoint();
            for &row in matches.iter().rev() {
                if self.buffer.line_count() == 1 {
                    self.buffer.set_line(0, "");
                } else {
                    self.buffer.delete_line(row);
                }
            }
            self.cursor.row = self.cursor.row.min(self.buffer.line_count().saturating_sub(1));
            self.clamp_cursor(false);
            return matches.len();
        }
        if let crate::command::ExCommand::Substitute(spec) = crate::command::parse(cmd) {
            // Lines aren't deleted, so the indices stay valid as we go.
            let mut count = 0;
            for &row in &matches {
                let mut s = spec.clone();
                s.range = SubRange::Range(LineAddr::Num(row + 1), LineAddr::Num(row + 1));
                count += self.substitute(&s).0;
            }
            return count;
        }
        0
    }

    /// `&` — repeat the last `:s` on the current line.
    fn repeat_substitute(&mut self) {
        let Some(mut spec) = self.last_subst.clone() else {
            self.message = "No previous substitute".into();
            return;
        };
        spec.range = SubRange::CurrentLine;
        self.substitute(&spec);
    }

    /// `g&` — repeat the last substitute across the whole file (vim's
    /// `:%s//~/&`), reporting the number of changes.
    fn repeat_substitute_all(&mut self) {
        let Some(mut spec) = self.last_subst.clone() else {
            self.message = "No previous substitute".into();
            return;
        };
        spec.range = SubRange::WholeFile;
        let (subs, lines) = self.substitute(&spec);
        self.message = if subs == 0 {
            format!("E486: Pattern not found: {}", spec.pattern)
        } else {
            format!("{subs} substitution(s) on {lines} line(s)")
        };
    }

    /// The register currently being recorded into, if any (for the status line).
    pub fn recording_register(&self) -> Option<char> {
        self.recording
    }

    fn start_recording(&mut self, reg: char) {
        // An uppercase register name appends to the lowercase macro register;
        // a lowercase name records fresh.
        let target = reg.to_ascii_lowercase();
        if reg.is_ascii_uppercase() {
            self.macros.entry(target).or_default();
        } else {
            self.macros.insert(target, Vec::new());
        }
        self.recording = Some(target);
        self.message = format!("recording @{target}");
    }

    /// Replay macro register `reg` (or the last one for `@@`). Ex-commands
    /// inside a macro are not executed during replay.
    fn play_macro(&mut self, reg: char) {
        let target = if reg == '@' {
            self.last_macro
        } else {
            Some(reg.to_ascii_lowercase())
        };
        let Some(target) = target else {
            self.message = "No previously played macro".into();
            return;
        };
        self.last_macro = Some(target);
        let Some(keys) = self.macros.get(&target).cloned() else {
            return;
        };
        if self.replay_depth > 50 {
            return; // guard against runaway recursive macros
        }
        self.replay_depth += 1;
        for k in keys {
            let _ = self.handle_key(k);
        }
        self.replay_depth = self.replay_depth.saturating_sub(1);
    }

    /// The active command-line completion candidates and the selected index,
    /// for the wildmenu. `None` when no `Tab` completion cycle is in progress.
    pub fn completion_menu(&self) -> Option<(&[String], usize)> {
        let c = self.cmd_comp.as_ref()?;
        Some((&c.matches, c.idx))
    }

    /// Format the current value of an option for `:set {option}?`.
    pub fn option_value(&self, name: &str) -> String {
        let flag = |b: bool, n: &str| if b { n.to_string() } else { format!("no{n}") };
        match name {
            "number" | "nu" => flag(self.show_line_numbers, "number"),
            "relativenumber" | "rnu" => flag(self.relative_numbers, "relativenumber"),
            "hlsearch" | "hls" => flag(self.hlsearch, "hlsearch"),
            "ignorecase" | "ic" => flag(self.ignorecase, "ignorecase"),
            "smartcase" | "scs" => flag(self.smartcase, "smartcase"),
            "incsearch" | "is" => flag(self.incsearch, "incsearch"),
            "autoindent" | "ai" => flag(self.autoindent, "autoindent"),
            "expandtab" | "et" => flag(self.expandtab, "expandtab"),
            "list" => flag(self.list, "list"),
            "listchars" | "lcs" => self.listchars_summary(),
            "wrapscan" | "ws" => flag(self.wrapscan, "wrapscan"),
            "cursorline" | "cul" => flag(self.cursorline, "cursorline"),
            "cursorcolumn" | "cuc" => flag(self.cursorcolumn, "cursorcolumn"),
            "shiftwidth" | "sw" => format!("shiftwidth={}", self.shiftwidth),
            "shiftround" | "sr" => flag(self.shiftround, "shiftround"),
            "tabstop" | "ts" => format!("tabstop={}", self.tabstop),
            "scrolloff" | "so" => format!("scrolloff={}", self.scrolloff),
            "sidescrolloff" | "siso" => format!("sidescrolloff={}", self.sidescrolloff),
            "textwidth" | "tw" => format!("textwidth={}", self.textwidth),
            "colorcolumn" | "cc" => format!("colorcolumn={}", self.colorcolumn),
            "filetype" | "ft" | "syntax" => format!("filetype={}", self.language.name()),
            other => format!("E518: Unknown option: {other}"),
        }
    }

    /// The `list` markers `(tab_head, tab_fill, trail, eol)` for rendering.
    pub fn listchars(&self) -> (char, char, char, char) {
        (
            self.listchars_tab.0,
            self.listchars_tab.1,
            self.listchars_trail,
            self.listchars_eol,
        )
    }

    /// A textual `listchars=...` summary for `:set listchars?` and messages.
    fn listchars_summary(&self) -> String {
        format!(
            "listchars=tab:{}{},trail:{},eol:{}",
            self.listchars_tab.0, self.listchars_tab.1, self.listchars_trail, self.listchars_eol
        )
    }

    /// `:set listchars=tab:xy,trail:z,eol:e` — configure the `list` markers. `tab:`
    /// takes two characters (head + fill); `trail:` and `eol:` take one. Unknown
    /// keys are ignored.
    pub fn set_listchars(&mut self, spec: &str) {
        for item in spec.split(',') {
            let Some((key, val)) = item.split_once(':') else {
                continue;
            };
            let vchars: Vec<char> = val.chars().collect();
            match key.trim() {
                "tab" if vchars.len() >= 2 => {
                    self.listchars_tab = (vchars[0], vchars[1]);
                }
                "trail" if vchars.len() == 1 => {
                    self.listchars_trail = vchars[0];
                }
                "eol" if vchars.len() == 1 => {
                    self.listchars_eol = vchars[0];
                }
                _ => {}
            }
        }
        self.message = self.listchars_summary();
    }

    /// vim's `showmode` text for the bottom line — `-- INSERT --`, `-- VISUAL --`,
    /// etc. `None` in Normal and Command mode, where the message / command line
    /// takes the space instead.
    pub fn mode_indicator(&self) -> Option<&'static str> {
        match self.mode {
            Mode::Insert => Some("-- INSERT --"),
            Mode::Replace => Some("-- REPLACE --"),
            Mode::Visual => Some("-- VISUAL --"),
            Mode::VisualLine => Some("-- VISUAL LINE --"),
            Mode::VisualBlock => Some("-- VISUAL BLOCK --"),
            Mode::Normal | Mode::Command => None,
        }
    }

    /// vim's ruler scroll-position indicator: `All` when the whole buffer fits on
    /// screen, `Top` / `Bot` when the first / last line is visible, otherwise the
    /// percentage of the file above the window top.
    pub fn scroll_indicator(&self) -> String {
        let total = self.buffer.line_count();
        let above = self.top;
        let below = total.saturating_sub(self.top + self.view_rows);
        if below == 0 {
            if above == 0 {
                "All".into()
            } else {
                "Bot".into()
            }
        } else if above == 0 {
            "Top".into()
        } else {
            format!("{}%", (above * 100 / total.max(1)).min(99))
        }
    }

    /// Canonical option names shown by `:set` / `:set all`, in display order.
    const OPTION_NAMES: &'static [&'static str] = &[
        "number", "relativenumber", "hlsearch", "ignorecase", "smartcase",
        "incsearch", "autoindent", "expandtab", "list", "wrapscan", "cursorline",
        "cursorcolumn", "shiftwidth", "shiftround", "tabstop", "scrolloff",
        "sidescrolloff", "textwidth", "colorcolumn", "listchars", "filetype",
    ];

    /// A `:set` / `:set all` listing: one option per line with its current value.
    /// `all` lists every option; otherwise only those changed from their default
    /// (vim's bare `:set`). Reuses [`option_value`](Editor::option_value).
    pub fn options_listing(&self, all: bool) -> String {
        let title = if all { "all options" } else { "modified options" };
        let mut out = format!("options ({title}) — :bd to close\n\n");
        let default = Editor::new();
        let mut shown = 0;
        for name in Self::OPTION_NAMES {
            let val = self.option_value(name);
            if all || val != default.option_value(name) {
                out.push_str(&format!("  {val}\n"));
                shown += 1;
            }
        }
        if shown == 0 {
            out.push_str("  (all options are at their default values)\n");
        }
        out
    }

    /// The prefix character shown before the command line (`:`, `/`, `?`).
    pub fn cmdline_prefix(&self) -> char {
        match self.line_kind {
            LineKind::Ex => ':',
            LineKind::SearchFwd => '/',
            LineKind::SearchBack => '?',
        }
    }

    /// Re-detect the language from the current path + first line.
    pub fn redetect_language(&mut self) {
        let first = self.buffer.line(0).unwrap_or("").to_string();
        self.language = detect_language(self.buffer.path(), &first);
    }

    /// Remember the current position as the "previous" location (the `` `` ``
    /// mark) and push it onto the jump list, before a jump.
    fn record_jump(&mut self) {
        self.previous_pos = self.cursor;
        // Drop any forward history, then append this position.
        self.jumps.truncate(self.jump_idx);
        if self.jumps.last() != Some(&self.cursor) {
            self.jumps.push(self.cursor);
        }
        self.jump_idx = self.jumps.len();
    }

    /// `Ctrl-o` — go to an older position in the jump list.
    fn jump_back(&mut self) {
        if self.jump_idx == 0 {
            return;
        }
        // When leaving the live position, remember it so `Ctrl-i` can return.
        if self.jump_idx == self.jumps.len() {
            self.jumps.push(self.cursor);
        }
        self.jump_idx -= 1;
        if let Some(&pos) = self.jumps.get(self.jump_idx) {
            self.cursor = pos;
            self.clamp_cursor(false);
            self.scroll_into_view();
        }
    }

    /// `Ctrl-i` — go to a newer position in the jump list.
    fn jump_forward(&mut self) {
        if self.jump_idx + 1 >= self.jumps.len() {
            return;
        }
        self.jump_idx += 1;
        if let Some(&pos) = self.jumps.get(self.jump_idx) {
            self.cursor = pos;
            self.clamp_cursor(false);
            self.scroll_into_view();
        }
    }

    /// Jump to mark `c` (or the previous position for `` ` ``/`'`). `line_wise`
    /// lands on the first non-blank of the target line.
    fn jump_to_mark(&mut self, c: char, line_wise: bool) {
        let target = if c == '`' || c == '\'' {
            Some(self.previous_pos)
        } else {
            self.marks.get(&c).copied()
        };
        if let Some(mut p) = target {
            self.record_jump();
            let last = self.buffer.line_count().saturating_sub(1);
            p.row = p.row.min(last);
            self.cursor = p;
            if line_wise {
                self.move_first_nonblank();
            }
            self.clamp_cursor(false);
            self.scroll_into_view();
        } else {
            self.message = format!("E20: Mark not set: {c}");
        }
    }

    /// Jump to a 1-based line number (clamped), landing on the first non-blank.
    pub fn goto_line(&mut self, one_based: usize) {
        let target = one_based.saturating_sub(1);
        self.cursor.row = target.min(self.buffer.line_count().saturating_sub(1));
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Set the language explicitly (`:set ft=`).
    pub fn set_language(&mut self, lang: Language) {
        self.language = lang;
    }

    /// Execute a `:s` substitution (regex, with literal fallback). Replacement
    /// uses regex syntax for captures (`$1`, `${name}`). Returns
    /// `(substitutions, lines_changed)`.
    /// Resolve a `:s` spec's effective pattern — an empty pattern reuses the last
    /// search (vim's `:s//repl/`) — compile it honoring the `i` flag plus
    /// ignorecase/smartcase, and record it as the current search pattern. Returns
    /// the compiled regex, or `None` when it cannot be built. Shared by
    /// [`substitute`](Editor::substitute) and
    /// [`substitute_confirm_start`](Editor::substitute_confirm_start).
    fn resolve_substitute(&mut self, spec: &SubstituteSpec) -> Option<Regex> {
        let pat = if spec.pattern.is_empty() {
            self.last_search.clone()
        } else {
            spec.pattern.clone()
        };
        let ic = spec.ignorecase || self.effective_ignorecase(&pat);
        let re = pattern::build_opts(&pat, ic)?;
        self.last_search = pat;
        Some(re)
    }

    pub fn substitute(&mut self, spec: &SubstituteSpec) -> (usize, usize) {
        let Some(re) = self.resolve_substitute(spec) else {
            return (0, 0);
        };
        let (start, end) = self.resolve_range(spec.range);

        // The `n` flag just counts matches (no substitution, no undo step), and
        // highlights them like a search.
        if spec.count_only {
            let mut subs = 0;
            let mut lines = 0;
            for row in start..=end {
                let Some(line) = self.buffer.line(row) else { break };
                let m = re.find_iter(line).count();
                if m > 0 {
                    lines += 1;
                    subs += if spec.global { m } else { 1 };
                }
            }
            self.search_re = Some(re);
            self.hlsearch = true;
            return (subs, lines);
        }

        // Remember for `&` (repeat last substitution); `resolve_substitute`
        // already made the pattern the current search pattern.
        self.last_subst = Some(spec.clone());
        // vim-style replacement (`\1`, `&`) -> regex crate syntax.
        let replacement = pattern::vim_replacement(&spec.replacement);

        // First pass: compute new lines without mutating, so we only push an
        // undo checkpoint when something actually changes.
        let mut edits: Vec<(usize, String)> = Vec::new();
        let mut subs = 0;
        for row in start..=end {
            if row >= self.buffer.line_count() {
                break;
            }
            let line = self.buffer.line(row).unwrap_or("");
            let matches = re.find_iter(line).count();
            if matches == 0 {
                continue;
            }
            let (new, c) = if spec.global {
                (re.replace_all(line, replacement.as_str()).into_owned(), matches)
            } else {
                (re.replace(line, replacement.as_str()).into_owned(), 1)
            };
            subs += c;
            edits.push((row, new));
        }

        if edits.is_empty() {
            return (0, 0);
        }

        self.checkpoint();
        let lines_changed = edits.len();
        let last_row = edits.last().map(|(r, _)| *r).unwrap_or(self.cursor.row);
        for (row, new) in edits {
            self.buffer.set_line(row, new);
        }
        self.cursor.row = last_row.min(self.buffer.line_count().saturating_sub(1));
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
        (subs, lines_changed)
    }

    /// Start an interactive `:s///c` session: compile the pattern, seek the first
    /// match and show its prompt. Subsequent y/n/a/q/l keys are handled by
    /// [`handle_subst_confirm`](Editor::handle_subst_confirm).
    pub fn substitute_confirm_start(&mut self, spec: &SubstituteSpec) {
        let Some(re) = self.resolve_substitute(spec) else {
            self.message = "E486: Pattern not found".into();
            return;
        };
        let (start, end) = self.resolve_range(spec.range);
        self.search_re = Some(re.clone());
        self.hlsearch = true;
        self.subst_confirm = Some(SubstConfirm {
            re,
            repl: pattern::vim_replacement(&spec.replacement),
            display: spec.replacement.clone(),
            global: spec.global,
            end_row: end,
            row: start,
            byte_col: 0,
            match_bytes: None,
            count: 0,
            lines: HashSet::new(),
            checkpointed: false,
        });
        if !self.subst_confirm_seek() {
            self.finish_subst_confirm(true);
        }
    }

    /// Whether a `:s///c` confirm session is active (keys route to it).
    pub fn substitute_confirm_active(&self) -> bool {
        self.subst_confirm.is_some()
    }

    /// Find the next match from the session's current position, move the cursor to
    /// it and show the prompt. Returns false when the range is exhausted.
    fn subst_confirm_seek(&mut self) -> bool {
        let Some(mut st) = self.subst_confirm.take() else {
            return false;
        };
        let found = loop {
            if st.row > st.end_row || st.row >= self.buffer.line_count() {
                break None;
            }
            let line = self.buffer.line(st.row).unwrap_or("");
            if st.byte_col <= line.len() {
                if let Some(m) = st.re.find_at(line, st.byte_col) {
                    break Some((st.row, m.start(), m.end()));
                }
            }
            st.row += 1;
            st.byte_col = 0;
        };
        match found {
            Some((row, s, e)) => {
                st.row = row;
                st.match_bytes = Some((s, e));
                let line = self.buffer.line(row).unwrap_or("");
                let ccol = line[..s].chars().count();
                self.cursor = Position::new(row, ccol);
                self.clamp_cursor(true);
                self.scroll_into_view();
                let display = st.display.clone();
                self.subst_confirm = Some(st);
                self.message =
                    format!("replace with \"{display}\"? (y)es (n)o (a)ll (q)uit (l)ast");
                true
            }
            None => {
                self.subst_confirm = Some(st);
                false
            }
        }
    }

    /// Apply the current match's replacement (used by `y`, `a`, `l`).
    fn subst_apply_current(&mut self) {
        let Some(mut st) = self.subst_confirm.take() else {
            return;
        };
        if let Some((s, e)) = st.match_bytes {
            let row = st.row;
            let line = self.buffer.line(row).unwrap_or("").to_string();
            if let Some(caps) = st.re.captures_at(&line, s) {
                let mut expansion = String::new();
                caps.expand(&st.repl, &mut expansion);
                let new_line = format!("{}{}{}", &line[..s], expansion, &line[e..]);
                if !st.checkpointed {
                    self.checkpoint();
                    st.checkpointed = true;
                }
                // Resume after the inserted text. For a zero-width match, step
                // over one following char so we don't re-match in place; past
                // end-of-line, push byte_col beyond the line so seek moves on.
                let after = s + expansion.len();
                st.byte_col = if e == s {
                    match new_line[after..].chars().next() {
                        Some(c) => after + c.len_utf8(),
                        None => new_line.len() + 1,
                    }
                } else {
                    after
                };
                self.buffer.set_line(row, new_line);
                st.count += 1;
                st.lines.insert(row);
            }
            st.match_bytes = None;
        }
        self.subst_confirm = Some(st);
    }

    /// Advance to the next match after a replacement (`y`). Returns false when
    /// the range is exhausted.
    fn subst_advance_after_replace(&mut self) -> bool {
        if let Some(st) = self.subst_confirm.as_mut() {
            if !st.global {
                st.row += 1;
                st.byte_col = 0;
            }
        }
        self.subst_confirm_seek()
    }

    /// Advance past a skipped match (`n`). Returns false when exhausted.
    fn subst_advance_after_skip(&mut self) -> bool {
        if let Some(mut st) = self.subst_confirm.take() {
            if let Some((s, e)) = st.match_bytes {
                // The match end is already a char boundary; a zero-width match
                // steps over one whole char (or past the line) to make progress.
                st.byte_col = if e > s {
                    e
                } else {
                    let line = self.buffer.line(st.row).unwrap_or("");
                    match line[s..].chars().next() {
                        Some(c) => s + c.len_utf8(),
                        None => line.len() + 1,
                    }
                };
            }
            st.match_bytes = None;
            if !st.global {
                st.row += 1;
                st.byte_col = 0;
            }
            self.subst_confirm = Some(st);
        }
        self.subst_confirm_seek()
    }

    /// End the session and report the tally.
    fn finish_subst_confirm(&mut self, none_found: bool) {
        if let Some(st) = self.subst_confirm.take() {
            if none_found && st.count == 0 {
                self.message = "E486: Pattern not found".into();
            } else {
                let s_p = if st.count == 1 { "" } else { "s" };
                let n = st.lines.len();
                let l_p = if n == 1 { "" } else { "s" };
                self.message = format!("{} substitution{s_p} on {n} line{l_p}", st.count);
            }
        }
        self.clamp_cursor(false);
    }

    /// Handle a key during a `:s///c` session.
    pub fn handle_subst_confirm(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('y') => {
                self.subst_apply_current();
                if !self.subst_advance_after_replace() {
                    self.finish_subst_confirm(false);
                }
            }
            KeyCode::Char('n') => {
                if !self.subst_advance_after_skip() {
                    self.finish_subst_confirm(false);
                }
            }
            KeyCode::Char('l') => {
                self.subst_apply_current();
                self.finish_subst_confirm(false);
            }
            KeyCode::Char('a') => {
                loop {
                    self.subst_apply_current();
                    if !self.subst_advance_after_replace() {
                        break;
                    }
                }
                self.finish_subst_confirm(false);
            }
            KeyCode::Char('q') | KeyCode::Esc => self.finish_subst_confirm(false),
            _ => {} // ignore; the prompt stays up
        }
        Action::None
    }

    /// Resolve a range to an inclusive `(start_row, end_row)` pair (public wrapper
    /// for `:normal` over a range).
    pub fn range_rows(&self, range: SubRange) -> (usize, usize) {
        self.resolve_range(range)
    }

    /// `:delmarks {marks}` — delete the named marks. `"!"` clears all lowercase
    /// marks; otherwise `spec` lists marks to drop, with ranges like `a-d`.
    pub fn delete_marks(&mut self, spec: &str) {
        let spec = spec.trim();
        if spec.is_empty() {
            self.message = "E471: Argument required".into();
            return;
        }
        if spec == "!" {
            self.marks.retain(|&k, _| !k.is_ascii_lowercase());
            self.message = "deleted all lowercase marks".into();
            return;
        }
        let chars: Vec<char> = spec.chars().filter(|c| !c.is_whitespace()).collect();
        let mut targets: Vec<char> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if i + 2 < chars.len() && chars[i + 1] == '-' {
                let (a, b) = (chars[i], chars[i + 2]);
                if a <= b {
                    targets.extend(a..=b);
                }
                i += 3;
            } else {
                targets.push(chars[i]);
                i += 1;
            }
        }
        let n = targets.iter().filter(|c| self.marks.remove(c).is_some()).count();
        self.message = format!("deleted {n} mark(s)");
    }

    /// A `:marks` listing: each mark with its line, column, and line text.
    pub fn marks_listing(&self) -> String {
        let mut entries: Vec<(char, Position)> =
            self.marks.iter().map(|(&c, &p)| (c, p)).collect();
        entries.sort_by_key(|(c, _)| *c);
        let mut out = String::from("marks — :bd to close\n\n mark  line  col  text\n");
        for (c, p) in entries {
            let text = self.buffer.line(p.row).unwrap_or("");
            out.push_str(&format!(
                " {c:<4}  {:>4}  {:>3}  {}\n",
                p.row + 1,
                p.col + 1,
                text.trim_start()
            ));
        }
        out
    }

    /// A `:registers` listing: the unnamed, named, numbered, and small-delete
    /// registers with their contents (newlines shown as `^J`).
    pub fn registers_listing(&self) -> String {
        let mut out = String::from("registers — :bd to close\n\n reg  content\n");
        let mut row = |name: String, reg: &Register| {
            if reg.text.is_empty() {
                return;
            }
            let shown: String = reg.text.replace('\n', "^J").chars().take(60).collect();
            out.push_str(&format!(" {name:<4} {shown}\n"));
        };
        row("\"\"".into(), &self.register);
        for c in ('a'..='z').chain('0'..='9').chain(std::iter::once('-')) {
            if let Some(r) = self.registers.get(&c) {
                row(format!("\"{c}"), r);
            }
        }
        out
    }

    /// A `:jumps` listing of the jump-list positions.
    pub fn jumps_listing(&self) -> String {
        let mut out = String::from("jumps — :bd to close\n\n jump  line  col\n");
        for (i, p) in self.jumps.iter().enumerate() {
            out.push_str(&format!(" {i:>4}  {:>4}  {:>3}\n", p.row + 1, p.col + 1));
        }
        out
    }

    /// A `:changes` listing of the change-list positions. A `>` marks the current
    /// slot that `g;` / `g,` navigate from (shown past the end when at the live
    /// position), mirroring vim's `:changes`.
    pub fn changes_listing(&self) -> String {
        let mut out = String::from("changes — :bd to close\n\n   change  line  col  text\n");
        for (i, p) in self.changelist.iter().enumerate() {
            let here = if i == self.change_idx { '>' } else { ' ' };
            let text = self.buffer.line(p.row).unwrap_or("");
            out.push_str(&format!(
                " {here} {i:>4}  {:>4}  {:>3}  {}\n",
                p.row + 1,
                p.col + 1,
                text.trim_start()
            ));
        }
        if self.change_idx >= self.changelist.len() {
            out.push_str(" >\n");
        }
        out
    }

    /// A `:history` listing of the command-line and/or search history, oldest
    /// first with a 1-based index (newest entry has the highest number), matching
    /// vim's `:history`.
    pub fn history_listing(&self, kind: HistoryKind) -> String {
        let mut out = String::from("history — :bd to close\n");
        let mut section = |title: &str, items: &[String], prefix: char| {
            out.push_str(&format!("\n  #  {title} history\n"));
            for (i, entry) in items.iter().enumerate() {
                out.push_str(&format!(" {:>3}  {prefix}{entry}\n", i + 1));
            }
        };
        if matches!(kind, HistoryKind::Cmd | HistoryKind::All) {
            section("cmd", &self.cmd_history, ':');
        }
        if matches!(kind, HistoryKind::Search | HistoryKind::All) {
            section("search", &self.search_history, '/');
        }
        out
    }

    fn resolve_range(&self, range: SubRange) -> (usize, usize) {
        let last = self.buffer.line_count().saturating_sub(1);
        match range {
            SubRange::CurrentLine => (self.cursor.row, self.cursor.row),
            SubRange::WholeFile => (0, last),
            SubRange::Range(a, b) => {
                let ra = self.resolve_addr(a);
                let rb = self.resolve_addr(b);
                let (s, e) = if ra <= rb { (ra, rb) } else { (rb, ra) };
                (s.min(last), e.min(last))
            }
        }
    }

    fn resolve_addr(&self, addr: LineAddr) -> usize {
        let last = self.buffer.line_count().saturating_sub(1);
        match addr {
            LineAddr::Current => self.cursor.row,
            LineAddr::Last => last,
            LineAddr::Num(n) => n.saturating_sub(1),
            LineAddr::Mark(c) => self.marks.get(&c).map(|p| p.row).unwrap_or(self.cursor.row),
        }
    }

    /// Resolve a `:move`/`:copy` destination to an insertion index in
    /// `0..=line_count` (lines are inserted starting at that index). Address `0`
    /// means "before the first line"; line `n` means "after line n".
    fn resolve_dest(&self, addr: LineAddr) -> usize {
        let n = self.buffer.line_count();
        match addr {
            LineAddr::Current => (self.cursor.row + 1).min(n),
            LineAddr::Last => n,
            LineAddr::Num(0) => 0,
            LineAddr::Num(k) => k.min(n),
            LineAddr::Mark(c) => {
                let row = self.marks.get(&c).map(|p| p.row).unwrap_or(self.cursor.row);
                (row + 1).min(n)
            }
        }
    }

    /// `:r[ead] file` — insert `text`'s lines just below the cursor line. The
    /// cursor moves to the first inserted line. A trailing newline doesn't create
    /// a spurious empty line.
    pub fn read_lines_below(&mut self, text: &str) {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let lines: Vec<&str> = body.split('\n').collect();
        if lines.is_empty() {
            return;
        }
        self.checkpoint();
        let at = self.cursor.row + 1;
        for (k, line) in lines.iter().enumerate() {
            self.buffer.insert_line(at + k, *line);
        }
        self.cursor.row = at.min(self.buffer.line_count().saturating_sub(1));
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
        self.message = format!("{} line(s) read", lines.len());
    }

    /// `:r !cmd` — run a shell command and insert its output below the cursor.
    pub fn read_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            self.message = "E471: Argument required".into();
            return;
        }
        match Self::run_shell_filter(cmd, "") {
            Some(out) => self.read_lines_below(&out.replace("\r\n", "\n")),
            None => self.message = format!("E485: Can't run: {cmd}"),
        }
    }

    /// `:[range]d[elete]` — delete the range's lines into the unnamed register.
    pub fn delete_lines(&mut self, range: SubRange) {
        let (a, b) = self.resolve_range(range);
        self.apply_op('d', OpTarget::Lines(a, b));
        self.message = format!("{} line(s) deleted", b - a + 1);
    }

    /// `:[range]y[ank]` — yank the range's lines into the unnamed register.
    pub fn yank_lines(&mut self, range: SubRange) {
        let (a, b) = self.resolve_range(range);
        self.apply_op('y', OpTarget::Lines(a, b));
        self.message = format!("{} line(s) yanked", b - a + 1);
    }

    /// `:[range]>` / `:[range]<` — shift the range right/left by `times`
    /// shiftwidths. The cursor lands on the first shifted line.
    pub fn shift_lines(&mut self, range: SubRange, dedent: bool, times: usize) {
        let (a, b) = self.resolve_range(range);
        self.checkpoint();
        for _ in 0..times.max(1) {
            for r in a..=b {
                if dedent {
                    self.dedent_line(r);
                } else {
                    self.indent_line(r);
                }
            }
        }
        self.cursor.row = a;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// `:[addr]pu[t] [reg]` — put a register's text as whole lines after `dest`
    /// (vim's `:put` is always linewise). The cursor lands on the last line put.
    pub fn put_register(&mut self, dest: LineAddr, register: Option<char>) {
        let reg = self.register_text(register.unwrap_or('"'));
        if reg.text.is_empty() {
            self.message = "Nothing to put".into();
            return;
        }
        let dest_ins = self.resolve_dest(dest);
        let lines: Vec<&str> = reg.text.split('\n').collect();
        self.checkpoint();
        for (k, line) in lines.iter().enumerate() {
            self.buffer.insert_line(dest_ins + k, *line);
        }
        let last = lines.len().saturating_sub(1);
        let end_col = lines[last].chars().count().saturating_sub(1);
        self.set_change_marks(Position::new(dest_ins, 0), Position::new(dest_ins + last, end_col));
        self.cursor.row = (dest_ins + lines.len()).saturating_sub(1);
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
        self.message = format!("{} line(s) put", lines.len());
    }

    /// `:[range]j[oin]` — join the range's lines into one. `raw` keeps surrounding
    /// whitespace (like `gJ`); otherwise whitespace is collapsed to a space (`J`).
    /// A single-line range joins the current line with the one below it.
    pub fn join_lines(&mut self, range: SubRange, raw: bool) {
        let (a, b) = self.resolve_range(range);
        let joins = if b > a { b - a } else { 1 };
        self.checkpoint();
        for _ in 0..joins {
            let joined = if raw {
                self.buffer.join_line_raw(a)
            } else {
                self.buffer.join_line(a)
            };
            if !joined {
                break;
            }
        }
        self.cursor.row = a.min(self.buffer.line_count().saturating_sub(1));
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// `:[range]copy dest` — copy the range's lines to after `dest`. The cursor
    /// lands on the last copied line.
    pub fn copy_lines(&mut self, range: SubRange, dest: LineAddr) {
        let (s, e) = self.resolve_range(range);
        let dest_ins = self.resolve_dest(dest);
        self.checkpoint();
        let lines: Vec<String> = (s..=e)
            .map(|r| self.buffer.line(r).unwrap_or("").to_string())
            .collect();
        for (k, text) in lines.iter().enumerate() {
            self.buffer.insert_line(dest_ins + k, text.clone());
        }
        self.cursor.row = (dest_ins + lines.len()).saturating_sub(1);
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.message = format!("{} line(s) copied", lines.len());
    }

    /// `:[range]move dest` — move the range's lines to after `dest`. Rejects a
    /// destination inside the moved block (vim's E134). The cursor lands on the
    /// last moved line.
    pub fn move_lines(&mut self, range: SubRange, dest: LineAddr) {
        let (s, e) = self.resolve_range(range);
        let dest_ins = self.resolve_dest(dest);
        if dest_ins >= s && dest_ins <= e + 1 {
            self.message = "E134: cannot move lines into themselves".into();
            return;
        }
        self.checkpoint();
        let count = e - s + 1;
        let lines: Vec<String> = (s..=e)
            .map(|r| self.buffer.line(r).unwrap_or("").to_string())
            .collect();
        for _ in 0..count {
            self.buffer.delete_line(s);
        }
        // Destinations past the removed block shift up by `count`.
        let ins = if dest_ins > e { dest_ins - count } else { dest_ins };
        for (k, text) in lines.iter().enumerate() {
            self.buffer.insert_line(ins + k, text.clone());
        }
        self.cursor.row = (ins + count).saturating_sub(1);
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.message = format!("{count} line(s) moved");
    }

    /// The visual selection as an inclusive `(start, end)` ordered pair, if in a
    /// visual mode.
    pub fn selection(&self) -> Option<(Position, Position)> {
        if !self.mode.is_visual() {
            return None;
        }
        let (a, b) = (self.visual_anchor, self.cursor);
        Some(order(a, b))
    }

    // ---- viewport --------------------------------------------------------

    /// Update the known text-area size and scroll the cursor into view.
    pub fn set_viewport(&mut self, rows: usize, cols: usize) {
        self.view_rows = rows.max(1);
        self.view_cols = cols.max(1);
        self.scroll_into_view();
    }

    /// `zz` — center the current line in the viewport.
    fn center_line(&mut self) {
        self.top = self.cursor.row.saturating_sub(self.view_rows / 2);
    }

    /// `zt` — scroll so the current line is at the top.
    fn line_to_top(&mut self) {
        self.top = self.cursor.row;
    }

    /// `zb` — scroll so the current line is at the bottom.
    fn line_to_bottom(&mut self) {
        self.top = (self.cursor.row + 1).saturating_sub(self.view_rows);
    }

    /// `Ctrl-e` / `Ctrl-y` — scroll the view by `delta` lines, keeping the
    /// cursor on screen.
    fn scroll_view(&mut self, delta: isize) {
        let last = self.buffer.line_count().saturating_sub(1);
        if delta >= 0 {
            self.top = (self.top + delta as usize).min(last);
        } else {
            self.top = self.top.saturating_sub((-delta) as usize);
        }
        if self.cursor.row < self.top {
            self.cursor.row = self.top;
        }
        let bottom = self.top + self.view_rows.saturating_sub(1);
        if self.cursor.row > bottom {
            self.cursor.row = bottom.min(last);
        }
        self.clamp_cursor(false);
    }

    /// `Ctrl-f` / `Ctrl-b` — scroll forward / backward one full page, with vim's
    /// two-line overlap. The cursor lands on the first non-blank of the top line
    /// (forward) or bottom line (backward) of the new view. `count` pages at once.
    fn page_scroll(&mut self, forward: bool, count: usize) {
        let last = self.buffer.line_count().saturating_sub(1);
        let step = self.view_rows.saturating_sub(2).max(1) * count.max(1);
        if forward {
            self.top = (self.top + step).min(last);
            self.cursor.row = self.top;
        } else {
            self.top = self.top.saturating_sub(step);
            self.cursor.row = (self.top + self.view_rows.saturating_sub(1)).min(last);
        }
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    fn scroll_into_view(&mut self) {
        // Keep `scrolloff` lines of context above and below the cursor, capped to
        // half the window so the margin can never exceed what fits. Near the file
        // edges the margin shrinks naturally rather than scrolling past the ends.
        let last = self.buffer.line_count().saturating_sub(1);
        let so = self.scrolloff.min(self.view_rows.saturating_sub(1) / 2);
        let top_margin = self.cursor.row.saturating_sub(so);
        if top_margin < self.top {
            self.top = top_margin;
        }
        let bottom_margin = (self.cursor.row + so).min(last);
        if bottom_margin >= self.top + self.view_rows {
            self.top = bottom_margin + 1 - self.view_rows;
        }
        // Horizontal: keep `sidescrolloff` columns of context left/right of the
        // cursor, capped to half the window width.
        let siso = self.sidescrolloff.min(self.view_cols.saturating_sub(1) / 2);
        let left_margin = self.cursor.col.saturating_sub(siso);
        if left_margin < self.left {
            self.left = left_margin;
        }
        let right_margin = self.cursor.col + siso;
        if right_margin >= self.left + self.view_cols {
            self.left = right_margin + 1 - self.view_cols;
        }
    }

    // ---- key handling ----------------------------------------------------

    /// Handle a key event. Returns an [`Action`] for the app.
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // The menu bar is a modal overlay that captures all keys while open.
        if self.menu.is_some() {
            return self.handle_menu_key(key);
        }

        // An interactive `:s///c` confirm session captures y/n/a/q/l first.
        if self.subst_confirm.is_some() {
            return self.handle_subst_confirm(key);
        }

        // User-defined normal-mode mappings (`:nnoremap`). Expand only from a
        // resting Normal state, for an unmodified mapped character, and never
        // while already expanding a mapping (so mappings are non-recursive).
        if !self.nmap.is_empty() && !self.mapping_active && self.at_rest() {
            if let KeyCode::Char(c) = key.code {
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    if let Some(rhs) = self.nmap.get(&c).cloned() {
                        self.mapping_active = true;
                        let mut act = Action::None;
                        for ev in Self::parse_map_rhs(&rhs) {
                            let a = self.handle_key(ev);
                            if !matches!(a, Action::None) {
                                act = a;
                            }
                        }
                        self.mapping_active = false;
                        return act;
                    }
                }
            }
        }

        // The key after `q`/`@` selects the macro register (never recorded).
        if let Some(mm) = self.expect_macro.take() {
            if let KeyCode::Char(c) = key.code {
                match mm {
                    MacroMode::Record => self.start_recording(c),
                    // `@:` repeats the last command-line (ex) command.
                    MacroMode::Play if c == ':' => {
                        match self.cmd_history.last().cloned() {
                            Some(cmd) => return Action::RunEx(cmd),
                            None => self.message = "No previous command-line command".into(),
                        }
                    }
                    MacroMode::Play => self.play_macro(c),
                }
            }
            return Action::None;
        }

        // While recording, capture the raw key (except the `q` that stops it).
        if let Some(reg) = self.recording {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let is_stop = self.mode == Mode::Normal
                && matches!(key.code, KeyCode::Char('q'))
                && !ctrl
                && self.replay_depth == 0;
            if is_stop {
                self.recording = None;
                self.message = "recorded".into();
                return Action::None;
            }
            self.macros.entry(reg).or_default().push(key);
        }

        // `.` repeats the last change from a resting normal state. A leading count
        // (`3.`) repeats it that many times.
        if !self.dot_replaying
            && self.mode == Mode::Normal
            && matches!(key.code, KeyCode::Char('.'))
        {
            let count = self.pending_count.take();
            if self.at_rest() {
                for _ in 0..count.unwrap_or(1).max(1) {
                    self.replay_dot();
                }
                return Action::None;
            }
            self.pending_count = count; // not actually resting; leave state intact
        }

        // Accumulate keys for the `.` register unless we're replaying it.
        if !self.dot_replaying {
            self.dot_capture.push(key);
        }

        // Remember the selection extent so we can restore it with `gv` once the
        // key below exits visual mode.
        let pre_visual = if self.mode.is_visual() {
            self.selection().map(|(s, e)| (s, e, self.mode))
        } else {
            None
        };

        let was_insert = self.mode == Mode::Insert;
        // Command-line editing takes priority when active.
        let action = if self.mode == Mode::Command {
            self.handle_cmdline(key)
        } else {
            match self.mode {
                Mode::Insert => {
                    self.handle_insert(key);
                    Action::None
                }
                Mode::Replace => {
                    self.handle_replace(key);
                    Action::None
                }
                _ => self.handle_normal(key),
            }
        };

        if let Some(v) = pre_visual {
            if !self.mode.is_visual() {
                self.last_visual = Some(v);
                // `$`-to-eol block state only applies to the active block; if we
                // left visual without an `A`, drop it so it can't leak.
                if self.mode != Mode::Insert {
                    self.block_dollar = false;
                }
            }
        }

        // Track the text of the current insert session for the `".` register and
        // insert-mode `Ctrl-a`: reset on entering insert, snapshot on leaving.
        let now_insert = self.mode == Mode::Insert;
        if !was_insert && now_insert {
            self.cur_insert.clear();
        } else if was_insert && !now_insert && !self.cur_insert.is_empty() {
            self.last_insert_text = std::mem::take(&mut self.cur_insert);
        }

        // At a resting point, finalize (or discard) the captured change.
        if !self.dot_replaying && self.at_rest() {
            if self.buffer.revision() != self.dot_rev_at_rest {
                self.dot = std::mem::take(&mut self.dot_capture);
            } else {
                self.dot_capture.clear();
            }
            self.dot_rev_at_rest = self.buffer.revision();
        }

        // Insert-mode `Ctrl-o`: the Ctrl-o key arms (1); the next keys run one
        // Normal command (2); when it completes at rest we return to insert. If the
        // command switched to another mode itself (e.g. `cc`, `:`), just stay there.
        match self.insert_oneshot {
            1 => self.insert_oneshot = 2,
            2 => {
                if self.mode != Mode::Normal {
                    self.insert_oneshot = 0;
                } else if self.at_rest() {
                    self.insert_oneshot = 0;
                    self.enter_insert_here();
                }
            }
            _ => {}
        }

        // Track the partially-typed command for the showcmd indicator: grow it
        // while a Normal-mode command is pending, clear it once we're at rest or
        // leave Normal mode.
        if self.mode == Mode::Normal && !self.at_rest() {
            if let KeyCode::Char(c) = key.code {
                self.pending_keys.push(c);
            }
        } else {
            self.pending_keys.clear();
        }

        action
    }

    /// The partially-typed Normal-mode command, for the showcmd indicator
    /// (empty when the editor is at rest).
    pub fn pending_command(&self) -> &str {
        &self.pending_keys
    }

    /// The size of the current visual selection, for the showcmd indicator
    /// (vim-style): charwise shows the column count on a single line or the line
    /// count across lines, linewise the line count, and block `rows x cols`.
    /// `None` when not in visual mode.
    pub fn visual_size(&self) -> Option<String> {
        match self.mode {
            Mode::VisualBlock => {
                let (rmin, rmax, cmin, cmax) = self.block_rect()?;
                Some(format!("{}x{}", rmax - rmin + 1, cmax - cmin + 1))
            }
            Mode::VisualLine => {
                let (s, e) = self.selection()?;
                Some(format!("{}", e.row - s.row + 1))
            }
            Mode::Visual => {
                let (s, e) = self.selection()?;
                if s.row == e.row {
                    Some(format!("{}", e.col - s.col + 1))
                } else {
                    Some(format!("{}", e.row - s.row + 1))
                }
            }
            _ => None,
        }
    }

    /// Whether the editor is at a clean resting point in Normal mode (no pending
    /// operator/count/prefix), used to bound `.`-repeat capture.
    fn at_rest(&self) -> bool {
        self.mode == Mode::Normal
            && self.pending_op.is_none()
            && self.pending_count.is_none()
            && self.pending_textobj.is_none()
            && self.pending_op_gg.is_none()
            && self.pending_case.is_none()
            && self.pending_case_obj.is_none()
            && !self.pending_comment
            && !self.pending_format
            && self.pending_format_obj.is_none()
            && !self.pending_replace
            && self.pending_find.is_none()
            && self.pending_op_find.is_none()
            && self.pending_vis_obj.is_none()
            && self.pending_bracket.is_none()
            && !self.pending_z_quit
            && self.pending_mark.is_none()
            && !self.expect_register
            && self.expect_macro.is_none()
    }

    /// Replay the keystrokes of the last change (`.`).
    fn replay_dot(&mut self) {
        if self.dot.is_empty() {
            self.message = "Nothing to repeat".into();
            return;
        }
        let keys = self.dot.clone();
        self.dot_replaying = true;
        for k in keys {
            let _ = self.handle_key(k);
        }
        self.dot_replaying = false;
        self.dot_rev_at_rest = self.buffer.revision();
    }

    fn handle_cmdline(&mut self, key: KeyEvent) -> Action {
        // Tab / Shift-Tab cycle through command-line completions.
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.cmdline_complete(key.code == KeyCode::BackTab);
            return Action::None;
        }
        // Any other key ends an in-progress completion cycle.
        self.cmd_comp = None;
        // Command-line control shortcuts (so Ctrl-combos don't insert a letter).
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => {
                    self.cmdline_delete_word();
                    self.hist_idx = None;
                    self.update_incsearch();
                }
                KeyCode::Char('u') => {
                    self.cmdline.clear();
                    self.hist_idx = None;
                    self.update_incsearch();
                }
                _ => {}
            }
            return Action::None;
        }
        match key.code {
            KeyCode::Esc => {
                self.cancel_search_preview();
                self.mode = Mode::Normal;
                self.cmdline.clear();
                self.hist_idx = None;
                Action::None
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.cmdline);
                self.mode = Mode::Normal;
                self.hist_idx = None;
                self.push_history(&text);
                match self.line_kind {
                    LineKind::Ex => Action::RunEx(text),
                    LineKind::SearchFwd => {
                        // Commit from the original position so the preview jump
                        // doesn't make the search skip to the next match.
                        self.cursor = self.search_origin;
                        self.search_forward = true;
                        let (pat, off) = split_search_offset(&text);
                        self.search_offset = off;
                        self.set_search(pat);
                        self.search(true);
                        Action::None
                    }
                    LineKind::SearchBack => {
                        self.cursor = self.search_origin;
                        self.search_forward = false;
                        let (pat, off) = split_search_offset(&text);
                        self.search_offset = off;
                        self.set_search(pat);
                        self.search(false);
                        Action::None
                    }
                }
            }
            KeyCode::Up => {
                self.history_recall(true);
                self.update_incsearch();
                Action::None
            }
            KeyCode::Down => {
                self.history_recall(false);
                self.update_incsearch();
                Action::None
            }
            KeyCode::Backspace => {
                self.hist_idx = None;
                if self.cmdline.pop().is_none() {
                    self.cancel_search_preview();
                    self.mode = Mode::Normal;
                } else {
                    self.update_incsearch();
                }
                Action::None
            }
            KeyCode::Char(c) => {
                self.hist_idx = None;
                self.cmdline.push(c);
                self.update_incsearch();
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Ex-command names offered for `:`-line Tab completion.
    const EX_COMMANDS: &'static [&'static str] = &[
        "abbreviate", "autoindent", "bdelete", "bnext", "bprevious", "buffer", "buffers",
        "changes", "colorscheme", "copy", "cursorline", "delete", "delmarks", "edit", "expandtab",
        "files", "global", "help", "history", "hlsearch", "iabbrev", "ignorecase", "incsearch",
        "join", "jumps", "list", "marks", "move", "nnoremap", "nohlsearch", "normal",
        "number", "nunmap", "put", "quit", "quitall", "read", "registers",
        "relativenumber", "retab", "set", "smartcase", "sort", "source", "substitute",
        "theme", "unabbreviate", "version", "vglobal", "wall", "wq", "wqall", "write", "yank",
    ];

    /// `:set` option names offered for Tab completion (toggles, their `no`
    /// variants, and value options by bare name).
    const SET_OPTIONS: &'static [&'static str] = &[
        "all", "autoindent", "colorcolumn", "cursorcolumn", "cursorline", "expandtab",
        "filetype", "hlsearch", "ignorecase", "incsearch", "list", "listchars", "noautoindent",
        "nocursorcolumn", "nocursorline", "noexpandtab", "nohlsearch",
        "noignorecase", "noincsearch", "nolist", "nonumber", "norelativenumber",
        "noshiftround", "nosmartcase", "nowrapscan", "number", "relativenumber",
        "scrolloff", "shiftround", "shiftwidth", "sidescrolloff", "smartcase",
        "tabstop", "textwidth", "wrapscan",
    ];

    /// Tab completion on the `:` command line. Completes the first word against
    /// ex-command names, or the last word against option names after `:set`.
    /// Repeated Tab (or Shift-Tab) cycles through the matches.
    fn cmdline_complete(&mut self, backward: bool) {
        if self.line_kind != LineKind::Ex {
            return;
        }
        // Continue an active cycle if the line still matches what we produced.
        if let Some(comp) = &self.cmd_comp {
            let expected = format!("{}{}", comp.base, comp.matches[comp.idx]);
            if self.cmdline == expected {
                let n = comp.matches.len();
                let idx = if backward {
                    (comp.idx + n - 1) % n
                } else {
                    (comp.idx + 1) % n
                };
                let next = format!("{}{}", comp.base, comp.matches[idx]);
                if let Some(c) = self.cmd_comp.as_mut() {
                    c.idx = idx;
                }
                self.cmdline = next;
                return;
            }
            self.cmd_comp = None;
        }

        // Start a fresh completion: work out the base (text kept verbatim) and
        // the stem (the partial word being completed) plus its candidate set.
        let indent = self.cmdline.len() - self.cmdline.trim_start().len();
        let body = &self.cmdline[indent..];
        let first_space = body.find(' ');
        let (base, stem, candidates): (String, String, Vec<&str>) = match first_space {
            None => {
                // First word -> command names (letters only; skip ranges, etc.).
                if !body.is_empty() && !body.chars().all(|c| c.is_ascii_alphabetic()) {
                    return;
                }
                (self.cmdline[..indent].to_string(), body.to_string(), Self::EX_COMMANDS.to_vec())
            }
            Some(_) => {
                let cmd = &body[..first_space.unwrap()];
                if cmd != "set" && cmd != "se" {
                    return; // only :set argument completion is supported
                }
                let last = self.cmdline.rfind(' ').unwrap();
                let stem = self.cmdline[last + 1..].to_string();
                if stem.contains('=') {
                    return; // completing a value, not an option name
                }
                (self.cmdline[..=last].to_string(), stem, Self::SET_OPTIONS.to_vec())
            }
        };

        let matches: Vec<String> = candidates
            .iter()
            .filter(|c| c.starts_with(stem.as_str()))
            .map(|c| c.to_string())
            .collect();
        if matches.is_empty() {
            return;
        }
        let idx = if backward { matches.len() - 1 } else { 0 };
        self.cmdline = format!("{}{}", base, matches[idx]);
        self.cmd_comp = Some(CmdComp { base, matches, idx });
    }

    /// `Ctrl-w` on the command line: delete the trailing whitespace and word.
    fn cmdline_delete_word(&mut self) {
        while self.cmdline.chars().next_back().is_some_and(char::is_whitespace) {
            self.cmdline.pop();
        }
        while self.cmdline.chars().next_back().is_some_and(|c| !c.is_whitespace()) {
            self.cmdline.pop();
        }
    }

    fn push_history(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let hist = if self.line_kind == LineKind::Ex {
            &mut self.cmd_history
        } else {
            &mut self.search_history
        };
        if hist.last().map(|s| s.as_str()) != Some(text) {
            hist.push(text.to_string());
            if hist.len() > 100 {
                hist.remove(0);
            }
        }
    }

    /// Up (`older = true`) / Down recall through the active history list.
    fn history_recall(&mut self, older: bool) {
        let hist = if self.line_kind == LineKind::Ex {
            &self.cmd_history
        } else {
            &self.search_history
        };
        if hist.is_empty() {
            return;
        }
        let len = hist.len();
        let mut idx = match self.hist_idx {
            Some(i) => i,
            None => {
                self.hist_saved = self.cmdline.clone();
                len
            }
        };
        if older {
            if idx == 0 {
                return;
            }
            idx -= 1;
        } else {
            if idx >= len {
                return;
            }
            idx += 1;
        }
        self.cmdline = if idx >= len {
            self.hist_saved.clone()
        } else {
            hist[idx].clone()
        };
        self.hist_idx = Some(idx);
    }

    fn handle_insert(&mut self, key: KeyEvent) {
        // Capture typed keys so a counted insert (`3ihi`) can repeat on Esc.
        let capture =
            !self.insert_replaying && key.code != KeyCode::Esc && !self.suppress_insert_capture;
        self.suppress_insert_capture = false;
        if capture {
            self.insert_keys.push(key);
        }
        // Any key other than the completion cycle keys ends a completion session.
        let is_completion_key = key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('n') | KeyCode::Char('p'));
        if !is_completion_key {
            self.completion = None;
        }
        // `Ctrl-x` submode: the previous key was Ctrl-x. `Ctrl-l` completes the
        // whole line; anything else aborts and is handled normally.
        if self.insert_ctrl_x {
            self.insert_ctrl_x = false;
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('l') => {
                        self.insert_line_completion(true);
                        self.scroll_into_view();
                        return;
                    }
                    KeyCode::Char('f') => {
                        self.insert_file_completion(true);
                        self.scroll_into_view();
                        return;
                    }
                    _ => {}
                }
            }
        }
        // Register name after Ctrl-r.
        if self.insert_pending_reg {
            self.insert_pending_reg = false;
            if let KeyCode::Char(c) = key.code {
                let reg = self.register_text(c);
                self.insert_register_text(&reg);
            }
            self.scroll_into_view();
            return;
        }
        // Digraph entry after Ctrl-k: collect two characters, then insert the
        // composed character. A non-char key (e.g. Esc) cancels.
        if let Some(pending) = self.insert_digraph {
            match pending {
                None => {
                    self.insert_digraph = match key.code {
                        KeyCode::Char(c) => Some(Some(c)),
                        _ => None,
                    };
                }
                Some(first) => {
                    self.insert_digraph = None;
                    if let KeyCode::Char(second) = key.code {
                        if let Some(ch) = Self::digraph(first, second) {
                            self.buffer.insert_char(self.cursor, ch);
                            self.cursor.col += 1;
                            self.cur_insert.push(ch);
                        }
                    }
                }
            }
            self.scroll_into_view();
            return;
        }
        // Literal / numeric entry after Ctrl-v.
        if let Some(state) = self.insert_literal {
            self.handle_insert_literal(state, key);
            self.scroll_into_view();
            return;
        }
        // Insert-mode control shortcuts.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => self.insert_delete_word_before(),
                KeyCode::Char('u') => self.insert_delete_to_line_start(),
                KeyCode::Char('r') => self.insert_pending_reg = true,
                KeyCode::Char('k') => self.insert_digraph = Some(None),
                KeyCode::Char('v') => self.insert_literal = Some(InsertLiteral::Start),
                KeyCode::Char('t') => self.insert_indent(true),
                KeyCode::Char('d') => self.insert_indent(false),
                KeyCode::Char('n') => self.insert_completion(true),
                KeyCode::Char('p') => self.insert_completion(false),
                KeyCode::Char('x') => self.insert_ctrl_x = true,
                KeyCode::Char('a') => {
                    // Insert the text from the last insert session.
                    let reg = self.register_text('.');
                    self.insert_register_text(&reg);
                    self.cur_insert.push_str(&reg.text);
                }
                KeyCode::Char('o') => {
                    // Run one Normal-mode command, then come back to insert.
                    self.mode = Mode::Normal;
                    self.insert_oneshot = 1;
                }
                // Copy the character directly below (Ctrl-e) / above (Ctrl-y).
                KeyCode::Char('e') => self.insert_char_from_adjacent(1),
                KeyCode::Char('y') => self.insert_char_from_adjacent(-1),
                _ => {}
            }
            self.scroll_into_view();
            return;
        }
        match key.code {
            KeyCode::Esc => {
                // Block insert (Ctrl-v I/A/c): replicate to the other rows.
                self.finish_block_insert();
                // Repeat the inserted text for a counted insert (3i, 3o, …).
                let repeat = self.insert_repeat;
                if repeat > 1 && !self.insert_replaying {
                    let entry = self.insert_entry;
                    let keys = self.insert_keys.clone();
                    self.insert_replaying = true;
                    for _ in 1..repeat {
                        match entry {
                            'o' => self.open_below(),
                            'O' => self.open_above(),
                            _ => {}
                        }
                        for k in &keys {
                            self.handle_insert(*k);
                        }
                    }
                    self.insert_replaying = false;
                }
                self.insert_repeat = 1;
                self.record_insert_end();
                self.mode = Mode::Normal;
                // vim moves left when leaving insert mode
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
                self.clamp_cursor(false);
            }
            KeyCode::Char(c) => {
                // A non-keyword char triggers an abbreviation on the word before it.
                if !is_word_char(c) {
                    self.maybe_expand_abbrev();
                }
                self.buffer.insert_char(self.cursor, c);
                self.cursor.col += 1;
                self.cur_insert.push(c);
                // `:set textwidth` breaks the line when typing past the limit.
                self.maybe_auto_wrap();
            }
            KeyCode::Enter => {
                self.maybe_expand_abbrev();
                self.cur_insert.push('\n');
                let indent = if self.autoindent {
                    self.leading_indent(self.cursor.row)
                } else {
                    String::new()
                };
                self.buffer.split_line(self.cursor);
                self.cursor.row += 1;
                self.cursor.col = 0;
                if !indent.is_empty() {
                    self.buffer.insert_str(self.cursor, &indent);
                    self.cursor.col = indent.chars().count();
                }
            }
            KeyCode::Backspace => {
                self.cur_insert.pop();
                self.backspace();
            }
            KeyCode::Tab => {
                if self.expandtab {
                    let n = self.tabstop.max(1);
                    self.buffer.insert_str(self.cursor, &" ".repeat(n));
                    self.cursor.col += n;
                    self.cur_insert.push_str(&" ".repeat(n));
                } else {
                    self.buffer.insert_char(self.cursor, '\t');
                    self.cursor.col += 1;
                    self.cur_insert.push('\t');
                }
            }
            KeyCode::Left => self.move_left(1),
            KeyCode::Right => self.move_right(1, true),
            KeyCode::Up => self.move_up(1),
            KeyCode::Down => self.move_down(1),
            _ => {}
        }
        self.scroll_into_view();
    }

    fn handle_replace(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.record_insert_end();
                self.mode = Mode::Normal;
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
                self.clamp_cursor(false);
            }
            KeyCode::Char(c) => {
                if self.cursor.col < self.cur_len() {
                    // Overwrite the character, remembering the original.
                    let orig = self
                        .buffer
                        .line(self.cursor.row)
                        .and_then(|l| l.chars().nth(self.cursor.col));
                    self.replace_stack.push(orig);
                    self.buffer.replace_char(self.cursor, c);
                } else {
                    // Past end of line: append (record as an insertion).
                    self.replace_stack.push(None);
                    self.buffer.insert_char(self.cursor, c);
                }
                self.cursor.col += 1;
            }
            KeyCode::Backspace => {
                if let Some(entry) = self.replace_stack.pop() {
                    if self.cursor.col > 0 {
                        self.cursor.col -= 1;
                    }
                    match entry {
                        Some(orig) => self.buffer.replace_char(self.cursor, orig),
                        None => {
                            self.buffer.delete_char(self.cursor);
                        }
                    }
                } else if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
            }
            KeyCode::Enter => {
                self.buffer.split_line(self.cursor);
                self.cursor.row += 1;
                self.cursor.col = 0;
                self.replace_stack.clear();
            }
            KeyCode::Left => self.move_left(1),
            KeyCode::Right => self.move_right(1, true),
            KeyCode::Up => self.move_up(1),
            KeyCode::Down => self.move_down(1),
            _ => {}
        }
        self.scroll_into_view();
    }

    fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
            self.buffer.delete_char(self.cursor);
        } else if self.cursor.row > 0 {
            let prev_len = self.buffer.line_len(self.cursor.row - 1);
            // join current line into previous
            let cur = self
                .buffer
                .line(self.cursor.row)
                .unwrap_or("")
                .to_string();
            self.buffer.delete_line(self.cursor.row);
            self.cursor.row -= 1;
            self.cursor.col = prev_len;
            self.buffer.insert_str(self.cursor, &cur);
        }
    }

    /// `Ctrl-n` (forward) / `Ctrl-p` (backward) keyword completion. On the first
    /// press it finds the word prefix before the cursor, gathers matching words
    /// from the buffer, and inserts the first/last one; subsequent presses cycle.
    /// Insert the character at the cursor's column on the line `delta` rows away
    /// (+1 = below for `Ctrl-e`, -1 = above for `Ctrl-y`). No-op if there's no
    /// such line or column.
    fn insert_char_from_adjacent(&mut self, delta: isize) {
        let row = self.cursor.row as isize + delta;
        if row < 0 {
            return;
        }
        let Some(line) = self.buffer.line(row as usize) else {
            return;
        };
        let Some(c) = line.chars().nth(self.cursor.col) else {
            return;
        };
        self.buffer.insert_char(self.cursor, c);
        self.cursor.col += 1;
        self.cur_insert.push(c);
    }

    fn insert_completion(&mut self, forward: bool) {
        if let Some(comp) = self.completion.as_ref() {
            let n = comp.candidates.len();
            let idx = if forward {
                (comp.idx + 1) % n
            } else {
                (comp.idx + n - 1) % n
            };
            let cand = comp.candidates[idx].clone();
            let start = comp.start_col;
            self.apply_completion(start, &cand);
            if let Some(c) = self.completion.as_mut() {
                c.idx = idx;
            }
            return;
        }
        let line: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col.min(line.len());
        let mut start = col;
        while start > 0 && is_word_char(line[start - 1]) {
            start -= 1;
        }
        if start == col {
            self.message = "No completion prefix".into();
            return;
        }
        let prefix: String = line[start..col].iter().collect();
        let candidates = self.gather_candidates(&prefix);
        if candidates.is_empty() {
            self.message = format!("No match for \"{prefix}\"");
            return;
        }
        let idx = if forward { 0 } else { candidates.len() - 1 };
        let cand = candidates[idx].clone();
        self.completion = Some(Completion {
            start_col: start,
            candidates,
            idx,
        });
        self.apply_completion(start, &cand);
    }

    /// `Ctrl-x Ctrl-l` — whole-line completion. Completes the current line from
    /// other buffer lines that start with the text before the cursor; cycle the
    /// candidates with `Ctrl-n` / `Ctrl-p`.
    fn insert_line_completion(&mut self, forward: bool) {
        if self.completion.is_some() {
            self.insert_completion(forward);
            return;
        }
        let cur: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let end = self.cursor.col.min(cur.len());
        let prefix: String = cur[..end].iter().collect();
        let mut seen = std::collections::HashSet::new();
        let mut candidates = Vec::new();
        for row in 0..self.buffer.line_count() {
            if row == self.cursor.row {
                continue;
            }
            let line = self.buffer.line(row).unwrap_or("");
            if line.starts_with(&prefix) && line != prefix && seen.insert(line.to_string()) {
                candidates.push(line.to_string());
            }
        }
        if candidates.is_empty() {
            self.message = "No line completion".into();
            return;
        }
        let idx = if forward { 0 } else { candidates.len() - 1 };
        let cand = candidates[idx].clone();
        self.completion = Some(Completion {
            start_col: 0,
            candidates,
            idx,
        });
        self.apply_completion(0, &cand);
    }

    /// Define a normal-mode mapping (`:nnoremap lhs rhs`). `lhs` must be a single
    /// character; `rhs` is a key sequence (see [`parse_map_rhs`]).
    pub fn set_nmap(&mut self, lhs: char, rhs: &str) {
        self.nmap.insert(lhs, rhs.to_string());
        self.message = format!("map: {lhs} -> {rhs}");
    }

    /// Remove a normal-mode mapping (`:nunmap lhs`).
    pub fn remove_nmap(&mut self, lhs: char) {
        if self.nmap.remove(&lhs).is_some() {
            self.message = format!("unmapped: {lhs}");
        } else {
            self.message = format!("E31: No such mapping: {lhs}");
        }
    }

    /// A `:nnoremap` (no-arg) listing of the defined mappings.
    pub fn nmap_listing(&self) -> String {
        let mut entries: Vec<(&char, &String)> = self.nmap.iter().collect();
        entries.sort_by_key(|(c, _)| **c);
        let mut out = String::from("mappings — :bd to close\n\n lhs  rhs\n");
        for (lhs, rhs) in entries {
            out.push_str(&format!(" {lhs:<4} {rhs}\n"));
        }
        out
    }

    /// Translate a mapping's `rhs` into key events, recognising `<CR>`, `<Esc>`,
    /// `<Space>`, `<Tab>`, `<BS>`, and `<C-x>` (control) notations.
    fn parse_map_rhs(rhs: &str) -> Vec<KeyEvent> {
        let chars: Vec<char> = rhs.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '<' {
                if let Some(rel) = chars[i..].iter().position(|&c| c == '>') {
                    let tag: String = chars[i + 1..i + rel].iter().collect();
                    let low = tag.to_ascii_lowercase();
                    let ev = match low.as_str() {
                        "cr" | "enter" | "return" => Some(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        "esc" => Some(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                        "space" => Some(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
                        "tab" => Some(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                        "bs" | "backspace" => Some(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
                        _ if low.starts_with("c-") && tag.chars().count() == 3 => {
                            let ch = tag.chars().nth(2).unwrap().to_ascii_lowercase();
                            Some(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
                        }
                        _ => None,
                    };
                    if let Some(ev) = ev {
                        out.push(ev);
                        i += rel + 1;
                        continue;
                    }
                }
            }
            out.push(KeyEvent::new(KeyCode::Char(chars[i]), KeyModifiers::NONE));
            i += 1;
        }
        out
    }

    /// Define an insert-mode abbreviation (`:iabbrev lhs rhs`).
    pub fn set_abbrev(&mut self, lhs: &str, rhs: &str) {
        self.abbreviations.insert(lhs.to_string(), rhs.to_string());
        self.message = format!("abbr: {lhs} -> {rhs}");
    }

    /// Remove an abbreviation (`:unabbreviate lhs`).
    pub fn remove_abbrev(&mut self, lhs: &str) {
        if self.abbreviations.remove(lhs).is_some() {
            self.message = format!("removed abbr: {lhs}");
        } else {
            self.message = format!("E24: No such abbreviation: {lhs}");
        }
    }

    /// A `:abbreviate` listing of the defined abbreviations.
    pub fn abbrev_listing(&self) -> String {
        let mut entries: Vec<(&String, &String)> = self.abbreviations.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = String::from("abbreviations — :bd to close\n\n lhs            rhs\n");
        for (lhs, rhs) in entries {
            out.push_str(&format!(" {lhs:<14} {rhs}\n"));
        }
        out
    }

    /// Expand the keyword immediately before the cursor if it matches an insert
    /// abbreviation. Called in insert mode just before a non-keyword char (or
    /// Enter) is inserted.
    fn maybe_expand_abbrev(&mut self) {
        if self.abbreviations.is_empty() {
            return;
        }
        let line: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col.min(line.len());
        let mut start = col;
        while start > 0 && is_word_char(line[start - 1]) {
            start -= 1;
        }
        if start == col {
            return;
        }
        let word: String = line[start..col].iter().collect();
        let Some(rhs) = self.abbreviations.get(&word).cloned() else {
            return;
        };
        let prefix: String = line[..start].iter().collect();
        let suffix: String = line[col..].iter().collect();
        self.buffer.set_line(self.cursor.row, format!("{prefix}{rhs}{suffix}"));
        self.cursor.col = start + rhs.chars().count();
        // Keep the insert-session text (for dot-repeat) in sync: swap the typed
        // lhs for the rhs.
        let wlen = word.chars().count();
        let kept = self.cur_insert.chars().count().saturating_sub(wlen);
        self.cur_insert = self.cur_insert.chars().take(kept).collect();
        self.cur_insert.push_str(&rhs);
    }

    /// `Ctrl-x Ctrl-f` — filename completion. Completes the path token before the
    /// cursor from directory entries; directories gain a trailing `/` so you can
    /// keep descending. Cycle with `Ctrl-n` / `Ctrl-p`.
    fn insert_file_completion(&mut self, forward: bool) {
        if self.completion.is_some() {
            self.insert_completion(forward);
            return;
        }
        let line: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col.min(line.len());
        let is_path = |c: char| c.is_alphanumeric() || "/\\._-+~:".contains(c);
        let mut start = col;
        while start > 0 && is_path(line[start - 1]) {
            start -= 1;
        }
        let token: String = line[start..col].iter().collect();
        let (dir, stem) = match token.rfind(['/', '\\']) {
            Some(i) => (token[..=i].to_string(), token[i + 1..].to_string()),
            None => (String::new(), token.clone()),
        };
        let read_from = if dir.is_empty() { ".".to_string() } else { dir.clone() };
        let Ok(entries) = std::fs::read_dir(&read_from) else {
            self.message = format!("E484: can't read \"{read_from}\"");
            return;
        };
        let mut candidates: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.starts_with(&stem) {
                    return None;
                }
                let slash = if e.path().is_dir() { "/" } else { "" };
                Some(format!("{dir}{name}{slash}"))
            })
            .collect();
        if candidates.is_empty() {
            self.message = "No file match".into();
            return;
        }
        candidates.sort();
        let idx = if forward { 0 } else { candidates.len() - 1 };
        let cand = candidates[idx].clone();
        self.completion = Some(Completion {
            start_col: start,
            candidates,
            idx,
        });
        self.apply_completion(start, &cand);
    }

    /// Replace the characters from `start` to the cursor on the current line with
    /// `cand`, leaving the cursor at the end of the inserted word.
    fn apply_completion(&mut self, start: usize, cand: &str) {
        let line: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let end = self.cursor.col.min(line.len());
        let prefix: String = line[..start].iter().collect();
        let suffix: String = line[end..].iter().collect();
        self.buffer.set_line(self.cursor.row, format!("{prefix}{cand}{suffix}"));
        self.cursor.col = start + cand.chars().count();
    }

    /// Collect unique words in the buffer that start with `prefix` (and aren't
    /// exactly `prefix`), in document order.
    fn gather_candidates(&self, prefix: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for row in 0..self.buffer.line_count() {
            let line = self.buffer.line(row).unwrap_or("");
            let mut word = String::new();
            for ch in line.chars() {
                if is_word_char(ch) {
                    word.push(ch);
                    continue;
                }
                if !word.is_empty() {
                    if word.starts_with(prefix) && word != prefix && seen.insert(word.clone()) {
                        out.push(word.clone());
                    }
                    word.clear();
                }
            }
            if !word.is_empty()
                && word.starts_with(prefix)
                && word != prefix
                && seen.insert(word.clone())
            {
                out.push(word);
            }
        }
        out
    }

    /// `Ctrl-w` in insert mode: delete the word (and preceding spaces) before
    /// the cursor.
    fn insert_delete_word_before(&mut self) {
        if self.cursor.col == 0 {
            self.backspace();
            return;
        }
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col;
        let mut start = col;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        if start > 0 {
            let class = Self::char_class(chars[start - 1]);
            while start > 0 && Self::char_class(chars[start - 1]) == class {
                start -= 1;
            }
        }
        let new: String = chars[..start].iter().chain(&chars[col..]).collect();
        self.buffer.set_line(self.cursor.row, new);
        self.cursor.col = start;
    }

    /// The contents of a register by name (`"` = unnamed).
    fn register_text(&self, name: char) -> Register {
        match name {
            '"' => self.register.clone(),
            // `%` is the read-only current file-name register.
            '%' => Register {
                text: self
                    .buffer
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                linewise: false,
            },
            // `.` is the read-only last-inserted-text register.
            '.' => Register {
                text: self.last_insert_text.clone(),
                linewise: false,
            },
            // `+` / `*` read the system clipboard. A trailing newline marks
            // line-wise text (stripped to match the internal representation).
            // When no clipboard tool is available, fall back to the mirror
            // kept under `+`.
            '+' | '*' => match crate::clipboard::read() {
                Some(mut text) => {
                    let linewise = text.ends_with('\n');
                    if linewise {
                        text.pop();
                    }
                    Register { text, linewise }
                }
                None => self.registers.get(&'+').cloned().unwrap_or_default(),
            },
            other => {
                // An uppercase register name reads its lowercase register.
                let key = other.to_ascii_lowercase();
                self.registers.get(&key).cloned().unwrap_or_default()
            }
        }
    }

    /// Write `reg` into named register `name`. An uppercase name *appends* to the
    /// lowercase register of the same letter (vim's append-register behavior);
    /// any other name replaces.
    fn write_named_register(&mut self, name: char, reg: Register) {
        if name == '+' || name == '*' {
            // Push to the system clipboard; append a newline for line-wise
            // text so other applications paste it as whole lines. Mirror it
            // under `+` so `:registers` shows it and reads still work when no
            // clipboard tool is present.
            let payload = if reg.linewise {
                format!("{}\n", reg.text)
            } else {
                reg.text.clone()
            };
            crate::clipboard::write(&payload);
            self.registers.insert('+', reg);
            return;
        }
        if name.is_ascii_uppercase() {
            let lower = name.to_ascii_lowercase();
            let combined = match self.registers.get(&lower) {
                Some(existing) if !existing.text.is_empty() => {
                    let linewise = existing.linewise || reg.linewise;
                    let text = if linewise {
                        format!("{}\n{}", existing.text, reg.text)
                    } else {
                        format!("{}{}", existing.text, reg.text)
                    };
                    Register { text, linewise }
                }
                _ => reg,
            };
            self.registers.insert(lower, combined);
        } else {
            self.registers.insert(name, reg);
        }
    }

    /// Insert a register's text at the cursor (handling embedded newlines), for
    /// insert-mode `Ctrl-r`.
    fn insert_register_text(&mut self, reg: &Register) {
        for ch in reg.text.chars() {
            if ch == '\n' {
                self.buffer.split_line(self.cursor);
                self.cursor.row += 1;
                self.cursor.col = 0;
            } else {
                self.buffer.insert_char(self.cursor, ch);
                self.cursor.col += 1;
            }
        }
        if reg.linewise {
            self.buffer.split_line(self.cursor);
            self.cursor.row += 1;
            self.cursor.col = 0;
        }
    }

    /// Insert-mode `Ctrl-v`: a literal key, or a numeric character code. After
    /// `Ctrl-v`, `u`/`U` reads 4/8 hex digits, `x`/`X` 2 hex, `o`/`O` 3 octal,
    /// and a leading digit reads up to 3 decimal digits; any other key is
    /// inserted literally (so `Ctrl-v Tab` inserts a real tab even with
    /// `expandtab`). A full run finalizes automatically; a shorter run
    /// finalizes when a non-digit arrives (that key ends the run).
    fn handle_insert_literal(&mut self, state: InsertLiteral, key: KeyEvent) {
        let code = key.code;
        match state {
            InsertLiteral::Start => {
                self.insert_literal = None;
                match code {
                    KeyCode::Char('u') => self.start_literal_digits(16, 4),
                    KeyCode::Char('U') => self.start_literal_digits(16, 8),
                    KeyCode::Char('x') | KeyCode::Char('X') => self.start_literal_digits(16, 2),
                    KeyCode::Char('o') | KeyCode::Char('O') => self.start_literal_digits(8, 3),
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        // A leading decimal digit begins a 3-digit decimal run.
                        self.insert_literal = Some(InsertLiteral::Digits {
                            radix: 10,
                            max: 3,
                            acc: c as u32 - '0' as u32,
                            count: 1,
                        });
                    }
                    KeyCode::Char(c) => self.insert_literal_char(c),
                    KeyCode::Tab => self.insert_literal_char('\t'),
                    _ => {}
                }
            }
            InsertLiteral::Digits {
                radix,
                max,
                acc,
                count,
            } => {
                let digit = match code {
                    KeyCode::Char(c) => c.to_digit(radix),
                    _ => None,
                };
                match digit {
                    Some(d) => {
                        let acc = acc * radix + d;
                        let count = count + 1;
                        if count >= max {
                            self.finish_literal_digits(acc);
                        } else {
                            self.insert_literal = Some(InsertLiteral::Digits {
                                radix,
                                max,
                                acc,
                                count,
                            });
                        }
                    }
                    // A non-digit ends the run: insert what we have, then handle
                    // the terminating key normally (it was already captured, so
                    // suppress the re-capture in the nested call).
                    None => {
                        self.finish_literal_digits(acc);
                        self.suppress_insert_capture = true;
                        self.handle_insert(key);
                    }
                }
            }
        }
    }

    fn start_literal_digits(&mut self, radix: u32, max: usize) {
        self.insert_literal = Some(InsertLiteral::Digits {
            radix,
            max,
            acc: 0,
            count: 0,
        });
    }

    fn finish_literal_digits(&mut self, acc: u32) {
        self.insert_literal = None;
        if let Some(ch) = char::from_u32(acc) {
            self.insert_literal_char(ch);
        }
    }

    /// Insert one character verbatim (used by `Ctrl-v`), bypassing abbreviation
    /// expansion. A newline splits the line like a normal `Enter`.
    fn insert_literal_char(&mut self, ch: char) {
        if ch == '\n' {
            self.buffer.split_line(self.cursor);
            self.cursor.row += 1;
            self.cursor.col = 0;
        } else {
            self.buffer.insert_char(self.cursor, ch);
            self.cursor.col += 1;
        }
        self.cur_insert.push(ch);
    }

    /// Auto-wrap while typing: when `textwidth` is set and the current line grows
    /// past it, break at the last blank before the cursor and carry the trailing
    /// word (plus autoindent) down to a new line. A word with no blank to break on
    /// is left long, matching vim's default behavior.
    fn maybe_auto_wrap(&mut self) {
        let tw = self.textwidth;
        if tw == 0 {
            return;
        }
        let row = self.cursor.row;
        let chars: Vec<char> = self.buffer.line(row).unwrap_or("").chars().collect();
        let limit = self.cursor.col.min(chars.len());
        // Measure display columns up to the cursor so tabs count by their width.
        let prefix: String = chars[..limit].iter().collect();
        if Self::expand_tabs(&prefix, self.tabstop.max(1)).chars().count() <= tw {
            return;
        }
        // Find the last blank strictly before the cursor to break on.
        let Some(brk) = (0..limit).rev().find(|&i| chars[i] == ' ' || chars[i] == '\t') else {
            return;
        };
        let indent = if self.autoindent {
            self.leading_indent(row)
        } else {
            String::new()
        };
        let first: String = chars[..brk].iter().collect();
        let rest: String = chars[brk + 1..].iter().collect();
        let moved = self.cursor.col - (brk + 1);
        self.buffer.set_line(row, first);
        self.buffer.insert_line(row + 1, format!("{indent}{rest}"));
        self.cursor.row = row + 1;
        self.cursor.col = indent.chars().count() + moved;
    }

    /// `Ctrl-t` / `Ctrl-d` in insert mode: indent / dedent the current line,
    /// keeping the cursor on the same character.
    fn insert_indent(&mut self, indent: bool) {
        let row = self.cursor.row;
        let before = self.buffer.line_len(row);
        if indent {
            self.indent_line(row);
            let added = self.buffer.line_len(row).saturating_sub(before);
            self.cursor.col += added;
        } else {
            self.dedent_line(row);
            let removed = before.saturating_sub(self.buffer.line_len(row));
            self.cursor.col = self.cursor.col.saturating_sub(removed);
        }
    }

    /// `Ctrl-u` in insert mode: delete from the line start to the cursor.
    fn insert_delete_to_line_start(&mut self) {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let new: String = chars[self.cursor.col.min(chars.len())..].iter().collect();
        self.buffer.set_line(self.cursor.row, new);
        self.cursor.col = 0;
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Action {
        // Pending `r<char>` replace (`<n>r<char>` replaces n chars).
        if self.pending_replace {
            self.pending_replace = false;
            // In visual mode, `r<c>` replaces every selected character with `c`.
            if self.mode.is_visual() {
                if let KeyCode::Char(c) = key.code {
                    self.replace_selection(c);
                } else {
                    self.mode = Mode::Normal; // Esc / other cancels
                }
                return Action::None;
            }
            let n = self.pending_replace_count.max(1);
            if let KeyCode::Char(c) = key.code {
                // Only act if the whole run fits on the line (vim behavior).
                if self.cursor.col + n <= self.cur_len() {
                    self.checkpoint();
                    for k in 0..n {
                        let pos = Position::new(self.cursor.row, self.cursor.col + k);
                        self.buffer.replace_char(pos, c);
                    }
                    self.cursor.col += n - 1;
                    self.clamp_cursor(false);
                }
            } else if key.code == KeyCode::Enter {
                // r<Enter> splits the line.
                self.checkpoint();
                self.buffer.delete_char(self.cursor);
                self.buffer.split_line(self.cursor);
                self.cursor.row += 1;
                self.cursor.col = 0;
            }
            return Action::None;
        }

        // Register name after `"` (e.g. `"a`).
        if self.expect_register {
            self.expect_register = false;
            if let KeyCode::Char(c) = key.code {
                // Keep the case: an uppercase name means "append" on write.
                self.pending_register = Some(c);
            }
            return Action::None;
        }

        // Pending `f`/`F`/`t`/`T` find-char target.
        if let Some(cmd) = self.pending_find.take() {
            if let KeyCode::Char(c) = key.code {
                self.do_find(cmd, c, self.pending_find_count);
                self.last_find = Some((cmd, c));
            }
            self.clamp_cursor(false);
            self.scroll_into_view();
            return Action::None;
        }

        // Target char after an operator + `f`/`F`/`t`/`T` (e.g. `dfx`, `ct)`).
        if let Some((op, fc, cnt)) = self.pending_op_find.take() {
            if let KeyCode::Char(target) = key.code {
                if let Some(col) = self.find_col(fc, target, cnt) {
                    let cur = self.cursor.col;
                    // f/t are forward+inclusive; F/T are backward.
                    let (s, e) = if fc == 'f' || fc == 't' {
                        (cur, col + 1)
                    } else {
                        (col, cur)
                    };
                    self.apply_op(op, OpTarget::Chars(s, e));
                }
            }
            return Action::None;
        }

        // Second bracket after `[` / `]` -> section motion (`[[ ]] [] ][`).
        if let Some(first) = self.pending_bracket.take() {
            let n = self.pending_bracket_count;
            match (first, key.code) {
                ('[', KeyCode::Char('[')) => self.section_motion(false, true, n),
                (']', KeyCode::Char(']')) => self.section_motion(true, true, n),
                ('[', KeyCode::Char(']')) => self.section_motion(false, false, n),
                (']', KeyCode::Char('[')) => self.section_motion(true, false, n),
                // Unmatched-bracket motions: `[(` / `[{` jump back to the
                // enclosing open bracket; `])` / `]}` forward to the close.
                ('[', KeyCode::Char('(')) => self.unmatched_bracket('(', ')', false, n),
                ('[', KeyCode::Char('{')) => self.unmatched_bracket('{', '}', false, n),
                (']', KeyCode::Char(')')) => self.unmatched_bracket('(', ')', true, n),
                (']', KeyCode::Char('}')) => self.unmatched_bracket('{', '}', true, n),
                // Indent-adjusted paste: `]p` below, `[p` / `[P` / `]P` above.
                (']', KeyCode::Char('p')) => self.paste_reindent(true),
                ('[', KeyCode::Char('p'))
                | ('[', KeyCode::Char('P'))
                | (']', KeyCode::Char('P')) => self.paste_reindent(false),
                _ => {}
            }
            return Action::None;
        }

        // Second key after `Z` -> `ZZ` (write & quit) or `ZQ` (quit, no save).
        if self.pending_z_quit {
            self.pending_z_quit = false;
            return match key.code {
                KeyCode::Char('Z') => Action::RunEx("x".into()),
                KeyCode::Char('Q') => Action::RunEx("q!".into()),
                _ => Action::None,
            };
        }

        // Motion / doubled after `gc` (comment toggle).
        if self.pending_comment {
            self.pending_comment = false;
            let rows = match key.code {
                KeyCode::Char('c') => Some((self.cursor.row, self.cursor.row)),
                code => self.motion_target(code, 1).map(|t| self.target_rows(t)),
            };
            if let Some((a, b)) = rows {
                self.toggle_comment_lines(a, b);
            }
            return Action::None;
        }

        // Motion (or doubled `q`) after `gq` — reflow those lines.
        if self.pending_format {
            self.pending_format = false;
            // `gqip` / `gqap` — wait for the text-object char after i/a.
            if matches!(key.code, KeyCode::Char('i') | KeyCode::Char('a')) {
                if let KeyCode::Char(iora) = key.code {
                    self.pending_format_obj = Some(iora);
                }
                return Action::None;
            }
            let rows = match key.code {
                KeyCode::Char('q') => Some((self.cursor.row, self.cursor.row)),
                code => self.motion_target(code, 1).map(|t| self.target_rows(t)),
            };
            if let Some((a, b)) = rows {
                self.reflow_lines(a, b);
            }
            return Action::None;
        }

        // Text-object char after `gq` + `i`/`a` (e.g. `gqip` reflows a paragraph).
        if let Some(iora) = self.pending_format_obj.take() {
            if let KeyCode::Char(obj) = key.code {
                if let Some(t) = self.text_object(iora, obj) {
                    let (a, b) = self.target_rows(t);
                    self.reflow_lines(a, b);
                }
            }
            return Action::None;
        }

        // Object char after a case operator + `i`/`a` (e.g. `guiw`).
        if let Some((cop, iora)) = self.pending_case_obj.take() {
            if let KeyCode::Char(obj) = key.code {
                if let Some(t) = self.text_object(iora, obj) {
                    self.apply_case_op(cop, t);
                }
            }
            return Action::None;
        }

        // Motion / object / doubled after `gu`/`gU`/`g~`.
        if let Some(cop) = self.pending_case.take() {
            match key.code {
                KeyCode::Char('i') => self.pending_case_obj = Some((cop, 'i')),
                KeyCode::Char('a') => self.pending_case_obj = Some((cop, 'a')),
                KeyCode::Char(c) if c == cop.key() => {
                    self.apply_case_op(cop, OpTarget::Lines(self.cursor.row, self.cursor.row));
                }
                code => {
                    if let Some(t) = self.motion_target(code, 1) {
                        self.apply_case_op(cop, t);
                    }
                }
            }
            return Action::None;
        }

        // Second `g` after an operator (e.g. `dgg` -> to top of file, line-wise).
        if let Some(op) = self.pending_op_gg.take() {
            match key.code {
                KeyCode::Char('g') => {
                    let target = OpTarget::Lines(0, self.cursor.row);
                    self.apply_op(op, target);
                }
                // `dgn` / `cgn` / `ygn` — operate on the next search match.
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    let forward = key.code == KeyCode::Char('n');
                    if let Some((row, sc, ec)) = self.next_match_range(forward) {
                        self.cursor = Position::new(row, sc);
                        self.apply_op(op, OpTarget::Chars(sc, ec));
                    }
                }
                _ => {}
            }
            return Action::None;
        }

        // Object char after visual-mode `i`/`a` (e.g. `viw`, `vi(`, `vap`): set
        // the selection to span the text object.
        if let Some(iora) = self.pending_vis_obj.take() {
            if let KeyCode::Char(obj) = key.code {
                if let Some(t) = self.text_object(iora, obj) {
                    match t {
                        OpTarget::Chars(s, e) => {
                            self.visual_anchor = Position::new(self.cursor.row, s);
                            self.cursor = Position::new(self.cursor.row, e.saturating_sub(1));
                        }
                        OpTarget::Lines(a, b) => {
                            self.visual_anchor = Position::new(a, 0);
                            let ec = self
                                .buffer
                                .line(b)
                                .map(|l| l.chars().count().saturating_sub(1))
                                .unwrap_or(0);
                            self.cursor = Position::new(b, ec);
                        }
                        OpTarget::Span(s, e) => {
                            self.visual_anchor = s;
                            self.cursor = e;
                        }
                    }
                    self.clamp_cursor(true);
                    self.scroll_into_view();
                }
            }
            return Action::None;
        }

        // Object char after `d`/`y`/`c`/`>`/`<` + `i`/`a` (e.g. `diw`, `ci(`, `>ip`).
        if let Some((op, iora, n)) = self.pending_textobj.take() {
            if let KeyCode::Char(obj) = key.code {
                if let Some(t) = self.text_object_count(iora, obj, n) {
                    if op == '>' || op == '<' {
                        let (a, b) = self.target_rows(t);
                        self.shift_range(a, b, op == '>');
                    } else {
                        self.apply_op(op, t);
                    }
                }
            }
            return Action::None;
        }

        // Mark letter after `m` / `` ` `` / `'`.
        if let Some(pm) = self.pending_mark.take() {
            if let KeyCode::Char(c) = key.code {
                match pm {
                    PendingMark::Set => {
                        self.marks.insert(c, self.cursor);
                    }
                    PendingMark::JumpExact => self.jump_to_mark(c, false),
                    PendingMark::JumpLine => self.jump_to_mark(c, true),
                }
            }
            return Action::None;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            match key.code {
                KeyCode::Char('r') => {
                    let n = self.pending_count.take().unwrap_or(1);
                    self.redo_times(n);
                    return Action::None;
                }
                KeyCode::Char('d') => {
                    self.move_down(self.view_rows / 2);
                    return Action::None;
                }
                KeyCode::Char('u') => {
                    self.move_up(self.view_rows / 2);
                    return Action::None;
                }
                KeyCode::Char('f') => {
                    let c = self.pending_count.take().unwrap_or(1);
                    self.page_scroll(true, c);
                    return Action::None;
                }
                KeyCode::Char('b') => {
                    let c = self.pending_count.take().unwrap_or(1);
                    self.page_scroll(false, c);
                    return Action::None;
                }
                KeyCode::Char('e') => {
                    self.scroll_view(1);
                    return Action::None;
                }
                KeyCode::Char('y') => {
                    self.scroll_view(-1);
                    return Action::None;
                }
                KeyCode::Char('o') => {
                    self.jump_back();
                    return Action::None;
                }
                KeyCode::Char('i') => {
                    self.jump_forward();
                    return Action::None;
                }
                KeyCode::Char('a') => {
                    self.modify_number_ctrl(1);
                    return Action::None;
                }
                KeyCode::Char('x') => {
                    self.modify_number_ctrl(-1);
                    return Action::None;
                }
                KeyCode::Char('v') => {
                    self.toggle_visual(Mode::VisualBlock);
                    return Action::None;
                }
                KeyCode::Char('g') => {
                    // `g Ctrl-g` reports document counts; `Ctrl-g` alone, file info.
                    if self.pending_op == Some('g') {
                        self.pending_op = None;
                        self.pending_op_count = None;
                        self.show_cursor_counts();
                    } else {
                        self.show_file_info();
                    }
                    return Action::None;
                }
                _ => {}
            }
        }

        // Ctrl-i arrives as Tab in most terminals -> jump forward.
        if key.code == KeyCode::Tab {
            self.jump_forward();
            return Action::None;
        }

        let code = key.code;

        // Count accumulation (digits, but a leading 0 is the "line start" motion).
        if let KeyCode::Char(c @ '0'..='9') = code {
            if !(c == '0' && self.pending_count.is_none()) {
                let d = c as usize - '0' as usize;
                self.pending_count = Some(self.pending_count.unwrap_or(0) * 10 + d);
                return Action::None;
            }
        }

        let had_count = self.pending_count.is_some();
        let count = self.pending_count.take().unwrap_or(1);

        // Operator-pending (d, y, c, g, z, >, <). The total count multiplies the
        // count typed before the operator by the count typed before the motion.
        if let Some(op) = self.pending_op.take() {
            let op_count = self.pending_op_count.take().unwrap_or(1);
            // `i`/`a` after d/y/c (and the shift operators >/<) begins a text
            // object (e.g. diw, ci(, >ip).
            if matches!(op, 'd' | 'y' | 'c' | '>' | '<')
                && matches!(code, KeyCode::Char('i') | KeyCode::Char('a'))
            {
                if let KeyCode::Char(iora) = code {
                    self.pending_textobj = Some((op, iora, op_count.saturating_mul(count)));
                }
                return Action::None;
            }
            // `g` after d/y/c awaits a second `g` (e.g. dgg).
            if matches!(op, 'd' | 'y' | 'c') && code == KeyCode::Char('g') {
                self.pending_op_gg = Some(op);
                return Action::None;
            }
            // `gf` / `gF` — open the file whose name is under the cursor (`gF`
            // also jumps to a trailing `:line` number).
            if op == 'g' && matches!(code, KeyCode::Char('f') | KeyCode::Char('F')) {
                return self.goto_file(code == KeyCode::Char('F'));
            }
            // `f`/`F`/`t`/`T` after d/y/c awaits the target char (e.g. dfx, ct)).
            if matches!(op, 'd' | 'y' | 'c')
                && matches!(
                    code,
                    KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Char('t') | KeyCode::Char('T')
                )
            {
                if let KeyCode::Char(fc) = code {
                    self.pending_op_find = Some((op, fc, op_count.saturating_mul(count)));
                }
                return Action::None;
            }
            self.apply_operator(op, code, op_count.saturating_mul(count));
            return Action::None;
        }

        match code {
            KeyCode::Char('h') | KeyCode::Left => self.move_left(count),
            KeyCode::Char('l') | KeyCode::Right => self.move_right(count, false),
            KeyCode::Char('j') | KeyCode::Down => self.move_down(count),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(count),
            KeyCode::Char('0') | KeyCode::Home => self.cursor.col = 0,
            KeyCode::Char('$') | KeyCode::End => {
                // In visual-block, `$` extends the block to each line's own end,
                // so a following `A` appends ragged-right.
                if self.mode == Mode::VisualBlock {
                    self.block_dollar = true;
                }
                self.move_line_end();
            }
            KeyCode::Char('^') => self.move_first_nonblank(),
            KeyCode::Char('+') | KeyCode::Enter => {
                self.move_down(count);
                self.move_first_nonblank();
            }
            KeyCode::Char('-') => {
                self.move_up(count);
                self.move_first_nonblank();
            }
            KeyCode::Char('_') => {
                self.move_down(count.saturating_sub(1));
                self.move_first_nonblank();
            }
            KeyCode::Char('|') => {
                let max = self.cur_len().saturating_sub(1);
                self.cursor.col = count.saturating_sub(1).min(max);
            }
            KeyCode::Char('w') => self.move_word_forward(count, false),
            KeyCode::Char('W') => self.move_word_forward(count, true),
            KeyCode::Char('b') => self.move_word_backward(count, false),
            KeyCode::Char('B') => self.move_word_backward(count, true),
            KeyCode::Char('e') => self.move_word_end(count, false),
            KeyCode::Char('E') => self.move_word_end(count, true),
            KeyCode::Char('f') => {
                self.pending_find = Some('f');
                self.pending_find_count = count;
            }
            KeyCode::Char('F') => {
                self.pending_find = Some('F');
                self.pending_find_count = count;
            }
            KeyCode::Char('t') => {
                self.pending_find = Some('t');
                self.pending_find_count = count;
            }
            KeyCode::Char('T') => {
                self.pending_find = Some('T');
                self.pending_find_count = count;
            }
            KeyCode::Char(';') => self.repeat_find(false, count),
            KeyCode::Char(',') => self.repeat_find(true, count),
            KeyCode::Char('%') => {
                if had_count {
                    // `{count}%` — jump to the line at `count` percent of the file.
                    self.goto_percent(count);
                } else if let Some(p) = self.matching_bracket() {
                    self.cursor = p;
                }
            }
            KeyCode::Char('}') => {
                self.cursor.row = self.paragraph_forward();
                self.cursor.col = 0;
            }
            KeyCode::Char('{') => {
                self.cursor.row = self.paragraph_backward();
                self.cursor.col = 0;
            }
            KeyCode::Char(')') => self.sentence_motion(true, count),
            KeyCode::Char('(') => self.sentence_motion(false, count),
            KeyCode::Char('[') => {
                self.pending_bracket = Some('[');
                self.pending_bracket_count = count;
            }
            KeyCode::Char(']') => {
                self.pending_bracket = Some(']');
                self.pending_bracket_count = count;
            }
            KeyCode::Char('"') => self.expect_register = true,
            KeyCode::Char('m') => self.pending_mark = Some(PendingMark::Set),
            KeyCode::Char('`') => self.pending_mark = Some(PendingMark::JumpExact),
            KeyCode::Char('\'') => self.pending_mark = Some(PendingMark::JumpLine),
            KeyCode::Char('q') => self.expect_macro = Some(MacroMode::Record),
            KeyCode::Char('@') => self.expect_macro = Some(MacroMode::Play),
            KeyCode::Char('G') => self.goto_line_or_end(count),
            KeyCode::Char('g') => {
                self.pending_op = Some('g');
                self.pending_op_count = Some(count);
            }
            KeyCode::Char('z') => {
                self.pending_op = Some('z');
                self.pending_op_count = Some(count);
            }
            KeyCode::Char('H') => {
                self.record_jump();
                let last = self.buffer.line_count().saturating_sub(1);
                let bottom = (self.top + self.view_rows.saturating_sub(1)).min(last);
                // `[count]H` -> count-1 lines below the top, bounded to the window.
                self.cursor.row = (self.top + count.saturating_sub(1)).min(bottom);
                self.move_first_nonblank();
            }
            KeyCode::Char('M') => {
                self.record_jump();
                let last = self.buffer.line_count().saturating_sub(1);
                self.cursor.row = (self.top + self.view_rows / 2).min(last);
                self.move_first_nonblank();
            }
            KeyCode::Char('L') => {
                self.record_jump();
                let last = self.buffer.line_count().saturating_sub(1);
                let bottom = (self.top + self.view_rows.saturating_sub(1)).min(last);
                // `[count]L` -> count-1 lines above the bottom, not above the top.
                self.cursor.row = bottom.saturating_sub(count.saturating_sub(1)).max(self.top);
                self.move_first_nonblank();
            }
            KeyCode::Char('d') => {
                if self.mode == Mode::VisualBlock {
                    self.block_delete();
                } else if self.mode.is_visual() {
                    self.visual_delete();
                } else {
                    self.pending_op = Some('d');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('y') => {
                if self.mode.is_visual() {
                    self.visual_yank();
                } else {
                    self.pending_op = Some('y');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('c') => {
                if self.mode == Mode::VisualBlock {
                    self.block_change();
                } else if self.mode.is_visual() {
                    self.visual_delete();
                    self.mode = Mode::Insert;
                } else {
                    self.pending_op = Some('c');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('x') => {
                if self.mode == Mode::VisualBlock {
                    self.block_delete();
                } else if self.mode.is_visual() {
                    self.visual_delete();
                } else {
                    self.delete_char_under(count);
                }
            }
            KeyCode::Char('X') => self.delete_char_before(count),
            KeyCode::Char('r') => {
                self.pending_replace = true;
                self.pending_replace_count = count;
            }
            KeyCode::Char('D') => self.delete_to_eol(),
            KeyCode::Char('C') => self.change_to_eol(),
            KeyCode::Char('Y') => {
                let last = self.buffer.line_count().saturating_sub(1);
                self.apply_op('y', OpTarget::Lines(self.cursor.row, (self.cursor.row + count - 1).min(last)));
            }
            KeyCode::Char('s') => {
                if self.mode.is_visual() {
                    self.visual_delete();
                    self.mode = Mode::Insert;
                } else {
                    self.substitute_char(count);
                }
            }
            KeyCode::Char('S') => self.substitute_line(),
            KeyCode::Char('~') => {
                if self.mode.is_visual() {
                    self.transform_selection(CaseOp::Toggle);
                } else {
                    self.toggle_case(count);
                }
            }
            KeyCode::Char('U') if self.mode.is_visual() => {
                self.transform_selection(CaseOp::Upper);
            }
            KeyCode::Char('U') => self.undo_line(),
            KeyCode::Char('>') => {
                if self.mode.is_visual() {
                    self.shift_selection(true);
                } else {
                    self.pending_op = Some('>');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('<') => {
                if self.mode.is_visual() {
                    self.shift_selection(false);
                } else {
                    self.pending_op = Some('<');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('!') => {
                if let Some((s, e)) = self.selection() {
                    // Visual `!` — prefill the filter command for the selection.
                    self.mode = Mode::Normal;
                    self.start_filter_cmdline(s.row, e.row);
                } else {
                    self.pending_op = Some('!');
                    self.pending_op_count = Some(count);
                }
            }
            KeyCode::Char('i') => {
                if self.mode.is_visual() {
                    self.pending_vis_obj = Some('i'); // viw, vi(, …
                } else {
                    self.enter_insert_here();
                    self.insert_repeat = count;
                    self.insert_entry = 'i';
                }
            }
            KeyCode::Char('a') => {
                if self.mode.is_visual() {
                    self.pending_vis_obj = Some('a'); // vaw, va(, …
                } else {
                    self.move_right(1, true);
                    self.enter_insert_here();
                    self.insert_repeat = count;
                    self.insert_entry = 'a';
                }
            }
            KeyCode::Char('I') => {
                if self.mode == Mode::VisualBlock {
                    self.block_insert_start(false);
                } else {
                    self.move_first_nonblank();
                    self.enter_insert_here();
                    self.insert_repeat = count;
                    self.insert_entry = 'I';
                }
            }
            KeyCode::Char('A') => {
                if self.mode == Mode::VisualBlock {
                    self.block_insert_start(true);
                } else {
                    self.move_line_end_exclusive();
                    self.enter_insert_here();
                    self.insert_repeat = count;
                    self.insert_entry = 'A';
                }
            }
            KeyCode::Char('R') => {
                self.checkpoint();
                self.replace_stack.clear();
                self.mode = Mode::Replace;
            }
            KeyCode::Char('Z') if !self.mode.is_visual() => self.pending_z_quit = true,
            KeyCode::Char('o') => {
                if self.mode.is_visual() {
                    // Swap the cursor and the anchor (move to the other end).
                    std::mem::swap(&mut self.cursor, &mut self.visual_anchor);
                } else {
                    self.open_below();
                    self.insert_repeat = count;
                    self.insert_entry = 'o';
                }
            }
            KeyCode::Char('O') => {
                self.open_above();
                self.insert_repeat = count;
                self.insert_entry = 'O';
            }
            KeyCode::Char('u') => {
                if self.mode.is_visual() {
                    self.transform_selection(CaseOp::Lower);
                } else {
                    self.undo_times(count);
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if self.mode.is_visual() {
                    self.visual_paste();
                } else {
                    let after = code == KeyCode::Char('p');
                    for _ in 0..count {
                        self.paste(after);
                    }
                }
            }
            KeyCode::Char('J') => {
                if self.mode.is_visual() {
                    self.join_selection(false);
                } else {
                    self.checkpoint();
                    for _ in 0..count.saturating_sub(1).max(1) {
                        if !self.buffer.join_line(self.cursor.row) {
                            break;
                        }
                    }
                }
            }
            KeyCode::Char('v') => self.toggle_visual(Mode::Visual),
            KeyCode::Char('V') => self.toggle_visual(Mode::VisualLine),
            // `n` repeats in the last search's direction; `N` reverses it. A
            // count repeats that many times (`3n` -> the 3rd next match).
            KeyCode::Char('n') => {
                for _ in 0..count {
                    self.search_repeat(self.search_forward);
                }
            }
            KeyCode::Char('N') => {
                for _ in 0..count {
                    self.search_repeat(!self.search_forward);
                }
            }
            KeyCode::Char('&') => self.repeat_substitute(),
            KeyCode::Char('*') => {
                if self.mode.is_visual() {
                    self.search_selection(true);
                } else {
                    self.search_word(true, true, count);
                }
            }
            KeyCode::Char('#') => {
                if self.mode.is_visual() {
                    self.search_selection(false);
                } else {
                    self.search_word(false, true, count);
                }
            }
            KeyCode::Char(':') => {
                // From visual mode, set the `'<`/`'>` marks to the selection and
                // prefill the range so the ex-command acts on it (`:'<,'>...`).
                let prefill = if self.mode.is_visual() {
                    self.selection().map(|(s, e)| {
                        self.marks.insert('<', s);
                        self.marks.insert('>', e);
                        "'<,'>".to_string()
                    })
                } else {
                    None
                };
                self.mode = Mode::Command;
                self.line_kind = LineKind::Ex;
                self.cmdline = prefill.unwrap_or_default();
                self.hist_idx = None;
            }
            KeyCode::Char('/') => self.enter_search(true),
            KeyCode::Char('?') => self.enter_search(false),
            KeyCode::Esc => {
                if self.mode.is_visual() {
                    self.mode = Mode::Normal;
                }
                self.pending_op = None;
                self.pending_op_count = None;
                self.pending_op_gg = None;
                self.pending_count = None;
            }
            _ => {}
        }

        self.clamp_cursor(false);
        self.scroll_into_view();
        Action::None
    }

    fn apply_operator(&mut self, op: char, code: KeyCode, count: usize) {
        match op {
            'g' => match code {
                KeyCode::Char('g') => {
                    self.record_jump();
                    // `gg` -> first line; `<n>gg` -> line n (first non-blank).
                    let last = self.buffer.line_count().saturating_sub(1);
                    self.cursor.row = count.saturating_sub(1).min(last);
                    self.move_first_nonblank();
                }
                KeyCode::Char('u') => self.pending_case = Some(CaseOp::Lower),
                KeyCode::Char('U') => self.pending_case = Some(CaseOp::Upper),
                KeyCode::Char('~') => self.pending_case = Some(CaseOp::Toggle),
                KeyCode::Char('?') => {
                    // g? — ROT13. On a selection, transform it now; otherwise
                    // wait for a motion / text object (g??, g?ap, g?w, …).
                    if self.mode.is_visual() {
                        self.transform_selection(CaseOp::Rot13);
                    } else {
                        self.pending_case = Some(CaseOp::Rot13);
                    }
                }
                KeyCode::Char('*') => self.search_word(true, false, count),
                KeyCode::Char('#') => self.search_word(false, false, count),
                KeyCode::Char('e') => self.move_word_end_back(count, false),
                KeyCode::Char('E') => self.move_word_end_back(count, true),
                KeyCode::Char('d') => self.goto_declaration(false),
                KeyCode::Char('D') => self.goto_declaration(true),
                KeyCode::Char('i') => {
                    // gi — resume insert at the last insert position.
                    let last = self.buffer.line_count().saturating_sub(1);
                    self.cursor.row = self.last_insert.row.min(last);
                    self.cursor.col = self.last_insert.col.min(self.cur_len());
                    self.enter_insert_here();
                }
                KeyCode::Char('J') => {
                    if self.mode.is_visual() {
                        self.join_selection(true);
                    } else {
                        self.checkpoint();
                        for _ in 0..count.saturating_sub(1).max(1) {
                            if !self.buffer.join_line_raw(self.cursor.row) {
                                break;
                            }
                        }
                    }
                }
                KeyCode::Char('_') => {
                    // g_ — last non-blank char (count-1 lines down).
                    self.move_down(count.saturating_sub(1));
                    self.cursor.col = self.last_nonblank_col();
                }
                KeyCode::Char(';') => self.change_jump(true),
                KeyCode::Char(',') => self.change_jump(false),
                KeyCode::Char('a') => self.show_char_info(),
                KeyCode::Char('I') => {
                    // gI — insert at the very first column (before any indent).
                    self.cursor.col = 0;
                    self.enter_insert_here();
                    self.insert_repeat = count;
                    self.insert_entry = 'I';
                }
                KeyCode::Char('p') => self.paste_g(true),
                KeyCode::Char('P') => self.paste_g(false),
                KeyCode::Char('&') => self.repeat_substitute_all(),
                KeyCode::Char('q') | KeyCode::Char('w') => {
                    // gq / gw — reflow. On a selection, format it now; otherwise
                    // wait for a motion.
                    if let Some((s, e)) = self.selection() {
                        self.reflow_lines(s.row, e.row);
                        self.mode = Mode::Normal;
                    } else {
                        self.pending_format = true;
                    }
                }
                KeyCode::Char('n') => self.select_next_match(true),
                KeyCode::Char('N') => self.select_next_match(false),
                KeyCode::Char('v') => {
                    // gv — reselect the last visual selection.
                    if let Some((s, e, m)) = self.last_visual {
                        self.mode = m;
                        self.visual_anchor = s;
                        self.cursor = e;
                        self.clamp_cursor(false);
                        self.scroll_into_view();
                    }
                }
                KeyCode::Char('c') => {
                    if let Some((s, e)) = self.selection() {
                        self.toggle_comment_lines(s.row, e.row);
                        self.mode = Mode::Normal;
                    } else {
                        self.pending_comment = true;
                    }
                }
                _ => {}
            },
            'z' => match code {
                KeyCode::Char('z') => self.center_line(),
                KeyCode::Char('t') => self.line_to_top(),
                KeyCode::Char('b') => self.line_to_bottom(),
                // The `.`/`<CR>`/`-` variants also move to the first non-blank.
                KeyCode::Char('.') => {
                    self.center_line();
                    self.move_first_nonblank();
                }
                KeyCode::Enter => {
                    self.line_to_top();
                    self.move_first_nonblank();
                }
                KeyCode::Char('-') => {
                    self.line_to_bottom();
                    self.move_first_nonblank();
                }
                _ => {}
            },
            '>' | '<' => {
                let last = self.buffer.line_count().saturating_sub(1);
                let rows = if code == KeyCode::Char(op) {
                    // Doubled (`>>`/`<<`): `count` lines from the cursor.
                    Some((self.cursor.row, (self.cursor.row + count - 1).min(last)))
                } else {
                    self.motion_target(code, count).map(|t| self.target_rows(t))
                };
                if let Some((a, b)) = rows {
                    self.shift_range(a, b, op == '>');
                }
            }
            '!' => {
                // `!!` (current line, counted) or `!{motion}` prefill the filter
                // command line with the matching range.
                let last = self.buffer.line_count().saturating_sub(1);
                let rows = if code == KeyCode::Char('!') {
                    Some((self.cursor.row, (self.cursor.row + count - 1).min(last)))
                } else {
                    self.motion_target(code, count).map(|t| self.target_rows(t))
                };
                if let Some((a, b)) = rows {
                    self.start_filter_cmdline(a, b);
                }
            }
            'd' | 'y' | 'c' => {
                // Doubled operator (dd/yy/cc) acts on `count` whole lines.
                let doubled = code == KeyCode::Char(op);
                // `cw`/`cW` behave like `ce`/`cE` (vim's special case).
                let motion = if op == 'c' {
                    match code {
                        KeyCode::Char('w') => KeyCode::Char('e'),
                        KeyCode::Char('W') => KeyCode::Char('E'),
                        other => other,
                    }
                } else {
                    code
                };
                let target = if doubled {
                    let last = self.buffer.line_count().saturating_sub(1);
                    OpTarget::Lines(self.cursor.row, (self.cursor.row + count - 1).min(last))
                } else {
                    match self.motion_target(motion, count) {
                        Some(t) => t,
                        None => return,
                    }
                };
                self.apply_op(op, target);
            }
            _ => {}
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// The inclusive row range an operator target touches, for the line-wise
    /// operators (`gc`, `gq`, `>`/`<`) that act on whole lines regardless of the
    /// target's shape. A charwise `Chars` target stays on the cursor's row.
    fn target_rows(&self, target: OpTarget) -> (usize, usize) {
        match target {
            OpTarget::Lines(a, b) => (a, b),
            OpTarget::Chars(_, _) => (self.cursor.row, self.cursor.row),
            OpTarget::Span(s, e) => (s.row, e.row),
        }
    }

    /// The text span a motion covers, relative to the cursor, for use by an
    /// operator (`d`/`y`/`c`). `None` for keys that aren't operator motions.
    fn motion_target(&self, code: KeyCode, count: usize) -> Option<OpTarget> {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let len = self.cur_len();
        let last = self.buffer.line_count().saturating_sub(1);
        Some(match code {
            KeyCode::Char('w') => OpTarget::Chars(col, self.word_forward_col_n(count, false)),
            KeyCode::Char('W') => OpTarget::Chars(col, self.word_forward_col_n(count, true)),
            KeyCode::Char('e') => OpTarget::Chars(col, (self.word_end_col_n(count, false) + 1).min(len)),
            KeyCode::Char('E') => OpTarget::Chars(col, (self.word_end_col_n(count, true) + 1).min(len)),
            KeyCode::Char('$') | KeyCode::End => OpTarget::Chars(col, len),
            KeyCode::Char('0') | KeyCode::Home => OpTarget::Chars(0, col),
            KeyCode::Char('^') => OpTarget::Chars(self.first_nonblank_col(), col),
            KeyCode::Char('l') | KeyCode::Right => OpTarget::Chars(col, (col + count).min(len)),
            KeyCode::Char('h') | KeyCode::Left => OpTarget::Chars(col.saturating_sub(count), col),
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('+') | KeyCode::Enter => {
                OpTarget::Lines(row, (row + count).min(last))
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('-') => {
                OpTarget::Lines(row.saturating_sub(count), row)
            }
            KeyCode::Char('_') => OpTarget::Lines(row, (row + count - 1).min(last)),
            KeyCode::Char('G') => OpTarget::Lines(row, last),
            // Screen motions are line-wise operator targets (`dH`, `dL`, `dM`).
            KeyCode::Char('H') => {
                let bottom = (self.top + self.view_rows.saturating_sub(1)).min(last);
                OpTarget::Lines((self.top + count.saturating_sub(1)).min(bottom), row)
            }
            KeyCode::Char('M') => OpTarget::Lines(row, (self.top + self.view_rows / 2).min(last)),
            KeyCode::Char('L') => {
                let bottom = (self.top + self.view_rows.saturating_sub(1)).min(last);
                OpTarget::Lines(row, bottom.saturating_sub(count.saturating_sub(1)).max(self.top))
            }
            // `%` — operate from the cursor to the matching bracket, inclusive
            // (charwise, may cross lines). `d%`, `y%`, `c%`.
            KeyCode::Char('%') => {
                let m = self.matching_bracket()?;
                let cur = self.cursor;
                let (a, b) = if (m.row, m.col) >= (cur.row, cur.col) {
                    (cur, m)
                } else {
                    (m, cur)
                };
                return Some(OpTarget::Span(a, b));
            }
            _ => return None,
        })
    }

    /// Apply operator `op` to a computed target span.
    fn apply_op(&mut self, op: char, target: OpTarget) {
        let is_change = op == 'c';
        let is_delete = op == 'd' || is_change;
        match target {
            OpTarget::Chars(s, e) => {
                let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
                let len = chars.len();
                let (s, e) = (s.min(len), e.min(len));
                let (s, e) = (s.min(e), s.max(e));
                let text: String = chars[s..e].iter().collect();
                let row = self.cursor.row;
                if is_delete {
                    self.checkpoint();
                    let kept: String = chars[..s].iter().chain(&chars[e..]).collect();
                    self.buffer.set_line(self.cursor.row, kept);
                    self.store_delete(text, false);
                    self.cursor.col = s;
                    self.set_change_marks(Position::new(row, s), Position::new(row, s));
                    if is_change {
                        self.mode = Mode::Insert;
                    }
                } else {
                    self.store_yank(text, false);
                    self.cursor.col = s;
                    let end = if e > s { e - 1 } else { s };
                    self.set_change_marks(Position::new(row, s), Position::new(row, end));
                }
            }
            OpTarget::Lines(a, b) => {
                let last = self.buffer.line_count().saturating_sub(1);
                let (a, b) = (a.min(last), b.min(last));
                let (a, b) = (a.min(b), a.max(b));
                let text = (a..=b)
                    .map(|r| self.buffer.line(r).unwrap_or("").to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                if is_delete {
                    self.checkpoint();
                    for _ in a..=b {
                        if self.buffer.line_count() == 1 {
                            self.buffer.set_line(0, "");
                            break;
                        }
                        self.buffer.delete_line(a);
                    }
                    self.store_delete(text, true);
                    if is_change {
                        let at = a.min(self.buffer.line_count());
                        self.buffer.insert_line(at, "");
                        self.cursor = Position::new(at, 0);
                        self.mode = Mode::Insert;
                    } else {
                        self.cursor.row = a.min(self.buffer.line_count().saturating_sub(1));
                        self.move_first_nonblank();
                    }
                    let m = self.cursor.row;
                    self.set_change_marks(Position::new(m, 0), Position::new(m, 0));
                } else {
                    let end_col = self
                        .buffer
                        .line(b)
                        .map(|l| l.chars().count().saturating_sub(1))
                        .unwrap_or(0);
                    self.store_yank(text, true);
                    self.cursor.row = a;
                    self.set_change_marks(Position::new(a, 0), Position::new(b, end_col));
                }
            }
            OpTarget::Span(start, end) => {
                // Inclusive charwise span across lines (multi-line text objects).
                let text = self.extract_range(start, end, false);
                if is_delete {
                    self.checkpoint();
                    self.store_delete(text, false);
                    self.delete_range(start, end, false);
                    self.cursor = start;
                    self.set_change_marks(start, start);
                    if is_change {
                        self.mode = Mode::Insert;
                    }
                } else {
                    self.store_yank(text, false);
                    self.cursor = start;
                    self.set_change_marks(start, end);
                }
            }
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Apply a case transform (`gu`/`gU`/`g~`) over a computed span.
    fn apply_case_op(&mut self, cop: CaseOp, target: OpTarget) {
        self.checkpoint();
        match target {
            OpTarget::Chars(s, e) => {
                let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
                let len = chars.len();
                let (s, e) = (s.min(len), e.min(len));
                let (s, e) = (s.min(e), s.max(e));
                let new: String = chars
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| if i >= s && i < e { cop.apply(c) } else { c })
                    .collect();
                self.buffer.set_line(self.cursor.row, new);
                self.cursor.col = s;
            }
            OpTarget::Lines(a, b) => {
                let last = self.buffer.line_count().saturating_sub(1);
                let (a, b) = (a.min(last), b.min(last));
                let (a, b) = (a.min(b), a.max(b));
                for row in a..=b {
                    let new: String =
                        self.buffer.line(row).unwrap_or("").chars().map(|c| cop.apply(c)).collect();
                    self.buffer.set_line(row, new);
                }
                self.cursor.row = a;
            }
            OpTarget::Span(start, end) => {
                // Transform an inclusive charwise span across lines.
                for row in start.row..=end.row {
                    let chars: Vec<char> =
                        self.buffer.line(row).unwrap_or("").chars().collect();
                    let len = chars.len();
                    let c0 = if row == start.row { start.col.min(len) } else { 0 };
                    let c1 = if row == end.row { (end.col + 1).min(len) } else { len };
                    let new: String = chars
                        .iter()
                        .enumerate()
                        .map(|(i, &c)| if i >= c0 && i < c1 { cop.apply(c) } else { c })
                        .collect();
                    self.buffer.set_line(row, new);
                }
                self.cursor = start;
            }
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// `gq` — reflow the inclusive row range to the current `textwidth` (or 79
    /// when unset), greedily wrapping words. The first line's leading indent is
    /// preserved on every wrapped line; blank lines are left as paragraph breaks.
    fn reflow_lines(&mut self, a: usize, b: usize) {
        let last = self.buffer.line_count().saturating_sub(1);
        let (a, b) = (a.min(last), b.min(last));
        let width = if self.textwidth == 0 { 79 } else { self.textwidth };
        // Preserve the indent of the first line.
        let first = self.buffer.line(a).unwrap_or("");
        let indent: String = first.chars().take_while(|c| c.is_whitespace()).collect();
        // Gather all words across the range.
        let mut words: Vec<String> = Vec::new();
        for row in a..=b {
            for w in self.buffer.line(row).unwrap_or("").split_whitespace() {
                words.push(w.to_string());
            }
        }
        if words.is_empty() {
            return;
        }
        // Greedy wrap.
        let mut out: Vec<String> = Vec::new();
        let mut cur = indent.clone();
        for w in words {
            let candidate = if cur.trim().is_empty() {
                format!("{cur}{w}")
            } else {
                format!("{cur} {w}")
            };
            if candidate.chars().count() > width && cur.trim() != "" {
                out.push(cur);
                cur = format!("{indent}{w}");
            } else {
                cur = candidate;
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
        self.checkpoint();
        // Insert the reflowed lines before the old range, then delete the old
        // lines (now shifted down). Doing it in this order means the buffer is
        // never momentarily empty, so no sentinel blank line is left behind.
        for (k, line) in out.iter().enumerate() {
            self.buffer.insert_line(a + k, line.clone());
        }
        let count = b - a + 1;
        for _ in 0..count {
            self.buffer.delete_line(a + out.len());
        }
        self.cursor.row = (a + out.len()).saturating_sub(1).min(self.buffer.line_count().saturating_sub(1));
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Toggle line comments over an inclusive row range using the current
    /// language's comment marker. If every non-blank line is already commented,
    /// uncomment; otherwise comment.
    fn toggle_comment_lines(&mut self, a: usize, b: usize) {
        let Some(token) = line_comment_token(self.language) else {
            self.message = "No comment marker for this filetype".into();
            return;
        };
        let last = self.buffer.line_count().saturating_sub(1);
        let (a, b) = (a.min(last), b.min(last));
        let (a, b) = (a.min(b), a.max(b));

        // Are all non-blank lines already commented?
        let mut any_nonblank = false;
        let all_commented = (a..=b).all(|r| {
            let line = self.buffer.line(r).unwrap_or("");
            let t = line.trim_start();
            if t.is_empty() {
                true
            } else {
                any_nonblank = true;
                t.starts_with(token)
            }
        });
        if !any_nonblank {
            return;
        }

        self.checkpoint();
        for r in a..=b {
            let line = self.buffer.line(r).unwrap_or("").to_string();
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue; // leave blank lines untouched
            }
            let indent_len = line.len() - trimmed.len();
            let (indent, rest) = line.split_at(indent_len);
            if all_commented {
                // Remove the token and a single following space if present.
                let mut stripped = rest.strip_prefix(token).unwrap_or(rest);
                stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
                self.buffer.set_line(r, format!("{indent}{stripped}"));
            } else {
                self.buffer.set_line(r, format!("{indent}{token} {rest}"));
            }
        }
        self.cursor.row = a;
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    fn first_nonblank_col(&self) -> usize {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        line.chars().take_while(|c| c.is_whitespace()).count()
    }

    /// Column of the last non-blank character on the current line.
    fn last_nonblank_col(&self) -> usize {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        line.trim_end().chars().count().saturating_sub(1)
    }

    fn is_blank_row(&self, row: usize) -> bool {
        self.buffer
            .line(row)
            .map(|l| l.trim().is_empty())
            .unwrap_or(true)
    }

    /// `}` — the next blank line after the cursor (or the last line).
    fn paragraph_forward(&self) -> usize {
        let n = self.buffer.line_count();
        let mut r = self.cursor.row + 1;
        while r < n && !self.is_blank_row(r) {
            r += 1;
        }
        r.min(n.saturating_sub(1))
    }

    /// `{` — the previous blank line before the cursor (or the first line).
    fn paragraph_backward(&self) -> usize {
        if self.cursor.row == 0 {
            return 0;
        }
        let mut r = self.cursor.row - 1;
        while r > 0 && !self.is_blank_row(r) {
            r -= 1;
        }
        r
    }

    /// Flatten the whole buffer to a char vector with a parallel position map.
    /// A line break between two lines is a `'\n'` char mapped to the end of the
    /// upper line. Used by the sentence motions.
    fn flatten(&self) -> (Vec<char>, Vec<Position>) {
        let n = self.buffer.line_count();
        let mut flat = Vec::new();
        let mut map = Vec::new();
        for row in 0..n {
            let line = self.buffer.line(row).unwrap_or("");
            for (col, ch) in line.chars().enumerate() {
                flat.push(ch);
                map.push(Position::new(row, col));
            }
            if row + 1 < n {
                flat.push('\n');
                map.push(Position::new(row, line.chars().count()));
            }
        }
        (flat, map)
    }

    /// Offsets in `flat` where a sentence begins: the start, the first non-blank
    /// after `.`/`!`/`?` (plus any closing `)]"'`) followed by whitespace, and
    /// blank lines (paragraph boundaries).
    fn sentence_starts(flat: &[char]) -> Vec<usize> {
        let n = flat.len();
        let is_ws = |c: char| c == ' ' || c == '\t' || c == '\n';
        let mut starts = vec![0usize];
        let mut i = 0;
        while i < n {
            let c = flat[i];
            if matches!(c, '.' | '!' | '?') {
                let mut j = i + 1;
                while j < n && matches!(flat[j], ')' | ']' | '"' | '\'') {
                    j += 1;
                }
                if j < n && is_ws(flat[j]) {
                    while j < n && is_ws(flat[j]) {
                        j += 1;
                    }
                    if j < n {
                        starts.push(j);
                    }
                    i = j;
                    continue;
                }
            }
            // A blank line (consecutive newlines) is a paragraph boundary.
            if c == '\n' && i + 1 < n && flat[i + 1] == '\n' {
                let mut j = i + 1;
                while j < n && flat[j] == '\n' {
                    starts.push(j);
                    j += 1;
                }
                if j < n {
                    starts.push(j);
                }
                i = j;
                continue;
            }
            i += 1;
        }
        starts.sort_unstable();
        starts.dedup();
        starts
    }

    /// `)` / `(` — move forward / backward by `count` sentences.
    fn sentence_motion(&mut self, forward: bool, count: usize) {
        let (flat, map) = self.flatten();
        if flat.is_empty() {
            return;
        }
        let starts = Self::sentence_starts(&flat);
        let cur = map.iter().position(|&p| p == self.cursor).unwrap_or(0);
        let target = if forward {
            let first = starts.partition_point(|&s| s <= cur); // first start > cur
            let idx = first + count.max(1) - 1;
            if idx < starts.len() {
                starts[idx]
            } else {
                flat.len() // past the last sentence -> end of buffer
            }
        } else {
            let here = starts.partition_point(|&s| s <= cur); // count of starts <= cur
            let cur_idx = here.saturating_sub(1);
            let cur_is_start = here > 0 && starts[cur_idx] == cur;
            let steps = if cur_is_start { count.max(1) } else { count.max(1) - 1 };
            starts[cur_idx.saturating_sub(steps)]
        };
        let pos = if target >= map.len() {
            *map.last().unwrap()
        } else {
            map[target]
        };
        self.cursor = pos;
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Section motion: `]]`/`[[` jump to the next/previous line whose first
    /// character (column 0) is `{` (an *open*-brace boundary); `][`/`[]` do the
    /// same for `}` (a *close*-brace boundary). This matches vim's default C-style
    /// `sections` navigation (a brace in the first column), so the skill transfers
    /// directly. A jump is recorded so `Ctrl-o` returns. Lands on column 0 of the
    /// boundary line, or the first/last line when no further boundary exists.
    fn section_motion(&mut self, forward: bool, open: bool, count: usize) {
        let marker = if open { '{' } else { '}' };
        let last = self.buffer.line_count().saturating_sub(1);
        self.record_jump();
        let starts_with = |row: usize| -> bool {
            self.buffer.line(row).and_then(|l| l.chars().next()) == Some(marker)
        };
        let mut row = self.cursor.row;
        for _ in 0..count.max(1) {
            if forward {
                let mut r = row + 1;
                while r <= last && !starts_with(r) {
                    r += 1;
                }
                row = r.min(last);
            } else {
                if row == 0 {
                    break;
                }
                let mut r = row - 1;
                while r > 0 && !starts_with(r) {
                    r -= 1;
                }
                row = r;
            }
        }
        self.cursor.row = row;
        self.cursor.col = 0;
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// The column `count` word-starts forward on the current line (bounded to EOL).
    fn word_forward_col_n(&self, count: usize, big: bool) -> usize {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let len = chars.len();
        let mut col = self.cursor.col;
        for _ in 0..count.max(1) {
            if col >= len {
                break;
            }
            let class = Self::class_of(chars[col], big);
            while col < len && Self::class_of(chars[col], big) == class && class != 0 {
                col += 1;
            }
            while col < len && chars[col].is_whitespace() {
                col += 1;
            }
        }
        col
    }

    /// The column of the end of the `count`-th word forward on the current line.
    fn word_end_col_n(&self, count: usize, big: bool) -> usize {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let len = chars.len();
        let mut i = self.cursor.col;
        for _ in 0..count.max(1) {
            i += 1;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            if i < len {
                let class = Self::class_of(chars[i], big);
                while i + 1 < len && Self::class_of(chars[i + 1], big) == class {
                    i += 1;
                }
            }
        }
        if i < len {
            i
        } else {
            len.saturating_sub(1)
        }
    }

    // ---- motions ---------------------------------------------------------

    fn cur_len(&self) -> usize {
        self.buffer.line_len(self.cursor.row)
    }

    fn move_left(&mut self, n: usize) {
        self.cursor.col = self.cursor.col.saturating_sub(n);
    }

    fn move_right(&mut self, n: usize, allow_eol: bool) {
        let max = if allow_eol {
            self.cur_len()
        } else {
            self.cur_len().saturating_sub(1)
        };
        self.cursor.col = (self.cursor.col + n).min(max);
    }

    fn move_up(&mut self, n: usize) {
        self.cursor.row = self.cursor.row.saturating_sub(n);
    }

    fn move_down(&mut self, n: usize) {
        self.cursor.row = (self.cursor.row + n).min(self.buffer.line_count().saturating_sub(1));
    }

    fn move_line_end(&mut self) {
        self.cursor.col = self.cur_len().saturating_sub(1);
    }

    fn move_line_end_exclusive(&mut self) {
        self.cursor.col = self.cur_len();
    }

    fn move_first_nonblank(&mut self) {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        let col = line.chars().take_while(|c| c.is_whitespace()).count();
        self.cursor.col = col.min(self.cur_len().saturating_sub(1));
    }

    fn goto_line_or_end(&mut self, count: usize) {
        self.record_jump();
        // With an explicit count `G` goes to that line; bare `G` goes to end.
        let target = if self.pending_count_was_explicit(count) {
            count.saturating_sub(1)
        } else {
            self.buffer.line_count().saturating_sub(1)
        };
        self.cursor.row = target.min(self.buffer.line_count().saturating_sub(1));
        self.move_first_nonblank();
    }

    // `count` defaulted to 1; treat 1 as "bare" for G (matches common use).
    fn pending_count_was_explicit(&self, count: usize) -> bool {
        count != 1
    }

    /// `{count}%` — jump to the line `count` percent of the way through the file
    /// (vim's formula), landing on its first non-blank. Records a jump.
    fn goto_percent(&mut self, pct: usize) {
        let n = self.buffer.line_count();
        if n == 0 {
            return;
        }
        let pct = pct.min(100);
        let line = (pct * n).div_ceil(100).clamp(1, n);
        self.record_jump();
        self.cursor.row = line - 1;
        self.move_first_nonblank();
        self.scroll_into_view();
    }

    fn char_class(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || c == '_' {
            1
        } else {
            2
        }
    }

    /// Character class for word motions. `big` collapses word/punctuation into a
    /// single class so `W`/`B`/`E` treat whitespace-delimited WORDs.
    fn class_of(c: char, big: bool) -> u8 {
        if big {
            if c.is_whitespace() {
                0
            } else {
                1
            }
        } else {
            Self::char_class(c)
        }
    }

    fn move_word_forward(&mut self, count: usize, big: bool) {
        for _ in 0..count {
            let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
            let mut col = self.cursor.col;
            if col >= chars.len() {
                // move to next line start
                if self.cursor.row + 1 < self.buffer.line_count() {
                    self.cursor.row += 1;
                    self.cursor.col = 0;
                }
                continue;
            }
            let start_class = Self::class_of(chars[col], big);
            // skip current run
            while col < chars.len() && Self::class_of(chars[col], big) == start_class && start_class != 0
            {
                col += 1;
            }
            // skip whitespace
            while col < chars.len() && chars[col].is_whitespace() {
                col += 1;
            }
            if col >= chars.len() && self.cursor.row + 1 < self.buffer.line_count() {
                self.cursor.row += 1;
                self.cursor.col = 0;
            } else {
                self.cursor.col = col;
            }
        }
    }

    /// Compute a text object span on the current line (count 1). `iora` is `i`
    /// (inner) or `a` (around); `obj` selects the object (`w`, brackets, quotes).
    fn text_object(&self, iora: char, obj: char) -> Option<OpTarget> {
        self.text_object_count(iora, obj, 1)
    }

    /// Like [`text_object`](Editor::text_object) but honours a `count` for the
    /// objects where it is meaningful: word (`d3iw`, `d2aw`) and paragraph
    /// (`d2ap`). Other objects ignore the count.
    fn text_object_count(&self, iora: char, obj: char, count: usize) -> Option<OpTarget> {
        let around = iora == 'a';
        let count = count.max(1);

        // Paragraph object (line-wise), valid even on a blank line.
        if obj == 'p' {
            let n = self.buffer.line_count();
            let mut blank = self.is_blank_row(self.cursor.row);
            let mut a = self.cursor.row;
            while a > 0 && self.is_blank_row(a - 1) == blank {
                a -= 1;
            }
            let mut b = self.cursor.row;
            while b + 1 < n && self.is_blank_row(b + 1) == blank {
                b += 1;
            }
            if around {
                while b + 1 < n && self.is_blank_row(b + 1) {
                    b += 1;
                }
            }
            // Extend over further paragraphs (and the blank runs between them).
            for _ in 1..count {
                if b + 1 >= n {
                    break;
                }
                blank = self.is_blank_row(b + 1);
                b += 1;
                while b + 1 < n && self.is_blank_row(b + 1) == blank {
                    b += 1;
                }
                if around {
                    while b + 1 < n && self.is_blank_row(b + 1) {
                        b += 1;
                    }
                }
            }
            return Some(OpTarget::Lines(a, b));
        }

        // Indentation object (line-wise): `ii` selects the run of lines indented
        // at least as far as the current line (blank lines strictly inside the
        // block are kept); `ai` also takes the less-indented header line above.
        if obj == 'i' {
            return self.indent_object(around);
        }

        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        if chars.is_empty() {
            return None;
        }
        let col = self.cursor.col.min(chars.len() - 1);

        // Word object (`w` = word, `W` = WORD).
        if obj == 'w' || obj == 'W' {
            let big = obj == 'W';
            let class = Self::class_of(chars[col], big);
            let mut start = col;
            while start > 0 && Self::class_of(chars[start - 1], big) == class {
                start -= 1;
            }
            let mut end = col + 1;
            while end < chars.len() && Self::class_of(chars[end], big) == class {
                end += 1;
            }
            if around {
                let before = end;
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
                // If no trailing whitespace, absorb leading whitespace instead.
                if end == before {
                    while start > 0 && chars[start - 1].is_whitespace() {
                        start -= 1;
                    }
                }
            }
            // A count takes further units. `iw` counts maximal same-class runs
            // (word, then space, then word, …); `aw` counts whole words together
            // with their trailing whitespace.
            for _ in 1..count {
                if end >= chars.len() {
                    break;
                }
                let cl = Self::class_of(chars[end], big);
                while end < chars.len() && Self::class_of(chars[end], big) == cl {
                    end += 1;
                }
                if around {
                    while end < chars.len() && chars[end].is_whitespace() {
                        end += 1;
                    }
                }
            }
            return Some(OpTarget::Chars(start, end));
        }

        // Sentence object (`s`), scoped to the current line. Sentences break at
        // `.`/`!`/`?` + optional closing `)]"'` + whitespace. `is` excludes the
        // trailing whitespace; `as` keeps it.
        if obj == 's' {
            let is_ws = |c: char| c == ' ' || c == '\t';
            let mut starts = vec![0usize];
            let mut i = 0;
            while i < chars.len() {
                if matches!(chars[i], '.' | '!' | '?') {
                    let mut j = i + 1;
                    while j < chars.len() && matches!(chars[j], ')' | ']' | '"' | '\'') {
                        j += 1;
                    }
                    if j < chars.len() && is_ws(chars[j]) {
                        while j < chars.len() && is_ws(chars[j]) {
                            j += 1;
                        }
                        if j < chars.len() {
                            starts.push(j);
                        }
                        i = j;
                        continue;
                    }
                }
                i += 1;
            }
            let s = starts.iter().rev().find(|&&st| st <= col).copied().unwrap_or(0);
            let mut e = starts.iter().find(|&&st| st > col).copied().unwrap_or(chars.len());
            if !around {
                while e > s && chars[e - 1].is_whitespace() {
                    e -= 1;
                }
            }
            return Some(OpTarget::Chars(s, e));
        }

        // Tag object (`it` inner / `at` around an XML/HTML `<name>…</name>` pair).
        if obj == 't' {
            return self.tag_object(around);
        }

        // Pair / quote objects.
        let (open, close) = match obj {
            '(' | ')' | 'b' => ('(', ')'),
            '{' | '}' | 'B' => ('{', '}'),
            '[' | ']' => ('[', ']'),
            '<' | '>' => ('<', '>'),
            '"' => ('"', '"'),
            '\'' => ('\'', '\''),
            '`' => ('`', '`'),
            _ => return None,
        };
        if open == close {
            let (o, c) = Self::find_quotes(&chars, col, open)?;
            return Some(if around {
                OpTarget::Chars(o, c + 1)
            } else {
                OpTarget::Chars(o + 1, c)
            });
        }
        // Bracket pair: try the current line first (fast; keeps single-line
        // behavior), then fall back to an enclosing pair that spans lines.
        if let Some((o, c)) = Self::find_pair(&chars, col, open, close) {
            return Some(if around {
                OpTarget::Chars(o, c + 1)
            } else {
                OpTarget::Chars(o + 1, c)
            });
        }
        let (op, cp) = self.find_pair_pos(open, close)?;
        if around {
            Some(OpTarget::Span(op, cp))
        } else {
            let start = self.pos_after(op);
            let end = self.pos_before(cp);
            if start.row > end.row || (start.row == end.row && start.col > end.col) {
                // Empty inner region (adjacent brackets) -> no-op.
                Some(OpTarget::Chars(self.cursor.col, self.cursor.col))
            } else {
                Some(OpTarget::Span(start, end))
            }
        }
    }

    /// The `it` / `at` tag text object. Finds the innermost `<name …>…</name>`
    /// pair enclosing the cursor (across lines, honouring nesting). `it` selects
    /// the content between the tags; `at` selects the whole thing including both
    /// tags. Self-closing tags (`<br/>`) are ignored.
    fn tag_object(&self, around: bool) -> Option<OpTarget> {
        let (flat, map) = self.flatten();
        if flat.is_empty() {
            return None;
        }
        let cur = map
            .iter()
            .position(|p| p.row == self.cursor.row && p.col >= self.cursor.col)
            .unwrap_or(flat.len() - 1)
            .min(flat.len() - 1);

        // Parse every (non self-closing) tag as (name, is_close, lt, gt), where
        // `lt`/`gt` are the offsets of its `<` and `>` in the flattened text.
        let mut tags: Vec<(String, bool, usize, usize)> = Vec::new();
        let mut i = 0;
        while i < flat.len() {
            if flat[i] == '<' {
                if let Some(rel) = flat[i + 1..].iter().position(|&c| c == '>') {
                    let gt = i + 1 + rel;
                    let mut k = i + 1;
                    let is_close = flat.get(k) == Some(&'/');
                    if is_close {
                        k += 1;
                    }
                    let mut name = String::new();
                    while k < gt
                        && (flat[k].is_alphanumeric() || matches!(flat[k], '-' | '_' | ':'))
                    {
                        name.push(flat[k]);
                        k += 1;
                    }
                    let is_self = gt > 0 && flat[gt - 1] == '/';
                    if !name.is_empty() && !is_self {
                        tags.push((name, is_close, i, gt));
                    }
                    i = gt + 1;
                    continue;
                }
            }
            i += 1;
        }

        // Match open/close tags with a stack; the first completed pair that
        // encloses the cursor is the innermost one.
        let mut stack: Vec<usize> = Vec::new();
        for idx in 0..tags.len() {
            let (name, is_close, lt, gt) = (&tags[idx].0, tags[idx].1, tags[idx].2, tags[idx].3);
            if !is_close {
                stack.push(idx);
            } else if let Some(pos) = stack.iter().rposition(|&oi| tags[oi].0 == *name) {
                let (_, _, olt, ogt) = tags[stack[pos]];
                stack.truncate(pos);
                if olt <= cur && cur <= gt {
                    return Some(if around {
                        OpTarget::Span(map[olt], map[gt])
                    } else {
                        let istart = ogt + 1;
                        let iend = lt.saturating_sub(1);
                        if istart > iend {
                            OpTarget::Chars(self.cursor.col, self.cursor.col)
                        } else {
                            OpTarget::Span(map[istart], map[iend])
                        }
                    });
                }
            }
        }
        None
    }

    /// The `ii` / `ai` indentation text object. Anchoring on the cursor's line
    /// (or the nearest non-blank line if it is blank), expand to the contiguous
    /// run of lines indented at least as far, keeping blank lines that sit
    /// strictly inside the block. `around` (`ai`) also includes the header line
    /// above — the less-indented line that opens the block.
    fn indent_object(&self, around: bool) -> Option<OpTarget> {
        let n = self.buffer.line_count();
        if n == 0 {
            return None;
        }
        // Leading-whitespace width of a line; `None` for a blank (whitespace-only)
        // line, which does not constrain the block's indent.
        let indent = |row: usize| -> Option<usize> {
            let line = self.buffer.line(row)?;
            if line.trim().is_empty() {
                None
            } else {
                Some(line.chars().take_while(|c| c.is_whitespace()).count())
            }
        };
        // Anchor on a non-blank line.
        let mut r = self.cursor.row.min(n - 1);
        if indent(r).is_none() {
            let below = (r + 1..n).find(|&row| indent(row).is_some());
            r = below
                .or_else(|| (0..r).rev().find(|&row| indent(row).is_some()))?;
        }
        let base = indent(r)?;
        // Expand upward, stepping over interior blank runs that are followed by a
        // line still in the block.
        let mut a = r;
        while a > 0 {
            let p = a - 1;
            match indent(p) {
                Some(v) if v >= base => a = p,
                Some(_) => break,
                None => {
                    let mut pp = p;
                    while pp > 0 && indent(pp).is_none() {
                        pp -= 1;
                    }
                    match indent(pp) {
                        Some(v) if v >= base => a = pp,
                        _ => break,
                    }
                }
            }
        }
        // Expand downward the same way.
        let mut b = r;
        while b + 1 < n {
            let q = b + 1;
            match indent(q) {
                Some(v) if v >= base => b = q,
                Some(_) => break,
                None => {
                    let mut qq = q;
                    while qq + 1 < n && indent(qq).is_none() {
                        qq += 1;
                    }
                    match indent(qq) {
                        Some(v) if v >= base => b = qq,
                        _ => break,
                    }
                }
            }
        }
        if around && a > 0 {
            a -= 1;
        }
        Some(OpTarget::Lines(a, b))
    }

    /// The position one character after `p`, crossing to the next line's start
    /// when `p` is at end of line. Returns `p` unchanged at end of buffer.
    fn pos_after(&self, p: Position) -> Position {
        let len = self.buffer.line(p.row).map(|l| l.chars().count()).unwrap_or(0);
        if p.col + 1 < len {
            Position::new(p.row, p.col + 1)
        } else if p.row + 1 < self.buffer.line_count() {
            Position::new(p.row + 1, 0)
        } else {
            p
        }
    }

    /// The position one character before `p`, crossing to the previous line's
    /// end when `p` is at column 0. Returns `p` unchanged at the buffer start.
    fn pos_before(&self, p: Position) -> Position {
        if p.col > 0 {
            Position::new(p.row, p.col - 1)
        } else if p.row > 0 {
            let prev = self.buffer.line(p.row - 1).map(|l| l.chars().count()).unwrap_or(0);
            Position::new(p.row - 1, prev.saturating_sub(1))
        } else {
            p
        }
    }

    /// The position of the nearest unmatched `open` bracket at or before the
    /// cursor, scanning backward across lines (for multi-line pair objects).
    fn enclosing_open(&self, open: char, close: char) -> Option<Position> {
        let cur = self.cursor;
        let mut depth = 0i32;
        let mut row = cur.row;
        loop {
            let line: Vec<char> = self.buffer.line(row)?.chars().collect();
            let mut col = if row == cur.row {
                (cur.col as isize).min(line.len() as isize - 1)
            } else {
                line.len() as isize - 1
            };
            while col >= 0 {
                let c = line[col as usize];
                let on_cursor = row == cur.row && col == cur.col as isize;
                if c == close && !on_cursor {
                    depth += 1;
                } else if c == open {
                    if depth == 0 {
                        return Some(Position::new(row, col as usize));
                    }
                    depth -= 1;
                }
                col -= 1;
            }
            if row == 0 {
                return None;
            }
            row -= 1;
        }
    }

    /// Find the enclosing `open`/`close` pair across lines, as `(open_pos,
    /// close_pos)`. Used when the pair does not fit on the current line.
    fn find_pair_pos(&self, open: char, close: char) -> Option<(Position, Position)> {
        let o = self.enclosing_open(open, close)?;
        let c = self.scan_bracket(o.row, o.col, open, close, true)?;
        Some((o, c))
    }

    fn find_pair(chars: &[char], col: usize, open: char, close: char) -> Option<(usize, usize)> {
        // Nearest enclosing open bracket at or before the cursor.
        let mut depth = 0i32;
        let mut o = None;
        let mut i = col as isize;
        while i >= 0 {
            let c = chars[i as usize];
            if c == close && (i as usize) != col {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    o = Some(i as usize);
                    break;
                }
                depth -= 1;
            }
            i -= 1;
        }
        let o = o?;
        // Matching close after it.
        let mut depth = 0i32;
        let mut j = o + 1;
        while j < chars.len() {
            let c = chars[j];
            if c == open {
                depth += 1;
            } else if c == close {
                if depth == 0 {
                    return Some((o, j));
                }
                depth -= 1;
            }
            j += 1;
        }
        None
    }

    fn find_quotes(chars: &[char], col: usize, q: char) -> Option<(usize, usize)> {
        let positions: Vec<usize> = chars
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == q)
            .map(|(i, _)| i)
            .collect();
        for pair in positions.chunks(2) {
            if let [a, b] = *pair {
                if col >= a && col <= b {
                    return Some((a, b));
                }
            }
        }
        None
    }

    /// Find the bracket matching the one at (or next on the line after) the
    /// cursor. Matches `()`, `[]`, `{}` with nesting, scanning across lines.
    /// The char range `[start, end)` of the search match the cursor is currently
    /// on (its own row), for the distinct current-match highlight. `None` when
    /// highlighting is off or the cursor isn't inside a match.
    pub fn current_match(&self) -> Option<(usize, usize)> {
        if !self.hlsearch {
            return None;
        }
        let re = self.search_re.as_ref()?;
        let line = self.buffer.line(self.cursor.row)?;
        for m in re.find_iter(line) {
            if m.start() == m.end() {
                continue;
            }
            let s = line[..m.start()].chars().count();
            let e = line[..m.end()].chars().count();
            if self.cursor.col >= s && self.cursor.col < e {
                return Some((s, e));
            }
        }
        None
    }

    /// For matchparen highlighting: if the cursor sits exactly on a bracket,
    /// return the position of its match (not the next bracket on the line, unlike
    /// `%`). `None` when the cursor isn't on a bracket or the pair is unbalanced.
    pub fn match_highlight(&self) -> Option<Position> {
        const OPEN: [char; 3] = ['(', '[', '{'];
        const CLOSE: [char; 3] = [')', ']', '}'];
        let line: Vec<char> = self.buffer.line(self.cursor.row)?.chars().collect();
        let bch = *line.get(self.cursor.col)?;
        if let Some(idx) = OPEN.iter().position(|&c| c == bch) {
            self.scan_bracket(self.cursor.row, self.cursor.col, bch, CLOSE[idx], true)
        } else if let Some(idx) = CLOSE.iter().position(|&c| c == bch) {
            self.scan_bracket(self.cursor.row, self.cursor.col, bch, OPEN[idx], false)
        } else {
            None
        }
    }

    fn matching_bracket(&self) -> Option<Position> {
        const OPEN: [char; 3] = ['(', '[', '{'];
        const CLOSE: [char; 3] = [')', ']', '}'];
        let line: Vec<char> = self.buffer.line(self.cursor.row)?.chars().collect();

        // Locate the bracket at or after the cursor on the current line.
        let bcol = line
            .iter()
            .enumerate()
            .skip(self.cursor.col)
            .find(|(_, &ch)| OPEN.contains(&ch) || CLOSE.contains(&ch))
            .map(|(i, _)| i)?;
        let bch = line[bcol];

        if let Some(idx) = OPEN.iter().position(|&c| c == bch) {
            self.scan_bracket(self.cursor.row, bcol, bch, CLOSE[idx], true)
        } else if let Some(idx) = CLOSE.iter().position(|&c| c == bch) {
            self.scan_bracket(self.cursor.row, bcol, bch, OPEN[idx], false)
        } else {
            None
        }
    }

    /// Compose a digraph (vim `Ctrl-k` + two chars) into a single character.
    /// Covers the common accented letters, ligatures, and symbols using vim's
    /// digraph names. Like vim, the two characters may be given in either order.
    fn digraph(a: char, b: char) -> Option<char> {
        fn lookup(x: char, y: char) -> Option<char> {
            Some(match (x, y) {
                // Grave accent
                ('a', '`') => 'à', ('e', '`') => 'è', ('i', '`') => 'ì',
                ('o', '`') => 'ò', ('u', '`') => 'ù',
                ('A', '`') => 'À', ('E', '`') => 'È', ('I', '`') => 'Ì',
                ('O', '`') => 'Ò', ('U', '`') => 'Ù',
                // Acute accent
                ('a', '\'') => 'á', ('e', '\'') => 'é', ('i', '\'') => 'í',
                ('o', '\'') => 'ó', ('u', '\'') => 'ú', ('y', '\'') => 'ý',
                ('A', '\'') => 'Á', ('E', '\'') => 'É', ('I', '\'') => 'Í',
                ('O', '\'') => 'Ó', ('U', '\'') => 'Ú', ('Y', '\'') => 'Ý',
                // Circumflex
                ('a', '^') => 'â', ('e', '^') => 'ê', ('i', '^') => 'î',
                ('o', '^') => 'ô', ('u', '^') => 'û',
                ('A', '^') => 'Â', ('E', '^') => 'Ê', ('I', '^') => 'Î',
                ('O', '^') => 'Ô', ('U', '^') => 'Û',
                // Diaeresis / umlaut
                ('a', ':') => 'ä', ('e', ':') => 'ë', ('i', ':') => 'ï',
                ('o', ':') => 'ö', ('u', ':') => 'ü', ('y', ':') => 'ÿ',
                ('A', ':') => 'Ä', ('E', ':') => 'Ë', ('I', ':') => 'Ï',
                ('O', ':') => 'Ö', ('U', ':') => 'Ü',
                // Tilde
                ('a', '~') => 'ã', ('o', '~') => 'õ', ('n', '~') => 'ñ',
                ('A', '~') => 'Ã', ('O', '~') => 'Õ', ('N', '~') => 'Ñ',
                // Cedilla, ring, ligatures, and other letters
                ('c', ',') => 'ç', ('C', ',') => 'Ç',
                ('a', 'a') => 'å', ('A', 'A') => 'Å',
                ('a', 'e') => 'æ', ('A', 'E') => 'Æ', ('s', 's') => 'ß',
                ('o', '/') => 'ø', ('O', '/') => 'Ø',
                // Currency and trademarks
                ('E', 'u') => '€', ('P', 'o') => '£', ('Y', 'e') => '¥',
                ('c', 't') => '¢', ('C', 'o') => '©', ('R', 'g') => '®',
                ('T', 'M') => '™',
                // Math and misc symbols
                ('+', '-') => '±', ('D', 'G') => '°', ('M', 'y') => 'µ',
                ('1', '2') => '½', ('1', '4') => '¼', ('3', '4') => '¾',
                ('*', 'X') => '×', ('-', ':') => '÷',
                ('<', '<') => '«', ('>', '>') => '»',
                ('S', 'E') => '§', ('!', 'I') => '¡', ('?', 'I') => '¿',
                // Arrows
                ('-', '>') => '→', ('<', '-') => '←',
                ('-', '!') => '↑', ('-', 'v') => '↓',
                _ => return None,
            })
        }
        lookup(a, b).or_else(|| lookup(b, a))
    }

    /// `[(` / `[{` / `])` / `]}` — jump to the `count`-th *unmatched* bracket of
    /// the given kind. Searching backward (`forward == false`) finds the
    /// enclosing `open_ch`; searching forward finds the enclosing `close_ch`.
    /// Nesting is tracked so inner balanced pairs are skipped.
    fn unmatched_bracket(&mut self, open_ch: char, close_ch: char, forward: bool, count: usize) {
        let mut depth = 0i32;
        let mut remaining = count.max(1);
        let mut row = self.cursor.row;
        // Start one step away from the cursor so the char under it is skipped.
        let mut col: isize = self.cursor.col as isize + if forward { 1 } else { -1 };
        loop {
            let line: Vec<char> = match self.buffer.line(row) {
                Some(l) => l.chars().collect(),
                None => return,
            };
            while col >= 0 && (col as usize) < line.len() {
                let c = line[col as usize];
                // The "inner" bracket deepens nesting; the "target" bracket pops
                // it, and once balance is zero it is the unmatched one we want.
                let (inner, target) = if forward { (open_ch, close_ch) } else { (close_ch, open_ch) };
                if c == inner {
                    depth += 1;
                } else if c == target {
                    if depth == 0 {
                        remaining -= 1;
                        if remaining == 0 {
                            self.record_jump();
                            self.cursor = Position::new(row, col as usize);
                            self.scroll_into_view();
                            return;
                        }
                    } else {
                        depth -= 1;
                    }
                }
                col += if forward { 1 } else { -1 };
            }
            if forward {
                row += 1;
                if row >= self.buffer.line_count() {
                    return;
                }
                col = 0;
            } else {
                if row == 0 {
                    return;
                }
                row -= 1;
                col = self.buffer.line(row).map(|l| l.chars().count()).unwrap_or(0) as isize - 1;
            }
        }
    }

    /// Scan for the bracket matching `from_ch` (its counterpart is `to_ch`),
    /// forward when `forward`, tracking nesting depth.
    fn scan_bracket(
        &self,
        srow: usize,
        scol: usize,
        from_ch: char,
        to_ch: char,
        forward: bool,
    ) -> Option<Position> {
        let mut depth = 0i32;
        let mut row = srow;
        let mut col = scol as isize;
        loop {
            let line: Vec<char> = self.buffer.line(row)?.chars().collect();
            while col >= 0 && (col as usize) < line.len() {
                let c = line[col as usize];
                if c == from_ch {
                    depth += 1;
                } else if c == to_ch {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position::new(row, col as usize));
                    }
                }
                col += if forward { 1 } else { -1 };
            }
            if forward {
                row += 1;
                if row >= self.buffer.line_count() {
                    return None;
                }
                col = 0;
            } else {
                if row == 0 {
                    return None;
                }
                row -= 1;
                col = self.buffer.line(row)?.chars().count() as isize - 1;
            }
        }
    }

    fn move_word_end(&mut self, count: usize, big: bool) {
        for _ in 0..count.max(1) {
            let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
            let len = chars.len();
            // Cross to the next line if there's nothing left on this one.
            if self.cursor.col + 1 >= len {
                if self.cursor.row + 1 < self.buffer.line_count() {
                    self.cursor.row += 1;
                    self.cursor.col = 0;
                } else {
                    break;
                }
            }
            let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
            let len = chars.len();
            let mut i = self.cursor.col + 1;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            if i < len {
                let class = Self::class_of(chars[i], big);
                while i + 1 < len && Self::class_of(chars[i + 1], big) == class {
                    i += 1;
                }
                self.cursor.col = i;
            }
        }
    }

    /// The column the cursor lands on for `f`/`F`/`t`/`T` to `target`, honoring
    /// `count` (e.g. `3fx` → the 3rd `x`). `None` if there aren't enough.
    fn find_col(&self, cmd: char, target: char, count: usize) -> Option<usize> {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col;
        let n = count.max(1);
        match cmd {
            'f' | 't' => {
                let start = (col + 1).min(chars.len());
                let at = chars[start..]
                    .iter()
                    .enumerate()
                    .filter(|&(_, &ch)| ch == target)
                    .nth(n - 1)
                    .map(|(i, _)| start + i);
                at.map(|i| if cmd == 't' { i.saturating_sub(1) } else { i })
            }
            'F' | 'T' => {
                let end = col.min(chars.len());
                let at = chars[..end]
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|&(_, &ch)| ch == target)
                    .nth(n - 1)
                    .map(|(i, _)| i);
                at.map(|i| if cmd == 'T' { i + 1 } else { i })
            }
            _ => None,
        }
    }

    fn do_find(&mut self, cmd: char, target: char, count: usize) {
        if let Some(i) = self.find_col(cmd, target, count) {
            self.cursor.col = i;
        }
    }

    fn repeat_find(&mut self, reverse: bool, count: usize) {
        let Some((cmd, target)) = self.last_find else {
            self.message = "No previous f/t search".into();
            return;
        };
        let effective = if reverse {
            match cmd {
                'f' => 'F',
                'F' => 'f',
                't' => 'T',
                'T' => 't',
                other => other,
            }
        } else {
            cmd
        };
        self.do_find(effective, target, count);
        self.clamp_cursor(false);
    }

    fn toggle_case(&mut self, count: usize) {
        if self.cur_len() == 0 {
            return;
        }
        self.checkpoint();
        for _ in 0..count.max(1) {
            if self.cursor.col >= self.cur_len() {
                break;
            }
            let line = self.buffer.line(self.cursor.row).unwrap_or("");
            let Some(ch) = line.chars().nth(self.cursor.col) else {
                break;
            };
            let toggled: Option<char> = if ch.is_uppercase() {
                ch.to_lowercase().next()
            } else if ch.is_lowercase() {
                ch.to_uppercase().next()
            } else {
                None
            };
            if let Some(t) = toggled {
                self.buffer.replace_char(self.cursor, t);
            }
            self.move_right(1, false);
        }
    }

    fn change_to_eol(&mut self) {
        self.checkpoint();
        let line = self.buffer.line(self.cursor.row).unwrap_or("").to_string();
        let byte = line
            .char_indices()
            .nth(self.cursor.col)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        self.store_delete(line[byte..].to_string(), false);
        self.buffer.set_line(self.cursor.row, line[..byte].to_string());
        self.mode = Mode::Insert;
    }

    fn substitute_char(&mut self, count: usize) {
        if self.cur_len() == 0 {
            self.enter_insert_here();
            return;
        }
        self.checkpoint();
        let mut removed = String::new();
        for _ in 0..count.max(1) {
            if self.cursor.col >= self.cur_len() {
                break;
            }
            if let Some(c) = self.buffer.delete_char(self.cursor) {
                removed.push(c);
            }
        }
        self.store_delete(removed, false);
        self.mode = Mode::Insert;
    }

    fn substitute_line(&mut self) {
        self.checkpoint();
        let indent = self.leading_indent(self.cursor.row);
        self.buffer.set_line(self.cursor.row, indent.clone());
        self.cursor.col = indent.chars().count();
        self.mode = Mode::Insert;
    }

    /// Leading-whitespace width of `row` in display columns.
    fn indent_width(&self, row: usize) -> usize {
        let line = self.buffer.line(row).unwrap_or("");
        let lead: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        Self::expand_tabs(&lead, self.tabstop.max(1)).chars().count()
    }

    /// Rewrite `row`'s leading whitespace to `cols` display columns (tabs then
    /// spaces when `noexpandtab`), keeping the rest of the line intact.
    fn set_indent_columns(&mut self, row: usize, cols: usize) {
        let line = self.buffer.line(row).unwrap_or("").to_string();
        let rest: String = line.chars().skip_while(|c| *c == ' ' || *c == '\t').collect();
        let indent = if self.expandtab {
            " ".repeat(cols)
        } else {
            let ts = self.tabstop.max(1);
            format!("{}{}", "\t".repeat(cols / ts), " ".repeat(cols % ts))
        };
        self.buffer.set_line(row, format!("{indent}{rest}"));
    }

    fn indent_line(&mut self, row: usize) {
        if self.shiftround {
            let sw = self.shiftwidth.max(1);
            let new = (self.indent_width(row) / sw + 1) * sw;
            self.set_indent_columns(row, new);
            return;
        }
        let line = self.buffer.line(row).unwrap_or("").to_string();
        let prefix = if self.expandtab {
            " ".repeat(self.shiftwidth)
        } else {
            "\t".to_string()
        };
        self.buffer.set_line(row, format!("{prefix}{line}"));
    }

    fn dedent_line(&mut self, row: usize) {
        let sw = self.shiftwidth.max(1);
        if self.shiftround {
            let cur = self.indent_width(row);
            if cur > 0 {
                self.set_indent_columns(row, (cur - 1) / sw * sw);
            }
            return;
        }
        let line = self.buffer.line(row).unwrap_or("");
        let mut removed = 0;
        let new: String = {
            let mut chars = line.chars().peekable();
            // Remove up to `shiftwidth` leading spaces, or a single leading tab.
            while removed < sw {
                match chars.peek() {
                    Some(' ') => {
                        chars.next();
                        removed += 1;
                    }
                    Some('\t') if removed == 0 => {
                        chars.next();
                        removed += sw;
                    }
                    _ => break,
                }
            }
            chars.collect()
        };
        if removed > 0 {
            self.buffer.set_line(row, new);
        }
    }

    /// Indent (`>`) or dedent (`<`) the inclusive row range `a..=b` under one
    /// undo step, landing the cursor on the first non-blank of the first line.
    fn shift_range(&mut self, a: usize, b: usize, indent: bool) {
        let last = self.buffer.line_count().saturating_sub(1);
        let (a, b) = (a.min(last), b.min(last));
        self.checkpoint();
        for r in a..=b {
            if indent {
                self.indent_line(r);
            } else {
                self.dedent_line(r);
            }
        }
        self.cursor.row = a;
        self.move_first_nonblank();
    }

    fn shift_selection(&mut self, indent: bool) {
        if let Some((start, end)) = self.selection() {
            self.checkpoint();
            for row in start.row..=end.row {
                if indent {
                    self.indent_line(row);
                } else {
                    self.dedent_line(row);
                }
            }
            self.cursor.row = start.row;
            self.move_first_nonblank();
        }
        self.mode = Mode::Normal;
    }

    /// `ge`/`gE` — move backward to the end of the previous word on this line.
    fn move_word_end_back(&mut self, count: usize, big: bool) {
        for _ in 0..count.max(1) {
            let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
            let n = chars.len();
            let mut i = self.cursor.col as isize - 1;
            let mut landed = false;
            while i >= 0 {
                let c = chars[i as usize];
                if !c.is_whitespace() {
                    let k = i as usize;
                    let is_end = k + 1 >= n
                        || chars[k + 1].is_whitespace()
                        || Self::class_of(chars[k + 1], big) != Self::class_of(c, big);
                    if is_end {
                        self.cursor.col = k;
                        landed = true;
                        break;
                    }
                }
                i -= 1;
            }
            if !landed {
                // Cross to the end of the previous line if possible.
                if self.cursor.row > 0 {
                    self.cursor.row -= 1;
                    self.cursor.col = self.cur_len().saturating_sub(1);
                } else {
                    self.cursor.col = 0;
                    break;
                }
            }
        }
    }

    fn move_word_backward(&mut self, count: usize, big: bool) {
        for _ in 0..count {
            if self.cursor.col == 0 {
                if self.cursor.row > 0 {
                    self.cursor.row -= 1;
                    self.cursor.col = self.cur_len().saturating_sub(1);
                }
                continue;
            }
            let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
            let mut col = self.cursor.col;
            col = col.saturating_sub(1);
            while col > 0 && chars[col].is_whitespace() {
                col -= 1;
            }
            if col > 0 {
                let class = Self::class_of(chars[col], big);
                while col > 0 && Self::class_of(chars[col - 1], big) == class {
                    col -= 1;
                }
            }
            self.cursor.col = col;
        }
    }

    // ---- inserts / opens -------------------------------------------------

    /// Reset count-insert capture (unless we're mid-replay). Called by every
    /// insert entry so a change without a count never repeats.
    fn begin_insert_session(&mut self) {
        if !self.insert_replaying {
            self.insert_repeat = 1;
            self.insert_keys.clear();
        }
    }

    fn enter_insert_here(&mut self) {
        self.begin_insert_session();
        self.checkpoint();
        self.mode = Mode::Insert;
    }

    fn open_below(&mut self) {
        self.begin_insert_session();
        self.checkpoint();
        let indent = self.leading_indent(self.cursor.row);
        self.buffer.insert_line(self.cursor.row + 1, indent.clone());
        self.cursor.row += 1;
        self.cursor.col = indent.chars().count();
        self.mode = Mode::Insert;
    }

    fn open_above(&mut self) {
        self.begin_insert_session();
        self.checkpoint();
        let indent = self.leading_indent(self.cursor.row);
        self.buffer.insert_line(self.cursor.row, indent.clone());
        self.cursor.col = indent.chars().count();
        self.mode = Mode::Insert;
    }

    fn leading_indent(&self, row: usize) -> String {
        let line = self.buffer.line(row).unwrap_or("");
        line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
    }

    // ---- deletes / yanks / paste ----------------------------------------

    fn delete_char_under(&mut self, count: usize) {
        if self.cur_len() == 0 {
            return;
        }
        self.checkpoint();
        let mut removed = String::new();
        for _ in 0..count {
            if self.cursor.col >= self.cur_len() {
                break;
            }
            if let Some(c) = self.buffer.delete_char(self.cursor) {
                removed.push(c);
            }
        }
        self.store_delete(removed, false);
        self.clamp_cursor(false);
    }

    /// `Ctrl-a` / `Ctrl-x` — add `delta * count` to the decimal number under or
    /// after the cursor on the current line (a leading `-` is kept).
    /// Compute the result of bumping the first number at or after `from_col` on
    /// `row` by `delta * count`. Returns `(new_line, end_col)` without mutating,
    /// or `None` if there's no number. Honors an immediately-preceding `-`.
    fn compute_number_bump(
        &self,
        row: usize,
        delta: isize,
        count: usize,
        from_col: usize,
    ) -> Option<(String, usize)> {
        let chars: Vec<char> = self.buffer.line(row).unwrap_or("").chars().collect();
        let n = chars.len();
        let amount = delta as i64 * count as i64;

        // Tokenize the line into number tokens (hex `0x…`, binary `0b…`, decimal
        // with an optional leading `-`), then act on the first one whose end is
        // past the cursor — i.e. the number under or after the cursor.
        // Token: (span_start, span_end, radix, prefix_len, negative).
        let is_bin = |c: char| c == '0' || c == '1';
        let mut i = 0usize;
        let mut token: Option<(usize, usize, u32, usize, bool)> = None;
        while i < n {
            let c = chars[i];
            if c == '0'
                && i + 2 < n
                && (chars[i + 1] == 'x' || chars[i + 1] == 'X')
                && chars[i + 2].is_ascii_hexdigit()
            {
                let mut e = i + 2;
                while e < n && chars[e].is_ascii_hexdigit() {
                    e += 1;
                }
                if e > from_col {
                    token = Some((i, e, 16, 2, false));
                    break;
                }
                i = e;
            } else if c == '0'
                && i + 2 < n
                && (chars[i + 1] == 'b' || chars[i + 1] == 'B')
                && is_bin(chars[i + 2])
            {
                let mut e = i + 2;
                while e < n && is_bin(chars[e]) {
                    e += 1;
                }
                if e > from_col {
                    token = Some((i, e, 2, 2, false));
                    break;
                }
                i = e;
            } else if c.is_ascii_digit() {
                let mut e = i;
                while e < n && chars[e].is_ascii_digit() {
                    e += 1;
                }
                let negative = i > 0 && chars[i - 1] == '-';
                let start = if negative { i - 1 } else { i };
                if e > from_col {
                    token = Some((start, e, 10, 0, negative));
                    break;
                }
                i = e;
            } else {
                i += 1;
            }
        }

        let (start, end, radix, prefix_len, _neg) = token?;
        let digits_start = start + prefix_len;
        let body: String = chars[digits_start..end].iter().collect();
        let newtoken = match radix {
            16 => {
                let val = i64::from_str_radix(&body, 16).ok()?;
                let newval = (val + amount).max(0);
                let width = end - digits_start;
                let digits = format!("{newval:0width$x}");
                let digits = if body.chars().any(|c| c.is_ascii_uppercase()) {
                    digits.to_uppercase()
                } else {
                    digits
                };
                let prefix: String = chars[start..digits_start].iter().collect();
                format!("{prefix}{digits}")
            }
            2 => {
                let val = i64::from_str_radix(&body, 2).ok()?;
                let newval = (val + amount).max(0);
                let width = end - digits_start;
                let digits = format!("{newval:0width$b}");
                let prefix: String = chars[start..digits_start].iter().collect();
                format!("{prefix}{digits}")
            }
            _ => {
                // Decimal: the span already includes any leading `-`.
                let span: String = chars[start..end].iter().collect();
                let val = span.parse::<i64>().ok()?;
                (val + amount).to_string()
            }
        };
        let before: String = chars[..start].iter().collect();
        let after: String = chars[end..].iter().collect();
        let end_col = start + newtoken.chars().count().saturating_sub(1);
        Some((format!("{before}{newtoken}{after}"), end_col))
    }

    fn modify_number(&mut self, delta: isize, count: usize) {
        match self.compute_number_bump(self.cursor.row, delta, count, self.cursor.col) {
            Some((new, col)) => {
                self.checkpoint();
                self.buffer.set_line(self.cursor.row, new);
                self.cursor.col = col;
                self.clamp_cursor(false);
            }
            None => self.message = "No number under cursor".into(),
        }
    }

    /// Dispatch `Ctrl-a`/`Ctrl-x` (and the `g`-prefixed `g Ctrl-a`/`g Ctrl-x`).
    /// A pending `g` selects the stacked/sequence form in visual mode. The count
    /// typed before `g` multiplies the per-line step.
    fn modify_number_ctrl(&mut self, delta: isize) {
        let staged = self.pending_op == Some('g');
        let op_count = if staged {
            self.pending_op = None;
            self.pending_op_count.take().unwrap_or(1)
        } else {
            1
        };
        let count = self.pending_count.take().unwrap_or(1).saturating_mul(op_count);
        if self.mode.is_visual() {
            self.modify_number_visual(delta, count, staged);
        } else {
            self.modify_number(delta, count);
        }
    }

    /// Visual `Ctrl-a`/`Ctrl-x`: bump the first number on every selected line by
    /// `delta * count`, under one undo step, then return to Normal. When
    /// `stacked` (vim's `g Ctrl-a`), the step grows per changed line — the 1st
    /// line gets `delta*count`, the 2nd `delta*count*2`, … — turning equal
    /// numbers into an incrementing sequence.
    fn modify_number_visual(&mut self, delta: isize, count: usize, stacked: bool) {
        let Some((start, end)) = self.selection() else {
            self.mode = Mode::Normal;
            return;
        };
        let mut edits: Vec<(usize, String)> = Vec::new();
        let mut rank = 0usize;
        for row in start.row..=end.row {
            let step = if stacked { count * (rank + 1) } else { count };
            if let Some((new, _)) = self.compute_number_bump(row, delta, step, 0) {
                edits.push((row, new));
                rank += 1;
            }
        }
        if !edits.is_empty() {
            self.checkpoint();
            for (row, new) in edits {
                self.buffer.set_line(row, new);
            }
        }
        self.cursor = Position::new(start.row, 0);
        self.move_first_nonblank();
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    /// `X` — delete up to `count` characters before the cursor.
    fn delete_char_before(&mut self, count: usize) {
        if self.cursor.col == 0 {
            return;
        }
        let n = count.min(self.cursor.col);
        self.checkpoint();
        let mut removed = String::new();
        for _ in 0..n {
            self.cursor.col -= 1;
            if let Some(c) = self.buffer.delete_char(self.cursor) {
                removed.push(c);
            }
        }
        // Collected in reverse; restore left-to-right order.
        let removed: String = removed.chars().rev().collect();
        self.store_delete(removed, false);
        self.clamp_cursor(false);
    }

    fn delete_to_eol(&mut self) {
        self.checkpoint();
        let line = self.buffer.line(self.cursor.row).unwrap_or("").to_string();
        let byte = line
            .char_indices()
            .nth(self.cursor.col)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        let kept = line[..byte].to_string();
        self.store_delete(line[byte..].to_string(), false);
        self.buffer.set_line(self.cursor.row, kept);
        self.clamp_cursor(false);
    }

    /// Store yanked text: unnamed register + the yank register `"0` (or the
    /// pending named register if one was given, e.g. `"ayy`).
    fn store_yank(&mut self, text: String, linewise: bool) {
        // The black-hole register "_ discards without touching any register.
        if self.pending_register == Some('_') {
            self.pending_register = None;
            return;
        }
        let reg = Register { text, linewise };
        if let Some(name) = self.pending_register.take() {
            self.write_named_register(name, reg.clone());
        } else {
            self.registers.insert('0', reg.clone());
        }
        self.register = reg;
    }

    /// Store deleted/changed text: unnamed register plus either a pending named
    /// register, the numbered ring `"1`–`"9` (line-wise / multi-line deletes), or
    /// the small-delete register `"-` (within-line deletes). Mirrors vim.
    fn store_delete(&mut self, text: String, linewise: bool) {
        // The black-hole register "_ discards without touching any register.
        if self.pending_register == Some('_') {
            self.pending_register = None;
            return;
        }
        let reg = Register { text, linewise };
        if let Some(name) = self.pending_register.take() {
            self.write_named_register(name, reg.clone());
        } else if linewise || reg.text.contains('\n') {
            // Shift "1 -> "2 ... "8 -> "9, then store into "1.
            for d in (1..9).rev() {
                let from = char::from_digit(d, 10).unwrap();
                let to = char::from_digit(d + 1, 10).unwrap();
                if let Some(r) = self.registers.get(&from).cloned() {
                    self.registers.insert(to, r);
                }
            }
            self.registers.insert('1', reg.clone());
        } else {
            self.registers.insert('-', reg.clone());
        }
        self.register = reg;
    }

    /// The register to read for a paste: the pending named one if set, else the
    /// unnamed register. Clears the pending register.
    fn active_register(&mut self) -> Register {
        if let Some(name) = self.pending_register.take() {
            // Route through register_text so special registers (`%`) resolve.
            self.register_text(name)
        } else {
            self.register.clone()
        }
    }

    fn paste(&mut self, after: bool) {
        let reg = self.active_register();
        if reg.text.is_empty() && !reg.linewise {
            return;
        }
        self.checkpoint();
        self.paste_text(&reg, after);
    }

    /// `]p` / `[p` — paste the register's lines below / above the current line,
    /// reindenting so the first pasted line matches the current line's indent and
    /// the rest keep their indent relative to it. Always line-wise.
    fn paste_reindent(&mut self, after: bool) {
        let reg = self.active_register();
        if reg.text.is_empty() {
            return;
        }
        self.checkpoint();
        let cur_indent = self.leading_indent(self.cursor.row);
        let lines: Vec<&str> = reg.text.split('\n').collect();
        // Baseline: the indent (in chars) of the first non-blank pasted line.
        let base = lines
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').count())
            .unwrap_or(0);
        let row = if after {
            self.cursor.row + 1
        } else {
            self.cursor.row
        };
        for (i, line) in lines.iter().enumerate() {
            let new = if line.trim().is_empty() {
                String::new()
            } else {
                let orig = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                let rel = orig.saturating_sub(base);
                format!("{}{}{}", cur_indent, " ".repeat(rel), line.trim_start())
            };
            self.buffer.insert_line(row + i, new);
        }
        let last = lines.len().saturating_sub(1);
        let end_col = self
            .buffer
            .line(row + last)
            .map(|l| l.chars().count().saturating_sub(1))
            .unwrap_or(0);
        self.set_change_marks(Position::new(row, 0), Position::new(row + last, end_col));
        self.cursor.row = row;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// `gp` / `gP` — like `p`/`P`, but leave the cursor just after the pasted
    /// text rather than on its last character / first line.
    fn paste_g(&mut self, after: bool) {
        let reg = self.active_register();
        if reg.text.is_empty() && !reg.linewise {
            return;
        }
        self.checkpoint();
        self.paste_text(&reg, after);
        if reg.linewise {
            let added = reg.text.split('\n').count();
            self.cursor.row =
                (self.cursor.row + added).min(self.buffer.line_count().saturating_sub(1));
            self.move_first_nonblank();
        } else {
            self.cursor.col += 1; // one past the last pasted character
        }
        self.clamp_cursor(true);
        self.scroll_into_view();
    }

    /// Insert register `reg` at/after the cursor. Shared by `p`/`P` and visual
    /// paste; the caller is responsible for the undo checkpoint.
    fn paste_text(&mut self, reg: &Register, after: bool) {
        if reg.linewise {
            let row = if after {
                self.cursor.row + 1
            } else {
                self.cursor.row
            };
            // A linewise register may hold several lines (e.g. `2yy`, `yG`).
            let lines: Vec<&str> = reg.text.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                self.buffer.insert_line(row + i, line.to_string());
            }
            let last = lines.len().saturating_sub(1);
            let end_col = lines[last].chars().count().saturating_sub(1);
            self.set_change_marks(Position::new(row, 0), Position::new(row + last, end_col));
            self.cursor.row = row;
            self.move_first_nonblank();
        } else {
            let mut pos = self.cursor;
            if after && self.cur_len() > 0 {
                pos.col += 1;
            }
            self.buffer.insert_str(pos, &reg.text);
            self.set_change_marks(pos, Self::region_end(pos, &reg.text));
            self.cursor.col = pos.col + reg.text.chars().count().saturating_sub(1);
        }
        self.clamp_cursor(false);
    }

    /// Record the `` `[ `` and `` `] `` marks (start / end of the text just
    /// changed, yanked, or put) for later jumps.
    fn set_change_marks(&mut self, start: Position, end: Position) {
        self.marks.insert('[', start);
        self.marks.insert(']', end);
    }

    /// The position of the last character of `text` when inserted starting at
    /// `start`, accounting for any embedded newlines.
    fn region_end(start: Position, text: &str) -> Position {
        let newlines = text.matches('\n').count();
        if newlines == 0 {
            Position::new(start.row, start.col + text.chars().count().saturating_sub(1))
        } else {
            let last_len = text.rsplit('\n').next().unwrap_or("").chars().count();
            Position::new(start.row + newlines, last_len.saturating_sub(1))
        }
    }

    // ---- visual mode -----------------------------------------------------

    /// Apply a case transform to the current visual selection, then return to
    /// Normal mode.
    /// Visual `J`/`gJ`: join all selected lines into one (`raw` keeps whitespace),
    /// then return to Normal with the cursor on the joined line.
    fn join_selection(&mut self, raw: bool) {
        let Some((start, end)) = self.selection() else {
            self.mode = Mode::Normal;
            return;
        };
        self.checkpoint();
        let joins = (end.row - start.row).max(1);
        for _ in 0..joins {
            let ok = if raw {
                self.buffer.join_line_raw(start.row)
            } else {
                self.buffer.join_line(start.row)
            };
            if !ok {
                break;
            }
        }
        self.cursor.row = start.row.min(self.buffer.line_count().saturating_sub(1));
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    /// Visual `r<c>`: replace every character in the selection with `c`,
    /// respecting charwise / linewise / block shapes, then return to Normal.
    fn replace_selection(&mut self, c: char) {
        let Some((start, end)) = self.selection() else {
            self.mode = Mode::Normal;
            return;
        };
        let block = self.block_rect();
        let linewise = self.mode == Mode::VisualLine;
        self.checkpoint();
        if let Some((rmin, rmax, cmin, cmax)) = block {
            for row in rmin..=rmax {
                let len = self.buffer.line(row).map(|l| l.chars().count()).unwrap_or(0);
                for col in cmin..(cmax + 1).min(len) {
                    self.buffer.replace_char(Position::new(row, col), c);
                }
            }
        } else {
            for row in start.row..=end.row {
                let len = self.buffer.line(row).map(|l| l.chars().count()).unwrap_or(0);
                let (c0, c1) = if linewise {
                    (0, len)
                } else if start.row == end.row {
                    (start.col.min(len), (end.col + 1).min(len))
                } else if row == start.row {
                    (start.col.min(len), len)
                } else if row == end.row {
                    (0, (end.col + 1).min(len))
                } else {
                    (0, len)
                };
                for col in c0..c1 {
                    self.buffer.replace_char(Position::new(row, col), c);
                }
            }
        }
        self.cursor = Position::new(start.row, if linewise { 0 } else { start.col });
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    fn transform_selection(&mut self, op: CaseOp) {
        let Some((start, end)) = self.selection() else {
            return;
        };
        let linewise = self.mode == Mode::VisualLine;
        self.checkpoint();
        for row in start.row..=end.row {
            let chars: Vec<char> = self.buffer.line(row).unwrap_or("").chars().collect();
            let len = chars.len();
            let (c0, c1) = if linewise {
                (0, len)
            } else if start.row == end.row {
                (start.col.min(len), (end.col + 1).min(len))
            } else if row == start.row {
                (start.col.min(len), len)
            } else if row == end.row {
                (0, (end.col + 1).min(len))
            } else {
                (0, len)
            };
            let new: String = chars
                .iter()
                .enumerate()
                .map(|(i, &c)| if i >= c0 && i < c1 { op.apply(c) } else { c })
                .collect();
            self.buffer.set_line(row, new);
        }
        self.cursor = Position::new(start.row, if linewise { 0 } else { start.col });
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    /// Sort every line in the buffer. `reverse` flips the order; `unique`
    /// removes duplicate lines after sorting.
    /// Sort the lines in `range` in place. `reverse`/`unique`/`numeric`/`ignorecase`
    /// are vim's `:sort` flags. `unique` may shrink the range, pulling later lines
    /// up.
    /// `:left` / `:right` / `:center` — align each line in the range. For `Left`
    /// `width` is the indent (default 0); for `Right`/`Center` it is the target
    /// width (default `textwidth`, or 80 when unset).
    pub fn align_lines(&mut self, range: SubRange, kind: AlignKind, width: Option<usize>) {
        let (a, b) = self.resolve_range(range);
        let target = width.unwrap_or(if self.textwidth == 0 { 80 } else { self.textwidth });
        self.checkpoint();
        for row in a..=b {
            let line = self.buffer.line(row).unwrap_or("");
            let trimmed = line.trim();
            if trimmed.is_empty() {
                self.buffer.set_line(row, String::new());
                continue;
            }
            let len = trimmed.chars().count();
            let pad = match kind {
                AlignKind::Left => width.unwrap_or(0),
                AlignKind::Right => target.saturating_sub(len),
                AlignKind::Center => target.saturating_sub(len) / 2,
            };
            self.buffer.set_line(row, format!("{}{}", " ".repeat(pad), trimmed));
        }
        self.cursor.row = a.min(self.buffer.line_count().saturating_sub(1));
        self.move_first_nonblank();
        self.clamp_cursor(false);
    }

    /// Run `cmd` through the platform shell, piping `input` to its stdin and
    /// returning its stdout. `None` on spawn / IO failure.
    fn run_shell_filter(cmd: &str, input: &str) -> Option<String> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let (sh, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
        let mut child = Command::new(sh)
            .arg(flag)
            .arg(cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes()).ok()?;
        }
        let out = child.wait_with_output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Replace rows `a..=b` with `lines` (an empty `lines` deletes the rows,
    /// keeping at least one line in the buffer).
    fn splice_lines(&mut self, a: usize, b: usize, lines: &[String]) {
        let total = self.buffer.line_count();
        let covered_all = a == 0 && b + 1 >= total;
        for row in (a..=b.min(total.saturating_sub(1))).rev() {
            self.buffer.delete_line(row);
        }
        if covered_all {
            // The buffer now holds a single empty guard line at index 0.
            if let Some(first) = lines.first() {
                self.buffer.set_line(0, first.clone());
                for (i, line) in lines.iter().enumerate().skip(1) {
                    self.buffer.insert_line(i, line.clone());
                }
            }
        } else {
            for (i, line) in lines.iter().enumerate() {
                self.buffer.insert_line(a + i, line.clone());
            }
        }
    }

    /// Enter the command line prefilled with `{a+1},{b+1}!` so the user can type a
    /// shell command to filter those lines (the `!{motion}` / `!!` / visual `!`).
    fn start_filter_cmdline(&mut self, a: usize, b: usize) {
        self.mode = Mode::Command;
        self.line_kind = LineKind::Ex;
        self.cmdline = format!("{},{}!", a + 1, b + 1);
        self.hist_idx = None;
    }

    /// `:[range]!cmd` — replace the range's lines with the output of piping them
    /// through the shell command `cmd`. With `range: None` (bare `:!cmd`) the
    /// command is just run and the first line of its output shown.
    pub fn filter_range(&mut self, range: Option<SubRange>, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            self.message = "E471: Argument required".into();
            return;
        }
        let Some(range) = range else {
            // Bare `:!cmd` — run without a range, report a line of output.
            match Self::run_shell_filter(cmd, "") {
                Some(out) => {
                    let line = out.lines().next().unwrap_or("").trim_end();
                    self.message = if line.is_empty() {
                        format!("ran: {cmd}")
                    } else {
                        line.to_string()
                    };
                }
                None => self.message = format!("E485: Can't run: {cmd}"),
            }
            return;
        };
        let (a, b) = self.resolve_range(range);
        // Feed the shell the platform-native line ending — Windows tools such as
        // `sort` mis-handle bare LF input. Output is split tolerantly below.
        let eol = if cfg!(windows) { "\r\n" } else { "\n" };
        let mut input = String::new();
        for row in a..=b {
            input.push_str(self.buffer.line(row).unwrap_or(""));
            input.push_str(eol);
        }
        let Some(output) = Self::run_shell_filter(cmd, &input) else {
            self.message = format!("E485: Can't run: {cmd}");
            return;
        };
        // Split output into lines, dropping the trailing newline and any `\r`.
        let mut lines: Vec<String> = output
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        self.checkpoint();
        let produced = lines.len();
        self.splice_lines(a, b, &lines);
        self.cursor.row = a.min(self.buffer.line_count().saturating_sub(1));
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.scroll_into_view();
        self.message = format!("filtered {produced} line(s) through: {cmd}");
    }

    /// Expand every tab in `line` to spaces, column-aware, for a tab stop of `ts`.
    fn expand_tabs(line: &str, ts: usize) -> String {
        let mut out = String::new();
        let mut col = 0;
        for c in line.chars() {
            if c == '\t' {
                let next = (col / ts + 1) * ts;
                for _ in col..next {
                    out.push(' ');
                }
                col = next;
            } else {
                out.push(c);
                col += 1;
            }
        }
        out
    }

    /// Re-tabulate a line's leading whitespace into tabs (+ remainder spaces) for
    /// a tab stop of `ts`, leaving the rest of the line untouched.
    fn tabify_leading(line: &str, ts: usize) -> String {
        let mut width = 0;
        let mut rest_start = line.len();
        for (i, c) in line.char_indices() {
            match c {
                ' ' => width += 1,
                '\t' => width = (width / ts + 1) * ts,
                _ => {
                    rest_start = i;
                    break;
                }
            }
        }
        let mut out = "\t".repeat(width / ts);
        out.push_str(&" ".repeat(width % ts));
        out.push_str(&line[rest_start..]);
        out
    }

    /// `:retab [N]` — normalise indentation to the current `tabstop` (set to `N`
    /// first when given) and `expandtab`: with `expandtab` every tab becomes
    /// spaces; otherwise each line's leading whitespace is re-tabulated into tabs.
    pub fn retab(&mut self, new_ts: Option<usize>) {
        if let Some(n) = new_ts.filter(|&n| n > 0) {
            self.tabstop = n;
        }
        let ts = self.tabstop.max(1);
        let edits: Vec<(usize, String)> = (0..self.buffer.line_count())
            .filter_map(|row| {
                let line = self.buffer.line(row).unwrap_or("");
                let new = if self.expandtab {
                    Self::expand_tabs(line, ts)
                } else {
                    Self::tabify_leading(line, ts)
                };
                (new != line).then_some((row, new))
            })
            .collect();
        if edits.is_empty() {
            self.message = "retab: no change".into();
            return;
        }
        self.checkpoint();
        let count = edits.len();
        for (row, new) in edits {
            self.buffer.set_line(row, new);
        }
        self.clamp_cursor(false);
        self.message = format!("retab: {count} line(s) changed");
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sort_lines(
        &mut self,
        range: SubRange,
        reverse: bool,
        unique: bool,
        numeric: bool,
        ignorecase: bool,
        pattern: Option<String>,
        use_match: bool,
    ) {
        let (a, b) = self.resolve_range(range);
        if b <= a {
            return;
        }
        self.checkpoint();
        let mut lines: Vec<String> =
            (a..=b).map(|r| self.buffer.line(r).unwrap_or("").to_string()).collect();
        // With a `/pattern/`, derive the sort key from each line: the matched text
        // (`r` flag) or what follows the match; a non-matching line keys as empty.
        let re = pattern.as_deref().and_then(pattern::build);
        let key_of = |line: &str| -> String {
            match &re {
                Some(re) => match re.find(line) {
                    Some(m) => {
                        if use_match {
                            line[m.start()..m.end()].to_string()
                        } else {
                            line[m.end()..].to_string()
                        }
                    }
                    None => String::new(),
                },
                None => line.to_string(),
            }
        };
        if numeric {
            lines.sort_by_key(|l| first_number(&key_of(l)));
        } else if ignorecase {
            lines.sort_by_key(|l| key_of(l).to_lowercase());
        } else {
            lines.sort_by_key(|l| key_of(l));
        }
        if unique {
            if ignorecase && !numeric {
                lines.dedup_by(|x, y| x.to_lowercase() == y.to_lowercase());
            } else {
                lines.dedup();
            }
        }
        if reverse {
            lines.reverse();
        }
        // Insert the sorted lines before the range, then delete the originals, so
        // the buffer is never momentarily empty (no sentinel blank line is left).
        let count = b - a + 1;
        for (k, line) in lines.iter().enumerate() {
            self.buffer.insert_line(a + k, line.clone());
        }
        for _ in 0..count {
            self.buffer.delete_line(a + lines.len());
        }
        self.cursor = Position::new(a, 0);
        self.clamp_cursor(false);
    }

    fn toggle_visual(&mut self, target: Mode) {
        if self.mode == target {
            self.mode = Mode::Normal;
        } else {
            let was_visual = self.mode.is_visual();
            self.mode = target;
            if !was_visual {
                self.visual_anchor = self.cursor;
            }
        }
    }

    /// The block selection rectangle `(rmin, rmax, cmin, cmax)` in VisualBlock.
    pub fn block_rect(&self) -> Option<(usize, usize, usize, usize)> {
        if self.mode != Mode::VisualBlock {
            return None;
        }
        let a = self.visual_anchor;
        let c = self.cursor;
        Some((
            a.row.min(c.row),
            a.row.max(c.row),
            a.col.min(c.col),
            a.col.max(c.col),
        ))
    }

    fn pad_line_to(&mut self, row: usize, col: usize) {
        let len = self.buffer.line_len(row);
        if len < col {
            self.buffer
                .insert_str(Position::new(row, len), &" ".repeat(col - len));
        }
    }

    /// Remove columns `[cmin, cmax]` from each row in `[rmin, rmax]`.
    fn remove_block_columns(&mut self, rmin: usize, rmax: usize, cmin: usize, cmax: usize) {
        for r in rmin..=rmax {
            let chars: Vec<char> = self.buffer.line(r).unwrap_or("").chars().collect();
            let len = chars.len();
            let s = cmin.min(len);
            let e = (cmax + 1).min(len);
            if s < e {
                let kept: String = chars[..s].iter().chain(&chars[e..]).collect();
                self.buffer.set_line(r, kept);
            }
        }
    }

    fn block_delete(&mut self) {
        if let Some((rmin, rmax, cmin, cmax)) = self.block_rect() {
            self.checkpoint();
            self.remove_block_columns(rmin, rmax, cmin, cmax);
            self.cursor = Position::new(rmin, cmin);
        }
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    /// `c` in block mode: delete the rectangle, then block-insert at its left.
    fn block_change(&mut self) {
        let Some((rmin, rmax, cmin, cmax)) = self.block_rect() else {
            return;
        };
        self.checkpoint();
        self.remove_block_columns(rmin, rmax, cmin, cmax);
        self.begin_insert_session();
        self.block_insert = Some((rmin, rmax, cmin, false, false));
        self.cursor = Position::new(rmin, cmin.min(self.buffer.line_len(rmin)));
        self.mode = Mode::Insert;
    }

    /// `I`/`A` in block mode: start inserting; replicated to every row on Esc.
    fn block_insert_start(&mut self, append: bool) {
        let Some((rmin, rmax, cmin, cmax)) = self.block_rect() else {
            return;
        };
        // Ragged-right append (`$A`): start at the top row's own end, and on Esc
        // append at each row's own end rather than a fixed column.
        let to_eol = append && self.block_dollar;
        self.block_dollar = false;
        let col = if to_eol {
            self.buffer.line_len(rmin)
        } else if append {
            cmax + 1
        } else {
            cmin
        };
        self.checkpoint();
        self.begin_insert_session();
        if !to_eol {
            self.pad_line_to(rmin, col);
        }
        self.block_insert = Some((rmin, rmax, col, append, to_eol));
        self.cursor = Position::new(rmin, col);
        self.mode = Mode::Insert;
    }

    /// Apply the just-typed block-insert text to the remaining rows (on Esc).
    fn finish_block_insert(&mut self) {
        let Some((rmin, rmax, col, append, to_eol)) = self.block_insert.take() else {
            return;
        };
        // Only replicate single-line inserts typed on the top row.
        if self.cursor.row != rmin || self.cursor.col < col {
            return;
        }
        let chars: Vec<char> = self.buffer.line(rmin).unwrap_or("").chars().collect();
        let text: String = chars[col..self.cursor.col.min(chars.len())].iter().collect();
        if text.is_empty() {
            return;
        }
        for r in (rmin + 1)..=rmax {
            let len = self.buffer.line_len(r);
            if to_eol {
                // Append at this row's own end (ragged-right).
                self.buffer.insert_str(Position::new(r, len), &text);
            } else if append {
                self.pad_line_to(r, col);
                self.buffer.insert_str(Position::new(r, col), &text);
            } else if col <= len {
                self.buffer.insert_str(Position::new(r, col), &text);
            }
        }
    }

    fn visual_yank(&mut self) {
        if let Some((start, end)) = self.selection() {
            let linewise = self.mode == Mode::VisualLine;
            let text = self.extract_range(start, end, linewise);
            self.store_yank(text, linewise);
            let end_mark = if linewise {
                let col = self
                    .buffer
                    .line(end.row)
                    .map(|l| l.chars().count().saturating_sub(1))
                    .unwrap_or(0);
                Position::new(end.row, col)
            } else {
                end
            };
            self.set_change_marks(start, end_mark);
        }
        self.mode = Mode::Normal;
    }

    /// Visual-mode `p`/`P`: replace the selection with the active register's
    /// contents. The register to put is captured before the deletion, and the
    /// deleted text goes to the unnamed register (as in vim).
    fn visual_paste(&mut self) {
        let Some((start, end)) = self.selection() else {
            self.mode = Mode::Normal;
            return;
        };
        let linewise = self.mode == Mode::VisualLine;
        let reg = self.active_register();
        self.checkpoint();
        let deleted = self.extract_range(start, end, linewise);
        self.delete_range(start, end, linewise);
        self.cursor = if linewise {
            Position::new(start.row.min(self.buffer.line_count().saturating_sub(1)), 0)
        } else {
            start
        };
        self.paste_text(&reg, false);
        self.store_delete(deleted, linewise);
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    fn visual_delete(&mut self) {
        if let Some((start, end)) = self.selection() {
            let linewise = self.mode == Mode::VisualLine;
            self.checkpoint();
            let text = self.extract_range(start, end, linewise);
            self.store_delete(text, linewise);
            self.delete_range(start, end, linewise);
            self.cursor = if linewise {
                Position::new(start.row.min(self.buffer.line_count().saturating_sub(1)), 0)
            } else {
                start
            };
            self.set_change_marks(self.cursor, self.cursor);
        }
        self.mode = Mode::Normal;
        self.clamp_cursor(false);
    }

    fn extract_range(&self, start: Position, end: Position, linewise: bool) -> String {
        if linewise {
            let mut out = Vec::new();
            for r in start.row..=end.row {
                out.push(self.buffer.line(r).unwrap_or("").to_string());
            }
            return out.join("\n");
        }
        if start.row == end.row {
            let line = self.buffer.line(start.row).unwrap_or("");
            let chars: Vec<char> = line.chars().collect();
            let e = (end.col + 1).min(chars.len());
            let s = start.col.min(e);
            return chars[s..e].iter().collect();
        }
        let mut out = String::new();
        let first: Vec<char> = self.buffer.line(start.row).unwrap_or("").chars().collect();
        out.extend(first.iter().skip(start.col));
        out.push('\n');
        for r in (start.row + 1)..end.row {
            out.push_str(self.buffer.line(r).unwrap_or(""));
            out.push('\n');
        }
        let last: Vec<char> = self.buffer.line(end.row).unwrap_or("").chars().collect();
        let e = (end.col + 1).min(last.len());
        out.extend(last.iter().take(e));
        out
    }

    fn delete_range(&mut self, start: Position, end: Position, linewise: bool) {
        if linewise {
            for _ in start.row..=end.row {
                if self.buffer.line_count() == 1 {
                    self.buffer.set_line(0, "");
                    break;
                }
                self.buffer.delete_line(start.row);
            }
            return;
        }
        if start.row == end.row {
            let chars: Vec<char> = self.buffer.line(start.row).unwrap_or("").chars().collect();
            let e = (end.col + 1).min(chars.len());
            let kept: String = chars[..start.col].iter().chain(&chars[e..]).collect();
            self.buffer.set_line(start.row, kept);
            return;
        }
        // multi-line: keep head of start + tail of end, remove middle
        let head: String = self
            .buffer
            .line(start.row)
            .unwrap_or("")
            .chars()
            .take(start.col)
            .collect();
        let tail: String = self
            .buffer
            .line(end.row)
            .unwrap_or("")
            .chars()
            .skip(end.col + 1)
            .collect();
        for _ in start.row..end.row {
            self.buffer.delete_line(start.row + 1);
        }
        self.buffer.set_line(start.row, format!("{head}{tail}"));
    }

    // ---- search ----------------------------------------------------------

    /// The word under (or next on the line after) the cursor.
    fn word_under_cursor(&self) -> Option<String> {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        if chars.is_empty() {
            return None;
        }
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut col = self.cursor.col.min(chars.len() - 1);
        if !is_word(chars[col]) {
            col = (col..chars.len()).find(|&i| is_word(chars[i]))?;
        }
        let mut start = col;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = col + 1;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        Some(chars[start..end].iter().collect())
    }

    /// `gd` / `gD` — jump to the "declaration" of the identifier under the
    /// cursor. `gD` goes to its first whole-word occurrence in the whole file;
    /// `gd` goes to the nearest earlier occurrence (a local-declaration
    /// heuristic), falling back to the first in the file.
    /// The file-name token under the cursor (and its end char index on the row),
    /// for `gf`/`gF`. Scans outward over characters valid in a path (letters,
    /// digits, and `/\._-+#$%~=`). `:` is excluded so a trailing `:line` suffix or
    /// a prose colon isn't swept into the name.
    fn file_under_cursor(&self) -> Option<(String, usize)> {
        let chars: Vec<char> = self.buffer.line(self.cursor.row)?.chars().collect();
        if chars.is_empty() {
            return None;
        }
        let is_fname = |c: char| c.is_alphanumeric() || "/\\._-+#$%~=".contains(c);
        let col = self.cursor.col.min(chars.len() - 1);
        if !is_fname(chars[col]) {
            return None;
        }
        let mut s = col;
        while s > 0 && is_fname(chars[s - 1]) {
            s -= 1;
        }
        let mut e = col + 1;
        while e < chars.len() && is_fname(chars[e]) {
            e += 1;
        }
        let name: String = chars[s..e].iter().collect();
        (!name.is_empty()).then_some((name, e))
    }

    /// `gf` / `gF` — open the file whose name is under the cursor. Tries the name
    /// as given (cwd-relative or absolute), then relative to the current file's
    /// directory; opens the first that exists, else reports it is not found. When
    /// `with_line`, a trailing `:N` after the name opens the file at line N (`gF`).
    fn goto_file(&mut self, with_line: bool) -> Action {
        let Some((name, end)) = self.file_under_cursor() else {
            self.message = "E446: No file name under the cursor".into();
            return Action::None;
        };
        // `gF`: a `:N` suffix immediately after the name gives a target line.
        let line = with_line
            .then(|| {
                let chars: Vec<char> = self.buffer.line(self.cursor.row)?.chars().collect();
                if chars.get(end) != Some(&':') {
                    return None;
                }
                let digits: String = chars[end + 1..].iter().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<usize>().ok()
            })
            .flatten();
        let mut candidates = vec![std::path::PathBuf::from(&name)];
        if let Some(dir) = self.buffer.path().and_then(|p| p.parent()) {
            candidates.push(dir.join(&name));
        }
        match candidates.iter().find(|p| p.exists()) {
            Some(found) => {
                let p = found.to_string_lossy();
                match line {
                    Some(n) => Action::RunEx(format!("edit +{n} {p}")),
                    None => Action::RunEx(format!("edit {p}")),
                }
            }
            None => {
                self.message = format!("E447: Can't find file \"{name}\"");
                Action::None
            }
        }
    }

    fn goto_declaration(&mut self, from_top: bool) {
        let Some(word) = self.word_under_cursor() else {
            self.message = "No word under cursor".into();
            return;
        };
        let Some(re) = pattern::build(&format!(r"\b{}\b", regex::escape(&word))) else {
            return;
        };
        let found = if from_top {
            self.first_match_in_file(&re)
        } else {
            self.nearest_match_above(&re).or_else(|| self.first_match_in_file(&re))
        };
        match found {
            Some(pos) => {
                self.record_jump();
                self.cursor = pos;
                self.clamp_cursor(false);
                self.scroll_into_view();
            }
            None => self.message = format!("Not found: {word}"),
        }
    }

    /// The first match of `re` scanning the buffer top to bottom.
    fn first_match_in_file(&self, re: &Regex) -> Option<Position> {
        for row in 0..self.buffer.line_count() {
            let line = self.buffer.line(row).unwrap_or("");
            if let Some(m) = re.find(line) {
                return Some(Position::new(row, line[..m.start()].chars().count()));
            }
        }
        None
    }

    /// The nearest match of `re` at or above the cursor (scanning upward), before
    /// the cursor column on the cursor's own line.
    fn nearest_match_above(&self, re: &Regex) -> Option<Position> {
        for row in (0..=self.cursor.row).rev() {
            let line = self.buffer.line(row).unwrap_or("");
            let limit = if row == self.cursor.row {
                byte_at_col(line, self.cursor.col)
            } else {
                line.len()
            };
            let mut best = None;
            for m in re.find_iter(line) {
                if m.start() < limit {
                    best = Some(m.start());
                } else {
                    break;
                }
            }
            if let Some(b) = best {
                return Some(Position::new(row, line[..b].chars().count()));
            }
        }
        None
    }

    /// `*`/`#` (whole word) and `g*`/`g#` (substring): search for the word under
    /// the cursor.
    fn search_word(&mut self, forward: bool, boundary: bool, count: usize) {
        let Some(word) = self.word_under_cursor() else {
            self.message = "No word under cursor".into();
            return;
        };
        let escaped = regex::escape(&word);
        let pat = if boundary {
            format!(r"\b{escaped}\b")
        } else {
            escaped
        };
        self.search_offset = None;
        self.set_search(pat);
        self.search_forward = forward;
        // A count jumps to the count-th occurrence (`3*`).
        for _ in 0..count.max(1) {
            self.search_repeat(forward);
        }
    }

    fn search(&mut self, forward: bool) {
        if self.last_search.is_empty() {
            return;
        }
        self.search_repeat(forward);
    }

    /// Undo up to `n` changes (count-aware `u`, and `:earlier N`).
    pub fn undo_times(&mut self, n: usize) {
        let mut done = 0;
        for _ in 0..n.max(1) {
            match self.buffer.undo(self.cursor) {
                Some(pos) => {
                    self.cursor = pos;
                    done += 1;
                }
                None => break,
            }
        }
        if done == 0 {
            self.message = "Already at oldest change".into();
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Redo up to `n` changes (count-aware `Ctrl-r`, and `:later N`).
    pub fn redo_times(&mut self, n: usize) {
        let mut done = 0;
        for _ in 0..n.max(1) {
            match self.buffer.redo(self.cursor) {
                Some(pos) => {
                    self.cursor = pos;
                    done += 1;
                }
                None => break,
            }
        }
        if done == 0 {
            self.message = "Already at newest change".into();
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// Visual-mode `*` / `#`: search for the selected text literally (regex
    /// escaped), then return to Normal mode and jump to the next/previous match.
    fn search_selection(&mut self, forward: bool) {
        let Some((start, end)) = self.selection() else {
            self.mode = Mode::Normal;
            return;
        };
        let text = self.extract_range(start, end, false);
        self.mode = Mode::Normal;
        if text.is_empty() {
            return;
        }
        // Search from the selection start so the current occurrence is skipped.
        self.cursor = start;
        self.search_offset = None;
        self.set_search(regex::escape(&text));
        self.search_forward = forward;
        self.search_repeat(forward);
    }

    /// Find the search match the cursor is on, or the next/previous one, as a
    /// `(row, start_col, end_col_exclusive)` span. Matches are single-line.
    /// `gn` uses this; it wraps around the buffer.
    fn next_match_range(&mut self, forward: bool) -> Option<(usize, usize, usize)> {
        if self.last_search.is_empty() {
            return None;
        }
        if self.search_re.is_none() {
            let ic = self.effective_ignorecase(&self.last_search);
            self.search_re = pattern::build_opts(&self.last_search, ic);
        }
        let re = self.search_re.clone()?;
        let n = self.buffer.line_count();
        if n == 0 {
            return None;
        }
        let cur = self.cursor;
        if forward {
            for step in 0..=n {
                let row = (cur.row + step) % n;
                let line = self.buffer.line(row).unwrap_or("");
                for m in re.find_iter(line) {
                    if m.start() == m.end() {
                        continue;
                    }
                    let sc = line[..m.start()].chars().count();
                    let ec = line[..m.end()].chars().count();
                    // On the cursor's own row, only a match reaching past the
                    // cursor counts (so one already under the cursor is picked).
                    if step != 0 || ec > cur.col {
                        return Some((row, sc, ec));
                    }
                }
            }
        } else {
            for step in 0..=n {
                let row = (cur.row + n - (step % n)) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let mut best: Option<(usize, usize)> = None;
                for m in re.find_iter(line) {
                    if m.start() == m.end() {
                        continue;
                    }
                    let sc = line[..m.start()].chars().count();
                    let ec = line[..m.end()].chars().count();
                    let take = if step == 0 {
                        sc <= cur.col
                    } else if step == n {
                        sc > cur.col
                    } else {
                        true
                    };
                    if take {
                        best = Some((sc, ec)); // keep the rightmost match on the row
                    }
                }
                if let Some((sc, ec)) = best {
                    return Some((row, sc, ec));
                }
            }
        }
        None
    }

    /// `gn` / `gN` — visually select the match under or after/before the cursor.
    fn select_next_match(&mut self, forward: bool) {
        match self.next_match_range(forward) {
            Some((row, sc, ec)) => {
                self.record_jump();
                self.hlsearch = true;
                self.mode = Mode::Visual;
                self.visual_anchor = Position::new(row, sc);
                self.cursor = Position::new(row, ec.saturating_sub(1));
                self.clamp_cursor(true);
                self.scroll_into_view();
            }
            None => {
                self.message = if self.last_search.is_empty() {
                    "No previous search".into()
                } else {
                    format!("Pattern not found: {}", self.last_search)
                };
            }
        }
    }

    fn search_repeat(&mut self, forward: bool) {
        if self.last_search.is_empty() {
            self.message = "No previous search".into();
            return;
        }
        // Ensure a compiled regex exists (e.g. after `n` with no prior compile).
        if self.search_re.is_none() {
            let ic = self.effective_ignorecase(&self.last_search);
            self.search_re = pattern::build_opts(&self.last_search, ic);
        }
        let Some(re) = self.search_re.clone() else {
            return;
        };
        self.hlsearch = true;
        self.record_jump();
        let needle = self.last_search.clone();
        let origin = self.cursor;
        match self.find_match(&re, forward, origin) {
            Some(pos) => {
                // With `nowrapscan`, reject a match found only by wrapping past the
                // end/start of the buffer.
                if !self.wrapscan {
                    let o = (origin.row, origin.col);
                    let p = (pos.row, pos.col);
                    let wrapped = if forward { p <= o } else { p >= o };
                    if wrapped {
                        let edge = if forward { "BOTTOM" } else { "TOP" };
                        self.message = format!("search hit {edge} without match: {needle}");
                        return;
                    }
                }
                self.cursor = pos;
                self.apply_search_offset(&re);
                let sigil = if forward { '/' } else { '?' };
                let count = self.search_count(&re);
                self.message = format!("{sigil}{needle}{count}");
            }
            None => self.message = format!("Pattern not found: {needle}"),
        }
    }

    /// Apply the active search offset (`/pat/e`, `/pat/+N`, …) after the cursor
    /// has landed on a match's start.
    fn apply_search_offset(&mut self, re: &Regex) {
        let Some(off) = self.search_offset else {
            return;
        };
        match off {
            SearchOffset::Line(n) => {
                let last = self.buffer.line_count().saturating_sub(1) as isize;
                self.cursor.row = (self.cursor.row as isize + n).clamp(0, last) as usize;
                self.move_first_nonblank();
            }
            SearchOffset::Start(n) => {
                self.cursor.col = (self.cursor.col as isize + n).max(0) as usize;
                self.clamp_cursor(false);
            }
            SearchOffset::End(n) => {
                let line = self.buffer.line(self.cursor.row).unwrap_or("").to_string();
                let b = byte_at_col(&line, self.cursor.col);
                if let Some(m) = re.find(&line[b..]) {
                    let mlen = line[b + m.start()..b + m.end()].chars().count();
                    let end = self.cursor.col + mlen.saturating_sub(1);
                    self.cursor.col = (end as isize + n).max(0) as usize;
                    self.clamp_cursor(false);
                }
            }
        }
    }

    /// A ` [idx/total]` indicator of the cursor's position among all matches of
    /// `re` in the buffer (vim's searchcount). Empty if there are none. Counting
    /// stops at `MAX` matches, reported as e.g. ` [>1000]`, to bound the cost on
    /// huge files.
    fn search_count(&self, re: &Regex) -> String {
        const MAX: usize = 1000;
        let mut total = 0usize;
        let mut idx = 0usize;
        let mut capped = false;
        'outer: for row in 0..self.buffer.line_count() {
            let line = self.buffer.line(row).unwrap_or("");
            for m in re.find_iter(line) {
                if m.start() == m.end() {
                    continue;
                }
                total += 1;
                let col = line[..m.start()].chars().count();
                if row == self.cursor.row && col == self.cursor.col {
                    idx = total;
                }
                if total >= MAX {
                    capped = true;
                    break 'outer;
                }
            }
        }
        if total == 0 {
            String::new()
        } else if capped {
            format!(" [>{MAX}]")
        } else {
            format!(" [{idx}/{total}]")
        }
    }

    /// Undo an incsearch preview (on Esc or an emptied search line): restore the
    /// cursor to where the search began and the highlight state to what it was.
    fn cancel_search_preview(&mut self) {
        if self.line_kind == LineKind::Ex {
            return;
        }
        self.cursor = self.search_origin;
        self.search_re = self.saved_search_re.take();
        self.last_search = std::mem::take(&mut self.saved_last_search);
        self.hlsearch = self.saved_hlsearch;
        self.scroll_into_view();
    }

    /// Preview the first match of the in-progress `/`/`?` pattern (incsearch).
    /// Moves the cursor to the match (or back to the search origin if the pattern
    /// is empty or has no match) and highlights it, without committing anything.
    fn update_incsearch(&mut self) {
        if !self.incsearch {
            return;
        }
        let forward = match self.line_kind {
            LineKind::SearchFwd => true,
            LineKind::SearchBack => false,
            LineKind::Ex => return,
        };
        if self.cmdline.is_empty() {
            // Nothing typed: show no preview, restore the pre-search view.
            self.cursor = self.search_origin;
            self.search_re = None;
            self.scroll_into_view();
            return;
        }
        // Preview only the pattern part; any trailing `/offset` is not a pattern.
        let (pat, _) = split_search_offset(&self.cmdline);
        if pat.is_empty() {
            return;
        }
        let ic = self.effective_ignorecase(&pat);
        if let Some(re) = pattern::build_opts(&pat, ic) {
            self.cursor = self
                .find_match(&re, forward, self.search_origin)
                .unwrap_or(self.search_origin);
            self.search_re = Some(re);
            self.hlsearch = true;
            self.scroll_into_view();
        }
    }

    /// Find the next (`forward`) or previous match of `re`, scanning from just
    /// after/before `origin` and wrapping around the buffer. Pure: it does not
    /// move the cursor or touch any search state, so both `n`/`N` and the
    /// incsearch preview can share it.
    fn find_match(&self, re: &Regex, forward: bool, origin: Position) -> Option<Position> {
        let n = self.buffer.line_count();
        if n == 0 {
            return None;
        }
        if forward {
            for step in 0..=n {
                let row = (origin.row + step) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let from = if step == 0 {
                    byte_after_col(line, origin.col)
                } else {
                    0
                };
                let from = from.min(line.len());
                if let Some(m) = re.find(&line[from..]) {
                    let byte = from + m.start();
                    return Some(Position::new(row, line[..byte].chars().count()));
                }
            }
        } else {
            for step in 0..=n {
                let row = (origin.row + n - (step % n)) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let limit = if step == 0 {
                    byte_at_col(line, origin.col)
                } else {
                    line.len()
                };
                let limit = limit.min(line.len());
                let mut best = None;
                for m in re.find_iter(line) {
                    if m.start() < limit {
                        best = Some(m.start());
                    } else {
                        break;
                    }
                }
                if let Some(byte) = best {
                    return Some(Position::new(row, line[..byte].chars().count()));
                }
            }
        }
        None
    }

    // ---- misc ------------------------------------------------------------

    fn checkpoint(&mut self) {
        self.buffer.checkpoint(self.cursor);
        // The `` `. `` mark tracks the position of the last change.
        self.marks.insert('.', self.cursor);
        self.record_change(self.cursor);
        // `U` baseline: capture a line's content before the first change in a
        // streak on it. The snapshot above hasn't mutated the buffer yet, so the
        // line still holds its pre-change text here.
        let row = self.cursor.row;
        if self.line_undo.as_ref().map(|(r, _)| *r) != Some(row) {
            let text = self.buffer.line(row).unwrap_or("").to_string();
            self.line_undo = Some((row, text));
        }
    }

    /// `U` — restore the most recently changed line to its content before that
    /// change. `U` is itself a change that `u` can undo, and repeating `U`
    /// toggles the line back (matching vim).
    fn undo_line(&mut self) {
        let Some((row, saved)) = self.line_undo.clone() else {
            self.message = "E21: No line changes to undo".into();
            return;
        };
        if row >= self.buffer.line_count() {
            return;
        }
        let current = self.buffer.line(row).unwrap_or("").to_string();
        // Snapshot for `u` without disturbing the `U` baseline we set below.
        self.buffer.checkpoint(self.cursor);
        self.buffer.set_line(row, saved);
        self.line_undo = Some((row, current)); // toggle on the next `U`
        self.cursor.row = row;
        self.cursor.col = 0;
        self.move_first_nonblank();
        self.clamp_cursor(false);
        self.message = "1 line restored".into();
    }

    /// Record a change position in the changelist (for `g;` / `g,`), collapsing a
    /// repeat on the same line and capping the history. Resets the walk index to
    /// the live end.
    fn record_change(&mut self, pos: Position) {
        if self.changelist.last().map(|p| p.row) != Some(pos.row) {
            self.changelist.push(pos);
            if self.changelist.len() > 100 {
                self.changelist.remove(0);
            }
        } else if let Some(last) = self.changelist.last_mut() {
            *last = pos; // keep the newest column on the same line
        }
        self.change_idx = self.changelist.len();
    }

    /// `Ctrl-g` — report the file name, modified flag, line count and cursor
    /// position (vim's file-info line).
    fn show_file_info(&mut self) {
        let name = self
            .buffer
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let lines = self.buffer.line_count();
        let modified = if self.buffer.is_dirty() { " [Modified]" } else { "" };
        let pct = ((self.cursor.row + 1) * 100 / lines.max(1)).min(100);
        self.message = format!(
            "\"{name}\"{modified} {lines} lines --{pct}%--  line {} of {lines}",
            self.cursor.row + 1
        );
    }

    /// Total `(words, chars, bytes)` in the buffer (newlines count as one char /
    /// byte each). Used by `g Ctrl-g`.
    pub fn document_stats(&self) -> (usize, usize, usize) {
        let n = self.buffer.line_count();
        let mut words = 0;
        let mut chars = 0;
        let mut bytes = 0;
        for row in 0..n {
            let line = self.buffer.line(row).unwrap_or("");
            words += line.split_whitespace().count();
            chars += line.chars().count();
            bytes += line.len();
            if row + 1 < n {
                chars += 1; // the line break
                bytes += 1;
            }
        }
        (words, chars, bytes)
    }

    /// `g Ctrl-g` — show cursor position and document word/char/byte counts.
    fn show_cursor_counts(&mut self) {
        let (words, chars, bytes) = self.document_stats();
        let lines = self.buffer.line_count();
        self.message = format!(
            "line {} of {lines}  col {}  —  {words} words, {chars} chars, {bytes} bytes",
            self.cursor.row + 1,
            self.cursor.col + 1,
        );
    }

    /// `ga` — report the character under the cursor as decimal, hex and octal
    /// (vim's `:ascii`).
    fn show_char_info(&mut self) {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        match line.chars().nth(self.cursor.col) {
            Some(c) => {
                let n = c as u32;
                self.message = format!("<{c}> {n}, Hex {n:x}, Octal {n:o}");
            }
            None => self.message = "NUL".into(),
        }
    }

    /// `g;` — jump to an older change position; `g,` (older = false) — to a newer
    /// one. Returns to the live position list end when exhausted.
    fn change_jump(&mut self, older: bool) {
        if self.changelist.is_empty() {
            self.message = "changelist is empty".into();
            return;
        }
        if older {
            if self.change_idx == 0 {
                self.message = "at start of changelist".into();
                return;
            }
            self.change_idx -= 1;
        } else {
            if self.change_idx + 1 >= self.changelist.len() {
                self.message = "at end of changelist".into();
                return;
            }
            self.change_idx += 1;
        }
        if let Some(&pos) = self.changelist.get(self.change_idx) {
            let last = self.buffer.line_count().saturating_sub(1);
            self.cursor.row = pos.row.min(last);
            self.cursor.col = pos.col.min(self.cur_len());
            self.clamp_cursor(false);
            self.scroll_into_view();
        }
    }

    /// Record where insert/replace mode ended (for `gi` and the `` `^ `` mark).
    fn record_insert_end(&mut self) {
        self.last_insert = self.cursor;
        self.marks.insert('^', self.cursor);
    }

    fn clamp_cursor(&mut self, allow_eol: bool) {
        let rows = self.buffer.line_count();
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }
        let len = self.cur_len();
        let max = if allow_eol || self.mode == Mode::Insert || self.mode == Mode::Replace {
            len
        } else {
            len.saturating_sub(1)
        };
        if self.cursor.col > max {
            self.cursor.col = max;
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// The span an operator (`d`/`y`/`c`) acts on.
#[derive(Debug, Clone, Copy)]
enum OpTarget {
    /// Character columns `[start, end)` on the current row.
    Chars(usize, usize),
    /// Inclusive line range.
    Lines(usize, usize),
    /// An inclusive charwise span across lines (`start`..=`end`), for multi-line
    /// text objects such as `ci{` spanning several rows.
    Span(Position, Position),
}

/// A case transformation applied to characters in a visual selection.
#[derive(Debug, Clone, Copy)]
enum CaseOp {
    Lower,
    Upper,
    Toggle,
    Rot13,
}

/// Insert-mode `Ctrl-v` literal / numeric entry state.
#[derive(Debug, Clone, Copy)]
enum InsertLiteral {
    /// `Ctrl-v` was pressed; the next key picks a radix (`u`/`x`/`o`), starts a
    /// decimal run (a digit), or is inserted literally.
    Start,
    /// Collecting up to `max` digits in `radix` into `acc` (`count` so far).
    Digits {
        radix: u32,
        max: usize,
        acc: u32,
        count: usize,
    },
}

/// A search offset (`/pat/e`, `/pat/s-1`, `/pat/+2`): where the cursor lands
/// relative to the match.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SearchOffset {
    /// `e[±N]` — the match's last character, shifted by N.
    End(isize),
    /// `s[±N]` / `b[±N]` — the match's first character, shifted by N.
    Start(isize),
    /// `[±]N` — N lines below/above the match, at the first non-blank.
    Line(isize),
}

/// Split a typed search into `(pattern, offset)` at the last unescaped `/`,
/// when the suffix is a valid offset spec. Otherwise the whole string is the
/// pattern (so ordinary patterns containing `/` still work when the tail is not
/// an offset).
fn split_search_offset(text: &str) -> (String, Option<SearchOffset>) {
    // Find the last `/` not preceded by a backslash.
    let bytes = text.as_bytes();
    let mut idx = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'/' && (i == 0 || bytes[i - 1] != b'\\') {
            idx = Some(i);
        }
    }
    let Some(i) = idx else {
        return (text.to_string(), None);
    };
    let spec = &text[i + 1..];
    match parse_search_offset(spec) {
        Some(off) => (text[..i].to_string(), Some(off)),
        None => (text.to_string(), None),
    }
}

fn parse_search_offset(spec: &str) -> Option<SearchOffset> {
    if spec.is_empty() {
        return None;
    }
    let (kind, rest) = match spec.chars().next().unwrap() {
        'e' => (Some(true), &spec[1..]),  // end
        's' | 'b' => (Some(false), &spec[1..]), // start
        _ => (None, spec),
    };
    let n = if rest.is_empty() {
        0
    } else {
        // A bare sign means ±1; otherwise parse the signed number.
        match rest {
            "+" => 1,
            "-" => -1,
            _ => rest.parse::<isize>().ok()?,
        }
    };
    match kind {
        Some(true) => Some(SearchOffset::End(n)),
        Some(false) => Some(SearchOffset::Start(n)),
        None => {
            // A line offset requires an explicit number or sign.
            if rest.is_empty() {
                None
            } else {
                Some(SearchOffset::Line(n))
            }
        }
    }
}

/// An in-progress command-line Tab-completion cycle.
struct CmdComp {
    /// The command-line text preceding the completed word (kept verbatim).
    base: String,
    /// Candidate completions that match the stem, in display order.
    matches: Vec<String>,
    /// Index of the currently shown candidate within `matches`.
    idx: usize,
}

impl CaseOp {
    /// The trigger key for this operator (used to detect the doubled form).
    fn key(self) -> char {
        match self {
            CaseOp::Lower => 'u',
            CaseOp::Upper => 'U',
            CaseOp::Toggle => '~',
            CaseOp::Rot13 => '?',
        }
    }

    fn apply(self, c: char) -> char {
        match self {
            CaseOp::Lower => c.to_lowercase().next().unwrap_or(c),
            CaseOp::Upper => c.to_uppercase().next().unwrap_or(c),
            CaseOp::Toggle => {
                if c.is_uppercase() {
                    c.to_lowercase().next().unwrap_or(c)
                } else if c.is_lowercase() {
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c
                }
            }
            CaseOp::Rot13 => {
                if c.is_ascii_lowercase() {
                    (((c as u8 - b'a' + 13) % 26) + b'a') as char
                } else if c.is_ascii_uppercase() {
                    (((c as u8 - b'A' + 13) % 26) + b'A') as char
                } else {
                    c
                }
            }
        }
    }
}

/// The first integer appearing in `line` (with an optional leading `-`), for
/// `:sort n`. Lines with no number sort as 0.
fn first_number(line: &str) -> i64 {
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = if i > 0 && bytes[i - 1] == b'-' { i - 1 } else { i };
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            return line[start..j].parse::<i64>().unwrap_or(0);
        }
    }
    0
}

/// Order two positions into `(earlier, later)`.
fn order(a: Position, b: Position) -> (Position, Position) {
    if (a.row, a.col) <= (b.row, b.col) {
        (a, b)
    } else {
        (b, a)
    }
}

/// Byte offset just after the character at column `col` (used to start a forward
/// search one character past the origin). Falls back to the line's end.
fn byte_after_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col + 1)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// Byte offset of the character at column `col` (the exclusive limit for a
/// backward search from the origin). Falls back to the line's end.
fn byte_at_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
