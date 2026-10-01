//! The text buffer: line storage, edit primitives, and undo/redo.
//!
//! Columns are tracked as *character* indices (not bytes) so multi-byte
//! UTF-8 content behaves sensibly; conversion to byte offsets happens only at
//! the point of mutation.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A cursor / edit location. `row` and `col` are both zero-based; `col` is a
/// character index into the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor: Position,
}

/// An in-memory text buffer with undo/redo history.
#[derive(Debug, Clone)]
pub struct Buffer {
    lines: Vec<String>,
    path: Option<PathBuf>,
    dirty: bool,
    /// Monotonic counter bumped on every content mutation; used to detect that
    /// a change occurred (e.g. for the `.` repeat command).
    revision: u64,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    /// An empty buffer holding a single empty line.
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            path: None,
            dirty: false,
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Build a buffer from raw text, splitting on `\n` (a trailing `\r` per line
    /// is stripped so Windows CRLF files load cleanly).
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        // A trailing newline yields a final empty element; keep exactly one.
        Self {
            lines,
            path: None,
            dirty: false,
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Load a buffer from disk. A nonexistent path yields an empty buffer whose
    /// `path` is set, so `:w` will create the file.
    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut buf = if path.exists() {
            let text = fs::read_to_string(path)?;
            Buffer::from_text(&text)
        } else {
            Buffer::new()
        };
        buf.path = Some(path.to_path_buf());
        buf.dirty = false;
        Ok(buf)
    }

    /// The file path backing this buffer, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Set/replace the backing path (used by `:w <name>`).
    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The current content revision (bumped on every mutation).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Mark the buffer modified and bump the revision counter.
    fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Number of lines (always >= 1).
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Borrow a line by row index.
    pub fn line(&self, row: usize) -> Option<&str> {
        self.lines.get(row).map(|s| s.as_str())
    }

    /// All lines (read-only) — used by the renderer and highlighters.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Character length of a line (0 for out-of-range).
    pub fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// Serialize the buffer to a single string with `\n` separators.
    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Save to the current path. Returns the number of bytes written.
    pub fn save(&mut self) -> io::Result<usize> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::other("no file name"))?;
        self.save_as(&path)
    }

    /// Save to a specific path and adopt it as the backing path.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> io::Result<usize> {
        let mut text = self.to_text();
        text.push('\n'); // POSIX-friendly trailing newline
        fs::write(path.as_ref(), &text)?;
        self.path = Some(path.as_ref().to_path_buf());
        self.dirty = false;
        Ok(text.len())
    }

    // ---- undo/redo -------------------------------------------------------

    /// Record the current state so it can be restored by `undo`. Call this
    /// once before a logical edit. Clears the redo stack.
    pub fn checkpoint(&mut self, cursor: Position) {
        self.undo_stack.push(Snapshot {
            lines: self.lines.clone(),
            cursor,
        });
        self.redo_stack.clear();
        // Bound history to keep memory in check.
        const MAX_HISTORY: usize = 1000;
        if self.undo_stack.len() > MAX_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the most recent checkpointed change. Returns the cursor position to
    /// restore, or `None` if there is nothing to undo.
    pub fn undo(&mut self, current_cursor: Position) -> Option<Position> {
        let snap = self.undo_stack.pop()?;
        self.redo_stack.push(Snapshot {
            lines: self.lines.clone(),
            cursor: current_cursor,
        });
        self.lines = snap.lines;
        self.touch();
        Some(snap.cursor)
    }

    /// Redo a previously undone change.
    pub fn redo(&mut self, current_cursor: Position) -> Option<Position> {
        let snap = self.redo_stack.pop()?;
        self.undo_stack.push(Snapshot {
            lines: self.lines.clone(),
            cursor: current_cursor,
        });
        self.lines = snap.lines;
        self.touch();
        Some(snap.cursor)
    }

    // ---- editing primitives ---------------------------------------------

    fn byte_index(line: &str, col: usize) -> usize {
        line.char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    }

    /// Insert a single character at `pos`.
    pub fn insert_char(&mut self, pos: Position, ch: char) {
        if let Some(line) = self.lines.get_mut(pos.row) {
            let bi = Self::byte_index(line, pos.col);
            line.insert(bi, ch);
            self.touch();
        }
    }

    /// Insert a string (no newlines) at `pos`.
    pub fn insert_str(&mut self, pos: Position, text: &str) {
        if let Some(line) = self.lines.get_mut(pos.row) {
            let bi = Self::byte_index(line, pos.col);
            line.insert_str(bi, text);
            self.touch();
        }
    }

    /// Delete the character at `pos`, returning it if present.
    pub fn delete_char(&mut self, pos: Position) -> Option<char> {
        let line = self.lines.get_mut(pos.row)?;
        let bi = Self::byte_index(line, pos.col);
        if bi >= line.len() {
            return None;
        }
        let ch = line[bi..].chars().next()?;
        line.remove(bi);
        self.touch();
        Some(ch)
    }

    /// Split the line at `pos`, moving the remainder down to a new line.
    pub fn split_line(&mut self, pos: Position) {
        if pos.row >= self.lines.len() {
            return;
        }
        let bi = Self::byte_index(&self.lines[pos.row], pos.col);
        let rest = self.lines[pos.row].split_off(bi);
        self.lines.insert(pos.row + 1, rest);
        self.touch();
    }

    /// Join `row + 1` onto the end of `row` (with a single space, vim-style),
    /// returning true if a join happened.
    pub fn join_line(&mut self, row: usize) -> bool {
        if row + 1 >= self.lines.len() {
            return false;
        }
        let next = self.lines.remove(row + 1);
        let trimmed = next.trim_start();
        let cur = &mut self.lines[row];
        if !cur.is_empty() && !trimmed.is_empty() {
            cur.push(' ');
        }
        cur.push_str(trimmed);
        self.touch();
        true
    }

    /// Join `row + 1` onto `row` with no space inserted (vim `gJ`), returning
    /// true if a join happened.
    pub fn join_line_raw(&mut self, row: usize) -> bool {
        if row + 1 >= self.lines.len() {
            return false;
        }
        let next = self.lines.remove(row + 1);
        self.lines[row].push_str(&next);
        self.touch();
        true
    }

    /// Insert a whole line at `row`.
    pub fn insert_line(&mut self, row: usize, text: impl Into<String>) {
        let row = row.min(self.lines.len());
        self.lines.insert(row, text.into());
        self.touch();
    }

    /// Delete a whole line, returning its contents. The buffer always keeps at
    /// least one (empty) line.
    pub fn delete_line(&mut self, row: usize) -> Option<String> {
        if row >= self.lines.len() {
            return None;
        }
        let removed = self.lines.remove(row);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.touch();
        Some(removed)
    }

    /// Replace the single character at `pos` with `ch` (used by `r`). No-op if
    /// the position is past the end of the line.
    pub fn replace_char(&mut self, pos: Position, ch: char) {
        if let Some(line) = self.lines.get_mut(pos.row) {
            let bi = Self::byte_index(line, pos.col);
            if bi < line.len() {
                let removed_len = line[bi..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
                line.replace_range(bi..bi + removed_len, &ch.to_string());
                self.touch();
            }
        }
    }

    /// Replace an entire line's contents.
    pub fn set_line(&mut self, row: usize, text: impl Into<String>) {
        if let Some(line) = self.lines.get_mut(row) {
            *line = text.into();
            self.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_has_one_empty_line() {
        let b = Buffer::new();
        assert_eq!(b.line_count(), 1);
        assert_eq!(b.line(0), Some(""));
        assert!(!b.is_dirty());
    }

    #[test]
    fn from_text_splits_lines_and_strips_cr() {
        let b = Buffer::from_text("alpha\r\nbeta\ngamma");
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line(0), Some("alpha"));
        assert_eq!(b.line(1), Some("beta"));
        assert_eq!(b.line(2), Some("gamma"));
    }

    #[test]
    fn insert_and_delete_char() {
        let mut b = Buffer::from_text("hi");
        b.insert_char(Position::new(0, 1), 'X');
        assert_eq!(b.line(0), Some("hXi"));
        assert!(b.is_dirty());
        let removed = b.delete_char(Position::new(0, 1));
        assert_eq!(removed, Some('X'));
        assert_eq!(b.line(0), Some("hi"));
    }

    #[test]
    fn insert_char_multibyte_column() {
        let mut b = Buffer::from_text("héllo");
        // Insert after the 'é' (char col 2).
        b.insert_char(Position::new(0, 2), 'X');
        assert_eq!(b.line(0), Some("héXllo"));
    }

    #[test]
    fn split_and_join() {
        let mut b = Buffer::from_text("hello world");
        b.split_line(Position::new(0, 5));
        assert_eq!(b.line(0), Some("hello"));
        assert_eq!(b.line(1), Some(" world"));
        assert!(b.join_line(0));
        assert_eq!(b.line(0), Some("hello world"));
    }

    #[test]
    fn delete_line_keeps_one() {
        let mut b = Buffer::from_text("only");
        assert_eq!(b.delete_line(0).as_deref(), Some("only"));
        assert_eq!(b.line_count(), 1);
        assert_eq!(b.line(0), Some(""));
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut b = Buffer::from_text("abc");
        let cur = Position::new(0, 0);
        b.checkpoint(cur);
        b.insert_char(Position::new(0, 0), 'Z');
        assert_eq!(b.line(0), Some("Zabc"));
        let restored = b.undo(Position::new(0, 1));
        assert_eq!(restored, Some(cur));
        assert_eq!(b.line(0), Some("abc"));
        let redone = b.redo(Position::new(0, 0));
        assert_eq!(redone, Some(Position::new(0, 1)));
        assert_eq!(b.line(0), Some("Zabc"));
    }

    #[test]
    fn checkpoint_clears_redo() {
        let mut b = Buffer::from_text("a");
        b.checkpoint(Position::default());
        b.insert_char(Position::new(0, 1), 'b');
        b.undo(Position::new(0, 2));
        // New edit should invalidate redo.
        b.checkpoint(Position::default());
        b.insert_char(Position::new(0, 1), 'c');
        assert!(b.redo(Position::default()).is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rvim_test_{}.txt", std::process::id()));
        let mut b = Buffer::from_text("line1\nline2");
        b.save_as(&path).unwrap();
        let loaded = Buffer::from_file(&path).unwrap();
        assert_eq!(loaded.line(0), Some("line1"));
        assert_eq!(loaded.line(1), Some("line2"));
        assert!(!loaded.is_dirty());
        let _ = std::fs::remove_file(&path);
    }
}
