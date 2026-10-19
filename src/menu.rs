//! The auto-hiding top menu bar.
//!
//! The menu is a *teaching aid* for the command line: every leaf is an
//! ex-command, shown with its shortcut, and selecting one drops the user into
//! the command line with that command pre-filled (rather than running it
//! blindly). Menus are a plain data tree so navigation is pure and testable;
//! rendering lives in [`crate::ui`].

use crate::i18n::{tr, Locale};

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
/// menus reflect what's actually registered. Display labels are localized to
/// `locale` (see [`crate::i18n`]); the ex-commands they run stay unchanged.
pub fn build_menus(
    themes: &[&str],
    langs: &[&str],
    plugin_cmds: &[&str],
    locale: Locale,
) -> Vec<Menu> {
    // Translate a static UI label for the active locale.
    let t = |s: &'static str| tr(locale, s);

    let file = Menu {
        title: t("File").into(),
        items: vec![
            MenuItem::command(t("Write"), "w"),
            MenuItem::command_hint(t("Write As…"), "w", "w "),
            MenuItem::command(t("Write & Quit"), "wq"),
            MenuItem::command(t("Save & Quit"), "x"),
            MenuItem::command(t("Quit"), "q"),
            MenuItem::command(t("Quit Without Saving"), "q!"),
            MenuItem::command_hint(t("Open File…"), "e", "e "),
            MenuItem::command_hint(t("Read File…"), "r", "r "),
            MenuItem::command_hint(t("Source Script…"), "source", "source "),
        ],
    };

    let buffers = Menu {
        title: t("Buffers").into(),
        items: vec![
            MenuItem::command(t("List Buffers"), "ls"),
            MenuItem::command(t("Next Buffer"), "bn"),
            MenuItem::command(t("Previous Buffer"), "bp"),
            MenuItem::command_hint(t("Go To Buffer…"), "b", "b "),
            MenuItem::command(t("Close Buffer"), "bd"),
        ],
    };

    let edit = Menu {
        title: t("Edit").into(),
        items: vec![
            MenuItem::command_hint(t("Substitute (line)…"), "s///", "s///"),
            MenuItem::command_hint(t("Substitute (file)…"), "%s///g", "%s///g"),
            MenuItem::command(t("Sort Lines"), "sort"),
            MenuItem::command(t("Sort Descending"), "sort!"),
            MenuItem::command(t("Sort Unique"), "sort u"),
            MenuItem::command_hint(t("Go To Line…"), ":{n}", ""),
            MenuItem::command(t("Clear Search Highlight"), "noh"),
        ],
    };

    let theme_items: Vec<MenuItem> = themes
        .iter()
        .map(|th| MenuItem::command(&capitalize(th), &format!("theme {th}")))
        .collect();

    let view = Menu {
        title: t("View").into(),
        items: vec![
            MenuItem::submenu(t("Theme"), theme_items),
            MenuItem::submenu(
                t("Line Numbers"),
                vec![
                    MenuItem::command(t("Absolute On"), "set number"),
                    MenuItem::command(t("Absolute Off"), "set nonumber"),
                    MenuItem::command(t("Relative On"), "set relativenumber"),
                    MenuItem::command(t("Relative Off"), "set norelativenumber"),
                ],
            ),
            MenuItem::submenu(
                t("Folding"),
                vec![
                    MenuItem::command(t("Method: Manual"), "set foldmethod=manual"),
                    MenuItem::command(t("Method: Indent"), "set foldmethod=indent"),
                    MenuItem::command(t("Method: Marker"), "set foldmethod=marker"),
                    MenuItem::command(t("Method: Syntax"), "set foldmethod=syntax"),
                    MenuItem::command(t("Fold Column On"), "set foldcolumn=4"),
                    MenuItem::command(t("Fold Column Off"), "set foldcolumn=0"),
                ],
            ),
            MenuItem::submenu(
                t("Options"),
                vec![
                    MenuItem::command(t("Mouse On"), "set mouse"),
                    MenuItem::command(t("Mouse Off"), "set nomouse"),
                    MenuItem::command(t("Search Highlight On"), "set hlsearch"),
                    MenuItem::command(t("Search Highlight Off"), "set nohlsearch"),
                    MenuItem::command(t("Auto-indent On"), "set autoindent"),
                    MenuItem::command(t("Auto-indent Off"), "set noautoindent"),
                    MenuItem::command(t("Smart-indent On"), "set smartindent"),
                    MenuItem::command(t("Smart-indent Off"), "set nosmartindent"),
                    MenuItem::command(t("Show Whitespace On"), "set list"),
                    MenuItem::command(t("Show Whitespace Off"), "set nolist"),
                    MenuItem::command(t("Cursor Line On"), "set cursorline"),
                    MenuItem::command(t("Cursor Line Off"), "set nocursorline"),
                ],
            ),
        ],
    };

    let window = Menu {
        title: t("Window").into(),
        items: vec![
            MenuItem::command(t("Split"), "split"),
            MenuItem::command(t("Vertical Split"), "vsplit"),
            MenuItem::command_hint(t("Split File…"), "split", "split "),
            MenuItem::command(t("Close Window"), "close"),
            MenuItem::command(t("Only This Window"), "only"),
        ],
    };

    // Generated from the language table so new languages appear automatically.
    let mut ft_items: Vec<MenuItem> = langs
        .iter()
        .map(|l| MenuItem::command(&capitalize(l), &format!("set ft={l}")))
        .collect();
    ft_items.push(MenuItem::command(t("Plain Text"), "set ft=text"));
    let language = Menu {
        title: t("Language").into(),
        items: vec![MenuItem::submenu(t("Set Filetype"), ft_items)],
    };

    let tools = Menu {
        title: t("Tools").into(),
        items: plugin_cmds
            .iter()
            .map(|c| MenuItem::command(&capitalize(c), c))
            .collect(),
    };

    let help = Menu {
        title: t("Help").into(),
        items: vec![
            MenuItem::command(t("Help"), "help"),
            MenuItem::command(t("Version"), "version"),
        ],
    };

    vec![file, buffers, edit, view, window, language, tools, help]
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
        MenuState::new(build_menus(&["matrix", "cobalt"], &["rust"], &["wordcount"], Locale::English))
    }

    #[test]
    fn menus_cover_core_commands() {
        let menus = build_menus(&["matrix"], &["rust"], &["wordcount"], Locale::English);
        let titles: Vec<&str> = menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["File", "Buffers", "Edit", "View", "Window", "Language", "Tools", "Help"]
        );
    }

    #[test]
    fn menu_titles_localize_to_vietnamese() {
        let menus = build_menus(&["matrix"], &["rust"], &[], Locale::Vietnamese);
        let titles: Vec<&str> = menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles[0], "Tệp"); // File
        assert_eq!(titles[2], "Sửa"); // Edit
        // The ex-commands the items run stay in English.
        if let MenuAction::Command(c) = &menus[0].items[0].action {
            assert_eq!(c, "w");
        } else {
            panic!("expected a command item");
        }
    }

    #[test]
    fn filetype_menu_is_generated_and_window_fold_entries_exist() {
        fn walk(items: &[MenuItem], out: &mut Vec<String>) {
            for it in items {
                match &it.action {
                    MenuAction::Command(c) => out.push(c.clone()),
                    MenuAction::Submenu(sub) => walk(sub, out),
                }
            }
        }
        let menus = build_menus(&["matrix"], &["typescript", "go"], &[], Locale::English);
        let mut cmds = Vec::new();
        for m in &menus {
            walk(&m.items, &mut cmds);
        }
        // Filetypes come from the provided language list.
        assert!(cmds.contains(&"set ft=typescript".to_string()));
        assert!(cmds.contains(&"set ft=go".to_string()));
        // New Window and Folding actions are present.
        assert!(cmds.contains(&"vsplit".to_string()));
        assert!(cmds.contains(&"only".to_string()));
        assert!(cmds.contains(&"set foldmethod=indent".to_string()));
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
