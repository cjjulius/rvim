# rvim

A modular, **vim-emulating terminal text editor** written in Rust. It runs
identically in **bash** and **PowerShell** (and any ANSI terminal) via
[crossterm](https://crates.io/crates/crossterm), with modal editing, color
theming, and syntax highlighting that autodetects and color-codes several
languages.

> Status: **v0.2** — a mature, deeply vim-compatible editor. Modal editing, the
> full operator/motion/text-object grammar, incremental search, line-range ex
> commands, `:normal`, visual-mode operators, four themes, seven language
> highlighters, a plugin system, and mouse support are all working. See
> [CHANGELOG.md](CHANGELOG.md) for what landed in 0.2.

---

## Quick start

Build once, then run:

```bash
cargo run --release
```

Open a file (language is autodetected from the extension):

```bash
cargo run --release -- src/main.rs
```

Start with a specific theme:

```bash
cargo run --release -- --theme retrowave examples/sample.pgsql
```

Or use the prebuilt binary directly:

```bash
# bash
./target/release/rvim examples/sample.rs
```

```powershell
# PowerShell
.\target\release\rvim.exe examples\sample.rs
```

Convenience launchers (add the repo root to your PATH, or call them directly):

```bash
./scripts/rvim.sh --theme cobalt examples/sample.tsql
```

```powershell
.\scripts\rvim.ps1 --theme cobalt examples\sample.tsql
```

Press `:help` inside the editor for a keybinding cheatsheet (it opens in its own
buffer, leaving your file untouched — `:bd` closes it), and `:q` to quit.

---

## Features

### Modal editing (vim-style)
- **Modes:** Normal, Insert, Replace (`R` — overtype), Visual, Visual-Line,
  Visual-Block (`Ctrl-v`), Command.
- **Motions:** `h j k l`, arrows, `w`/`b`/`e`/`ge` (word) and `W`/`B`/`E`/`gE` (WORD),
  `0`/`^`/`$`/`g_` (line ends), `|` (column), `+`/`-`/`Enter` (line first
  non-blank), `{`/`}` (paragraph), `(`/`)` (sentence, counted),
  `[[`/`]]`/`[]`/`][` (section — brace in
  column 0), `[(`/`[{`/`])`/`]}` (unmatched enclosing bracket, counted),
  `gg`/`G`/`<n>gg`/`<n>G`,
  `f`/`F`/`t`/`T`+`;`/`,` (find char on line, counted — `3fx` — and usable as
  operator motions — `dfx`, `ct)`, `dFx`), `%` (matching bracket; also an
  operator motion — `d%`/`y%`/`c%` act from the cursor to the match, across lines),
  `<n>%` (jump to n% of the file),
  `H`/`M`/`L` (top/middle/bottom of screen; `<n>H`/`<n>L` count in from the
  edge; also line-wise operator motions — `dL`, `yH`), `Ctrl-d`/`Ctrl-u` (half-page).
- **Scrolling:** `zz`/`zt`/`zb` (center/top/bottom the current line),
  `z.`/`z<CR>`/`z-` (same, then jump to first non-blank),
  `Ctrl-f`/`Ctrl-b` (full page forward/back, 2-line overlap, counted),
  `Ctrl-e`/`Ctrl-y` (scroll one line).
- **Editing:** `i a I A o O`, `gI` (insert at column 0), `x`/`X`, `r<c>` (with
  count), `~` (toggle case), `s`/`S`, `D`/`C`, `Y` (yank line), `p`/`P`,
  `gp`/`gP` (paste, cursor after), `]p`/`[p` (paste linewise, reindented to the
  current line), `J`/`gJ` (join with/without space; counted),
  `>>`/`<<` (indent, honoring `shiftwidth`) — also as an operator over a motion
  or text object (`>ip`, `>i{`, `<ap`), counts (e.g. `5j`).
- **Operators + motions:** `d`, `y`, `c` compose with motions — `dw`/`dW`/`yw`,
  `d$`/`y$`, `d0`, `de`/`dE`, `dj`/`dk`, `dG`/`dgg` (to EOF/BOF), `d}` and the
  doubled `dd`/`yy`/`cc`. `cw`/`cW` act like `ce`/`cE` (vim's special case).
  `>`/`<` also take a motion (`>j`, `>G`).
- **Counts:** prefix motions, operators, paste and inserts with a number —
  `5j`, `3dd`, `d3w`, `2d3w` (multiplied), `3p`, `3rx`, `<n>gg`, and counted
  inserts (`3ihi<Esc>` → `hihihi`, `3o`, `3a`).
- **Text objects:** `d`/`y`/`c` + `i`/`a` + object — `iw`/`aw` (word),
  `iW`/`aW` (WORD), `ip`/`ap` (paragraph), `is`/`as` (sentence),
  `ii`/`ai` (indentation block),
  `i(` `i{` `i[` `i<` and `i"` `i'`
  `` i` `` (inner), `a(` … (around). E.g. `diw`, `ci(`, `yi"`, `dap`, `das`, `dii`.
  Bracket objects (`i(`/`i{`/`i[`/`i<` and their `a` forms) span multiple lines,
  so `ci{` / `da(` work on a block or argument list across rows.
  The indentation object `ii` selects the run of lines indented at least as far
  as the cursor's (interior blank lines kept); `ai` also takes the header line
  above — handy for operating on a whole indented block (`dii`, `cii`, `>ai`).
- **Case operators:** `gu`/`gU`/`g~` (lower/upper/toggle) and `g?` (ROT13) over
  a motion, a text object, or doubled for the whole line — `guw`, `gUiw`, `g~$`,
  `guu`, `g?w`, `g?ip`, `g??`; `g?` also works on a visual selection.
- **Comment toggling:** `gcc` toggles the current line, `gc<motion>` a range
  (e.g. `gcj`, `gcG`), and `gc` in visual mode the selection — using the current
  language's comment marker (`//`, `--`, `;`), indentation preserved.
- **Registers:** `"a`–`"z` prefix any yank/delete/paste to use a named register
  (e.g. `"ayy` … `"ap`); the uppercase name `"A`–`"Z` *appends* to that register
  (`"Ayy` adds to `"a`). Vim's read-only registers are populated automatically:
  `"0` (last yank), `"1`–`"9` (recent line/multi-line deletes, shifted), and
  `"-` (last small delete). The unnamed register is used when none is given. The
  black-hole register `"_` (e.g. `"_dd`) deletes without disturbing any register.
  The read-only `"%` register holds the current file name (`"%p`, or `Ctrl-r %`
  in insert mode), and `".` holds the last inserted text (`".p`, `Ctrl-r .`, or
  `Ctrl-a` in insert mode to re-insert it).
- **Marks:** `m<letter>` sets a mark, `` `<letter> `` jumps to it (exact),
  `'<letter>` jumps to its line; `` `` `` / `''` return to the previous position
  (also set by `G`, `gg`, and searches). Automatic marks: `` `. `` (last change),
  `` `^ `` (last insert), and `` `[ `` / `` `] `` (start / end of the text just
  changed, yanked, or put). `gi` resumes insert at the last insert position. Marks
  also work as ex-command addresses — `:'a,'bd`, `:'<,'>s/…` — including the
  `'<`/`'>` selection marks.
- **Jump list:** `Ctrl-o` jumps to an older position, `Ctrl-i` (or `Tab`) to a
  newer one — populated by `G`, `gg`, searches and mark jumps.
- **Change list:** `g;` jumps to an older edit position, `g,` to a newer one —
  so you can hop back to where you were just editing.
- **Macros:** `q<reg>` records keystrokes, `q` stops, `@<reg>` replays, `@@`
  repeats the last macro (a `recording @x` indicator shows in the status line).
  Recording to an uppercase register (`qA`) appends to that macro instead of
  overwriting it.
  `@:` repeats the last `:` command-line command.
- **Insert mode:** autoindent on Enter (`:set autoindent`/`noai`), `Ctrl-w`
  (delete word before cursor), `Ctrl-u` (delete to line start), `Ctrl-r<reg>`
  (paste a register), `Ctrl-t`/`Ctrl-d` (indent/dedent line), `Tab`.
- **Digraphs (`Ctrl-k`):** `Ctrl-k` plus two characters inserts a special
  character — e.g. `Ctrl-k a:` → `ä`, `Ctrl-k e'` → `é`, `Ctrl-k n~` → `ñ`,
  `Ctrl-k ->` → `→`, `Ctrl-k Eu` → `€`, `Ctrl-k +-` → `±`. The two keys may be
  given in either order, matching vim; the covered set spans accented Latin
  letters, common ligatures, currency, and a handful of math/arrow symbols.
- **Keyword completion:** `Ctrl-n` / `Ctrl-p` complete the word before the cursor
  from other words in the buffer, cycling forward / backward through the matches.
- **One-shot normal (`Ctrl-o`):** in insert mode, `Ctrl-o` runs a single
  Normal-mode command (e.g. `Ctrl-o dd`, `Ctrl-o 0`) and returns to insert.
- **Copy adjacent char:** in insert mode, `Ctrl-e` / `Ctrl-y` insert the character
  directly below / above the cursor.
- **Reflow:** `gq{motion}` / `gqq` / `gqip` / `gqap` (and `gw`) rewrap lines to
  `:set textwidth=N` (default 79), preserving the first line's indent — great for
  comments and prose. Works on a visual selection too.
- **Repeat:** `.` repeats the last change (a delete, paste, replace, indent, or a
  whole insert/change session); a count repeats it that many times (`3.`).
- **Info:** `Ctrl-g` shows the file name, modified flag, line count and position;
  `g Ctrl-g` reports word / character / byte counts; `ga` shows the character
  under the cursor as decimal / hex / octal.
- **Numbers:** `Ctrl-a` / `Ctrl-x` increment / decrement the number under (or
  next on) the line, with a count (`10Ctrl-a`); handles negatives, and
  recognizes hexadecimal (`0x1f`) and binary (`0b1010`) literals — preserving
  the prefix, digit width, and hex letter case. In visual mode they bump the
  first number on every selected line at once; `g Ctrl-a` / `g Ctrl-x` instead
  build an incrementing sequence (1, 2, 3, … — with `{count}` as the step).
- **Undo/redo:** `u` / `Ctrl-r` (snapshot-based, bounded history), counted
  (`3u`, `2Ctrl-r`); `:earlier [N]` / `:later [N]` step back/forward N changes.
- **Visual mode:** `v`/`V` then `d`/`x`, `y`, `c`/`s`, `>`/`<` (indent),
  `J`/`gJ` (join the selected lines), `r<c>` (replace every selected char with
  `c`), and `u`/`U`/`~`
  (lower/upper/toggle case of the selection). `o` swaps the active end of the
  selection; `gv` (from normal mode) reselects the last selection. `*`/`#` search
  for the selected text (literally) forward/back. `i`/`a` + an object select a
  text object (`viw`, `vi(`, `vap`, …).
  Pressing `:` from visual mode prefills the command line with the selection
  range (`:'<,'>`), so any ex-command — `:'<,'>s/…`, `:'<,'>d`, `:'<,'>m0` — runs
  on the selected lines. `p`/`P` over a selection replaces it with the register
  (the replaced text goes to the unnamed register).
- **Visual block (`Ctrl-v`):** select a rectangle, then `d`/`x` to delete it,
  `I`/`A` to insert/append text on every row, or `c` to change the block.
- **Search:** `/pattern`, `?pattern`, `n`/`N` (repeat in the last search's
  direction / reversed; wraps around); patterns are
  **regular expressions** (e.g. `/\bfn\s+\w+`). `*`/`#` search the word under the
  cursor (whole word) forward/back; `g*`/`g#` do so as a substring; `gd`/`gD`
  jump to the identifier's definition (nearest earlier use / first in file). All matches
  are highlighted — with the match the cursor is on shown in a brighter colour —
  clear with `:noh` (`:set hlsearch`/`nohlsearch`). Case
  handling follows `:set ignorecase` and `:set smartcase` (an uppercase letter in
  the pattern forces a case-sensitive search), with per-pattern `\c`/`\C` overrides.
  With `:set incsearch` (on by default) the first match is previewed as you type;
  `Enter` jumps to it, `Esc` returns to where you started. After a search (and on
  `n`/`N`) a `[index/total]` count shows where you are among the matches. Searches
  wrap around the file by default; `:set nowrapscan` stops at the last/first match.
  Search offsets are supported and reused by `n`/`N`: `/pat/e[±N]` (end of match),
  `/pat/s[±N]` / `/pat/b[±N]` (start of match), and `/pat/±N` (N lines away).
  `gn`/`gN` visually select the match under or after/before the cursor, and work
  as operator targets (`cgn`, `dgn`, `ygn`) — so `cgn` to change a match, then
  `.` to change the next, is the quick search-and-replace-by-hand workflow.

### Global command (`:g` / `:v`)
Run a command on every line matching a pattern:

| Command             | Effect                                             |
|---------------------|----------------------------------------------------|
| `:g/re/d`           | delete all lines matching `re`                      |
| `:v/re/d` (`:g!/re/d`) | delete all lines *not* matching `re`            |
| `:g/re/s/a/b/g`     | run the substitution on matching lines only         |
| `:g/re/normal A;`   | run Normal-mode keys on each matching line (`:normal`) |

### Multiple buffers
Open several files and switch between them:

| Command              | Effect                                   |
|----------------------|------------------------------------------|
| `:e <file>`          | open a file (switches to it if already open) |
| `:ls` / `:buffers`   | list open buffers (active marked `%`, `+` = unsaved) |
| `:bn` / `:bp`        | next / previous buffer                    |
| `:b <n>`             | switch to buffer number `n`               |
| `Ctrl-^` / `:b#`     | switch to the alternate (last) buffer     |
| `:bd`                | close the current buffer                  |

When more than one buffer is open, a **tab bar** appears across the top of the
screen listing every buffer (active one highlighted, `+` marks unsaved changes).

`:q` refuses to quit while any open buffer has unsaved changes (use `:q!` to
override). In Normal mode, `ZZ` writes the buffer and quits (like `:x`) and `ZQ`
quits without saving (like `:q!`).

> Tip: new to the command line? Press **Alt** (or **F10**) to open the menu bar
> and browse every command with its shortcut — see *Menu bar* below.

### Command line (ex commands)
`:w [file]` · `:q` · `:q!` · `:wq` · `:x` · `:qa`/`:wa`/`:wqa` (all buffers) · `:e <file>` · `:ls` · `:bn`/`:bp`/`:b <n>`/`:bd` · `Ctrl-^`/`:b#` (alternate buffer) · `:<n>` (goto line) ·
`:s/pat/rep/[g]` (search & replace) · `:theme <name>` · `:set number|nonumber` ·
`:set relativenumber|norelativenumber` · `:set ft=<lang>` · `:set mouse|nomouse` ·
`:set autoindent` · `:set expandtab|noexpandtab` · `:set shiftwidth=N` ·
`:set tabstop=N` · `:set scrolloff=N` (keep N lines of context around the cursor) ·
`:set textwidth=N` (wrap column for `gq`) · `:set sidescrolloff=N` (horizontal context) ·
`:set ignorecase|noignorecase` · `:set smartcase|nosmartcase` · `:set incsearch|noincsearch` ·
`:set list|nolist` (show whitespace) · `:set wrapscan|nowrapscan` · `:set cursorline|nocursorline` · `:set cursorcolumn|nocursorcolumn` ·
`:set colorcolumn=N` (column guide) · `:set {option}?` (show an option's current value) ·
`:noh` / `:set hlsearch|nohlsearch` · `:[range]sort[!] [u][n][i] [/pat/ [r]]` (reverse / unique / numeric / ignore-case; sort by text after `/pat/`, or the match itself with `r`; range-aware) ·
`:[range]m[ove] {addr}` / `:[range]t`|`:[range]co[py] {addr}` (move / copy lines) ·
`:[range]d[elete]` / `:[range]y[ank]` / `:[range]>`|`:[range]<` (delete / yank / shift lines) ·
`:[range]j[oin][!]` (join lines; `!` keeps whitespace) ·
`:[range]ce[nter] [w]` / `:[range]ri[ght] [w]` / `:[range]le[ft] [indent]` (align lines; width defaults to `textwidth`) ·
`:[addr]pu[t] [reg]` (put a register as lines) · `:[range]norm[al] {keys}` (run Normal-mode keys, per line over a range) ·
`:r[ead] <file>` (insert a file below the cursor) · `:e`/`:e!` (reload current file) ·
`:marks` · `:registers`/`:reg` · `:jumps` · `:changes` · `:history [:|/|all]` (introspection listings) · `:earlier [N]`/`:later [N]` (undo/redo N) ·
`:source <file>` · `:help` · `:version`

Several `:set` options can be combined in one command, e.g.
`:set number expandtab shiftwidth=2`. `:set {option}?` shows one option's value;
bare `:set` lists the options changed from their defaults, and `:set all` lists
every option.

On the command line, `Up`/`Down` recall previous commands (`:` history) or
searches (`/`/`?` history), and `Ctrl-w`/`Ctrl-u` delete the previous word / the
whole line. `Tab` completes the command name (`:sor`→`:sort`) — or, after
`:set `, the option name (`:set nu`→`:set number`) — and repeated `Tab` /
`Shift-Tab` cycle forward / backward through the matches. When more than one
matches, a **wildmenu** of candidates appears just above the command line with
the current pick highlighted, scrolling to keep it in view on a narrow terminal.

**Search & replace** (`:s`) supports ranges and the `g` (global) flag:

| Command            | Effect                                        |
|--------------------|-----------------------------------------------|
| `:s/foo/bar/`      | first `foo` on the current line               |
| `:s/foo/bar/g`     | every `foo` on the current line               |
| `:%s/foo/bar/g`    | every `foo` in the whole file                 |
| `:2,5s/foo/bar/`   | first `foo` per line, lines 2–5               |
| `:.,$s/foo//g`     | delete every `foo` from the cursor line to EOF|
| `:%s/foo//n`       | count matches of `foo` (the `n` flag; no change) |

Patterns are **regular expressions**, and the replacement uses **vim-style
backreferences** — `\1`–`\9` for groups and `&` for the whole match — e.g.
`:%s/(\w+)=(\w+)/\2=\1/g`. An invalid regex falls back to a literal match.
`&` (normal mode) repeats the last substitution on the current line, and `g&`
repeats it across the whole file. The `i`
flag makes matching case-insensitive (`:%s/foo/bar/gi`), and `\c`/`\C` in a
pattern force case-insensitive/sensitive matching for both `:s` and search. A
single undo (`u`) reverts an entire substitution.

### Menu bar (press Alt, or F10)
An auto-hiding menu bar lives at the top of the screen. It's a **teaching aid**
for the command line, not a replacement: every item shows its command-line
shortcut, and choosing one drops you into the `:` command line with that command
pre-filled so you learn it by doing.

- **Open:** `Alt`+a letter (e.g. `Alt`+`f` opens *File*) or `F10`. The letter
  jumps straight to the matching menu.
- **Navigate:** arrow keys or `h`/`j`/`k`/`l`; `Enter` opens a submenu or selects
  an item; items marked `▸` open a cascading submenu with clear borders.
- **Long lists scroll** (▲/▼ in the border) — e.g. *View ▸ Theme*,
  *Language ▸ Set Filetype*.
- **Esc** backs out one layer at a time; the last `Esc` hides the bar and returns
  to the cursor.
- **Mouse** (with `:set mouse`): click a top-level title to open it, click a
  dropdown item to open its submenu or run it, click empty space to dismiss.
- **Flicker-free:** each frame (text area + menu overlay) is painted inside a
  synchronized terminal update (DEC mode 2026), so the dropdown never flashes
  over a half-drawn buffer on terminals that support it.

Menus are grouped logically — **File**, **Buffers**, **Edit**, **View**,
**Language**, **Tools**, **Help** — and together list every command-line command
rvim supports (e.g. `Write & Quit  wq`).

### Configuration (`~/.rvimrc`)
On startup rvim runs the ex-commands in `~/.rvimrc` (or `$RVIMRC`, or
`%USERPROFILE%\.rvimrc` on Windows). Comments start with `"` or `#`:

```text
" ~/.rvimrc
theme cobalt
set number
set mouse
```

Skip it with `rvim --no-config`. A CLI `--theme` overrides the config file.
You can also load a settings file at runtime with `:source <file>`. See
[`examples/rvimrc.example`](examples/rvimrc.example).

### Color theming
Three built-in themes, switchable live with `:theme <name>` (or cycle with a
bare `:theme`):

| Theme            | Vibe                                              |
|------------------|---------------------------------------------------|
| `matrix`         | green phosphor on black (default)                 |
| `retrowave`      | neon pink/cyan on deep purple                     |
| `cobalt`         | warm gold/cyan accents on deep blue               |
| `high-contrast`  | **accessibility:** pure black/white, colorblind-safe (Okabe–Ito) token palette |

### Syntax highlighting + language autodetection
Languages are detected from the file extension (with a content-based fallback),
then tokenized and color-coded:

| Language              | Extensions                          |
|-----------------------|-------------------------------------|
| Rust                  | `.rs`                               |
| T-SQL (SQL Server)    | `.tsql`                             |
| PostgreSQL            | `.pgsql`, `.psql`                   |
| Trino / Starburst SQL | `.trino`, `.trinosql`, `.presto`    |
| Snowflake SQL         | `.snow`, `.snowsql`, `.snowflake`   |
| ANSI SQL              | `.sql` (+ `-- dialect: <x>` hint)   |
| Z80 assembly          | `.z80`, `.asm`, `.s`                |

A generic `.sql` file can be pinned to a dialect with a first-line hint such as
`-- dialect: trino`, or at runtime with `:set ft=snowflake`. Block comments
(`/* … */`) are tracked across line boundaries, so multi-line comments stay
correctly colored even when scrolled.

### Accessibility & navigation
- **High-contrast theme** (`:theme high-contrast`) — pure black/white chrome with
  a colorblind-safe Okabe–Ito token palette for maximum legibility.
- **Relative line numbers** (`:set relativenumber` / `:set rnu`) — hybrid mode:
  the cursor line shows its absolute number (left-aligned to stand out), every
  other line shows its distance, so `12j` / `8k` jumps are countable at a glance.
  `:set norelativenumber` (`nornu`) returns to absolute numbers.
- **Matching-bracket highlight** — when the cursor rests on a `()`, `[]`, or `{}`
  bracket, its partner is highlighted (in reverse video, so it reads on any
  theme) even across lines, making nesting easy to follow.
- **Whitespace view (`:set list`)** — show tabs (`▸···`) and trailing spaces (`·`)
  with an end-of-line `$` marker, so hidden whitespace is visible; `:set nolist`
  hides them again. Markers are width-preserving, so columns stay exact.
- **Color column (`:set colorcolumn=N`)** — highlight column N as a visual
  line-length guide (works past the end of short lines too); `:set cc=0` turns
  it off. Pair it with `:set textwidth` to see your wrap boundary.
- **Command preview (showcmd)** — the partially-typed command (count, operator,
  text-object prefix) appears at the bottom-right as you type, so you can see
  exactly what rvim is waiting for — e.g. `2d` while `2dw` is mid-entry. It clears
  the instant the command completes or is cancelled. In visual mode the same
  corner shows the selection size — column or line count, or `rows x cols` for a
  block — so you always know how much is selected.

### Mouse support
`:set mouse` enables click-to-position and scroll-wheel paging;
`:set nomouse` disables it.

---

## Architecture & modularity

rvim is deliberately split into small, decoupled modules so new components can be
added with minimal coupling:

```
src/
├── main.rs        CLI entry (arg parsing)
├── lib.rs         crate root / module wiring
├── app.rs         event loop + ex-command execution
├── app_tests.rs   app / ex-command execution tests (kept separate)
├── editor.rs      cursor, viewport, motions, edit operations, search
├── editor_tests.rs  editor unit tests (kept out of editor.rs to keep it lean)
├── buffer.rs      text storage + edit primitives + undo/redo
├── mode.rs        the modal state enum
├── command.rs     ex-command parser (`:...`)
├── command_tests.rs  ex-command parser unit tests (kept separate)
├── config.rs      ~/.rvimrc loading + parsing
├── menu.rs        Alt-activated menu bar: data tree + navigation state
├── pattern.rs     regex compilation (literal fallback) for search & :s
├── terminal.rs    raw-mode / alt-screen RAII guard (cross-platform)
├── ui.rs          gutter + highlighted text + status/command lines
├── theme.rs       Theme + ThemeRegistry (matrix, retrowave, cobalt)
├── plugin.rs      Plugin trait + PluginManager (+ example plugin)
└── syntax/
    ├── mod.rs        Highlighter trait, spec-driven tokenizer, detection
    └── languages.rs  per-language specs + custom Z80 highlighter
```

**Adding a language:** describe it with a `LangSpec` (comment markers, keyword /
type / builtin word lists, string rules) and register it in
`syntax::languages::builtin_highlighters`. Truly exotic languages implement the
`Highlighter` trait directly (see `Z80Highlighter`).

**Adding a theme:** construct a `Theme` and push it into
`ThemeRegistry::with_builtins` (or `ThemeRegistry::add` at runtime).

**Adding a plugin:** implement the `Plugin` trait (register ex-commands via
`commands()` and handle them in `on_command`) and register it with the
`PluginManager`. The bundled `WordCountPlugin` (`:wordcount` / `:wc`) is a
worked example.

---

## Testing

Significant components have unit tests. Run everything:

```bash
cargo test
```

Run a focused subset (tests are namespaced by module, so you can skip
inapplicable ones):

```bash
cargo test buffer::      # just the buffer
cargo test editor::      # motions & operators
cargo test syntax::      # highlighters & detection
cargo test command::     # ex-command parsing
```

rvim aims for **keystroke compatibility with vim** so your muscle memory
transfers; see the keybinding sections above. (Some advanced vim features differ
or are absent — those are noted in the roadmap.)

Current suite: **513 tests** across buffer, editor, menu, syntax, themes,
commands, config, pattern, plugins, modes, and UI layout.

---

## Roadmap

- Multi-line string highlighting (block comments spanning lines ✅ done;
  regex search & substitute ✅ done)
- Split windows & tabs (multiple buffers ✅ done)
- User-defined key mappings in `~/.rvimrc` (config file loading ✅ done)
- Ex-commands inside replayed macros; cross-line text objects
  (named registers ✅, marks ✅, macros ✅, `.` repeat ✅, text objects ✅ done)
- Richer mouse (drag-select), and a menu bar
- More accessibility options (screen-reader hints, configurable font-agnostic cues)
- Dynamic plugin loading

---

## License

Released under the [MIT License](LICENSE). © 2026 cjjulius.
