//! The application: owns all subsystems, runs the event loop, and executes
//! ex-commands (dispatching unknown ones to plugins).

use crate::command::{self, ExCommand};
use crate::editor::{Action, Editor};
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
use std::io::{self, BufWriter};

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
        }
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
        app.editor.message = format!("\"{path}\" {} lines", app.editor.buffer.line_count());
        Ok(app)
    }

    /// Load `path` into an inactive buffer (for extra files given on the command
    /// line); the first file stays active. Reachable afterward with `:bn`/`:bp`.
    pub fn open_additional(&mut self, path: &str) -> io::Result<()> {
        let mut ed = Editor::from_file(path)?;
        self.inherit_prefs(&mut ed);
        self.others.push(ed);
        Ok(())
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
    }

    /// `:e <file>` — edit a file, switching to it if already open.
    fn edit_file(&mut self, path: &str, line: Option<usize>) {
        if self.editor.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path) {
            if let Some(n) = line {
                self.editor.goto_line(n);
            }
            self.editor.message = format!("already editing \"{path}\"");
            return;
        }
        if let Some(i) = self
            .others
            .iter()
            .position(|e| e.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path))
        {
            self.remember_alternate();
            std::mem::swap(&mut self.editor, &mut self.others[i]);
            if let Some(n) = line {
                self.editor.goto_line(n);
            }
            self.editor.message = format!("\"{path}\" (buffer switched)");
            return;
        }
        match Editor::from_file(path) {
            Ok(mut ed) => {
                self.inherit_prefs(&mut ed);
                let lines = ed.buffer.line_count();
                self.remember_alternate();
                let old = std::mem::replace(&mut self.editor, ed);
                if old.buffer.path().is_some() || old.buffer.is_dirty() {
                    self.others.push(old);
                }
                if let Some(n) = line {
                    self.editor.goto_line(n);
                }
                self.editor.message = format!("\"{path}\" {lines} lines");
            }
            Err(e) => self.editor.message = format!("E212: Can't open \"{path}\": {e}"),
        }
    }

    /// `:e` / `:e!` with no argument — reload the current file from disk. Without
    /// `force`, refuses when there are unsaved changes. The cursor row is kept
    /// where possible.
    fn reload_file(&mut self, force: bool) {
        let Some(path) = self.editor.buffer.path().map(|p| p.display().to_string()) else {
            self.editor.message = "E32: No file name".into();
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
                let lines = ed.buffer.line_count();
                self.editor = ed;
                self.editor.message = format!("\"{path}\" {lines} lines --reloaded--");
            }
            Err(e) => self.editor.message = format!("E212: Can't open \"{path}\": {e}"),
        }
    }

    /// `:bn` — rotate to the next buffer.
    fn buffer_next(&mut self) {
        if self.others.is_empty() {
            self.editor.message = "only one buffer".into();
            return;
        }
        self.remember_alternate();
        let next = self.others.remove(0);
        let old = std::mem::replace(&mut self.editor, next);
        self.others.push(old);
        self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
    }

    /// `:bp` — rotate to the previous buffer.
    fn buffer_prev(&mut self) {
        if let Some(prev) = self.others.pop() {
            self.remember_alternate();
            let old = std::mem::replace(&mut self.editor, prev);
            self.others.insert(0, old);
            self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
        } else {
            self.editor.message = "only one buffer".into();
        }
    }

    /// `:b <n>` — switch to the nth buffer (1 = active, 2.. = others).
    fn buffer_goto(&mut self, n: usize) {
        if n == 1 {
            return;
        }
        let idx = n.wrapping_sub(2);
        if idx < self.others.len() {
            self.remember_alternate();
            std::mem::swap(&mut self.editor, &mut self.others[idx]);
            self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
        } else {
            self.editor.message = format!("E86: Buffer {n} does not exist");
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
        self.editor.message = format!("buffer closed; now \"{}\"", Self::buffer_name(&self.editor));
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
        }
    }

    /// `:split` / `:vsplit` (and `Ctrl-w s` / `Ctrl-w v`). The new window takes
    /// focus and shows `file`, or a duplicate view of the current buffer.
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
            self.editor.message = "E444: Cannot close last window".into();
            return false;
        }
        if self.editor.buffer.is_dirty() && !force {
            self.editor.message =
                "E37: No write since last change (add ! to override)".into();
            return false;
        }
        let next = self.layout.prev(self.focused).unwrap_or(self.focused);
        self.layout.close(self.focused);
        if let Some(pos) = self.panes.iter().position(|p| p.id == next) {
            let pane = self.panes.remove(pos);
            self.editor = pane.editor;
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
                (ed.buffer.revision(), ed.language, ed.top)
            };
            let fresh = matches!(
                self.win_block_memo.get(id),
                Some((r, l, t, _)) if *r == rev && *l == lang && *t == top
            );
            if !fresh {
                let state =
                    self.syntax
                        .block_state_at(lang, self.editor_ref(*id).buffer.lines(), top);
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
                // Use the memoized state when present; otherwise compute it (so the
                // view is correct even if called without a prior refresh).
                let top_state = match self.win_block_memo.get(&id) {
                    Some((r, l, t, s))
                        if *r == editor.buffer.revision()
                            && *l == editor.language
                            && *t == editor.top =>
                    {
                        *s
                    }
                    _ => self
                        .syntax
                        .block_state_at(editor.language, editor.buffer.lines(), editor.top),
                };
                ui::WindowView {
                    editor,
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
                            &self.plugins.all_commands(),
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
                    let action = self.editor.handle_key(key);
                    if let Action::RunEx(cmd) = action {
                        self.run_ex(&cmd);
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

    /// Execute a parsed ex-command.
    pub fn run_ex(&mut self, input: &str) {
        // `:set a b c` — apply each space-separated option in turn (vim allows
        // several options per `:set`). Each token is itself a valid `:set` command.
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("set ").or_else(|| trimmed.strip_prefix("se ")) {
            let opts: Vec<&str> = rest.split_whitespace().collect();
            if opts.len() > 1 {
                for opt in opts {
                    self.run_ex(&format!("set {opt}"));
                }
                return;
            }
        }
        match command::parse(input) {
            ExCommand::Empty => {}
            ExCommand::Write(arg) => {
                self.do_write(arg);
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
            ExCommand::WriteQuit(arg) => {
                if self.do_write(arg) {
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
            ExCommand::Edit { path, line } => self.edit_file(&path, line),
            ExCommand::Reload { force } => self.reload_file(force),
            ExCommand::ReadFile(path) => {
                if let Some(cmd) = path.strip_prefix('!') {
                    self.editor.read_command(cmd);
                } else {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => self.editor.read_lines_below(&text),
                        Err(e) => {
                            self.editor.message = format!("E484: can't open \"{path}\": {e}")
                        }
                    }
                }
            }
            ExCommand::SplitWindow { vertical, file } => {
                self.split_window(vertical, file.as_deref());
            }
            ExCommand::CloseWindow { force } => {
                self.close_window(force);
            }
            ExCommand::OnlyWindow { force } => self.only_window(force),
            ExCommand::BufferList => self.buffer_list(),
            ExCommand::BufferNext => self.buffer_next(),
            ExCommand::BufferPrev => self.buffer_prev(),
            ExCommand::Buffer(n) => self.buffer_goto(n),
            ExCommand::BufferDelete => self.buffer_delete(),
            ExCommand::BufferAlternate => self.switch_alternate(),
            ExCommand::Marks => {
                let text = self.editor.marks_listing();
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
                        self.editor.message = format!("theme: {name}");
                    } else {
                        self.editor.message = format!(
                            "Unknown theme '{name}'. Available: {}",
                            self.themes.names().join(", ")
                        );
                    }
                }
                None => {
                    let name = self.themes.cycle().to_string();
                    self.editor.message = format!("theme: {name}");
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
            ExCommand::SetTextWidth(n) => {
                self.editor.textwidth = n;
                self.editor.message = format!("textwidth={n}");
            }
            ExCommand::SetFiletype(name) => match Language::from_name(&name) {
                Some(lang) => {
                    self.editor.set_language(lang);
                    self.editor.message = format!("filetype: {}", lang.name());
                }
                None => self.editor.message = format!("Unknown filetype '{name}'"),
            },
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
            ExCommand::ToggleWrapScan(on) => {
                self.editor.wrapscan = on;
                self.editor.message = format!("wrapscan {}", if on { "on" } else { "off" });
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
            ExCommand::SetColorColumn(n) => {
                self.editor.colorcolumn = n;
                self.editor.message = format!("colorcolumn={n}");
            }
            ExCommand::MoveLines { range, dest } => {
                self.editor.move_lines(range, dest);
            }
            ExCommand::CopyLines { range, dest } => {
                self.editor.copy_lines(range, dest);
            }
            ExCommand::DeleteLines(range) => {
                self.editor.delete_lines(range);
            }
            ExCommand::YankLines(range) => {
                self.editor.yank_lines(range);
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
            ExCommand::Global {
                pattern,
                invert,
                command,
            } => {
                if command.is_empty() {
                    self.editor.message = "E471: Argument required".into();
                } else if let command::ExCommand::Normal { keys, .. } =
                    command::parse(command.trim())
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

    fn do_write(&mut self, arg: Option<String>) -> bool {
        if let Some(path) = arg {
            self.editor.buffer.set_path(&path);
        }
        match self.editor.buffer.save() {
            Ok(bytes) => {
                self.editor.redetect_language();
                let name = self
                    .editor
                    .buffer
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.editor.message = format!("\"{name}\" {bytes}B written");
                true
            }
            Err(e) => {
                self.editor.message = format!("E212: write failed: {e}");
                false
            }
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
            if let Action::RunEx(cmd) = self.editor.handle_key(ev) {
                self.run_ex(&cmd);
            }
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
                self.editor.message = format!("E492: Not an editor command: {name}");
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
         \tzR / zM            open all / close all folds\n\
         \tzd / zE            delete fold under cursor / all folds\n\
         \tzi                 toggle folding on / off\n\
         \tzj / zk            jump to next / previous fold\n\
         \t[z / ]z            jump to start / end of the current fold\n\
         \t:set foldcolumn=N   show a fold-indicator column N wide\n\
         \t:set foldmethod=indent   build folds from indentation\n\
         \t:set foldmethod=marker   build folds from fold markers\n\
         \t:set foldmethod=syntax   build folds from brace blocks (manual = default)\n\
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
         \tyy / p / P         yank line / paste after / before\n\
         \t]p / [p            paste below / above, reindented to current line\n\
         \t\"a yy / \"a p       named registers a-z (\"A-\"Z append); auto: \"0 \"1-9 \"-\n\
         \t\"_dd               black-hole register (delete, keep registers)\n\
         \t\"%p                paste the current file name (% register)\n\
         \t\"+y \"+p            system clipboard (\"* = PRIMARY selection on X11)\n\
         \tJ / gJ             join lines (with / without space)\n\
         \tu / Ctrl-r          undo / redo (counted; also :earlier N / :later N)\n\
         \t.  <n>.            repeat last change (n times)\n\
         \tCtrl-a / Ctrl-x    increment / decrement number (dec / 0x hex / 0b bin)\n\
         \n\
         SEARCH\n\
         \t/pat  ?pat         search fwd / back (regex)  n / N  next / prev\n\
         \t  offsets: /pat/e (match end) /pat/s±N (start) /pat/±N (lines)\n\
         \t* / #  g* / g#     search word under cursor (whole / substring)\n\
         \tgd / gD            go to definition (nearest above / first in file)\n\
         \tgf / gF            open the file name under the cursor (gF: at :line)\n\
         \tgn / gN            select next / prev match (cgn + . to repeat)\n\
         \t&  / g&            repeat last :s on current line / whole file\n\
         \tm<x> `<x> '<x>     set mark / jump exact / jump line   `` prev pos\n\
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
         \t(command line) Tab      complete command / :set option (wildmenu, cycles)\n\
         \t(command line) C-w/C-u  delete previous word / whole line\n\
         \t(command line) C-r<r>   insert register (C-r C-w = word under cursor)\n\
         \t:w [file]  :q  :q!  :wq  :x   write / quit variants\n\
         \t:qa  :wa  :wqa     quit / write / write-quit all buffers (! to force)\n\
         \tZZ / ZQ            write & quit / quit without saving\n\
         \t:e <file>          open file     :r <file>  read file below cursor\n\
         \t:e / :e!            reload current file (! discards changes)\n\
         \t:ls :bn :bp :b<n>  list / next / prev / goto buffer   :bd close\n\
         \t  (:n / :N are aliases for :bn / :bp)\n\
         \tCtrl-^ / :b#       switch to the alternate (last) buffer\n\
         \t:marks :reg :jumps list marks / registers / jump list\n\
         \t:changes           list the change list (g; / g, navigate it)\n\
         \t:delmarks a b / !  delete the named marks (! clears all a-z marks)\n\
         \t:history [:|/|all] list command-line / search history\n\
         \t:s/pat/rep/[ginc]  substitute (g all, i ignore-case, n count, c confirm)\n\
         \t:g/re/d  :v/re/d   run cmd on (non-)matching lines (d, s///, normal)\n\
         \t:[range]norm {{keys}}  run Normal-mode keys (per line over a range)\n\
         \t:theme <name>      themes: {themes}\n\
         \t:set number|nonumber   :set relativenumber|nornu\n\
         \t:set autoindent|noai   :set expandtab|noet\n\
         \t:set shiftwidth=N  :set tabstop=N  :set scrolloff=N  :set textwidth=N\n\
         \t:set shiftround|nosr   :set joinspaces|nojs   :set matchpairs=(:),<:>\n\
         \t:set sidescrolloff=N   horizontal context columns\n\
         \t:set {{option}}?        show an option's current value\n\
         \t:set / :set all        list modified / all options\n\
         \t:retab [N]             normalise tabs/spaces to tabstop (set to N)\n\
         \t!{{motion}} / :range!cmd  filter lines through a shell command\n\
         \t:set ignorecase|noic   :set smartcase|noscs   (search case)\n\
         \t:set incsearch|nois    preview match while typing /?\n\
         \t:set list|nolist       show tabs / trailing whitespace\n\
         \t:set listchars=tab:xy,trail:z   customise list markers\n\
         \t:set cursorline|nocul  highlight the cursor's line\n\
         \t:set cursorcolumn|nocuc  highlight the cursor's column\n\
         \t:set colorcolumn=N     highlight column N (cc=0 off)\n\
         \t:set wrapscan|nows     search wraps around the file (default on)\n\
         \t:set ft=<lang>     {langs}\n\
         \t:set mouse|nomouse toggle mouse support\n\
         \t:[range]sort[!] [uni] sort (! rev, u uniq, n numeric, i ic, /pat/[r])\n\
         \t:[range]m {{addr}}   move lines    :[range]t/co {{addr}}  copy lines\n\
         \t:[range]d / y       delete / yank lines   :[range]> / <  shift lines\n\
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
