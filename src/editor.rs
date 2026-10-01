//! The editor state machine: cursor, viewport, motions and edit operations.
//!
//! `Editor` owns the buffer and all *self-contained* behavior (Normal / Insert /
//! Visual editing, incremental search). Ex commands (`:...`) touch other
//! subsystems (themes, plugins), so [`handle_key`](Editor::handle_key) returns
//! an [`Action`] the [`crate::app::App`] executes.

use crate::buffer::{Buffer, Position};
use crate::command::{LineAddr, SubRange, SubstituteSpec};
use crate::menu::{MenuOutcome, MenuState};
use crate::mode::Mode;
use crate::pattern;
use std::collections::HashMap;
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
    /// Whether Enter in insert mode copies the previous line's indentation.
    pub autoindent: bool,
    /// Insert spaces instead of a tab character (`:set expandtab`).
    pub expandtab: bool,
    /// Columns inserted/removed by `>>`/`<<` and `=` (`:set shiftwidth`).
    pub shiftwidth: usize,
    /// Visual width of a tab, and spaces inserted by Tab (`:set tabstop`).
    pub tabstop: usize,
    pub view_rows: usize,
    pub view_cols: usize,

    line_kind: LineKind,
    register: Register,
    registers: HashMap<char, Register>,
    pending_register: Option<char>,
    expect_register: bool,
    visual_anchor: Position,
    /// The last visual selection (start, end, mode) for `gv`.
    last_visual: Option<(Position, Position, Mode)>,
    last_search: String,
    search_re: Option<Regex>,
    last_subst: Option<SubstituteSpec>,
    pending_count: Option<usize>,
    pending_op: Option<char>,
    pending_op_count: Option<usize>,
    /// After `d`/`y`/`c` + `i`/`a`: the (operator, i-or-a) awaiting an object char.
    pending_textobj: Option<(char, char)>,
    /// After `d`/`y`/`c` + `g`: the operator awaiting the second `g` (e.g. `dgg`).
    pending_op_gg: Option<char>,
    /// After `gu`/`gU`/`g~`: a case operator awaiting a motion/object.
    pending_case: Option<CaseOp>,
    /// After a case operator + `i`/`a`: awaiting an object char.
    pending_case_obj: Option<(CaseOp, char)>,
    /// After `gc`: a comment-toggle operator awaiting a motion.
    pending_comment: bool,
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
    /// Active block insert (`Ctrl-v` then `I`/`A`): (rmin, rmax, col, append).
    /// Applied to every row on Esc.
    block_insert: Option<(usize, usize, usize, bool)>,
    pending_find: Option<char>,
    last_find: Option<(char, char)>,
    marks: HashMap<char, Position>,
    pending_mark: Option<PendingMark>,
    previous_pos: Position,
    /// Jump history for `Ctrl-o` / `Ctrl-i`; `jump_idx` points at the current
    /// slot (== `jumps.len()` when at the live position).
    jumps: Vec<Position>,
    jump_idx: usize,
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
            autoindent: true,
            expandtab: true,
            shiftwidth: 4,
            tabstop: 4,
            view_rows: 24,
            view_cols: 80,
            line_kind: LineKind::Ex,
            register: Register::default(),
            registers: HashMap::new(),
            pending_register: None,
            expect_register: false,
            visual_anchor: Position::default(),
            last_visual: None,
            last_search: String::new(),
            search_re: None,
            last_subst: None,
            pending_count: None,
            pending_op: None,
            pending_op_count: None,
            pending_textobj: None,
            pending_op_gg: None,
            pending_case: None,
            pending_case_obj: None,
            pending_comment: false,
            pending_replace: false,
            pending_replace_count: 1,
            replace_stack: Vec::new(),
            insert_repeat: 1,
            insert_entry: 'i',
            insert_keys: Vec::new(),
            insert_replaying: false,
            insert_pending_reg: false,
            block_insert: None,
            pending_find: None,
            last_find: None,
            marks: HashMap::new(),
            pending_mark: None,
            previous_pos: Position::default(),
            jumps: Vec::new(),
            jump_idx: 0,
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

    /// Set the search pattern and (re)compile its regex, enabling highlight.
    fn set_search(&mut self, pat: String) {
        self.search_re = pattern::build(&pat);
        self.last_search = pat;
        self.hlsearch = true;
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

    /// The register currently being recorded into, if any (for the status line).
    pub fn recording_register(&self) -> Option<char> {
        self.recording
    }

    fn start_recording(&mut self, reg: char) {
        self.recording = Some(reg);
        self.macros.insert(reg, Vec::new());
        self.message = format!("recording @{reg}");
    }

    /// Replay macro register `reg` (or the last one for `@@`). Ex-commands
    /// inside a macro are not executed during replay.
    fn play_macro(&mut self, reg: char) {
        let target = if reg == '@' { self.last_macro } else { Some(reg) };
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
    pub fn substitute(&mut self, spec: &SubstituteSpec) -> (usize, usize) {
        let Some(re) = pattern::build_opts(&spec.pattern, spec.ignorecase) else {
            return (0, 0);
        };
        // Remember for `&` (repeat last substitution).
        self.last_subst = Some(spec.clone());
        // vim-style replacement (`\1`, `&`) -> regex crate syntax.
        let replacement = pattern::vim_replacement(&spec.replacement);
        let (start, end) = self.resolve_range(spec.range);

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
        }
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

    fn scroll_into_view(&mut self) {
        if self.cursor.row < self.top {
            self.top = self.cursor.row;
        } else if self.cursor.row >= self.top + self.view_rows {
            self.top = self.cursor.row + 1 - self.view_rows;
        }
        if self.cursor.col < self.left {
            self.left = self.cursor.col;
        } else if self.cursor.col >= self.left + self.view_cols {
            self.left = self.cursor.col + 1 - self.view_cols;
        }
    }

    // ---- key handling ----------------------------------------------------

    /// Handle a key event. Returns an [`Action`] for the app.
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // The menu bar is a modal overlay that captures all keys while open.
        if self.menu.is_some() {
            return self.handle_menu_key(key);
        }

        // The key after `q`/`@` selects the macro register (never recorded).
        if let Some(mm) = self.expect_macro.take() {
            if let KeyCode::Char(c) = key.code {
                match mm {
                    MacroMode::Record => self.start_recording(c),
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

        // `.` repeats the last change (only from a resting normal state).
        if !self.dot_replaying
            && self.mode == Mode::Normal
            && matches!(key.code, KeyCode::Char('.'))
            && self.at_rest()
        {
            self.replay_dot();
            return Action::None;
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
            }
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

        action
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
            && !self.pending_replace
            && self.pending_find.is_none()
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
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.cmdline.clear();
                Action::None
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.cmdline);
                self.mode = Mode::Normal;
                match self.line_kind {
                    LineKind::Ex => Action::RunEx(text),
                    LineKind::SearchFwd => {
                        self.set_search(text);
                        self.search(true);
                        Action::None
                    }
                    LineKind::SearchBack => {
                        self.set_search(text);
                        self.search(false);
                        Action::None
                    }
                }
            }
            KeyCode::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
                Action::None
            }
            KeyCode::Char(c) => {
                self.cmdline.push(c);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_insert(&mut self, key: KeyEvent) {
        // Capture typed keys so a counted insert (`3ihi`) can repeat on Esc.
        if !self.insert_replaying && key.code != KeyCode::Esc {
            self.insert_keys.push(key);
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
        // Insert-mode control shortcuts.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => self.insert_delete_word_before(),
                KeyCode::Char('u') => self.insert_delete_to_line_start(),
                KeyCode::Char('r') => self.insert_pending_reg = true,
                KeyCode::Char('t') => self.insert_indent(true),
                KeyCode::Char('d') => self.insert_indent(false),
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
                self.mode = Mode::Normal;
                // vim moves left when leaving insert mode
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
                self.clamp_cursor(false);
            }
            KeyCode::Char(c) => {
                self.buffer.insert_char(self.cursor, c);
                self.cursor.col += 1;
            }
            KeyCode::Enter => {
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
            KeyCode::Backspace => self.backspace(),
            KeyCode::Tab => {
                if self.expandtab {
                    let n = self.tabstop.max(1);
                    self.buffer.insert_str(self.cursor, &" ".repeat(n));
                    self.cursor.col += n;
                } else {
                    self.buffer.insert_char(self.cursor, '\t');
                    self.cursor.col += 1;
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
            other => self.registers.get(&other).cloned().unwrap_or_default(),
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
                self.pending_register = Some(c.to_ascii_lowercase());
            }
            return Action::None;
        }

        // Pending `f`/`F`/`t`/`T` find-char target.
        if let Some(cmd) = self.pending_find.take() {
            if let KeyCode::Char(c) = key.code {
                self.do_find(cmd, c);
                self.last_find = Some((cmd, c));
            }
            self.clamp_cursor(false);
            self.scroll_into_view();
            return Action::None;
        }

        // Motion / doubled after `gc` (comment toggle).
        if self.pending_comment {
            self.pending_comment = false;
            let rows = match key.code {
                KeyCode::Char('c') => Some((self.cursor.row, self.cursor.row)),
                code => self.motion_target(code, 1).map(|t| match t {
                    OpTarget::Chars(_, _) => (self.cursor.row, self.cursor.row),
                    OpTarget::Lines(a, b) => (a, b),
                }),
            };
            if let Some((a, b)) = rows {
                self.toggle_comment_lines(a, b);
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
            if key.code == KeyCode::Char('g') {
                let target = OpTarget::Lines(0, self.cursor.row);
                self.apply_op(op, target);
            }
            return Action::None;
        }

        // Object char after `d`/`y`/`c` + `i`/`a` (e.g. `diw`, `ci(`).
        if let Some((op, iora)) = self.pending_textobj.take() {
            if let KeyCode::Char(obj) = key.code {
                if let Some(t) = self.text_object(iora, obj) {
                    self.apply_op(op, t);
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
                    if let Some(pos) = self.buffer.redo(self.cursor) {
                        self.cursor = pos;
                        self.clamp_cursor(false);
                    } else {
                        self.message = "Already at newest change".into();
                    }
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
                    let c = self.pending_count.take().unwrap_or(1);
                    self.modify_number(1, c);
                    return Action::None;
                }
                KeyCode::Char('x') => {
                    let c = self.pending_count.take().unwrap_or(1);
                    self.modify_number(-1, c);
                    return Action::None;
                }
                KeyCode::Char('v') => {
                    self.toggle_visual(Mode::VisualBlock);
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

        let count = self.pending_count.take().unwrap_or(1);

        // Operator-pending (d, y, c, g, z, >, <). The total count multiplies the
        // count typed before the operator by the count typed before the motion.
        if let Some(op) = self.pending_op.take() {
            let op_count = self.pending_op_count.take().unwrap_or(1);
            // `i`/`a` after d/y/c begins a text object (e.g. diw, ci().
            if matches!(op, 'd' | 'y' | 'c')
                && matches!(code, KeyCode::Char('i') | KeyCode::Char('a'))
            {
                if let KeyCode::Char(iora) = code {
                    self.pending_textobj = Some((op, iora));
                }
                return Action::None;
            }
            // `g` after d/y/c awaits a second `g` (e.g. dgg).
            if matches!(op, 'd' | 'y' | 'c') && code == KeyCode::Char('g') {
                self.pending_op_gg = Some(op);
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
            KeyCode::Char('$') | KeyCode::End => self.move_line_end(),
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
            KeyCode::Char('f') => self.pending_find = Some('f'),
            KeyCode::Char('F') => self.pending_find = Some('F'),
            KeyCode::Char('t') => self.pending_find = Some('t'),
            KeyCode::Char('T') => self.pending_find = Some('T'),
            KeyCode::Char(';') => self.repeat_find(false),
            KeyCode::Char(',') => self.repeat_find(true),
            KeyCode::Char('%') => {
                if let Some(p) = self.matching_bracket() {
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
                self.cursor.row = self.top.min(self.buffer.line_count().saturating_sub(1));
                self.move_first_nonblank();
            }
            KeyCode::Char('M') => {
                let last = self.buffer.line_count().saturating_sub(1);
                self.cursor.row = (self.top + self.view_rows / 2).min(last);
                self.move_first_nonblank();
            }
            KeyCode::Char('L') => {
                let last = self.buffer.line_count().saturating_sub(1);
                self.cursor.row = (self.top + self.view_rows.saturating_sub(1)).min(last);
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
            KeyCode::Char('i') => {
                self.enter_insert_here();
                self.insert_repeat = count;
                self.insert_entry = 'i';
            }
            KeyCode::Char('a') => {
                self.move_right(1, true);
                self.enter_insert_here();
                self.insert_repeat = count;
                self.insert_entry = 'a';
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
                } else if let Some(pos) = self.buffer.undo(self.cursor) {
                    self.cursor = pos;
                    self.clamp_cursor(false);
                } else {
                    self.message = "Already at oldest change".into();
                }
            }
            KeyCode::Char('p') => {
                for _ in 0..count {
                    self.paste(true);
                }
            }
            KeyCode::Char('P') => {
                for _ in 0..count {
                    self.paste(false);
                }
            }
            KeyCode::Char('J') => {
                self.checkpoint();
                self.buffer.join_line(self.cursor.row);
            }
            KeyCode::Char('v') => self.toggle_visual(Mode::Visual),
            KeyCode::Char('V') => self.toggle_visual(Mode::VisualLine),
            KeyCode::Char('n') => self.search_repeat(true),
            KeyCode::Char('N') => self.search_repeat(false),
            KeyCode::Char('&') => self.repeat_substitute(),
            KeyCode::Char('*') => self.search_word(true, true),
            KeyCode::Char('#') => self.search_word(false, true),
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.line_kind = LineKind::Ex;
                self.cmdline.clear();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Command;
                self.line_kind = LineKind::SearchFwd;
                self.cmdline.clear();
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Command;
                self.line_kind = LineKind::SearchBack;
                self.cmdline.clear();
            }
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
                KeyCode::Char('*') => self.search_word(true, false),
                KeyCode::Char('#') => self.search_word(false, false),
                KeyCode::Char('e') => self.move_word_end_back(count, false),
                KeyCode::Char('E') => self.move_word_end_back(count, true),
                KeyCode::Char('J') => {
                    self.checkpoint();
                    self.buffer.join_line_raw(self.cursor.row);
                }
                KeyCode::Char('_') => {
                    // g_ — last non-blank char (count-1 lines down).
                    self.move_down(count.saturating_sub(1));
                    self.cursor.col = self.last_nonblank_col();
                }
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
                _ => {}
            },
            '>' | '<' => {
                let last = self.buffer.line_count().saturating_sub(1);
                let rows = if code == KeyCode::Char(op) {
                    // Doubled (`>>`/`<<`): `count` lines from the cursor.
                    Some((self.cursor.row, (self.cursor.row + count - 1).min(last)))
                } else {
                    match self.motion_target(code, count) {
                        Some(OpTarget::Lines(a, b)) => Some((a, b)),
                        Some(OpTarget::Chars(_, _)) => Some((self.cursor.row, self.cursor.row)),
                        None => None,
                    }
                };
                if let Some((a, b)) = rows {
                    self.checkpoint();
                    for r in a..=b {
                        if op == '>' {
                            self.indent_line(r);
                        } else {
                            self.dedent_line(r);
                        }
                    }
                    self.cursor.row = a;
                    self.move_first_nonblank();
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
                if is_delete {
                    self.checkpoint();
                    let kept: String = chars[..s].iter().chain(&chars[e..]).collect();
                    self.buffer.set_line(self.cursor.row, kept);
                    self.store_delete(text, false);
                    self.cursor.col = s;
                    if is_change {
                        self.mode = Mode::Insert;
                    }
                } else {
                    self.store_yank(text, false);
                    self.cursor.col = s;
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
                } else {
                    self.store_yank(text, true);
                    self.cursor.row = a;
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
        }
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

    /// Compute a text object span on the current line. `iora` is `i` (inner) or
    /// `a` (around); `obj` selects the object (`w`, brackets, quotes).
    fn text_object(&self, iora: char, obj: char) -> Option<OpTarget> {
        let around = iora == 'a';

        // Paragraph object (line-wise), valid even on a blank line.
        if obj == 'p' {
            let blank = self.is_blank_row(self.cursor.row);
            let n = self.buffer.line_count();
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
            return Some(OpTarget::Lines(a, b));
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
            return Some(OpTarget::Chars(start, end));
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
        let (o, c) = if open == close {
            Self::find_quotes(&chars, col, open)?
        } else {
            Self::find_pair(&chars, col, open, close)?
        };
        if around {
            Some(OpTarget::Chars(o, c + 1))
        } else {
            Some(OpTarget::Chars(o + 1, c))
        }
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

    fn do_find(&mut self, cmd: char, target: char) {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let col = self.cursor.col;
        match cmd {
            'f' => {
                if let Some(i) = (col + 1..chars.len()).find(|&i| chars[i] == target) {
                    self.cursor.col = i;
                }
            }
            'F' => {
                if let Some(i) = (0..col).rev().find(|&i| chars[i] == target) {
                    self.cursor.col = i;
                }
            }
            't' => {
                if let Some(i) = (col + 1..chars.len()).find(|&i| chars[i] == target) {
                    self.cursor.col = i.saturating_sub(1);
                }
            }
            'T' => {
                if let Some(i) = (0..col).rev().find(|&i| chars[i] == target) {
                    self.cursor.col = i + 1;
                }
            }
            _ => {}
        }
    }

    fn repeat_find(&mut self, reverse: bool) {
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
        self.do_find(effective, target);
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

    fn indent_line(&mut self, row: usize) {
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
    fn modify_number(&mut self, delta: isize, count: usize) {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let n = chars.len();
        let mut i = self.cursor.col;
        // If not on a digit, scan forward to the next one on this line.
        if i >= n || !chars[i].is_ascii_digit() {
            while i < n && !chars[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i >= n {
            self.message = "No number under cursor".into();
            return;
        }
        // Expand to the full digit run.
        let mut start = i;
        while start > 0 && chars[start - 1].is_ascii_digit() {
            start -= 1;
        }
        let mut end = i;
        while end < n && chars[end].is_ascii_digit() {
            end += 1;
        }
        // Include an immediately-preceding minus sign.
        let span_start = if start > 0 && chars[start - 1] == '-' {
            start - 1
        } else {
            start
        };
        let numstr: String = chars[span_start..end].iter().collect();
        let Ok(val) = numstr.parse::<i64>() else {
            return;
        };
        let newval = val + delta as i64 * count as i64;
        let newstr = newval.to_string();
        let before: String = chars[..span_start].iter().collect();
        let after: String = chars[end..].iter().collect();
        self.checkpoint();
        self.buffer
            .set_line(self.cursor.row, format!("{before}{newstr}{after}"));
        self.cursor.col = span_start + newstr.chars().count().saturating_sub(1);
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
        let reg = Register { text, linewise };
        if let Some(name) = self.pending_register.take() {
            self.registers.insert(name, reg.clone());
        } else {
            self.registers.insert('0', reg.clone());
        }
        self.register = reg;
    }

    /// Store deleted/changed text: unnamed register plus either a pending named
    /// register, the numbered ring `"1`–`"9` (line-wise / multi-line deletes), or
    /// the small-delete register `"-` (within-line deletes). Mirrors vim.
    fn store_delete(&mut self, text: String, linewise: bool) {
        let reg = Register { text, linewise };
        if let Some(name) = self.pending_register.take() {
            self.registers.insert(name, reg.clone());
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
            self.registers.get(&name).cloned().unwrap_or_default()
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
        if reg.linewise {
            let row = if after {
                self.cursor.row + 1
            } else {
                self.cursor.row
            };
            // A linewise register may hold several lines (e.g. `2yy`, `yG`).
            for (i, line) in reg.text.split('\n').enumerate() {
                self.buffer.insert_line(row + i, line.to_string());
            }
            self.cursor.row = row;
            self.move_first_nonblank();
        } else {
            let mut pos = self.cursor;
            if after && self.cur_len() > 0 {
                pos.col += 1;
            }
            self.buffer.insert_str(pos, &reg.text);
            self.cursor.col = pos.col + reg.text.chars().count().saturating_sub(1);
        }
        self.clamp_cursor(false);
    }

    // ---- visual mode -----------------------------------------------------

    /// Apply a case transform to the current visual selection, then return to
    /// Normal mode.
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
    pub fn sort_buffer(&mut self, reverse: bool, unique: bool) {
        if self.buffer.line_count() <= 1 {
            return;
        }
        self.checkpoint();
        let mut lines: Vec<String> = self.buffer.lines().to_vec();
        lines.sort();
        if unique {
            lines.dedup();
        }
        if reverse {
            lines.reverse();
        }
        for (row, line) in lines.iter().enumerate() {
            if row < self.buffer.line_count() {
                self.buffer.set_line(row, line.clone());
            } else {
                self.buffer.insert_line(row, line.clone());
            }
        }
        // Remove any surplus lines if `unique` shrank the buffer.
        while self.buffer.line_count() > lines.len() {
            self.buffer.delete_line(self.buffer.line_count() - 1);
        }
        self.cursor = Position::default();
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
        self.block_insert = Some((rmin, rmax, cmin, false));
        self.cursor = Position::new(rmin, cmin.min(self.buffer.line_len(rmin)));
        self.mode = Mode::Insert;
    }

    /// `I`/`A` in block mode: start inserting; replicated to every row on Esc.
    fn block_insert_start(&mut self, append: bool) {
        let Some((rmin, rmax, cmin, cmax)) = self.block_rect() else {
            return;
        };
        let col = if append { cmax + 1 } else { cmin };
        self.checkpoint();
        self.begin_insert_session();
        self.pad_line_to(rmin, col);
        self.block_insert = Some((rmin, rmax, col, append));
        self.cursor = Position::new(rmin, col);
        self.mode = Mode::Insert;
    }

    /// Apply the just-typed block-insert text to the remaining rows (on Esc).
    fn finish_block_insert(&mut self) {
        let Some((rmin, rmax, col, append)) = self.block_insert.take() else {
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
            if append {
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
        }
        self.mode = Mode::Normal;
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

    /// `*`/`#` (whole word) and `g*`/`g#` (substring): search for the word under
    /// the cursor.
    fn search_word(&mut self, forward: bool, boundary: bool) {
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
        self.set_search(pat);
        self.search_repeat(forward);
    }

    fn search(&mut self, forward: bool) {
        if self.last_search.is_empty() {
            return;
        }
        self.search_repeat(forward);
    }

    fn search_repeat(&mut self, forward: bool) {
        if self.last_search.is_empty() {
            self.message = "No previous search".into();
            return;
        }
        // Ensure a compiled regex exists (e.g. after `n` with no prior compile).
        if self.search_re.is_none() {
            self.search_re = pattern::build(&self.last_search);
        }
        let Some(re) = self.search_re.clone() else {
            return;
        };
        self.hlsearch = true;
        self.record_jump();
        let needle = self.last_search.clone();
        let n = self.buffer.line_count();
        if forward {
            for step in 0..=n {
                let row = (self.cursor.row + step) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let from = if step == 0 { self.byte_after_cursor() } else { 0 };
                let from = from.min(line.len());
                if let Some(m) = re.find(&line[from..]) {
                    let byte = from + m.start();
                    self.cursor.row = row;
                    self.cursor.col = line[..byte].chars().count();
                    self.message = format!("/{needle}");
                    return;
                }
            }
        } else {
            for step in 0..=n {
                let row = (self.cursor.row + n - (step % n)) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let limit = if step == 0 {
                    self.byte_before_cursor()
                } else {
                    line.len()
                };
                let limit = limit.min(line.len());
                // Last match starting before `limit`.
                let mut best = None;
                for m in re.find_iter(line) {
                    if m.start() < limit {
                        best = Some(m.start());
                    } else {
                        break;
                    }
                }
                if let Some(byte) = best {
                    self.cursor.row = row;
                    self.cursor.col = line[..byte].chars().count();
                    self.message = format!("?{needle}");
                    return;
                }
            }
        }
        self.message = format!("Pattern not found: {needle}");
    }

    fn byte_after_cursor(&self) -> usize {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        line.char_indices()
            .nth(self.cursor.col + 1)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    }

    fn byte_before_cursor(&self) -> usize {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        line.char_indices()
            .nth(self.cursor.col)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    }

    // ---- misc ------------------------------------------------------------

    fn checkpoint(&mut self) {
        self.buffer.checkpoint(self.cursor);
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
}

/// A case transformation applied to characters in a visual selection.
#[derive(Debug, Clone, Copy)]
enum CaseOp {
    Lower,
    Upper,
    Toggle,
}

impl CaseOp {
    /// The trigger key for this operator (used to detect the doubled form).
    fn key(self) -> char {
        match self {
            CaseOp::Lower => 'u',
            CaseOp::Upper => 'U',
            CaseOp::Toggle => '~',
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
        }
    }
}

/// Order two positions into `(earlier, later)`.
fn order(a: Position, b: Position) -> (Position, Position) {
    if (a.row, a.col) <= (b.row, b.col) {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn special(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ed_with(text: &str) -> Editor {
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text(text);
        ed
    }

    #[test]
    fn basic_motion_hjkl() {
        let mut ed = ed_with("abc\ndef\nghi");
        ed.handle_key(key('l'));
        ed.handle_key(key('j'));
        assert_eq!(ed.cursor, Position::new(1, 1));
        ed.handle_key(key('h'));
        ed.handle_key(key('k'));
        assert_eq!(ed.cursor, Position::new(0, 0));
    }

    #[test]
    fn insert_text_and_escape() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        assert_eq!(ed.mode, Mode::Insert);
        for c in "hello".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.mode, Mode::Normal);
        assert_eq!(ed.buffer.line(0), Some("hello"));
    }

    #[test]
    fn append_puts_cursor_after() {
        let mut ed = ed_with("ab");
        ed.handle_key(key('a'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("aXb"));
    }

    #[test]
    fn dd_deletes_line_and_p_pastes() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('d'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("two"));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("one"));
    }

    #[test]
    fn x_deletes_char() {
        let mut ed = ed_with("abc");
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("bc"));
    }

    #[test]
    fn o_opens_line_below_in_insert() {
        let mut ed = ed_with("top");
        ed.handle_key(key('o'));
        assert_eq!(ed.mode, Mode::Insert);
        for c in "new".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(1), Some("new"));
    }

    #[test]
    fn undo_after_insert() {
        let mut ed = ed_with("abc");
        ed.handle_key(key('x')); // delete 'a'
        assert_eq!(ed.buffer.line(0), Some("bc"));
        ed.handle_key(key('u'));
        assert_eq!(ed.buffer.line(0), Some("abc"));
    }

    #[test]
    fn word_motion_forward() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w'));
        assert_eq!(ed.cursor.col, 4);
        ed.handle_key(key('w'));
        assert_eq!(ed.cursor.col, 8);
    }

    #[test]
    fn count_prefix_moves_multiple() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('3'));
        ed.handle_key(key('l'));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn goto_gg_and_bottom() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.handle_key(key('G'));
        assert_eq!(ed.cursor.row, 3);
        ed.handle_key(key('g'));
        ed.handle_key(key('g'));
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn visual_line_delete() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn search_forward_finds_next() {
        let mut ed = ed_with("alpha\nbeta\ngamma beta");
        ed.last_search = "beta".into();
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 1);
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn colon_enters_command_mode_and_returns_action() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        assert_eq!(ed.mode, Mode::Command);
        for c in "wq".chars() {
            ed.handle_key(key(c));
        }
        let action = ed.handle_key(special(KeyCode::Enter));
        assert_eq!(action, Action::RunEx("wq".into()));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn replace_char() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('r'));
        ed.handle_key(key('b'));
        assert_eq!(ed.buffer.line(0), Some("bat"));
    }

    fn big_buffer(lines: usize) -> Editor {
        let text: Vec<String> = (0..lines).map(|i| format!("line{i}")).collect();
        let mut ed = ed_with(&text.join("\n"));
        ed.view_rows = 10;
        ed
    }

    #[test]
    fn zz_zt_zb_position_viewport() {
        let mut ed = big_buffer(100);
        ed.cursor.row = 50;
        ed.handle_key(key('z'));
        ed.handle_key(key('z'));
        assert_eq!(ed.top, 45); // centered (50 - 10/2)

        ed.handle_key(key('z'));
        ed.handle_key(key('t'));
        assert_eq!(ed.top, 50); // line to top

        ed.handle_key(key('z'));
        ed.handle_key(key('b'));
        assert_eq!(ed.top, 41); // 50 + 1 - 10
    }

    #[test]
    fn hml_jump_within_viewport() {
        let mut ed = big_buffer(100);
        ed.top = 20;
        ed.cursor.row = 25;
        ed.handle_key(key('H'));
        assert_eq!(ed.cursor.row, 20);
        ed.handle_key(key('M'));
        assert_eq!(ed.cursor.row, 25); // 20 + 10/2
        ed.handle_key(key('L'));
        assert_eq!(ed.cursor.row, 29); // 20 + 10 - 1
    }

    #[test]
    fn ctrl_e_and_y_scroll_one_line() {
        let mut ed = big_buffer(100);
        ed.top = 10;
        ed.cursor.row = 15;
        ed.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert_eq!(ed.top, 11);
        ed.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert_eq!(ed.top, 10);
    }

    #[test]
    fn ctrl_e_pulls_cursor_into_view() {
        let mut ed = big_buffer(100);
        ed.top = 0;
        ed.cursor.row = 0;
        // Scroll down 5 lines; cursor (row 0) would be above the view, so it
        // should be pulled down to the new top.
        for _ in 0..5 {
            ed.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        }
        assert_eq!(ed.top, 5);
        assert_eq!(ed.cursor.row, 5);
    }

    fn rust_ed(text: &str) -> Editor {
        let mut ed = ed_with(text);
        ed.language = Language::Rust;
        ed
    }

    #[test]
    fn comment_toggle_gcc() {
        let mut ed = rust_ed("let x = 1;");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c')); // comment current line
        assert_eq!(ed.buffer.line(0), Some("// let x = 1;"));
        // Toggle back.
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("let x = 1;"));
    }

    #[test]
    fn comment_toggle_preserves_indent() {
        let mut ed = rust_ed("    indented();");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("    // indented();"));
    }

    #[test]
    fn comment_toggle_range_with_motion() {
        let mut ed = rust_ed("a();\nb();\nc();");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('j')); // comment current + next line
        assert_eq!(ed.buffer.line(0), Some("// a();"));
        assert_eq!(ed.buffer.line(1), Some("// b();"));
        assert_eq!(ed.buffer.line(2), Some("c();"));
    }

    #[test]
    fn comment_toggle_visual_and_sql_marker() {
        let mut ed = ed_with("SELECT 1;\nFROM t;");
        ed.language = Language::PgSql;
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("-- SELECT 1;"));
        assert_eq!(ed.buffer.line(1), Some("-- FROM t;"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn case_op_gu_with_motion() {
        let mut ed = ed_with("HELLO WORLD");
        ed.handle_key(key('g'));
        ed.handle_key(key('u'));
        ed.handle_key(key('w')); // lowercase "HELLO " -> "hello "
        assert_eq!(ed.buffer.line(0), Some("hello WORLD"));
    }

    #[test]
    fn case_op_g_upper_with_text_object() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // on "bar"
        ed.handle_key(key('g'));
        ed.handle_key(key('U'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // uppercase inner word
        assert_eq!(ed.buffer.line(0), Some("foo BAR baz"));
    }

    #[test]
    fn case_op_doubled_line() {
        let mut ed = ed_with("MixedCase Line");
        ed.handle_key(key('g'));
        ed.handle_key(key('u'));
        ed.handle_key(key('u')); // guu -> lowercase whole line
        assert_eq!(ed.buffer.line(0), Some("mixedcase line"));
    }

    #[test]
    fn case_op_toggle_with_dollar() {
        let mut ed = ed_with("aBcD");
        ed.handle_key(key('g'));
        ed.handle_key(key('~'));
        ed.handle_key(key('$')); // toggle to end of line
        assert_eq!(ed.buffer.line(0), Some("AbCd"));
    }

    #[test]
    fn text_object_diw() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // cursor on "bar" (col 4)
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // delete inner word "bar"
        assert_eq!(ed.buffer.line(0), Some("foo  baz"));
    }

    #[test]
    fn text_object_daw_removes_trailing_space() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // on "bar"
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('w')); // delete "bar " (a word)
        assert_eq!(ed.buffer.line(0), Some("foo baz"));
    }

    #[test]
    fn text_object_ci_parens() {
        let mut ed = ed_with("call(arg1, arg2)");
        ed.cursor = Position::new(0, 6); // inside parens (on 'r' of arg1)
        ed.handle_key(key('c'));
        ed.handle_key(key('i'));
        ed.handle_key(key('(')); // change inner parens
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("call(X)"));
    }

    #[test]
    fn text_object_di_quotes() {
        let mut ed = ed_with("say \"hello world\" now");
        // move cursor inside the quotes
        ed.cursor = Position::new(0, 8);
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('"'));
        assert_eq!(ed.buffer.line(0), Some("say \"\" now"));
    }

    #[test]
    fn text_object_da_parens_includes_delims() {
        let mut ed = ed_with("x(inner)y");
        ed.cursor = Position::new(0, 3);
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('(')); // delete "(inner)"
        assert_eq!(ed.buffer.line(0), Some("xy"));
    }

    #[test]
    fn dot_repeats_x() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x')); // delete 'a' -> "bcdef"
        assert_eq!(ed.buffer.line(0), Some("bcdef"));
        ed.handle_key(key('.')); // repeat -> "cdef"
        assert_eq!(ed.buffer.line(0), Some("cdef"));
        ed.handle_key(key('.')); // -> "def"
        assert_eq!(ed.buffer.line(0), Some("def"));
    }

    #[test]
    fn dot_repeats_dd() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "a"
        assert_eq!(ed.buffer.line(0), Some("b"));
        ed.handle_key(key('.')); // delete "b"
        assert_eq!(ed.buffer.line(0), Some("c"));
    }

    #[test]
    fn dot_repeats_insert_change() {
        let mut ed = ed_with("one\ntwo");
        // Insert "# " at the start of the line.
        ed.handle_key(key('I'));
        ed.handle_key(key('#'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("# one"));
        // Move to next line and repeat.
        ed.handle_key(key('j'));
        ed.handle_key(key('.'));
        assert_eq!(ed.buffer.line(1), Some("# two"));
    }

    #[test]
    fn dot_unchanged_by_navigation() {
        let mut ed = ed_with("abc\ndef");
        ed.handle_key(key('x')); // change: delete 'a'
        ed.handle_key(key('j')); // navigation (no change)
        ed.handle_key(key('0'));
        ed.handle_key(key('.')); // should repeat the delete, not the navigation
        assert_eq!(ed.buffer.line(1), Some("ef"));
    }

    #[test]
    fn macro_record_and_replay() {
        let mut ed = ed_with("a\nb\nc\nd");
        // Record into register q: delete a line (dd).
        ed.handle_key(key('q'));
        ed.handle_key(key('q')); // start recording into q
        assert_eq!(ed.recording_register(), Some('q'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // dd (recorded)
        ed.handle_key(key('q')); // stop recording
        assert_eq!(ed.recording_register(), None);
        assert_eq!(ed.buffer.line(0), Some("b"));
        // Replay: delete another line.
        ed.handle_key(key('@'));
        ed.handle_key(key('q'));
        assert_eq!(ed.buffer.line(0), Some("c"));
        // @@ repeats the last macro.
        ed.handle_key(key('@'));
        ed.handle_key(key('@'));
        assert_eq!(ed.buffer.line(0), Some("d"));
    }

    #[test]
    fn macro_records_insert_sequence() {
        let mut ed = ed_with("x\ny");
        ed.handle_key(key('q'));
        ed.handle_key(key('a')); // record into a
        ed.handle_key(key('I')); // insert at line start
        ed.handle_key(key('>'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(key('q')); // stop
        assert_eq!(ed.buffer.line(0), Some("> x"));
        // Replay on the next line.
        ed.handle_key(key('j'));
        ed.handle_key(key('0'));
        ed.handle_key(key('@'));
        ed.handle_key(key('a'));
        assert_eq!(ed.buffer.line(1), Some("> y"));
    }

    #[test]
    fn mark_set_and_jump_exact() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.cursor = Position::new(2, 1);
        ed.handle_key(key('m'));
        ed.handle_key(key('a')); // set mark a at (2,1)
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // to top
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(key('`'));
        ed.handle_key(key('a')); // jump back to mark a
        assert_eq!(ed.cursor, Position::new(2, 1));
    }

    #[test]
    fn mark_jump_line_lands_on_first_nonblank() {
        let mut ed = ed_with("l0\n  indented\nl2");
        ed.cursor = Position::new(1, 5);
        ed.handle_key(key('m'));
        ed.handle_key(key('x'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g'));
        ed.handle_key(key('\'')); // 'x -> line of mark, first non-blank
        ed.handle_key(key('x'));
        assert_eq!(ed.cursor.row, 1);
        assert_eq!(ed.cursor.col, 2); // first non-blank
    }

    #[test]
    fn backtick_backtick_returns_to_previous() {
        let mut ed = ed_with("a\nb\nc\nd\ne");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('G')); // jump to last line, records previous (1,0)
        assert_eq!(ed.cursor.row, 4);
        ed.handle_key(key('`'));
        ed.handle_key(key('`')); // back to previous
        assert_eq!(ed.cursor.row, 1);
    }

    #[test]
    fn count_before_operator_3dd() {
        let mut ed = ed_with("a\nb\nc\nd\ne");
        ed.handle_key(key('3'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete 3 lines
        assert_eq!(ed.buffer.line(0), Some("d"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn count_between_operator_and_motion_d3w() {
        let mut ed = ed_with("one two three four");
        ed.handle_key(key('d'));
        ed.handle_key(key('3'));
        ed.handle_key(key('w')); // delete 3 words
        assert_eq!(ed.buffer.line(0), Some("four"));
    }

    #[test]
    fn multiplied_counts_2d3w() {
        let mut ed = ed_with("a b c d e f g");
        ed.handle_key(key('2'));
        ed.handle_key(key('d'));
        ed.handle_key(key('3'));
        ed.handle_key(key('w')); // 2*3 = 6 words deleted
        assert_eq!(ed.buffer.line(0), Some("g"));
    }

    #[test]
    fn count_paste_3p() {
        let mut ed = ed_with("x");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "x" linewise
        ed.handle_key(key('3'));
        ed.handle_key(key('p')); // paste 3 times
        assert_eq!(ed.buffer.line_count(), 4);
    }

    #[test]
    fn yank_word_and_paste() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('y'));
        ed.handle_key(key('w')); // yank "foo "
        ed.handle_key(key('$'));
        ed.handle_key(key('p')); // paste after last char
        assert_eq!(ed.buffer.line(0), Some("foo barfoo "));
    }

    #[test]
    fn yank_to_eol() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('w')); // cursor at col 6 (start of "world")
        ed.handle_key(key('y'));
        ed.handle_key(key('$')); // yank "world"
        ed.handle_key(key('0'));
        ed.handle_key(key('P')); // paste before line start
        assert_eq!(ed.buffer.line(0), Some("worldhello world"));
    }

    #[test]
    fn delete_to_line_start() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('3'));
        ed.handle_key(key('l')); // col 3
        ed.handle_key(key('d'));
        ed.handle_key(key('0')); // delete cols [0,3)
        assert_eq!(ed.buffer.line(0), Some("lo"));
    }

    #[test]
    fn delete_down_two_lines() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('j')); // delete current + next (a, b)
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn delete_to_end_with_d_g() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.handle_key(key('j')); // row 1
        ed.handle_key(key('d'));
        ed.handle_key(key('G')); // delete rows 1..=3
        assert_eq!(ed.buffer.line_count(), 1);
        assert_eq!(ed.buffer.line(0), Some("l0"));
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn ge_moves_to_previous_word_end() {
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 9); // on 'a' of "baz"
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // end of "bar" -> col 6
        assert_eq!(ed.cursor.col, 6);
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // end of "foo" -> col 2
        assert_eq!(ed.cursor.col, 2);
    }

    #[test]
    fn ge_stops_at_punctuation_but_big_e_spans() {
        let mut ed = ed_with("foo.bar baz");
        ed.cursor = Position::new(0, 8); // on 'b' of "baz"
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // small ge -> end of "bar" (col 6)
        assert_eq!(ed.cursor.col, 6);
        let mut ed2 = ed_with("foo.bar baz");
        ed2.cursor = Position::new(0, 8);
        ed2.handle_key(key('g'));
        ed2.handle_key(key('E')); // big gE -> end of WORD "foo.bar" (col 6 too here)
        assert_eq!(ed2.cursor.col, 6);
    }

    #[test]
    fn block_delete_removes_rectangle() {
        let mut ed = ed_with("abcd\nefgh\nijkl");
        // cursor at (0,1); block select to (2,2) -> columns 1..=2 over 3 rows
        ed.handle_key(key('l')); // col 1
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // row 2
        ed.handle_key(key('l')); // col 2
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("ad"));
        assert_eq!(ed.buffer.line(1), Some("eh"));
        assert_eq!(ed.buffer.line(2), Some("il"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn block_insert_prepends_each_row() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // block over column 0, rows 0..2
        ed.handle_key(key('I'));
        ed.handle_key(key('#'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("# one"));
        assert_eq!(ed.buffer.line(1), Some("# two"));
        assert_eq!(ed.buffer.line(2), Some("# three"));
    }

    #[test]
    fn block_append_pads_short_rows() {
        let mut ed = ed_with("aa\nb\nccc");
        ed.handle_key(key('$')); // col 1 on "aa"
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // rows 0..2, col ~1
        ed.handle_key(key('A'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Esc));
        // Append at column 2 (cmax+1); short rows get padded, longer rows get
        // the text inserted at that column.
        assert_eq!(ed.buffer.line(0), Some("aaX"));
        assert_eq!(ed.buffer.line(1), Some("b X"));
        assert_eq!(ed.buffer.line(2), Some("ccXc"));
    }

    #[test]
    fn ctrl_a_increments_number() {
        let mut ed = ed_with("value = 41");
        ed.handle_key(ctrl('a')); // cursor at 0; finds 41 -> 42
        assert_eq!(ed.buffer.line(0), Some("value = 42"));
        assert_eq!(ed.cursor.col, 9); // on last digit
    }

    #[test]
    fn ctrl_x_decrements_with_count() {
        let mut ed = ed_with("x10y");
        ed.handle_key(key('5'));
        ed.handle_key(ctrl('x')); // 10 - 5 = 5
        assert_eq!(ed.buffer.line(0), Some("x5y"));
    }

    #[test]
    fn ctrl_a_handles_negative() {
        let mut ed = ed_with("n = -1");
        ed.handle_key(key('$')); // on '1'
        ed.handle_key(ctrl('a')); // -1 + 1 = 0
        assert_eq!(ed.buffer.line(0), Some("n = 0"));
    }

    #[test]
    fn ctrl_a_crosses_into_negative() {
        let mut ed = ed_with("3");
        ed.handle_key(key('5'));
        ed.handle_key(ctrl('x')); // 3 - 5 = -2
        assert_eq!(ed.buffer.line(0), Some("-2"));
    }

    #[test]
    fn insert_ctrl_r_pastes_register() {
        let mut ed = ed_with("word\ntarget");
        ed.handle_key(key('y'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // yiw -> unnamed = "word"
        ed.handle_key(key('j'));
        ed.handle_key(key('A')); // append at end of "target"
        ed.handle_key(ctrl('r'));
        ed.handle_key(key('"')); // paste unnamed register
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(1), Some("targetword"));
    }

    #[test]
    fn insert_ctrl_r_named_register() {
        let mut ed = ed_with("hi");
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // "ayy -> register a = "hi"
        ed.handle_key(key('A'));
        ed.handle_key(ctrl('r'));
        ed.handle_key(key('a'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("hihi"));
    }

    #[test]
    fn insert_ctrl_t_and_ctrl_d_indent() {
        let mut ed = ed_with("code");
        ed.shiftwidth = 2;
        ed.handle_key(key('A')); // insert at end, cursor col 4
        ed.handle_key(ctrl('t')); // indent -> "  code", cursor col 6
        assert_eq!(ed.buffer.line(0), Some("  code"));
        assert_eq!(ed.cursor.col, 6);
        ed.handle_key(ctrl('d')); // dedent -> "code", cursor col 4
        assert_eq!(ed.buffer.line(0), Some("code"));
        assert_eq!(ed.cursor.col, 4);
    }

    #[test]
    fn count_insert_repeats_text() {
        let mut ed = ed_with("");
        ed.handle_key(key('3'));
        ed.handle_key(key('i'));
        ed.handle_key(key('h'));
        ed.handle_key(key('i'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("hihihi"));
    }

    #[test]
    fn count_append_repeats() {
        let mut ed = ed_with("x");
        ed.handle_key(key('3'));
        ed.handle_key(key('a'));
        ed.handle_key(key('-'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("x---"));
    }

    #[test]
    fn count_open_creates_multiple_lines() {
        let mut ed = ed_with("top");
        ed.handle_key(key('3'));
        ed.handle_key(key('o'));
        ed.handle_key(key('z'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("top"));
        assert_eq!(ed.buffer.line(1), Some("z"));
        assert_eq!(ed.buffer.line(2), Some("z"));
        assert_eq!(ed.buffer.line(3), Some("z"));
        assert_eq!(ed.buffer.line_count(), 4);
    }

    #[test]
    fn plain_insert_not_repeated() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(key('a'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("a"));
    }

    #[test]
    fn replace_mode_overtypes() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('R'));
        assert_eq!(ed.mode, Mode::Replace);
        ed.handle_key(key('J'));
        ed.handle_key(key('A'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("JAllo"));
    }

    #[test]
    fn replace_mode_appends_past_eol() {
        let mut ed = ed_with("ab");
        ed.handle_key(key('$')); // on 'b' (col 1)
        ed.handle_key(key('R'));
        ed.handle_key(key('X')); // overwrite 'b' -> "aX"
        ed.handle_key(key('Y')); // past EOL -> append
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("aXY"));
    }

    #[test]
    fn replace_mode_backspace_restores_original() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('R'));
        ed.handle_key(key('X')); // c->X "Xat"
        ed.handle_key(key('Y')); // a->Y "XYt"
        ed.handle_key(special(KeyCode::Backspace)); // restore 'a' -> "Xat"
        ed.handle_key(special(KeyCode::Backspace)); // restore 'c' -> "cat"
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("cat"));
    }

    #[test]
    fn line_motions_plus_minus_underscore() {
        let mut ed = ed_with("a\n  b\n   c\nd");
        ed.handle_key(key('+')); // next line, first non-blank
        assert_eq!(ed.cursor, Position::new(1, 2));
        ed.handle_key(key('+'));
        assert_eq!(ed.cursor, Position::new(2, 3));
        ed.handle_key(key('-')); // prev line, first non-blank
        assert_eq!(ed.cursor, Position::new(1, 2));
        ed.handle_key(key('2'));
        ed.handle_key(key('_')); // down count-1 = 1 line, first non-blank
        assert_eq!(ed.cursor, Position::new(2, 3));
    }

    #[test]
    fn goto_column_bar() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('4'));
        ed.handle_key(key('|')); // column 4 (0-based 3)
        assert_eq!(ed.cursor.col, 3);
        ed.handle_key(key('|')); // bare | -> column 1 (0-based 0)
        assert_eq!(ed.cursor.col, 0);
    }

    #[test]
    fn g_underscore_last_nonblank() {
        let mut ed = ed_with("hello   ");
        ed.handle_key(key('g'));
        ed.handle_key(key('_'));
        assert_eq!(ed.cursor.col, 4); // 'o', ignoring trailing spaces
    }

    #[test]
    fn delete_to_next_line_with_plus() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('+')); // delete current + next line
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn visual_o_swaps_ends() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // cursor col 2
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // anchor 2, cursor 4
        ed.handle_key(key('o')); // swap -> cursor 2, anchor 4
        assert_eq!(ed.cursor.col, 2);
        // Extend left; selection start moves with cursor.
        ed.handle_key(key('h'));
        let (s, e) = ed.selection().unwrap();
        assert_eq!(s.col, 1);
        assert_eq!(e.col, 4);
    }

    #[test]
    fn gv_reselects_last_visual() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select cols 0..=2
        ed.handle_key(special(KeyCode::Esc)); // exit visual
        assert_eq!(ed.mode, Mode::Normal);
        ed.handle_key(key('g'));
        ed.handle_key(key('v')); // reselect
        assert_eq!(ed.mode, Mode::Visual);
        let (s, e) = ed.selection().unwrap();
        assert_eq!((s.col, e.col), (0, 2));
    }

    #[test]
    fn gv_reselects_after_operation() {
        let mut ed = ed_with("HELLO");
        ed.handle_key(key('v'));
        ed.handle_key(key('l')); // select "HE"
        ed.handle_key(key('u')); // lowercase -> "heLLO", exits visual
        assert_eq!(ed.buffer.line(0), Some("heLLO"));
        ed.handle_key(key('g'));
        ed.handle_key(key('v')); // reselect same extent
        ed.handle_key(key('U')); // uppercase it back
        assert_eq!(ed.buffer.line(0), Some("HELLO"));
    }

    #[test]
    fn yank_register_zero() {
        let mut ed = ed_with("yanked\ndeleted\ntarget");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "yanked" -> "0 and unnamed
        ed.handle_key(key('j'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "deleted" -> "1 and unnamed
        // Unnamed now holds the delete; "0 still holds the yank.
        ed.handle_key(key('"'));
        ed.handle_key(key('0'));
        ed.handle_key(key('p')); // paste "0 (the yank)
        assert_eq!(ed.buffer.line(2), Some("yanked"));
    }

    #[test]
    fn numbered_delete_registers_shift() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "one" -> "1
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "two" -> "1, "one" shifts to "2
        // "1 == most recent delete ("two"), "2 == older ("one").
        ed.handle_key(key('"'));
        ed.handle_key(key('1'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("two"));
        ed.handle_key(key('"'));
        ed.handle_key(key('2'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("one"));
    }

    #[test]
    fn small_delete_register_dash() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x')); // delete 'a' (small) -> "-
        ed.handle_key(key('$'));
        ed.handle_key(key('"'));
        ed.handle_key(key('-'));
        ed.handle_key(key('p')); // paste small-delete register
        assert_eq!(ed.buffer.line(0), Some("bcdefa"));
    }

    #[test]
    fn jumplist_back_and_forward() {
        let mut ed = ed_with("l0\nl1\nl2\nl3\nl4\nl5");
        // Jump around with G/gg (both record jumps).
        ed.handle_key(key('G')); // from (0,0) to last line (row 5); records 0
        assert_eq!(ed.cursor.row, 5);
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // to row 0; records 5
        assert_eq!(ed.cursor.row, 0);
        // Ctrl-o goes back to the previous jump origin (row 5).
        ed.handle_key(ctrl('o'));
        assert_eq!(ed.cursor.row, 5);
        // Ctrl-o again -> row 0 (the earlier origin).
        ed.handle_key(ctrl('o'));
        assert_eq!(ed.cursor.row, 0);
        // Ctrl-i goes forward again.
        ed.handle_key(ctrl('i'));
        assert_eq!(ed.cursor.row, 5);
    }

    #[test]
    fn jumplist_back_with_no_history_is_noop() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(ctrl('o')); // nothing recorded yet
        assert_eq!(ed.cursor.row, 1);
    }

    #[test]
    fn shiftwidth_controls_indent() {
        let mut ed = ed_with("code");
        ed.shiftwidth = 2;
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("  code")); // 2 spaces
        ed.handle_key(key('<'));
        ed.handle_key(key('<'));
        assert_eq!(ed.buffer.line(0), Some("code"));
    }

    #[test]
    fn noexpandtab_indents_with_tab() {
        let mut ed = ed_with("code");
        ed.expandtab = false;
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("\tcode"));
    }

    #[test]
    fn insert_tab_respects_expandtab_and_tabstop() {
        let mut ed = ed_with("");
        ed.tabstop = 3;
        ed.handle_key(key('i'));
        ed.handle_key(special(KeyCode::Tab));
        assert_eq!(ed.buffer.line(0), Some("   ")); // 3 spaces
        let mut ed2 = ed_with("");
        ed2.expandtab = false;
        ed2.handle_key(key('i'));
        ed2.handle_key(special(KeyCode::Tab));
        assert_eq!(ed2.buffer.line(0), Some("\t"));
    }

    #[test]
    fn gj_joins_without_space() {
        let mut ed = ed_with("foo\nbar");
        ed.handle_key(key('g'));
        ed.handle_key(key('J'));
        assert_eq!(ed.buffer.line(0), Some("foobar"));
        // plain J inserts a space
        let mut ed2 = ed_with("foo\nbar");
        ed2.handle_key(key('J'));
        assert_eq!(ed2.buffer.line(0), Some("foo bar"));
    }

    #[test]
    fn paragraph_motions() {
        let mut ed = ed_with("a\nb\n\nc\nd\n\ne");
        ed.handle_key(key('}')); // to first blank (row 2)
        assert_eq!(ed.cursor.row, 2);
        ed.handle_key(key('}')); // to next blank (row 5)
        assert_eq!(ed.cursor.row, 5);
        ed.handle_key(key('{')); // back to blank (row 2)
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn paragraph_text_object_dip() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('p')); // delete the paragraph "a","b"
        assert_eq!(ed.buffer.line(0), Some(""));
        assert_eq!(ed.buffer.line(1), Some("c"));
    }

    #[test]
    fn paragraph_text_object_dap_eats_trailing_blank() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // delete "a","b" + the blank line
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn big_word_motions_w_b_e() {
        let mut ed = ed_with("foo.bar baz.qux");
        ed.handle_key(key('W')); // skip whole WORD "foo.bar" -> start of "baz.qux"
        assert_eq!(ed.cursor.col, 8);
        ed.handle_key(key('B')); // back to start of "foo.bar"
        assert_eq!(ed.cursor.col, 0);
        ed.handle_key(key('E')); // end of WORD "foo.bar"
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn small_w_stops_at_punctuation() {
        let mut ed = ed_with("foo.bar");
        ed.handle_key(key('w')); // small word stops at '.'
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn delete_big_word_d_w() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('d'));
        ed.handle_key(key('W')); // delete "foo.bar " (WORD + trailing space)
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn change_big_word_like_ce() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('c'));
        ed.handle_key(key('W')); // like cE: change "foo.bar", keep the space
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("X baz"));
    }

    #[test]
    fn capital_x_deletes_before_cursor() {
        let mut ed = ed_with("abcd");
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // col 2
        ed.handle_key(key('X')); // delete 'b'
        assert_eq!(ed.buffer.line(0), Some("acd"));
    }

    #[test]
    fn capital_y_yanks_lines() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('2'));
        ed.handle_key(key('Y')); // yank 2 lines
        ed.handle_key(key('G'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(3), Some("one"));
        assert_eq!(ed.buffer.line(4), Some("two"));
    }

    #[test]
    fn count_gg_goes_to_line() {
        let mut ed = ed_with("l0\nl1\nl2\nl3\nl4");
        ed.handle_key(key('3'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // 3gg -> line 3 (row 2)
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn operator_dgg_deletes_to_top() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('G')); // last line (row 3)
        ed.handle_key(key('k')); // row 2
        ed.handle_key(key('d'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // delete rows 0..=2
        assert_eq!(ed.buffer.line_count(), 1);
        assert_eq!(ed.buffer.line(0), Some("d"));
    }

    #[test]
    fn count_replace_3r() {
        let mut ed = ed_with("aaaa");
        ed.handle_key(key('3'));
        ed.handle_key(key('r'));
        ed.handle_key(key('x')); // replace 3 chars
        assert_eq!(ed.buffer.line(0), Some("xxxa"));
    }

    #[test]
    fn count_tilde_toggles_n_chars() {
        let mut ed = ed_with("abcd");
        ed.handle_key(key('3'));
        ed.handle_key(key('~')); // toggle 3 chars
        assert_eq!(ed.buffer.line(0), Some("ABCd"));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn shift_operator_with_motion() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('>'));
        ed.handle_key(key('j')); // indent 2 lines
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(1), Some("    b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
    }

    #[test]
    fn count_shift_lines() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('3'));
        ed.handle_key(key('>'));
        ed.handle_key(key('>')); // 3>> indent 3 lines
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(2), Some("    c"));
        assert_eq!(ed.buffer.line(3), Some("d"));
    }

    #[test]
    fn visual_x_deletes_selection() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "hel"
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("lo"));
    }

    #[test]
    fn text_object_a_big_w() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('W')); // delete a WORD "foo.bar " incl trailing space
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn change_word_behaves_like_ce() {
        // vim: `cw` acts like `ce` — it does NOT eat the trailing space.
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('c'));
        ed.handle_key(key('w'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("X bar"));
    }

    #[test]
    fn dd_and_yy_still_work() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('d'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("two"));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("two"));
    }

    #[test]
    fn autoindent_on_enter() {
        let mut ed = ed_with("    code");
        ed.handle_key(key('A')); // append at end of line
        ed.handle_key(special(KeyCode::Enter));
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(1), Some("    x"));
    }

    #[test]
    fn no_autoindent_when_disabled() {
        let mut ed = ed_with("    code");
        ed.autoindent = false;
        ed.handle_key(key('A'));
        ed.handle_key(special(KeyCode::Enter));
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(1), Some("x"));
    }

    #[test]
    fn insert_ctrl_w_deletes_word_before() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('A')); // insert at end
        ed.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(ed.buffer.line(0), Some("foo "));
    }

    #[test]
    fn insert_ctrl_u_deletes_to_line_start() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('l'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // col 3
        ed.handle_key(key('i')); // insert before col 3
        ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(ed.buffer.line(0), Some("lo"));
        assert_eq!(ed.cursor.col, 0);
    }

    #[test]
    fn named_register_yank_and_paste() {
        let mut ed = ed_with("alpha\nbeta\ngamma");
        // Yank line 0 into register a.
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Move down and paste from register a.
        ed.handle_key(key('j'));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("alpha"));
    }

    #[test]
    fn named_register_independent_from_unnamed() {
        let mut ed = ed_with("keep\nother");
        // Yank "keep" into register a.
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Now yank "other" into the unnamed register.
        ed.handle_key(key('j'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Unnamed paste yields "other"; register a still holds "keep".
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("other"));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(3), Some("keep"));
    }

    #[test]
    fn visual_uppercase_selection() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('v'));
        for _ in 0..4 {
            ed.handle_key(key('l')); // select "hello"
        }
        ed.handle_key(key('U'));
        assert_eq!(ed.buffer.line(0), Some("HELLO world"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_line_lowercase_and_toggle() {
        let mut ed = ed_with("MixedCase");
        ed.handle_key(key('V'));
        ed.handle_key(key('u'));
        assert_eq!(ed.buffer.line(0), Some("mixedcase"));
        ed.handle_key(key('V'));
        ed.handle_key(key('~'));
        assert_eq!(ed.buffer.line(0), Some("MIXEDCASE"));
    }

    #[test]
    fn sort_buffer_ascending_and_reverse() {
        let mut ed = ed_with("banana\napple\ncherry");
        ed.sort_buffer(false, false);
        assert_eq!(ed.buffer.line(0), Some("apple"));
        assert_eq!(ed.buffer.line(1), Some("banana"));
        assert_eq!(ed.buffer.line(2), Some("cherry"));
        ed.sort_buffer(true, false);
        assert_eq!(ed.buffer.line(0), Some("cherry"));
        assert_eq!(ed.buffer.line(2), Some("apple"));
    }

    #[test]
    fn sort_buffer_unique_removes_duplicates() {
        let mut ed = ed_with("b\na\nb\nc\na");
        ed.sort_buffer(false, true);
        assert_eq!(ed.buffer.line_count(), 3);
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
    }

    #[test]
    fn percent_matches_brackets() {
        let mut ed = ed_with("(a+b)");
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 4); // ( -> )
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 0); // ) -> (
    }

    #[test]
    fn percent_nested_brackets() {
        let mut ed = ed_with("(a(b)c)");
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 6); // outer ( -> outer )
    }

    #[test]
    fn percent_scans_forward_to_bracket_on_line() {
        let mut ed = ed_with("x = (1)");
        ed.handle_key(key('%')); // cursor at 0, not a bracket -> finds ( then matches )
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn percent_matches_across_lines() {
        let mut ed = ed_with("foo(\n  bar\n)");
        // move cursor onto the '(' at row 0 col 3
        ed.cursor = Position::new(0, 3);
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor, Position::new(2, 0));
    }

    #[test]
    fn find_char_f_and_t() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('f'));
        ed.handle_key(key('o'));
        assert_eq!(ed.cursor.col, 4);
        let mut ed2 = ed_with("hello world");
        ed2.handle_key(key('t'));
        ed2.handle_key(key('o'));
        assert_eq!(ed2.cursor.col, 3);
    }

    #[test]
    fn find_char_f_backward() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('$')); // col 4 ('o')
        ed.handle_key(key('F'));
        ed.handle_key(key('l'));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn repeat_find_semicolon_and_comma() {
        let mut ed = ed_with("o.o.o");
        ed.handle_key(key('f'));
        ed.handle_key(key('o')); // col 2
        assert_eq!(ed.cursor.col, 2);
        ed.handle_key(key(';')); // next o -> col 4
        assert_eq!(ed.cursor.col, 4);
        ed.handle_key(key(',')); // reverse -> col 2
        assert_eq!(ed.cursor.col, 2);
    }

    #[test]
    fn word_end_motion() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('e'));
        assert_eq!(ed.cursor.col, 2);
        ed.handle_key(key('e'));
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn delete_to_eol_with_d() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('5'));
        ed.handle_key(key('l')); // col 5
        ed.handle_key(key('D'));
        assert_eq!(ed.buffer.line(0), Some("hello"));
    }

    #[test]
    fn change_to_eol_with_c() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('5'));
        ed.handle_key(key('l')); // col 5
        ed.handle_key(key('C'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('!'));
        assert_eq!(ed.buffer.line(0), Some("hello!"));
    }

    #[test]
    fn toggle_case_tilde() {
        let mut ed = ed_with("aBc");
        ed.handle_key(key('~'));
        assert_eq!(ed.buffer.line(0), Some("ABc"));
        assert_eq!(ed.cursor.col, 1);
    }

    #[test]
    fn indent_and_dedent_line() {
        let mut ed = ed_with("code");
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("    code"));
        ed.handle_key(key('<'));
        ed.handle_key(key('<'));
        assert_eq!(ed.buffer.line(0), Some("code"));
    }

    #[test]
    fn visual_line_indent() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(1), Some("    b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn substitute_char_s() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('s'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('b'));
        assert_eq!(ed.buffer.line(0), Some("bat"));
    }

    #[test]
    fn substitute_line_s_keeps_indent() {
        let mut ed = ed_with("    keep");
        ed.handle_key(key('S'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("    x"));
    }

    #[test]
    fn substitute_regex_digits() {
        let mut ed = ed_with("item12 and item345");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: r"\d+".into(),
            replacement: "#".into(),
            global: true,
            ignorecase: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("item# and item#"));
    }

    #[test]
    fn substitute_ignorecase_flag() {
        let mut ed = ed_with("Foo FOO foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "x".into(),
            global: true,
            ignorecase: true,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 3);
        assert_eq!(ed.buffer.line(0), Some("x x x"));
    }

    #[test]
    fn substitute_vim_capture_group() {
        // vim-style backrefs: \1 \2 \3
        let mut ed = ed_with("2026-09-30");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: r"(\d+)-(\d+)-(\d+)".into(),
            replacement: r"\3/\2/\1".into(),
            global: false,
            ignorecase: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 1);
        assert_eq!(ed.buffer.line(0), Some("30/09/2026"));
    }

    #[test]
    fn repeat_substitute_with_ampersand() {
        let mut ed = ed_with("foo foo\nfoo foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: true,
            ignorecase: false,
        };
        ed.substitute(&spec); // line 0 -> "bar bar"
        assert_eq!(ed.buffer.line(0), Some("bar bar"));
        ed.handle_key(key('j'));
        ed.handle_key(key('&')); // repeat on line 1
        assert_eq!(ed.buffer.line(1), Some("bar bar"));
    }

    #[test]
    fn substitute_invalid_regex_matches_literally() {
        let mut ed = ed_with("a (b) c");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "(b".into(), // invalid regex -> literal
            replacement: "X".into(),
            global: false,
            ignorecase: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 1);
        assert_eq!(ed.buffer.line(0), Some("a X) c"));
    }

    fn menu_ed() -> Editor {
        let mut ed = ed_with("hello");
        ed.open_menu(crate::menu::build_menus(&["matrix"], &["wordcount"]));
        ed
    }

    #[test]
    fn menu_open_select_pastes_into_command_line() {
        let mut ed = menu_ed();
        assert!(ed.is_menu_open());
        ed.handle_key(special(KeyCode::Down)); // open File dropdown
        ed.handle_key(special(KeyCode::Enter)); // select "Write" (w)
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "w");
    }

    #[test]
    fn menu_esc_backs_out_then_closes() {
        let mut ed = menu_ed();
        ed.handle_key(special(KeyCode::Down)); // dropdown open (depth 1)
        ed.handle_key(special(KeyCode::Esc)); // back to bar only
        assert!(ed.is_menu_open());
        ed.handle_key(special(KeyCode::Esc)); // close
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn menu_mouse_select_runs_command() {
        let mut ed = menu_ed();
        ed.handle_key(special(KeyCode::Down)); // open File dropdown (level 0)
        ed.menu_mouse_select(0, 2); // click "Write & Quit" (wq)
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "wq");
    }

    #[test]
    fn menu_mouse_select_opens_submenu() {
        let mut ed = menu_ed();
        ed.menu_open_initial('v'); // View dropdown open at "Theme" (a submenu)
        ed.menu_mouse_select(0, 0); // click "Theme"
        assert!(ed.is_menu_open());
        assert_eq!(ed.menu().unwrap().depth(), 2); // submenu opened
    }

    #[test]
    fn menu_submenu_selection() {
        let mut ed = menu_ed();
        ed.menu_open_initial('v'); // View menu, dropdown open at "Theme"
        ed.handle_key(special(KeyCode::Enter)); // open Theme submenu
        ed.handle_key(special(KeyCode::Enter)); // first theme
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "theme matrix");
    }

    #[test]
    fn star_searches_word_under_cursor() {
        let mut ed = ed_with("foo bar foo baz");
        // cursor on first "foo" (col 0)
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 8); // second "foo"
    }

    #[test]
    fn hash_searches_backward() {
        let mut ed = ed_with("foo bar foo baz");
        ed.cursor = Position::new(0, 8); // on second "foo"
        ed.handle_key(key('#'));
        assert_eq!(ed.cursor.col, 0); // first "foo"
    }

    #[test]
    fn star_uses_word_boundaries() {
        let mut ed = ed_with("foo foobar foo");
        // whole-word "foo" is only at 0 and 11; from col 0, next is 11
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 11);
    }

    #[test]
    fn g_star_ignores_word_boundaries() {
        let mut ed = ed_with("foo foobar");
        // g* matches the substring "foo" inside "foobar" (col 4)
        ed.handle_key(key('g'));
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 4);
    }

    #[test]
    fn search_regex_finds_pattern() {
        let mut ed = ed_with("alpha1\nbeta22\ngamma333");
        ed.set_search(r"\d\d+".into()); // 2+ digits
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 1); // beta22
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 2); // gamma333
    }

    #[test]
    fn substitute_current_line_first_only() {
        let mut ed = ed_with("foo foo foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: false,
            ignorecase: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (1, 1));
        assert_eq!(ed.buffer.line(0), Some("bar foo foo"));
    }

    #[test]
    fn substitute_global_whole_file() {
        let mut ed = ed_with("a x a\nx a x\nno match");
        let spec = SubstituteSpec {
            range: SubRange::WholeFile,
            pattern: "x".into(),
            replacement: "Q".into(),
            global: true,
            ignorecase: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (3, 2));
        assert_eq!(ed.buffer.line(0), Some("a Q a"));
        assert_eq!(ed.buffer.line(1), Some("Q a Q"));
        assert_eq!(ed.buffer.line(2), Some("no match"));
    }

    #[test]
    fn substitute_numeric_range() {
        let mut ed = ed_with("z\nz\nz\nz");
        let spec = SubstituteSpec {
            range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(3)),
            pattern: "z".into(),
            replacement: "Y".into(),
            global: false,
            ignorecase: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (2, 2));
        assert_eq!(ed.buffer.line(0), Some("z"));
        assert_eq!(ed.buffer.line(1), Some("Y"));
        assert_eq!(ed.buffer.line(2), Some("Y"));
        assert_eq!(ed.buffer.line(3), Some("z"));
    }

    #[test]
    fn substitute_not_found_makes_no_change_and_no_undo() {
        let mut ed = ed_with("hello");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "zzz".into(),
            replacement: "!".into(),
            global: true,
            ignorecase: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 0);
        assert_eq!(ed.buffer.line(0), Some("hello"));
        // Nothing changed, so there should be nothing to undo.
        assert!(ed.buffer.undo(ed.cursor).is_none());
    }

    #[test]
    fn substitute_empty_replacement_deletes_text() {
        let mut ed = ed_with("re-mo-ve");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "-".into(),
            replacement: "".into(),
            global: true,
            ignorecase: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("remove"));
    }
}
