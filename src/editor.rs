//! The editor state machine: cursor, viewport, motions and edit operations.
//!
//! `Editor` owns the buffer and all *self-contained* behavior (Normal / Insert /
//! Visual editing, incremental search). Ex commands (`:...`) touch other
//! subsystems (themes, plugins), so [`handle_key`](Editor::handle_key) returns
//! an [`Action`] the [`crate::app::App`] executes.

use crate::buffer::{Buffer, Position};
use crate::command::{LineAddr, SubRange, SubstituteSpec};
use crate::mode::Mode;
use std::collections::HashMap;
use crate::syntax::{detect_language, Language};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    pub view_rows: usize,
    pub view_cols: usize,

    line_kind: LineKind,
    register: Register,
    registers: HashMap<char, Register>,
    pending_register: Option<char>,
    expect_register: bool,
    visual_anchor: Position,
    last_search: String,
    pending_count: Option<usize>,
    pending_op: Option<char>,
    pending_replace: bool,
    pending_find: Option<char>,
    last_find: Option<(char, char)>,
}

/// Spaces inserted/removed by the `>>` / `<<` shift operators.
const SHIFT_WIDTH: usize = 4;

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
            view_rows: 24,
            view_cols: 80,
            line_kind: LineKind::Ex,
            register: Register::default(),
            registers: HashMap::new(),
            pending_register: None,
            expect_register: false,
            visual_anchor: Position::default(),
            last_search: String::new(),
            pending_count: None,
            pending_op: None,
            pending_replace: false,
            pending_find: None,
            last_find: None,
        }
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

    /// Execute a `:s` substitution (literal matching). Returns
    /// `(substitutions, lines_changed)`.
    pub fn substitute(&mut self, spec: &SubstituteSpec) -> (usize, usize) {
        if spec.pattern.is_empty() {
            return (0, 0);
        }
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
            let (new, c) = replace_literal(line, &spec.pattern, &spec.replacement, spec.global);
            if c > 0 {
                subs += c;
                edits.push((row, new));
            }
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
        // Command-line editing takes priority when active.
        if self.mode == Mode::Command {
            return self.handle_cmdline(key);
        }
        match self.mode {
            Mode::Insert => {
                self.handle_insert(key);
                Action::None
            }
            Mode::Normal | Mode::Visual | Mode::VisualLine => self.handle_normal(key),
            Mode::Command => Action::None,
        }
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
                        self.last_search = text;
                        self.hlsearch = true;
                        self.search(true);
                        Action::None
                    }
                    LineKind::SearchBack => {
                        self.last_search = text;
                        self.hlsearch = true;
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
        // Insert-mode control shortcuts.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => self.insert_delete_word_before(),
                KeyCode::Char('u') => self.insert_delete_to_line_start(),
                _ => {}
            }
            self.scroll_into_view();
            return;
        }
        match key.code {
            KeyCode::Esc => {
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
                self.buffer.insert_str(self.cursor, "    ");
                self.cursor.col += 4;
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

    /// `Ctrl-u` in insert mode: delete from the line start to the cursor.
    fn insert_delete_to_line_start(&mut self) {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let new: String = chars[self.cursor.col.min(chars.len())..].iter().collect();
        self.buffer.set_line(self.cursor.row, new);
        self.cursor.col = 0;
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Action {
        // Pending `r<char>` replace.
        if self.pending_replace {
            self.pending_replace = false;
            if let KeyCode::Char(c) = key.code {
                self.checkpoint();
                self.buffer.replace_char(self.cursor, c);
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
                _ => {}
            }
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

        // Operator-pending (d, y, g).
        if let Some(op) = self.pending_op.take() {
            self.apply_operator(op, code);
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
            KeyCode::Char('w') => self.move_word_forward(count),
            KeyCode::Char('b') => self.move_word_backward(count),
            KeyCode::Char('e') => self.move_word_end(),
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
            KeyCode::Char('"') => self.expect_register = true,
            KeyCode::Char('G') => self.goto_line_or_end(count),
            KeyCode::Char('g') => self.pending_op = Some('g'),
            KeyCode::Char('z') => self.pending_op = Some('z'),
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
                if self.mode.is_visual() {
                    self.visual_delete();
                } else {
                    self.pending_op = Some('d');
                }
            }
            KeyCode::Char('y') => {
                if self.mode.is_visual() {
                    self.visual_yank();
                } else {
                    self.pending_op = Some('y');
                }
            }
            KeyCode::Char('c') => {
                if self.mode.is_visual() {
                    self.visual_delete();
                    self.mode = Mode::Insert;
                } else {
                    self.pending_op = Some('c');
                }
            }
            KeyCode::Char('x') => self.delete_char_under(count),
            KeyCode::Char('r') => self.pending_replace = true,
            KeyCode::Char('D') => self.delete_to_eol(),
            KeyCode::Char('C') => self.change_to_eol(),
            KeyCode::Char('s') => self.substitute_char(),
            KeyCode::Char('S') => self.substitute_line(),
            KeyCode::Char('~') => {
                if self.mode.is_visual() {
                    self.transform_selection(CaseOp::Toggle);
                } else {
                    self.toggle_case();
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
                }
            }
            KeyCode::Char('<') => {
                if self.mode.is_visual() {
                    self.shift_selection(false);
                } else {
                    self.pending_op = Some('<');
                }
            }
            KeyCode::Char('i') => self.enter_insert_here(),
            KeyCode::Char('a') => {
                self.move_right(1, true);
                self.enter_insert_here();
            }
            KeyCode::Char('I') => {
                self.move_first_nonblank();
                self.enter_insert_here();
            }
            KeyCode::Char('A') => {
                self.move_line_end_exclusive();
                self.enter_insert_here();
            }
            KeyCode::Char('o') => self.open_below(),
            KeyCode::Char('O') => self.open_above(),
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
            KeyCode::Char('p') => self.paste(true),
            KeyCode::Char('P') => self.paste(false),
            KeyCode::Char('J') => {
                self.checkpoint();
                self.buffer.join_line(self.cursor.row);
            }
            KeyCode::Char('v') => self.toggle_visual(Mode::Visual),
            KeyCode::Char('V') => self.toggle_visual(Mode::VisualLine),
            KeyCode::Char('n') => self.search_repeat(true),
            KeyCode::Char('N') => self.search_repeat(false),
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
                self.pending_count = None;
            }
            _ => {}
        }

        self.clamp_cursor(false);
        self.scroll_into_view();
        Action::None
    }

    fn apply_operator(&mut self, op: char, code: KeyCode) {
        match op {
            'g' => {
                if code == KeyCode::Char('g') {
                    self.cursor.row = 0;
                    self.cursor.col = 0;
                }
            }
            'z' => match code {
                KeyCode::Char('z') => self.center_line(),
                KeyCode::Char('t') => self.line_to_top(),
                KeyCode::Char('b') => self.line_to_bottom(),
                _ => {}
            },
            '>' => {
                if code == KeyCode::Char('>') {
                    self.checkpoint();
                    self.indent_line(self.cursor.row);
                    self.move_first_nonblank();
                }
            }
            '<' => {
                if code == KeyCode::Char('<') {
                    self.checkpoint();
                    self.dedent_line(self.cursor.row);
                    self.move_first_nonblank();
                }
            }
            'd' | 'y' | 'c' => {
                // Doubled operator (dd/yy/cc) acts on the whole current line.
                let doubled = code == KeyCode::Char(op);
                let target = if doubled {
                    Some(OpTarget::Lines(self.cursor.row, self.cursor.row))
                } else {
                    self.motion_target(code)
                };
                if let Some(t) = target {
                    self.apply_op(op, t);
                }
            }
            _ => {}
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    /// The text span a motion covers, relative to the cursor, for use by an
    /// operator (`d`/`y`/`c`). `None` for keys that aren't operator motions.
    fn motion_target(&self, code: KeyCode) -> Option<OpTarget> {
        let row = self.cursor.row;
        let col = self.cursor.col;
        let len = self.cur_len();
        let last = self.buffer.line_count().saturating_sub(1);
        Some(match code {
            KeyCode::Char('w') => OpTarget::Chars(col, self.word_forward_col()),
            KeyCode::Char('e') => OpTarget::Chars(col, (self.word_end_col() + 1).min(len)),
            KeyCode::Char('$') | KeyCode::End => OpTarget::Chars(col, len),
            KeyCode::Char('0') | KeyCode::Home => OpTarget::Chars(0, col),
            KeyCode::Char('^') => OpTarget::Chars(self.first_nonblank_col(), col),
            KeyCode::Char('l') | KeyCode::Right => OpTarget::Chars(col, (col + 1).min(len)),
            KeyCode::Char('h') | KeyCode::Left => OpTarget::Chars(col.saturating_sub(1), col),
            KeyCode::Char('j') | KeyCode::Down => OpTarget::Lines(row, (row + 1).min(last)),
            KeyCode::Char('k') | KeyCode::Up => OpTarget::Lines(row.saturating_sub(1), row),
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
                    self.store_register(text, false);
                    self.cursor.col = s;
                    if is_change {
                        self.mode = Mode::Insert;
                    }
                } else {
                    self.store_register(text, false);
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
                    self.store_register(text, true);
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
                    self.store_register(text, true);
                    self.cursor.row = a;
                }
            }
        }
        self.clamp_cursor(false);
        self.scroll_into_view();
    }

    fn first_nonblank_col(&self) -> usize {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        line.chars().take_while(|c| c.is_whitespace()).count()
    }

    /// The column of the next word start on the current line (bounded to EOL).
    fn word_forward_col(&self) -> usize {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let len = chars.len();
        let mut col = self.cursor.col;
        if col >= len {
            return len;
        }
        let class = Self::char_class(chars[col]);
        while col < len && Self::char_class(chars[col]) == class && class != 0 {
            col += 1;
        }
        while col < len && chars[col].is_whitespace() {
            col += 1;
        }
        col
    }

    /// The column of the end of the next word on the current line.
    fn word_end_col(&self) -> usize {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let len = chars.len();
        let mut i = self.cursor.col + 1;
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i < len {
            let class = Self::char_class(chars[i]);
            while i + 1 < len && Self::char_class(chars[i + 1]) == class {
                i += 1;
            }
            i
        } else {
            self.cursor.col
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

    fn move_word_forward(&mut self, count: usize) {
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
            let start_class = Self::char_class(chars[col]);
            // skip current run
            while col < chars.len() && Self::char_class(chars[col]) == start_class && start_class != 0
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

    fn move_word_end(&mut self) {
        let chars: Vec<char> = self.buffer.line(self.cursor.row).unwrap_or("").chars().collect();
        let len = chars.len();
        let mut i = self.cursor.col + 1;
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i < len {
            let class = Self::char_class(chars[i]);
            while i + 1 < len && Self::char_class(chars[i + 1]) == class {
                i += 1;
            }
            self.cursor.col = i;
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

    fn toggle_case(&mut self) {
        let line = self.buffer.line(self.cursor.row).unwrap_or("");
        let Some(ch) = line.chars().nth(self.cursor.col) else {
            return;
        };
        let toggled: String = if ch.is_uppercase() {
            ch.to_lowercase().collect()
        } else if ch.is_lowercase() {
            ch.to_uppercase().collect()
        } else {
            return; // non-alphabetic: no change, no cursor move
        };
        self.checkpoint();
        // Handle the (rare) case where case change alters char count.
        if toggled.chars().count() == 1 {
            self.buffer.replace_char(self.cursor, toggled.chars().next().unwrap());
        } else {
            self.buffer.delete_char(self.cursor);
            self.buffer.insert_str(self.cursor, &toggled);
        }
        self.move_right(1, false);
    }

    fn change_to_eol(&mut self) {
        self.checkpoint();
        let line = self.buffer.line(self.cursor.row).unwrap_or("").to_string();
        let byte = line
            .char_indices()
            .nth(self.cursor.col)
            .map(|(i, _)| i)
            .unwrap_or(line.len());
        self.store_register(line[byte..].to_string(), false);
        self.buffer.set_line(self.cursor.row, line[..byte].to_string());
        self.mode = Mode::Insert;
    }

    fn substitute_char(&mut self) {
        if self.cur_len() == 0 {
            self.enter_insert_here();
            return;
        }
        self.checkpoint();
        self.buffer.delete_char(self.cursor);
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
        self.buffer.set_line(row, format!("{}{line}", " ".repeat(SHIFT_WIDTH)));
    }

    fn dedent_line(&mut self, row: usize) {
        let line = self.buffer.line(row).unwrap_or("");
        let mut removed = 0;
        let new: String = {
            let mut chars = line.chars().peekable();
            // Remove up to SHIFT_WIDTH leading spaces, or a single leading tab.
            while removed < SHIFT_WIDTH {
                match chars.peek() {
                    Some(' ') => {
                        chars.next();
                        removed += 1;
                    }
                    Some('\t') if removed == 0 => {
                        chars.next();
                        removed += SHIFT_WIDTH;
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

    fn move_word_backward(&mut self, count: usize) {
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
                let class = Self::char_class(chars[col]);
                while col > 0 && Self::char_class(chars[col - 1]) == class {
                    col -= 1;
                }
            }
            self.cursor.col = col;
        }
    }

    // ---- inserts / opens -------------------------------------------------

    fn enter_insert_here(&mut self) {
        self.checkpoint();
        self.mode = Mode::Insert;
    }

    fn open_below(&mut self) {
        self.checkpoint();
        let indent = self.leading_indent(self.cursor.row);
        self.buffer.insert_line(self.cursor.row + 1, indent.clone());
        self.cursor.row += 1;
        self.cursor.col = indent.chars().count();
        self.mode = Mode::Insert;
    }

    fn open_above(&mut self) {
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
        self.store_register(removed, false);
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
        self.store_register(line[byte..].to_string(), false);
        self.buffer.set_line(self.cursor.row, kept);
        self.clamp_cursor(false);
    }

    /// Store text into the unnamed register, and into a named register too if
    /// one is pending (`"a…`). Clears the pending register.
    fn store_register(&mut self, text: String, linewise: bool) {
        let reg = Register { text, linewise };
        if let Some(name) = self.pending_register.take() {
            self.registers.insert(name, reg.clone());
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
            self.buffer.insert_line(row, reg.text.clone());
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
            self.mode = target;
            self.visual_anchor = self.cursor;
        }
    }

    fn visual_yank(&mut self) {
        if let Some((start, end)) = self.selection() {
            let linewise = self.mode == Mode::VisualLine;
            let text = self.extract_range(start, end, linewise);
            self.store_register(text, linewise);
        }
        self.mode = Mode::Normal;
    }

    fn visual_delete(&mut self) {
        if let Some((start, end)) = self.selection() {
            let linewise = self.mode == Mode::VisualLine;
            self.checkpoint();
            let text = self.extract_range(start, end, linewise);
            self.store_register(text, linewise);
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

    fn search(&mut self, forward: bool) {
        if self.last_search.is_empty() {
            return;
        }
        self.search_repeat(forward);
    }

    fn search_repeat(&mut self, forward: bool) {
        let needle = self.last_search.clone();
        if needle.is_empty() {
            self.message = "No previous search".into();
            return;
        }
        self.hlsearch = true;
        let n = self.buffer.line_count();
        if forward {
            // rest of current line after cursor, then following lines, then wrap
            for step in 0..=n {
                let row = (self.cursor.row + step) % n;
                let line = self.buffer.line(row).unwrap_or("");
                let from = if step == 0 { self.byte_after_cursor() } else { 0 };
                if let Some(bpos) = line[from.min(line.len())..].find(&needle) {
                    let byte = from + bpos;
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
                if let Some(bpos) = line[..limit.min(line.len())].rfind(&needle) {
                    self.cursor.row = row;
                    self.cursor.col = line[..bpos].chars().count();
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
        let max = if allow_eol || self.mode == Mode::Insert {
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

/// Literal (non-regex) find/replace within one line. Returns the new line and
/// the number of replacements made.
fn replace_literal(line: &str, pat: &str, rep: &str, global: bool) -> (String, usize) {
    if pat.is_empty() {
        return (line.to_string(), 0);
    }
    if global {
        let count = line.matches(pat).count();
        if count == 0 {
            (line.to_string(), 0)
        } else {
            (line.replace(pat, rep), count)
        }
    } else if let Some(idx) = line.find(pat) {
        let mut s = String::with_capacity(line.len() - pat.len() + rep.len());
        s.push_str(&line[..idx]);
        s.push_str(rep);
        s.push_str(&line[idx + pat.len()..]);
        (s, 1)
    } else {
        (line.to_string(), 0)
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

    #[test]
    fn change_word_still_works() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('c'));
        ed.handle_key(key('w')); // change "foo " -> insert
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("Xbar"));
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
    fn substitute_current_line_first_only() {
        let mut ed = ed_with("foo foo foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: false,
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
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("remove"));
    }
}
