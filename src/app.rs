//! The application: owns all subsystems, runs the event loop, and executes
//! ex-commands (dispatching unknown ones to plugins).

use crate::command::{self, ExCommand};
use crate::editor::{Action, Editor};
use crate::mode::Mode;
use crate::plugin::{PluginDoc, PluginManager};
use crate::syntax::{Language, Registry};
use crate::terminal::TerminalGuard;
use crate::theme::ThemeRegistry;
use crate::ui::{self, Layout};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::io::{self, BufWriter};

/// Top-level editor application.
pub struct App {
    /// The active buffer.
    pub editor: Editor,
    /// Other open buffers (inactive), in list order.
    others: Vec<Editor>,
    themes: ThemeRegistry,
    syntax: Registry,
    plugins: PluginManager,
    quit: bool,
    want_mouse: bool,
}

impl App {
    /// A new app over an empty scratch buffer.
    pub fn new() -> Self {
        Self {
            editor: Editor::new(),
            others: Vec::new(),
            themes: ThemeRegistry::with_builtins(),
            syntax: Registry::with_builtins(),
            plugins: PluginManager::with_builtins(),
            quit: false,
            want_mouse: false,
        }
    }

    /// A new app editing `path` (created on save if it doesn't exist).
    pub fn open(path: &str) -> io::Result<Self> {
        let mut app = App::new();
        app.editor = Editor::from_file(path)?;
        app.editor.message = format!("\"{path}\" {} lines", app.editor.buffer.line_count());
        Ok(app)
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
    }

    /// `:e <file>` — edit a file, switching to it if already open.
    fn edit_file(&mut self, path: &str) {
        if self.editor.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path) {
            self.editor.message = format!("already editing \"{path}\"");
            return;
        }
        if let Some(i) = self
            .others
            .iter()
            .position(|e| e.buffer.path().map(|p| p.display().to_string()).as_deref() == Some(path))
        {
            std::mem::swap(&mut self.editor, &mut self.others[i]);
            self.editor.message = format!("\"{path}\" (buffer switched)");
            return;
        }
        match Editor::from_file(path) {
            Ok(mut ed) => {
                self.inherit_prefs(&mut ed);
                let lines = ed.buffer.line_count();
                let old = std::mem::replace(&mut self.editor, ed);
                if old.buffer.path().is_some() || old.buffer.is_dirty() {
                    self.others.push(old);
                }
                self.editor.message = format!("\"{path}\" {lines} lines");
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
        let next = self.others.remove(0);
        let old = std::mem::replace(&mut self.editor, next);
        self.others.push(old);
        self.editor.message = format!("\"{}\"", Self::buffer_name(&self.editor));
    }

    /// `:bp` — rotate to the previous buffer.
    fn buffer_prev(&mut self) {
        if let Some(prev) = self.others.pop() {
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
            );
            self.editor
                .set_viewport(layout.text_rows as usize, layout.text_cols as usize);

            ui::render(&mut out, &self.editor, self.themes.current(), &self.syntax, &tabs)?;

            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
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
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if m.row >= layout.top_offset && m.row < layout.top_offset + layout.text_rows {
                    let row = self.editor.top + (m.row - layout.top_offset) as usize;
                    let col = self
                        .editor
                        .left
                        .saturating_add((m.column.saturating_sub(layout.gutter_width)) as usize);
                    let max_row = self.editor.buffer.line_count().saturating_sub(1);
                    self.editor.cursor.row = row.min(max_row);
                    let max_col = self.editor.buffer.line_len(self.editor.cursor.row);
                    self.editor.cursor.col = col.min(max_col.saturating_sub(1));
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

    /// Execute a parsed ex-command.
    pub fn run_ex(&mut self, input: &str) {
        match command::parse(input) {
            ExCommand::Empty => {}
            ExCommand::Write(arg) => {
                self.do_write(arg);
            }
            ExCommand::Quit { force } => {
                let dirty = self.editor.buffer.is_dirty()
                    || self.others.iter().any(|e| e.buffer.is_dirty());
                if dirty && !force {
                    self.editor.message =
                        "E37: No write since last change (add ! to override)".into();
                } else {
                    self.quit = true;
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
            ExCommand::Edit(path) => self.edit_file(&path),
            ExCommand::BufferList => self.buffer_list(),
            ExCommand::BufferNext => self.buffer_next(),
            ExCommand::BufferPrev => self.buffer_prev(),
            ExCommand::Buffer(n) => self.buffer_goto(n),
            ExCommand::BufferDelete => self.buffer_delete(),
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
            ExCommand::SetFiletype(name) => match Language::from_name(&name) {
                Some(lang) => {
                    self.editor.set_language(lang);
                    self.editor.message = format!("filetype: {}", lang.name());
                }
                None => self.editor.message = format!("Unknown filetype '{name}'"),
            },
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
            ExCommand::Sort { reverse, unique } => {
                let before = self.editor.buffer.line_count();
                self.editor.sort_buffer(reverse, unique);
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
                } else {
                    let s_p = if subs == 1 { "" } else { "s" };
                    let l_p = if lines == 1 { "" } else { "s" };
                    format!("{subs} substitution{s_p} on {lines} line{l_p}")
                };
            }
            ExCommand::Global {
                pattern,
                invert,
                command,
            } => {
                if command.is_empty() {
                    self.editor.message = "E471: Argument required".into();
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

    fn open_help(&mut self) {
        let help = help_text(&self.themes.names(), &self.plugins.all_commands());
        self.editor.buffer = crate::buffer::Buffer::from_text(&help);
        self.editor.set_language(Language::PlainText);
        self.editor.cursor = crate::buffer::Position::default();
        self.editor.top = 0;
        self.editor.message = "help — :q to close".into();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn help_text(themes: &[&str], plugin_cmds: &[&str]) -> String {
    format!(
        "rvim {ver} — quick help  (press :q to close)\n\
         \n\
         MODES\n\
         \ti / a / I / A      insert (before/after/line-start/line-end)\n\
         \tR                  replace (overtype) mode\n\
         \to / O              open line below / above\n\
         \t  (insert) C-w/C-u delete word-before / to line-start\n\
         \t  (insert) C-r<r>  paste register   C-t / C-d  indent / dedent\n\
         \tv / V / Ctrl-v     visual / visual-line / visual-block\n\
         \t  (v-block) d I A c  delete / insert / append / change rectangle\n\
         \t  (visual) u/U/~   lower / upper / toggle case of selection\n\
         \t  (visual) o       swap selection end     gv  reselect last\n\
         \tEsc                back to normal mode\n\
         \tAlt / F10          open the top menu bar (command helper)\n\
         \n\
         MOTIONS\n\
         \th j k l  arrows    move left/down/up/right\n\
         \tw / b / e / ge     word forward / back / end / prev-end (W/B/E/gE = WORD)\n\
         \t{{ / }}             paragraph back / forward\n\
         \t[[ ]] [] ][         section back/fwd (open brace), close-brace variants\n\
         \tf/F/t/T <c>        find char on line   ; ,  repeat / reverse\n\
         \t%                  jump to matching bracket () [] {{}}\n\
         \t0 / ^ / $ / g_      line start / first-nonblank / end / last-nonblank\n\
         \t+ / - / _  |        line first-nonblank down/up/down   | = column\n\
         \tgg / G             top / bottom (or <n>G, :<n>)\n\
         \tH / M / L          top / middle / bottom of screen\n\
         \tzz / zt / zb       center / top / bottom current line\n\
         \tCtrl-d / Ctrl-u    half-page down / up\n\
         \tCtrl-f / Ctrl-b    full-page forward / back (counted)\n\
         \tCtrl-e / Ctrl-y    scroll one line down / up\n\
         \n\
         EDITING\n\
         \tx / X              delete char under / before   r<c>  replace\n\
         \tY                  yank line (= yy)\n\
         \t~                  toggle case        s / S  subst char / line\n\
         \td/y/c + motion     e.g. dw d$ d0 de dj dG yw y$ cc  (dd/yy/cc)\n\
         \td/y/c + i/a + obj   text objects: diw daW dip ci( yi\" da{{ ...\n\
         \tdgg / dG           delete to top / bottom of file\n\
         \tgu / gU / g~ + mot  lower / upper / toggle case (guw gUiw guu)\n\
         \tgcc  gc<motion>     toggle line comment (also visual gc)\n\
         \tD / C              delete / change to end of line\n\
         \t>> / <<            indent / dedent (also in visual mode)\n\
         \tyy / p / P         yank line / paste after / before\n\
         \t\"a yy / \"a p       named registers a-z; auto: \"0 yank \"1-9 del \"- small\n\
         \tJ / gJ             join lines (with / without space)\n\
         \tu / Ctrl-r          undo / redo\n\
         \t.                  repeat last change\n\
         \tCtrl-a / Ctrl-x    increment / decrement number\n\
         \n\
         SEARCH\n\
         \t/pat  ?pat         search fwd / back (regex)  n / N  next / prev\n\
         \t* / #  g* / g#     search word under cursor (whole / substring)\n\
         \t&                  repeat last :s on the current line\n\
         \tm<x> `<x> '<x>     set mark / jump exact / jump line   `` prev pos\n\
         \tCtrl-o / Ctrl-i    jump list: older / newer position\n\
         \tgi  `.  `^         resume insert / last change / last insert\n\
         \tq<x> q  @<x>  @@   record macro / stop / replay / repeat\n\
         \n\
         COMMANDS\n\
         \t(command line) Up/Down  recall previous commands / searches\n\
         \t:w [file]  :q  :q!  :wq  :x   write / quit variants\n\
         \t:qa  :wa  :wqa     quit / write / write-quit all buffers (! to force)\n\
         \tZZ / ZQ            write & quit / quit without saving\n\
         \t:e <file>          open file\n\
         \t:ls :bn :bp :b<n>  list / next / prev / goto buffer   :bd close\n\
         \t:g/re/d  :v/re/d   run cmd on (non-)matching lines (d, s///)\n\
         \t:theme <name>      themes: {themes}\n\
         \t:set number|nonumber   :set relativenumber|nornu\n\
         \t:set autoindent|noai   :set expandtab|noet\n\
         \t:set shiftwidth=N  :set tabstop=N  :set scrolloff=N\n\
         \t:set ignorecase|noic   :set smartcase|noscs   (search case)\n\
         \t:set incsearch|nois    preview match while typing /?\n\
         \t:set ft=<lang>     rust tsql pgsql trino snowflake z80 sql\n\
         \t:set mouse|nomouse toggle mouse support\n\
         \t:sort[!] [u]       sort lines (! reverse, u unique)\n\
         \t:[range]m {{addr}}   move lines    :[range]t/co {{addr}}  copy lines\n\
         \t:[range]d / y       delete / yank lines   :[range]> / <  shift lines\n\
         \t:noh               clear search highlight\n\
         \t:{{n}}               jump to line n\n\
         \tplugin commands:   {plugins}\n",
        ver = crate::VERSION,
        themes = themes.join(", "),
        plugins = plugin_cmds.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ex_theme_switch() {
        let mut app = App::new();
        app.run_ex("theme cobalt");
        assert_eq!(app.themes.current().name, "cobalt");
    }

    #[test]
    fn run_ex_unknown_theme_message() {
        let mut app = App::new();
        app.run_ex("theme nope");
        assert!(app.editor.message.contains("Unknown theme"));
    }

    #[test]
    fn run_ex_toggle_numbers() {
        let mut app = App::new();
        app.run_ex("set nonumber");
        assert!(!app.editor.show_line_numbers);
        app.run_ex("set number");
        assert!(app.editor.show_line_numbers);
    }

    #[test]
    fn run_ex_set_filetype() {
        let mut app = App::new();
        app.run_ex("set ft=rust");
        assert_eq!(app.editor.language, Language::Rust);
    }

    #[test]
    fn run_ex_goto_line() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nb\nc\nd");
        app.run_ex("3");
        assert_eq!(app.editor.cursor.row, 2);
    }

    #[test]
    fn run_ex_plugin_passthrough() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hello world");
        app.run_ex("wordcount");
        assert!(app.editor.message.contains("words"));
    }

    #[test]
    fn run_ex_quit_blocked_when_dirty() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x");
        app.editor.buffer.insert_char(crate::buffer::Position::new(0, 1), 'y');
        app.run_ex("q");
        assert!(!app.quit);
        app.run_ex("q!");
        assert!(app.quit);
    }

    #[test]
    fn run_ex_quit_all_respects_dirty() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x");
        app.editor.buffer.insert_char(crate::buffer::Position::new(0, 1), 'y');
        app.run_ex("qa");
        assert!(!app.quit); // blocked: unsaved changes
        app.run_ex("qa!");
        assert!(app.quit); // forced
    }

    #[test]
    fn run_ex_write_all_reports_unnamed_buffer() {
        let mut app = App::new();
        // A fresh scratch buffer has no file name, so it can't be written.
        app.editor.buffer = crate::buffer::Buffer::from_text("scratch");
        app.run_ex("wa");
        assert!(app.editor.message.contains("failed"), "{}", app.editor.message);
        // write-quit-all without force must not quit when a buffer can't be saved.
        app.run_ex("wqa");
        assert!(!app.quit);
        app.run_ex("wqa!");
        assert!(app.quit);
    }

    #[test]
    fn run_ex_substitute_whole_file() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("cat\ncat\ndog");
        app.run_ex("%s/cat/COW/g");
        assert_eq!(app.editor.buffer.line(0), Some("COW"));
        assert_eq!(app.editor.buffer.line(1), Some("COW"));
        assert_eq!(app.editor.buffer.line(2), Some("dog"));
        assert!(app.editor.message.contains("2 substitutions"));
    }

    #[test]
    fn run_ex_substitute_not_found_message() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hello");
        app.run_ex("s/zzz/x/");
        assert!(app.editor.message.contains("Pattern not found"));
    }

    #[test]
    fn run_ex_mouse_toggle() {
        let mut app = App::new();
        app.run_ex("set mouse");
        assert!(app.want_mouse);
        app.run_ex("set nomouse");
        assert!(!app.want_mouse);
    }

    #[test]
    fn run_ex_relativenumber_forces_gutter_on() {
        let mut app = App::new();
        app.run_ex("set nonumber");
        assert!(!app.editor.show_line_numbers);
        app.run_ex("set relativenumber");
        assert!(app.editor.relative_numbers);
        assert!(app.editor.show_line_numbers); // forced back on
        app.run_ex("set nornu");
        assert!(!app.editor.relative_numbers);
    }

    #[test]
    fn multiple_buffers_open_and_navigate() {
        let mut app = App::new();
        app.run_ex("e foo.rs"); // active foo.rs (initial scratch discarded)
        app.run_ex("e bar.tsql"); // active bar.tsql, foo in others
        assert!(app.editor.buffer.path().unwrap().ends_with("bar.tsql"));
        assert_eq!(app.others.len(), 1);
        // Language autodetected on switch.
        assert_eq!(app.editor.language, Language::TSql);

        app.run_ex("bn"); // rotate -> foo.rs
        assert!(app.editor.buffer.path().unwrap().ends_with("foo.rs"));
        app.run_ex("bp"); // back -> bar.tsql
        assert!(app.editor.buffer.path().unwrap().ends_with("bar.tsql"));
    }

    #[test]
    fn buffer_switch_when_already_open() {
        let mut app = App::new();
        app.run_ex("e a.rs");
        app.run_ex("e b.rs");
        // Re-opening a.rs should switch, not create a duplicate.
        app.run_ex("e a.rs");
        assert!(app.editor.buffer.path().unwrap().ends_with("a.rs"));
        assert_eq!(app.others.len(), 1);
    }

    #[test]
    fn buffer_list_and_delete() {
        let mut app = App::new();
        app.run_ex("e one.rs");
        app.run_ex("e two.rs");
        app.run_ex("ls");
        assert!(app.editor.message.contains("one.rs"));
        assert!(app.editor.message.contains("two.rs"));
        // Delete current (two.rs); one.rs becomes active.
        app.run_ex("bd");
        assert!(app.editor.buffer.path().unwrap().ends_with("one.rs"));
        assert!(app.others.is_empty());
        // Can't delete the last buffer.
        app.run_ex("bd");
        assert!(app.editor.message.contains("cannot close last buffer"));
    }

    #[test]
    fn run_ex_high_contrast_theme() {
        let mut app = App::new();
        app.run_ex("theme high-contrast");
        assert_eq!(app.themes.current().name, "high-contrast");
    }

    #[test]
    fn apply_config_lines_applies_settings() {
        let mut app = App::new();
        let lines = vec![
            "theme cobalt".to_string(),
            "set nonumber".to_string(),
            "set mouse".to_string(),
        ];
        app.apply_config_lines(&lines);
        assert_eq!(app.themes.current().name, "cobalt");
        assert!(!app.editor.show_line_numbers);
        assert!(app.want_mouse);
    }

    #[test]
    fn source_command_runs_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rvim_rc_{}.rc", std::process::id()));
        std::fs::write(&path, "\" comment\ntheme retrowave\nset nonumber\n").unwrap();
        let mut app = App::new();
        app.run_ex(&format!("source {}", path.display()));
        assert_eq!(app.themes.current().name, "retrowave");
        assert!(!app.editor.show_line_numbers);
        assert!(app.editor.message.contains("sourced"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn source_missing_file_reports_error() {
        let mut app = App::new();
        app.run_ex("source /no/such/rvimrc-xyz");
        assert!(app.editor.message.contains("Can't open"));
    }
}
