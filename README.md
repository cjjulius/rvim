# rvim

A modular, **vim-emulating terminal text editor** written in Rust. It runs
identically in **bash** and **PowerShell** (and any ANSI terminal) via
[crossterm](https://crates.io/crates/crossterm), with modal editing, color
theming, and syntax highlighting that autodetects and color-codes several
languages.

> Status: **v0.2** — a mature, deeply vim-compatible editor. Modal editing, the
> full operator/motion/text-object grammar, incremental search, line-range ex
> commands, `:normal`, visual-mode operators, nine themes, twenty-three language
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

Open several files at once (the rest become buffers, reachable with `:bn`/`:bp`):

```bash
cargo run --release -- src/main.rs src/editor.rs
```

Open at a specific line with `+N` (bare `+` opens at the last line), or at the
first match of a pattern with `+/PATTERN`. Run an ex command on startup with
`-c <command>` (repeatable), e.g. `-c "set number"`. Open read-only with `-R`
(or `--readonly`) so `:w` is refused unless you force it with `:w!`:

```bash
cargo run --release -- +42 src/main.rs
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

Like vim, rvim is built on a small grammar: an optional **count**, an
**operator**, and a **motion** or **text object**. Learn the pieces and they
combine freely.

```
[count] operator [count] motion | text-object
   3       d                w          ->  3dw    delete three words
           c                i(         ->  ci(    change inside ( … )
   2       >                ip         ->  2>ip   indent a paragraph, twice
```

A bare motion just moves the cursor; doubling an operator (`dd`, `>>`, `==`)
makes it act on whole lines.

**Modes**

| Mode | Enter with | For |
|------|-----------|-----|
| Normal | `Esc` | moving around and running operators (home base) |
| Insert | `i` `a` `o` `I` `A` `O` | typing text |
| Replace | `R` | overtype existing characters |
| Visual / Line / Block | `v` / `V` / `Ctrl-v` | select a span / whole lines / a rectangle |
| Command | `:` `/` `?` | ex commands and search |

**Moving the cursor**

| Move to | Keys |
|---------|------|
| Left, down, up, right | `h` `j` `k` `l` (or arrows) |
| Line start / first non-blank / end | `0` / `^` / `$` (and `g_`, last non-blank) |
| First non-blank of next / previous line | `+` or `Enter` / `-` |
| Column N | `N\|` |
| Word / WORD — forward, back, end | `w` `b` `e` `ge` / `W` `B` `E` `gE` |
| Find a char on the line | `f` `F` `t` `T`, repeat with `;` `,` (counts: `3fx`) |
| Matching bracket | `%` (pairs set by `:set matchpairs`) |
| Paragraph / sentence | `{` `}` / `(` `)` |
| Section / unmatched enclosing bracket | `[[` `]]` `[]` `][` / `[(` `[{` `])` `]}` |
| File top / bottom / line N | `gg` / `G` / `NG` |
| N percent into the file | `N%` |
| Screen top / middle / bottom | `H` `M` `L` |
| Half page / full page | `Ctrl-d` `Ctrl-u` / `Ctrl-f` `Ctrl-b` |
| Recenter line / scroll one line | `zz` `zt` `zb` (also `z.` `z<CR>` `z-`) / `Ctrl-e` `Ctrl-y` |

`j`/`k` keep their goal column across shorter lines (and stay at the line end
after `$`). Almost every motion doubles as an operator target, so `d}`, `y%`,
`dL`, and `c3w` all work.

**Changing text**

| Keys | Action |
|------|--------|
| `d` `y` `c` + motion | delete / yank / change over the motion (`dw`, `y$`, `ci(`, `dG`) |
| `dd` `yy` `cc` | whole line (counts: `3dd`); `cw`/`cW` act like `ce`/`cE` |
| `x` `X` / `r<c>` / `~` | delete char / replace char / toggle case |
| `s` `S` / `C` `D` / `Y` | change char / line, change / delete to end of line, yank line |
| `i` `a` `I` `A` / `o` `O` / `gI` | insert, append, line start/end / open line below/above / insert at column 0 |
| `p` `P` / `gp` `gP` / `]p` `[p` | paste after / before / with cursor after / reindented to the current line |
| `J` `gJ` | join lines, with / without a space |
| `>>` `<<` (or `>`+motion) | indent / dedent — honors `shiftwidth`, `shiftround`; `>ip`, `>i{`, `<ap` |
| `=` (`==`, `=G`, `=ip`) | reindent to brace nesting, or pipe through `:set equalprg` |
| `gu` `gU` `g~` / `g?` | lowercase / uppercase / toggle / ROT13 over a motion or object |
| `gcc` / `gc`+motion | toggle comments (`gcj`, `gcG`, or a visual selection) |

Counts combine everywhere: `5j`, `3dd`, `d3w`, `2d3w` (multiplied), `3p`, `3rx`,
and even counted inserts (`3ihi<Esc>` → `hihihi`, `3o`).

The `=` reindenter ignores braces inside strings and `//` or `/* */` comments and
leaves comment bodies aligned, so `gg=G` tidies a whole file; it is a deliberate
no-op in indentation-structured languages (Python, YAML, …). Point
`:set equalprg=<cmd>` at an external formatter (e.g. `clang-format`, `rustfmt`) to
pipe lines through it instead — escape spaces as `clang-format\ --style=Google`.

**Text objects** — pair an operator with `i` (inner) or `a` (around) plus an object:

| Object | Keys (`i` inner / `a` around) | Selects |
|--------|------------------------------|---------|
| Word / WORD | `iw` `aw` / `iW` `aW` | the word (plus surrounding space for `a`) |
| Sentence / paragraph | `is` `as` / `ip` `ap` | the sentence / paragraph |
| Indent block | `ii` `ai` | lines at/under the cursor's indent (`ai` adds the header line) |
| Brackets | `i(` `i{` `i[` `i<` (+ `a`) | inside / around the pair, across lines |
| Quotes | `i"` `i'` `` i` `` | inside the quotes |
| Tag | `it` `at` | innermost `<name>…</name>` element (nested, multi-line) |

Examples: `diw`, `ci(`, `yi"`, `dap`, `das`, `dii`, `cit`, `dat`. Word and
paragraph objects take counts (`d3iw`, `d2ap`), and the count may lead instead
(`2daw`).

**Registers** — prefix any yank/delete/paste with `"x` to use register `x`:

| Register | Holds |
|----------|-------|
| `"a`–`"z` | named registers (`"ayy` … `"ap`); `"A`–`"Z` *append* to them |
| `"0` | the last yank |
| `"1`–`"9` / `"-` | recent line deletes (shifted) / the last small delete |
| `"_` | the black-hole — `"_dd` deletes without touching any register |
| `"+` / `"*` | system clipboard / PRIMARY selection (one clipboard on Windows & macOS) |
| `"%` `".` `"/` `":` | file name / last insert / last search / last command (read-only) |

With no prefix, the unnamed register is used. `"+y` copies across programs and
`"+p` pastes; rvim uses your platform's clipboard tool (`wl-clipboard`/`xclip`/
`xsel` on Linux), or set `RVIM_CLIPBOARD` to a file path for a shared fallback.

**Marks, jumps & history**

| Feature | Keys |
|---------|------|
| Set / jump to mark (exact / line) | `m<x>` / `` `<x> `` / `'<x>` |
| Back to previous spot | `` `` `` / `''` (also set by `G`, `gg`, searches) |
| Auto marks | `` `. `` last change, `` `^ `` last insert, `` `[ `` `` `] `` last changed text |
| Resume insert at last spot | `gi` |
| Jump list: older / newer | `Ctrl-o` / `Ctrl-i` (or `Tab`) |
| Change list: older / newer edit | `g;` / `g,` |

Marks also work as ex-command addresses — `:'a,'bd`, `:'<,'>s/…` (including the
`'<`/`'>` selection marks).

**Macros & repeat**

| Keys | Action |
|------|--------|
| `q<x>` … `q` | record keystrokes into register `x` (`qA` appends); status shows `recording @x` |
| `@<x>` / `@@` | replay a macro / repeat the last one |
| `@:` | repeat the last `:` command |
| `.` | repeat the last change (`3.` repeats it three times) |

**Insert mode**

| Keys | Action |
|------|--------|
| `Ctrl-w` / `Ctrl-u` | delete the word before the cursor / back to line start |
| `Ctrl-t` / `Ctrl-d` | indent / dedent the current line |
| `Ctrl-r<reg>` | insert a register (`Ctrl-r %` file name, `Ctrl-r /` last search) |
| `Ctrl-a` | re-insert the text from your last insert |
| `Ctrl-o {cmd}` | run one Normal-mode command, then return to insert |
| `Ctrl-e` / `Ctrl-y` | copy the character below / above the cursor |
| `Ctrl-v` | next key literally, or a code: `065`, `u00e9`, `x…`, `o…` |
| `Ctrl-k` + two chars | digraph: `a:`→`ä`, `e'`→`é`, `->`→`→`, `Eu`→`€`, `+-`→`±` |
| `Ctrl-n` / `Ctrl-p` | complete the word from the buffer, cycling matches |
| `Ctrl-x Ctrl-l` / `Ctrl-x Ctrl-f` | complete a whole line / a file path |

Auto-indent follows `:set autoindent` / `smartindent`, and `:iabbrev teh the`
expands abbreviations as you type. `:set paste` suspends auto-indent,
abbreviations and auto-wrap (the status line shows `[paste]`) so pasted text keeps
its own layout.

**Formatting** — `gq{motion}` / `gqq` / `gqip` (and `gw`) rewrap text to
`:set textwidth=N`, ideal for comments and prose; with a textwidth set, auto-wrap
also breaks long lines as you type (`textwidth=0` turns it off).

**Undo, info & numbers**

| Keys | Action |
|------|--------|
| `u` / `Ctrl-r` | undo / redo (counted: `3u`); `:earlier [N]` / `:later [N]` step by N changes |
| `U` | restore the most recently changed line (toggles) |
| `Ctrl-g` / `g Ctrl-g` / `ga` | file info / word-char-byte counts / character code under cursor |
| `Ctrl-a` / `Ctrl-x` | increment / decrement the number (hex, binary, negatives; counted) |
| `g Ctrl-a` / `g Ctrl-x` (visual) | turn a column of numbers into a 1, 2, 3 … sequence |

**Visual mode** — select with `v` (characters), `V` (lines), or `Ctrl-v` (block), then:

| Keys | Action |
|------|--------|
| `d` `x` / `y` / `c` `s` | delete / yank / change the selection |
| `>` `<` / `=` | indent / dedent / reindent |
| `J` `gJ` / `r<c>` | join the lines / replace every selected char with `c` |
| `u` `U` `~` / `g?` | lowercase / uppercase / toggle case / ROT13 |
| `o` / `gv` | swap the active end / reselect the last selection (from Normal) |
| `*` `#` | search for the selected text, forward / back |
| `i` `a` + object | select a text object (`viw`, `vi(`, `vap`) |
| `p` `P` | replace the selection with a register |
| `:` | prefill `:'<,'>` so an ex-command runs on the selection (`:'<,'>s/…`, `:'<,'>m0`) |

**Visual block (`Ctrl-v`)** selects a rectangle: `d`/`x` delete it, `I`/`A`
insert/append on every row, `c` changes it, `$` extends to each line's own end
(so `$A` appends ragged-right), and `y` then `p`/`P` moves a column of text
(short lines padded).

**Search** — patterns are regular expressions (e.g. `/\bfn\s+\w+`):

| Keys | Action |
|------|--------|
| `/pat` `?pat` / `n` `N` | search forward / back; repeat same / reversed direction (wraps) |
| `*` `#` / `g*` `g#` | the word under the cursor, whole word / substring (counted: `3*`) |
| `gd` `gD` | jump to the identifier's definition (nearest earlier use / first in file) |
| `gf` `gF` | open the file named under the cursor (`gF` at a trailing `:line`) |
| `gx` | open the URL under the cursor in your browser |
| `gn` `gN` | select the next / previous match — an operator target, so `cgn` then `.` |
| `:noh` | clear the match highlight |

Case follows `:set ignorecase` / `smartcase` (per-pattern `\c` / `\C`), word
boundaries `\<` `\>` work, and `:set nowrapscan` stops at the ends. With `:set
incsearch` (on) the first match previews as you type — and typing a `:s`
substitution highlights its matches live. A `[index/total]` count shows your
position, and search offsets (`/pat/e`, `/pat/s±N`, `/pat/±N`) carry over to
`n`/`N`.

### Global command (`:g` / `:v`)
Run a command on every line matching a pattern:

| Command             | Effect                                             |
|---------------------|----------------------------------------------------|
| `:g/re/d`           | delete all lines matching `re`                      |
| `:v/re/d` (`:g!/re/d`) | delete all lines *not* matching `re`            |
| `:g/re/s/a/b/g`     | run the substitution on matching lines only         |
| `:g/re/>` (`:g/re/<`) | indent / dedent every matching line (`>>` shifts twice) |
| `:g/re/j` (`:g/re/j!`) | join each matching line with the next (`j!` without a space) |
| `:g/re/normal A;`   | run Normal-mode keys on each matching line (`:normal`) |
| `/re` then `:g//d`  | an empty pattern reuses the last search             |

### Multiple buffers
Open several files and switch between them:

| Command              | Effect                                   |
|----------------------|------------------------------------------|
| `:e <file>`          | open a file (switches to it if already open) |
| `:ls` / `:buffers`   | list open buffers (active marked `%`, `+` = unsaved) |
| `:bn` / `:bp` (or `:n` / `:N`) | next / previous buffer          |
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
Grouped by what they do. Most line commands accept a leading range such as
`:%` (whole file) or `:2,5` (lines 2 to 5).

**Files and buffers**

- `:w [file]`, `:wq`, `:x`, `:q`, `:q!` — write and quit (`!` discards changes). `:N,Mw file` writes just those lines to a file. Files keep the line endings they were loaded with (Windows CRLF or Unix LF); `:set fileformat=dos` or `=unix` changes which is written.
- `:set readonly` (`ro`) guards a file against accidental overwrite: `:w` is refused (shown as `[RO]` in the status line) until you force it with `:w!`. `:set noreadonly` lifts the guard.
- `:qa` / `:wa` / `:wqa` — quit or write every buffer at once.
- `:e <file>` opens a file; `:e` / `:e!` reloads the current one; `:enew` starts a new empty buffer. `:r <file>` inserts a file below the cursor, and `:r !cmd` inserts a command's output.
- `:cd <dir>` changes the working directory (so relative `:e`/`:w`/`:r` paths resolve from there); a leading `~` is your home, and `:cd` alone goes home. `:pwd` shows the current directory.
- `:ls` lists buffers; `:bn` / `:bp` (or `:n` / `:N`) / `:b <n>` / `:bd` switch or close them; `Ctrl-^` (or `:b#`) returns to the previous buffer. `:bufdo {cmd}` runs a command on every open buffer (e.g. `:bufdo %s/old/new/g`).
- `:set autowrite` saves a modified, named buffer automatically when you switch away from it, so you can move between files without a manual `:w` each time.
- `:set autoread` reloads a file when it changes on disk and you have no unsaved edits, handy when a build tool or another program rewrites it under you. Background buffers refresh too; if a changed file has unsaved edits, your copy is kept and a warning is shown instead.
- `:<n>` jumps to line `n`.
- `:command Name expansion` defines a shortcut (the name must start with a capital), e.g. `:command W w` makes `:W` save; `:delcommand Name` removes one and `:command` lists them.

**Search and replace**

- `:s/pat/rep/[g]` substitutes on the current line; add `g` for every match and a range for more (see the table below). Prefer replacing every match by default? `:set gdefault` flips it, so a bare `:s` acts global and an explicit `g` restricts to the first.
- `:noh` clears the search highlight (`:set hlsearch` / `nohlsearch` toggles it).

**Working with lines**

- `:m {addr}` and `:t` / `:co {addr}` move or copy lines.
- `:d` / `:y` delete or yank (add a letter to target a register, e.g. `:1,5y a`); `:>` / `:<` shift the indent.
- `:j[oin][!]` joins lines (`!` keeps the whitespace).
- `:ce[nter]` / `:ri[ght]` / `:le[ft]` align lines to `textwidth`.
- `:sort[!] [u][n][i] [/pat/ [r]]` sorts (reverse, unique, numeric, ignore-case, or by the text after a pattern).
- `:put [reg]` inserts a register as lines; `:normal {keys}` runs Normal-mode keys on each line.
- `:retab [N]` normalises tabs and spaces; `:[range]!cmd` filters lines through a shell command.

**Marks, registers, history**

- `:marks`, `:registers` (`:reg`), `:jumps`, `:changes`, `:history [:|/|all]` each open a listing.
- `:messages` recalls recent status-line notes and errors, so a message that flashed by isn't lost.
- `:delmarks a b` / `:delmarks!` delete named or all marks.
- `:earlier [N]` / `:later [N]` step back and forward through changes.

**Appearance and options**

- `:theme <name>` switches the color theme.
- `:set <option>` toggles or sets an option; `:set {option}?` shows a value, and `:set all` lists them all.
- Options you will reach for most:
  - Display: `number`, `relativenumber`, `cursorline`, `colorcolumn=N[,M]`, `list`.
  - Search: `ignorecase`, `smartcase`, `incsearch`.
  - Indent: `expandtab`, `shiftwidth=N`, `tabstop=N`, `shiftround`, `smartindent`.
  - Joining: `joinspaces` (two spaces after a sentence when you press `J`).
  - Other: `scrolloff=N`, `textwidth=N`, `matchpairs`, `mouse`, `title` (show the
    file name in the terminal window title), `backup` (keep a `~` copy on save).
  - Status line: `showmode`, `showcmd`, `ruler` (all on by default; turn off to
    trim the bottom line).

**Other**

- `:source <file>` runs commands from a file; `:help` and `:version` show help and the version.

Several `:set` options can be combined in one command, e.g.
`:set number expandtab shiftwidth=2`. `:set {option}?` shows one option's value;
bare `:set` lists the options changed from their defaults, and `:set all` lists
every option.

**Filtering through a shell command.** `!` sends lines to an external command
and replaces them with its output. Press `!` plus a motion (or `!!` for the
current line), or select lines and press `!`; rvim prefills the range and waits
for the command. `:%!sort` sorts the file, `!}sort` sorts to the next blank line.
A bare `:!cmd` just runs the command.

On the command line, `Up`/`Down` recall previous commands (`:` history) or
searches (`/`/`?` history), and `Ctrl-w`/`Ctrl-u` delete the previous word / the
whole line. `Ctrl-r` then a register name inserts that register (for example
`Ctrl-r "` pastes the last yank), and `Ctrl-r Ctrl-w` inserts the word under the
cursor. `Tab` completes the command name (`:sor`→`:sort`), the option name after
`:set ` (`:set nu`→`:set number`), or a filename for commands that take one
(`:e src/ma`→`:e src/main.rs`, directories gaining a trailing slash). Repeated
`Tab` / `Shift-Tab` cycle forward / backward through the matches. When more than one
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
| `:%s/foo/bar/gc`   | confirm each replacement (the `c` flag)       |
| `/foo` then `:%s//bar/g` | empty pattern reuses the last search pattern |

An empty `:s` pattern reuses the last search pattern, so `/foo` followed by
`:%s//bar/g` replaces every `foo` — and running `:s` also sets that search
pattern, so a following `n` jumps to the next occurrence.

Patterns are **regular expressions**, and the replacement uses **vim-style
backreferences** — `\1`–`\9` for groups and `&` for the whole match — e.g.
`:%s/(\w+)=(\w+)/\2=\1/g`. Case escapes work too: `\u`/`\l` upper/lower-case the
next character and `\U`/`\L` run until `\E` (so `:%s/\w+/\U&/g` shouts every word).
An invalid regex falls back to a literal match.
`&` (normal mode) repeats the last substitution on the current line, and `g&`
repeats it across the whole file. The `i`
flag makes matching case-insensitive (`:%s/foo/bar/gi`), and `\c`/`\C` in a
pattern force case-insensitive/sensitive matching for both `:s` and search. A
single undo (`u`) reverts an entire substitution.

With the `c` flag each match is confirmed interactively: the cursor stops on the
match and you press `y` to replace it, `n` to skip, `a` to replace all remaining,
`l` to replace this one and stop, or `q` (or Esc) to quit.

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
  *Language ▸ Set Filetype* (every supported language, generated from the same
  table the editor uses).
- **Menus:** File, Buffers, Edit, View (themes, line numbers, folding, options),
  Window (splits), Language, Tools (plugins), Help.
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

### Interface language
The interface can be shown in your own language — menus, mode indicators, the
status-line position marker and common messages (including file open/write and
buffer notices). `:set language=vi` switches to
Vietnamese, `:set language=fr` to French, and `:set language=en` back to English
(put any of them in `~/.rvimrc` to make it the default). The commands themselves
never change — only the labels you read. Translations fall back to English for
anything a language has not covered yet, and adding a new language is a small,
self-contained addition, so more can follow.

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

**Per-file settings (modelines).** A file can carry its own options in a comment
near the top or bottom, so a project's indentation travels with its files. rvim
reads the first and last few lines for a `vim:` comment and applies the options
it finds:

```text
# vim: set ts=2 sw=2 et ft=yaml:
```

Both the `vim: set ... :` and the shorter `vim: ts=2:sw=2` forms work. Only
`:set` options are ever applied, never commands. Turn it off with
`:set nomodeline`, or change how many lines are scanned with `:set modelines=N`.

**Key mappings.** `:nnoremap {key} {keys}` remaps a single Normal-mode key to a
sequence, for example `:nnoremap Y y$` or `:nnoremap ; :`. The right-hand side
understands `<CR>`, `<Esc>`, `<Space>`, `<Tab>`, `<BS>`, and `<C-x>` (control).
Mappings are non-recursive; `:nunmap {key}` removes one and bare `:nnoremap`
lists them. Put them in `~/.rvimrc` to make them permanent.

### Color theming
Nine built-in themes, switchable live with `:theme <name>` (or cycle with a
bare `:theme`):

| Theme            | Vibe                                              |
|------------------|---------------------------------------------------|
| `matrix`         | green phosphor on black (default)                 |
| `retrowave`      | neon pink/cyan on deep purple                     |
| `cobalt`         | warm gold/cyan accents on deep blue               |
| `gruvbox`        | warm, earthy retro palette                         |
| `nord`           | cool, muted arctic palette                          |
| `dracula`        | soft dark with vivid pink, purple and green accents |
| `solarized-dark` | the classic low-contrast blue-grey Solarized palette |
| `paper`          | light theme: dark text on a warm paper background, for bright rooms |
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
| JSON                  | `.json`                             |
| Python                | `.py`, `.pyw` (+ `python` shebang)  |
| TOML                  | `.toml`                             |
| JavaScript            | `.js`, `.mjs`, `.cjs`, `.jsx`       |
| TypeScript            | `.ts`, `.tsx`, `.mts`, `.cts`       |
| Go                    | `.go`                               |
| Shell                 | `.sh`, `.bash`, `.zsh` (+ shebang)  |
| C / C++               | `.c`, `.h`, `.cpp`, `.cc`, `.hpp` (with `#` directives) |
| Java                  | `.java`                             |
| C#                    | `.cs`                               |
| Swift                 | `.swift`                            |
| PHP                   | `.php`, `.phtml`                     |
| Kotlin                | `.kt`, `.kts`                        |
| Ruby                  | `.rb`                               |
| Lua                   | `.lua`                              |
| Scala                 | `.scala`, `.sc`                     |
| Dart                  | `.dart`                             |
| R                     | `.r`                                |
| YAML                  | `.yaml`, `.yml`                     |

A generic `.sql` file can be pinned to a dialect with a first-line hint such as
`-- dialect: trino`, or at runtime with `:set ft=snowflake`. Multi-line
constructs — block comments (`/* … */`), Python triple-quoted strings, and
JavaScript template literals / Go raw strings — are tracked across line
boundaries (and independently of each other), so they stay correctly colored
even when scrolled.

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
  hides them again. Non-breaking spaces (a frequent invisible-bug source) are
  flagged with their own marker, and you can reveal every ordinary space too.
  Markers are width-preserving, so columns stay exact. Customize them with
  `:set listchars=tab:>-,trail:~,eol:#,nbsp:+,space:.` (`tab:` takes two
  characters, the rest one each).
- **Color columns (`:set colorcolumn=N`)** — highlight one or more columns as
  visual line-length guides (e.g. `:set cc=80,120`, and they work past the end of
  short lines too); `:set cc=0` turns them off. Pair with `:set textwidth` to see
  your wrap boundary.
- **Mode indicator (showmode)** — the bottom line shows the active mode as
  `-- INSERT --`, `-- REPLACE --`, `-- VISUAL --`, `-- VISUAL LINE --`, or
  `-- VISUAL BLOCK --`, so you always know which mode you are in. Normal mode
  shows nothing (and the message or command line takes the space).
- **Cursor shape** — the cursor is a block in Normal mode, a bar while inserting,
  and an underline while replacing, so the active mode is clear at the cursor too
  (on terminals that support it; others simply keep their usual cursor).
- **Command preview (showcmd)** — the partially-typed command (count, operator,
  text-object prefix) appears at the bottom-right as you type, so you can see
  exactly what rvim is waiting for — e.g. `2d` while `2dw` is mid-entry. It clears
  the instant the command completes or is cancelled. In visual mode the same
  corner shows the selection size — column or line count, or `rows x cols` for a
  block — so you always know how much is selected.
- **Scroll position (ruler)** — the status line shows `Top`, `Bot`, or `All` when
  the top, bottom, or whole file is on screen, and a percentage otherwise, so you
  can see where you are in a long file at a glance.
- **Line-ending flag** — the status line shows `[dos]` next to the file name when
  a file uses Windows CRLF endings, so you spot that common gotcha at a glance.
  Unix LF files stay uncluttered (no tag).
- **Error messages stand out** — when a command fails (no write since last
  change, pattern not found, unknown command), the message appears in a distinct
  error color instead of blending in with ordinary status text, so you notice
  the failure. Each theme carries its own error color tuned to its palette.
- **Off-screen line markers** — when a line runs wider than the window, a `>` at
  the right edge shows it continues, and a `<` at the left edge shows text is
  hidden because the view scrolled right. You always know when content sits just
  out of sight. Customize or hide them with `:set listchars=extends:»,precedes:«`
  (set either to a space to turn it off).

### Code folding
Collapse ranges of lines to a single summary row so you can read structure
without the noise. By default you choose the ranges by hand. The automatic
methods build folds for you: `:set foldmethod=indent` from the indentation,
`:set foldmethod=marker` from `{{{` / `}}}` markers, and `:set foldmethod=syntax`
from the language's structure — brace-delimited blocks for brace languages (braces
inside strings and comments are ignored), and indentation for languages that have
no braces (Python, YAML, Ruby, Lua, Shell). Press `zM` to collapse the automatic
folds and `zR` to open them all.

- Create a fold with `zf` plus a motion (`zf5j`, `zfG`, `zf}`), or select lines
  in visual mode and press `zf`. `zF` folds a count of lines (`3zF`).
- A closed fold shows as a dimmed `+-- N lines: <first line>` bar; the line
  number of its first line stays in the gutter.
- `zo` opens the fold under the cursor, `zc` closes it, and `za` toggles it; the
  capital forms `zO` / `zC` / `zA` do the same but reach every nested fold inside.
  `zR` opens every fold, `zM` closes every fold.
- `zd` removes the fold under the cursor (keeping the text), `zE` removes all.
- `zi` turns folding off and on without discarding your folds.
- `zj` and `zk` jump to the next and previous fold; `[z` and `]z` jump to the
  start and end of the fold you are in.
- `:set foldcolumn=N` shows a small column on the left that marks where folds
  start and which lines they cover, so folds are visible at a glance.

Movement respects folds: `j` and `k` step over a closed fold as if it were one
line, the screen motions `H`, `M`, `L` and the page keys count collapsed folds as
a single row, and the cursor always rests on the fold's header rather than
vanishing inside it. Folds can be nested. (Manual folds track line ranges and are
not
auto-adjusted across large structural edits — recreate them, or `zR` to reveal
everything, after reshaping the file.)

### Split windows
View more than one file (or more than one place in a file) at once. Use `:split`
to stack windows top and bottom, or `:vsplit` to place them side by side; add a
file name to open it in the new window. Move between windows with `Ctrl-w` then a
direction key (`h` `j` `k` `l`), or cycle with `Ctrl-w w`. Close the focused
window with `:close` (or `Ctrl-w c`), and drop everything but the focused one
with `:only`. Rearrange them with `Ctrl-w x` (swap with the next window) and
`Ctrl-w r` (rotate all of them). By default a new window opens above or to the
left; `:set splitbelow` and `:set splitright` flip that. Resize the focused window
with `Ctrl-w +` / `Ctrl-w -` (taller/shorter) and `Ctrl-w >` / `Ctrl-w <`
(wider/narrower), and `Ctrl-w =` makes them equal again. Each window keeps its own
cursor and scroll position. `:windo {cmd}` runs a command in every window.

Splitting the current file (a plain `:split` or `:vsplit`) gives you two windows
on one shared buffer: type in either and the change shows in both instantly, and
undo is shared. Each window still keeps its own cursor and scroll, so you can work
on two parts of the same file at once. Opening a different file in the split keeps
it fully independent, as you would expect.

### Mouse support
`:set mouse` enables click-to-position, drag-to-select (dragging starts a visual
selection), and scroll-wheel paging; `:set nomouse` disables it. A double-click
selects the word under the pointer and a triple-click selects the whole line.
With split windows, clicking a pane focuses it and the wheel scrolls whichever
pane is under the pointer.

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
├── i18n.rs        UI localization: locales + English-keyed translation lookup
├── menu.rs        Alt-activated menu bar: data tree + navigation state
├── pattern.rs     regex compilation (literal fallback) for search & :s
├── terminal.rs    raw-mode / alt-screen RAII guard (cross-platform)
├── ui.rs          gutter + highlighted text + status/command lines
├── theme.rs       Theme + ThemeRegistry (matrix, retrowave, cobalt, gruvbox, nord, dracula, solarized-dark, paper)
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

Current suite: **896 tests** across buffer, editor, clipboard, menu, syntax,
themes, commands, config, pattern, plugins, folds, windows, modes, and UI
layout.

---

## Roadmap

Still planned, not yet built:

- More accessibility options: screen-reader hints and configurable cues.
- Dynamic plugin loading.

---

## License

Released under the [MIT License](LICENSE). © 2026 cjjulius.
