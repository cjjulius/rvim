//! The auto-hiding top menu bar.
//!
//! The menu is a *teaching aid* for the command line: every leaf is an
//! ex-command, shown with its shortcut, and selecting one drops the user into
//! the command line with that command pre-filled (rather than running it
//! blindly). Menus are a plain data tree so navigation is pure and testable;
//! rendering lives in [`crate::ui`].

/// What activating a menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// Pre-fill the command line with this text (without the leading `:`).
    Command(String),
    /// Open a nested submenu.
    Submenu(Vec<MenuItem>),
}

/// One entry in a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    /// The shortcut shown on the right (e.g. `wq`), or empty for submenus.
    pub hint: String,
    pub action: MenuAction,
}

impl MenuItem {
    fn command(label: &str, command: &str) -> Self {
        Self {
            label: label.to_string(),
            hint: command.to_string(),
            action: MenuAction::Command(command.to_string()),
        }
    }

    /// A command whose displayed hint differs from the pre-filled text (e.g. an
    /// item that takes an argument: hint `w`, pre-fill `w `).
    fn command_hint(label: &str, hint: &str, command: &str) -> Self {
        Self {
            label: label.to_string(),
            hint: hint.to_string(),
            action: MenuAction::Command(command.to_string()),
        }
    }

    fn submenu(label: &str, items: Vec<MenuItem>) -> Self {
        Self {
            label: label.to_string(),
            hint: String::new(),
            action: MenuAction::Submenu(items),
        }
    }

    pub fn is_submenu(&self) -> bool {
        matches!(self.action, MenuAction::Submenu(_))
    }
}

/// A top-level menu in the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Build the full menu tree. `themes` and `plugin_cmds` make the View/Tools
/// menus reflect what's actually registered.
pub fn build_menus(themes: &[&str], plugin_cmds: &[&str]) -> Vec<Menu> {
    let file = Menu {
        title: "File".into(),
        items: vec![
            MenuItem::command("Write", "w"),
            MenuItem::command_hint("Write As…", "w", "w "),
            MenuItem::command("Write & Quit", "wq"),
            MenuItem::command("Save & Quit", "x"),
            MenuItem::command("Quit", "q"),
            MenuItem::command("Quit Without Saving", "q!"),
            MenuItem::command_hint("Open File…", "e", "e "),
            MenuItem::command_hint("Source Script…", "source", "source "),
        ],
    };

    let buffers = Menu {
        title: "Buffers".into(),
        items: vec![
            MenuItem::command("List Buffers", "ls"),
            MenuItem::command("Next Buffer", "bn"),
            MenuItem::command("Previous Buffer", "bp"),
            MenuItem::command_hint("Go To Buffer…", "b", "b "),
            MenuItem::command("Close Buffer", "bd"),
        ],
    };

    let edit = Menu {
        title: "Edit".into(),
        items: vec![
            MenuItem::command_hint("Substitute (line)…", "s///", "s///"),
            MenuItem::command_hint("Substitute (file)…", "%s///g", "%s///g"),
            MenuItem::command("Sort Lines", "sort"),
            MenuItem::command("Sort Descending", "sort!"),
            MenuItem::command("Sort Unique", "sort u"),
            MenuItem::command_hint("Go To Line…", ":{n}", ""),
            MenuItem::command("Clear Search Highlight", "noh"),
        ],
    };

    let theme_items: Vec<MenuItem> = themes
        .iter()
        .map(|t| MenuItem::command(&capitalize(t), &format!("theme {t}")))
        .collect();

    let view = Menu {
        title: "View".into(),
        items: vec![
            MenuItem::submenu("Theme", theme_items),
            MenuItem::submenu(
                "Line Numbers",
                vec![
                    MenuItem::command("Absolute On", "set number"),
                    MenuItem::command("Absolute Off", "set nonumber"),
                    MenuItem::command("Relative On", "set relativenumber"),
                    MenuItem::command("Relative Off", "set norelativenumber"),
                ],
            ),
            MenuItem::submenu(
                "Options",
                vec![
                    MenuItem::command("Mouse On", "set mouse"),
                    MenuItem::command("Mouse Off", "set nomouse"),
                    MenuItem::command("Search Highlight On", "set hlsearch"),
                    MenuItem::command("Search Highlight Off", "set nohlsearch"),
                    MenuItem::command("Auto-indent On", "set autoindent"),
                    MenuItem::command("Auto-indent Off", "set noautoindent"),
                ],
            ),
        ],
    };

    let language = Menu {
        title: "Language".into(),
        items: vec![MenuItem::submenu(
            "Set Filetype",
            vec![
                MenuItem::command("Rust", "set ft=rust"),
                MenuItem::command("T-SQL", "set ft=tsql"),
                MenuItem::command("PostgreSQL", "set ft=pgsql"),
                MenuItem::command("Trino / Starburst", "set ft=trino"),
                MenuItem::command("Snowflake", "set ft=snowflake"),
                MenuItem::command("Z80 Assembly", "set ft=z80"),
                MenuItem::command("ANSI SQL", "set ft=sql"),
                MenuItem::command("Plain Text", "set ft=text"),
            ],
        )],
    };

    let tools = Menu {
        title: "Tools".into(),
        items: plugin_cmds
            .iter()
            .map(|c| MenuItem::command(&capitalize(c), c))
            .collect(),
    };

    let help = Menu {
        title: "Help".into(),
        items: vec![
            MenuItem::command("Help", "help"),
            MenuItem::command("Version", "version"),
        ],
    };

    vec![file, buffers, edit, view, language, tools, help]
}

/// The result of a navigation key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuOutcome {
    /// Nothing to do (stay open).
    None,
    /// Close the menu entirely.
    Close,
    /// Pre-fill the command line with this text and close the menu.
    Run(String),
}

/// Live navigation state over a [`Menu`] tree.
///
/// `stack` holds the selected index at each open dropdown level. An empty stack
/// means only the bar is highlighted (no dropdown shown yet).
#[derive(Debug, Clone)]
pub struct MenuState {
    pub menus: Vec<Menu>,
    pub top: usize,
    pub stack: Vec<usize>,
}

impl MenuState {
    pub fn new(menus: Vec<Menu>) -> Self {
        Self {
            menus,
            top: 0,
            stack: Vec::new(),
        }
    }

    /// Number of open dropdown levels (0 = bar only).
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The items shown at dropdown level `level` (0-based), if open.
    pub fn items_at(&self, level: usize) -> Option<&[MenuItem]> {
        let mut items: &[MenuItem] = &self.menus.get(self.top)?.items;
        for d in 0..level {
            let idx = *self.stack.get(d)?;
            match &items.get(idx)?.action {
                MenuAction::Submenu(sub) => items = sub,
                MenuAction::Command(_) => return None,
            }
        }
        Some(items)
    }

    /// The selected index at dropdown level `level`.
    pub fn selected_at(&self, level: usize) -> usize {
        self.stack.get(level).copied().unwrap_or(0)
    }

    fn deepest_items(&self) -> Option<&[MenuItem]> {
        if self.stack.is_empty() {
            None
        } else {
            self.items_at(self.stack.len() - 1)
        }
    }

    /// Jump to the top-level menu whose title starts with `ch` and open it.
    pub fn open_initial(&mut self, ch: char) {
        let lower = ch.to_ascii_lowercase();
        if let Some(i) = self
            .menus
            .iter()
            .position(|m| m.title.to_ascii_lowercase().starts_with(lower))
        {
            self.top = i;
            self.stack = vec![0];
        }
    }

    pub fn move_top(&mut self, delta: isize) {
        let n = self.menus.len() as isize;
        if n == 0 {
            return;
        }
        self.top = (((self.top as isize + delta) % n + n) % n) as usize;
        if !self.stack.is_empty() {
            self.stack = vec![0]; // reopen the newly-selected menu
        }
    }

    pub fn down(&mut self) {
        if self.stack.is_empty() {
            self.stack.push(0);
            return;
        }
        let level = self.stack.len() - 1;
        if let Some(items) = self.items_at(level) {
            let n = items.len().max(1);
            self.stack[level] = (self.stack[level] + 1) % n;
        }
    }

    pub fn up(&mut self) {
        if self.stack.is_empty() {
            return;
        }
        let level = self.stack.len() - 1;
        if let Some(items) = self.items_at(level) {
            let n = items.len().max(1);
            self.stack[level] = (self.stack[level] + n - 1) % n;
        }
    }

    pub fn right(&mut self) {
        match self.deepest_items() {
            None => self.move_top(1),
            Some(items) => {
                let sel = self.selected_at(self.stack.len() - 1);
                if items.get(sel).map(|i| i.is_submenu()).unwrap_or(false) {
                    self.stack.push(0);
                } else {
                    self.move_top(1);
                }
            }
        }
    }

    pub fn left(&mut self) {
        if self.stack.len() >= 2 {
            self.stack.pop();
        } else {
            self.move_top(-1);
        }
    }

    pub fn enter(&mut self) -> MenuOutcome {
        if self.stack.is_empty() {
            self.stack.push(0);
            return MenuOutcome::None;
        }
        let level = self.stack.len() - 1;
        let sel = self.stack[level];
        let action = self
            .items_at(level)
            .and_then(|items| items.get(sel))
            .map(|i| i.action.clone());
        match action {
            Some(MenuAction::Submenu(_)) => {
                self.stack.push(0);
                MenuOutcome::None
            }
            Some(MenuAction::Command(cmd)) => MenuOutcome::Run(cmd),
            None => MenuOutcome::None,
        }
    }

    pub fn esc(&mut self) -> MenuOutcome {
        if self.stack.is_empty() {
            MenuOutcome::Close
        } else {
            self.stack.pop();
            MenuOutcome::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> MenuState {
        MenuState::new(build_menus(&["matrix", "cobalt"], &["wordcount"]))
    }

    #[test]
    fn menus_cover_core_commands() {
        let menus = build_menus(&["matrix"], &["wordcount"]);
        let titles: Vec<&str> = menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["File", "Buffers", "Edit", "View", "Language", "Tools", "Help"]
        );
    }

    #[test]
    fn open_and_navigate_to_command() {
        let mut s = state();
        // Bar only; Down opens the File dropdown at item 0 ("Write").
        s.down();
        assert_eq!(s.depth(), 1);
        let out = s.enter();
        assert_eq!(out, MenuOutcome::Run("w".into()));
    }

    #[test]
    fn move_between_top_menus_reopens_dropdown() {
        let mut s = state();
        s.down(); // open File
        s.move_top(1); // -> Buffers, dropdown reopened at 0
        assert_eq!(s.top, 1);
        assert_eq!(s.depth(), 1);
        let out = s.enter();
        assert_eq!(out, MenuOutcome::Run("ls".into()));
    }

    #[test]
    fn submenu_opens_and_runs() {
        let mut s = state();
        s.top = 3; // View
        s.stack = vec![0]; // "Theme" submenu item highlighted
        let out = s.enter(); // open Theme submenu
        assert_eq!(out, MenuOutcome::None);
        assert_eq!(s.depth(), 2);
        let out = s.enter(); // first theme -> "theme matrix"
        assert_eq!(out, MenuOutcome::Run("theme matrix".into()));
    }

    #[test]
    fn esc_backs_out_one_level_then_closes() {
        let mut s = state();
        s.top = 3;
        s.stack = vec![0];
        s.enter(); // into Theme submenu, depth 2
        assert_eq!(s.depth(), 2);
        assert_eq!(s.esc(), MenuOutcome::None); // -> depth 1
        assert_eq!(s.depth(), 1);
        assert_eq!(s.esc(), MenuOutcome::None); // -> depth 0 (bar only)
        assert_eq!(s.depth(), 0);
        assert_eq!(s.esc(), MenuOutcome::Close); // final esc closes
    }

    #[test]
    fn left_closes_submenu_but_moves_top_at_root() {
        let mut s = state();
        s.top = 3; // View
        s.stack = vec![0];
        s.enter(); // Theme submenu open, depth 2
        s.left(); // closes submenu
        assert_eq!(s.depth(), 1);
        s.left(); // at root level -> move to previous top menu
        assert_eq!(s.top, 2); // Edit
    }

    #[test]
    fn open_initial_jumps_to_menu() {
        let mut s = state();
        s.open_initial('v'); // View
        assert_eq!(s.top, 3);
        assert_eq!(s.depth(), 1);
    }

    #[test]
    fn down_wraps_within_dropdown() {
        let mut s = state();
        s.down(); // File open at 0
        let n = s.items_at(0).unwrap().len();
        for _ in 0..n {
            s.down();
        }
        assert_eq!(s.selected_at(0), 0); // wrapped back to top
    }
}
