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
- **Motions:** `h j k l`, arrows, `w`/`b`/`e` (word), `0`/`^`/`$`, `gg`/`G`,
  `<n>G`, `f`/`F`/`t`/`T`+`;`/`,` (find char on line), `%` (matching bracket),
  `H`/`M`/`L` (top/middle/bottom of screen), `Ctrl-d`/`Ctrl-u` (half-page).
- **Scrolling:** `zz`/`zt`/`zb` (center/top/bottom the current line),
  `Ctrl-e`/`Ctrl-y` (scroll one line).
- **Editing:** `i a I A o O`, `x`, `r<c>`, `~` (toggle case), `s`/`S`,
  `dd`/`dw`/`d$`/`D`, `cc`/`cw`/`C`, `yy`, `p`/`P`, `J` (join),
  `>>`/`<<` (indent/dedent, also on a visual selection), counts (e.g. `5j`).
- **Registers:** `"a`–`"z` prefix any yank/delete/paste to use a named register
  (e.g. `"ayy` … `"ap`); the unnamed register is used otherwise.
- **Undo/redo:** `u` / `Ctrl-r` (snapshot-based, bounded history).
- **Visual mode:** `v`/`V` then `d`/`y`/`c`, `>`/`<` (indent), and
  `u`/`U`/`~` (lower/upper/toggle case of the selection).
- **Search:** `/pattern`, `?pattern`, `n`/`N` (wraps around); all matches are
  highlighted — clear the highlight with `:noh` (`:set hlsearch`/`nohlsearch`).

### Command line (ex commands)
`:w [file]` · `:q` · `:q!` · `:wq` · `:x` · `:e <file>` · `:<n>` (goto line) ·
`:s/pat/rep/[g]` (search & replace) · `:theme <name>` · `:set number|nonumber` ·
`:set relativenumber|norelativenumber` · `:set ft=<lang>` · `:set mouse|nomouse` ·
`:noh` / `:set hlsearch|nohlsearch` · `:sort[!] [u]` · `:source <file>` ·
`:help` · `:version`

**Search & replace** (`:s`) supports ranges and the `g` (global) flag:

| Command            | Effect                                        |
|--------------------|-----------------------------------------------|
| `:s/foo/bar/`      | first `foo` on the current line               |
| `:s/foo/bar/g`     | every `foo` on the current line               |
| `:%s/foo/bar/g`    | every `foo` in the whole file                 |
| `:2,5s/foo/bar/`   | first `foo` per line, lines 2–5               |
| `:.,$s/foo//g`     | delete every `foo` from the cursor line to EOF|

Matching is **literal** (not yet regex — see roadmap). A single undo (`u`)
reverts an entire substitution.

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

Current suite: **129 tests** across buffer, editor, syntax, themes, commands,
config, plugins, modes, and UI layout.

---

## Roadmap

- Regex support for `:s` (currently literal matching)
- Multi-line string highlighting (block comments spanning lines ✅ done)
- Split windows / multiple buffers & tabs
- User-defined key mappings in `~/.rvimrc` (config file loading ✅ done)
- Registers (named), macros (`q`), marks
- Richer mouse (drag-select), and a menu bar
- More accessibility options (screen-reader hints, configurable font-agnostic cues)
- Dynamic plugin loading

---

## License

MIT.
