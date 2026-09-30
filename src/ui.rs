//! Rendering: the gutter, the syntax-highlighted text area, the status line and
//! the command line. All drawing goes through crossterm's `queue!` for a single
//! buffered flush per frame.

use crate::buffer::Position;
use crate::editor::Editor;
use crate::mode::Mode;
use crate::syntax::{Registry, Token, TokenKind};
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
    /// Screen rows reserved at the very top (1 for the tab bar, else 0).
    pub top_offset: u16,
}

impl Layout {
    pub fn compute(
        cols: u16,
        rows: u16,
        line_count: usize,
        show_numbers: bool,
        show_tabline: bool,
    ) -> Self {
        let gutter_width = gutter_width(line_count, show_numbers);
        let top_offset = if show_tabline { 1 } else { 0 };
        // Reserved rows: tab bar (optional) + status line + command line.
        let text_rows = rows.saturating_sub(2 + top_offset).max(1);
        let text_cols = cols.saturating_sub(gutter_width).max(1);
        Self {
            cols,
            rows,
            gutter_width,
            text_rows,
            text_cols,
            top_offset,
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

/// The gutter cell text for a line: absolute number, or (in relative mode) the
/// distance from the cursor, with the current line left-aligned so it stands
/// out (hybrid line numbers). Includes the trailing separator space.
pub fn gutter_label(row: usize, cursor_row: usize, relative: bool, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let field = width - 1;
    let is_current = row == cursor_row;
    let num = if !relative || is_current {
        row + 1
    } else {
        row.abs_diff(cursor_row)
    };
    let s = num.to_string();
    if relative && is_current {
        format!("{s:<field$} ")
    } else {
        format!("{s:>field$} ")
    }
}

/// Map each character (given as `(byte_offset, char)`) to the token kind that
/// covers it, in a single pass. Assumes `tokens` are ordered by `start` and
/// non-overlapping (as emitted by the tokenizer) — O(chars + tokens).
pub fn char_token_kinds(chars: &[(usize, char)], tokens: &[Token]) -> Vec<Option<TokenKind>> {
    let mut kinds = vec![None; chars.len()];
    let mut ti = 0usize;
    for (ci, (b, _)) in chars.iter().enumerate() {
        while ti < tokens.len() && tokens[ti].end <= *b {
            ti += 1;
        }
        if ti < tokens.len() && tokens[ti].start <= *b {
            kinds[ci] = Some(tokens[ti].kind);
        }
    }
    kinds
}

/// Char-index ranges `[start, end)` of every occurrence of `needle` in `line`.
/// Empty when `needle` is empty. Non-overlapping, left to right.
pub fn search_match_ranges(line: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let nchars = needle.chars().count();
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while let Some(rel) = line[start..].find(needle) {
        let bstart = start + rel;
        let cstart = line[..bstart].chars().count();
        ranges.push((cstart, cstart + nchars));
        start = bstart + needle.len();
    }
    ranges
}

/// One entry in the tab/buffer bar.
pub struct TabEntry {
    pub name: String,
    pub active: bool,
    pub dirty: bool,
}

/// The label for a tab: ` <n> <basename>[+] `.
pub fn tab_label(index: usize, name: &str, dirty: bool) -> String {
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let mark = if dirty { "+" } else { "" };
    format!(" {index} {base}{mark} ")
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
    tabs: &[TabEntry],
) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    let show_tabline = tabs.len() > 1;
    let layout = Layout::compute(
        cols,
        rows,
        editor.buffer.line_count(),
        editor.show_line_numbers,
        show_tabline,
    );

    queue!(out, Hide, MoveTo(0, 0))?;

    if show_tabline {
        draw_tabline(out, theme, &layout, tabs)?;
    }

    let sel = editor.selection();
    let linewise = editor.mode == Mode::VisualLine;
    let search = if editor.hlsearch && !editor.search_query().is_empty() {
        Some(editor.search_query())
    } else {
        None
    };

    // Carry block-comment state from the top of the buffer to the first visible
    // line, then thread it through the visible rows.
    let mut in_block =
        syntax.block_state_at(editor.language, editor.buffer.lines(), editor.top);

    for y in 0..layout.text_rows {
        let row = editor.top + y as usize;
        queue!(out, MoveTo(0, layout.top_offset + y))?;
        draw_gutter(out, editor, theme, &layout, row)?;

        let line_bg = if row == editor.cursor.row {
            theme.cursor_line_bg
        } else {
            theme.bg
        };

        if let Some(line) = editor.buffer.line(row) {
            let (tokens, next_block) =
                syntax.highlight_stateful(editor.language, line, in_block);
            in_block = next_block;
            draw_text_line(
                out, line, &tokens, theme, &layout, editor.left, row, line_bg, sel, linewise,
                search,
            )?;
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
        let cy = layout.top_offset + (editor.cursor.row.saturating_sub(editor.top)) as u16;
        queue!(
            out,
            MoveTo(
                cx.min(layout.cols.saturating_sub(1)),
                cy.min(layout.top_offset + layout.text_rows - 1)
            ),
            Show
        )?;
    }

    out.flush()
}

fn draw_tabline(
    out: &mut impl Write,
    theme: &Theme,
    layout: &Layout,
    tabs: &[TabEntry],
) -> io::Result<()> {
    queue!(out, MoveTo(0, 0))?;
    let mut used = 0usize;
    let total = layout.cols as usize;
    for (i, tab) in tabs.iter().enumerate() {
        let label = tab_label(i + 1, &tab.name, tab.dirty);
        let (fg, bg) = if tab.active {
            (theme.mode_fg, theme.mode_bg)
        } else {
            (theme.status_fg, theme.status_bg)
        };
        let shown: String = label.chars().take(total.saturating_sub(used)).collect();
        if shown.is_empty() {
            break;
        }
        used += shown.chars().count();
        queue!(out, SetForegroundColor(fg), SetBackgroundColor(bg), Print(shown))?;
    }
    // Fill the rest of the tab bar.
    if used < total {
        queue!(
            out,
            SetForegroundColor(theme.status_fg),
            SetBackgroundColor(theme.status_bg),
            Print(" ".repeat(total - used))
        )?;
    }
    queue!(out, ResetColor)
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
        let text = gutter_label(
            row,
            editor.cursor.row,
            editor.relative_numbers,
            layout.gutter_width as usize,
        );
        (fg, text)
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
    tokens: &[Token],
    theme: &Theme,
    layout: &Layout,
    left: usize,
    row: usize,
    line_bg: Color,
    sel: Option<(Position, Position)>,
    linewise: bool,
    search: Option<&str>,
) -> io::Result<()> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let matches = search.map(|n| search_match_ranges(line, n)).unwrap_or_default();

    // Per-char foreground based on tokens (single pass).
    let kinds = char_token_kinds(&chars, tokens);
    let mut fg = vec![theme.fg; chars.len()];
    for (ci, kind) in kinds.iter().enumerate() {
        if let Some(k) = kind {
            fg[ci] = theme.token_color(*k);
        }
    }

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
        let in_match = matches.iter().any(|&(s, e)| ci >= s && ci < e);
        // Priority: selection > search match > line background.
        let bg = if selected {
            theme.selection_bg
        } else if in_match {
            theme.search_bg
        } else {
            line_bg
        };
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
    let rec = match editor.recording_register() {
        Some(r) => format!("recording @{r}  "),
        None => String::new(),
    };
    let mid = format!(" {file}{dirty} ");
    let right = format!(" {rec}{lang} | {pos} | {pct}% ");

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
    fn search_match_ranges_finds_all() {
        assert_eq!(search_match_ranges("a bar b bar", "bar"), vec![(2, 5), (8, 11)]);
        assert_eq!(search_match_ranges("no hits here", "xyz"), vec![]);
        assert_eq!(search_match_ranges("anything", ""), vec![]);
    }

    #[test]
    fn search_match_ranges_overlapping_are_non_overlapping() {
        // "aaaa" searching "aa" yields non-overlapping matches at 0 and 2.
        assert_eq!(search_match_ranges("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn char_token_kinds_maps_and_leaves_gaps() {
        // "ab cd": token covers bytes 0..2 (Keyword) and 3..5 (Number).
        let chars: Vec<(usize, char)> = "ab cd".char_indices().collect();
        let tokens = vec![
            Token::new(0, 2, TokenKind::Keyword),
            Token::new(3, 5, TokenKind::Number),
        ];
        let kinds = char_token_kinds(&chars, tokens.as_slice());
        assert_eq!(kinds[0], Some(TokenKind::Keyword));
        assert_eq!(kinds[1], Some(TokenKind::Keyword));
        assert_eq!(kinds[2], None); // the space
        assert_eq!(kinds[3], Some(TokenKind::Number));
        assert_eq!(kinds[4], Some(TokenKind::Number));
    }

    #[test]
    fn char_token_kinds_empty_tokens() {
        let chars: Vec<(usize, char)> = "abc".char_indices().collect();
        let kinds = char_token_kinds(&chars, &[]);
        assert_eq!(kinds, vec![None, None, None]);
    }

    #[test]
    fn gutter_label_absolute() {
        // width 4 => 3-char field + trailing space
        assert_eq!(gutter_label(0, 5, false, 4), "  1 ");
        assert_eq!(gutter_label(41, 0, false, 4), " 42 ");
        assert_eq!(gutter_label(9, 0, false, 0), "");
    }

    #[test]
    fn gutter_label_relative_hybrid() {
        // current line shows absolute, left-aligned
        assert_eq!(gutter_label(5, 5, true, 4), "6   ");
        // other lines show distance, right-aligned
        assert_eq!(gutter_label(2, 5, true, 4), "  3 ");
        assert_eq!(gutter_label(8, 5, true, 4), "  3 ");
    }

    #[test]
    fn layout_reserves_two_rows() {
        let l = Layout::compute(80, 24, 10, true, false);
        assert_eq!(l.text_rows, 22);
        assert_eq!(l.gutter_width, 4);
        assert_eq!(l.text_cols, 76);
        assert_eq!(l.top_offset, 0);
    }

    #[test]
    fn layout_reserves_tabline_row() {
        let l = Layout::compute(80, 24, 10, true, true);
        assert_eq!(l.top_offset, 1);
        assert_eq!(l.text_rows, 21); // one fewer for the tab bar
    }

    #[test]
    fn tab_label_uses_basename_and_dirty_marker() {
        assert_eq!(tab_label(1, "src/main.rs", false), " 1 main.rs ");
        assert_eq!(tab_label(2, "notes.txt", true), " 2 notes.txt+ ");
        assert_eq!(tab_label(3, "[No Name]", false), " 3 [No Name] ");
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
