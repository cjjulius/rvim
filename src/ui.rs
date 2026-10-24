//! Rendering: the gutter, the syntax-highlighted text area, the status line and
//! the command line. All drawing goes through crossterm's `queue!` for a single
//! buffered flush per frame.

use crate::buffer::{Buffer, Position};
use crate::editor::Editor;
use crate::menu::{MenuAction, MenuItem, MenuState};
use crate::mode::Mode;
use crate::syntax::{LineState, Registry, Token, TokenKind};
use crate::theme::Theme;
use crossterm::cursor::{Hide, MoveTo, SetCursorStyle, Show};
use crossterm::style::{
    Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
};
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
    /// Columns of the gutter used by the fold-indicator column (`:set
    /// foldcolumn`); these are the leftmost `fold_width` cells of `gutter_width`.
    pub fold_width: u16,
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
            fold_width: 0,
        }
    }

    /// Reserve `fc` fold-indicator columns at the left of the gutter, shrinking
    /// the text area to match. Returns the adjusted layout.
    pub fn with_foldcolumn(mut self, fc: u16) -> Self {
        self.fold_width = fc;
        self.gutter_width = self.gutter_width.saturating_add(fc);
        // Never let the gutter consume the whole width (leaves room for text and
        // keeps draw/cursor/mouse from spilling past the window).
        let max_gutter = self.cols.saturating_sub(1);
        if self.gutter_width > max_gutter {
            self.gutter_width = max_gutter;
            self.fold_width = self.fold_width.min(self.gutter_width);
        }
        self.text_cols = self.cols.saturating_sub(self.gutter_width).max(1);
        self
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

/// The cursor shape for a mode: a bar while inserting, an underline while
/// replacing, a block otherwise (vim's familiar shapes). Terminals that don't
/// understand DECSCUSR ignore it.
fn mode_cursor_style(mode: Mode) -> SetCursorStyle {
    match mode {
        Mode::Insert => SetCursorStyle::SteadyBar,
        Mode::Replace => SetCursorStyle::SteadyUnderScore,
        _ => SetCursorStyle::SteadyBlock,
    }
}

/// Draw one full frame.
pub fn render(
    out: &mut impl Write,
    editor: &Editor,
    buf: &Buffer,
    theme: &Theme,
    syntax: &Registry,
    tabs: &[TabEntry],
    top_state: LineState,
) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    let show_tabline = tabs.len() > 1;
    let layout = Layout::compute(
        cols,
        rows,
        buf.line_count(),
        editor.show_line_numbers,
        show_tabline,
    )
    .with_foldcolumn(editor.foldcolumn as u16);

    // Bracket the whole frame in a synchronized update (DEC mode 2026). On
    // supporting terminals (Windows Terminal, modern xterms) every cell we draw
    // this frame — the text area *and* the menu overlay on top of it — is
    // presented atomically, so the menu never flashes over a half-drawn buffer.
    // Terminals that don't understand the sequence simply ignore it.
    queue!(out, BeginSynchronizedUpdate, Hide, MoveTo(0, 0))?;

    if show_tabline {
        draw_tabline(out, theme, &layout, tabs)?;
    }

    // Draw the gutter + text area (fold-aware); returns the cursor's screen row
    // offset within the area for positioning below.
    let cursor_y = draw_text_area(out, editor, buf, theme, syntax, &layout, 0, layout.top_offset, top_state)?;

    draw_status_line(out, editor, buf, theme, &layout)?;
    // While completing on the command line, the candidate wildmenu takes over
    // the status-line row (vim shows it just above the command line).
    draw_wildmenu(out, editor, theme, &layout)?;
    draw_command_line(out, editor, theme, &layout)?;

    // The menu bar overlays everything and keeps the text cursor hidden.
    if let Some(menu) = editor.menu() {
        draw_menu(out, theme, &layout, menu)?;
        queue!(out, Hide)?;
    } else if editor.mode == Mode::Command {
        let x = 1 + editor.cmdline.chars().count() as u16; // after ':' or '/'
        queue!(
            out,
            mode_cursor_style(editor.mode),
            MoveTo(x.min(layout.cols.saturating_sub(1)), layout.rows - 1),
            Show
        )?;
    } else {
        let cx = layout.gutter_width + (editor.cursor.col.saturating_sub(editor.left)) as u16;
        // With folds the cursor's screen row is where the walk above drew its line
        // (the fold header, if the cursor sits in a closed fold); without folds it
        // is simply the distance below the top line.
        let cy = cursor_y
            .map(|y| layout.top_offset + y)
            .unwrap_or_else(|| {
                layout.top_offset + (editor.cursor.row.saturating_sub(editor.top)) as u16
            });
        queue!(
            out,
            mode_cursor_style(editor.mode),
            MoveTo(
                cx.min(layout.cols.saturating_sub(1)),
                cy.min(layout.top_offset + layout.text_rows - 1)
            ),
            Show
        )?;
    }

    queue!(out, EndSynchronizedUpdate)?;
    out.flush()
}

/// Draw one editor's gutter + text area into `layout.text_rows` rows starting at
/// screen cell `(origin_x, origin_y)`, bounded to `layout`'s widths. Fold-aware.
/// Returns the cursor's row offset within the area (relative to `origin_y`), or
/// `None` when the cursor line isn't on screen. Shared by the single-window
/// `render` and the multi-pane `render_split`.
#[allow(clippy::too_many_arguments)]
fn draw_text_area(
    out: &mut impl Write,
    editor: &Editor,
    buf: &Buffer,
    theme: &Theme,
    syntax: &Registry,
    layout: &Layout,
    origin_x: u16,
    origin_y: u16,
    top_state: LineState,
) -> io::Result<Option<u16>> {
    let block = editor.block_rect();
    let sel = if block.is_some() { None } else { editor.selection() };
    let linewise = editor.mode == Mode::VisualLine;
    let search = editor.search_regex();
    let paren = editor.match_highlight();
    // Color-column guides as 0-based columns (computed once for all rows).
    // saturating_sub guards the hot path against a stray 0 in the (public) field.
    let color_cols: Vec<usize> = editor.color_columns().iter().map(|c| c.saturating_sub(1)).collect();

    // Frame-constant drawing parameters, built once and shared by every line.
    let ctx = RenderCtx {
        theme,
        layout,
        left: editor.left,
        sel,
        linewise,
        search,
        tab_width: editor.tabstop.max(1),
        block,
        list: editor.list,
        listchars: editor.listchars(),
        space_mark: editor.listchars_space,
        nbsp_mark: editor.listchars_nbsp,
        color_cols: &color_cols,
    };

    let mut line_state = top_state;
    let folding = editor.folding();
    let mut row = editor.top;
    let mut cursor_y: Option<u16> = None;

    for y in 0..layout.text_rows {
        queue!(out, MoveTo(origin_x, origin_y + y))?;
        // The gutter is painted for every row, including those past the end of the
        // buffer (where it draws blank cells), so the tilde always sits in the text
        // column and the gutter never shows stale content.
        draw_gutter(out, editor, buf, theme, layout, row)?;

        if row >= buf.line_count() {
            queue!(
                out,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.gutter_fg),
                Print("~"),
                SetBackgroundColor(theme.bg),
                Print(" ".repeat(layout.text_cols.saturating_sub(1) as usize)),
                ResetColor,
            )?;
            continue;
        }

        if row == editor.cursor.row {
            cursor_y = Some(y);
        }

        let header = if folding { editor.folds().header(row) } else { None };
        if let Some((_, end)) = header {
            draw_fold_line(out, editor, buf, theme, layout, row, end)?;
            // Advance the carry-over state across the hidden fold lines without
            // tokenizing them (they are not drawn).
            let mut l = row;
            while l <= end && l < buf.line_count() {
                if let Some(line) = buf.line(l) {
                    line_state = syntax.advance_block_state(editor.language, line, line_state);
                }
                l += 1;
            }
            row = end + 1;
            queue!(out, ResetColor)?;
            continue;
        }

        let line_bg = if editor.cursorline && row == editor.cursor.row {
            theme.cursor_line_bg
        } else {
            theme.bg
        };

        if let Some(line) = buf.line(row) {
            let (tokens, next_state) =
                syntax.highlight_stateful(editor.language, line, line_state);
            line_state = next_state;
            let match_col = paren.filter(|p| p.row == row).map(|p| p.col);
            let cursor_col = if editor.cursorcolumn {
                Some(editor.cursor.col)
            } else {
                None
            };
            let cur_match = if row == editor.cursor.row {
                editor.current_match()
            } else {
                None
            };
            let truncated_right = draw_text_line(
                out,
                &ctx,
                line,
                &tokens,
                row,
                line_bg,
                LineHl { match_col, cursor_col, cur_match },
            )?;
            // Edge markers (configurable via listchars; a space hides one): the
            // precedes glyph when text is hidden to the left (scrolled right), the
            // extends glyph when the line continues past the right edge.
            let pre = editor.listchars_precedes;
            let ext = editor.listchars_extends;
            let precedes =
                (editor.left > 0 && !line.is_empty() && pre != ' ').then_some(pre);
            let extends = (truncated_right && ext != ' ').then_some(ext);
            draw_edge_markers(
                out,
                theme,
                origin_x + layout.gutter_width,
                origin_y + y,
                layout.text_cols,
                line_bg,
                (precedes, extends),
            )?;
        }
        row += 1;
        queue!(out, ResetColor)?;
    }
    Ok(cursor_y)
}

/// One window to draw in a split layout.
pub struct WindowView<'a> {
    pub editor: &'a Editor,
    pub buffer: &'a Buffer,
    pub rect: crate::window::Rect,
    pub top_state: LineState,
    pub focused: bool,
}

/// Draw a full frame with multiple windows (after at least one `:split`). Each
/// window gets its own gutter, text area and status line; side-by-side windows
/// are divided by a vertical separator. The command line spans the bottom row and
/// reflects the focused window.
pub fn render_split(
    out: &mut impl Write,
    views: &[WindowView],
    theme: &Theme,
    syntax: &Registry,
    tabs: &[TabEntry],
) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    let show_tabline = tabs.len() > 1;
    let bar_layout = Layout::compute(cols, rows, 1, false, show_tabline);

    queue!(out, BeginSynchronizedUpdate, Hide, MoveTo(0, 0))?;
    if show_tabline {
        draw_tabline(out, theme, &bar_layout, tabs)?;
    }

    let mut focus_cursor: Option<(u16, u16)> = None;
    let mut focus_editor: Option<&Editor> = None;
    for v in views {
        let pos = draw_window(out, v, theme, syntax)?;
        if v.focused {
            focus_cursor = pos;
            focus_editor = Some(v.editor);
        }
    }

    // Vertical separators: any window with another window directly to its right
    // owns the one-column gap at `rect.x + rect.w`.
    draw_separators(out, views, theme)?;

    // The command line / message row belongs to the focused window.
    if let Some(editor) = focus_editor {
        // While completing on the command line, the wildmenu takes the row just
        // above the command line (same as single-window mode).
        draw_wildmenu(out, editor, theme, &bar_layout)?;
        draw_command_line(out, editor, theme, &bar_layout)?;
        if editor.mode == Mode::Command {
            let x = 1 + editor.cmdline.chars().count() as u16;
            queue!(
                out,
                mode_cursor_style(editor.mode),
                MoveTo(x.min(cols.saturating_sub(1)), rows - 1),
                Show
            )?;
        } else if let Some((cx, cy)) = focus_cursor {
            queue!(
                out,
                mode_cursor_style(editor.mode),
                MoveTo(cx.min(cols.saturating_sub(1)), cy),
                Show
            )?;
        }
    }

    queue!(out, EndSynchronizedUpdate)?;
    out.flush()
}

/// Draw a single window (gutter + text + its own status line) within its rect.
/// Returns the absolute cursor position when this is the focused window.
fn draw_window(
    out: &mut impl Write,
    view: &WindowView,
    theme: &Theme,
    syntax: &Registry,
) -> io::Result<Option<(u16, u16)>> {
    let editor = view.editor;
    let buf = view.buffer;
    let rect = view.rect;
    let mut fold_width = editor.foldcolumn as u16;
    let mut gutter_width =
        gutter_width(buf.line_count(), editor.show_line_numbers) + fold_width;
    // Keep the gutter within the pane so it can't spill into the neighbour.
    let max_gutter = rect.w.saturating_sub(1);
    if gutter_width > max_gutter {
        gutter_width = max_gutter;
        fold_width = fold_width.min(gutter_width);
    }
    let text_rows = rect.h.saturating_sub(1).max(1); // reserve the last row for the status line
    let text_cols = rect.w.saturating_sub(gutter_width).max(1);
    // A layout describing just this window; top_offset is 0 because the absolute
    // vertical position is handled through `origin_y`.
    let per = Layout {
        cols: rect.w,
        rows: rect.h,
        gutter_width,
        text_rows,
        text_cols,
        top_offset: 0,
        fold_width,
    };
    let cursor_y = draw_text_area(out, editor, buf, theme, syntax, &per, rect.x, rect.y, view.top_state)?;
    draw_window_status(out, editor, buf, theme, rect, view.focused)?;

    let pos = if view.focused {
        let cx = rect.x + gutter_width + (editor.cursor.col.saturating_sub(editor.left)) as u16;
        let cy = rect.y + cursor_y.unwrap_or(0);
        Some((
            cx.min(rect.x + rect.w.saturating_sub(1)),
            cy.min(rect.y + text_rows.saturating_sub(1)),
        ))
    } else {
        None
    };
    Ok(pos)
}

/// A per-window status line on the last row of `rect`: file name, position, and a
/// bright highlight when the window is focused.
fn draw_window_status(
    out: &mut impl Write,
    editor: &Editor,
    buf: &Buffer,
    theme: &Theme,
    rect: crate::window::Rect,
    focused: bool,
) -> io::Result<()> {
    let y = rect.y + rect.h.saturating_sub(1);
    let file = buf
        .path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "[No Name]".to_string());
    let flags = buffer_flags(editor, buf);
    let pos = format!("{}:{}", editor.cursor.row + 1, editor.cursor.col + 1);
    let text = format!(" {file}{flags} ");
    let w = rect.w as usize;
    let left: String = text.chars().take(w).collect();
    let right = format!(" {pos} ");
    let used = left.chars().count() + right.chars().count();
    let fill = w.saturating_sub(used);
    let (fg, bg) = if focused {
        (theme.mode_fg, theme.mode_bg)
    } else {
        (theme.status_fg, theme.status_bg)
    };
    let line: String = if used <= w {
        format!("{left}{}{right}", " ".repeat(fill))
    } else {
        left.chars().take(w).collect()
    };
    queue!(
        out,
        MoveTo(rect.x, y),
        SetBackgroundColor(bg),
        SetForegroundColor(fg),
        Print(line),
        ResetColor
    )
}

/// Draw the one-column vertical separators between side-by-side windows.
fn draw_separators(
    out: &mut impl Write,
    views: &[WindowView],
    theme: &Theme,
) -> io::Result<()> {
    for v in views {
        let sep_x = v.rect.x + v.rect.w;
        // A separator is owned by this window only if another window begins
        // exactly one column to its right (i.e. the gap column is real).
        let has_right = views
            .iter()
            .any(|o| o.rect.x == sep_x + 1 && overlaps_rows(&o.rect, &v.rect));
        if !has_right {
            continue;
        }
        for dy in 0..v.rect.h {
            queue!(
                out,
                MoveTo(sep_x, v.rect.y + dy),
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.gutter_fg),
                Print("│"),
                ResetColor
            )?;
        }
    }
    Ok(())
}

fn overlaps_rows(a: &crate::window::Rect, b: &crate::window::Rect) -> bool {
    a.y < b.y + b.h && b.y < a.y + a.h
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
    buf: &Buffer,
    theme: &Theme,
    layout: &Layout,
    row: usize,
) -> io::Result<()> {
    if layout.gutter_width == 0 {
        return Ok(());
    }
    let within = row < buf.line_count();
    let fold_w = layout.fold_width as usize;
    let num_w = layout.gutter_width.saturating_sub(layout.fold_width) as usize;

    // Fold-indicator column (`:set foldcolumn`), leftmost.
    if fold_w > 0 {
        let cells = if within {
            editor.fold_column_cells(row, fold_w)
        } else {
            " ".repeat(fold_w)
        };
        queue!(
            out,
            SetBackgroundColor(theme.gutter_bg),
            SetForegroundColor(theme.gutter_fg),
            Print(cells)
        )?;
    }

    // Line-number column.
    if num_w > 0 {
        let (fg, text) = if within {
            let fg = if row == editor.cursor.row {
                theme.current_line_nr_fg
            } else {
                theme.gutter_fg
            };
            (
                fg,
                gutter_label(row, editor.cursor.row, editor.relative_numbers, num_w),
            )
        } else {
            (theme.gutter_fg, " ".repeat(num_w))
        };
        queue!(
            out,
            SetBackgroundColor(theme.gutter_bg),
            SetForegroundColor(fg),
            Print(text)
        )?;
    }
    Ok(())
}

/// The foldtext for a closed fold: `+--  N lines: <first line>` padded to the
/// text width with dots, mirroring vim's default. Exposed for testing.
pub fn fold_text(level: usize, start_line: &str, count: usize, width: usize) -> String {
    let dashes = "-".repeat(level.min(width.max(1)).max(1));
    let body = format!("+{dashes}{count} lines: {} ", start_line.trim_start());
    let mut s: String = body.chars().take(width).collect();
    let fill = width.saturating_sub(s.chars().count());
    s.push_str(&"·".repeat(fill));
    s
}

/// Draw a closed fold's header row: a dimmed `+-- N lines: …` bar.
fn draw_fold_line(
    out: &mut impl Write,
    editor: &Editor,
    buf: &Buffer,
    theme: &Theme,
    layout: &Layout,
    start: usize,
    end: usize,
) -> io::Result<()> {
    let count = end - start + 1;
    let first = buf.line(start).unwrap_or("");
    let level = editor.folds().level(start);
    let text = fold_text(level, first, count, layout.text_cols as usize);
    queue!(
        out,
        SetBackgroundColor(theme.cursor_line_bg),
        SetForegroundColor(theme.gutter_fg),
        Print(text)
    )
}

/// Per-frame constants shared by every line drawn in one frame (bundled so the
/// per-line draw call stays readable).
#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    theme: &'a Theme,
    layout: &'a Layout,
    left: usize,
    sel: Option<(Position, Position)>,
    linewise: bool,
    search: Option<&'a regex::Regex>,
    tab_width: usize,
    block: Option<(usize, usize, usize, usize)>,
    list: bool,
    listchars: (char, char, char, char),
    space_mark: char,
    nbsp_mark: char,
    color_cols: &'a [usize],
}

/// Highlight positions specific to a single line.
#[derive(Clone, Copy)]
struct LineHl {
    match_col: Option<usize>,
    cursor_col: Option<usize>,
    cur_match: Option<(usize, usize)>,
}

/// Draw one text line, clipped to the horizontal scroll window. Returns whether
/// the line extends past the right edge (so the caller can show an edge marker).
fn draw_text_line(
    out: &mut impl Write,
    ctx: &RenderCtx,
    line: &str,
    tokens: &[Token],
    row: usize,
    line_bg: Color,
    hl: LineHl,
) -> io::Result<bool> {
    let &RenderCtx {
        theme,
        layout,
        left,
        sel,
        linewise,
        search,
        tab_width,
        block,
        list,
        listchars,
        space_mark,
        nbsp_mark,
        color_cols,
    } = ctx;
    let LineHl { match_col, cursor_col, cur_match } = hl;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let matches = search.map(|re| crate::pattern::match_ranges(re, line)).unwrap_or_default();
    // With `:set list`, trailing whitespace (after the last non-blank char) is
    // shown with middot/tab markers so it's visible.
    let trail_start = chars
        .iter()
        .rposition(|&(_, c)| !c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);

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
    let mut truncated_right = false;

    for ci in left..chars.len() {
        if printed >= width {
            // More characters remain than fit: the line extends past the edge.
            truncated_right = true;
            break;
        }
        let ch = chars[ci].1;
        let selected = sel
            .map(|s| in_selection(s, linewise, row, ci))
            .unwrap_or(false);
        let in_block = block
            .map(|(rmin, rmax, cmin, cmax)| row >= rmin && row <= rmax && ci >= cmin && ci <= cmax)
            .unwrap_or(false);
        let in_match = matches.iter().any(|&(s, e)| ci >= s && ci < e);
        let in_cur_match = cur_match.map(|(s, e)| ci >= s && ci < e).unwrap_or(false);
        let is_paren = match_col == Some(ci);
        // Priority: selection/block > matching bracket > current search match >
        // other search match > line. The matched bracket uses reverse video.
        let (mut cfg, bg) = if selected || in_block {
            (fg[ci], theme.selection_bg)
        } else if is_paren {
            (line_bg, fg[ci])
        } else if in_cur_match {
            (fg[ci], theme.cur_search_bg)
        } else if in_match {
            (fg[ci], theme.search_bg)
        } else if color_cols.contains(&ci) {
            (fg[ci], theme.color_column_bg)
        } else if cursor_col == Some(ci) {
            (fg[ci], theme.cursor_line_bg)
        } else {
            (fg[ci], line_bg)
        };
        // Compute the glyph(s) for this cell, substituting list markers for
        // whitespace. Widths are preserved so selection/search columns stay exact.
        let plain = !(selected || in_block || is_paren || in_match || in_cur_match);
        let glyph: String = if ch == '\t' {
            if list {
                if plain {
                    cfg = theme.gutter_fg;
                }
                let mut s = String::from(listchars.0);
                s.push_str(&listchars.1.to_string().repeat(tab_width.saturating_sub(1)));
                s
            } else {
                " ".repeat(tab_width)
            }
        } else if list && ch == '\u{00A0}' {
            // Non-breaking space: invisible normally, so always flagged under list.
            if plain {
                cfg = theme.gutter_fg;
            }
            nbsp_mark.to_string()
        } else if list && ci >= trail_start && ch == ' ' {
            if plain {
                cfg = theme.gutter_fg;
            }
            listchars.2.to_string()
        } else if list && ch == ' ' && space_mark != ' ' {
            // Ordinary spaces, only when a non-blank `space:` marker is configured.
            if plain {
                cfg = theme.gutter_fg;
            }
            space_mark.to_string()
        } else {
            ch.to_string()
        };
        if !started {
            run_fg = cfg;
            run_bg = bg;
            started = true;
        } else if cfg != run_fg || bg != run_bg {
            runs.push((run_fg, run_bg, std::mem::take(&mut run)));
            run_fg = cfg;
            run_bg = bg;
        }
        printed += if ch == '\t' { tab_width } else { 1 };
        run.push_str(&glyph);
    }
    if !run.is_empty() {
        runs.push((run_fg, run_bg, run));
    }

    for (f, b, text) in runs {
        queue!(out, SetForegroundColor(f), SetBackgroundColor(b), Print(text))?;
    }

    // Pad the rest of the row with the line background. With `:set list`, mark the
    // end of the actual line text with a `$` in the first padding cell.
    if printed < width {
        if list && left <= chars.len() {
            queue!(
                out,
                SetBackgroundColor(line_bg),
                SetForegroundColor(theme.gutter_fg),
                Print(listchars.3),
                Print(" ".repeat(width - printed - 1))
            )?;
        } else {
            // Fill the padding, marking any color columns and the cursor column
            // that fall past the line's end. Consecutive same-background cells are
            // batched into one write.
            let bg_at = |screen: usize| -> Color {
                let abs = screen + left;
                if color_cols.contains(&abs) {
                    theme.color_column_bg
                } else if cursor_col == Some(abs) {
                    theme.cursor_line_bg
                } else {
                    line_bg
                }
            };
            let mut s = printed;
            while s < width {
                let bg = bg_at(s);
                let mut e = s + 1;
                while e < width && bg_at(e) == bg {
                    e += 1;
                }
                queue!(out, SetBackgroundColor(bg), Print(" ".repeat(e - s)))?;
                s = e;
            }
        }
    }
    Ok(truncated_right)
}

/// Overlay the horizontal-scroll edge markers on a row that was just drawn.
/// `text_x` is the first text column (after the gutter); `y` the absolute row.
/// Each side draws its glyph only when `Some`.
fn draw_edge_markers(
    out: &mut impl Write,
    theme: &Theme,
    text_x: u16,
    y: u16,
    text_cols: u16,
    line_bg: Color,
    marks: (Option<char>, Option<char>),
) -> io::Result<()> {
    let (precedes, extends) = marks;
    if text_cols == 0 {
        return Ok(());
    }
    if let Some(ch) = precedes {
        queue!(
            out,
            MoveTo(text_x, y),
            SetBackgroundColor(line_bg),
            SetForegroundColor(theme.gutter_fg),
            Print(ch),
            ResetColor,
        )?;
    }
    if let Some(ch) = extends {
        queue!(
            out,
            MoveTo(text_x + text_cols - 1, y),
            SetBackgroundColor(line_bg),
            SetForegroundColor(theme.gutter_fg),
            Print(ch),
            ResetColor,
        )?;
    }
    Ok(())
}

/// The status-line flags shown after the file name: modified `[+]`, read-only
/// `[RO]`, paste mode `[paste]`, and non-Unix line endings `[dos]`. Each is shown
/// only when active, so the common case stays clean. Shared by both status lines.
fn buffer_flags(editor: &Editor, buf: &Buffer) -> String {
    let mut s = String::new();
    if buf.is_dirty() {
        s.push_str(" [+]");
    }
    if editor.readonly {
        s.push_str(" [RO]");
    }
    if editor.paste {
        s.push_str(" [paste]");
    }
    if matches!(buf.fileformat(), crate::buffer::LineEnding::Dos) {
        s.push_str(" [dos]");
    }
    s
}

fn draw_status_line(
    out: &mut impl Write,
    editor: &Editor,
    buf: &Buffer,
    theme: &Theme,
    layout: &Layout,
) -> io::Result<()> {
    let y = layout.rows - 2;
    queue!(out, MoveTo(0, y))?;

    let mode = editor.mode.label();
    let file = buf
        .path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "[No Name]".to_string());
    let flags = buffer_flags(editor, buf);
    let lang = editor.language.name();
    let pos = format!("{}:{}", editor.cursor.row + 1, editor.cursor.col + 1);
    let scroll = editor.scroll_indicator();

    let left = format!(" {mode} ");
    let rec = match editor.recording_register() {
        Some(r) => format!("recording @{r}  "),
        None => String::new(),
    };
    let mid = format!(" {file}{flags} ");
    // `:set ruler` controls the cursor position + scroll indicator.
    let right = if editor.ruler {
        format!(" {rec}{lang} | {pos} | {scroll} ")
    } else {
        format!(" {rec}{lang} ")
    };

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

/// Whether a status message should be rendered as an error. Follows vim's
/// convention where error messages are coded `E<number>:` (for example
/// `E37: No write since last change`). Ordinary messages that merely start
/// with `E` (like `Edit ...`) are not flagged, since the second character
/// must be a digit.
pub fn is_error_message(msg: &str) -> bool {
    let mut chars = msg.chars();
    chars.next() == Some('E') && chars.next().is_some_and(|c| c.is_ascii_digit())
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

    // Error messages (vim-style `E123:`) are shown in a distinct color so a
    // failed command stands out from ordinary status text.
    let mut is_error = false;
    let content = if editor.mode == Mode::Command {
        format!("{}{}", editor.cmdline_prefix(), editor.cmdline)
    } else if let Some(indicator) = editor.mode_indicator().filter(|_| editor.showmode) {
        // showmode: `-- INSERT --` / `-- VISUAL --` etc. while in those modes.
        indicator.to_string()
    } else {
        is_error = is_error_message(&editor.message);
        editor.message.clone()
    };
    let content: String = content.chars().take(layout.cols as usize).collect();
    let fg = if is_error { theme.error_fg } else { theme.message_fg };
    queue!(out, SetForegroundColor(fg), Print(&content), ResetColor)?;

    // showcmd: the pending command, or the selection size while in visual mode,
    // right-aligned (vim shows it bottom-right). Never overlaps the left content.
    if editor.mode != Mode::Command && editor.showcmd {
        let indicator = editor
            .visual_size()
            .unwrap_or_else(|| editor.pending_command().to_string());
        if !indicator.is_empty() {
            let cols = layout.cols as usize;
            let shown: String = indicator.chars().rev().take(10).collect::<Vec<_>>()
                .into_iter().rev().collect();
            let width = shown.chars().count();
            if cols >= width && cols - width > content.chars().count() {
                queue!(
                    out,
                    MoveTo((cols - width) as u16, y),
                    SetForegroundColor(theme.message_fg),
                    Print(shown),
                    ResetColor
                )?;
            }
        }
    }
    Ok(())
}

/// Choose which completion candidates to show on the wildmenu, as
/// `(cell_text, selected)` pairs. Each cell is the item padded with a space on
/// each side. A sliding window keeps the selected item visible when the full
/// list is wider than `width`, expanding right-then-left from the selection.
pub fn wildmenu_cells(items: &[String], selected: usize, width: usize) -> Vec<(String, bool)> {
    if items.is_empty() || width == 0 || selected >= items.len() {
        return Vec::new();
    }
    let cell_w = |s: &str| s.chars().count() + 2;
    let mut start = selected;
    let mut end = selected + 1;
    let mut used = cell_w(&items[selected]);
    loop {
        let mut grew = false;
        if end < items.len() && used + cell_w(&items[end]) <= width {
            used += cell_w(&items[end]);
            end += 1;
            grew = true;
        }
        if start > 0 && used + cell_w(&items[start - 1]) <= width {
            used += cell_w(&items[start - 1]);
            start -= 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    (start..end)
        .map(|i| (format!(" {} ", items[i]), i == selected))
        .collect()
}

/// Draw the wildmenu (command-line completion candidates) on the status-line
/// row while a `Tab` completion cycle is active. Returns whether it drew.
fn draw_wildmenu(
    out: &mut impl Write,
    editor: &Editor,
    theme: &Theme,
    layout: &Layout,
) -> io::Result<bool> {
    let Some((items, selected)) = editor.completion_menu() else {
        return Ok(false);
    };
    let cells = wildmenu_cells(items, selected, layout.cols as usize);
    if cells.is_empty() {
        return Ok(false);
    }
    let y = layout.rows.saturating_sub(2);
    queue!(
        out,
        MoveTo(0, y),
        SetBackgroundColor(theme.status_bg),
        SetForegroundColor(theme.status_fg),
        Clear(ClearType::CurrentLine)
    )?;
    for (text, is_sel) in cells {
        if is_sel {
            queue!(
                out,
                SetBackgroundColor(theme.mode_bg),
                SetForegroundColor(theme.mode_fg),
                Print(text),
                SetBackgroundColor(theme.status_bg),
                SetForegroundColor(theme.status_fg)
            )?;
        } else {
            queue!(out, Print(text))?;
        }
    }
    queue!(out, ResetColor)?;
    Ok(true)
}

// ---- menu bar --------------------------------------------------------------

/// The x column where each top-level menu title's segment begins.
pub fn menu_bar_positions(titles: &[String]) -> Vec<u16> {
    let mut x = 1u16;
    let mut positions = Vec::with_capacity(titles.len());
    for t in titles {
        positions.push(x);
        x += t.chars().count() as u16 + 3; // " title " + 1 gap
    }
    positions
}

fn item_right(item: &MenuItem) -> String {
    match &item.action {
        MenuAction::Submenu(_) => "▸".to_string(),
        MenuAction::Command(_) => item.hint.clone(),
    }
}

/// Lay out one dropdown row ` label …… right ` into exactly `inner_w` cells.
pub fn compose_row(label: &str, right: &str, inner_w: usize) -> String {
    let left = format!(" {label}");
    let right = format!("{right} ");
    let lw = left.chars().count();
    let rw = right.chars().count();
    if lw + rw <= inner_w {
        format!("{left}{}{right}", " ".repeat(inner_w - lw - rw))
    } else {
        let mut s: String = format!("{left}{right}").chars().take(inner_w).collect();
        let pad = inner_w.saturating_sub(s.chars().count());
        s.push_str(&" ".repeat(pad));
        s
    }
}

/// Geometry of one open dropdown level, shared by rendering and mouse hit-tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuLevelGeom {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub scroll: usize,
    /// Number of item rows actually shown.
    pub visible: usize,
    pub items_len: usize,
}

fn dropdown_inner_width(items: &[MenuItem]) -> usize {
    items
        .iter()
        .map(|it| it.label.chars().count() + item_right(it).chars().count() + 3)
        .max()
        .unwrap_or(10)
        .max(10)
}

/// Compute the box geometry for every open dropdown level (one entry per level).
/// Deterministic from the menu state + terminal size, so hit-testing matches
/// what was drawn.
pub fn menu_geometry(menu: &MenuState, cols: u16, rows: u16) -> Vec<MenuLevelGeom> {
    let titles: Vec<String> = menu.menus.iter().map(|m| m.title.clone()).collect();
    let positions = menu_bar_positions(&titles);
    let mut out = Vec::new();
    let mut x = positions.get(menu.top).copied().unwrap_or(0);
    let mut y = 1u16;
    for level in 0..menu.depth() {
        let items = match menu.items_at(level) {
            Some(it) => it,
            None => break,
        };
        let sel = menu.selected_at(level);
        let box_w = ((dropdown_inner_width(items) as u16) + 2).min(cols.saturating_sub(x).max(4));
        let max_rows = rows.saturating_sub(2).saturating_sub(y) as usize;
        let (scroll, shown) = if max_rows < 3 {
            (0, 0)
        } else {
            let visible = (max_rows - 2).max(1);
            let n = items.len();
            let scroll = if n > visible {
                sel.saturating_sub(visible - 1).min(n - visible)
            } else {
                0
            };
            (scroll, visible.min(n))
        };
        out.push(MenuLevelGeom {
            x,
            y,
            width: box_w,
            scroll,
            visible: shown,
            items_len: items.len(),
        });
        let sel_row = y + 1 + sel.saturating_sub(scroll) as u16;
        x = x.saturating_add(box_w).min(cols.saturating_sub(1));
        y = sel_row;
    }
    out
}

/// Map a mouse click to `(level, item_index)` within an open dropdown, if it
/// landed on an item row.
pub fn menu_hit_test(geom: &[MenuLevelGeom], col: u16, row: u16) -> Option<(usize, usize)> {
    for (level, g) in geom.iter().enumerate() {
        let y0 = g.y + 1;
        let y1 = y0 + g.visible as u16;
        if col >= g.x && col < g.x + g.width && row >= y0 && row < y1 {
            let idx = (row - y0) as usize + g.scroll;
            if idx < g.items_len {
                return Some((level, idx));
            }
        }
    }
    None
}

fn draw_menu(
    out: &mut impl Write,
    theme: &Theme,
    layout: &Layout,
    menu: &MenuState,
) -> io::Result<()> {
    let cols = layout.cols;
    let bar_bg = theme.status_bg;
    let bar_fg = theme.status_fg;
    let sel_bg = theme.mode_bg;
    let sel_fg = theme.mode_fg;

    // The bar across row 0.
    let titles: Vec<String> = menu.menus.iter().map(|m| m.title.clone()).collect();
    let positions = menu_bar_positions(&titles);
    queue!(
        out,
        MoveTo(0, 0),
        SetBackgroundColor(bar_bg),
        SetForegroundColor(bar_fg),
        Print(" ".repeat(cols as usize))
    )?;
    for (i, t) in titles.iter().enumerate() {
        let x = positions[i];
        if x >= cols {
            break;
        }
        let seg: String = format!(" {t} ").chars().take((cols - x) as usize).collect();
        let (fg, bg) = if i == menu.top {
            (sel_fg, sel_bg)
        } else {
            (bar_fg, bar_bg)
        };
        queue!(
            out,
            MoveTo(x, 0),
            SetForegroundColor(fg),
            SetBackgroundColor(bg),
            Print(seg)
        )?;
    }

    // Cascading dropdowns, positioned by the shared geometry.
    let geom = menu_geometry(menu, layout.cols, layout.rows);
    for (level, g) in geom.iter().enumerate() {
        if let Some(items) = menu.items_at(level) {
            draw_dropdown(out, theme, g, items, menu.selected_at(level))?;
        }
    }

    queue!(out, ResetColor)
}

/// Draw a single bordered dropdown box at a precomputed geometry.
fn draw_dropdown(
    out: &mut impl Write,
    theme: &Theme,
    g: &MenuLevelGeom,
    items: &[MenuItem],
    sel: usize,
) -> io::Result<()> {
    if g.visible == 0 {
        return Ok(());
    }
    let bg = theme.status_bg;
    let fg = theme.status_fg;
    let sel_bg = theme.mode_bg;
    let sel_fg = theme.mode_fg;
    let inner_w = g.width.saturating_sub(2) as usize;

    let top_fill = if g.scroll > 0 {
        format!("─▲{}", "─".repeat(inner_w.saturating_sub(2)))
    } else {
        "─".repeat(inner_w)
    };
    queue!(
        out,
        MoveTo(g.x, g.y),
        SetBackgroundColor(bg),
        SetForegroundColor(fg),
        Print(format!("┌{top_fill}┐"))
    )?;

    for row in 0..g.visible {
        let idx = g.scroll + row;
        let item = &items[idx];
        let yy = g.y + 1 + row as u16;
        let body = compose_row(&item.label, &item_right(item), inner_w);
        let (ifg, ibg) = if idx == sel { (sel_fg, sel_bg) } else { (fg, bg) };
        queue!(
            out,
            MoveTo(g.x, yy),
            SetForegroundColor(fg),
            SetBackgroundColor(bg),
            Print("│"),
            SetForegroundColor(ifg),
            SetBackgroundColor(ibg),
            Print(body),
            SetForegroundColor(fg),
            SetBackgroundColor(bg),
            Print("│")
        )?;
    }

    let more_below = g.scroll + g.visible < g.items_len;
    let bot_fill = if more_below {
        format!("─▼{}", "─".repeat(inner_w.saturating_sub(2)))
    } else {
        "─".repeat(inner_w)
    };
    queue!(
        out,
        MoveTo(g.x, g.y + 1 + g.visible as u16),
        SetForegroundColor(fg),
        SetBackgroundColor(bg),
        Print(format!("└{bot_fill}┘"))
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No per-line highlights, for draw_text_line tests.
    const NO_HL: LineHl = LineHl {
        match_col: None,
        cursor_col: None,
        cur_match: None,
    };

    #[test]
    fn error_messages_detected_by_e_code() {
        assert!(is_error_message("E37: No write since last change"));
        assert!(is_error_message("E486: Pattern not found: foo"));
        // Ordinary messages are not errors, even when they start with E.
        assert!(!is_error_message("Edit saved"));
        assert!(!is_error_message("2 lines yanked"));
        assert!(!is_error_message(""));
        assert!(!is_error_message("E: generic"));
    }

    #[test]
    fn fold_text_shows_line_count_and_fills() {
        let s = fold_text(1, "    fn main() {", 12, 30);
        assert_eq!(s.chars().count(), 30);
        assert!(s.starts_with("+-12 lines: fn main() {"));
        assert!(s.ends_with('·')); // padded with dots
    }

    #[test]
    fn fold_text_truncates_when_narrow() {
        let s = fold_text(2, "something long here", 3, 8);
        assert_eq!(s.chars().count(), 8);
    }

    #[test]
    fn render_split_draws_both_windows_and_a_separator() {
        use crate::buffer::Buffer;
        use crate::window::Rect;
        let mut a = Editor::new();
        a.buffer = Buffer::from_text("AAAA\nAAAA");
        a.show_line_numbers = false;
        let mut b = Editor::new();
        b.buffer = Buffer::from_text("BBBB\nBBBB");
        b.show_line_numbers = false;
        let views = vec![
            WindowView {
                editor: &a,
                buffer: &a.buffer,
                rect: Rect::new(0, 0, 20, 10),
                top_state: LineState::Normal,
                focused: true,
            },
            WindowView {
                editor: &b,
                buffer: &b.buffer,
                rect: Rect::new(21, 0, 20, 10),
                top_state: LineState::Normal,
                focused: false,
            },
        ];
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render_split(&mut buf, &views, &theme, &syntax, &[]).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains("AAAA"), "left window text missing");
        assert!(out.contains("BBBB"), "right window text missing");
        assert!(out.contains('│'), "vertical separator missing");
        // The focused window shows the cursor (Show sequence emitted).
        assert!(out.contains("\u{1b}[?25h"), "focused cursor not shown");
    }

    #[test]
    fn render_split_positions_focused_cursor_within_its_rect() {
        use crate::buffer::Buffer;
        use crate::window::Rect;
        let mut a = Editor::new();
        a.buffer = Buffer::from_text("top\ntext");
        a.show_line_numbers = false;
        // Focused window is the second one, at an x offset and a y offset.
        let mut b = Editor::new();
        b.buffer = Buffer::from_text("hello\nworld");
        b.show_line_numbers = false;
        b.cursor.row = 1;
        b.cursor.col = 3;
        let rect_b = Rect::new(20, 0, 20, 8);
        let views = vec![
            WindowView { editor: &a, buffer: &a.buffer, rect: Rect::new(0, 0, 19, 8), top_state: LineState::Normal, focused: false },
            WindowView { editor: &b, buffer: &b.buffer, rect: rect_b, top_state: LineState::Normal, focused: true },
        ];
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render_split(&mut buf, &views, &theme, &syntax, &[]).unwrap();
        let out = String::from_utf8_lossy(&buf);
        // The final cursor MoveTo (just before Show) must land inside window b's
        // rect: column 20..40, row 0..8 (1-based in the escape sequence).
        let re = regex::Regex::new("\u{1b}\\[(\\d+);(\\d+)H\u{1b}\\[\\?25h").unwrap();
        let caps = re.captures(&out).expect("no cursor MoveTo before Show");
        let row: u16 = caps[1].parse().unwrap();
        let col: u16 = caps[2].parse().unwrap();
        // Cursor at b (row 1, col 3), gutter 0 -> x = 20+3 = 23, y = 0+1 = 1;
        // escape is 1-based, so expect col 24, row 2.
        assert_eq!((row, col), (2, 24), "focused cursor landed outside window b");
    }

    #[test]
    fn cursor_style_matches_mode() {
        assert!(matches!(mode_cursor_style(Mode::Insert), SetCursorStyle::SteadyBar));
        assert!(matches!(mode_cursor_style(Mode::Replace), SetCursorStyle::SteadyUnderScore));
        assert!(matches!(mode_cursor_style(Mode::Normal), SetCursorStyle::SteadyBlock));
        assert!(matches!(mode_cursor_style(Mode::Visual), SetCursorStyle::SteadyBlock));
    }

    #[test]
    fn render_emits_mode_cursor_shape() {
        use crate::buffer::Buffer;
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        // Normal mode -> steady block (ESC [2 q).
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("x");
        ed.set_viewport(10, 20);
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        assert!(String::from_utf8_lossy(&buf).contains("\u{1b}[2 q"));
        // Insert mode -> steady bar (ESC [6 q).
        ed.mode = Mode::Insert;
        let mut buf2: Vec<u8> = Vec::new();
        render(&mut buf2, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        assert!(String::from_utf8_lossy(&buf2).contains("\u{1b}[6 q"));
    }

    #[test]
    fn foldcolumn_shifts_cursor_x_by_its_width() {
        use crate::buffer::Buffer;
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("hello world");
        ed.show_line_numbers = false; // gutter is purely the foldcolumn
        ed.foldcolumn = 3;
        ed.cursor.col = 4;
        ed.set_viewport(10, 40);
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        let out = String::from_utf8_lossy(&buf);
        // Final cursor MoveTo before Show: x = foldcolumn(3) + col(4) = 7 -> col 8 (1-based).
        let re = regex::Regex::new("\u{1b}\\[(\\d+);(\\d+)H\u{1b}\\[\\?25h").unwrap();
        let caps = re.captures(&out).expect("cursor MoveTo before Show");
        assert_eq!(&caps[2], "8", "cursor x not shifted by foldcolumn width");
    }

    #[test]
    fn foldcolumn_renders_markers_in_the_gutter() {
        use crate::buffer::Buffer;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("a\nb\nc\nd\ne");
        ed.show_line_numbers = false;
        ed.foldcolumn = 1;
        // Fold lines 1..=3, then open so the extent shows as bars/dashes.
        ed.cursor.row = 1;
        for c in ['z', 'f', '2', 'j', 'z', 'R'] {
            ed.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        ed.set_viewport(10, 20);
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains('|'), "fold-extent bar missing from foldcolumn");
    }

    #[test]
    fn past_end_rows_paint_gutter_before_tilde() {
        use crate::buffer::Buffer;
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("one\ntwo"); // 2 lines, tall window => tildes
        ed.show_line_numbers = true;
        ed.set_viewport(10, 20);
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        let out = String::from_utf8_lossy(&buf);
        // Strip ANSI escape sequences so we see the actual printed glyphs.
        let re = regex::Regex::new("\u{1b}\\[[0-9;?]*[A-Za-z]").unwrap();
        let plain = re.replace_all(&out, "");
        // The gutter (4 cells here) is painted before the tilde, so '~' is never at
        // column 0 — it follows the gutter's blank cells.
        assert!(plain.contains('~'));
        assert!(plain.contains("    ~"), "tilde should sit after the gutter, got: {plain:?}");
    }

    #[test]
    fn render_draws_fold_header_for_closed_fold() {
        use crate::buffer::Buffer;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("alpha\nbeta\ngamma\ndelta");
        ed.set_viewport(10, 20);
        // Fold lines 0..=2.
        for c in ['z', 'f', '2', 'j'] {
            ed.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, &ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains("3 lines: alpha")); // the fold header
        assert!(out.contains("delta")); // the line after the fold still shows
        assert!(!out.contains("beta")); // hidden inside the fold
    }

    #[test]
    fn list_mode_marks_tabs_trailing_space_and_eol() {
        let theme = crate::theme::matrix();
        let layout = Layout::compute(80, 24, 1, false, false);
        let mut buf: Vec<u8> = Vec::new();
        let ctx = RenderCtx {
            theme: &theme, layout: &layout, left: 0, sel: None, linewise: false, search: None,
            tab_width: 4, block: None, list: true, listchars: ('▸', '·', '·', '$'), space_mark: ' ', nbsp_mark: '␣', color_cols: &[],
        };
        draw_text_line(&mut buf, &ctx, "a\tb  ", &[], 0, theme.bg, NO_HL).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains('▸')); // tab marker
        assert!(out.contains('·')); // trailing-space / tab-fill marker
        assert!(out.contains('$')); // end-of-line marker
    }

    #[test]
    fn list_mode_marks_nbsp_and_configured_space() {
        let theme = crate::theme::matrix();
        let layout = Layout::compute(80, 24, 1, false, false);
        // nbsp always flagged under list; ordinary spaces only when space: is set.
        let ctx = RenderCtx {
            theme: &theme, layout: &layout, left: 0, sel: None, linewise: false, search: None,
            tab_width: 4, block: None, list: true, listchars: ('▸', '·', '·', '$'),
            space_mark: '•', nbsp_mark: '␣', color_cols: &[],
        };
        let mut buf: Vec<u8> = Vec::new();
        draw_text_line(&mut buf, &ctx, "a\u{00A0}b c", &[], 0, theme.bg, NO_HL).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains('␣'), "non-breaking space marked");
        assert!(out.contains('•'), "ordinary space marked when configured");
        // With the default space marker (a space) ordinary spaces are untouched.
        let ctx2 = RenderCtx { space_mark: ' ', ..ctx };
        let mut buf2: Vec<u8> = Vec::new();
        draw_text_line(&mut buf2, &ctx2, "a b", &[], 0, theme.bg, NO_HL).unwrap();
        assert!(!String::from_utf8_lossy(&buf2).contains('•'));
    }

    #[test]
    fn showmode_and_ruler_toggles_gate_their_display() {
        use crate::buffer::Buffer;
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let render_to = |ed: &Editor| {
            let mut buf: Vec<u8> = Vec::new();
            render(&mut buf, ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        };
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("hello");
        ed.mode = Mode::Insert;
        ed.set_viewport(10, 40);
        // Defaults on: the showmode indicator and ruler position show.
        let on = render_to(&ed);
        assert!(on.contains("-- INSERT --")); // command-line mode indicator
        assert!(on.contains("1:1")); // ruler position
        // Turning them off removes each (the status-line mode label is separate).
        ed.showmode = false;
        ed.ruler = false;
        let off = render_to(&ed);
        assert!(!off.contains("-- INSERT --"));
        assert!(!off.contains("1:1"));
    }

    #[test]
    fn status_line_flags_dos_line_endings_only() {
        use crate::buffer::{Buffer, LineEnding};
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let render_to = |ed: &Editor| {
            let mut buf: Vec<u8> = Vec::new();
            render(&mut buf, ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        };
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("hello");
        ed.set_viewport(10, 40);
        // Unix (the default) shows no format tag.
        assert!(!render_to(&ed).contains("[dos]"));
        // DOS line endings are flagged.
        ed.buffer.set_fileformat(LineEnding::Dos);
        assert!(render_to(&ed).contains("[dos]"));
    }

    #[test]
    fn status_line_flags_paste_and_readonly() {
        use crate::buffer::Buffer;
        let theme = crate::theme::matrix();
        let syntax = Registry::default();
        let render_to = |ed: &Editor| {
            let mut buf: Vec<u8> = Vec::new();
            render(&mut buf, ed, &ed.buffer, &theme, &syntax, &[], LineState::Normal).unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        };
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text("hello");
        ed.set_viewport(10, 40);
        assert!(!render_to(&ed).contains("[paste]"));
        assert!(!render_to(&ed).contains("[RO]"));
        ed.paste = true;
        ed.readonly = true;
        let out = render_to(&ed);
        assert!(out.contains("[paste]"));
        assert!(out.contains("[RO]"));
    }

    #[test]
    fn long_lines_report_truncation_and_edge_markers_draw() {
        let theme = crate::theme::matrix();
        let layout = Layout::compute(10, 24, 1, false, false); // 10-wide text area
        let ctx = RenderCtx {
            theme: &theme, layout: &layout, left: 0, sel: None, linewise: false, search: None,
            tab_width: 4, block: None, list: false, listchars: ('▸', '·', '·', '$'), space_mark: ' ', nbsp_mark: '␣', color_cols: &[],
        };
        // A line wider than the text area reports right-truncation; a short one does not.
        let mut buf = Vec::new();
        let long = "abcdefghijklmnopqrstuvwxyz";
        assert!(draw_text_line(&mut buf, &ctx, long, &[], 0, theme.bg, NO_HL).unwrap());
        let mut buf2 = Vec::new();
        assert!(!draw_text_line(&mut buf2, &ctx, "abc", &[], 0, theme.bg, NO_HL).unwrap());

        // The overlay emits each configured glyph only for the edges that apply.
        let mut both = Vec::new();
        draw_edge_markers(&mut both, &theme, 0, 0, layout.text_cols, theme.bg, (Some('<'), Some('>'))).unwrap();
        let s = String::from_utf8_lossy(&both);
        assert!(s.contains('<') && s.contains('>'));
        let mut none = Vec::new();
        draw_edge_markers(&mut none, &theme, 0, 0, layout.text_cols, theme.bg, (None, None)).unwrap();
        let s2 = String::from_utf8_lossy(&none);
        assert!(!s2.contains('<') && !s2.contains('>'));
    }

    #[test]
    fn multiple_color_columns_are_drawn() {
        let theme = crate::theme::matrix();
        let layout = Layout::compute(20, 24, 1, false, false);
        let mut buf: Vec<u8> = Vec::new();
        // Line "ab"; guides at 0-based columns 1 (inside, the 'b') and 4 (past EOL).
        let ctx = RenderCtx {
            theme: &theme, layout: &layout, left: 0, sel: None, linewise: false, search: None,
            tab_width: 4, block: None, list: false, listchars: ('▸', '·', '·', '$'),
            space_mark: ' ', nbsp_mark: '␣', color_cols: &[1, 4],
        };
        draw_text_line(&mut buf, &ctx, "ab", &[], 0, theme.bg, NO_HL).unwrap();
        let out = String::from_utf8_lossy(&buf);
        // The color-column background should appear for both guide cells.
        let mut probe: Vec<u8> = Vec::new();
        queue!(probe, SetBackgroundColor(theme.color_column_bg)).unwrap();
        let esc = String::from_utf8_lossy(&probe).into_owned();
        assert!(out.matches(&esc).count() >= 2, "both guide columns should be colored");
    }

    #[test]
    fn list_off_renders_no_markers() {
        let theme = crate::theme::matrix();
        let layout = Layout::compute(80, 24, 1, false, false);
        let mut buf: Vec<u8> = Vec::new();
        let ctx = RenderCtx {
            theme: &theme, layout: &layout, left: 0, sel: None, linewise: false, search: None,
            tab_width: 4, block: None, list: false, listchars: ('▸', '·', '·', '$'), space_mark: ' ', nbsp_mark: '␣', color_cols: &[],
        };
        draw_text_line(&mut buf, &ctx, "a\tb  ", &[], 0, theme.bg, NO_HL).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(!out.contains('▸'));
        assert!(!out.contains('$'));
    }

    #[test]
    fn draw_menu_renders_bar_and_dropdown() {
        use crate::menu::{build_menus, MenuState};
        let mut state = MenuState::new(build_menus(&["matrix"], &["rust"], &["wordcount"], crate::i18n::Locale::English));
        state.stack = vec![0]; // open the File dropdown
        let layout = Layout::compute(80, 24, 10, true, false);
        let theme = crate::theme::matrix();
        let mut buf: Vec<u8> = Vec::new();
        draw_menu(&mut buf, &theme, &layout, &state).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains("File"));
        assert!(out.contains("Write"));
        assert!(out.contains('┌') && out.contains('┘')); // a box was drawn
    }

    #[test]
    fn menu_geometry_and_hit_test() {
        use crate::menu::{build_menus, MenuState};
        let mut state = MenuState::new(build_menus(&["matrix"], &["rust"], &["wordcount"], crate::i18n::Locale::English));
        state.stack = vec![0]; // File dropdown open
        let geom = menu_geometry(&state, 80, 24);
        assert_eq!(geom.len(), 1);
        let g = geom[0];
        // The first item sits at row g.y+1 within the box; a click there hits item 0.
        assert_eq!(menu_hit_test(&geom, g.x + 1, g.y + 1), Some((0, 0)));
        // A click on the second row hits item 1.
        assert_eq!(menu_hit_test(&geom, g.x + 1, g.y + 2), Some((0, 1)));
        // A click outside the box misses.
        assert_eq!(menu_hit_test(&geom, g.x + g.width + 5, g.y + 1), None);
    }

    #[test]
    fn menu_bar_positions_account_for_titles() {
        let titles = vec!["File".to_string(), "Edit".to_string()];
        // "File" is 4 chars -> segment 6 + 1 gap = 7; next at 1+7 = 8.
        assert_eq!(menu_bar_positions(&titles), vec![1, 8]);
    }

    #[test]
    fn synchronized_update_emits_mode_2026() {
        // The anti-flicker fix relies on DEC private mode 2026: the frame is
        // bracketed so the terminal presents the text area and the menu overlay
        // as one atomic update. Guard the exact sequences we depend on.
        let mut begin: Vec<u8> = Vec::new();
        queue!(begin, BeginSynchronizedUpdate).unwrap();
        assert_eq!(begin, b"\x1b[?2026h");
        let mut end: Vec<u8> = Vec::new();
        queue!(end, EndSynchronizedUpdate).unwrap();
        assert_eq!(end, b"\x1b[?2026l");
    }

    #[test]
    fn compose_row_right_aligns_hint() {
        let row = compose_row("Write", "wq", 12);
        assert_eq!(row.chars().count(), 12);
        assert!(row.starts_with(" Write"));
        assert!(row.ends_with("wq "));
    }

    #[test]
    fn compose_row_truncates_when_too_narrow() {
        let row = compose_row("A very long label", "x", 8);
        assert_eq!(row.chars().count(), 8);
    }

    #[test]
    fn wildmenu_shows_all_items_when_they_fit() {
        let items = vec!["wq".to_string(), "wall".to_string(), "write".to_string()];
        let cells = wildmenu_cells(&items, 1, 80);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0], (" wq ".to_string(), false));
        assert_eq!(cells[1], (" wall ".to_string(), true)); // selected
        assert_eq!(cells[2], (" write ".to_string(), false));
    }

    #[test]
    fn wildmenu_window_keeps_selected_visible() {
        let items: Vec<String> = (0..10).map(|i| format!("opt{i}")).collect();
        // Narrow width: only a few cells fit; the selected one must be present.
        let cells = wildmenu_cells(&items, 9, 16);
        assert!(!cells.is_empty());
        assert!(cells.iter().any(|(t, sel)| *sel && t.contains("opt9")));
        // Everything shown fits the width.
        let total: usize = cells.iter().map(|(t, _)| t.chars().count()).sum();
        assert!(total <= 16);
    }

    #[test]
    fn wildmenu_empty_for_no_items() {
        assert!(wildmenu_cells(&[], 0, 80).is_empty());
    }

    #[test]
    fn gutter_width_scales_with_line_count() {
        assert_eq!(gutter_width(5, true), 4); // min 3 digits + 1
        assert_eq!(gutter_width(1000, true), 5); // 4 digits + 1
        assert_eq!(gutter_width(100000, true), 7); // 6 digits + 1
        assert_eq!(gutter_width(42, false), 0);
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
