//! The application: owns all subsystems, runs the event loop, and executes
//! ex-commands (dispatching unknown ones to plugins).

use crate::command::{self, ExCommand};
use crate::buffer::Buffer;
use crate::editor::{Action, BufferId, Editor};
use crate::mode::Mode;
use crate::plugin::{PluginDoc, PluginManager};
use crate::syntax::{Language, LineState, Registry};
use crate::terminal::TerminalGuard;
use crate::theme::ThemeRegistry;
use crate::ui::{self, Layout};
use crate::window::{self, Move, SplitKind, WinId};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::collections::HashMap;
use std::io::{self, BufWriter, Write};

/// How many recent status messages `:messages` keeps.
const MESSAGE_LOG_CAP: usize = 200;

/// How long the `:set visualbell` screen flash stays inverted, in milliseconds.
const VISUAL_BELL_MS: u64 = 80;

/// Split a `:set` argument list into individual options. A backslash escapes the
/// next character, so a value may contain spaces (vim's `equalprg=a\ b`) or a
/// literal backslash (`\\`); the escaping backslashes are removed. Returns the
/// options with their escapes resolved.
fn split_set_options(rest: &str) -> Vec<String> {
    let mut opts = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // A backslash takes the next character literally (or stays literal at
            // end of input).
            cur.push(chars.next().unwrap_or('\\'));
            in_token = true;
        } else if c.is_whitespace() {
            if in_token {
                opts.push(std::mem::take(&mut cur));
                in_token = false;
            }
        } else {
            cur.push(c);
            in_token = true;
        }
    }
    if in_token {
        opts.push(cur);
    }
    opts
}

/// One non-focused on-screen window: its layout id and the editor it shows.
struct WinPane {
    id: WinId,
    editor: Editor,
}

/// Top-level editor application.
pub struct App {
    /// The focused window's editor (also the active buffer).
    pub editor: Editor,
    /// Other open buffers (inactive), in list order.
    others: Vec<Editor>,
    /// The split-window geometry. A single `Leaf` until the first `:split`.
    layout: window::Layout,
    /// The other visible windows (everything in `layout` except the focused one).
    panes: Vec<WinPane>,
    /// The focused window's layout id; its editor is `self.editor`.
    focused: WinId,
    /// Monotonic source of fresh window ids.
    next_win: usize,
    /// Monotonic source of fresh buffer ids (window id 0's editor owns `BufferId(0)`).
    next_buf: usize,
    /// Set when `Ctrl-w` was pressed and the next key is a window command.
    pending_ctrl_w: bool,
    /// Last left-click (time, column, row, consecutive-click count) for detecting
    /// double-click (select word) and triple-click (select line).
    last_click: Option<(std::time::Instant, u16, u16, u8)>,
    themes: ThemeRegistry,
    syntax: Registry,
    plugins: PluginManager,
    quit: bool,
    want_mouse: bool,
    /// Memo for the block-comment fold feeding the first visible line, keyed by
    /// (buffer revision, language, top row). Avoids re-folding from the top of the
    /// buffer every frame when the view hasn't changed (e.g. cursor moving
    /// on-screen).
    block_memo: Option<(u64, Language, usize, LineState)>,
    /// The same memo, per window id, for the split-screen path so each pane's
    /// first-visible-line syntax state is only recomputed when its view changes.
    win_block_memo: HashMap<WinId, (u64, Language, usize, LineState)>,
    /// Path of the buffer most recently switched away from (vim's alternate file,
    /// reachable with `Ctrl-^` / `:b#`).
    alternate: Option<String>,
    /// User-defined ex commands (`:command Name expansion`), keyed by name.
    user_commands: HashMap<String, String>,
    /// Last window title pushed to the terminal (for `:set title`), so it is only
    /// re-sent when it changes.
    last_title: String,
    /// Lazily-loaded ctags index (from `./tags`) for `:tag` / `Ctrl-]`.
    tags: Option<crate::tags::Tags>,
    /// The tag stack: positions to return to with `:pop` / `Ctrl-t`.
    tag_stack: Vec<(String, usize, usize)>,
    /// Every match for the most recent `:tag`, with the currently-selected index,
    /// so `:tnext` / `:tprev` / `:tselect` can move between same-named definitions.
    tag_matches: Vec<crate::tags::TagEntry>,
    tag_match_idx: usize,
    tag_match_name: String,
    /// Global (uppercase `A`-`Z`) marks: mark -> (file path, row, col). Unlike the
    /// per-buffer lowercase marks, these persist across buffers and reopen the file.
    global_marks: HashMap<char, (String, usize, usize)>,
    /// Recent status-line messages, oldest first, for `:messages`. Captured once
    /// per frame when the message changes, so transient notes can be recalled.
    message_log: Vec<String>,
    /// The last message pushed to `message_log`, to avoid logging duplicates while
    /// a message stays on screen across frames.
    last_logged_msg: String,
}

impl App {
    /// A new app over an empty scratch buffer.
    pub fn new() -> Self {
        Self {
            editor: Editor::new(),
            others: Vec::new(),
            layout: window::Layout::new(WinId(0)),
            panes: Vec::new(),
            focused: WinId(0),
            next_win: 1,
            next_buf: 1,
            pending_ctrl_w: false,
            last_click: None,
            themes: ThemeRegistry::with_builtins(),
            syntax: Registry::with_builtins(),
            plugins: PluginManager::with_builtins(),
            quit: false,
            want_mouse: false,
            block_memo: None,
            win_block_memo: HashMap::new(),
            alternate: None,
            user_commands: HashMap::new(),
            last_title: String::new(),
            tags: None,
            tag_stack: Vec::new(),
            tag_matches: Vec::new(),
            tag_match_idx: 0,
            tag_match_name: String::new(),
            global_marks: HashMap::new(),
            message_log: Vec::new(),
            last_logged_msg: String::new(),
        }
    }

    /// Jump to a tag by name (`:tag` / `Ctrl-]`). Loads `./tags` on first use,
    /// pushes the current position onto the tag stack, then opens the tag's file
    /// and moves to its address (a line number or a `/pattern/` search).
    fn tag_jump(&mut self, name: &str) {
        if name.is_empty() {
            self.notify("E471: Argument required");
            return;
        }
        self.load_tags();
        let matches: Vec<crate::tags::TagEntry> = self
            .tags
            .as_ref()
            .map(|t| t.find_all(name).to_vec())
            .unwrap_or_default();
        if matches.is_empty() {
            self.editor.message = format!("E426: tag not found: {name}");
            return;
        }
        // Remember where we were so Ctrl-t can return.
        if let Some(p) = self.editor.buffer.path().map(|p| p.display().to_string()) {
            self.tag_stack
                .push((p, self.editor.cursor.row, self.editor.cursor.col));
        }
        self.tag_matches = matches;
        self.tag_match_idx = 0;
        self.tag_match_name = name.to_string();
        self.goto_current_tag_match();
    }

    /// Lazily read and parse `./tags` once.
    fn load_tags(&mut self) {
        if self.tags.is_none() {
            let text = std::fs::read_to_string("tags").unwrap_or_default();
            self.tags = Some(crate::tags::Tags::parse(&text));
        }
    }

    /// Open the file for the currently-selected tag match and move to its address.
    /// Does not touch the tag stack (the original position is already recorded).
    fn goto_current_tag_match(&mut self) {
        let Some(entry) = self.tag_matches.get(self.tag_match_idx).cloned() else {
            return;
        };
        match crate::tags::parse_address(&entry.address) {
            Some(crate::tags::Address::Line(n)) => self.edit_file(&entry.file, Some(n)),
            Some(crate::tags::Address::Search(pat)) => {
                self.edit_file(&entry.file, None);
                // ctags patterns are literal line text (with `^`/`$` anchors), which
                // may contain regex metacharacters — match them literally.
                let body = pat.trim_start_matches('^').trim_end_matches('$');
                self.editor
                    .search_from_start(&format!("^{}", regex::escape(body)));
            }
            None => self.edit_file(&entry.file, None),
        }
        let total = self.tag_matches.len();
        self.editor.message = if total > 1 {
            format!("tag {} of {}: {}", self.tag_match_idx + 1, total, self.tag_match_name)
        } else {
            format!("tag: {}", self.tag_match_name)
        };
    }

    /// `:tnext` / `:tprev` / `:tfirst` / `:tlast` — move within the match list.
    fn tag_match_move(&mut self, mv: crate::command::TagMove) {
        use crate::command::TagMove;
        let total = self.tag_matches.len();
        if total == 0 {
            self.notify("E73: tag stack empty");
            return;
        }
        let next = match mv {
            TagMove::Next => {
                if self.tag_match_idx + 1 >= total {
                    self.notify("E428: Cannot go beyond last matching tag");
                    return;
                }
                self.tag_match_idx + 1
            }
            TagMove::Prev => {
                if self.tag_match_idx == 0 {
                    self.notify("E425: Cannot go before first matching tag");
                    return;
                }
                self.tag_match_idx - 1
            }
            TagMove::First => 0,
            TagMove::Last => total - 1,
        };
        self.tag_match_idx = next;
        self.goto_current_tag_match();
    }

    /// `:tselect [name]` — list every match for the tag so one can be chosen.
    fn tag_select(&mut self, name: Option<&str>) {
        // A bare `:tselect` reuses the current tag; a name loads its matches.
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            self.load_tags();
            let matches: Vec<crate::tags::TagEntry> = self
                .tags
                .as_ref()
                .map(|t| t.find_all(name).to_vec())
                .unwrap_or_default();
            if matches.is_empty() {
                self.editor.message = format!("E426: tag not found: {name}");
                return;
            }
            self.tag_matches = matches;
            self.tag_match_idx = 0;
            self.tag_match_name = name.to_string();
        }
        if self.tag_matches.is_empty() {
            self.notify("E73: tag stack empty");
            return;
        }
        let mut body = format!("tags matching \"{}\" — :bd to close\n\n", self.tag_match_name);
        body.push_str("  use :tnext / :tprev / :tfirst / :tlast to move\n\n");
        for (i, e) in self.tag_matches.iter().enumerate() {
            let cur = if i == self.tag_match_idx { '>' } else { ' ' };
            body.push_str(&format!("{} {}  {}  {}\n", cur, i + 1, e.file, e.address));
        }
        self.open_scratch(&body, "tags matching", "tag matches — :bd to close");
    }

    /// `:pop` / `Ctrl-t` — return to the position before the last tag jump.
    fn tag_pop(&mut self) {
        let Some((file, row, col)) = self.tag_stack.pop() else {
            self.notify("E73: tag stack empty");
            return;
        };
        self.edit_file(&file, None);
        self.editor.set_cursor_clamped(row, col);
    }

    /// Record the current status message for `:messages` if it changed. Called
    /// once per frame; keeps the most recent [`MESSAGE_LOG_CAP`] entries.
    fn log_message(&mut self) {
        let m = &self.editor.message;
        if m.is_empty() || *m == self.last_logged_msg {
            return;
        }
        self.last_logged_msg = m.clone();
        self.message_log.push(m.clone());
        let overflow = self.message_log.len().saturating_sub(MESSAGE_LOG_CAP);
        if overflow > 0 {
            self.message_log.drain(0..overflow);
        }
    }

    /// A `:messages` listing, newest last (vim order).
    fn messages_listing(&self) -> String {
        if self.message_log.is_empty() {
            return "messages — :bd to close\n\n  (no messages)\n".to_string();
        }
        let mut out = String::from("messages — :bd to close\n\n");
        for m in &self.message_log {
            out.push_str("  ");
            out.push_str(m);
            out.push('\n');
        }
        out
    }

    /// Expand a leading user-defined command (`:command`) into its definition,
    /// appending any typed arguments. Resolves chains with a bounded loop so a
    /// self-referential alias can't spin forever. Returns the resolved input.
    fn resolve_user_command(&self, input: &str) -> String {
        let mut cur = input.trim().to_string();
        for _ in 0..16 {
            let (word, rest) = match cur.split_once(char::is_whitespace) {
                Some((w, r)) => (w.to_string(), r.trim().to_string()),
                None => (cur.clone(), String::new()),
            };
            // User commands must start uppercase, so builtins are never shadowed.
            if !word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                break;
            }
            match self.user_commands.get(&word) {
                Some(exp) if rest.is_empty() => cur = exp.clone(),
                Some(exp) => cur = format!("{exp} {rest}"),
                None => break,
            }
        }
        cur
    }

    /// Record the current buffer as the alternate (called before switching away).
    fn remember_alternate(&mut self) {
        self.alternate = self.editor.buffer.path().map(|p| p.display().to_string());
    }

    /// `Ctrl-^` / `:b#` — switch to the alternate buffer (the last one left).
    fn switch_alternate(&mut self) {
        let Some(alt) = self.alternate.clone() else {
            self.editor.message = "E23: No alternate file".into();
            return;
        };
        let pos = self.others.iter().position(|e| {
            e.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(alt.as_str())
        });
        match pos {
            Some(i) => {
                self.autowrite_current();
                self.remember_alternate();
                std::mem::swap(&mut self.editor, &mut self.others[i]);
                self.editor.message = format!("\"{alt}\"");
            }
            None => self.editor.message = format!("E23: alternate buffer not open: {alt}"),
        }
    }

    /// A new app editing `path` (created on save if it doesn't exist).
    pub fn open(path: &str) -> io::Result<Self> {
        let mut app = App::new();
        app.editor = Editor::from_file(path)?;
        app.editor.buf_id = app.fresh_buf_id();
        let lines = app.editor.buffer.line_count().to_string();
        app.notify_fmt("\"{path}\" {lines} lines", &[("{path}", path), ("{lines}", &lines)]);
        Ok(app)
    }

    /// Load `path` into an inactive buffer (for extra files given on the command
    /// line); the first file stays active. Reachable afterward with `:bn`/`:bp`.
    pub fn open_additional(&mut self, path: &str) -> io::Result<()> {
        let mut ed = Editor::from_file(path)?;
        self.inherit_prefs(&mut ed);
        ed.buf_id = self.fresh_buf_id();
        self.others.push(ed);
        Ok(())
    }

    /// Mark every loaded buffer read-only (the `-R` command-line flag): `:w` is then
    /// refused unless forced with `:w!`.
    pub fn set_readonly_all(&mut self) {
        self.editor.readonly = true;
        for e in &mut self.others {
            e.readonly = true;
        }
        for p in &mut self.panes {
            p.editor.readonly = true;
        }
    }

    /// Pick a starting theme by name (falls back to the default silently).
    pub fn set_theme(&mut self, name: &str) {
        self.themes.set_current(name);
    }

    /// Load and apply the user's `rvimrc` (if present). Safe to call before
    /// `run`; missing files are silently ignored.
    pub fn load_config(&mut self) {
        if let Some(path) = crate::config::default_config_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let cmds = crate::config::parse_config(&text);
                let n = cmds.len();
                self.apply_config_lines(&cmds);
                self.editor.message = format!("{}: {n} settings applied", path.display());
            }
        }
    }

    /// Apply a list of ex-command strings (used for config and `:source`).
    pub fn apply_config_lines(&mut self, lines: &[String]) {
        for line in lines {
            self.run_ex(line);
        }
    }

    /// Apply any modeline options found in the focused buffer (vim's `modeline`).
    /// Each requested option goes through the normal `:set` path, so only options
    /// are ever applied — never commands. The pre-existing message (e.g. the
    /// file-load line) is preserved. Call this after loading a file from disk.
    pub fn apply_modelines(&mut self) {
        if !self.editor.modeline {
            return;
        }
        // Mark done even when nothing is found, so a later focus doesn't re-scan.
        self.editor.modeline_done = true;
        let opts = self.editor.modeline_options();
        if opts.is_empty() {
            return;
        }
        let saved = std::mem::take(&mut self.editor.message);
        self.run_ex(&format!("set {}", opts.join(" ")));
        self.editor.message = saved;
    }

    /// Apply the focused buffer's modeline the first time it is seen. Called when
    /// a buffer becomes focused (buffer switch), so files opened in the background
    /// (extra CLI arguments, `:bn`/`:b`) get their modeline on first view.
    fn apply_modelines_if_needed(&mut self) {
        if !self.editor.modeline_done {
            self.apply_modelines();
        }
    }

    // ---- buffer management ----------------------------------------------

    fn buffer_name(ed: &Editor) -> String {
        ed.buffer
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string())
    }

    /// Carry per-view preferences to a newly-activated editor.
    fn inherit_prefs(&self, ed: &mut Editor) {
        ed.show_line_numbers = self.editor.show_line_numbers;
        ed.relative_numbers = self.editor.relative_numbers;
        ed.hlsearch = self.editor.hlsearch;
        ed.splitbelow = self.editor.splitbelow;
        ed.splitright = self.editor.splitright;
        ed.modeline = self.editor.modeline;
        ed.modelines = self.editor.modelines;
        ed.autowrite = self.editor.autowrite;
        ed.autoread = self.editor.autoread;
        ed.errorbells = self.editor.errorbells;
        ed.visualbell = self.editor.visualbell;
        ed.clipboard_unnamed = self.editor.clipboard_unnamed;
    }

    /// `:e <file>` — edit a file, switching to it if already open.
    fn edit_file(&mut self, path: &str, line: Option<usize>) {
        if self.editor.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path) {
            if let Some(n) = line {
                self.editor.goto_line(n);
            }
            self.notify_fmt("already editing \"{path}\"", &[("{path}", path)]);
            return;
        }
        self.autowrite_current();
        if let Some(i) = self
            .others
            .iter()
            .position(|e| e.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path))
        {
            self.remember_alternate();
            std::mem::swap(&mut self.editor, &mut self.others[i]);
            self.apply_modelines_if_needed();
            if let Some(n) = line {
                self.editor.goto_line(n);
            }
            self.notify_fmt("\"{path}\" (buffer switched)", &[("{path}", path)]);
            return;
        }
        match Editor::from_file(path) {
            Ok(mut ed) => {
                self.inherit_prefs(&mut ed);
                ed.buf_id = self.fresh_buf_id(); // newly opened file is a distinct buffer
                let lines = ed.buffer.line_count();
                self.remember_alternate();
                let old = std::mem::replace(&mut self.editor, ed);
                if old.buffer.path().is_some() || old.buffer.is_dirty() {
                    self.others.push(old);
                }
                if let Some(n) = line {
                    self.editor.goto_line(n);
                }
                let lines = lines.to_string();
                self.notify_fmt("\"{path}\" {lines} lines", &[("{path}", path), ("{lines}", &lines)]);
                self.apply_modelines();
            }
            Err(e) => self.editor.message = format!("E212: Can't open \"{path}\": {e}"),
        }
    }

    /// `:e` / `:e!` with no argument — reload the current file from disk. Without
    /// `force`, refuses when there are unsaved changes. The cursor row is kept
    /// where possible.
    fn reload_file(&mut self, force: bool) {
        let Some(path) = self.editor.buffer.path().map(|p| p.display().to_string()) else {
            self.notify("E32: No file name");
            return;
        };
        if self.editor.buffer.is_dirty() && !force {
            self.editor.message =
                "E37: No write since last change (add ! to override)".into();
            return;
        }
        let row = self.editor.cursor.row;
        match Editor::from_file(&path) {
            Ok(mut ed) => {
                self.inherit_prefs(&mut ed);
                ed.cursor.row = row.min(ed.buffer.line_count().saturating_sub(1));
                let lines = ed.buffer.line_count().to_string();
                self.editor = ed;
                self.notify_fmt(
                    "\"{path}\" {lines} lines --reloaded--",
                    &[("{path}", &path), ("{lines}", &lines)],
                );
                self.apply_modelines();
            }
            Err(e) => self.editor.message = format!("E212: Can't open \"{path}\": {e}"),
        }
    }

    /// `:bn` — rotate to the next buffer.
    /// `:bufdo {cmd}` — run `cmd` on every open buffer, returning to the starting
    /// one. Each buffer is briefly made current; the rotation ends where it began.
    fn buf_do(&mut self, cmd: &str) {
        if cmd.trim().is_empty() {
            return;
        }
        let count = self.others.len() + 1;
        for _ in 0..count {
            if self.quit {
                break;
            }
            self.run_ex(cmd);
            if self.others.is_empty() {
                break; // single buffer: ran once, nothing to rotate
            }
            self.buffer_next();
        }
        let count = count.to_string();
        self.notify_fmt("{count} buffer(s) done", &[("{count}", &count)]);
    }

    /// `:windo {cmd}` — run `cmd` in every window, returning focus to the window
    /// it started in.
    fn win_do(&mut self, cmd: &str) {
        if cmd.trim().is_empty() {
            return;
        }
        let original = self.focused;
        for id in self.layout.ids() {
            if self.quit {
                break;
            }
            self.activate_window(id);
            self.run_ex(cmd);
        }
        self.activate_window(original);
    }

    /// `:mksession[!] [file]` — write a session file of ex-commands that reopen the
    /// current buffers, restore the theme and land on the current cursor line.
    /// Replay it with `:source <file>`. Window splits and options are not saved.
    fn make_session(&mut self, file: Option<&str>, force: bool) {
        let path = file.unwrap_or("Session.vim");
        if !force && std::path::Path::new(path).exists() {
            self.editor.message = format!("E189: \"{path}\" exists (add ! to override)");
            return;
        }
        // Background buffers first, then the focused one last so it is current on
        // restore; skip unnamed/scratch buffers and de-duplicate shared paths.
        let current = self.editor.buffer.path().map(|p| p.display().to_string());
        let mut files: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ed in &self.others {
            if let Some(p) = ed.buffer.path().map(|p| p.display().to_string()) {
                if Some(&p) != current.as_ref() && seen.insert(p.clone()) {
                    files.push(p);
                }
            }
        }
        if current.is_none() && files.is_empty() {
            self.editor.message = "E: no named buffers to save".into();
            return;
        }
        let mut out = String::from("\" rvim session file — restore with :source\n");
        out.push_str(&format!("theme {}\n", self.themes.current().name));
        for f in &files {
            out.push_str(&format!("edit {f}\n"));
        }
        if let Some(cur) = &current {
            out.push_str(&format!("edit {cur}\n"));
        }
        // Options apply to the now-focused buffer, so set them after the edits.
        for opt in self.editor.session_options() {
            out.push_str(&format!("set {opt}\n"));
        }
        if let Some(_cur) = &current {
            out.push_str(&format!("{}\n", self.editor.cursor.row + 1));
        }
        match std::fs::write(path, out) {
            Ok(()) => self.editor.message = format!("session written to \"{path}\""),
            Err(e) => self.editor.message = format!("E: can't write {path}: {e}"),
        }
    }

    /// `:TOhtml [file]` — export the current buffer to a syntax-highlighted HTML
    /// file using the active theme. The default name is the file's own name with
    /// `.html` appended (or `export.html` for an unnamed buffer).
    fn export_html(&mut self, file: Option<&str>) {
        let title = self
            .editor
            .buffer
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "rvim".to_string());
        let path = match file {
            Some(f) => f.to_string(),
            None => match self.editor.buffer.path() {
                Some(p) => format!("{}.html", p.display()),
                None => "export.html".to_string(),
            },
        };
        let html = crate::export::to_html(
            &title,
            self.editor.buffer.lines(),
            self.editor.language,
            self.themes.current(),
            &self.syntax,
        );
        match std::fs::write(&path, html) {
            Ok(()) => self.editor.message = format!("HTML written to \"{path}\""),
            Err(e) => self.editor.message = format!("E: can't write {path}: {e}"),
        }
    }

    fn buffer_next(&mut self) {
        if self.others.is_empty() {
            self.notify("only one buffer");
            return;
        }
        self.autowrite_current();
        self.remember_alternate();
        let next = self.others.remove(0);
        let old = std::mem::replace(&mut self.editor, next);
        self.others.push(old);
        self.apply_modelines_if_needed();
        self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
    }

    /// `:bp` — rotate to the previous buffer.
    fn buffer_prev(&mut self) {
        if let Some(prev) = self.others.pop() {
            self.autowrite_current();
            self.remember_alternate();
            let old = std::mem::replace(&mut self.editor, prev);
            self.others.insert(0, old);
            self.apply_modelines_if_needed();
            self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
        } else {
            self.notify("only one buffer");
        }
    }

    /// `:b <n>` — switch to the nth buffer (1 = active, 2.. = others).
    fn buffer_goto(&mut self, n: usize) {
        if n == 1 {
            return;
        }
        let idx = n.wrapping_sub(2);
        if idx < self.others.len() {
            self.autowrite_current();
            self.remember_alternate();
            std::mem::swap(&mut self.editor, &mut self.others[idx]);
            self.apply_modelines_if_needed();
            self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
        } else {
            let n = n.to_string();
            self.notify_fmt("E86: Buffer {n} does not exist", &[("{n}", &n)]);
        }
    }

    /// `:bd` — close the current buffer (blocked if unsaved / last buffer).
    fn buffer_delete(&mut self) {
        if self.editor.buffer.is_dirty() {
            self.editor.message =
                "E89: No write since last change (add ! to override)".into();
            return;
        }
        if self.others.is_empty() {
            self.editor.message = "E90: cannot close last buffer".into();
            return;
        }
        self.editor = self.others.remove(0);
        let name = Self::buffer_name(&self.editor);
        self.notify_fmt("buffer closed; now \"{name}\"", &[("{name}", &name)]);
    }

    /// Build the tab-bar entries (active first, then the others in order).
    fn tab_entries(&self) -> Vec<ui::TabEntry> {
        let mut tabs = vec![ui::TabEntry {
            name: Self::buffer_name(&self.editor),
            active: true,
            dirty: self.editor.buffer.is_dirty(),
        }];
        for ed in &self.others {
            tabs.push(ui::TabEntry {
                name: Self::buffer_name(ed),
                active: false,
                dirty: ed.buffer.is_dirty(),
            });
        }
        tabs
    }

    /// `:ls` — a one-line listing of open buffers (active marked `%`).
    fn buffer_list(&mut self) {
        let mut parts = vec![format!("1 %{}", Self::buffer_name(&self.editor))];
        for (i, ed) in self.others.iter().enumerate() {
            let dirty = if ed.buffer.is_dirty() { "+" } else { "" };
            parts.push(format!("{} {}{}", i + 2, Self::buffer_name(ed), dirty));
        }
        self.editor.message = parts.join("  |  ");
    }

    /// The block-comment fold state feeding the first visible line, memoized by
    /// (buffer revision, language, top row) so an unchanged view is O(1).
    fn block_state_top(&mut self) -> LineState {
        let rev = self.editor.buffer.revision();
        let lang = self.editor.language;
        let top = self.editor.top;
        match self.block_memo {
            Some((r, l, t, v)) if r == rev && l == lang && t == top => v,
            _ => {
                let v = self.syntax.block_state_at(lang, self.editor.buffer.lines(), top);
                self.block_memo = Some((rev, lang, top, v));
                v
            }
        }
    }

    // ---- window management ----------------------------------------------

    /// Number of on-screen windows.
    fn window_count(&self) -> usize {
        self.layout.count()
    }

    /// The text-area rectangle the window layout tiles: everything below the
    /// optional tab bar, above the command line.
    fn window_area(&self, cols: u16, rows: u16, show_tabline: bool) -> window::Rect {
        let top = if show_tabline { 1 } else { 0 };
        window::Rect::new(0, top, cols, rows.saturating_sub(top).saturating_sub(1))
    }

    /// Give each visible window the viewport size of its rectangle (reserving its
    /// status-line row and gutter), so scrolling tracks the cursor per window.
    fn size_windows(&mut self, area: window::Rect) {
        let rects = self.layout.rects(area);
        for (id, rect) in rects {
            let ed = if id == self.focused {
                &mut self.editor
            } else if let Some(p) = self.panes.iter_mut().find(|p| p.id == id) {
                &mut p.editor
            } else {
                continue;
            };
            let gw = ui::gutter_width(ed.buffer.line_count(), ed.show_line_numbers)
                + ed.foldcolumn as u16;
            let text_rows = rect.h.saturating_sub(1).max(1) as usize;
            let text_cols = rect.w.saturating_sub(gw).max(1) as usize;
            ed.set_viewport(text_rows, text_cols);
        }
    }

    /// Move focus to `target` (a pane id) by swapping its editor with the focused
    /// one. No-op if `target` is already focused or unknown.
    fn activate_window(&mut self, target: WinId) {
        if target == self.focused {
            return;
        }
        if let Some(pos) = self.panes.iter().position(|p| p.id == target) {
            let pane = self.panes.remove(pos);
            let old = std::mem::replace(&mut self.editor, pane.editor);
            self.panes.push(WinPane { id: self.focused, editor: old });
            self.focused = target;
            // The newly focused window may share a buffer with another; make sure
            // it holds the authoritative copy so edits land on the current text.
            self.normalize_focus_buffer();
        }
    }

    /// `:split` / `:vsplit` (and `Ctrl-w s` / `Ctrl-w v`). The new window takes
    /// focus and shows `file`, or a duplicate view of the current buffer.
    /// Set the status message, localized to the current UI language.
    fn notify(&mut self, msg: &'static str) {
        self.editor.message = crate::i18n::tr(self.editor.locale, msg).into();
    }

    /// Set a localized status message from a template with `{name}`-style
    /// placeholders. The English template is the translation key; after translating
    /// (or falling back to English), each placeholder is replaced with its value.
    fn notify_fmt(&mut self, template: &'static str, args: &[(&str, &str)]) {
        let mut s = crate::i18n::tr(self.editor.locale, template).to_string();
        for (key, val) in args {
            s = s.replace(key, val);
        }
        self.editor.message = s;
    }

    /// Allocate a fresh buffer id for a newly opened file.
    fn fresh_buf_id(&mut self) -> BufferId {
        let id = BufferId(self.next_buf);
        self.next_buf += 1;
        id
    }

    fn split_window(&mut self, vertical: bool, file: Option<&str>) {
        let new_id = WinId(self.next_win);
        let kind = if vertical {
            SplitKind::Vertical
        } else {
            SplitKind::Horizontal
        };
        // `splitbelow`/`splitright` put the new window after (below/right); the
        // default is before (above/left).
        let before = if vertical {
            !self.editor.splitright
        } else {
            !self.editor.splitbelow
        };
        if !self.layout.split_dir(self.focused, kind, new_id, before) {
            return;
        }
        let new_editor = match file {
            Some(path) => match Editor::from_file(path) {
                Ok(mut ed) => {
                    self.inherit_prefs(&mut ed);
                    ed.buf_id = self.fresh_buf_id(); // a different file is a distinct buffer
                    ed
                }
                Err(e) => {
                    self.layout.close(new_id); // undo the split
                    self.editor.message = format!("E212: Can't open \"{path}\": {e}");
                    return;
                }
            },
            None => self.editor.duplicate_view(),
        };
        self.next_win += 1;
        let old = std::mem::replace(&mut self.editor, new_editor);
        self.panes.push(WinPane {
            id: self.focused,
            editor: old,
        });
        self.focused = new_id;
    }

    /// `:close` / `Ctrl-w c` / `Ctrl-w q` — close the focused window. Refuses the
    /// last window, or an unsaved one without `force`. Returns whether it closed.
    fn close_window(&mut self, force: bool) -> bool {
        if self.window_count() == 1 {
            self.notify("E444: Cannot close last window");
            return false;
        }
        if self.editor.buffer.is_dirty() && !force {
            self.notify("E37: No write since last change (add ! to override)");
            return false;
        }
        let next = self.layout.prev(self.focused).unwrap_or(self.focused);
        self.layout.close(self.focused);
        if let Some(pos) = self.panes.iter().position(|p| p.id == next) {
            let pane = self.panes.remove(pos);
            let old = std::mem::replace(&mut self.editor, pane.editor);
            // If the closed window shared its buffer with the one now focused, keep
            // the more-recently-edited content rather than the surviving view's
            // possibly stale copy.
            if self.editor.buf_id == old.buf_id
                && old.buffer.revision() > self.editor.buffer.revision()
            {
                self.editor.buffer = old.buffer;
            }
            self.focused = next;
        }
        true
    }

    /// `:only` / `Ctrl-w o` — close every window but the focused one.
    fn only_window(&mut self, force: bool) {
        if self.window_count() == 1 {
            return;
        }
        if !force && self.panes.iter().any(|p| p.editor.buffer.is_dirty()) {
            self.editor.message =
                "E37: another window has unsaved changes (add ! to override)".into();
            return;
        }
        self.layout.only(self.focused);
        self.panes.clear();
    }

    /// `Ctrl-w w` / `Ctrl-w W` — cycle focus through the windows.
    fn focus_cycle(&mut self, forward: bool) {
        let target = if forward {
            self.layout.next(self.focused)
        } else {
            self.layout.prev(self.focused)
        };
        if let Some(t) = target {
            self.activate_window(t);
        }
    }

    /// `Ctrl-w x` — exchange the focused window's content with the next window's.
    /// Focus stays put; the two windows swap what they show.
    fn window_exchange(&mut self) {
        let Some(next) = self.layout.next(self.focused) else {
            return;
        };
        if next == self.focused {
            return;
        }
        if let Some(p) = self.panes.iter_mut().find(|p| p.id == next) {
            std::mem::swap(&mut self.editor, &mut p.editor);
        }
        // Keep the authoritative copy with the focused window after the exchange.
        self.normalize_focus_buffer();
    }

    /// `Ctrl-w r` — rotate every window's content one slot along the layout order
    /// (the last window's content wraps to the first). Focus stays on its window.
    fn window_rotate(&mut self) {
        let ids = self.layout.ids();
        if ids.len() < 2 {
            return;
        }
        // Pull each window's editor out in layout order.
        let mut eds: Vec<Editor> = Vec::with_capacity(ids.len());
        for &id in &ids {
            if id == self.focused {
                eds.push(std::mem::take(&mut self.editor));
            } else if let Some(pos) = self.panes.iter().position(|p| p.id == id) {
                eds.push(self.panes.remove(pos).editor);
            }
        }
        eds.rotate_right(1);
        // Reassign in layout order.
        self.panes.clear();
        for (&id, ed) in ids.iter().zip(eds) {
            if id == self.focused {
                self.editor = ed;
            } else {
                self.panes.push(WinPane { id, editor: ed });
            }
        }
        // Rotation can move the authoritative copy of a shared buffer off the
        // focused window; pull it back so edits still land on the current text.
        self.normalize_focus_buffer();
    }

    /// `Ctrl-w +`/`-` (height) and `Ctrl-w >`/`<` (width) — resize the focused
    /// window by one step along the given axis.
    fn window_resize(&mut self, kind: SplitKind, delta: i16) {
        self.layout.resize(self.focused, kind, delta);
    }

    /// `Ctrl-w =` — make all windows equal size.
    fn window_equalize(&mut self) {
        self.layout.equalize();
    }

    /// `Ctrl-w h/j/k/l` — move focus to the adjacent window in `area`.
    fn focus_dir(&mut self, dir: Move, area: window::Rect) {
        if let Some(t) = self.layout.neighbor(self.focused, dir, area) {
            self.activate_window(t);
        }
    }

    /// Dispatch the second key of a `Ctrl-w` window command.
    fn handle_window_key(&mut self, key: crossterm::event::KeyEvent, area: window::Rect) {
        self.editor.message.clear();
        match key.code {
            KeyCode::Char('s') => self.split_window(false, None),
            KeyCode::Char('v') => self.split_window(true, None),
            KeyCode::Char('w') => self.focus_cycle(true),
            KeyCode::Char('W') => self.focus_cycle(false),
            KeyCode::Char('h') | KeyCode::Left => self.focus_dir(Move::Left, area),
            KeyCode::Char('j') | KeyCode::Down => self.focus_dir(Move::Down, area),
            KeyCode::Char('k') | KeyCode::Up => self.focus_dir(Move::Up, area),
            KeyCode::Char('l') | KeyCode::Right => self.focus_dir(Move::Right, area),
            KeyCode::Char('c') | KeyCode::Char('q') => {
                self.close_window(false);
            }
            KeyCode::Char('o') => self.only_window(false),
            KeyCode::Char('x') => self.window_exchange(),
            KeyCode::Char('r') => self.window_rotate(),
            KeyCode::Char('+') => self.window_resize(SplitKind::Horizontal, 1),
            KeyCode::Char('-') => self.window_resize(SplitKind::Horizontal, -1),
            KeyCode::Char('>') => self.window_resize(SplitKind::Vertical, 1),
            KeyCode::Char('<') => self.window_resize(SplitKind::Vertical, -1),
            KeyCode::Char('=') => self.window_equalize(),
            _ => {}
        }
    }

    /// The per-window views for the renderer (focused window first is not
    /// required; order follows the layout). Computes each window's syntax state.
    /// The editor shown in window `id` (falls back to the focused editor).
    fn editor_ref(&self, id: WinId) -> &Editor {
        if id == self.focused {
            &self.editor
        } else {
            self.panes
                .iter()
                .find(|p| p.id == id)
                .map(|p| &p.editor)
                .unwrap_or(&self.editor)
        }
    }

    /// The single authoritative buffer for `bid`. Windows that share a `BufferId`
    /// (e.g. after a plain `:split`) are views of one buffer, so they all resolve
    /// here. Only one view is edited at a time (the focused one), so the edited
    /// copy is the one with the highest `revision()`; picking it keeps every shared
    /// view showing the same, current text (#25), no matter how windows were moved.
    fn authoritative_buffer(&self, bid: BufferId) -> &Buffer {
        let mut best = &self.editor.buffer;
        let mut best_key = (self.editor.buf_id == bid, self.editor.buffer.revision());
        for p in &self.panes {
            let key = (p.editor.buf_id == bid, p.editor.buffer.revision());
            // Prefer windows that actually show `bid`, then the highest revision.
            if key > best_key {
                best = &p.editor.buffer;
                best_key = key;
            }
        }
        best
    }

    /// The buffer that window `id` displays (resolved through shared identity).
    fn buffer_for(&self, id: WinId) -> &Buffer {
        self.authoritative_buffer(self.editor_ref(id).buf_id)
    }

    /// Ensure the focused editor holds the authoritative (most-recently edited)
    /// copy of its buffer, so edits land on the current text even after windows
    /// were reordered. Call after any op that moves which editor is focused.
    fn normalize_focus_buffer(&mut self) {
        let bid = self.editor.buf_id;
        let focus_rev = self.editor.buffer.revision();
        if let Some(p) = self
            .panes
            .iter_mut()
            .filter(|p| p.editor.buf_id == bid)
            .max_by_key(|p| p.editor.buffer.revision())
        {
            if p.editor.buffer.revision() > focus_rev {
                std::mem::swap(&mut self.editor.buffer, &mut p.editor.buffer);
            }
        }
    }

    /// Refresh each window's first-visible-line syntax state into the memo,
    /// recomputing only when its (revision, language, top) changed — the costly
    /// part is re-folding from the top of the buffer for multiline languages.
    /// Kept separate from [`window_views`](App::window_views) so the latter can
    /// stay a shared borrow alongside the theme and syntax registry at render.
    fn refresh_window_syntax(&mut self, area: window::Rect) {
        let rects = self.layout.rects(area);
        for (id, _) in &rects {
            let (rev, lang, top) = {
                let ed = self.editor_ref(*id);
                (self.buffer_for(*id).revision(), ed.language, ed.top)
            };
            let fresh = matches!(
                self.win_block_memo.get(id),
                Some((r, l, t, _)) if *r == rev && *l == lang && *t == top
            );
            if !fresh {
                let state =
                    self.syntax
                        .block_state_at(lang, self.buffer_for(*id).lines(), top);
                self.win_block_memo.insert(*id, (rev, lang, top, state));
            }
        }
        // Drop memo entries for windows that no longer exist.
        self.win_block_memo
            .retain(|id, _| rects.iter().any(|(rid, _)| rid == id));
    }

    fn window_views(&self, area: window::Rect) -> Vec<ui::WindowView<'_>> {
        self.layout
            .rects(area)
            .into_iter()
            .map(|(id, rect)| {
                let editor = self.editor_ref(id);
                let buffer = self.buffer_for(id);
                // Use the memoized state when present; otherwise compute it (so the
                // view is correct even if called without a prior refresh).
                let top_state = match self.win_block_memo.get(&id) {
                    Some((r, l, t, s))
                        if *r == buffer.revision()
                            && *l == editor.language
                            && *t == editor.top =>
                    {
                        *s
                    }
                    _ => self
                        .syntax
                        .block_state_at(editor.language, buffer.lines(), editor.top),
                };
                ui::WindowView {
                    editor,
                    buffer,
                    rect,
                    top_state,
                    focused: id == self.focused,
                }
            })
            .collect()
    }

    /// Run the interactive event loop until the user quits.
    pub fn run(&mut self) -> io::Result<()> {
        let mut guard = TerminalGuard::enter()?;
        let mut mouse_on = false;
        let mut out = BufWriter::new(io::stdout());

        loop {
            // `:set autoread` — pick up external changes to the current file.
            self.check_autoread();

            // Reconcile mouse capture with the desired state.
            if self.want_mouse != mouse_on {
                if self.want_mouse {
                    guard.enable_mouse()?;
                } else {
                    guard.disable_mouse()?;
                }
                mouse_on = self.want_mouse;
            }

            // Update viewport so scrolling tracks the cursor.
            let tabs = self.tab_entries();
            let (cols, rows) = TerminalGuard::size()?;
            let layout = Layout::compute(
                cols,
                rows,
                self.editor.buffer.line_count(),
                self.editor.show_line_numbers,
                tabs.len() > 1,
            )
            .with_foldcolumn(self.editor.foldcolumn as u16);
            let split = !self.panes.is_empty();
            let area = self.window_area(cols, rows, tabs.len() > 1);
            // Keep syntax folds current with edits (no-op unless fdm=syntax and the
            // buffer changed); other fold methods refresh inside set_viewport.
            self.editor.recompute_syntax_folds(&self.syntax);

            // `:set title` — update the terminal window title when it changes.
            if self.editor.title {
                let title = self.editor.title_string();
                if title != self.last_title {
                    let _ = crossterm::execute!(out, crossterm::terminal::SetTitle(&title));
                    self.last_title = title;
                }
            }

            // Capture the current status message for `:messages` (once per change).
            self.log_message();

            if split {
                // Multiple windows: size each to its rectangle and draw them all.
                self.size_windows(area);
                self.refresh_window_syntax(area);
                let views = self.window_views(area);
                ui::render_split(&mut out, &views, self.themes.current(), &self.syntax, &tabs)?;
            } else {
                self.editor
                    .set_viewport(layout.text_rows as usize, layout.text_cols as usize);
                // Block-comment state feeding the first visible line, memoized so an
                // unchanged view doesn't re-fold from the top of the buffer each frame.
                let in_block_top = self.block_state_top();
                ui::render(
                    &mut out,
                    &self.editor,
                    &self.editor.buffer,
                    self.themes.current(),
                    &self.syntax,
                    &tabs,
                    in_block_top,
                )?;
            }

            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    // The second key of a `Ctrl-w` window command.
                    if self.pending_ctrl_w {
                        self.pending_ctrl_w = false;
                        self.handle_window_key(key, area);
                        continue;
                    }
                    // `Ctrl-w` starts a window command (an app-level concern).
                    if self.editor.mode == Mode::Normal
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('w')
                    {
                        self.pending_ctrl_w = true;
                        continue;
                    }
                    // Alt+<key> (or F10) opens the menu bar from Normal mode.
                    if !self.editor.is_menu_open()
                        && self.editor.mode == Mode::Normal
                        && (key.modifiers.contains(KeyModifiers::ALT)
                            || key.code == KeyCode::F(10))
                    {
                        let menus = crate::menu::build_menus(
                            &self.themes.names(),
                            &crate::syntax::language_names(),
                            &self.plugins.all_commands(),
                            self.editor.locale,
                        );
                        self.editor.open_menu(menus);
                        if let KeyCode::Char(c) = key.code {
                            self.editor.menu_open_initial(c);
                        }
                        continue;
                    }
                    // Ctrl-^ (often reported as Ctrl-6) switches to the alternate
                    // buffer — an app-level concern, so handle it before the editor.
                    if self.editor.mode == Mode::Normal
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('^') | KeyCode::Char('6'))
                    {
                        self.editor.message.clear();
                        self.switch_alternate();
                        continue;
                    }
                    // Fresh action clears the previous message.
                    if self.editor.mode != Mode::Command {
                        self.editor.message.clear();
                    }
                    let msg_before = self.editor.message.clone();
                    let action = self.editor.handle_key(key);
                    self.apply_editor_action(action);
                    // Error cue: on a fresh error message (vim-coded `E<n>:`), flash
                    // the screen (`:set visualbell`) or ring the terminal bell
                    // (`:set errorbells`). Visualbell takes precedence so it works on
                    // its own as a silent, visible cue.
                    if self.editor.message != msg_before
                        && ui::is_error_message(&self.editor.message)
                    {
                        if self.editor.visualbell {
                            // DECSCNM reverse-video flash — the classic terminal
                            // visual bell, restored after a brief pause.
                            let _ = out.write_all(b"\x1b[?5h");
                            let _ = out.flush();
                            std::thread::sleep(std::time::Duration::from_millis(VISUAL_BELL_MS));
                            let _ = out.write_all(b"\x1b[?5l");
                            let _ = out.flush();
                        } else if self.editor.errorbells {
                            let _ = out.write_all(b"\x07");
                            let _ = out.flush();
                        }
                    }
                }
                Event::Mouse(m) => self.handle_mouse(m, &layout),
                Event::Resize(_, _) => { /* redrawn next loop */ }
                _ => {}
            }

            if self.quit {
                break;
            }
        }
        Ok(())
    }

    /// Advance the consecutive-click counter for a left press at `(m.column,
    /// m.row)`: returns 1 for a single click, 2 for a double, 3 for a triple
    /// (same cell within 400ms), then resets.
    fn click_count(&mut self, m: &event::MouseEvent) -> u8 {
        let now = std::time::Instant::now();
        let count = match self.last_click {
            Some((t, c, r, n))
                if c == m.column
                    && r == m.row
                    && n < 3
                    && now.duration_since(t) < std::time::Duration::from_millis(400) =>
            {
                n + 1
            }
            _ => 1,
        };
        self.last_click = Some((now, m.column, m.row, count));
        count
    }

    /// Apply word/line selection for a double/triple click on the focused editor.
    fn apply_multi_click(&mut self, count: u8) {
        match count {
            2 => self.editor.mouse_select_word(),
            3 => self.editor.mouse_select_line(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, m: event::MouseEvent, layout: &Layout) {
        // While the menu is open, clicks drive the menu bar.
        if self.editor.is_menu_open() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                if m.row == 0 {
                    let titles: Vec<String> = self
                        .editor
                        .menu()
                        .map(|mn| mn.menus.iter().map(|x| x.title.clone()).collect())
                        .unwrap_or_default();
                    let positions = ui::menu_bar_positions(&titles);
                    for (i, title) in titles.iter().enumerate() {
                        let start = positions[i];
                        let end = start + title.chars().count() as u16 + 2;
                        if m.column >= start && m.column < end {
                            self.editor.menu_click_top(i);
                            break;
                        }
                    }
                } else {
                    // Hit-test the open dropdowns; click an item to act on it,
                    // click empty space to dismiss.
                    let hit = self.editor.menu().and_then(|menu| {
                        let geom = ui::menu_geometry(menu, layout.cols, layout.rows);
                        ui::menu_hit_test(&geom, m.column, m.row)
                    });
                    match hit {
                        Some((level, idx)) => self.editor.menu_mouse_select(level, idx),
                        None => self.editor.close_menu(),
                    }
                }
            }
            return;
        }
        // With split windows, route the event to the pane under the pointer.
        if !self.panes.is_empty() {
            self.handle_mouse_split(m, layout);
            return;
        }
        // Screen cell -> buffer position, if the click is in the text area.
        // Capture scroll offsets as locals so the closure doesn't borrow `self`.
        let (view_top, view_left) = (self.editor.top, self.editor.left);
        let text_pos = |m: &event::MouseEvent| -> Option<(usize, usize)> {
            if m.row >= layout.top_offset && m.row < layout.top_offset + layout.text_rows {
                let row = view_top + (m.row - layout.top_offset) as usize;
                let col =
                    view_left.saturating_add((m.column.saturating_sub(layout.gutter_width)) as usize);
                Some((row, col))
            } else {
                None
            }
        };
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // A fresh click clears any selection and positions the cursor;
                // a double/triple click selects the word/line.
                let count = self.click_count(&m);
                self.editor.clear_visual();
                if let Some((row, col)) = text_pos(&m) {
                    self.editor.set_cursor_clamped(row, col);
                    self.apply_multi_click(count);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Dragging extends a character-wise selection from the click point.
                if let Some((row, col)) = text_pos(&m) {
                    self.editor.begin_mouse_visual();
                    self.editor.set_cursor_clamped(row, col);
                }
            }
            MouseEventKind::ScrollDown => {
                self.editor.top = (self.editor.top + 3)
                    .min(self.editor.buffer.line_count().saturating_sub(1));
            }
            MouseEventKind::ScrollUp => {
                self.editor.top = self.editor.top.saturating_sub(3);
            }
            _ => {}
        }
    }

    /// A mutable handle to the editor shown in window `id`.
    fn editor_mut(&mut self, id: WinId) -> Option<&mut Editor> {
        if id == self.focused {
            Some(&mut self.editor)
        } else {
            self.panes.iter_mut().find(|p| p.id == id).map(|p| &mut p.editor)
        }
    }

    /// Mouse handling when the screen is split: focus and position within the pane
    /// under the pointer, and scroll the hovered pane.
    fn handle_mouse_split(&mut self, m: event::MouseEvent, layout: &Layout) {
        let area = self.window_area(layout.cols, layout.rows, layout.top_offset > 0);
        let hit = self
            .layout
            .rects(area)
            .into_iter()
            .find(|(_, r)| {
                m.column >= r.x && m.column < r.x + r.w && m.row >= r.y && m.row < r.y + r.h
            });
        let Some((id, rect)) = hit else { return };
        // The last row of a pane is its status line, not text.
        let in_text = m.row < rect.y + rect.h.saturating_sub(1);
        // Buffer position of the click within `ed`'s rect.
        let pos = |ed: &Editor| -> (usize, usize) {
            let gw = ui::gutter_width(ed.buffer.line_count(), ed.show_line_numbers)
                + ed.foldcolumn as u16;
            let row = ed.top + (m.row - rect.y) as usize;
            let col = ed
                .left
                .saturating_add((m.column.saturating_sub(rect.x + gw)) as usize);
            (row, col)
        };
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let count = self.click_count(&m);
                self.activate_window(id);
                self.editor.clear_visual();
                if in_text {
                    let (row, col) = pos(&self.editor);
                    self.editor.set_cursor_clamped(row, col);
                    self.apply_multi_click(count);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Extend a selection only within the already-focused pane.
                if id == self.focused && in_text {
                    let (row, col) = pos(&self.editor);
                    self.editor.begin_mouse_visual();
                    self.editor.set_cursor_clamped(row, col);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(ed) = self.editor_mut(id) {
                    ed.top = (ed.top + 3).min(ed.buffer.line_count().saturating_sub(1));
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(ed) = self.editor_mut(id) {
                    ed.top = ed.top.saturating_sub(3);
                }
            }
            _ => {}
        }
    }

    /// Handle an [`Action`] returned by the editor's key handling.
    fn apply_editor_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::RunEx(cmd) => self.run_ex(&cmd),
            Action::OpenScratch { title, body } => {
                self.open_scratch(&body, &title, &title);
            }
            Action::SetGlobalMark(c) => self.set_global_mark(c),
            Action::JumpGlobalMark { mark, line_wise } => self.jump_global_mark(mark, line_wise),
        }
    }

    /// `m{A-Z}` — record a global mark at the cursor, tagged with the current file.
    fn set_global_mark(&mut self, mark: char) {
        match self.editor.buffer.path().map(|p| p.display().to_string()) {
            Some(file) => {
                self.global_marks
                    .insert(mark, (file, self.editor.cursor.row, self.editor.cursor.col));
            }
            None => self.editor.message = "E name: cannot set a global mark in an unnamed buffer".into(),
        }
    }

    /// `` `{A-Z} `` / `'{A-Z}` — jump to a global mark, opening its file if needed.
    fn jump_global_mark(&mut self, mark: char, line_wise: bool) {
        let Some((file, row, col)) = self.global_marks.get(&mark).cloned() else {
            self.editor.message = format!("E20: Mark not set: {mark}");
            return;
        };
        let here = self.editor.buffer.path().map(|p| p.display().to_string());
        if here.as_deref() != Some(file.as_str()) {
            self.edit_file(&file, None);
        }
        if line_wise {
            self.editor.goto_line(row + 1);
        } else {
            self.editor.set_cursor_clamped(row, col);
        }
    }

    /// Execute an ex-command. `:set` with several options is split and each is run
    /// in turn; splitting is escape-aware so a value may contain spaces, e.g.
    /// `:set equalprg=clang-format\ --style=Google`.
    pub fn run_ex(&mut self, input: &str) {
        // Expand any leading user-defined command first.
        let resolved = self.resolve_user_command(input);
        let input = resolved.as_str();
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("set ").or_else(|| trimmed.strip_prefix("se ")) {
            let opts = split_set_options(rest);
            if !opts.is_empty() {
                for opt in opts {
                    self.exec_ex(&format!("set {opt}"));
                }
                return;
            }
        }
        self.exec_ex(input);
    }

    /// Execute a single parsed ex-command (no `:set` option splitting).
    fn exec_ex(&mut self, input: &str) {
        match command::parse(input) {
            ExCommand::Empty => {}
            ExCommand::Write { path, force } => {
                self.do_write(path, force);
            }
            ExCommand::Quit { force } => {
                // With splits, `:q` closes the focused window (vim). Only the last
                // window quits the app.
                if self.window_count() > 1 {
                    self.close_window(force);
                } else {
                    let dirty = self.editor.buffer.is_dirty()
                        || self.others.iter().any(|e| e.buffer.is_dirty());
                    if dirty && !force {
                        self.editor.message =
                            "E37: No write since last change (add ! to override)".into();
                    } else {
                        self.quit = true;
                    }
                }
            }
            ExCommand::WriteQuit { path, force } => {
                if self.do_write(path, force) {
                    self.quit = true;
                }
            }
            ExCommand::QuitAll { force } => {
                let dirty = self.editor.buffer.is_dirty()
                    || self.others.iter().any(|e| e.buffer.is_dirty());
                if dirty && !force {
                    self.editor.message =
                        "E37: No write since last change (add ! to override)".into();
                } else {
                    self.quit = true;
                }
            }
            ExCommand::WriteAll => {
                let (written, failed) = self.write_all();
                self.editor.message = if failed == 0 {
                    format!("{written} buffer(s) written")
                } else {
                    format!("{written} written, {failed} failed (no file name?)")
                };
            }
            ExCommand::WriteQuitAll { force } => {
                let (_, failed) = self.write_all();
                if failed == 0 || force {
                    self.quit = true;
                } else {
                    self.editor.message =
                        format!("{failed} buffer(s) could not be written (add ! to override)");
                }
            }
            ExCommand::WriteRange { range, file } => self.write_range(range, file),
            ExCommand::WriteAppend { range, file } => self.write_append(range, &file),
            ExCommand::WriteFilter { range, cmd } => {
                if cmd.is_empty() {
                    self.notify("E471: Argument required");
                } else if let Some(out) = self.editor.pipe_to_command(range, &cmd) {
                    // Normalize line endings and drop the trailing blank line.
                    let mut lines: Vec<&str> =
                        out.split('\n').map(|l| l.trim_end_matches('\r')).collect();
                    while lines.last() == Some(&"") {
                        lines.pop();
                    }
                    let text = lines.join("\n");
                    if text.is_empty() {
                        self.editor.message = format!("ran: !{cmd}");
                    } else if lines.len() > 1 {
                        self.open_scratch(&text, "!output —", &format!("!{cmd} — :bd to close"));
                    } else {
                        self.editor.message = text;
                    }
                } else {
                    self.editor.message = format!("E485: Can't run: {cmd}");
                }
            }
            ExCommand::Edit { path, line } => self.edit_file(&path, line),
            ExCommand::Reload { force } => self.reload_file(force),
            ExCommand::ReadFile { addr, path } => {
                // `:[addr]r` inserts at the resolved index; bare `:r` below cursor.
                let at = addr.map(|a| self.editor.insert_index_for(a));
                if let Some(cmd) = path.strip_prefix('!') {
                    match at {
                        Some(i) => self.editor.read_command_at(cmd, i),
                        None => self.editor.read_command(cmd),
                    }
                } else {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => match at {
                            Some(i) => self.editor.read_lines_at(&text, i),
                            None => self.editor.read_lines_below(&text),
                        },
                        Err(e) => {
                            self.editor.message = format!("E484: can't open \"{path}\": {e}")
                        }
                    }
                }
            }
            ExCommand::SplitWindow { vertical, file } => {
                self.split_window(vertical, file.as_deref());
            }
            ExCommand::Enew => self.edit_new_buffer(),
            ExCommand::CloseWindow { force } => {
                self.close_window(force);
            }
            ExCommand::OnlyWindow { force } => self.only_window(force),
            ExCommand::BufferList => self.buffer_list(),
            ExCommand::BufDo(cmd) => self.buf_do(&cmd),
            ExCommand::WinDo(cmd) => self.win_do(&cmd),
            ExCommand::BufferNext => self.buffer_next(),
            ExCommand::BufferPrev => self.buffer_prev(),
            ExCommand::Buffer(n) => self.buffer_goto(n),
            ExCommand::BufferDelete => self.buffer_delete(),
            ExCommand::BufferAlternate => self.switch_alternate(),
            ExCommand::Marks => {
                let mut text = self.editor.marks_listing();
                if !self.global_marks.is_empty() {
                    let mut names: Vec<&char> = self.global_marks.keys().collect();
                    names.sort();
                    text.push_str("\nglobal marks\n");
                    for name in names {
                        let (file, row, _) = &self.global_marks[name];
                        text.push_str(&format!("  {}  {}:{}\n", name, file, row + 1));
                    }
                }
                self.open_scratch(&text, "marks —", "marks — :bd to close");
            }
            ExCommand::Registers => {
                let text = self.editor.registers_listing();
                self.open_scratch(&text, "registers —", "registers — :bd to close");
            }
            ExCommand::Jumps => {
                let text = self.editor.jumps_listing();
                self.open_scratch(&text, "jumps —", "jumps — :bd to close");
            }
            ExCommand::Messages => {
                let text = self.messages_listing();
                self.open_scratch(&text, "messages —", "messages — :bd to close");
            }
            ExCommand::Tag(name) => self.tag_jump(name.as_deref().unwrap_or("")),
            ExCommand::TagPop => self.tag_pop(),
            ExCommand::TagsList => {
                let mut body = String::from("tag stack — :bd to close\n\n");
                if self.tag_stack.is_empty() {
                    body.push_str("  (empty)\n");
                } else {
                    for (i, (file, row, _)) in self.tag_stack.iter().enumerate() {
                        body.push_str(&format!("  {}  {}:{}\n", i + 1, file, row + 1));
                    }
                }
                self.open_scratch(&body, "tags —", "tag stack — :bd to close");
            }
            ExCommand::TagMatch(mv) => self.tag_match_move(mv),
            ExCommand::TagSelect(name) => self.tag_select(name.as_deref()),
            ExCommand::Changes => {
                let text = self.editor.changes_listing();
                self.open_scratch(&text, "changes —", "changes — :bd to close");
            }
            ExCommand::DelMarks(spec) => self.editor.delete_marks(&spec),
            ExCommand::History(kind) => {
                let text = self.editor.history_listing(kind);
                self.open_scratch(&text, "history —", "history — :bd to close");
            }
            ExCommand::SetQuery(name) => {
                self.editor.message = self.editor.option_value(&name);
            }
            ExCommand::ShowOptions(all) => {
                let text = self.editor.options_listing(all);
                self.open_scratch(&text, "options (", "options — :bd to close");
            }
            ExCommand::Retab(n) => self.editor.retab(n),
            ExCommand::Filter { range, cmd } => self.editor.filter_range(range, &cmd),
            ExCommand::Align { range, kind, width } => {
                self.editor.align_lines(range, kind, width);
            }
            ExCommand::Earlier(n) => self.editor.undo_times(n),
            ExCommand::Later(n) => self.editor.redo_times(n),
            ExCommand::SetTheme(arg) => match arg {
                Some(name) => {
                    if self.themes.set_current(&name) {
                        self.notify_fmt("theme: {name}", &[("{name}", &name)]);
                    } else {
                        self.editor.message = format!(
                            "Unknown theme '{name}'. Available: {}",
                            self.themes.names().join(", ")
                        );
                    }
                }
                None => {
                    let name = self.themes.cycle().to_string();
                    self.notify_fmt("theme: {name}", &[("{name}", &name)]);
                }
            },
            ExCommand::ToggleNumbers(on) => {
                self.editor.show_line_numbers = on;
            }
            ExCommand::ToggleRelativeNumbers(on) => {
                self.editor.relative_numbers = on;
                // Relative numbers are only meaningful with the gutter shown.
                if on {
                    self.editor.show_line_numbers = true;
                }
            }
            ExCommand::ToggleAutoIndent(on) => {
                self.editor.autoindent = on;
            }
            ExCommand::ToggleSmartIndent(on) => {
                self.editor.smartindent = on;
            }
            ExCommand::TogglePaste(on) => {
                self.editor.paste = on;
                self.notify(if on { "paste" } else { "nopaste" });
            }
            ExCommand::ToggleReadonly(on) => {
                self.editor.readonly = on;
                self.notify(if on { "readonly" } else { "noreadonly" });
            }
            ExCommand::ToggleAutowrite(on) => {
                self.editor.autowrite = on;
                self.notify(if on { "autowrite" } else { "noautowrite" });
            }
            ExCommand::ToggleAutoread(on) => {
                self.editor.autoread = on;
                // Baseline the mtime so an edit made just before enabling it does
                // not trigger an immediate reload.
                self.editor.refresh_file_mtime();
                self.notify(if on { "autoread" } else { "noautoread" });
            }
            ExCommand::ToggleTitle(on) => {
                self.editor.title = on;
            }
            ExCommand::ToggleBackup(on) => {
                self.editor.backup = on;
            }
            ExCommand::ToggleShowMode(on) => self.editor.showmode = on,
            ExCommand::ToggleShowCmd(on) => self.editor.showcmd = on,
            ExCommand::ToggleRuler(on) => self.editor.ruler = on,
            ExCommand::ToggleErrorBells(on) => self.editor.errorbells = on,
            ExCommand::ToggleVisualBell(on) => self.editor.visualbell = on,
            ExCommand::ToggleExpandTab(on) => {
                self.editor.expandtab = on;
            }
            ExCommand::SetShiftWidth(n) => {
                self.editor.shiftwidth = n;
                self.editor.message = format!("shiftwidth={n}");
            }
            ExCommand::SetTabStop(n) => {
                self.editor.tabstop = n;
                self.editor.message = format!("tabstop={n}");
            }
            ExCommand::SetScrollOff(n) => {
                self.editor.scrolloff = n;
                self.editor.message = format!("scrolloff={n}");
            }
            ExCommand::SetSideScrollOff(n) => {
                self.editor.sidescrolloff = n;
                self.editor.message = format!("sidescrolloff={n}");
            }
            ExCommand::ToggleModeline(on) => {
                self.editor.modeline = on;
                self.editor.message = self.editor.option_value("modeline");
            }
            ExCommand::SetModelines(n) => {
                self.editor.modelines = n;
                self.editor.message = format!("modelines={n}");
            }
            ExCommand::SetTextWidth(n) => {
                self.editor.textwidth = n;
                self.editor.message = format!("textwidth={n}");
            }
            ExCommand::SetFiletype(name) => match Language::from_name(&name) {
                Some(lang) => {
                    self.editor.set_language(lang);
                    self.notify_fmt("filetype: {name}", &[("{name}", lang.name())]);
                }
                None => self.notify_fmt("Unknown filetype '{name}'", &[("{name}", &name)]),
            },
            ExCommand::SetLanguage(code) => match crate::i18n::Locale::from_code(&code) {
                Some(loc) => {
                    self.editor.locale = loc;
                    self.notify_fmt("language: {name}", &[("{name}", loc.display_name())]);
                }
                None => self.notify_fmt("E474: Unknown language '{code}'", &[("{code}", &code)]),
            },
            ExCommand::ChangeDir(dir) => self.change_dir(dir),
            ExCommand::Pwd => {
                self.editor.message = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "E187: unknown directory".into());
            }
            ExCommand::SetFileFormat(name) => self.editor.set_file_format(&name),
            ExCommand::SetFoldMethod(name) => {
                self.editor.set_fold_method(&name);
                // Syntax folding needs the registry; compute it here and report.
                if name == "syntax" {
                    self.editor.recompute_syntax_folds(&self.syntax);
                    let n = self.editor.folds().all().len();
                    self.editor.message = format!("foldmethod=syntax ({n} folds; zM to close)");
                }
            }
            ExCommand::SetFoldColumn(n) => {
                self.editor.foldcolumn = n.min(12);
                self.editor.message = format!("foldcolumn={}", self.editor.foldcolumn);
            }
            ExCommand::SetFoldLevel(n) => {
                self.editor.set_foldlevel(n);
                self.editor.message = format!("foldlevel={n}");
            }
            ExCommand::ToggleSplitBelow(on) => self.editor.splitbelow = on,
            ExCommand::ToggleSplitRight(on) => self.editor.splitright = on,
            ExCommand::Help => self.open_help(),
            ExCommand::Version => {
                self.editor.message = format!("rvim {}", crate::VERSION);
            }
            ExCommand::Goto(n) => self.editor.goto_line(n),
            ExCommand::ToggleHlSearch(on) => {
                self.editor.hlsearch = on;
                if !on {
                    self.editor.message = "search highlight cleared".into();
                }
            }
            ExCommand::ToggleIgnoreCase(on) => {
                self.editor.ignorecase = on;
                self.editor.message =
                    format!("ignorecase {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleSmartCase(on) => {
                self.editor.smartcase = on;
                self.editor.message =
                    format!("smartcase {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleIncSearch(on) => {
                self.editor.incsearch = on;
                self.editor.message =
                    format!("incsearch {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleList(on) => {
                self.editor.list = on;
                self.editor.message = format!("list {}", if on { "on" } else { "off" });
            }
            ExCommand::SetListchars(spec) => self.editor.set_listchars(&spec),
            ExCommand::SetMatchPairs(spec) => self.editor.set_matchpairs(&spec),
            ExCommand::SetEqualprg(cmd) => {
                self.editor.equalprg = cmd;
                self.editor.message = self.editor.option_value("equalprg");
            }
            ExCommand::SetKeywordprg(cmd) => {
                self.editor.keywordprg = cmd;
                self.editor.message = self.editor.option_value("keywordprg");
            }
            ExCommand::SetClipboardUnnamed(on) => {
                self.editor.clipboard_unnamed = on;
                self.editor.message = self.editor.option_value("clipboard");
            }
            ExCommand::SetNrformats(list) => {
                self.editor.nrformats_alpha =
                    list.split(',').any(|t| t.trim() == "alpha");
                self.editor.message = self.editor.option_value("nrformats");
            }
            ExCommand::SetVirtualEdit(list) => {
                self.editor.virtualedit_onemore =
                    list.split(',').any(|t| matches!(t.trim(), "onemore" | "all"));
                self.editor.message = self.editor.option_value("virtualedit");
            }
            ExCommand::SetFormatprg(cmd) => {
                self.editor.formatprg = cmd;
                self.editor.message = self.editor.option_value("formatprg");
            }
            ExCommand::ToggleWrapScan(on) => {
                self.editor.wrapscan = on;
                self.editor.message = format!("wrapscan {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleGdefault(on) => {
                self.editor.gdefault = on;
                self.editor.message = format!("gdefault {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleCursorLine(on) => {
                self.editor.cursorline = on;
                self.editor.message = format!("cursorline {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleCursorColumn(on) => {
                self.editor.cursorcolumn = on;
                self.editor.message = format!("cursorcolumn {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleShiftRound(on) => {
                self.editor.shiftround = on;
                self.editor.message = format!("shiftround {}", if on { "on" } else { "off" });
            }
            ExCommand::ToggleJoinSpaces(on) => {
                self.editor.joinspaces = on;
                self.editor.message = format!("joinspaces {}", if on { "on" } else { "off" });
            }
            ExCommand::SetColorColumn { abs, rel } => {
                self.editor.colorcolumns = abs;
                self.editor.colorcolumns_rel = rel;
                self.editor.message = self.editor.option_value("colorcolumn");
            }
            ExCommand::MoveLines { range, dest } => {
                self.editor.move_lines(range, dest);
            }
            ExCommand::CopyLines { range, dest } => {
                self.editor.copy_lines(range, dest);
            }
            ExCommand::DeleteLines { range, register } => {
                self.editor.delete_lines(range, register);
            }
            ExCommand::YankLines { range, register } => {
                self.editor.yank_lines(range, register);
            }
            ExCommand::ShiftLines { range, dedent, times } => {
                self.editor.shift_lines(range, dedent, times);
            }
            ExCommand::JoinLines { range, raw } => {
                self.editor.join_lines(range, raw);
            }
            ExCommand::PutRegister { dest, register } => {
                self.editor.put_register(dest, register);
            }
            ExCommand::Normal { range, keys } => self.run_normal(range, &keys),
            ExCommand::Sort { range, reverse, unique, numeric, ignorecase, pattern, use_match } => {
                let before = self.editor.buffer.line_count();
                self.editor
                    .sort_lines(range, reverse, unique, numeric, ignorecase, pattern, use_match);
                let after = self.editor.buffer.line_count();
                self.editor.message = if unique && after < before {
                    format!("sorted; {} duplicate line(s) removed", before - after)
                } else {
                    format!("sorted {after} lines")
                };
            }
            ExCommand::Substitute(spec) => {
                let (subs, lines) = self.editor.substitute(&spec);
                self.editor.message = if subs == 0 {
                    format!("E486: Pattern not found: {}", spec.pattern)
                } else if spec.count_only {
                    let s_p = if subs == 1 { "" } else { "es" };
                    let l_p = if lines == 1 { "" } else { "s" };
                    format!("{subs} match{s_p} on {lines} line{l_p}")
                } else {
                    let s_p = if subs == 1 { "" } else { "s" };
                    let l_p = if lines == 1 { "" } else { "s" };
                    format!("{subs} substitution{s_p} on {lines} line{l_p}")
                };
            }
            ExCommand::SubstituteConfirm(spec) => {
                self.editor.substitute_confirm_start(&spec);
            }
            ExCommand::Abbrev { lhs, rhs } => self.editor.set_abbrev(&lhs, &rhs),
            ExCommand::Unabbrev(lhs) => self.editor.remove_abbrev(&lhs),
            ExCommand::AbbrevList => {
                let text = self.editor.abbrev_listing();
                self.open_scratch(&text, "abbreviations —", "abbreviations — :bd to close");
            }
            ExCommand::MapKey { lhs, rhs } => self.editor.set_nmap(lhs, &rhs),
            ExCommand::Unmap(lhs) => self.editor.remove_nmap(lhs),
            ExCommand::MapList => {
                let text = self.editor.nmap_listing();
                self.open_scratch(&text, "mappings —", "mappings — :bd to close");
            }
            ExCommand::DefineCommand { name, expansion } => {
                if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    self.editor.message =
                        "E183: user command names must start with an uppercase letter".into();
                } else {
                    self.user_commands.insert(name, expansion);
                }
            }
            ExCommand::DeleteCommand(name) => {
                if self.user_commands.remove(&name).is_none() {
                    self.editor.message = format!("E184: no such user command: {name}");
                }
            }
            ExCommand::CommandList => {
                let mut names: Vec<_> = self.user_commands.iter().collect();
                names.sort_by(|a, b| a.0.cmp(b.0));
                let body = if names.is_empty() {
                    "no user commands".to_string()
                } else {
                    let mut s = String::from("user commands — :bd to close\n\n");
                    for (n, e) in names {
                        s.push_str(&format!("{n}\t{e}\n"));
                    }
                    s
                };
                self.open_scratch(&body, "commands —", "commands — :bd to close");
            }
            ExCommand::Global {
                pattern,
                invert,
                command,
            } => {
                let trimmed = command.trim();
                if trimmed.is_empty()
                    || matches!(trimmed, "p" | "print" | "#" | "nu" | "number" | "l" | "list")
                {
                    // `:g/re/p` (and bare `:g/re/`) — list matching lines in a scratch
                    // buffer; `#` / `number` prefix line numbers.
                    let numbered = matches!(trimmed, "#" | "nu" | "number");
                    let rows = self.editor.global_rows(&pattern, invert);
                    if rows.is_empty() {
                        self.notify("pattern not found");
                    } else {
                        let mut body = String::from("matching lines — :bd to close\n\n");
                        for row in rows {
                            let line = self.editor.buffer.line(row).unwrap_or("");
                            if numbered {
                                body.push_str(&format!("  {:>5}  {}\n", row + 1, line));
                            } else {
                                body.push_str(&format!("  {line}\n"));
                            }
                        }
                        self.open_scratch(&body, "matching —", "matching lines — :bd to close");
                    }
                } else if let command::ExCommand::Normal { keys, .. } =
                    command::parse(trimmed)
                {
                    // `:g/pat/normal {keys}` — run the keys on each matching line.
                    // Process bottom-to-top so earlier indices stay valid even if
                    // a key changes the line count.
                    let rows = self.editor.global_rows(&pattern, invert);
                    let count = rows.len();
                    for &row in rows.iter().rev() {
                        if row >= self.editor.buffer.line_count() {
                            continue;
                        }
                        self.editor.cursor.row = row;
                        self.editor.cursor.col = 0;
                        self.feed_normal_keys(&keys);
                    }
                    self.editor.message = format!("{count} line(s) affected");
                } else {
                    let affected = self.editor.global(&pattern, invert, &command);
                    self.editor.message = format!("{affected} line(s) affected");
                }
            }
            ExCommand::MkSession { file, force } => self.make_session(file.as_deref(), force),
            ExCommand::ToHtml(file) => self.export_html(file.as_deref()),
            ExCommand::Source(path) => match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let cmds = crate::config::parse_config(&text);
                    let n = cmds.len();
                    self.apply_config_lines(&cmds);
                    self.editor.message = format!("\"{path}\" sourced ({n} commands)");
                }
                Err(e) => self.editor.message = format!("E484: Can't open file {path}: {e}"),
            },
            ExCommand::Passthrough { name, args } => self.run_passthrough(&name, &args),
        }
    }

    /// Returns true on a successful write.
    /// `:[range]w file` — write just the range's lines to `file`.
    fn write_range(&mut self, range: command::SubRange, file: Option<String>) {
        let Some(file) = file else {
            self.editor.message = "E140: use :w <file> to write a range".into();
            return;
        };
        let (a, b) = self.editor.range_rows(range);
        let mut out = String::new();
        for row in a..=b {
            out.push_str(self.editor.buffer.line(row).unwrap_or(""));
            out.push('\n');
        }
        match std::fs::write(&file, out) {
            Ok(()) => self.editor.message = format!("\"{file}\" {} lines written", b - a + 1),
            Err(e) => self.editor.message = format!("E212: write failed: {e}"),
        }
    }

    /// `:[range]w >>file` — append the range's lines (or the whole buffer) to a
    /// file, creating it if needed. The buffer is not modified.
    fn write_append(&mut self, range: Option<command::SubRange>, file: &str) {
        if file.is_empty() {
            self.notify("E471: Argument required");
            return;
        }
        let (a, b) = match range {
            Some(r) => self.editor.range_rows(r),
            None => (0, self.editor.buffer.line_count().saturating_sub(1)),
        };
        let mut out = String::new();
        for row in a..=b {
            out.push_str(self.editor.buffer.line(row).unwrap_or(""));
            out.push('\n');
        }
        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
            .and_then(|mut f| std::io::Write::write_all(&mut f, out.as_bytes()));
        match result {
            Ok(()) => self.editor.message = format!("\"{file}\" {} lines appended", b - a + 1),
            Err(e) => self.editor.message = format!("E212: append failed: {e}"),
        }
    }

    /// `:enew` — open a new empty, unnamed buffer in the current window. The old
    /// buffer is kept in the buffer list (reachable with `:bn`/`:bp`) when it has a
    /// file name or unsaved changes, so nothing is lost.
    fn edit_new_buffer(&mut self) {
        let mut ed = Editor::new();
        self.inherit_prefs(&mut ed);
        ed.buf_id = self.fresh_buf_id();
        self.remember_alternate();
        let old = std::mem::replace(&mut self.editor, ed);
        if old.buffer.path().is_some() || old.buffer.is_dirty() {
            self.others.push(old);
        }
        self.editor.message = "[No Name]".into();
    }

    /// `:cd [dir]` — change the process working directory (affects relative paths
    /// for `:e`/`:w`/`:r`). No argument changes to the home directory; a leading
    /// `~` expands to home. Reports the new directory, or an error.
    fn change_dir(&mut self, dir: Option<String>) {
        let target: std::path::PathBuf = match dir {
            Some(d) => {
                let d = d.trim();
                if d == "~" {
                    match crate::config::home_dir() {
                        Some(h) => h,
                        None => {
                            self.editor.message = "E472: no home directory".into();
                            return;
                        }
                    }
                } else if let Some(rest) = d.strip_prefix("~/").or_else(|| d.strip_prefix("~\\")) {
                    match crate::config::home_dir() {
                        Some(h) => h.join(rest),
                        None => std::path::PathBuf::from(d),
                    }
                } else {
                    std::path::PathBuf::from(d)
                }
            }
            None => match crate::config::home_dir() {
                Some(h) => h,
                None => {
                    self.editor.message = "E472: no home directory".into();
                    return;
                }
            },
        };
        match std::env::set_current_dir(&target) {
            Ok(_) => {
                self.editor.message = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| target.display().to_string());
            }
            Err(e) => {
                self.editor.message =
                    format!("E344: Can't change directory to \"{}\": {e}", target.display());
            }
        }
    }

    fn do_write(&mut self, arg: Option<String>, force: bool) -> bool {
        // `:set readonly` guards against accidental overwrite; `:w!` overrides.
        if self.editor.readonly && !force {
            self.notify("E45: 'readonly' option is set (add ! to override)");
            return false;
        }
        if let Some(path) = arg {
            self.editor.buffer.set_path(&path);
        }
        // `:set backup` — keep the previous file contents as `{name}~`.
        if self.editor.backup {
            if let Some(p) = self.editor.buffer.path() {
                if p.exists() {
                    let mut bak = p.as_os_str().to_os_string();
                    bak.push("~");
                    let _ = std::fs::copy(p, std::path::PathBuf::from(bak));
                }
            }
        }
        match self.editor.buffer.save() {
            Ok(bytes) => {
                self.editor.redetect_language();
                // Our own write is the newest version; don't let autoread re-read it.
                self.editor.refresh_file_mtime();
                let name = self
                    .editor
                    .buffer
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let bytes = bytes.to_string();
                self.notify_fmt(
                    "\"{name}\" {bytes}B written",
                    &[("{name}", &name), ("{bytes}", &bytes)],
                );
                true
            }
            Err(e) => {
                self.editor.message = format!("E212: write failed: {e}");
                false
            }
        }
    }

    /// `:set autoread` — pick up external changes to open files. A clean buffer
    /// whose file changed on disk is reloaded; a modified one is kept and a `W12`
    /// warning is shown once (the focused buffer only). Background buffers are
    /// reloaded silently when clean. Cheap no-op unless `autoread` is on; called
    /// once per event in the main loop.
    fn check_autoread(&mut self) {
        if !self.editor.autoread {
            return;
        }
        // Focused buffer: reload when clean, warn (once) when it has local edits.
        if self.editor.file_changed_on_disk() {
            if self.editor.buffer.is_dirty() {
                // Don't clobber edits; bump the known mtime so we warn only once
                // per external change.
                self.editor.refresh_file_mtime();
                self.notify("W12: file changed on disk since editing started");
            } else {
                self.reload_focused_from_disk();
            }
        }
        // Background buffers: silently reload the clean ones that changed.
        for i in 0..self.others.len() {
            if self.others[i].buffer.is_dirty() || !self.others[i].file_changed_on_disk() {
                continue;
            }
            let Some(path) = self.others[i].buffer.path().map(|p| p.display().to_string()) else {
                continue;
            };
            let row = self.others[i].cursor.row;
            if let Ok(mut ed) = Editor::from_file(&path) {
                self.inherit_prefs(&mut ed);
                ed.buf_id = self.others[i].buf_id;
                ed.cursor.row = row.min(ed.buffer.line_count().saturating_sub(1));
                self.others[i] = ed;
            }
        }
    }

    /// Reload the focused buffer from disk in place (keeping its id and cursor row),
    /// used by `autoread`. The caller has already checked it is safe to do so.
    fn reload_focused_from_disk(&mut self) {
        let Some(path) = self.editor.buffer.path().map(|p| p.display().to_string()) else {
            return;
        };
        let row = self.editor.cursor.row;
        if let Ok(mut ed) = Editor::from_file(&path) {
            self.inherit_prefs(&mut ed);
            ed.buf_id = self.editor.buf_id;
            ed.cursor.row = row.min(ed.buffer.line_count().saturating_sub(1));
            self.editor = ed;
            self.apply_modelines();
            self.notify_fmt("\"{path}\" reloaded (changed on disk)", &[("{path}", &path)]);
        }
    }

    /// `:set autowrite` — save the focused buffer before leaving it, when it is
    /// modified, named, and not read-only. Called by the buffer-switch commands.
    fn autowrite_current(&mut self) {
        if self.editor.autowrite
            && self.editor.buffer.is_dirty()
            && !self.editor.readonly
            && self.editor.buffer.path().is_some()
        {
            self.do_write(None, false);
        }
    }

    /// Save the active buffer and every other open buffer that has a file name.
    /// Returns `(written, failed)`; a buffer with no path counts as a failure.
    fn write_all(&mut self) -> (usize, usize) {
        let mut written = 0;
        let mut failed = 0;
        for ed in std::iter::once(&mut self.editor).chain(self.others.iter_mut()) {
            if ed.buffer.path().is_none() {
                failed += 1;
                continue;
            }
            match ed.buffer.save() {
                Ok(_) => {
                    ed.redetect_language();
                    written += 1;
                }
                Err(_) => failed += 1,
            }
        }
        (written, failed)
    }

    /// `:[range]normal {keys}` — feed `keys` as Normal-mode input. With a range,
    /// run them at the start of each line in it; otherwise once at the cursor.
    fn run_normal(&mut self, range: Option<command::SubRange>, keys: &str) {
        let rows: Vec<usize> = match range {
            Some(r) => {
                let (a, b) = self.editor.range_rows(r);
                (a..=b).collect()
            }
            None => vec![self.editor.cursor.row],
        };
        for row in rows {
            let last = self.editor.buffer.line_count().saturating_sub(1);
            if row > last {
                break;
            }
            self.editor.cursor.row = row;
            self.editor.cursor.col = 0;
            self.feed_normal_keys(keys);
        }
    }

    /// Feed each character of `keys` to the editor as a Normal-mode key event,
    /// running any ex-command it produces, then press Esc so insert/pending state
    /// is always cleaned up (as vim does at the end of `:normal`).
    fn feed_normal_keys(&mut self, keys: &str) {
        for c in keys.chars() {
            let ev = crossterm::event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            let action = self.editor.handle_key(ev);
            self.apply_editor_action(action);
        }
        let esc = crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        self.editor.handle_key(esc);
    }

    fn run_passthrough(&mut self, name: &str, args: &str) {
        // A couple of `:set` options not handled by the parser.
        if name == "set" {
            match args.trim() {
                "mouse" | "mouse=a" => {
                    self.want_mouse = true;
                    self.editor.message = "mouse enabled".into();
                    return;
                }
                "nomouse" | "mouse=" => {
                    self.want_mouse = false;
                    self.editor.message = "mouse disabled".into();
                    return;
                }
                other => {
                    self.editor.message = format!("Unknown option: {other}");
                    return;
                }
            }
        }

        // Offer to plugins.
        let path_str = self
            .editor
            .buffer
            .path()
            .map(|p| p.display().to_string());
        let doc = PluginDoc {
            lines: self.editor.buffer.lines(),
            cursor: self.editor.cursor,
            language: self.editor.language.name(),
            path: path_str.as_deref(),
        };
        match self.plugins.dispatch(name, args, &doc) {
            Some(resp) => {
                if let Some(msg) = resp.message {
                    self.editor.message = msg;
                }
            }
            None => {
                self.notify_fmt("E492: Not an editor command: {name}", &[("{name}", name)]);
            }
        }
    }

    /// Open `text` in a throwaway scratch buffer, preserving the current buffer at
    /// the front of the list (so `:bd` returns to it). Shared by `:help`,
    /// `:marks`, `:registers`, and `:jumps`. `marker` is a substring of the first
    /// line used to detect "already showing this" and avoid stacking copies.
    fn open_scratch(&mut self, text: &str, marker: &str, message: &str) {
        let already = self.editor.buffer.path().is_none()
            && self
                .editor
                .buffer
                .line(0)
                .map(|l| l.contains(marker))
                .unwrap_or(false);
        let mut sed = Editor::new();
        sed.buffer = crate::buffer::Buffer::from_text(text);
        sed.set_language(Language::PlainText);
        self.inherit_prefs(&mut sed);
        if already {
            // Replace the current scratch in place rather than stacking another.
            self.editor.buffer = sed.buffer;
        } else {
            self.remember_alternate();
            let old = std::mem::replace(&mut self.editor, sed);
            self.others.insert(0, old);
        }
        self.editor.message = message.into();
    }

    fn open_help(&mut self) {
        let help = help_text(
            &self.themes.names(),
            &crate::syntax::language_names(),
            &self.plugins.all_commands(),
        );
        self.open_scratch(&help, "quick help", "help — :bd to close");
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn help_text(themes: &[&str], langs: &[&str], plugin_cmds: &[&str]) -> String {
    format!(
        "rvim {ver} — quick help  (press :bd to close)\n\
         \n\
         MODES\n\
         \ti / a / I / A      insert (before/after/line-start/line-end)\n\
         \tR                  replace (overtype) mode\n\
         \to / O              open line below / above\n\
         \t  (insert) C-w/C-u delete word-before / to line-start\n\
         \t  (insert) C-r<r>  paste register   C-t / C-d  indent / dedent\n\
         \t  (insert) C-n/C-p keyword completion (cycle matches in buffer)\n\
         \t  (insert) C-x C-l  whole-line completion   C-x C-f  filename\n\
         \t:iabbrev lhs rhs   insert abbreviation (:una lhs removes; :ab lists)\n\
         \t:nnoremap k keys   remap a Normal key (:nunmap k removes)\n\
         \t  (insert) C-k<2>  digraph, e.g. C-k a: -> ä, C-k -> arrow\n\
         \t  (insert) C-o     run one Normal command, then resume insert\n\
         \t  (insert) C-a     re-insert the last inserted text (\". register)\n\
         \t  (insert) C-e/C-y copy the char below / above the cursor\n\
         \tv / V / Ctrl-v     visual / visual-line / visual-block\n\
         \t  (v-block) d I A c  delete / insert / append / change rectangle\n\
         \t  (v-block) $A      append at each line's own end (ragged-right)\n\
         \t  (v-block) y / p   yank the rectangle / paste it back as a rectangle\n\
         \t  (visual) J / gJ  join the selected lines (with / without space)\n\
         \t  (visual) r<c>    replace every selected char with c\n\
         \t  (visual) C-a/C-x increment / decrement number on each line\n\
         \t  (visual) gC-a/gC-x  build an incrementing sequence (1,2,3,…)\n\
         \t  (visual) u/U/~   lower / upper / toggle case of selection\n\
         \t  (visual) g?      ROT13 the selection\n\
         \t  (visual) * / #   search for the selected text fwd / back\n\
         \t  (visual) i/a+obj select a text object (viw vi( vap)\n\
         \t  (visual) o       swap selection end     gv  reselect last\n\
         \t  (visual) p/P     replace selection with register\n\
         \tEsc                back to normal mode\n\
         \tAlt / F10          open the top menu bar (command helper)\n\
         \n\
         MOTIONS\n\
         \th j k l  arrows    move left/down/up/right\n\
         \tw / b / e / ge     word forward / back / end / prev-end (W/B/E/gE = WORD)\n\
         \t{{ / }}             paragraph back / forward   ( / )  sentence back / fwd\n\
         \t[[ ]] [] ][         section back/fwd (open brace), close-brace variants\n\
         \t[( [{{ ]) ]}}        jump to unmatched enclosing bracket (counted)\n\
         \tf/F/t/T <c>        find char (counted; op: dfx ct)); ; , repeat/rev\n\
         \t%                  jump to matching bracket () [] {{}}   <n>%  n% of file\n\
         \td% / y% / c%       operate from cursor to the matching bracket\n\
         \t0 / ^ / $ / g_      line start / first-nonblank / end / last-nonblank\n\
         \t+ / - / _  |        line first-nonblank down/up/down   | = column\n\
         \tgg / G             top / bottom (or <n>G, :<n>)\n\
         \tH / M / L          top / middle / bottom of screen (op: dL yH; <n>H/<n>L)\n\
         \tzz / zt / zb       center / top / bottom current line\n\
         \tz. / z<CR> / z-    same, then move to first non-blank\n\
         \tCtrl-d / Ctrl-u    half-page down / up\n\
         \tCtrl-f / Ctrl-b    full-page forward / back (counted)\n\
         \tCtrl-e / Ctrl-y    scroll one line down / up\n\
         \n\
         FOLDING\n\
         \tzf{{motion}} / zF   create fold (V_zf folds selection; <n>zF n lines)\n\
         \tzo / zc / za       open / close / toggle fold under cursor\n\
         \tzO / zC / zA       recursive open / close / toggle (nested folds)\n\
         \tzR / zM            open all / close all folds\n\
         \tzr / zm            open / close one fold level (:set foldlevel=N)\n\
         \tzd / zE            delete fold under cursor / all folds\n\
         \tzi                 toggle folding on / off\n\
         \tzj / zk            jump to next / previous fold\n\
         \t[z / ]z            jump to start / end of the current fold\n\
         \t:set foldcolumn=N   show a fold-indicator column N wide\n\
         \t:set foldmethod=indent   build folds from indentation\n\
         \t:set foldmethod=marker   build folds from fold markers\n\
         \t:set foldmethod=syntax   folds from braces, or indent for brace-less langs\n\
         \n\
         WINDOWS\n\
         \t:split / :vsplit    split the window (horizontal / vertical)\n\
         \tCtrl-w s / Ctrl-w v same, from Normal mode\n\
         \tCtrl-w h/j/k/l      move focus left / down / up / right\n\
         \tCtrl-w w / Ctrl-w W cycle focus forward / back\n\
         \tCtrl-w c / :close   close the focused window\n\
         \tCtrl-w o / :only    keep only the focused window\n\
         \tCtrl-w x / Ctrl-w r swap with next window / rotate all windows\n\
         \tCtrl-w +/- / >/<    resize focused window (height / width); = equalizes\n\
         \n\
         EDITING\n\
         \tx / X              delete char under / before   r<c>  replace\n\
         \tY                  yank line (= yy)\n\
         \t~                  toggle case        s / S  subst char / line\n\
         \tgI / gp / gP       insert at col 0 / paste leaving cursor after\n\
         \td/y/c + motion     e.g. dw d$ d0 de dj dG yw y$ cc  (dd/yy/cc)\n\
         \td/y/c + i/a + obj   text objects: diw daW dip das ci( yi\" da{{ ...\n\
         \tdgg / dG           delete to top / bottom of file\n\
         \tgu / gU / g~ + mot  lower / upper / toggle case (guw gUiw guu)\n\
         \tg? + mot            ROT13 (g?w g?ip g??, or on a visual selection)\n\
         \tgcc  gc<motion>     toggle line comment (also visual gc)\n\
         \tD / C              delete / change to end of line\n\
         \t>> / <<            indent / dedent (also >motion, >ip, visual)\n\
         \t== / =G / =ip       reindent lines by brace depth (or :set equalprg)\n\
         \tyy / p / P         yank line / paste after / before\n\
         \t]p / [p            paste below / above, reindented to current line\n\
         \t\"a yy / \"a p       named registers a-z (\"A-\"Z append); auto: \"0 \"1-9 \"-\n\
         \t\"_dd               black-hole register (delete, keep registers)\n\
         \t\"%p                paste the current file name (% register)\n\
         \t\"+y \"+p            system clipboard (\"* = PRIMARY selection on X11)\n\
         \t:set clipboard=unnamed  make plain y/d/p use the system clipboard\n\
         \tJ / gJ             join lines (with / without space)\n\
         \tu / Ctrl-r          undo / redo (counted; also :earlier N / :later N)\n\
         \t.  <n>.            repeat last change (n times)\n\
         \tCtrl-a / Ctrl-x    increment / decrement number (dec / 0x hex / 0b bin)\n\
         \t                   (:set nrformats=...,alpha also bumps a letter a->b)\n\
         \n\
         SEARCH\n\
         \t/pat  ?pat         search fwd / back (regex)  n / N  next / prev\n\
         \t  offsets: /pat/e (match end) /pat/s±N (start) /pat/±N (lines)\n\
         \t* / #  g* / g#     search word under cursor (whole / substring)\n\
         \t[I / ]I            list lines with the word under cursor (all / below)\n\
         \tgd / gD            go to definition (nearest above / first in file)\n\
         \tgf / gF            open the file name under the cursor (gF: at :line)\n\
         \tgx                 open the web URL under the cursor in your browser\n\
         \tK                  look up the word via :set keywordprg (man, pydoc, …)\n\
         \tCtrl-] / Ctrl-t    jump to the tag under cursor / back (needs a tags file)\n\
         \tgn / gN            select next / prev match (cgn + . to repeat)\n\
         \t&  / g&            repeat last :s on current line / whole file\n\
         \tm<x> `<x> '<x>     set mark / jump exact / jump line   `` prev pos\n\
         \tm<A-Z>  `<A-Z>     global marks: jump across files (reopens the file)\n\
         \t`[ `]              start / end of the last change / yank / put\n\
         \tCtrl-o / Ctrl-i    jump list: older / newer position\n\
         \tg; / g,            change list: older / newer edit position\n\
         \tCtrl-g / ga        file info / character code under cursor\n\
         \tg Ctrl-g           word / char / byte counts\n\
         \tgq{{motion}} / gqq / gqip   reflow lines/paragraph to textwidth (gw too)\n\
         \tgi  `.  `^         resume insert / last change / last insert\n\
         \tq<x> q  @<x>  @@   record macro / stop / replay / repeat   @: last :cmd\n\
         \t  (qX appends to macro register x)\n\
         \n\
         COMMANDS\n\
         \t(command line) Up/Down  recall previous commands / searches\n\
         \t(command line) Left/Right Home/End  move the edit cursor (also C-b/C-e)\n\
         \t(command line) Tab      complete command / :set option (wildmenu, cycles)\n\
         \t(command line) C-w/C-u  delete previous word / to line start\n\
         \t(command line) C-r<r>   insert register (C-r C-w = word under cursor)\n\
         \t:w [file]  :q  :q!  :wq  :x   write / quit variants\n\
         \t:qa  :wa  :wqa     quit / write / write-quit all buffers (! to force)\n\
         \tZZ / ZQ            write & quit / quit without saving\n\
         \t:e <file>          open file     :enew  new empty buffer\n\
         \t:[addr]r <file>    read file (or !cmd) below cursor (:0r top, :$r end)\n\
         \t:e / :e!            reload current file (! discards changes)\n\
         \t:mksession[!] [file]  save a session; restore it with :source <file>\n\
         \t:source <file>      run ex-commands from a file (config or session)\n\
         \t:TOhtml [file]      export the buffer to a highlighted HTML file\n\
         \t:ls :bn :bp :b<n>  list / next / prev / goto buffer   :bd close\n\
         \t  (:n / :N are aliases for :bn / :bp)\n\
         \tCtrl-^ / :b#       switch to the alternate (last) buffer\n\
         \t:marks :reg :jumps list marks / registers / jump list\n\
         \t:messages          recall recent status-line messages\n\
         \t:tag <name>  :pop  :tags   jump to a tag / back / list the tag stack\n\
         \t:tnext :tprev :tselect     move among same-named tag matches / list them\n\
         \t:changes           list the change list (g; / g, navigate it)\n\
         \t:delmarks a b / !  delete the named marks (! clears all a-z marks)\n\
         \t:history [:|/|all] list command-line / search history\n\
         \t:s/pat/rep/[ginc]  substitute (g all, i ignore-case, n count, c confirm)\n\
         \t  rep: \\1-\\9 & groups; \\U \\L \\u \\l \\E case; pat: \\< \\> word bounds\n\
         \t:g/re/d  :v/re/d   run cmd on (non-)matching lines (d, s///, >, <, j, p, normal)\n\
         \t:[range]norm {{keys}}  run Normal-mode keys (per line over a range)\n\
         \t:theme <name>      themes: {themes}\n\
         \t:set number|nonumber   :set relativenumber|nornu\n\
         \t:set autoindent|noai   :set expandtab|noet\n\
         \t:set shiftwidth=N  :set tabstop=N  :set scrolloff=N  :set textwidth=N\n\
         \t:set shiftround|nosr   :set joinspaces|nojs   :set matchpairs=(:),<:>\n\
         \t:set equalprg=<cmd>    filter `=` lines through an external formatter\n\
         \t:set formatprg=<cmd>   filter `gq` lines through an external formatter\n\
         \t:set keywordprg=<cmd>  program `K` runs on the word under the cursor\n\
         \t:set paste|nopaste     :set readonly|noro (:w! overrides)\n\
         \t:set virtualedit=onemore  let the cursor sit one past end of line\n\
         \t:set autowrite|noaw    save a named buffer when switching away\n\
         \t:set autoread|noar     reload the file when it changes on disk\n\
         \t:set errorbells|noeb   ring the terminal bell on error messages\n\
         \t:set visualbell|novb   flash the screen on errors instead of beeping\n\
         \t:set language=<code>   UI language (en, vi, fr)\n\
         \t:cd [dir]  :pwd        change / show the working directory\n\
         \t:set sidescrolloff=N   horizontal context columns\n\
         \t:set modeline|noml     apply a file's vim: options (:set modelines=N)\n\
         \t:set {{option}}?        show an option's current value\n\
         \t:set / :set all        list modified / all options\n\
         \t:retab [N]             normalise tabs/spaces to tabstop (set to N)\n\
         \t!{{motion}} / :range!cmd  filter lines through a shell command\n\
         \t:[range]w !cmd      pipe lines to a command, show its output (no change)\n\
         \t:[range]w >>file    append lines to a file\n\
         \t:set ignorecase|noic   :set smartcase|noscs   (search case)\n\
         \t:set gdefault|nogd     :s substitutes all matches on a line by default\n\
         \t:set incsearch|nois    preview match while typing /?\n\
         \t:set list|nolist       show tabs / trailing whitespace\n\
         \t:set listchars=tab:xy,trail:z   customise list markers\n\
         \t:set cursorline|nocul  highlight the cursor's line\n\
         \t:set cursorcolumn|nocuc  highlight the cursor's column\n\
         \t:set colorcolumn=N[,M] highlight guide column(s); +N/-N relative to tw\n\
         \t:set wrapscan|nows     search wraps around the file (default on)\n\
         \t:set ft=<lang>     {langs}\n\
         \t:set mouse|nomouse toggle mouse support\n\
         \t:[range]sort[!] [flags] sort (! rev, u uniq, n/f/x num/float/hex, i ic, /pat/[r])\n\
         \t:[range]m {{addr}}   move lines    :[range]t/co {{addr}}  copy lines\n\
         \t:[range]d / y [x]   delete / yank lines (into register x)   :[range]> / <  shift\n\
         \t:[range]j[!]        join lines (! keeps whitespace)\n\
         \t:[range]ce|ri|le [w]  center / right / left align lines\n\
         \t:[addr]pu [reg]     put a register as lines after addr\n\
         \t:noh               clear search highlight\n\
         \t:{{n}}               jump to line n\n\
         \tplugin commands:   {plugins}\n",
        ver = crate::VERSION,
        themes = themes.join(", "),
        langs = langs.join(" "),
        plugins = plugin_cmds.join(", "),
    )
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
