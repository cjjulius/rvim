//! Cross-platform terminal setup/teardown.
//!
//! [`TerminalGuard`] enables raw mode and the alternate screen on construction
//! and restores the terminal on drop — so even a panic leaves the user's shell
//! (bash or PowerShell) in a sane state. crossterm handles the Windows and
//! Unix backends transparently.

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use std::io::{self, stdout};

/// RAII guard that owns the terminal's raw/alternate-screen state.
pub struct TerminalGuard {
    mouse: bool,
}

impl TerminalGuard {
    /// Enter raw mode + alternate screen and hide the cursor.
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, Hide)?;
        Ok(Self { mouse: false })
    }

    /// Enable mouse capture (reported as crossterm mouse events).
    pub fn enable_mouse(&mut self) -> io::Result<()> {
        execute!(stdout(), EnableMouseCapture)?;
        self.mouse = true;
        Ok(())
    }

    /// Disable mouse capture.
    pub fn disable_mouse(&mut self) -> io::Result<()> {
        execute!(stdout(), DisableMouseCapture)?;
        self.mouse = false;
        Ok(())
    }

    /// Current terminal size as `(cols, rows)`.
    pub fn size() -> io::Result<(u16, u16)> {
        crossterm::terminal::size()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        if self.mouse {
            let _ = execute!(out, DisableMouseCapture);
        }
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
