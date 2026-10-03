//! System clipboard access for the `"+` and `"*` registers.
//!
//! A yank to `"+y` (or `"*y`) lands in the operating system clipboard, and
//! `"+p` pastes it back, so rvim shares a clipboard with other applications.
//! rvim shells out to the platform's clipboard utility rather than linking a
//! native dependency, keeping the build lean and the behavior easy to reason
//! about.
//!
//! When the environment variable `RVIM_CLIPBOARD` names a file, that file is
//! used as the clipboard instead of the OS. This is a headless fallback for
//! machines without a clipboard tool, and it is what the tests exercise so
//! they never touch the real clipboard.

use std::io::Write;
use std::process::{Command, Stdio};

/// Read the clipboard. Returns `None` when no clipboard tool is available or
/// the read fails, so callers can fall back to an internal register.
pub fn read() -> Option<String> {
    if let Some(path) = std::env::var_os("RVIM_CLIPBOARD") {
        return std::fs::read_to_string(path).ok();
    }
    for (cmd, args) in read_commands() {
        if let Some(text) = run_read(cmd, &args) {
            return Some(text);
        }
    }
    None
}

/// Write `text` to the clipboard. Returns `false` when no clipboard tool is
/// available or the write fails.
pub fn write(text: &str) -> bool {
    if let Some(path) = std::env::var_os("RVIM_CLIPBOARD") {
        return std::fs::write(path, text).is_ok();
    }
    for (cmd, args) in write_commands() {
        if run_write(cmd, &args, text) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn read_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("powershell", vec!["-NoProfile", "-Command", "Get-Clipboard"])]
}

#[cfg(target_os = "windows")]
fn write_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("clip", vec![])]
}

#[cfg(target_os = "macos")]
fn read_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("pbpaste", vec![])]
}

#[cfg(target_os = "macos")]
fn write_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("pbcopy", vec![])]
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn read_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("wl-paste", vec!["--no-newline"]),
        ("xclip", vec!["-selection", "clipboard", "-o"]),
        ("xsel", vec!["--clipboard", "--output"]),
    ]
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn write_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ]
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
