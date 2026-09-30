//! Color themes.
//!
//! A [`Theme`] defines the editor's UI chrome colors plus a color for every
//! [`TokenKind`]. New themes are added by pushing another [`Theme`] into
//! [`ThemeRegistry::with_builtins`] (or at runtime via [`ThemeRegistry::add`]),
//! keeping theming fully modular.

use crate::syntax::TokenKind;
use crossterm::style::Color;
use std::collections::HashMap;

/// A complete color scheme.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    // Editing surface.
    pub bg: Color,
    pub fg: Color,
    pub cursor_line_bg: Color,
    pub selection_bg: Color,
    // Gutter (line numbers).
    pub gutter_bg: Color,
    pub gutter_fg: Color,
    pub current_line_nr_fg: Color,
    // Status line + command line.
    pub status_bg: Color,
    pub status_fg: Color,
    pub mode_bg: Color,
    pub mode_fg: Color,
    pub message_fg: Color,
    // Syntax token colors.
    tokens: HashMap<TokenKind, Color>,
}

impl Theme {
    /// The color for a token kind, falling back to the default foreground.
    pub fn token_color(&self, kind: TokenKind) -> Color {
        self.tokens.get(&kind).copied().unwrap_or(self.fg)
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

/// A collection of available themes with a notion of "current".
pub struct ThemeRegistry {
    themes: Vec<Theme>,
    current: usize,
}

impl ThemeRegistry {
    /// Build a registry with the built-in themes (matrix is the default).
    /// `high-contrast` is a colorblind-friendly, maximum-contrast accessibility
    /// theme built on the Okabe–Ito palette.
    pub fn with_builtins() -> Self {
        Self {
            themes: vec![matrix(), retrowave(), cobalt(), high_contrast()],
            current: 0,
        }
    }

    /// Add a theme (e.g. from a plugin).
    pub fn add(&mut self, theme: Theme) {
        self.themes.push(theme);
    }

    /// The currently active theme.
    pub fn current(&self) -> &Theme {
        &self.themes[self.current]
    }

    /// All theme names, in order.
    pub fn names(&self) -> Vec<&str> {
        self.themes.iter().map(|t| t.name.as_str()).collect()
    }

    /// Switch to a theme by name (case-insensitive). Returns true on success.
    pub fn set_current(&mut self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        if let Some(idx) = self
            .themes
            .iter()
            .position(|t| t.name.to_ascii_lowercase() == name)
        {
            self.current = idx;
            true
        } else {
            false
        }
    }

    /// Advance to the next theme, wrapping around. Returns the new theme's name.
    pub fn cycle(&mut self) -> &str {
        self.current = (self.current + 1) % self.themes.len();
        &self.themes[self.current].name
    }
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

fn token_map(pairs: &[(TokenKind, Color)]) -> HashMap<TokenKind, Color> {
    pairs.iter().copied().collect()
}

/// The Matrix theme — green phosphor on black.
pub fn matrix() -> Theme {
    Theme {
        name: "matrix".into(),
        bg: rgb(8, 12, 8),
        fg: rgb(0, 200, 60),
        cursor_line_bg: rgb(12, 22, 12),
        selection_bg: rgb(0, 80, 30),
        gutter_bg: rgb(5, 8, 5),
        gutter_fg: rgb(0, 90, 30),
        current_line_nr_fg: rgb(120, 255, 120),
        status_bg: rgb(0, 180, 60),
        status_fg: rgb(0, 10, 0),
        mode_bg: rgb(170, 255, 170),
        mode_fg: rgb(0, 20, 0),
        message_fg: rgb(150, 255, 150),
        tokens: token_map(&[
            (TokenKind::Keyword, rgb(120, 255, 120)),
            (TokenKind::Type, rgb(0, 230, 120)),
            (TokenKind::Function, rgb(200, 255, 200)),
            (TokenKind::Builtin, rgb(0, 255, 160)),
            (TokenKind::String, rgb(120, 220, 120)),
            (TokenKind::Char, rgb(120, 220, 120)),
            (TokenKind::Number, rgb(180, 255, 90)),
            (TokenKind::Comment, rgb(0, 110, 40)),
            (TokenKind::Operator, rgb(0, 200, 100)),
            (TokenKind::Punctuation, rgb(0, 160, 70)),
            (TokenKind::Preprocessor, rgb(150, 255, 150)),
            (TokenKind::Label, rgb(200, 255, 150)),
            (TokenKind::Register, rgb(0, 255, 180)),
            (TokenKind::Variable, rgb(170, 255, 140)),
        ]),
    }
}

/// The Retrowave theme — neon pink/cyan on deep purple.
pub fn retrowave() -> Theme {
    Theme {
        name: "retrowave".into(),
        bg: rgb(26, 20, 48),
        fg: rgb(240, 235, 255),
        cursor_line_bg: rgb(40, 30, 66),
        selection_bg: rgb(80, 50, 120),
        gutter_bg: rgb(20, 15, 38),
        gutter_fg: rgb(110, 90, 160),
        current_line_nr_fg: rgb(255, 140, 220),
        status_bg: rgb(255, 80, 180),
        status_fg: rgb(25, 10, 40),
        mode_bg: rgb(0, 230, 230),
        mode_fg: rgb(20, 0, 40),
        message_fg: rgb(255, 180, 230),
        tokens: token_map(&[
            (TokenKind::Keyword, rgb(255, 80, 180)),
            (TokenKind::Type, rgb(0, 230, 230)),
            (TokenKind::Function, rgb(255, 210, 90)),
            (TokenKind::Builtin, rgb(255, 150, 80)),
            (TokenKind::String, rgb(0, 230, 180)),
            (TokenKind::Char, rgb(0, 230, 180)),
            (TokenKind::Number, rgb(255, 150, 80)),
            (TokenKind::Comment, rgb(130, 110, 170)),
            (TokenKind::Operator, rgb(255, 120, 200)),
            (TokenKind::Punctuation, rgb(180, 160, 210)),
            (TokenKind::Preprocessor, rgb(255, 120, 90)),
            (TokenKind::Label, rgb(255, 210, 90)),
            (TokenKind::Register, rgb(0, 230, 230)),
            (TokenKind::Variable, rgb(255, 170, 120)),
        ]),
    }
}

/// The Cobalt theme — warm accents on deep blue.
pub fn cobalt() -> Theme {
    Theme {
        name: "cobalt".into(),
        bg: rgb(0, 38, 66),
        fg: rgb(224, 231, 240),
        cursor_line_bg: rgb(0, 50, 84),
        selection_bg: rgb(0, 70, 120),
        gutter_bg: rgb(0, 30, 54),
        gutter_fg: rgb(60, 110, 150),
        current_line_nr_fg: rgb(255, 255, 255),
        status_bg: rgb(0, 90, 150),
        status_fg: rgb(230, 240, 255),
        mode_bg: rgb(255, 200, 0),
        mode_fg: rgb(0, 30, 60),
        message_fg: rgb(180, 220, 255),
        tokens: token_map(&[
            (TokenKind::Keyword, rgb(255, 200, 0)),
            (TokenKind::Type, rgb(60, 220, 220)),
            (TokenKind::Function, rgb(255, 230, 120)),
            (TokenKind::Builtin, rgb(120, 220, 255)),
            (TokenKind::String, rgb(140, 220, 120)),
            (TokenKind::Char, rgb(140, 220, 120)),
            (TokenKind::Number, rgb(255, 120, 90)),
            (TokenKind::Comment, rgb(80, 140, 180)),
            (TokenKind::Operator, rgb(255, 200, 0)),
            (TokenKind::Punctuation, rgb(150, 190, 220)),
            (TokenKind::Preprocessor, rgb(255, 150, 120)),
            (TokenKind::Label, rgb(255, 230, 120)),
            (TokenKind::Register, rgb(120, 220, 255)),
            (TokenKind::Variable, rgb(255, 180, 120)),
        ]),
    }
}

/// High-contrast accessibility theme — pure black/white with a colorblind-safe
/// (Okabe–Ito) token palette for maximum legibility.
pub fn high_contrast() -> Theme {
    Theme {
        name: "high-contrast".into(),
        bg: rgb(0, 0, 0),
        fg: rgb(255, 255, 255),
        cursor_line_bg: rgb(40, 40, 40),
        selection_bg: rgb(70, 70, 130),
        gutter_bg: rgb(0, 0, 0),
        gutter_fg: rgb(150, 150, 150),
        current_line_nr_fg: rgb(255, 255, 255),
        status_bg: rgb(255, 255, 255),
        status_fg: rgb(0, 0, 0),
        mode_bg: rgb(240, 228, 66),
        mode_fg: rgb(0, 0, 0),
        message_fg: rgb(255, 255, 255),
        tokens: token_map(&[
            (TokenKind::Keyword, rgb(240, 228, 66)),   // yellow
            (TokenKind::Type, rgb(86, 180, 233)),      // sky blue
            (TokenKind::Function, rgb(255, 255, 255)), // white
            (TokenKind::Builtin, rgb(0, 200, 150)),    // bluish green
            (TokenKind::String, rgb(230, 159, 0)),     // orange
            (TokenKind::Char, rgb(230, 159, 0)),
            (TokenKind::Number, rgb(230, 130, 190)),   // reddish purple
            (TokenKind::Comment, rgb(160, 160, 160)),  // grey
            (TokenKind::Operator, rgb(240, 228, 66)),
            (TokenKind::Punctuation, rgb(210, 210, 210)),
            (TokenKind::Preprocessor, rgb(230, 110, 40)), // vermillion
            (TokenKind::Label, rgb(240, 228, 66)),
            (TokenKind::Register, rgb(86, 180, 233)),
            (TokenKind::Variable, rgb(230, 130, 190)),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_starts_on_matrix() {
        let r = ThemeRegistry::with_builtins();
        assert_eq!(r.current().name, "matrix");
        assert_eq!(
            r.names(),
            vec!["matrix", "retrowave", "cobalt", "high-contrast"]
        );
    }

    #[test]
    fn high_contrast_is_selectable() {
        let mut r = ThemeRegistry::with_builtins();
        assert!(r.set_current("high-contrast"));
        assert_eq!(r.current().bg, rgb(0, 0, 0));
        assert_eq!(r.current().fg, rgb(255, 255, 255));
    }

    #[test]
    fn set_current_by_name_is_case_insensitive() {
        let mut r = ThemeRegistry::with_builtins();
        assert!(r.set_current("COBALT"));
        assert_eq!(r.current().name, "cobalt");
        assert!(!r.set_current("nope"));
        assert_eq!(r.current().name, "cobalt");
    }

    #[test]
    fn cycle_wraps() {
        let mut r = ThemeRegistry::with_builtins();
        assert_eq!(r.cycle(), "retrowave");
        assert_eq!(r.cycle(), "cobalt");
        assert_eq!(r.cycle(), "high-contrast");
        assert_eq!(r.cycle(), "matrix");
    }

    #[test]
    fn token_color_falls_back_to_fg() {
        let t = matrix();
        assert_eq!(t.token_color(TokenKind::Ident), t.fg);
        assert_ne!(t.token_color(TokenKind::Keyword), t.fg);
    }

    #[test]
    fn add_theme() {
        let mut r = ThemeRegistry::with_builtins();
        let mut custom = matrix();
        custom.name = "custom".into();
        r.add(custom);
        assert!(r.set_current("custom"));
    }
}
