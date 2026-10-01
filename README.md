# rvim

A modular, **vim-emulating terminal text editor** written in Rust. It runs
identically in **bash** and **PowerShell** (and any ANSI terminal) via
[crossterm](https://crates.io/crates/crossterm), with modal editing, color
theming, and syntax highlighting that autodetects and color-codes several
languages.

> Status: **v0.1** — a solid, tested foundation. Modal editing, motions,
> editing operators, search, undo/redo, three themes, seven language
> highlighters, a plugin system, and basic mouse support are all working.

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

Press `:help` inside the editor for a keybinding cheatsheet, and `:q` to quit.

---

## Features

### Modal editing (vim-style)
- **Modes:** Normal, Insert, Visual, Visual-Line, Command.
- **Motions:** `h j k l`, arrows, `w`/`b`/`e` (word) and `W`/`B`/`E` (WORD),
  `0`/`^`/`$`, `{`/`}` (paragraph), `gg`/`G`/`<n>gg`/`<n>G`,
  `f`/`F`/`t`/`T`+`;`/`,` (find char on line), `%` (matching bracket),
  `H`/`M`/`L` (top/middle/bottom of screen), `Ctrl-d`/`Ctrl-u` (half-page).
- **Scrolling:** `zz`/`zt`/`zb` (center/top/bottom the current line),
  `Ctrl-e`/`Ctrl-y` (scroll one line).
- **Editing:** `i a I A o O`, `x`/`X`, `r<c>` (with count), `~` (toggle case),
  `s`/`S`, `D`/`C`, `Y` (yank line), `p`/`P`, `J`/`gJ` (join with/without space),
  `>>`/`<<` (indent, honoring `shiftwidth`), counts (e.g. `5j`).
- **Operators + motions:** `d`, `y`, `c` compose with motions — `dw`/`dW`/`yw`,
  `d$`/`y$`, `d0`, `de`/`dE`, `dj`/`dk`, `dG`/`dgg` (to EOF/BOF), `d}` and the
  doubled `dd`/`yy`/`cc`. `cw`/`cW` act like `ce`/`cE` (vim's special case).
  `>`/`<` also take a motion (`>j`, `>G`).
- **Counts:** prefix motions, operators and paste with a number — `5j`, `3dd`,
  `d3w`, `2d3w` (multiplied), `3p`, `3rx`, `<n>gg`.
- **Text objects:** `d`/`y`/`c` + `i`/`a` + object — `iw`/`aw` (word),
  `iW`/`aW` (WORD), `ip`/`ap` (paragraph), `i(` `i{` `i[` `i<` and `i"` `i'`
  `` i` `` (inner), `a(` … (around). E.g. `diw`, `ci(`, `yi"`, `dap`.
- **Case operators:** `gu`/`gU`/`g~` (lower/upper/toggle) over a motion, a text
  object, or doubled for the whole line — `guw`, `gUiw`, `g~$`, `guu`.
- **Comment toggling:** `gcc` toggles the current line, `gc<motion>` a range
  (e.g. `gcj`, `gcG`), and `gc` in visual mode the selection — using the current
  language's comment marker (`//`, `--`, `;`), indentation preserved.
- **Registers:** `"a`–`"z` prefix any yank/delete/paste to use a named register
  (e.g. `"ayy` … `"ap`). Vim's read-only registers are populated automatically:
  `"0` (last yank), `"1`–`"9` (recent line/multi-line deletes, shifted), and
  `"-` (last small delete). The unnamed register is used when none is given.
- **Marks:** `m<letter>` sets a mark, `` `<letter> `` jumps to it (exact),
  `'<letter>` jumps to its line; `` `` `` / `''` return to the previous position
  (also set by `G`, `gg`, and searches).
- **Jump list:** `Ctrl-o` jumps to an older position, `Ctrl-i` (or `Tab`) to a
  newer one — populated by `G`, `gg`, searches and mark jumps.
- **Macros:** `q<reg>` records keystrokes, `q` stops, `@<reg>` replays, `@@`
  repeats the last macro (a `recording @x` indicator shows in the status line).
- **Insert mode:** autoindent on Enter (`:set autoindent`/`noai`), `Ctrl-w`
  (delete word before cursor), `Ctrl-u` (delete to line start), `Tab` (4 spaces).
- **Repeat:** `.` repeats the last change (a delete, paste, replace, indent, or a
  whole insert/change session).
- **Undo/redo:** `u` / `Ctrl-r` (snapshot-based, bounded history).
- **Visual mode:** `v`/`V` then `d`/`x`, `y`, `c`/`s`, `>`/`<` (indent), and
  `u`/`U`/`~` (lower/upper/toggle case of the selection).
- **Search:** `/pattern`, `?pattern`, `n`/`N` (wraps around); patterns are
  **regular expressions** (e.g. `/\bfn\s+\w+`). `*`/`#` search the word under the
  cursor (whole word) forward/back; `g*`/`g#` do so as a substring. All matches
  are highlighted — clear with `:noh` (`:set hlsearch`/`nohlsearch`).

### Multiple buffers
Open several files and switch between them:

| Command              | Effect                                   |
|----------------------|------------------------------------------|
| `:e <file>`          | open a file (switches to it if already open) |
| `:ls` / `:buffers`   | list open buffers (active marked `%`, `+` = unsaved) |
| `:bn` / `:bp`        | next / previous buffer                    |
| `:b <n>`             | switch to buffer number `n`               |
| `:bd`                | close the current buffer                  |

When more than one buffer is open, a **tab bar** appears across the top of the
screen listing every buffer (active one highlighted, `+` marks unsaved changes).

`:q` refuses to quit while any open buffer has unsaved changes (use `:q!` to
override).

> Tip: new to the command line? Press **Alt** (or **F10**) to open the menu bar
> and browse every command with its shortcut — see *Menu bar* below.

### Command line (ex commands)
`:w [file]` · `:q` · `:q!` · `:wq` · `:x` · `:e <file>` · `:ls` · `:bn`/`:bp`/`:b <n>`/`:bd` · `:<n>` (goto line) ·
`:s/pat/rep/[g]` (search & replace) · `:theme <name>` · `:set number|nonumber` ·
`:set relativenumber|norelativenumber` · `:set ft=<lang>` · `:set mouse|nomouse` ·
`:set autoindent` · `:set expandtab|noexpandtab` · `:set shiftwidth=N` ·
`:set tabstop=N` · `:noh` / `:set hlsearch|nohlsearch` · `:sort[!] [u]` ·
`:source <file>` · `:help` · `:version`

**Search & replace** (`:s`) supports ranges and the `g` (global) flag:

| Command            | Effect                                        |
|--------------------|-----------------------------------------------|
| `:s/foo/bar/`      | first `foo` on the current line               |
| `:s/foo/bar/g`     | every `foo` on the current line               |
| `:%s/foo/bar/g`    | every `foo` in the whole file                 |
| `:2,5s/foo/bar/`   | first `foo` per line, lines 2–5               |
| `:.,$s/foo//g`     | delete every `foo` from the cursor line to EOF|

Patterns are **regular expressions**, and the replacement supports capture
groups (`$1`, `${name}`) — e.g. `:%s/(\w+)=(\w+)/$2=$1/g`. An invalid regex
falls back to a literal match. A single undo (`u`) reverts an entire
substitution.

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
├── editor.rs      cursor, viewport, motions, edit operations, search
├── buffer.rs      text storage + edit primitives + undo/redo
├── mode.rs        the modal state enum
├── command.rs     ex-command parser (`:...`)
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

Current suite: **228 tests** across buffer, editor, menu, syntax, themes,
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

MIT.
