//! Rendering: the gutter, the syntax-highlighted text area, the status line and
//! the command line. All drawing goes through crossterm's `queue!` for a single
//! buffered flush per frame.

use crate::buffer::Position;
use crate::editor::Editor;
use crate::menu::{MenuAction, MenuItem, MenuState};
use crate::mode::Mode;
use crate::syntax::{Registry, Token, TokenKind};
use crate::theme::Theme;
use crossterm::cursor::{Hide, MoveTo, Show};
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

    // Bracket the whole frame in a synchronized update (DEC mode 2026). On
    // supporting terminals (Windows Terminal, modern xterms) every cell we draw
    // this frame — the text area *and* the menu overlay on top of it — is
    // presented atomically, so the menu never flashes over a half-drawn buffer.
    // Terminals that don't understand the sequence simply ignore it.
    queue!(out, BeginSynchronizedUpdate, Hide, MoveTo(0, 0))?;

    if show_tabline {
        draw_tabline(out, theme, &layout, tabs)?;
    }

    let block = editor.block_rect();
    let sel = if block.is_some() { None } else { editor.selection() };
    let linewise = editor.mode == Mode::VisualLine;
    let search = editor.search_regex();

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
                search, editor.tabstop.max(1), block,
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

    // The menu bar overlays everything and keeps the text cursor hidden.
    if let Some(menu) = editor.menu() {
        draw_menu(out, theme, &layout, menu)?;
        queue!(out, Hide)?;
    } else if editor.mode == Mode::Command {
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

    queue!(out, EndSynchronizedUpdate)?;
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
    search: Option<&regex::Regex>,
    tab_width: usize,
    block: Option<(usize, usize, usize, usize)>,
) -> io::Result<()> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let matches = search.map(|re| crate::pattern::match_ranges(re, line)).unwrap_or_default();

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
        let in_block = block
            .map(|(rmin, rmax, cmin, cmax)| row >= rmin && row <= rmax && ci >= cmin && ci <= cmax)
            .unwrap_or(false);
        let in_match = matches.iter().any(|&(s, e)| ci >= s && ci < e);
        // Priority: selection/block > search match > line background.
        let bg = if selected || in_block {
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
            run.push_str(&" ".repeat(tab_width));
            printed += tab_width;
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
    queue!(out, Print(&content), ResetColor)?;

    // showcmd: the partially-typed command, right-aligned (vim shows it bottom-right).
    // Only in Normal mode, and never let it overlap the left-hand content.
    if editor.mode != Mode::Command {
        let cmd = editor.pending_command();
        if !cmd.is_empty() {
            let cols = layout.cols as usize;
            let shown: String = cmd.chars().rev().take(10).collect::<Vec<_>>()
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

    #[test]
    fn draw_menu_renders_bar_and_dropdown() {
        use crate::menu::{build_menus, MenuState};
        let mut state = MenuState::new(build_menus(&["matrix"], &["wordcount"]));
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
        let mut state = MenuState::new(build_menus(&["matrix"], &["wordcount"]));
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
