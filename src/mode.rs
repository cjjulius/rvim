//! The editor's modal state.

use std::fmt;

/// The vim-style modes rvim supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Motions and operators (the default).
    #[default]
    Normal,
    /// Text entry.
    Insert,
    /// Overtype entry (`R`): typing overwrites existing characters.
    Replace,
    /// Character-wise selection.
    Visual,
    /// Line-wise selection.
    VisualLine,
    /// Block (rectangular) selection.
    VisualBlock,
    /// The `:` ex command line.
    Command,
}

impl Mode {
    /// A short uppercase label shown in the status line.
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Replace => "REPLACE",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
            Mode::VisualBlock => "V-BLOCK",
            Mode::Command => "COMMAND",
        }
    }

    /// Whether this mode is one of the visual selection modes.
    pub fn is_visual(&self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable() {
        assert_eq!(Mode::Normal.label(), "NORMAL");
        assert_eq!(Mode::Insert.label(), "INSERT");
        assert_eq!(Mode::VisualLine.label(), "V-LINE");
    }

    #[test]
    fn visual_detection() {
        assert!(Mode::Visual.is_visual());
        assert!(Mode::VisualLine.is_visual());
        assert!(!Mode::Normal.is_visual());
        assert!(!Mode::Insert.is_visual());
    }

    #[test]
    fn default_is_normal() {
        assert_eq!(Mode::default(), Mode::Normal);
    }
}
