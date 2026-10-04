//! System clipboard access for the `"+` and `"*` registers.
//!
//! A yank to `"+y` (or `"*y`) lands in the operating system clipboard, and
//! `"+p` pastes it back, so rvim shares a clipboard with other applications.
//! rvim shells out to the platform's clipboard utility rather than linking a
//! native dependency, keeping the build lean and the behavior easy to reason
//! about.
//!
//! On X11/Wayland the two registers map to different selections: `"+` is the
//! system CLIPBOARD and `"*` is the PRIMARY selection (what middle-click pastes).
//! On Windows and macOS there is only one clipboard, so both behave the same.
//!
//! When the environment variable `RVIM_CLIPBOARD` names a file, that file is used
//! as the clipboard instead of the OS (PRIMARY uses the same path plus a
//! `.primary` suffix). This is a headless fallback for machines without a
//! clipboard tool, and it is what the tests exercise so they never touch the
//! real clipboard.

use std::io::Write;
use std::process::{Command, Stdio};

/// Which selection a `"+` / `"*` register maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sel {
    /// The system clipboard (`"+`).
    Clipboard,
    /// The X11/Wayland PRIMARY selection (`"*`); the clipboard elsewhere.
    Primary,
}

/// File-backed fallback path for `sel`, if `RVIM_CLIPBOARD` is set.
fn fallback_path(sel: Sel) -> Option<std::path::PathBuf> {
    let base = std::env::var_os("RVIM_CLIPBOARD")?;
    let mut path = std::path::PathBuf::from(base);
    if sel == Sel::Primary {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".primary");
        path.set_file_name(name);
    }
    Some(path)
}

/// Read a selection. Returns `None` when no clipboard tool is available or the
/// read fails, so callers can fall back to an internal register.
pub fn read(sel: Sel) -> Option<String> {
    if let Some(path) = fallback_path(sel) {
        return std::fs::read_to_string(path).ok();
    }
    for (cmd, args) in read_commands(sel) {
        if let Some(text) = run_read(cmd, &args) {
            return Some(text);
        }
    }
    None
}

/// Write `text` to a selection. Returns `false` when no clipboard tool is
/// available or the write fails.
pub fn write(sel: Sel, text: &str) -> bool {
    if let Some(path) = fallback_path(sel) {
        return std::fs::write(path, text).is_ok();
    }
    for (cmd, args) in write_commands(sel) {
        if run_write(cmd, &args, text) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn read_commands(_sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("powershell", vec!["-NoProfile", "-Command", "Get-Clipboard"])]
}

#[cfg(target_os = "windows")]
fn write_commands(_sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("clip", vec![])]
}

#[cfg(target_os = "macos")]
fn read_commands(_sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("pbpaste", vec![])]
}

#[cfg(target_os = "macos")]
fn write_commands(_sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("pbcopy", vec![])]
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn read_commands(sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    match sel {
        Sel::Clipboard => vec![
            ("wl-paste", vec!["--no-newline"]),
            ("xclip", vec!["-selection", "clipboard", "-o"]),
            ("xsel", vec!["--clipboard", "--output"]),
        ],
        Sel::Primary => vec![
            ("wl-paste", vec!["--primary", "--no-newline"]),
            ("xclip", vec!["-selection", "primary", "-o"]),
            ("xsel", vec!["--primary", "--output"]),
        ],
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn write_commands(sel: Sel) -> Vec<(&'static str, Vec<&'static str>)> {
    match sel {
        Sel::Clipboard => vec![
            ("wl-copy", vec![]),
            ("xclip", vec!["-selection", "clipboard"]),
            ("xsel", vec!["--clipboard", "--input"]),
        ],
        Sel::Primary => vec![
            ("wl-copy", vec!["--primary"]),
            ("xclip", vec!["-selection", "primary"]),
            ("xsel", vec!["--primary", "--input"]),
        ],
    }
}

fn run_read(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // Normalize Windows line endings so clipboard text matches buffer text.
    Some(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

fn run_write(cmd: &str, args: &[&str], text: &str) -> bool {
    let child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() {
            return false;
        }
    }
    matches!(child.wait(), Ok(status) if status.success())
}
