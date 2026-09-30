//! Rendering: the gutter, the syntax-highlighted text area, the status line and
//! the command line. All drawing goes through crossterm's `queue!` for a single
//! buffered flush per frame.

use crate::buffer::Position;
use crate::editor::Editor;
use crate::mode::Mode;
use crate::syntax::Registry;
use crate::theme::Theme;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style::{
    Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{Clear, ClearType};
use crossterm::queue;
use std::io::{self, Write};

/// The full terminal layout for a given size.
pub struct Layout {
    pub cols: u16,
    pub rows: u16,
    pub gutter_width: u16,
    pub text_rows: u16,
    pub text_cols: u16,
}

impl Layout {
    pub fn compute(cols: u16, rows: u16, line_count: usize, show_numbers: bool) -> Self {
        let gutter_width = gutter_width(line_count, show_numbers);
        // Two reserved rows: status line + command line.
        let text_rows = rows.saturating_sub(2).max(1);
        let text_cols = cols.saturating_sub(gutter_width).max(1);
        Self {
            cols,
            rows,
            gutter_width,
            text_rows,
            text_cols,
        }
    }
}

/// Width of the line-number gutter (0 when disabled).
pub fn gutter_width(line_count: usize, show_numbers: bool) -> u16 {
    if !show_numbers {
        return 0;
    }
    let digits = line_count.max(1).to_string().len() as u16;
    digits.max(3) + 1
}

/// Whether `(row, col)` lies within the (inclusive) selection.
pub fn in_selection(sel: (Position, Position), linewise: bool, row: usize, col: usize) -> bool {
    let (s, e) = sel;
    if row < s.row || row > e.row {
        return false;
    }
    if linewise {
        return true;
    }
    if s.row == e.row {
        col >= s.col && col <= e.col
    } else if row == s.row {
        col >= s.col
    } else if row == e.row {
        col <= e.col
    } else {
        true
    }
}

/// Draw one full frame.
pub fn render(
    out: &mut impl Write,
    editor: &Editor,
    theme: &Theme,
    syntax: &Registry,
) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    let layout = Layout::compute(cols, rows, editor.buffer.line_count(), editor.show_line_numbers);

    queue!(out, Hide, MoveTo(0, 0))?;

    let sel = editor.selection();
    let linewise = editor.mode == Mode::VisualLine;

    for y in 0..layout.text_rows {
        let row = editor.top + y as usize;
        queue!(out, MoveTo(0, y))?;
        draw_gutter(out, editor, theme, &layout, row)?;

        let line_bg = if row == editor.cursor.row {
            theme.cursor_line_bg
        } else {
            theme.bg
        };

        if let Some(line) = editor.buffer.line(row) {
            draw_text_line(out, line, editor, theme, syntax, &layout, row, line_bg, sel, linewise)?;
        } else {
            // Past end of buffer: tilde like vim.
            queue!(
                out,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.gutter_fg),
                Print("~"),
                SetBackgroundColor(theme.bg),
                Print(" ".repeat(layout.text_cols.saturating_sub(1) as usize)),
            )?;
        }
        queue!(out, ResetColor)?;
    }

    draw_status_line(out, editor, theme, &layout)?;
    draw_command_line(out, editor, theme, &layout)?;

    // Place the real cursor.
    if editor.mode == Mode::Command {
        let x = 1 + editor.cmdline.chars().count() as u16; // after ':' or '/'
        queue!(out, MoveTo(x.min(layout.cols.saturating_sub(1)), layout.rows - 1), Show)?;
    } else {
        let cx = layout.gutter_width + (editor.cursor.col.saturating_sub(editor.left)) as u16;
        let cy = (editor.cursor.row.saturating_sub(editor.top)) as u16;
        queue!(
            out,
            MoveTo(cx.min(layout.cols.saturating_sub(1)), cy.min(layout.text_rows - 1)),
            Show
        )?;
    }

    out.flush()
}

fn draw_gutter(
    out: &mut impl Write,
    editor: &Editor,
    theme: &Theme,
    layout: &Layout,
    row: usize,
) -> io::Result<()> {
    if layout.gutter_width == 0 {
        return Ok(());
    }
    let within = row < editor.buffer.line_count();
    let (fg, text) = if within {
        let fg = if row == editor.cursor.row {
            theme.current_line_nr_fg
        } else {
            theme.gutter_fg
        };
        let num = (row + 1).to_string();
        let pad = (layout.gutter_width as usize).saturating_sub(num.len() + 1);
        (fg, format!("{}{} ", " ".repeat(pad), num))
    } else {
        (theme.gutter_fg, " ".repeat(layout.gutter_width as usize))
    };
    queue!(
        out,
        SetBackgroundColor(theme.gutter_bg),
        SetForegroundColor(fg),
        Print(text)
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_text_line(
    out: &mut impl Write,
    line: &str,
    editor: &Editor,
    theme: &Theme,
    syntax: &Registry,
    layout: &Layout,
    row: usize,
    line_bg: Color,
    sel: Option<(Position, Position)>,
    linewise: bool,
) -> io::Result<()> {
    let tokens = syntax.highlight(editor.language, line);
    let chars: Vec<(usize, char)> = line.char_indices().collect();

    // Per-char foreground based on tokens.
    let mut fg = vec![theme.fg; chars.len()];
    for tok in &tokens {
        let color = theme.token_color(tok.kind);
        for (ci, (b, _)) in chars.iter().enumerate() {
            if *b >= tok.start && *b < tok.end {
                fg[ci] = color;
            }
        }
    }

    let left = editor.left;
    let width = layout.text_cols as usize;

    // Batch consecutive characters that share (fg, bg) into runs.
    let mut runs: Vec<(Color, Color, String)> = Vec::new();
    let mut run = String::new();
    let mut run_fg = theme.fg;
    let mut run_bg = line_bg;
    let mut started = false;
    let mut printed = 0usize;

    for ci in left..chars.len() {
        if printed >= width {
            break;
        }
        let ch = chars[ci].1;
        let selected = sel
            .map(|s| in_selection(s, linewise, row, ci))
            .unwrap_or(false);
        let bg = if selected { theme.selection_bg } else { line_bg };
        let cfg = fg[ci];
        if !started {
            run_fg = cfg;
            run_bg = bg;
            started = true;
        } else if cfg != run_fg || bg != run_bg {
            runs.push((run_fg, run_bg, std::mem::take(&mut run)));
            run_fg = cfg;
            run_bg = bg;
        }
        // Render tabs as spaces for alignment.
        if ch == '\t' {
            run.push_str("    ");
            printed += 4;
        } else {
            run.push(ch);
            printed += 1;
        }
    }
    if !run.is_empty() {
        runs.push((run_fg, run_bg, run));
    }

    for (f, b, text) in runs {
        queue!(out, SetForegroundColor(f), SetBackgroundColor(b), Print(text))?;
    }

    // Pad the rest of the row with the line background.
    if printed < width {
        queue!(
            out,
            SetBackgroundColor(line_bg),
            Print(" ".repeat(width - printed))
        )?;
    }
    Ok(())
}

fn draw_status_line(
    out: &mut impl Write,
    editor: &Editor,
    theme: &Theme,
    layout: &Layout,
) -> io::Result<()> {
    let y = layout.rows - 2;
    queue!(out, MoveTo(0, y))?;

    let mode = editor.mode.label();
    let file = editor
        .buffer
        .path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "[No Name]".to_string());
    let dirty = if editor.buffer.is_dirty() { " [+]" } else { "" };
    let lang = editor.language.name();
    let pos = format!("{}:{}", editor.cursor.row + 1, editor.cursor.col + 1);
    let pct = {
        let total = editor.buffer.line_count();
        ((editor.cursor.row + 1) * 100 / total.max(1)).min(100)
    };

    let left = format!(" {mode} ");
    let mid = format!(" {file}{dirty} ");
    let right = format!(" {lang} | {pos} | {pct}% ");

    let total_w = layout.cols as usize;
    let used = left.chars().count() + mid.chars().count() + right.chars().count();
    let fill = total_w.saturating_sub(used);

    queue!(
        out,
        SetBackgroundColor(theme.mode_bg),
        SetForegroundColor(theme.mode_fg),
        Print(left),
        SetBackgroundColor(theme.status_bg),
        SetForegroundColor(theme.status_fg),
        Print(mid),
        Print(" ".repeat(fill)),
        Print(right),
        ResetColor
    )
}

fn draw_command_line(
    out: &mut impl Write,
    editor: &Editor,
    theme: &Theme,
    layout: &Layout,
) -> io::Result<()> {
    let y = layout.rows - 1;
    queue!(
        out,
        MoveTo(0, y),
        SetBackgroundColor(theme.bg),
        SetForegroundColor(theme.message_fg),
        Clear(ClearType::CurrentLine)
    )?;

    let content = if editor.mode == Mode::Command {
        format!("{}{}", editor.cmdline_prefix(), editor.cmdline)
    } else {
        editor.message.clone()
    };
    let content: String = content.chars().take(layout.cols as usize).collect();
    queue!(out, Print(content), ResetColor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_scales_with_line_count() {
        assert_eq!(gutter_width(5, true), 4); // min 3 digits + 1
        assert_eq!(gutter_width(1000, true), 5); // 4 digits + 1
        assert_eq!(gutter_width(100000, true), 7); // 6 digits + 1
        assert_eq!(gutter_width(42, false), 0);
    }

    #[test]
    fn layout_reserves_two_rows() {
        let l = Layout::compute(80, 24, 10, true);
        assert_eq!(l.text_rows, 22);
        assert_eq!(l.gutter_width, 4);
        assert_eq!(l.text_cols, 76);
    }

    #[test]
    fn selection_charwise_single_line() {
        let sel = (Position::new(0, 2), Position::new(0, 5));
        assert!(!in_selection(sel, false, 0, 1));
        assert!(in_selection(sel, false, 0, 2));
        assert!(in_selection(sel, false, 0, 5));
        assert!(!in_selection(sel, false, 0, 6));
    }

    #[test]
    fn selection_linewise_covers_whole_rows() {
        let sel = (Position::new(1, 3), Position::new(3, 0));
        assert!(in_selection(sel, true, 1, 0));
        assert!(in_selection(sel, true, 2, 99));
        assert!(!in_selection(sel, true, 0, 0));
        assert!(!in_selection(sel, true, 4, 0));
    }

    #[test]
    fn selection_multiline_charwise_edges() {
        let sel = (Position::new(1, 4), Position::new(3, 2));
        assert!(!in_selection(sel, false, 1, 3));
        assert!(in_selection(sel, false, 1, 4));
        assert!(in_selection(sel, false, 2, 100)); // middle line fully covered
        assert!(in_selection(sel, false, 3, 2));
        assert!(!in_selection(sel, false, 3, 3));
    }
}
