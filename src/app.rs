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
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use std::io::{self, BufWriter};

/// Top-level editor application.
pub struct App {
    pub editor: Editor,
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
            let (cols, rows) = TerminalGuard::size()?;
            let layout = Layout::compute(
                cols,
                rows,
                self.editor.buffer.line_count(),
                self.editor.show_line_numbers,
            );
            self.editor
                .set_viewport(layout.text_rows as usize, layout.text_cols as usize);

            ui::render(&mut out, &self.editor, self.themes.current(), &self.syntax)?;

            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
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
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if m.row < layout.text_rows {
                    let row = self.editor.top + m.row as usize;
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
                if self.editor.buffer.is_dirty() && !force {
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
            ExCommand::Edit(path) => match Editor::from_file(&path) {
                Ok(mut ed) => {
                    ed.show_line_numbers = self.editor.show_line_numbers;
                    self.editor = ed;
                    self.editor.message =
                        format!("\"{path}\" {} lines", self.editor.buffer.line_count());
                }
                Err(e) => self.editor.message = format!("E212: Can't open \"{path}\": {e}"),
            },
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
         \to / O              open line below / above\n\
         \tv / V              visual / visual-line\n\
         \t  (visual) u/U/~   lower / upper / toggle case of selection\n\
         \tEsc                back to normal mode\n\
         \n\
         MOTIONS\n\
         \th j k l  arrows    move left/down/up/right\n\
         \tw / b / e          word forward / back / end\n\
         \tf/F/t/T <c>        find char on line   ; ,  repeat / reverse\n\
         \t%                  jump to matching bracket () [] {{}}\n\
         \t0 / ^ / $          line start / first non-blank / line end\n\
         \tgg / G             top / bottom (or <n>G, :<n>)\n\
         \tH / M / L          top / middle / bottom of screen\n\
         \tzz / zt / zb       center / top / bottom current line\n\
         \tCtrl-d / Ctrl-u    half-page down / up\n\
         \tCtrl-e / Ctrl-y    scroll one line down / up\n\
         \n\
         EDITING\n\
         \tx                  delete char        r<c>  replace char\n\
         \t~                  toggle case        s / S  subst char / line\n\
         \tdd / dw / d$ / D   delete line/word/to-eol\n\
         \tcc / cw / C        change line/word/to-eol\n\
         \t>> / <<            indent / dedent (also in visual mode)\n\
         \tyy / p / P         yank line / paste after / before\n\
         \t\"a yy / \"a p       use named register a (any a-z)\n\
         \tJ                  join lines         u / Ctrl-r  undo / redo\n\
         \n\
         SEARCH\n\
         \t/pat  ?pat         search fwd / back  n / N  next / prev\n\
         \n\
         COMMANDS\n\
         \t:w [file]  :q  :q!  :wq  :x   write / quit variants\n\
         \t:e <file>          open file\n\
         \t:theme <name>      themes: {themes}\n\
         \t:set number|nonumber   :set relativenumber|nornu\n\
         \t:set ft=<lang>     rust tsql pgsql trino snowflake z80 sql\n\
         \t:set mouse|nomouse toggle mouse support\n\
         \t:sort[!] [u]       sort lines (! reverse, u unique)\n\
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
